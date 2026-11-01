//! One cancellable GIF/APNG worker; two queued frames, never an all-frames allocation.
use crate::domain::{Fingerprint, MediaFile};
use image::{AnimationDecoder, ImageDecoder};
use std::{
    fs::File,
    io::{self, BufReader, Read, Seek, SeekFrom},
    num::NonZeroU64,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Condvar, Mutex,
    },
    thread,
    time::Duration,
};

pub const MAX_PIXELS: u64 = 8 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;

pub struct AnimationFrame {
    pub image: image::RgbaImage,
    pub delay: Duration,
    pub sequence: u64,
    pub cycle: u32,
}
pub enum AnimationEvent {
    Frame(AnimationFrame),
    End,
    Error(String),
}
pub struct Delivery {
    pub token: u64,
    pub event: AnimationEvent,
}
struct Request {
    token: u64,
    file: MediaFile,
}
#[derive(Default)]
struct State {
    pending: Option<Request>,
    active: bool,
    stop: bool,
}
struct Shared {
    generation: AtomicU64,
    state: Mutex<State>,
    changed: Condvar,
    sent: AtomicU64,
}

pub struct AnimationService {
    shared: Arc<Shared>,
    receiver: mpsc::Receiver<Delivery>,
    worker: Option<thread::JoinHandle<()>>,
}
impl AnimationService {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let shared = Arc::new(Shared {
            generation: AtomicU64::new(0),
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
            sent: AtomicU64::new(0),
        });
        let (sender, receiver) = mpsc::sync_channel(2);
        let state = shared.clone();
        let worker = thread::spawn(move || {
            loop {
                let request = {
                    let mut lock = state.state.lock().unwrap();
                    loop {
                        if lock.stop {
                            return;
                        }
                        if let Some(request) = lock.pending.take() {
                            lock.active = true;
                            break request;
                        }
                        lock = state.changed.wait(lock).unwrap();
                    }
                };
                let result = stream(&request, &state, |event| {
                    let frame = matches!(event, AnimationEvent::Frame(_));
                    let mut delivery = Delivery {
                        token: request.token,
                        event,
                    };
                    loop {
                        if state.generation.load(Ordering::Acquire) != request.token {
                            return false;
                        }
                        match sender.try_send(delivery) {
                            Ok(()) => {
                                if frame {
                                    state.sent.fetch_add(1, Ordering::Relaxed);
                                }
                                wake();
                                return true;
                            }
                            Err(mpsc::TrySendError::Disconnected(_)) => return false,
                            Err(mpsc::TrySendError::Full(value)) => {
                                delivery = value;
                                let lock = state.state.lock().unwrap();
                                let _ = state
                                    .changed
                                    .wait_timeout(lock, Duration::from_millis(10))
                                    .unwrap();
                            }
                        }
                    }
                });
                // stream has returned: its CancellableFile/decoder have been dropped.
                state.state.lock().unwrap().active = false;
                wake();
                if let Err(error) = result {
                    if state.generation.load(Ordering::Acquire) == request.token {
                        // Error delivery uses the same bounded/cancellable path.
                        let mut delivery = Delivery {
                            token: request.token,
                            event: AnimationEvent::Error(error),
                        };
                        loop {
                            if state.generation.load(Ordering::Acquire) != request.token {
                                break;
                            }
                            match sender.try_send(delivery) {
                                Ok(()) => {
                                    wake();
                                    break;
                                }
                                Err(mpsc::TrySendError::Disconnected(_)) => return,
                                Err(mpsc::TrySendError::Full(value)) => {
                                    delivery = value;
                                    thread::sleep(Duration::from_millis(10));
                                }
                            }
                        }
                    }
                }
            }
        });
        Self {
            shared,
            receiver,
            worker: Some(worker),
        }
    }
    pub fn open(&self, file: MediaFile) -> u64 {
        let mut state = self.shared.state.lock().unwrap();
        let token = self.shared.generation.fetch_add(1, Ordering::AcqRel) + 1;
        while self.receiver.try_recv().is_ok() {}
        state.pending = Some(Request { token, file });
        drop(state);
        self.shared.changed.notify_all();
        token
    }
    pub fn cancel(&self) {
        let mut state = self.shared.state.lock().unwrap();
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        state.pending = None;
        while self.receiver.try_recv().is_ok() {}
        drop(state);
        self.shared.changed.notify_all();
    }
    pub fn released(&self) -> bool {
        let state = self.shared.state.lock().unwrap();
        state.pending.is_none() && !state.active
    }
    pub fn poll(&self) -> Option<Delivery> {
        let value = self.receiver.try_recv().ok();
        if value.is_some() {
            self.shared.changed.notify_all();
        }
        value
    }
    pub fn frames_sent(&self) -> u64 {
        self.shared.sent.load(Ordering::Relaxed)
    }
}
impl Drop for AnimationService {
    fn drop(&mut self) {
        self.cancel();
        self.shared.state.lock().unwrap().stop = true;
        self.shared.changed.notify_all();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct CancellableFile {
    file: File,
    shared: Arc<Shared>,
    token: u64,
    position: u64,
    length: u64,
}
impl CancellableFile {
    fn new(request: &Request, shared: &Arc<Shared>) -> Result<Self, String> {
        if request.file.fingerprint.bytes > MAX_FILE_BYTES {
            return Err(crate::ui_text!(msg_518d809683ad).into());
        }
        if Fingerprint::read(&request.file.path)? != request.file.fingerprint {
            return Err(crate::ui_text!(msg_c06f6ada4aed).into());
        }
        let file = File::open(&request.file.path).map_err(|e| e.to_string())?;
        Ok(Self {
            file,
            shared: shared.clone(),
            token: request.token,
            position: 0,
            length: request.file.fingerprint.bytes,
        })
    }
    fn check(&self) -> io::Result<()> {
        if self.shared.generation.load(Ordering::Acquire) != self.token {
            Err(io::Error::other("GIF task cancelled"))
        } else {
            Ok(())
        }
    }
}
impl Read for CancellableFile {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.check()?;
        let count = buffer
            .len()
            .min(self.length.saturating_sub(self.position) as usize);
        let read = self.file.read(&mut buffer[..count])?;
        self.position += read as u64;
        Ok(read)
    }
}
impl Seek for CancellableFile {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.check()?;
        let position = self.file.seek(from)?;
        if position > self.length {
            return Err(io::Error::other("GIF seek outside bounded source"));
        }
        self.position = position;
        Ok(position)
    }
}

