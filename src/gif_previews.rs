//! Visible GIFs share one worker and one byte/count-bounded delivery queue.
//! Decoder state keeps preview-sized canvases; raw subframe storage is shared.
use crate::domain::{Fingerprint, MediaFile, MediaId};
use image::RgbaImage;
use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufReader, Read},
    num::NonZeroU64,
    sync::{mpsc, Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant},
};

pub const MAX_VISIBLE: usize = 256;
pub const MAX_PREVIEW_PIXELS: u64 = 8 * 1024 * 1024;
const MAX_READY_BYTES: usize = 16 * 1024 * 1024;
#[derive(Clone)]
pub struct Request {
    pub id: MediaId,
    pub generation: u64,
    pub file: MediaFile,
    pub width: u32,
    pub height: u32,
}
impl Request {
    fn same(&self, other: &Self) -> bool {
        self.id == other.id
            && self.generation == other.generation
            && self.file.path == other.file.path
            && self.file.fingerprint == other.file.fingerprint
            && self.width == other.width
            && self.height == other.height
    }
}
#[derive(Clone)]
struct Task {
    token: u64,
    request: Request,
}
pub enum Event {
    Frame {
        image: Arc<RgbaImage>,
        sequence: u64,
        cycle: u32,
        delay: Duration,
    },
    End,
    Error(String),
}
pub struct Delivery {
    pub id: MediaId,
    pub token: u64,
    pub event: Event,
}
#[derive(Clone, Copy, Default, serde::Serialize)]
pub struct Stats {
    pub sources: usize,
    pub canvas_bytes: usize,
    pub ready_bytes: usize,
    pub frames: u64,
    pub decoding: bool,
}
#[derive(Default)]
struct State {
    wanted: Vec<Task>,
    revision: u64,
    serial: u64,
    stop: bool,
    stats: Stats,
}
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}
pub struct Service {
    shared: Arc<Shared>,
    receiver: mpsc::Receiver<Delivery>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Service {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
        });
        let (sender, receiver) = mpsc::sync_channel(32);
        let state = shared.clone();
        let worker = thread::spawn(move || worker(state, sender, wake));
        Self {
            shared,
            receiver,
            worker: Some(worker),
        }
    }
    pub fn set_visible(&self, mut requests: Vec<Request>) -> Vec<(MediaId, u64)> {
        requests.truncate(MAX_VISIBLE);
        for r in &mut requests {
            let pixels = u64::from(r.width) * u64::from(r.height);
            if pixels > 4 * 1024 * 1024 {
                let scale = ((4 * 1024 * 1024) as f64 / pixels as f64).sqrt();
                r.width = (r.width as f64 * scale).floor().max(1.) as u32;
                r.height = (r.height as f64 * scale).floor().max(1.) as u32;
            }
        }
        let total: u64 = requests
            .iter()
            .map(|r| u64::from(r.width) * u64::from(r.height))
            .sum();
        if total > MAX_PREVIEW_PIXELS {
            let scale = (MAX_PREVIEW_PIXELS as f64 / total as f64).sqrt();
            for r in &mut requests {
                r.width = (r.width as f64 * scale).floor().max(1.) as u32;
                r.height = (r.height as f64 * scale).floor().max(1.) as u32;
            }
        }
        while requests
            .iter()
            .map(|r| u64::from(r.width) * u64::from(r.height))
            .sum::<u64>()
            > MAX_PREVIEW_PIXELS
        {
            let r = requests
                .iter_mut()
                .max_by_key(|r| u64::from(r.width) * u64::from(r.height))
                .unwrap();
            if r.width >= r.height && r.width > 1 {
                r.width -= 1;
            } else if r.height > 1 {
                r.height -= 1;
            } else {
                break;
            }
        }
        let mut state = self.shared.state.lock().unwrap();
        if state.wanted.len() == requests.len()
            && state
                .wanted
                .iter()
                .zip(&requests)
                .all(|(a, b)| a.request.same(b))
        {
            return state
                .wanted
                .iter()
                .map(|t| (t.request.id, t.token))
                .collect();
        }
        let mut wanted = Vec::with_capacity(requests.len());
        for request in requests {
            let token = if let Some(old) = state.wanted.iter().find(|t| t.request.same(&request)) {
                old.token
            } else {
                state.serial = state.serial.wrapping_add(1).max(1);
                state.serial
            };
            wanted.push(Task { token, request });
        }
        state.wanted = wanted;
        state.revision = state.revision.wrapping_add(1);
        let tokens = state
            .wanted
            .iter()
            .map(|t| (t.request.id, t.token))
            .collect();
        drop(state);
        self.shared.changed.notify_all();
        tokens
    }
    pub fn poll(&self) -> Option<Delivery> {
        while let Ok(event) = self.receiver.try_recv() {
            let bytes = event_bytes(&event);
            {
                let mut state = self.shared.state.lock().unwrap();
                state.stats.ready_bytes = state.stats.ready_bytes.saturating_sub(bytes);
            }
            self.shared.changed.notify_all();
            if valid(&self.shared, event.id, event.token) {
                return Some(event);
            }
        }
        None
    }
    pub fn stats(&self) -> Stats {
        self.shared.state.lock().unwrap().stats
    }
    pub fn released(&self) -> bool {
        let s = self.shared.state.lock().unwrap();
        s.wanted.is_empty() && s.stats.sources == 0 && !s.stats.decoding
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.shared.state.lock().unwrap().stop = true;
        self.shared.changed.notify_all();
        if let Some(t) = self.worker.take() {
            let _ = t.join();
        }
    }
}
fn valid(shared: &Shared, id: MediaId, token: u64) -> bool {
    let s = shared.state.lock().unwrap();
    !s.stop
        && s.wanted
            .iter()
            .any(|t| t.request.id == id && t.token == token)
}
struct Input {
    file: File,
    shared: Arc<Shared>,
    id: MediaId,
    token: u64,
    remaining: u64,
}
impl Read for Input {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if !valid(&self.shared, self.id, self.token) {
            return Err(io::Error::other("Animation cancelled"));
        }
        let size = out.len().min(self.remaining as usize);
        let n = self.file.read(&mut out[..size])?;
        self.remaining -= n as u64;
        Ok(n)
    }
}
type Reader = gif::Decoder<BufReader<Input>>;
fn reader(task: &Task, shared: &Arc<Shared>) -> Result<Reader, String> {
    if task.request.file.fingerprint.bytes > 256 * 1024 * 1024 {
        return Err(crate::ui_text!(msg_518d809683ad).into());
    }
    if Fingerprint::read(&task.request.file.path)? != task.request.file.fingerprint {
        return Err(crate::ui_text!(msg_c06f6ada4aed).into());
    }
    let input = Input {
        file: File::open(&task.request.file.path).map_err(|e| e.to_string())?,
        shared: shared.clone(),
        id: task.request.id,
        token: task.token,
        remaining: task.request.file.fingerprint.bytes,
    };
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    options.set_memory_limit(gif::MemoryLimit::Bytes(
        NonZeroU64::new(32 * 1024 * 1024).unwrap(),
    ));
    let result = options
        .read_info(BufReader::new(input))
        .map_err(|e| e.to_string())?;
    let (w, h) = (u32::from(result.width()), u32::from(result.height()));
    if w == 0
        || h == 0
        || w > 8192
        || h > 8192
        || u64::from(w) * u64::from(h) > crate::animation::MAX_PIXELS
    {
        return Err(crate::ui_text!(msg_a9f14f579a80).into());
    }
    Ok(result)
}
#[derive(Clone, Copy)]
struct Frame {
    left: u32,
    top: u32,
    width: u32,
    height: u32,
    dispose: gif::DisposalMethod,
    delay: u16,
}
struct Stream {
    task: Task,
    reader: Option<Reader>,
    canvas: Arc<RgbaImage>,
    saved: Option<Arc<RgbaImage>>,
    last: Option<Frame>,
    source: (u32, u32),
    sequence: u64,
    cycle: u32,
    cycle_frames: u32,
    due: Instant,
    ended: bool,
}
impl Stream {
    fn new(task: Task, shared: &Arc<Shared>) -> Result<Self, String> {
        let input = reader(&task, shared)?;
        let (w, h) = (u32::from(input.width()), u32::from(input.height()));
        let scale = (task.request.width.max(1) as f64 / w as f64)
            .min(task.request.height.max(1) as f64 / h as f64)
            .min(1.);
        let (ow, oh) = (
            (w as f64 * scale).round().max(1.) as u32,
            (h as f64 * scale).round().max(1.) as u32,
        );
        Ok(Self {
            task,
            reader: Some(input),
            canvas: Arc::new(RgbaImage::new(ow, oh)),
            saved: None,
            last: None,
            source: (w, h),
            sequence: 0,
            cycle: 0,
            cycle_frames: 0,
            due: Instant::now(),
            ended: false,
        })
    }
    fn step(&mut self, shared: &Arc<Shared>, raw: &mut Vec<u8>) -> Result<Event, String> {
        if Fingerprint::read(&self.task.request.file.path)? != self.task.request.file.fingerprint {
            return Err(crate::ui_text!(msg_c06f6ada4aed).into());
        }
        let input = self.reader.as_mut().unwrap();
        let info = input
            .next_frame_info()
            .map_err(|e| e.to_string())?
            .map(|f| Frame {
                left: f.left.into(),
                top: f.top.into(),
                width: f.width.into(),
                height: f.height.into(),
                dispose: f.dispose,
                delay: f.delay,
            });
        let Some(info) = info else {
            if self.cycle_frames == 0 {
                return Err(crate::ui_text!(msg_2282f7536066).into());
            }
            let limit = match input.repeat() {
                gif::Repeat::Infinite => None,
                gif::Repeat::Finite(n) => Some(u32::from(n) + 1),
            };
            self.cycle += 1;
            if limit.is_some_and(|n| self.cycle >= n) {
                self.ended = true;
                self.reader = None;
                self.saved = None;
                self.canvas = Arc::new(RgbaImage::new(0, 0));
                return Ok(Event::End);
            }
            self.reader = Some(reader(&self.task, shared)?);
            Arc::make_mut(&mut self.canvas).fill(0);
            self.saved = None;
            self.last = None;
            self.cycle_frames = 0;
            return self.step(shared, raw);
        };
        if info.width == 0
            || info.height == 0
            || info.left + info.width > self.source.0
            || info.top + info.height > self.source.1
        {
            return Err("Invalid GIF frame region".into());
        }
        let bytes = info.width as usize * info.height as usize * 4;
        raw.resize(bytes, 0);
        input.read_into_buffer(raw).map_err(|e| e.to_string())?;
        let profile = input
            .icc_profile()
            .filter(|p| p.len() <= 4 * 1024 * 1024)
            .map(<[u8]>::to_vec);
        if !valid(shared, self.task.request.id, self.task.token) {
            return Err("Animation cancelled".into());
        }
        if let Some(last) = self.last {
            if last.dispose == gif::DisposalMethod::Previous {
                if let Some(saved) = self.saved.take() {
                    self.canvas = saved;
                }
            } else if last.dispose == gif::DisposalMethod::Background {
                let pixels = Arc::make_mut(&mut self.canvas);
                let (ow, oh) = pixels.dimensions();
                for y in 0..oh {
                    let sy = ((2 * u64::from(y) + 1) * u64::from(self.source.1)
                        / (2 * u64::from(oh))) as u32;
                    for x in 0..ow {
                        let sx = ((2 * u64::from(x) + 1) * u64::from(self.source.0)
                            / (2 * u64::from(ow))) as u32;
                        if sx >= last.left
                            && sx < last.left + last.width
                            && sy >= last.top
                            && sy < last.top + last.height
                        {
                            pixels.put_pixel(x, y, image::Rgba([0; 4]));
                        }
                    }
                }
            }
        }
        self.saved = if info.dispose == gif::DisposalMethod::Previous {
            Some(self.canvas.clone())
        } else {
            None
        };
        let pixels = Arc::make_mut(&mut self.canvas);
        let (ow, oh) = pixels.dimensions();
        for y in 0..oh {
            let sy =
                ((2 * u64::from(y) + 1) * u64::from(self.source.1) / (2 * u64::from(oh))) as u32;
            if sy < info.top || sy >= info.top + info.height {
                continue;
            }
            for x in 0..ow {
                let sx = ((2 * u64::from(x) + 1) * u64::from(self.source.0) / (2 * u64::from(ow)))
                    as u32;
                if sx < info.left || sx >= info.left + info.width {
                    continue;
                }
                let at = ((sy - info.top) * info.width + sx - info.left) as usize * 4;
                if raw[at + 3] != 0 {
                    pixels.put_pixel(
                        x,
                        y,
                        image::Rgba([raw[at], raw[at + 1], raw[at + 2], raw[at + 3]]),
                    );
                }
            }
        }
        if Fingerprint::read(&self.task.request.file.path)? != self.task.request.file.fingerprint {
            return Err(crate::ui_text!(msg_c06f6ada4aed).into());
        }
        let delay = Duration::from_millis(if info.delay == 0 {
            100
        } else {
            u64::from(info.delay) * 10
        });
        let image = if let Some(profile) = profile {
            let mut image = self.canvas.as_ref().clone();
            if let Err(e) = crate::color::icc_to_srgb(&mut image, &profile) {
                eprintln!("Animation preview profile: {e}");
            }
            Arc::new(image)
        } else {
            self.canvas.clone()
        };
        let event = Event::Frame {
            image,
            sequence: self.sequence,
            cycle: self.cycle,
            delay,
        };
        self.sequence += 1;
        self.cycle_frames += 1;
        self.last = Some(info);
        self.due = Instant::now() + delay;
        Ok(event)
    }
}
fn worker(shared: Arc<Shared>, sender: mpsc::SyncSender<Delivery>, wake: impl Fn()) {
    let mut streams: HashMap<MediaId, Stream> = HashMap::new();
    let mut failed: HashMap<MediaId, u64> = HashMap::new();
    let mut revision = u64::MAX;
    let mut raw = Vec::new();
    loop {
        let wanted = {
            let mut state = shared.state.lock().unwrap();
            if state.stop {
                return;
            }
            if state.revision != revision {
                revision = state.revision;
                state.stats.decoding = true;
                Some(state.wanted.clone())
            } else {
                state.stats.sources = streams.len();
                state.stats.canvas_bytes = streams
                    .values()
                    .map(|s| {
                        s.canvas.as_raw().len() + s.saved.as_ref().map_or(0, |p| p.as_raw().len())
                    })
                    .sum();
                None
            }
        };
        if let Some(wanted) = wanted {
            streams.retain(|id, s| {
                wanted
                    .iter()
                    .any(|t| t.request.id == *id && t.token == s.task.token)
            });
            failed.retain(|id, token| {
                wanted
                    .iter()
                    .any(|t| t.request.id == *id && t.token == *token)
            });
            for task in wanted {
                if !streams.contains_key(&task.request.id)
                    && failed.get(&task.request.id) != Some(&task.token)
                {
                    match Stream::new(task.clone(), &shared) {
                        Ok(s) => {
                            streams.insert(task.request.id, s);
                        }
                        Err(e) => {
                            failed.insert(task.request.id, task.token);
                            let _ = send(
                                &shared,
                                &sender,
                                Delivery {
                                    id: task.request.id,
                                    token: task.token,
                                    event: Event::Error(e),
                                },
                            );
                        }
                    }
                }
            }
            if streams.is_empty() {
                raw.clear();
                raw.shrink_to_fit();
            }
            let mut state = shared.state.lock().unwrap();
            state.stats.decoding = false;
            state.stats.sources = streams.len();
            state.stats.canvas_bytes = streams
                .values()
                .map(|s| s.canvas.as_raw().len() + s.saved.as_ref().map_or(0, |p| p.as_raw().len()))
                .sum();
            drop(state);
            wake();
        }
        let next = streams
            .iter()
            .filter(|(_, s)| !s.ended)
            .min_by_key(|(id, s)| (s.due, **id))
            .map(|(id, s)| (*id, s.due));
        if let Some((id, due)) = next {
            if due <= Instant::now() {
                shared.state.lock().unwrap().stats.decoding = true;
                let s = streams.get_mut(&id).unwrap();
                let token = s.task.token;
                let result = s.step(&shared, &mut raw);
                shared.state.lock().unwrap().stats.decoding = false;
                let event = match result {
                    Ok(event) => {
                        if matches!(event, Event::Frame { .. }) {
                            shared.state.lock().unwrap().stats.frames += 1;
                        }
                        event
                    }
                    Err(e) => {
                        s.ended = true;
                        s.reader = None;
                        s.saved = None;
                        s.canvas = Arc::new(RgbaImage::new(0, 0));
                        Event::Error(e)
                    }
                };
                send(&shared, &sender, Delivery { id, token, event });
                wake();
                continue;
            }
        }
        let state = shared.state.lock().unwrap();
        if state.stop {
            return;
        }
        let delay = next
            .map_or(Duration::from_millis(100), |(_, due)| {
                due.saturating_duration_since(Instant::now())
            })
            .min(Duration::from_millis(100));
        let _ = shared.changed.wait_timeout(state, delay).unwrap();
    }
}
fn send(shared: &Shared, sender: &mpsc::SyncSender<Delivery>, mut value: Delivery) -> bool {
    let bytes = event_bytes(&value);
    loop {
        if !valid(shared, value.id, value.token) {
            return false;
        }
        let mut state = shared.state.lock().unwrap();
        if state.stats.ready_bytes + bytes > MAX_READY_BYTES {
            let _ = shared
                .changed
                .wait_timeout(state, Duration::from_millis(5))
                .unwrap();
            continue;
        }
        state.stats.ready_bytes += bytes;
        match sender.try_send(value) {
            Ok(()) => return true,
            Err(mpsc::TrySendError::Disconnected(_)) => {
                state.stats.ready_bytes = state.stats.ready_bytes.saturating_sub(bytes);
                return false;
            }
            Err(mpsc::TrySendError::Full(v)) => {
                value = v;
                state.stats.ready_bytes = state.stats.ready_bytes.saturating_sub(bytes);
                if state.stop {
                    return false;
                }
                let _ = shared
                    .changed
                    .wait_timeout(state, Duration::from_millis(5))
                    .unwrap();
            }
        }
    }
}
fn event_bytes(value: &Delivery) -> usize {
    match &value.event {
        Event::Frame { image, .. } => image.as_raw().len(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::AnimationDecoder;
    fn fixture() -> MediaFile {
        let root = std::env::temp_dir().join(format!(
            "glimpse-list-gif-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("局部.gif");
        let mut encoder = gif::Encoder::new(
            File::create(&path).unwrap(),
            2,
            2,
            &[255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0],
        )
        .unwrap();
        encoder.set_repeat(gif::Repeat::Finite(0)).unwrap();
        for (i, (left, top, width, height, dispose, data)) in [
            (0, 0, 2, 2, gif::DisposalMethod::Keep, vec![0, 0, 0, 0]),
            (0, 0, 1, 1, gif::DisposalMethod::Previous, vec![1]),
            (1, 1, 1, 1, gif::DisposalMethod::Background, vec![2]),
            (0, 1, 2, 1, gif::DisposalMethod::Keep, vec![1, 3]),
        ]
        .into_iter()
        .enumerate()
        {
            let mut frame = gif::Frame::default();
            frame.left = left;
            frame.top = top;
            frame.width = width;
            frame.height = height;
            frame.dispose = dispose;
            frame.delay = [10, 35, 8, 17][i];
            frame.transparent = Some(3);
            frame.buffer = std::borrow::Cow::Owned(data);
            encoder.write_frame(&frame).unwrap();
        }
        drop(encoder);
        MediaFile {
            fingerprint: Fingerprint::read(&path).unwrap(),
            parent: root,
            path,
            kind: crate::domain::MediaKind::Image,
        }
    }
    fn wait(service: &Service) -> Delivery {
        let began = Instant::now();
        loop {
            if let Some(v) = service.poll() {
                return v;
            }
            assert!(began.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn released(service: &Service) {
        let began = Instant::now();
        while !service.released() {
            while service.poll().is_some() {}
            assert!(began.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(2));
        }
    }
    #[test]
    fn multiple_visible_gifs_play_without_hover_and_match_native_disposal_pixels() {
        let file = fixture();
        let before = std::fs::read(&file.path).unwrap();
        let reference =
            image::codecs::gif::GifDecoder::new(BufReader::new(File::open(&file.path).unwrap()))
                .unwrap()
                .into_frames()
                .collect_frames()
                .unwrap();
        let service = Service::new(|| {});
        let tokens = service.set_visible(
            (1..=3)
                .map(|id| Request {
                    id,
                    generation: 1,
                    file: file.clone(),
                    width: 2,
                    height: 2,
                })
                .collect(),
        );
        assert_eq!(tokens.len(), 3);
        let mut counts = HashMap::new();
        let mut ended = 0;
        while ended < 3 {
            let event = wait(&service);
            match event.event {
                Event::Frame {
                    image,
                    sequence,
                    delay,
                    ..
                } => {
                    assert_eq!(image.as_ref(), reference[sequence as usize].buffer());
                    assert_eq!(delay.as_millis(), [100, 350, 80, 170][sequence as usize]);
                    *counts.entry(event.id).or_insert(0) += 1;
                }
                Event::End => ended += 1,
                Event::Error(e) => panic!("{e}"),
            }
        }
        assert!(counts.values().all(|n| *n == 4));
        assert_eq!(counts.len(), 3);
        service.set_visible(vec![]);
        released(&service);
        assert_eq!(std::fs::read(&file.path).unwrap(), before);
    }
    #[test]
    fn leaving_view_cancels_backpressure_and_reentry_has_fresh_identity() {
        let file = fixture();
        let service = Service::new(|| {});
        let request = Request {
            id: 1,
            generation: 8,
            file,
            width: 2,
            height: 2,
        };
        let first = service.set_visible(vec![request.clone()])[0].1;
        let _ = wait(&service);
        service.set_visible(vec![]);
        released(&service);
        let second = service.set_visible(vec![request])[0].1;
        assert_ne!(first, second);
        let value = wait(&service);
        assert_eq!(value.token, second);
        assert!(matches!(value.event, Event::Frame { sequence: 0, .. }));
        assert!(service.stats().ready_bytes <= MAX_READY_BYTES);
        service.set_visible(vec![]);
        released(&service);
    }
}