/// Metadata only; callers run this on decoding workers, never the UI thread.
pub fn probe_animated(path: &std::path::Path) -> Result<bool, String> {
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if extension.eq_ignore_ascii_case("gif") {
        return Ok(true);
    }
    if extension.eq_ignore_ascii_case("png") || extension.eq_ignore_ascii_case("apng") {
        let decoder = image::codecs::png::PngDecoder::new(BufReader::new(
            File::open(path).map_err(|e| e.to_string())?,
        ))
        .map_err(|e| e.to_string())?;
        return decoder.is_apng().map_err(|e| e.to_string());
    }
    Ok(false)
}

fn stream(
    request: &Request,
    shared: &Arc<Shared>,
    mut emit: impl FnMut(AnimationEvent) -> bool,
) -> Result<(), String> {
    if request
        .file
        .path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("png") || e.eq_ignore_ascii_case("apng"))
    {
        return stream_apng(request, shared, emit);
    }
    // Probe compressed blocks without retaining or decoding their pixels. This also
    // sees late Netscape extensions; image::loop_count alone conflates absent/zero.
    let mut options = gif::DecodeOptions::new();
    options.skip_frame_decoding(true);
    options.set_memory_limit(gif::MemoryLimit::Bytes(
        NonZeroU64::new(32 * 1024 * 1024).unwrap(),
    ));
    let mut metadata = options
        .read_info(BufReader::new(CancellableFile::new(request, shared)?))
        .map_err(|e| e.to_string())?;
    validate_dimensions(u32::from(metadata.width()), u32::from(metadata.height()))?;
    while metadata
        .next_frame_info()
        .map_err(|e| e.to_string())?
        .is_some()
    {
        if shared.generation.load(Ordering::Acquire) != request.token {
            return Ok(());
        }
    }
    let cycles = match metadata.repeat() {
        gif::Repeat::Infinite => None,
        gif::Repeat::Finite(n) => Some(u32::from(n) + 1),
    };
    let mut profile = metadata
        .icc_profile()
        .filter(|profile| profile.len() <= 4 * 1024 * 1024)
        .map(<[u8]>::to_vec);
    drop(metadata);
    let mut cycle = 0u32;
    let mut sequence = 0u64;
    loop {
        if shared.generation.load(Ordering::Acquire) != request.token {
            return Ok(());
        }
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(128 * 1024 * 1024);
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);

        let mut decoder = image::codecs::gif::GifDecoder::new(BufReader::new(
            CancellableFile::new(request, shared)?,
        ))
        .map_err(|e| e.to_string())?;
        validate_dimensions(decoder.dimensions().0, decoder.dimensions().1)?;
        decoder.set_limits(limits).map_err(|e| e.to_string())?;
        let frames = decoder.into_frames();
        let mut count = 0;
        for frame in frames {
            if shared.generation.load(Ordering::Acquire) != request.token {
                return Ok(());
            }
            let frame = frame.map_err(|e| e.to_string())?;
            if Fingerprint::read(&request.file.path)? != request.file.fingerprint {
                return Err(crate::ui_text!(msg_c06f6ada4aed).into());
            }
            let delay = Duration::from(frame.delay());
            let delay = if delay.is_zero() {
                Duration::from_millis(100)
            } else {
                delay.max(Duration::from_millis(10))
            };
            let mut image = frame.into_buffer();
            if let Some(error) = profile
                .as_ref()
                .and_then(|profile| crate::color::icc_to_srgb(&mut image, profile).err())
            {
                eprintln!("{}", crate::ui_format!(msg_0d71e9d04ac2, error = error));
                profile = None;
            }
            if !emit(AnimationEvent::Frame(AnimationFrame {
                image,
                delay,
                sequence,
                cycle,
            })) {
                return Ok(());
            }
            sequence += 1;
            count += 1;
        }
        if count == 0 {
            return Err(crate::ui_text!(msg_2282f7536066).into());
        }
        cycle += 1;
        if cycles.is_some_and(|limit| cycle >= limit) {
            emit(AnimationEvent::End);
            return Ok(());
        }
    }
}
fn stream_apng(
    request: &Request,
    shared: &Arc<Shared>,
    mut emit: impl FnMut(AnimationEvent) -> bool,
) -> Result<(), String> {
    let mut cycle = 0_u32;
    let mut sequence = 0_u64;
    loop {
        if shared.generation.load(Ordering::Acquire) != request.token {
            return Ok(());
        }
        let mut decoder = png::Decoder::new(BufReader::new(CancellableFile::new(request, shared)?));
        decoder.set_limits(png::Limits {
            bytes: 32 * 1024 * 1024,
        });
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
        let (w, h) = (reader.info().width, reader.info().height);
        validate_dimensions(w, h)?;
        let control = reader
            .info()
            .animation_control
            .ok_or("PNG has no animation control")?;
        let separate_cover = reader.info().frame_control.is_none();
        let profile = reader
            .info()
            .icc_profile
            .as_ref()
            .filter(|p| p.len() <= 4 * 1024 * 1024)
            .map(|p| p.to_vec());
        let mut canvas = image::RgbaImage::new(w, h);
        let mut previous = None;
        let mut disposal = None;
        let mut raw = vec![0_u8; w as usize * h as usize * 4];
        for index in 0..u64::from(control.num_frames) + u64::from(separate_cover) {
            if shared.generation.load(Ordering::Acquire) != request.token {
                return Ok(());
            }
            let output = reader.next_frame(&mut raw).map_err(|e| e.to_string())?;
            if separate_cover && index == 0 {
                continue;
            }
            let frame = reader
                .info()
                .frame_control
                .ok_or("Missing APNG frame control")?;
            if frame.width != output.width
                || frame.height != output.height
                || frame
                    .x_offset
                    .checked_add(frame.width)
                    .is_none_or(|n| n > w)
                || frame
                    .y_offset
                    .checked_add(frame.height)
                    .is_none_or(|n| n > h)
            {
                return Err("Invalid APNG frame region".into());
            }
            if let Some((last, op)) = disposal.take() {
                let last: png::FrameControl = last;
                match op {
                    png::DisposeOp::Previous => {
                        if let Some(saved) = previous.take() {
                            canvas = saved;
                        }
                    }
                    png::DisposeOp::Background => {
                        for y in last.y_offset..last.y_offset + last.height {
                            for x in last.x_offset..last.x_offset + last.width {
                                canvas.put_pixel(x, y, image::Rgba([0; 4]));
                            }
                        }
                    }
                    _ => {}
                }
            }
            previous = if frame.dispose_op == png::DisposeOp::Previous {
                Some(canvas.clone())
            } else {
                None
            };
            let channels = match output.color_type {
                png::ColorType::Rgb => 3,
                png::ColorType::Rgba => 4,
                png::ColorType::Grayscale => 1,
                png::ColorType::GrayscaleAlpha => 2,
                _ => return Err("Unsupported APNG expanded color".into()),
            };
            if output.bit_depth != png::BitDepth::Eight {
                return Err("Unsupported APNG expanded depth".into());
            }
            for y in 0..frame.height {
                for x in 0..frame.width {
                    let at = (y as usize * frame.width as usize + x as usize) * channels;
                    let pixel = match output.color_type {
                        png::ColorType::Rgb => [raw[at], raw[at + 1], raw[at + 2], 255],
                        png::ColorType::Rgba => [raw[at], raw[at + 1], raw[at + 2], raw[at + 3]],
                        png::ColorType::Grayscale => [raw[at], raw[at], raw[at], 255],
                        _ => [raw[at], raw[at], raw[at], raw[at + 1]],
                    };
                    let target = canvas.get_pixel_mut(frame.x_offset + x, frame.y_offset + y);
                    if frame.blend_op == png::BlendOp::Source {
                        *target = image::Rgba(pixel);
                    } else {
                        let sa = u64::from(pixel[3]);
                        let da = u64::from(target[3]);
                        let alpha = sa * 255 + da * (255 - sa);
                        if alpha == 0 {
                            *target = image::Rgba([0; 4]);
                        } else {
                            for c in 0..3 {
                                target[c] = ((u64::from(pixel[c]) * sa * 255
                                    + u64::from(target[c]) * da * (255 - sa)
                                    + alpha / 2)
                                    / alpha) as u8;
                            }
                            target[3] = ((alpha + 127) / 255) as u8;
                        }
                    }
                }
            }
            if Fingerprint::read(&request.file.path)? != request.file.fingerprint {
                return Err(crate::ui_text!(msg_c06f6ada4aed).into());
            }
            let mut image = canvas.clone();
            if let Some(profile) = &profile {
                if let Err(e) = crate::color::icc_to_srgb(&mut image, profile) {
                    eprintln!("APNG color profile: {e}");
                }
            }
            let delay = if frame.delay_num == 0 {
                Duration::from_millis(100)
            } else {
                Duration::from_secs_f64(
                    f64::from(frame.delay_num)
                        / f64::from(if frame.delay_den == 0 {
                            100
                        } else {
                            frame.delay_den
                        }),
                )
                .max(Duration::from_millis(10))
            };
            if !emit(AnimationEvent::Frame(AnimationFrame {
                image,
                delay,
                sequence,
                cycle,
            })) {
                return Ok(());
            }
            sequence += 1;
            disposal = Some((frame, frame.dispose_op));
        }
        reader.finish().map_err(|e| e.to_string())?;
        cycle += 1;
        if control.num_plays > 0 && cycle >= control.num_plays {
            emit(AnimationEvent::End);
            return Ok(());
        }
    }
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), String> {
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        Err(crate::ui_text!(msg_a9f14f579a80).into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::MediaKind;
    use std::time::{Instant, SystemTime, UNIX_EPOCH};
    fn fixture(repeat: gif::Repeat) -> MediaFile {
        // Windows wall-clock readings can repeat across parallel test threads.
        // Never let one GIF test overwrite another test's still-open source.
        static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);
        let fixture_id = FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "media-browser-gif-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            fixture_id
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("动画.gif");
        let mut encoder = gif::Encoder::new(
            File::create(&path).unwrap(),
            2,
            1,
            &[255, 0, 0, 0, 255, 0, 0, 0, 255],
        )
        .unwrap();
        encoder.set_repeat(repeat).unwrap();
        for (index, delay) in [(0, 10), (1, 35), (2, 8)] {
            let mut frame = gif::Frame::default();
            frame.width = 2;
            frame.height = 1;
            frame.delay = delay;
            frame.buffer = std::borrow::Cow::Owned(vec![index, index]);
            encoder.write_frame(&frame).unwrap();
        }
        drop(encoder);
        MediaFile {
            fingerprint: Fingerprint::read(&path).unwrap(),
            parent: root,
            path,
            kind: MediaKind::Image,
        }
    }
    fn next(service: &AnimationService) -> Delivery {
        let began = Instant::now();
        loop {
            if let Some(value) = service.poll() {
                return value;
            }
            assert!(began.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(2));
        }
    }
    #[test]
    fn frame_colors_delays_finite_repeat_and_source_bytes_are_preserved() {
        let file = fixture(gif::Repeat::Finite(1));
        let before = std::fs::read(&file.path).unwrap();
        let service = AnimationService::new(|| {});
        let token = service.open(file.clone());
        for index in 0..6 {
            let value = next(&service);
            assert_eq!(value.token, token);
            let AnimationEvent::Frame(frame) = value.event else {
                panic!("Expected frame");
            };
            assert_eq!(frame.sequence, index);
            assert_eq!(frame.cycle, index as u32 / 3);
            assert_eq!(frame.delay.as_millis(), [100, 350, 80][index as usize % 3]);
            assert_eq!(
                frame.image.get_pixel(0, 0).0,
                [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]][index as usize % 3]
            );
        }
        assert!(matches!(next(&service).event, AnimationEvent::End));
        assert_eq!(std::fs::read(&file.path).unwrap(), before);
    }
    #[test]
    fn absent_repeat_plays_once_and_queue_backpressure_can_be_cancelled() {
        let service = AnimationService::new(|| {});
        let once = fixture(gif::Repeat::Finite(0));
        service.open(once.clone());
        for _ in 0..3 {
            assert!(matches!(next(&service).event, AnimationEvent::Frame(_)));
        }
        assert!(matches!(next(&service).event, AnimationEvent::End));
        service.open(fixture(gif::Repeat::Infinite));
        let before = service.frames_sent();
        thread::sleep(Duration::from_millis(80));
        assert!(
            service.frames_sent() - before <= 2,
            "Producer escaped bounded channel"
        );
        let new_token = service.open(once);
        loop {
            let value = next(&service);
            if value.token != new_token {
                continue;
            }
            let AnimationEvent::Frame(frame) = value.event else {
                panic!("Expected new source frame");
            };
            assert_eq!(frame.sequence, 0);
            break;
        }
        let began = Instant::now();
        drop(service);
        assert!(began.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn cancelled_backpressured_worker_acknowledges_closed_file() {
        let file = fixture(gif::Repeat::Infinite);
        let source = std::fs::read(&file.path).unwrap();
        let service = AnimationService::new(|| {});
        service.open(file.clone());
        let _ = next(&service);
        assert!(!service.released());
        service.cancel();
        let began = Instant::now();
        while !service.released() {
            assert!(began.elapsed() < Duration::from_secs(2));
            thread::sleep(Duration::from_millis(2));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let exclusive = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(0)
                .open(&file.path)
                .unwrap();
            drop(exclusive);
        }
        assert_eq!(std::fs::read(file.path).unwrap(), source);
    }
    #[test]
    fn stale_fingerprint_and_oversized_canvas_are_rejected() {
        let mut file = fixture(gif::Repeat::Finite(0));
        file.fingerprint.bytes += 1;
        let service = AnimationService::new(|| {});
        service.open(file);
        assert!(matches!(next(&service).event, AnimationEvent::Error(_)));
        assert!(validate_dimensions(8192, 8192).is_err());
        assert!(validate_dimensions(3840, 2160).is_ok());
    }
    #[test]
    fn partial_frames_restore_previous_canvas_and_dispose_to_transparency() {
        for disposal in [
            gif::DisposalMethod::Previous,
            gif::DisposalMethod::Background,
        ] {
            let file = fixture(gif::Repeat::Finite(0));
            let mut encoder = gif::Encoder::new(
                File::create(&file.path).unwrap(),
                3,
                1,
                &[255, 0, 0, 0, 255, 0, 0, 0, 255],
            )
            .unwrap();
            for (left, width, index, dispose) in [
                (0, 3, 0, gif::DisposalMethod::Keep),
                (1, 1, 1, disposal),
                (0, 1, 2, gif::DisposalMethod::Keep),
            ] {
                let mut frame = gif::Frame::default();
                frame.left = left;
                frame.width = width;
                frame.height = 1;
                frame.delay = 10;
                frame.dispose = dispose;
                frame.buffer = std::borrow::Cow::Owned(vec![index; width as usize]);
                encoder.write_frame(&frame).unwrap();
            }
            drop(encoder);
            let file = MediaFile {
                fingerprint: Fingerprint::read(&file.path).unwrap(),
                ..file
            };
            let service = AnimationService::new(|| {});
            service.open(file);
            for index in 0..3 {
                let AnimationEvent::Frame(frame) = next(&service).event else {
                    panic!("Expected composited frame");
                };
                assert_eq!(frame.image.dimensions(), (3, 1));
                if index == 1 {
                    assert_eq!(frame.image.get_pixel(1, 0).0, [0, 255, 0, 255]);
                }
                if index == 2 {
                    assert_eq!(frame.image.get_pixel(0, 0).0, [0, 0, 255, 255]);
                    if disposal == gif::DisposalMethod::Previous {
                        assert_eq!(frame.image.get_pixel(1, 0).0, [255, 0, 0, 255]);
                    } else {
                        assert_eq!(frame.image.get_pixel(1, 0).0[3], 0);
                    }
                    assert_eq!(frame.image.get_pixel(2, 0).0, [255, 0, 0, 255]);
                }
            }
        }
    }
    #[test]
    #[ignore = "Explicit fixture export for native GUI diagnostics, not a product assertion"]
    fn export_native_gif_fixtures() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test-media/gif-demo");
        assert!(
            !root.exists() || root.join(".media-browser-fixture").exists(),
            "Refusing unmarked directory"
        );
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join(".media-browser-fixture"),
            b"Generated GIF animation diagnostic samples",
        )
        .unwrap();
        for (name, repeat) in [
            ("循环.gif", gif::Repeat::Infinite),
            ("有限.gif", gif::Repeat::Finite(1)),
            ("单次.gif", gif::Repeat::Finite(0)),
        ] {
            let file = fixture(repeat);
            std::fs::copy(file.path, root.join(name)).unwrap();
        }
    }
    fn apng_fixture(plays: u32, cover: bool) -> MediaFile {
        let root = std::env::temp_dir().join(format!(
            "glimpse-apng-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join(if cover { "动画.apng" } else { "动画.png" });
        let mut encoder = png::Encoder::new(File::create(&path).unwrap(), 4, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_animated(4, plays).unwrap();
        encoder.set_sep_def_img(cover).unwrap();
        let mut writer = encoder.write_header().unwrap();
        if cover {
            writer
                .write_image_data(&[10, 20, 30, 255].repeat(8))
                .unwrap();
        }
        writer.set_frame_delay(1, 10).unwrap();
        writer.set_frame_dimension(4, 2).unwrap();
        writer.set_frame_position(0, 0).unwrap();
        writer.set_blend_op(png::BlendOp::Source).unwrap();
        writer.set_dispose_op(png::DisposeOp::None).unwrap();
        writer
            .write_image_data(&[255, 0, 0, 255].repeat(8))
            .unwrap();
        writer.set_frame_delay(7, 20).unwrap();
        writer.set_frame_dimension(2, 2).unwrap();
        writer.set_frame_position(0, 0).unwrap();
        writer.set_blend_op(png::BlendOp::Over).unwrap();
        writer.set_dispose_op(png::DisposeOp::Previous).unwrap();
        writer
            .write_image_data(&[0, 255, 0, 128].repeat(4))
            .unwrap();
        writer.set_frame_delay(2, 25).unwrap();
        writer.set_frame_dimension(2, 2).unwrap();
        writer.set_frame_position(2, 0).unwrap();
        writer.set_blend_op(png::BlendOp::Source).unwrap();
        writer.set_dispose_op(png::DisposeOp::Background).unwrap();
        writer
            .write_image_data(&[0, 0, 255, 255].repeat(4))
            .unwrap();
        writer.set_frame_delay(17, 100).unwrap();
        writer.set_frame_dimension(1, 1).unwrap();
        writer.set_frame_position(0, 0).unwrap();
        writer.set_blend_op(png::BlendOp::Source).unwrap();
        writer.set_dispose_op(png::DisposeOp::None).unwrap();
        writer.write_image_data(&[0, 255, 0, 255]).unwrap();
        writer.finish().unwrap();
        MediaFile {
            fingerprint: Fingerprint::read(&path).unwrap(),
            path,
            parent: root,
            kind: MediaKind::Image,
        }
    }
    #[test]
    fn apng_partial_alpha_disposal_loop_count_and_separate_cover() {
        for cover in [false, true] {
            let file = apng_fixture(2, cover);
            let before = std::fs::read(&file.path).unwrap();
            assert!(probe_animated(&file.path).unwrap());
            let service = AnimationService::new(|| {});
            let token = service.open(file.clone());
            for index in 0..8 {
                let value = next(&service);
                assert_eq!(value.token, token);
                let AnimationEvent::Frame(frame) = value.event else {
                    panic!("Missing APNG frame")
                };
                assert_eq!(frame.sequence, index);
                assert_eq!(frame.cycle, index as u32 / 4);
                assert_eq!(
                    frame.delay.as_millis(),
                    [100, 350, 80, 170][index as usize % 4]
                );
                let colors = (frame.image.get_pixel(0, 0).0, frame.image.get_pixel(3, 0).0);
                match index % 4 {
                    0 => assert_eq!(colors, ([255, 0, 0, 255], [255, 0, 0, 255])),
                    1 => assert_eq!(colors, ([127, 128, 0, 255], [255, 0, 0, 255])),
                    2 => assert_eq!(colors, ([255, 0, 0, 255], [0, 0, 255, 255])),
                    _ => assert_eq!(colors, ([0, 255, 0, 255], [0, 0, 0, 0])),
                }
            }
            assert!(matches!(next(&service).event, AnimationEvent::End));
            assert_eq!(std::fs::read(&file.path).unwrap(), before);
        }
    }
    #[test]
    fn apng_infinite_queue_remains_bounded_and_cancel_releases_source() {
        let file = apng_fixture(0, false);
        let service = AnimationService::new(|| {});
        service.open(file);
        thread::sleep(Duration::from_millis(100));
        assert!(service.frames_sent() <= 2);
        service.cancel();
        let began = Instant::now();
        while !service.released() {
            assert!(began.elapsed() < Duration::from_secs(3));
            thread::sleep(Duration::from_millis(2));
        }
    }
    #[test]
    fn apng_sixteen_bit_normalizes_to_existing_sdr_path_and_rejects_truncation() {
        let file = apng_fixture(1, false);
        let mut encoder = png::Encoder::new(File::create(&file.path).unwrap(), 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Sixteen);
        encoder.set_animated(1, 1).unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer.set_frame_delay(0, 0).unwrap();
        writer
            .write_image_data(&[255, 255, 128, 128, 0, 0, 255, 255])
            .unwrap();
        writer.finish().unwrap();
        let file = MediaFile {
            fingerprint: Fingerprint::read(&file.path).unwrap(),
            ..file
        };
        let bytes = std::fs::read(&file.path).unwrap();
        let service = AnimationService::new(|| {});
        service.open(file.clone());
        let AnimationEvent::Frame(frame) = next(&service).event else {
            panic!("16-bit APNG frame missing")
        };
        assert_eq!(frame.image.get_pixel(0, 0).0, [255, 128, 0, 255]);
        assert_eq!(frame.delay, Duration::from_millis(100));
        assert!(matches!(next(&service).event, AnimationEvent::End));
        service.cancel();
        while !service.released() {
            thread::sleep(Duration::from_millis(2));
        }
        std::fs::write(&file.path, &bytes[..bytes.len() / 2]).unwrap();
        let truncated = MediaFile {
            fingerprint: Fingerprint::read(&file.path).unwrap(),
            ..file
        };
        service.open(truncated);
        assert!(matches!(next(&service).event, AnimationEvent::Error(_)));
    }
}
