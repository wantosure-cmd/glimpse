//! Bounded viewport-driven media decoding. Source files are read only.
use image::{DynamicImage, ImageDecoder, ImageEncoder};
use media_browser::domain::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{mpsc, Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant, UNIX_EPOCH},
};

#[derive(Debug, Clone)]
pub struct ThumbJob {
    pub generation: u64,
    pub id: MediaId,
    pub file: MediaFile,
    pub bucket: u32,
    pub decode_bucket: u32,
    pub region: Option<media_browser::long_png::Region>,
    pub priority: u8,
}
impl ThumbJob {
    pub fn key(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(b"thumbnail-v4-scaled-jpeg-fast-png");
        if self.file.kind == MediaKind::Video {
            hash.update(b"video-v5-bounded-fragmented-preview");
        }
        if self
            .file
            .path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("png") || e.eq_ignore_ascii_case("apng"))
        {
            hash.update(b"png-animation-metadata-v1");
        }
        hash.update(path_blob(&self.file.path));
        hash.update(self.file.fingerprint.bytes.to_le_bytes());
        hash.update(self.file.fingerprint.modified_ns.to_le_bytes());
        hash.update(self.bucket.to_le_bytes());
        if let Some(r) = self.region {
            hash.update(b"long-png-viewport-v1");
            for n in [r.x, r.y, r.width, r.height] {
                hash.update(n.to_le_bytes());
            }
        }
        format!("{:x}", hash.finalize())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbMeta {
    #[serde(default)]
    pub animated: bool,
    pub width: u32,
    pub height: u32,
    pub duration_ms: Option<u64>,
}
#[derive(Debug)]
pub struct ThumbPixels {
    pub image: image::RgbaImage,
    pub meta: ThumbMeta,
    pub cache_hit: bool,
}
pub struct ThumbMetadata {
    pub job: ThumbJob,
    pub meta: ThumbMeta,
}
pub struct ThumbResult {
    pub job: ThumbJob,
    pub key: String,
    pub result: Result<ThumbPixels, String>,
}
// Diagnostic array order: cache lookup/preparation,header/probe,FFmpeg process,
// temporary PNG decode,temporary cleanup,final cache encode/write.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct VideoPhaseStats {
    pub enabled: bool,
    pub counts: [u64; 6],
    pub milliseconds: [f64; 6],
}
fn video_phase_began(enabled: bool) -> Option<Instant> {
    enabled.then(Instant::now)
}
fn video_phase_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var_os("GLIMPSE_VIDEO_PHASE_TIMING").as_deref() == Some(std::ffi::OsStr::new("1"))
    })
}
fn record_video_phase(state: &Arc<(Mutex<State>, Condvar)>, phase: usize, began: Option<Instant>) {
    if let Some(began) = began {
        let ms = began.elapsed().as_secs_f64() * 1000.;
        let mut state = state.0.lock().unwrap();
        state.stats.video_phases.enabled = true;
        state.stats.video_phases.counts[phase] += 1;
        state.stats.video_phases.milliseconds[phase] += ms;
    }
}
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct ThumbnailStats {
    pub video_phases: VideoPhaseStats,
    pub video_fast_header_hits: u64,
    pub full_image_workers: usize,
    pub scaled_jpeg_workers: usize,
    pub cache_hits: u64,
    pub decodes: u64,
    pub cache_hit_ms: f64,
    pub decode_ms: f64,
    pub maintenance_runs: u64,
    pub maintenance_ms: f64,
    pub discarded_results: u64,
    pub source_decodes: u64,
    pub source_reuses: u64,
    pub metadata_ready_direct: u64,
}
#[derive(Default)]
struct State {
    stats: ThumbnailStats,
    foreground_active: usize,
    background_active: bool,
    generation: u64,
    images: VecDeque<ThumbJob>,
    videos: VecDeque<ThumbJob>,
    wanted: HashSet<String>,
    inflight: HashSet<String>,
    delivered: HashSet<String>,
    ready: VecDeque<ThumbResult>,
    blocked: HashSet<MediaId>,
    active: HashMap<MediaId, usize>,
    // Reuse bounded decoded pixels between progressive levels. JPEG intermediates
    // are released once the clear thumbnail is produced.
    decoded: VecDeque<(String, Arc<image::RgbaImage>, ThumbMeta)>,
    decoded_bytes: usize,
    jpeg_sources: HashSet<String>,
    maintenance_pending: bool,
    animation_pressure: bool,
    trim_animation_cache: bool,
    maintenance_started: bool,
    cache_writes: u64,
    stop: bool,
}
pub struct ThumbnailService {
    shared: Arc<(Mutex<State>, Condvar)>,
    metadata: mpsc::Receiver<ThumbMetadata>,
    workers: Vec<thread::JoinHandle<()>>,
}
impl ThumbnailService {
    pub fn new(cache: PathBuf, wake: impl Fn() + Send + Sync + 'static) -> Self {
        let repeat = std::env::var_os("GLIMPSE_REPEAT_METADATA_READY").as_deref()
            == Some(std::ffi::OsStr::new("1"));
        Self::new_with_repeated_metadata_ready(cache, wake, repeat)
    }
    fn new_with_repeated_metadata_ready(
        cache: PathBuf,
        wake: impl Fn() + Send + Sync + 'static,
        repeat_metadata_ready: bool,
    ) -> Self {
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let (metadata_tx, metadata) = mpsc::sync_channel(64);
        let wake = Arc::new(wake);
        let mut workers = Vec::new();
        // Preserve two full-image decoders; extra lanes only decode bounded scaled JPEGs.
        // Large PNG/HEIC/TIFF and native-size detail requests retain the previous concurrency.
        let extra_jpeg = std::thread::available_parallelism()
            .map(|n| (n.get() / 4).clamp(2, 4) - 2)
            .unwrap_or(0);
        let lend_video_workers = std::thread::available_parallelism().is_ok_and(|n| n.get() >= 8);
        let lanes = [0u8, 0, 2, 2]
            .into_iter()
            .chain(std::iter::repeat_n(1, extra_jpeg));
        {
            let mut state = shared.0.lock().unwrap();
            state.stats.full_image_workers = 2;
            state.stats.scaled_jpeg_workers = extra_jpeg;
        }
        for lane in lanes {
            let shared = shared.clone();
            let metadata_tx = metadata_tx.clone();
            let cache = cache.clone();
            let wake = wake.clone();
            workers.push(thread::spawn(move || {
                loop {
                    let job = {
                        let mut state = shared.0.lock().unwrap();
                        loop {
                            if state.stop {
                                return;
                            }
                            let job = if lane == 2 {
                                if state.videos.front().is_some_and(|j| {
                                    j.priority < 4
                                        || (state.foreground_active == 0
                                            && !state.background_active)
                                }) {
                                    state.videos.pop_front()
                                } else {
                                    None
                                }
                            } else if lane == 1 {
                                state
                                    .images
                                    .iter()
                                    .position(|job| {
                                        scaled_jpeg_job(job) && !state.active.contains_key(&job.id)
                                    })
                                    .and_then(|i| state.images.remove(i))
                            } else {
                                state
                                    .images
                                    .iter()
                                    .position(|job| !state.active.contains_key(&job.id))
                                    .and_then(|i| state.images.remove(i))
                                    .or_else(|| {
                                        // Idle image lanes can help a local video-only first screen.
                                        // Never lend to prefetch, metadata or virtual drives; keep
                                        // their existing two-video limit and foreground priority.
                                        if lend_video_workers && can_lend_video_worker(&state) {
                                            state.videos.pop_front()
                                        } else {
                                            None
                                        }
                                    })
                            };
                            if let Some(job) = job {
                                if state.blocked.contains(&job.id) {
                                    continue;
                                }
                                state.inflight.insert(job.key());
                                *state.active.entry(job.id).or_default() += 1;
                                if job.priority < 4 {
                                    state.foreground_active += 1;
                                } else {
                                    state.background_active = true;
                                }
                                break job;
                            }
                            state = shared.1.wait(state).unwrap();
                        }
                    };
                    let key = job.key();
                    let metadata_only = job.bucket == 0;
                    let began = Instant::now();
                    let metadata_sent = std::cell::Cell::new(false);
                    let notify_meta = |meta| {
                        let valid = {
                            let state = shared.0.lock().unwrap();
                            !state.stop
                                && state.generation == job.generation
                                && !state.blocked.contains(&job.id)
                                && state.wanted.contains(&key)
                        };
                        if valid
                            && metadata_tx
                                .try_send(ThumbMetadata {
                                    job: job.clone(),
                                    meta,
                                })
                                .is_ok()
                        {
                            metadata_sent.set(true);
                            wake();
                        }
                    };
                    // A warm clear thumbnail can satisfy the initial small-preview
                    // request directly. Keep cancellation tied to that request,
                    // but deliver the real bucket/key so the UI needs one upload.
                    let (delivery_job, result) = if let Some((clear, pixels)) =
                        cached_clear_preview(&job, &cache)
                    {
                        notify_meta(pixels.meta.clone());
                        (clear, Ok(pixels))
                    } else {
                        let result = decode_job_with_metadata(&job, &cache, &shared, &notify_meta);
                        (job.clone(), result)
                    };
                    let deliver = {
                        let mut state = shared.0.lock().unwrap();
                        state.inflight.remove(&key);
                        if let Ok(pixels) = &result {
                            if !metadata_only {
                                if pixels.cache_hit {
                                    state.stats.cache_hits += 1;
                                    state.stats.cache_hit_ms +=
                                        began.elapsed().as_secs_f64() * 1000.;
                                } else {
                                    state.stats.decodes += 1;
                                    state.stats.decode_ms += began.elapsed().as_secs_f64() * 1000.;
                                }
                            }
                        }
                        if result
                            .as_ref()
                            .is_ok_and(|p| !metadata_only && !p.cache_hit)
                        {
                            state.cache_writes += 1;
                        }
                        if !metadata_only
                            && result.is_ok()
                            && (!state.maintenance_started || state.cache_writes >= 30)
                        {
                            state.maintenance_started = true;
                            state.cache_writes = 0;
                            state.maintenance_pending = true;
                        }
                        if job.priority < 4 {
                            state.foreground_active = state.foreground_active.saturating_sub(1);
                        } else {
                            state.background_active = false;
                        }
                        if let Some(count) = state.active.get_mut(&job.id) {
                            *count -= 1;
                            if *count == 0 {
                                state.active.remove(&job.id);
                            }
                        }
                        let valid = !state.stop
                            && !state.blocked.contains(&job.id)
                            && state.generation == job.generation
                            && state.wanted.contains(&key)
                            && !result
                                .as_ref()
                                .err()
                                .is_some_and(|e| e == "background-preempted");
                        if valid {
                            state.delivered.insert(key.clone());
                        }
                        valid
                    };
                    wake();
                    shared.1.notify_all();
                    if deliver {
                        if metadata_only
                            && result.is_ok()
                            && metadata_sent.get()
                            && !repeat_metadata_ready
                        {
                            // The existing64-slot channel already owns this metadata.
                            // Do not block geometry probes behind eight pixel results.
                            // Errors or a full metadata channel keep the original fallback.
                            shared.0.lock().unwrap().stats.metadata_ready_direct += 1;
                            continue;
                        }
                        let delivery_key = delivery_job.key();
                        let event = ThumbResult {
                            job: delivery_job,
                            key: delivery_key,
                            result,
                        };
                        if !send_ready(&shared, event, &*wake) {
                            return;
                        }
                    }
                }
            }));
        }
        let maintenance_state = shared.clone();
        workers.push(thread::spawn(move || {
            let mut earliest = Instant::now() + Duration::from_secs(2);
            loop {
                let mut state = maintenance_state.0.lock().unwrap();
                while !state.stop
                    && !state.trim_animation_cache
                    && (!state.maintenance_pending || Instant::now() < earliest)
                {
                    state = if state.maintenance_pending {
                        maintenance_state
                            .1
                            .wait_timeout(state, earliest.saturating_duration_since(Instant::now()))
                            .unwrap()
                            .0
                    } else {
                        maintenance_state.1.wait(state).unwrap()
                    };
                }
                if state.stop {
                    return;
                }
                if state.trim_animation_cache {
                    let mut retired = Vec::new();
                    while state.decoded_bytes > 32 * 1024 * 1024 {
                        let Some((key, pixels, _)) = state.decoded.pop_front() else {
                            break;
                        };
                        state.jpeg_sources.remove(&key);
                        state.decoded_bytes -= pixels.as_raw().len();
                        retired.push(pixels);
                    }
                    state.trim_animation_cache = false;
                    drop(state);
                    drop(retired);
                    continue;
                }
                state.maintenance_pending = false;
                drop(state);
                let began = Instant::now();
                let _ = prune_cache_cancelled(&cache, 2 * 1024 * 1024 * 1024, &|| {
                    maintenance_state.0.lock().unwrap().stop
                });
                {
                    let mut state = maintenance_state.0.lock().unwrap();
                    state.stats.maintenance_runs += 1;
                    state.stats.maintenance_ms += began.elapsed().as_secs_f64() * 1000.;
                }
                earliest = Instant::now() + Duration::from_secs(2);
            }
        }));
        Self {
            shared,
            metadata,
            workers,
        }
    }
    pub fn set_animation_pressure(&self, active: bool) {
        let mut state = self.shared.0.lock().unwrap();
        if state.animation_pressure == active {
            return;
        }
        state.animation_pressure = active;
        if active && state.decoded_bytes > 32 * 1024 * 1024 {
            state.trim_animation_cache = true;
        }
        drop(state);
        self.shared.1.notify_all();
    }
    pub fn set_requests(&self, generation: u64, mut jobs: Vec<ThumbJob>) {
        jobs.sort_by_key(|j| j.priority);
        jobs.truncate(256);
        let mut state = self.shared.0.lock().unwrap();
        if state.generation != generation {
            state.generation = generation;
            state.delivered.clear();
            state.decoded.clear();
            state.decoded_bytes = 0;
            state.jpeg_sources.clear();
        }
        jobs.retain(|job| !state.blocked.contains(&job.id));
        state.images.clear();
        state.videos.clear();
        state.wanted = jobs.iter().map(ThumbJob::key).collect();
        let wanted = state.wanted.clone();
        state.delivered.retain(|k| wanted.contains(k));
        let mut queued = HashSet::new();
        for job in jobs {
            let key = job.key();
            if state.inflight.contains(&key)
                || state.delivered.contains(&key)
                || !queued.insert(key)
            {
                continue;
            }
            match job.file.kind {
                MediaKind::Image => state.images.push_back(job),
                MediaKind::Video => state.videos.push_back(job),
            }
        }
        drop(state);
        self.shared.1.notify_all();
    }
    /// Atomically prevents queued/new opens, then lets already running decodes close naturally.
    pub fn block_sources(&self, ids: &HashSet<MediaId>) {
        let mut state = self.shared.0.lock().unwrap();
        state.blocked.extend(ids.iter().copied());
        state.decoded.clear();
        state.decoded_bytes = 0;
        state.jpeg_sources.clear();
        state.images.retain(|job| !ids.contains(&job.id));
        state.videos.retain(|job| !ids.contains(&job.id));
        drop(state);
        self.shared.1.notify_all();
    }
    pub fn sources_released(&self, ids: &HashSet<MediaId>) -> bool {
        let state = self.shared.0.lock().unwrap();
        ids.iter()
            .all(|id| state.blocked.contains(id) && !state.active.contains_key(id))
    }
    pub fn unblock_sources(&self, ids: &HashSet<MediaId>) {
        let mut state = self.shared.0.lock().unwrap();
        state.blocked.retain(|id| !ids.contains(id));
    }
    pub fn stats(&self) -> ThumbnailStats {
        self.shared.0.lock().unwrap().stats
    }
    pub fn poll_metadata(&self) -> Option<ThumbMetadata> {
        self.metadata.try_recv().ok()
    }
    pub fn poll_metadata_result(&self) -> Option<ThumbResult> {
        let mut state = self.shared.0.lock().unwrap();
        let index = state.ready.iter().position(|event| event.job.bucket == 0)?;
        let event = state.ready.remove(index);
        drop(state);
        self.shared.1.notify_all();
        event
    }
    pub fn poll(&self) -> Option<ThumbResult> {
        take_ready(&self.shared)
    }
    /// Extra list-only pixels: preserve detail/prefetch/generation/byte guards; video posters are opt-in.
    #[cfg(test)]
    pub fn poll_visible_image(
        &self,
        max_bytes: usize,
        visible: impl Fn(MediaId) -> bool,
    ) -> Option<ThumbResult> {
        self.poll_visible_list_pixels(max_bytes, false, visible)
    }

    pub fn poll_visible_list_pixels(
        &self,
        max_bytes: usize,
        allow_video_posters: bool,
        visible: impl Fn(MediaId) -> bool,
    ) -> Option<ThumbResult> {
        let mut state = self.shared.0.lock().unwrap();
        let index = state.ready.iter().position(|event| {
            event.job.generation == state.generation
                && !state.blocked.contains(&event.job.id)
                && event.job.priority == 1
                && (event.job.file.kind == MediaKind::Image
                    || allow_video_posters && event.job.file.kind == MediaKind::Video)
                && event.job.bucket > 0
                && visible(event.job.id)
                && event.result.as_ref().is_ok_and(|pixels| {
                    !pixels.image.is_empty() && pixels.image.as_raw().len() <= max_bytes
                })
        })?;
        let event = state.ready.remove(index);
        drop(state);
        self.shared.1.notify_all();
        event
    }
}

fn send_ready(shared: &Arc<(Mutex<State>, Condvar)>, event: ThumbResult, wake: &dyn Fn()) -> bool {
    let mut state = shared.0.lock().unwrap();
    // Keep exactly the previous channel's eight queued results. Pixels and
    // file decoding stay on workers; the UI uses bounded time/byte/count draining.
    while !state.stop && state.ready.len() >= 8 {
        state = shared
            .1
            .wait_timeout(state, Duration::from_millis(2))
            .unwrap()
            .0;
    }
    if state.stop {
        return false;
    }
    state.ready.push_back(event);
    drop(state);
    wake();
    true
}

fn take_ready(shared: &Arc<(Mutex<State>, Condvar)>) -> Option<ThumbResult> {
    let mut state = shared.0.lock().unwrap();
    let index = state
        .ready
        .iter()
        .enumerate()
        .min_by_key(|(index, event)| (event.job.priority, *index))
        .map(|(index, _)| index)?;
    let event = state.ready.remove(index);
    drop(state);
    shared.1.notify_all();
    event
}
fn can_lend_video_worker(state: &State) -> bool {
    state.images.is_empty()
        && state.foreground_active < 4
        && state.videos.front().is_some_and(|job| {
            job.priority == 1
                && job.bucket > 0
                && !state.active.contains_key(&job.id)
                && !media_browser::media_path::virtual_hint(&job.file.path)
        })
}
impl Drop for ThumbnailService {
    fn drop(&mut self) {
        self.shared.0.lock().unwrap().stop = true;
        self.shared.1.notify_all();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

pub fn fingerprint(path: &Path) -> Result<Fingerprint, String> {
    Fingerprint::read(path)
}
#[cfg(test)]
fn decode_job(
    job: &ThumbJob,
    cache: &Path,
    state: &Arc<(Mutex<State>, Condvar)>,
) -> Result<ThumbPixels, String> {
    decode_job_with_metadata(job, cache, state, &|_| {})
}
fn decode_job_with_metadata(
    job: &ThumbJob,
    cache: &Path,
    state: &Arc<(Mutex<State>, Condvar)>,
    notify: &dyn Fn(ThumbMeta),
) -> Result<ThumbPixels, String> {
    let video_timing = job.bucket > 0 && job.file.kind == MediaKind::Video && video_phase_enabled();
    let prepare_began = video_phase_began(video_timing);
    if fingerprint(&job.file.path)? != job.file.fingerprint {
        return Err(media_browser::ui_text!(msg_43aa5799789d).into());
    }
    std::fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    let key = job.key();
    if job.bucket == 0 {
        let meta = match job.file.kind {
            MediaKind::Image => probe_still_meta(&job.file.path)?,
            MediaKind::Video => decode_video(&job.file.path, 0, cache, &key, state, notify)?.1,
        };
        if fingerprint(&job.file.path)? != job.file.fingerprint {
            return Err(media_browser::ui_text!(msg_43aa5799789d).into());
        }
        notify(meta.clone());
        return Ok(ThumbPixels {
            image: image::RgbaImage::new(0, 0),
            meta,
            cache_hit: false,
        });
    }
    let image_path = cache.join(format!("{key}.png"));
    let meta_path = cache.join(format!("{key}.json"));
    if let Ok(bytes) = std::fs::read(&meta_path) {
        if let Ok(meta) = serde_json::from_slice::<ThumbMeta>(&bytes) {
            if meta.width > 0 && meta.height > 0 {
                notify(meta);
            }
        }
    } else if job.file.kind == MediaKind::Image {
        if let Ok(meta) = probe_still_meta(&job.file.path) {
            if fingerprint(&job.file.path)? == job.file.fingerprint {
                notify(meta);
            }
        }
    }
    if let Some(pixels) = load_cached_thumbnail(job, cache)? {
        return Ok(pixels);
    }
    record_video_phase(state, 0, prepare_began);
    let (image, meta) = match job.file.kind {
        MediaKind::Image => {
            let (image, meta) = decode_preview_cached(job, state)?;
            let image =
                if job.bucket > 0 && (image.width() > job.bucket || image.height() > job.bucket) {
                    DynamicImage::ImageRgba8(image)
                        .thumbnail(job.bucket, job.bucket)
                        .into_rgba8()
                } else {
                    image
                };
            let image = if job.bucket == 64 {
                image::imageops::blur(&image, 1.2)
            } else {
                image
            };
            (image, meta)
        }
        MediaKind::Video => decode_video(
            &job.file.path,
            job.bucket.max(256),
            cache,
            &key,
            state,
            notify,
        )?,
    };
    let cache_write_began = video_phase_began(video_timing);
    if fingerprint(&job.file.path)? != job.file.fingerprint {
        return Err(media_browser::ui_text!(msg_43aa5799789d).into());
    }
    let temporary = cache.join(format!("{key}.image.tmp"));
    let output = std::fs::File::create(&temporary).map_err(|e| e.to_string())?;
    image::codecs::png::PngEncoder::new_with_quality(
        output,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Sub,
    )
    .write_image(
        image.as_raw(),
        image.width(),
        image.height(),
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|e| e.to_string())?;
    replace_cache_file(&temporary, &image_path)?;
    let temporary = cache.join(format!("{key}.meta.tmp"));
    std::fs::write(&temporary, serde_json::to_vec(&meta).unwrap()).map_err(|e| e.to_string())?;
    replace_cache_file(&temporary, &meta_path)?;
    record_video_phase(state, 5, cache_write_began);
    Ok(ThumbPixels {
        image,
        meta,
        cache_hit: false,
    })
}
fn load_cached_thumbnail(job: &ThumbJob, cache: &Path) -> Result<Option<ThumbPixels>, String> {
    let key = job.key();
    let image_path = cache.join(format!("{key}.png"));
    let meta_path = cache.join(format!("{key}.json"));
    let Ok(bytes) = std::fs::read(&meta_path) else {
        return Ok(None);
    };
    let Ok(meta) = serde_json::from_slice::<ThumbMeta>(&bytes) else {
        return Ok(None);
    };
    if meta.width == 0 || meta.height == 0 {
        return Ok(None);
    }
    let Ok(image) = image::open(&image_path) else {
        return Ok(None);
    };
    if fingerprint(&job.file.path)? != job.file.fingerprint {
        return Err(media_browser::ui_text!(msg_43aa5799789d).into());
    }
    if let Ok(file) = std::fs::File::options().write(true).open(&image_path) {
        let _ = file.set_modified(std::time::SystemTime::now());
    }
    Ok(Some(ThumbPixels {
        image: image.into_rgba8(),
        meta,
        cache_hit: true,
    }))
}

fn cached_clear_preview(job: &ThumbJob, cache: &Path) -> Option<(ThumbJob, ThumbPixels)> {
    if job.file.kind != MediaKind::Image
        || job.bucket != 64
        || !(65..=1024).contains(&job.decode_bucket)
    {
        return None;
    }
    let mut clear = job.clone();
    clear.bucket = clear.decode_bucket;
    let pixels = load_cached_thumbnail(&clear, cache).ok()??;
    Some((clear, pixels))
}

fn replace_cache_file(source: &Path, target: &Path) -> Result<(), String> {
    // Both paths are generated cache files. Never use this helper for source media.
    std::fs::rename(source, target).map_err(|e| e.to_string())
}
pub fn probe_still_meta(path: &Path) -> Result<ThumbMeta, String> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let (width, height) = if matches!(ext.as_str(), "jpg" | "jpeg") {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut decoder = jpeg_decoder::Decoder::new(std::io::BufReader::new(file));
        decoder.read_info().map_err(|e| e.to_string())?;
        let info = decoder.info().ok_or("Missing JPEG dimensions")?;
        let orientation = decoder
            .exif_data()
            .and_then(image::metadata::Orientation::from_exif_chunk)
            .unwrap_or(image::metadata::Orientation::NoTransforms);
        let (w, h) = (u32::from(info.width), u32::from(info.height));
        if matches!(
            orientation,
            image::metadata::Orientation::Rotate90
                | image::metadata::Orientation::Rotate270
                | image::metadata::Orientation::Rotate90FlipH
                | image::metadata::Orientation::Rotate270FlipH
        ) {
            (h, w)
        } else {
            (w, h)
        }
    } else if matches!(ext.as_str(), "heic" | "heif") {
        crate::heif::Heif::new()?.dimensions(path)?
    } else {
        let reader = image::ImageReader::open(path)
            .map_err(|e| e.to_string())?
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
        let (w, h) = decoder.dimensions();
        let orientation = decoder.orientation().map_err(|e| e.to_string())?;
        if matches!(
            orientation,
            image::metadata::Orientation::Rotate90
                | image::metadata::Orientation::Rotate270
                | image::metadata::Orientation::Rotate90FlipH
                | image::metadata::Orientation::Rotate270FlipH
        ) {
            (h, w)
        } else {
            (w, h)
        }
    };
    if width == 0
        || height == 0
        || if ext == "png" && width.max(height) > 16384 {
            !media_browser::long_png::supported_size(width, height)
        } else {
            u64::from(width) * u64::from(height) > 64_000_000 || width > 16384 || height > 16384
        }
    {
        return Err("Invalid or oversized image dimensions".into());
    }
    // Keep the returned orientation identical to the pixel decoder.
    Ok(ThumbMeta {
        animated: media_browser::animation::probe_animated(path)?,
        width,
        height,
        duration_ms: None,
    })
}

fn decode_preview_cached(
    job: &ThumbJob,
    shared: &Arc<(Mutex<State>, Condvar)>,
) -> Result<(image::RgbaImage, ThumbMeta), String> {
    if job
        .file
        .path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("png"))
    {
        let (w, h) = image::image_dimensions(&job.file.path).map_err(|e| e.to_string())?;
        if w > 16384 || h > 16384 {
            let key = job.key();
            let (pixels, (width, height)) =
                media_browser::long_png::decode(&job.file.path, job.bucket, job.region, || {
                    let state = shared.0.lock().unwrap();
                    state.stop
                        || state.generation != job.generation
                        || state.blocked.contains(&job.id)
                        || !state.wanted.contains(&key)
                })?;
            return Ok((
                pixels,
                ThumbMeta {
                    animated: media_browser::animation::probe_animated(&job.file.path)?,
                    width,
                    height,
                    duration_ms: None,
                },
            ));
        }
    }
    let jpeg = job
        .file
        .path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"));
    let mut identity = job.clone();
    identity.bucket = 0;
    let identity = identity.key();
    let cached = {
        let mut state = shared.0.lock().unwrap();
        state
            .decoded
            .iter()
            .position(|(key, pixels, meta)| {
                key == &identity
                    && (!jpeg
                        || pixels.width().max(pixels.height()) >= job.bucket
                        || pixels.width().max(pixels.height()) >= meta.width.max(meta.height))
            })
            .map(|i| {
                let entry = state.decoded.remove(i).unwrap();
                let result = (entry.1.clone(), entry.2.clone());
                state.decoded.push_back(entry);
                state.stats.source_reuses += 1;
                result
            })
    };
    let (pixels, meta) = if let Some(cached) = cached {
        cached
    } else {
        let (pixels, meta) = if jpeg {
            let target = if job.decode_bucket <= 1024 {
                job.bucket.max(job.decode_bucket)
            } else {
                job.bucket
            };
            let (pixels, meta) = decode_preview(&job.file.path, target)?;
            (Arc::new(pixels), meta)
        } else {
            let pixels = Arc::new(decode_still(&job.file.path)?);
            let meta = ThumbMeta {
                animated: media_browser::animation::probe_animated(&job.file.path)?,
                width: pixels.width(),
                height: pixels.height(),
                duration_ms: None,
            };
            (pixels, meta)
        };
        let bytes = pixels.as_raw().len();
        let mut state = shared.0.lock().unwrap();
        state.stats.source_decodes += 1;
        let budget: usize = if state.animation_pressure {
            32 * 1024 * 1024
        } else {
            128 * 1024 * 1024
        };
        if let Some(index) = state
            .decoded
            .iter()
            .position(|(key, _, _)| key == &identity)
        {
            let (_, old, _) = state.decoded.remove(index).unwrap();
            state.decoded_bytes -= old.as_raw().len();
            state.jpeg_sources.remove(&identity);
        }
        let bridge = !jpeg
            || (job.bucket == 64
                && job.decode_bucket > 64
                && pixels.width().max(pixels.height()) >= job.decode_bucket);
        if bytes <= budget
            && bridge
            && (!jpeg || bytes <= 32 * 1024 * 1024)
            && state.generation == job.generation
            && !state.blocked.contains(&job.id)
        {
            if jpeg {
                loop {
                    let jpeg_bytes: usize = state
                        .decoded
                        .iter()
                        .filter(|(key, _, _)| state.jpeg_sources.contains(key))
                        .map(|(_, image, _)| image.as_raw().len())
                        .sum();
                    if jpeg_bytes + bytes <= 32 * 1024 * 1024 {
                        break;
                    }
                    let Some(index) = state
                        .decoded
                        .iter()
                        .position(|(key, _, _)| state.jpeg_sources.contains(key))
                    else {
                        break;
                    };
                    let (key, old, _) = state.decoded.remove(index).unwrap();
                    state.jpeg_sources.remove(&key);
                    state.decoded_bytes -= old.as_raw().len();
                }
            }
            while state.decoded_bytes + bytes > budget {
                if let Some((key, old, _)) = state.decoded.pop_front() {
                    state.jpeg_sources.remove(&key);
                    state.decoded_bytes -= old.as_raw().len();
                } else {
                    break;
                }
            }
            if jpeg {
                state.jpeg_sources.insert(identity.clone());
            }
            state.decoded_bytes += bytes;
            state
                .decoded
                .push_back((identity.clone(), pixels.clone(), meta.clone()));
        }
        (pixels, meta)
    };
    let image = if job.bucket > 0 && pixels.width().max(pixels.height()) > job.bucket {
        let scale = f64::from(job.bucket) / f64::from(pixels.width().max(pixels.height()));
        image::imageops::thumbnail(
            pixels.as_ref(),
            (f64::from(pixels.width()) * scale).round().max(1.) as u32,
            (f64::from(pixels.height()) * scale).round().max(1.) as u32,
        )
    } else {
        pixels.as_ref().clone()
    };
    // JPEG intermediates exist only to bridge soft -> clear. Once the clear
    // thumbnail is ready its GPU/disk copies cover revisits; retaining a whole
    // decoded source per scrolled-past JPEG needlessly fills the pixel cache.
    if (jpeg || meta.animated) && job.bucket > 64 {
        let mut state = shared.0.lock().unwrap();
        if let Some(index) = state
            .decoded
            .iter()
            .position(|(key, _, _)| key == &identity)
        {
            let (_, old, _) = state.decoded.remove(index).unwrap();
            state.decoded_bytes -= old.as_raw().len();
            state.jpeg_sources.remove(&identity);
        }
    }
    Ok((image, meta))
}

fn decode_preview(path: &Path, bucket: u32) -> Result<(image::RgbaImage, ThumbMeta), String> {
    if bucket > 0
        && path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
    {
        let source = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut decoder = jpeg_decoder::Decoder::new(std::io::BufReader::new(source));
        decoder.set_max_decoding_buffer_size(256 * 1024 * 1024);
        decoder.read_info().map_err(|e| e.to_string())?;
        let info = decoder
            .info()
            .ok_or(media_browser::ui_text!(msg_b7f97f5740db))?;
        let (w, h) = (u32::from(info.width), u32::from(info.height));
        if u64::from(w) * u64::from(h) > 64_000_000 || w > 16384 || h > 16384 {
            return Err(media_browser::ui_text!(msg_797d4fbe239f).into());
        }
        if bucket < w.max(h)
            && matches!(
                info.pixel_format,
                jpeg_decoder::PixelFormat::RGB24 | jpeg_decoder::PixelFormat::L8
            )
        {
            let scale = bucket as f64 / f64::from(w.max(h));
            let requested_w = (f64::from(w) * scale).ceil().max(1.) as u16;
            let requested_h = (f64::from(h) * scale).ceil().max(1.) as u16;
            let (sw, sh) = decoder
                .scale(requested_w, requested_h)
                .map_err(|e| e.to_string())?;
            let pixels = decoder.decode().map_err(|e| e.to_string())?;
            let mut image = match info.pixel_format {
                jpeg_decoder::PixelFormat::RGB24 => DynamicImage::ImageRgb8(
                    image::RgbImage::from_raw(sw.into(), sh.into(), pixels)
                        .ok_or(media_browser::ui_text!(msg_769675404fe3))?,
                ),
                _ => DynamicImage::ImageLuma8(
                    image::GrayImage::from_raw(sw.into(), sh.into(), pixels)
                        .ok_or(media_browser::ui_text!(msg_769675404fe3))?,
                ),
            };
            let orientation = decoder
                .exif_data()
                .and_then(image::metadata::Orientation::from_exif_chunk)
                .unwrap_or(image::metadata::Orientation::NoTransforms);
            image.apply_orientation(orientation);
            let swap = matches!(
                orientation,
                image::metadata::Orientation::Rotate90
                    | image::metadata::Orientation::Rotate270
                    | image::metadata::Orientation::Rotate90FlipH
                    | image::metadata::Orientation::Rotate270FlipH
            );
            let mut image = image.into_rgba8();
            if let Some(profile) = decoder.icc_profile() {
                if let Err(e) = media_browser::color::icc_to_srgb(&mut image, &profile) {
                    eprintln!("JPEG color profile: {e}");
                }
            }
            return Ok((
                image,
                ThumbMeta {
                    animated: false,
                    width: if swap { h } else { w },
                    height: if swap { w } else { h },
                    duration_ms: None,
                },
            ));
        }
    }
    let image = decode_still(path)?;
    let meta = ThumbMeta {
        animated: false,
        width: image.width(),
        height: image.height(),
        duration_ms: None,
    };
    Ok((image, meta))
}

pub fn decode_still(path: &Path) -> Result<image::RgbaImage, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(ext.as_str(), "heic" | "heif") {
        return crate::heif::Heif::new()?.decode(path);
    }
    let mut reader = image::ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(256 * 1024 * 1024);
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
    let (w, h) = decoder.dimensions();
    if u64::from(w) * u64::from(h) > 64_000_000 {
        return Err(media_browser::ui_text!(msg_fd7dc885475f).into());
    }
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let profile = decoder.icc_profile().map_err(|e| e.to_string())?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    image.apply_orientation(orientation);
    let mut image = image.into_rgba8();
    if let Some(profile) = profile {
        if let Err(e) = media_browser::color::icc_to_srgb(&mut image, &profile) {
            eprintln!("Image color profile: {e}");
        }
    }
    Ok(image)
}
fn hidden_command(exe: PathBuf) -> Command {
    let mut cmd = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd
}
fn run_process(
    mut command: Command,
    output: &Path,
    error: &Path,
    state: &Arc<(Mutex<State>, Condvar)>,
    timeout: Duration,
    background: bool,
) -> Result<(), String> {
    let stdout = std::fs::File::create(output).map_err(|e| e.to_string())?;
    let stderr = std::fs::File::create(error).map_err(|e| e.to_string())?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|e| media_browser::ui_format!(msg_e49013abfcba, e = e))?;
    let began = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(());
                }
                let bytes = std::fs::read(error).unwrap_or_default();
                return Err(media_browser::ui_format!(
                    msg_e5072d217250,
                    String::from_utf8_lossy(&bytes[..bytes.len().min(600)])
                ));
            }
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
        if background {
            let state = state.0.lock().unwrap();
            if state.foreground_active > 0
                || state.videos.front().is_some_and(|j| j.priority < 4)
                || !state.blocked.is_empty()
            {
                let _ = child.kill();
                let _ = child.wait();
                return Err("background-preempted".into());
            }
        }
        if began.elapsed() > timeout
            || state.0.lock().unwrap().stop
            || std::fs::metadata(error).is_ok_and(|m| m.len() > 512 * 1024)
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(media_browser::ui_text!(msg_353ba9444dbd).into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}
fn decode_video(
    path: &Path,
    bucket: u32,
    cache: &Path,
    key: &str,
    state: &Arc<(Mutex<State>, Condvar)>,
    notify: &dyn Fn(ThumbMeta),
) -> Result<(image::RgbaImage, ThumbMeta), String> {
    let legacy = std::env::var_os("GLIMPSE_LEGACY_SQUARE_PIXEL_HEADER").as_deref()
        == Some(std::ffi::OsStr::new("1"));
    decode_video_with_header_policy(path, bucket, cache, key, state, notify, !legacy)
}
fn decode_video_with_header_policy(
    path: &Path,
    bucket: u32,
    cache: &Path,
    key: &str,
    state: &Arc<(Mutex<State>, Condvar)>,
    notify: &dyn Fn(ThumbMeta),
    allow_non_square: bool,
) -> Result<(image::RgbaImage, ThumbMeta), String> {
    let timing = bucket > 0 && video_phase_enabled();
    let probe_began = video_phase_began(timing);
    let timeout = Duration::from_secs(if media_browser::media_path::virtual_hint(path) {
        60
    } else {
        12
    });
    let runtime = crate::native::runtime_dir();
    let probe = cache.join(format!("{key}.probe.tmp"));
    let errors = cache.join(format!("{key}.stderr.tmp"));
    let _cleanup = TemporaryFiles(vec![
        probe.clone(),
        errors.clone(),
        cache.join(format!("{key}.frame.tmp.png")),
        cache.join(format!("{key}.stdout.tmp")),
    ]);
    let mut quick = if allow_non_square {
        media_browser::mp4_header::probe(path)
    } else {
        media_browser::mp4_header::probe_square_pixels_only(path)
    };
    let streaming = quick
        .as_ref()
        .is_some_and(|h| h.fragmented && h.duration_ms.is_none());
    if bucket == 0 && streaming {
        quick = None;
    }
    if bucket > 0 && quick.is_some() {
        state.0.lock().unwrap().stats.video_fast_header_hits += 1;
    }
    let data = if let Some(meta) = quick {
        serde_json::json!({"streams":[{"codec_type":"video","width":meta.width,"height":meta.height}],"format":{"duration":meta.duration_ms.map(|ms|(ms as f64/1000.).to_string())}})
    } else {
        let mut cmd = hidden_command(runtime.join("ffprobe.exe"));
        cmd.args(["-v","error","-protocol_whitelist","file,pipe","-show_entries","format=duration:stream=codec_type,width,height,duration,sample_aspect_ratio:stream_side_data=rotation","-of","json"]).arg(path);
        run_process(cmd, &probe, &errors, state, timeout, bucket == 0)?;
        let data = std::fs::read(&probe).map_err(|e| e.to_string())?;
        if data.len() > 1024 * 1024 {
            return Err(media_browser::ui_text!(msg_7fecc448a328).into());
        }
        serde_json::from_slice::<serde_json::Value>(&data).map_err(|e| e.to_string())?
    };
    let stream = data["streams"]
        .as_array()
        .and_then(|s| s.iter().find(|s| s["codec_type"] == "video"))
        .ok_or(media_browser::ui_text!(msg_0f8b140bb123))?;
    let mut width = stream["width"].as_u64().unwrap_or(0) as u32;
    let mut height = stream["height"].as_u64().unwrap_or(0) as u32;
    if let Some((n, d)) = stream["sample_aspect_ratio"]
        .as_str()
        .and_then(|s| s.split_once(':'))
        .and_then(|(n, d)| Some((n.parse::<f64>().ok()?, d.parse::<f64>().ok()?)))
    {
        if n > 0. && d > 0. {
            width = (width as f64 * n / d).round().clamp(1., 16384.) as u32;
        }
    }
    let rotation = stream["side_data_list"]
        .as_array()
        .and_then(|list| list.iter().find_map(|r| r["rotation"].as_i64()))
        .unwrap_or(0);
    if rotation.unsigned_abs() % 180 == 90 {
        std::mem::swap(&mut width, &mut height);
    }
    let seconds = data["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| {
            stream["duration"]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok())
        });
    let meta = ThumbMeta {
        animated: false,
        width,
        height,
        duration_ms: seconds
            .filter(|s| s.is_finite() && *s >= 0.)
            .map(|s| (s * 1000.) as u64),
    };
    if meta.width > 0 && meta.height > 0 {
        notify(meta.clone());
    }
    if bucket == 0 {
        return Ok((image::RgbaImage::new(0, 0), meta));
    }
    record_video_phase(state, 1, probe_began);
    let position = seconds.map(|s| (s * 0.1).clamp(0., 1.)).unwrap_or(0.);
    let frame = cache.join(format!("{key}.frame.tmp.png"));
    let output = cache.join(format!("{key}.stdout.tmp"));
    let mut cmd = hidden_command(runtime.join("ffmpeg.exe"));
    cmd.args([
        "-v",
        "error",
        "-nostdin",
        "-y",
        "-protocol_whitelist",
        "file,pipe",
    ]);
    if streaming {
        cmd.args([
            "-seekable",
            "0",
            "-analyzeduration",
            "1000000",
            "-probesize",
            "1048576",
            "-fpsprobesize",
            "0",
            "-threads",
            "2",
            "-i",
        ])
        .arg(path);
    } else {
        cmd.args(["-ss", &position.to_string(), "-threads", "2", "-i"])
            .arg(path);
    }
    cmd.args([
        "-frames:v",
        "1",
        "-vf",
        &format!("scale={bucket}:{bucket}:force_original_aspect_ratio=decrease:reset_sar=1"),
        "-threads",
        "1",
    ])
    .arg(&frame);
    let process_began = video_phase_began(timing);
    let process_result = run_process(cmd, &output, &errors, state, timeout, false);
    record_video_phase(state, 2, process_began);
    if let Err(error) = process_result {
        let retry = {
            let state = state.0.lock().unwrap();
            streaming && !state.stop && state.blocked.is_empty() && state.wanted.contains(key)
        };
        if !retry {
            return Err(error);
        }
        // Conservative compatibility fallback if a fragmented file cannot stream.
        let mut fallback = hidden_command(runtime.join("ffmpeg.exe"));
        fallback
            .args([
                "-v",
                "error",
                "-nostdin",
                "-y",
                "-protocol_whitelist",
                "file,pipe",
                "-ss",
                &position.to_string(),
                "-threads",
                "2",
                "-i",
            ])
            .arg(path)
            .args([
                "-frames:v",
                "1",
                "-vf",
                &format!(
                    "scale={bucket}:{bucket}:force_original_aspect_ratio=decrease:reset_sar=1"
                ),
                "-threads",
                "1",
            ])
            .arg(&frame);
        let process_began = video_phase_began(timing);
        let result = run_process(fallback, &output, &errors, state, timeout, false);
        record_video_phase(state, 2, process_began);
        result?;
    }
    let png_began = video_phase_began(timing);
    let image = image::open(&frame).map_err(|e| e.to_string())?.into_rgba8();
    record_video_phase(state, 3, png_began);
    let cleanup_began = video_phase_began(timing);
    for generated in [frame, probe, errors, output] {
        let _ = std::fs::remove_file(generated);
    }
    record_video_phase(state, 4, cleanup_began);
    let meta = ThumbMeta {
        animated: false,
        width: if meta.width > 0 {
            meta.width
        } else {
            image.width()
        },
        height: if meta.height > 0 {
            meta.height
        } else {
            image.height()
        },
        ..meta
    };
    Ok((image, meta))
}
#[cfg(test)]
fn prune_cache(cache: &Path, budget: u64) -> Result<(), String> {
    prune_cache_cancelled(cache, budget, &|| false)
}
fn scaled_jpeg_job(job: &ThumbJob) -> bool {
    job.bucket > 0
        && job.bucket <= 2048
        && job
            .file
            .path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("jpeg"))
}
fn prune_cache_cancelled(
    cache: &Path,
    budget: u64,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), String> {
    let mut entries = Vec::new();
    let mut bytes = 0u64;
    for (i, entry) in std::fs::read_dir(cache)
        .map_err(|e| e.to_string())?
        .flatten()
        .enumerate()
    {
        if i % 128 == 0 && cancelled() {
            return Ok(());
        }
        let path = entry.path();
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if path.extension().and_then(|s| s.to_str()) != Some("png")
            || stem.len() != 64
            || !stem.bytes().all(|b| b.is_ascii_hexdigit())
        {
            continue;
        }
        if let Ok(metadata) = entry.metadata() {
            bytes += metadata.len();
            entries.push((
                metadata.modified().unwrap_or(UNIX_EPOCH),
                metadata.len(),
                path,
            ));
        }
    }
    entries.sort_by_key(|e| e.0);
    for (_, size, path) in entries {
        if cancelled() {
            return Ok(());
        }
        if bytes <= budget {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            bytes = bytes.saturating_sub(size);
            let _ = std::fs::remove_file(path.with_extension("json"));
        }
    }
    Ok(())
}
struct TemporaryFiles(Vec<PathBuf>);
impl Drop for TemporaryFiles {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> PathBuf {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-fixtures")
            .join(format!(
                "thumbs-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
    fn job(path: PathBuf) -> ThumbJob {
        ThumbJob {
            generation: 1,
            id: 1,
            file: MediaFile {
                parent: path.parent().unwrap().to_owned(),
                kind: media_kind(&path).unwrap(),
                fingerprint: fingerprint(&path).unwrap(),
                path,
            },
            bucket: 256,
            decode_bucket: 256,
            region: None,
            priority: 1,
        }
    }
    #[test]
    #[ignore = "Explicit locked FFmpeg/SAR source proof; native checker executes separately"]
    fn video_sar_header_preserves_cover_and_source() {
        let root = fixture();
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        for (number, name) in [
            "format-matrix-v1/h263-amr-3gp/sample.3gp",
            "test-h264-aac.mp4",
            "test-h264-aac.mov",
            "test-mpeg4-mp3.avi",
            "test-h263-aac.3gp",
        ]
        .into_iter()
        .enumerate()
        {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("test-media")
                .join(name);
            let source = std::fs::read(&path).unwrap();
            if number == 0 {
                assert!(media_browser::mp4_header::probe_square_pixels_only(&path).is_none());
                let header = media_browser::mp4_header::probe(&path).unwrap();
                assert_eq!((header.width, header.height), (256, 144));
            }
            let legacy_cache = root.join(format!("legacy-{number}"));
            let fast_cache = root.join(format!("fast-{number}"));
            std::fs::create_dir_all(&legacy_cache).unwrap();
            std::fs::create_dir_all(&fast_cache).unwrap();
            let before = decode_video_with_header_policy(
                &path,
                512,
                &legacy_cache,
                "legacy",
                &state,
                &|_| {},
                false,
            )
            .unwrap();
            let after = decode_video_with_header_policy(
                &path,
                512,
                &fast_cache,
                "fast",
                &state,
                &|_| {},
                true,
            )
            .unwrap();
            assert_eq!(before.0, after.0);
            assert_eq!(
                (before.1.width, before.1.height, before.1.duration_ms),
                (after.1.width, after.1.height, after.1.duration_ms)
            );
            assert_eq!(std::fs::read(path).unwrap(), source);
        }
    }

    #[test]
    fn idle_image_lanes_only_help_visible_local_video_posters_within_limit() {
        let mut state = State::default();
        let path = fixture().join("poster.mp4");
        std::fs::write(&path, b"scheduling fixture").unwrap();
        let mut poster = job(path);
        // No file read is needed to exercise scheduling eligibility.
        poster.file.path = PathBuf::from(r"\\?\E:\media\poster.mp4");
        state.videos.push_back(poster.clone());
        assert!(can_lend_video_worker(&state));
        state.foreground_active = 4;
        assert!(!can_lend_video_worker(&state));
        state.foreground_active = 0;
        state.videos[0].priority = 2;
        assert!(!can_lend_video_worker(&state));
        state.videos[0].priority = 1;
        state.videos[0].bucket = 0;
        assert!(!can_lend_video_worker(&state));
        state.videos[0] = poster.clone();
        state.videos[0].file.path = PathBuf::from(r"\\?\UNC\router\media\poster.mp4");
        if cfg!(windows) {
            assert!(!can_lend_video_worker(&state));
        }
        state.videos[0] = poster.clone();
        state.active.insert(poster.id, 1);
        assert!(!can_lend_video_worker(&state));
        state.active.clear();
        state.images.push_back(poster);
        assert!(!can_lend_video_worker(&state));
    }
    #[test]
    fn progressive_source_reuse_preserves_aspect_and_rejects_replaced_source() {
        let root = fixture();
        let path = root.join("cached.png");
        image::RgbaImage::from_pixel(600, 400, image::Rgba([210, 40, 60, 255]))
            .save(&path)
            .unwrap();
        let original = std::fs::read(&path).unwrap();
        let state = Arc::new((
            Mutex::new(State {
                generation: 1,
                ..Default::default()
            }),
            Condvar::new(),
        ));
        let mut low = job(path.clone());
        low.bucket = 64;
        let (small, meta) = decode_preview_cached(&low, &state).unwrap();
        assert_eq!((small.width(), small.height()), (64, 43));
        assert_eq!((meta.width, meta.height), (600, 400));
        let pixels = state.0.lock().unwrap().decoded[0].1.clone();
        low.bucket = 512;
        let (clear, _) = decode_preview_cached(&low, &state).unwrap();
        assert_eq!((clear.width(), clear.height()), (512, 341));
        assert!(Arc::ptr_eq(&pixels, &state.0.lock().unwrap().decoded[0].1));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        image::RgbaImage::from_pixel(800, 400, image::Rgba([10, 220, 30, 255]))
            .save(&path)
            .unwrap();
        let fresh = job(path);
        let (replaced, meta) = decode_preview_cached(&fresh, &state).unwrap();
        assert_eq!((meta.width, meta.height), (800, 400));
        assert!(replaced.get_pixel(0, 0)[1] > 200);
    }
    #[test]
    fn dimensions_only_job_does_not_decode_or_cache_pixels() {
        let root = fixture();
        let path = root.join("geometry.png");
        image::RgbaImage::new(300, 700).save(&path).unwrap();
        let mut job = job(path);
        job.bucket = 0;
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let result = decode_job(&job, &root.join("cache"), &state).unwrap();
        assert_eq!(result.image.as_raw().len(), 0);
        assert_eq!((result.meta.width, result.meta.height), (300, 700));
        assert!(!root
            .join("cache")
            .join(format!("{}.png", job.key()))
            .exists());
    }
    #[test]
    fn geometry_is_reported_before_pixel_decode_and_matches_oriented_source() {
        let root = fixture();
        let path = root.join("portrait.png");
        image::RgbaImage::from_pixel(100, 250, image::Rgba([30, 80, 120, 255]))
            .save(&path)
            .unwrap();
        let job = job(path);
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let observed = std::cell::RefCell::new(Vec::new());
        let result = decode_job_with_metadata(&job, &root.join("cache"), &state, &|meta| {
            observed.borrow_mut().push((meta.width, meta.height))
        })
        .unwrap();
        assert_eq!(&*observed.borrow(), &[(100, 250)]);
        assert_eq!((result.meta.width, result.meta.height), (100, 250));
        assert_eq!(
            (
                probe_still_meta(&job.file.path).unwrap().width,
                probe_still_meta(&job.file.path).unwrap().height
            ),
            (100, 250)
        );
    }
    #[test]
    fn cached_rgba_roundtrip_corrupt_cache_recovery_and_source_unchanged() {
        let root = fixture();
        let path = root.join("source.png");
        let image = image::RgbaImage::from_fn(40, 80, |x, y| {
            image::Rgba([x as u8, y as u8, 120, if x % 2 == 0 { 0 } else { 255 }])
        });
        image.save(&path).unwrap();
        let source = std::fs::read(&path).unwrap();
        let job = job(path.clone());
        let cache = root.join("cache");
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let first = decode_job(&job, &cache, &state).unwrap();
        assert!(!first.cache_hit);
        assert_eq!(first.image.dimensions(), (40, 80));
        assert_eq!(first.meta.width, 40);
        assert_eq!(first.image.get_pixel(0, 0)[3], 0);
        let second = decode_job(&job, &cache, &state).unwrap();
        assert!(second.cache_hit);
        assert_eq!(first.image, second.image);
        std::fs::write(
            cache.join(format!("{}.png", job.key())),
            b"corrupt generated cache",
        )
        .unwrap();
        let recovered = decode_job(&job, &cache, &state).unwrap();
        assert!(!recovered.cache_hit);
        assert_eq!(recovered.image, first.image);
        assert_eq!(std::fs::read(path).unwrap(), source);
    }
    #[test]
    fn progressive_preview_is_small_soft_cached_and_does_not_change_source() {
        let root = fixture();
        let path = root.join("progressive.jpg");
        image::RgbImage::from_fn(1800, 1200, |x, y| {
            image::Rgb([((x / 9 % 2) * 255) as u8, ((y / 9 % 2) * 255) as u8, 90])
        })
        .save(&path)
        .unwrap();
        let original = std::fs::read(&path).unwrap();
        let mut low = job(path.clone());
        low.bucket = 64;
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let cache = root.join("progressive-cache");
        let pixels = decode_job(&low, &cache, &state).unwrap();
        assert_eq!(pixels.image.width().max(pixels.image.height()), 64);
        assert_eq!((pixels.meta.width, pixels.meta.height), (1800, 1200));
        let (raw, _) = decode_preview(&path, 64).unwrap();
        let raw = DynamicImage::ImageRgba8(raw).thumbnail(64, 64).into_rgba8();
        assert_ne!(pixels.image, raw);
        let again = decode_job(&low, &cache, &state).unwrap();
        assert!(again.cache_hit);
        assert_eq!(again.image, pixels.image);
        let mut high = low;
        high.bucket = 512;
        let clear = decode_job(&high, &cache, &state).unwrap();
        assert_eq!(clear.image.width().max(clear.image.height()), 512);
        assert_ne!(high.key(), job(path.clone()).key());
        assert_eq!(std::fs::read(path).unwrap(), original);
    }
    #[test]
    fn jpeg_scaled_decode_preserves_dimensions_and_exif_rotation() {
        let root = fixture();
        let path = root.join("rotated.jpg");
        let source = image::RgbImage::from_fn(1200, 800, |x, y| {
            if x < 600 {
                image::Rgb([220, 20, (y % 40) as u8])
            } else {
                image::Rgb([20, 220, (y % 40) as u8])
            }
        });
        let mut encoded = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 90)
            .encode_image(&source)
            .unwrap();
        let mut exif =
            b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0".to_vec();
        let mut bytes = encoded[..2].to_vec();
        bytes.extend([0xff, 0xe1]);
        bytes.extend(((exif.len() + 2) as u16).to_be_bytes());
        bytes.append(&mut exif);
        bytes.extend(&encoded[2..]);
        std::fs::write(&path, &bytes).unwrap();
        let (scaled, meta) = decode_preview(&path, 256).unwrap();
        assert_eq!((meta.width, meta.height), (800, 1200));
        assert!(scaled.width() < 800 && scaled.height() < 1200);
        assert_eq!(scaled.width() * 3, scaled.height() * 2);
        let pixel = scaled.get_pixel(scaled.width() / 2, scaled.height() / 4);
        assert!(pixel[0] > pixel[1] + 100);
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    #[test]
    #[ignore = "release-only generated 48MP JPEG performance measurement"]
    fn large_jpeg_preview_benchmark() {
        let root = fixture();
        let path = root.join("generated-48mp.jpg");
        let source = image::RgbImage::from_fn(8000, 6000, |x, y| {
            image::Rgb([
                ((x / 13 + y / 19) % 256) as u8,
                ((x / 31 + y / 7) % 256) as u8,
                ((x ^ y) % 256) as u8,
            ])
        });
        image::codecs::jpeg::JpegEncoder::new_with_quality(
            std::fs::File::create(&path).unwrap(),
            92,
        )
        .encode_image(&source)
        .unwrap();
        drop(source);
        let hash = format!("{:x}", Sha256::digest(std::fs::read(&path).unwrap()));
        let mut rows = Vec::new();
        for round in 0..3 {
            let started = Instant::now();
            let full = decode_still(&path).unwrap();
            let preview = image::imageops::thumbnail(&full, 512, 384);
            let decoded = started.elapsed().as_secs_f64() * 1000.;
            let started = Instant::now();
            preview.save(root.join("baseline.png")).unwrap();
            let baseline_png = started.elapsed().as_secs_f64() * 1000.;
            drop(full);
            let started = Instant::now();
            let (scaled, meta) = decode_preview(&path, 512).unwrap();
            let intermediate = scaled.dimensions();
            let preview = image::imageops::thumbnail(&scaled, 512, 384);
            let optimized = started.elapsed().as_secs_f64() * 1000.;
            let started = Instant::now();
            image::codecs::png::PngEncoder::new_with_quality(
                std::fs::File::create(root.join("optimized.png")).unwrap(),
                image::codecs::png::CompressionType::Fast,
                image::codecs::png::FilterType::Sub,
            )
            .write_image(
                &preview,
                preview.width(),
                preview.height(),
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
            let optimized_png = started.elapsed().as_secs_f64() * 1000.;
            assert_eq!((meta.width, meta.height), (8000, 6000));
            assert_eq!(preview.dimensions(), (512, 384));
            rows.push(serde_json::json!({"round":round,"baseline_decode_resize_ms":decoded,"baseline_png_ms":baseline_png,"optimized_decode_resize_ms":optimized,"optimized_png_ms":optimized_png,"intermediate_dimensions":intermediate}));
        }
        assert_eq!(
            format!("{:x}", Sha256::digest(std::fs::read(&path).unwrap())),
            hash
        );
        std::fs::create_dir_all("artifacts").unwrap();
        std::fs::write("artifacts/ui-large-jpeg-benchmark.json",serde_json::to_vec_pretty(&serde_json::json!({"scope":"generated 48MP JPEG, three sequential warm-file runs; excludes GPU upload and presentation","source_sha256":hash,"source":path,"rows":rows})).unwrap()).unwrap();
    }
    #[test]
    fn stale_source_is_rejected_before_decode() {
        let root = fixture();
        let path = root.join("source.jpg");
        std::fs::write(&path, b"x").unwrap();
        let job = job(path.clone());
        std::fs::write(&path, b"changed").unwrap();
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        assert!(decode_job(&job, &root.join("cache"), &state)
            .unwrap_err()
            .contains("文件已变化"));
    }
    #[test]
    fn blocked_source_waits_for_running_decode_and_refuses_new_jobs() {
        let root = fixture();
        let path = root.join("large.png");
        image::RgbImage::from_fn(3600, 2400, |x, y| {
            image::Rgb([(x % 251) as u8, (y % 251) as u8, 100])
        })
        .save(&path)
        .unwrap();
        let job = job(path.clone());
        let ids = HashSet::from([job.id]);
        let service = ThumbnailService::new(root.join("cache"), || {});
        service.set_requests(1, vec![job.clone()]);
        let began = Instant::now();
        while !service
            .shared
            .0
            .lock()
            .unwrap()
            .active
            .contains_key(&job.id)
        {
            assert!(began.elapsed() < Duration::from_secs(2));
            thread::sleep(Duration::from_millis(1));
        }
        service.block_sources(&ids);
        service.set_requests(1, vec![job.clone()]);
        while !service.sources_released(&ids) {
            assert!(began.elapsed() < Duration::from_secs(10));
            thread::sleep(Duration::from_millis(2));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let exclusive = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .share_mode(0)
                .open(&path)
                .unwrap();
            drop(exclusive);
        }
        assert!(service.shared.0.lock().unwrap().images.is_empty());
        service.unblock_sources(&ids);
        service.set_requests(1, vec![job]);
        let began = Instant::now();
        loop {
            if let Some(result) = service.poll() {
                assert!(result.result.is_ok());
                break;
            }
            assert!(began.elapsed() < Duration::from_secs(10));
            thread::sleep(Duration::from_millis(2));
        }
    }
    #[test]
    fn metadata_success_uses_existing_channel_and_full_channel_keeps_fallback() {
        let root = fixture();
        let mut jobs = Vec::new();
        for id in 1..=80 {
            let path = root.join(format!("geometry-{id}.png"));
            image::RgbaImage::new(2, 3).save(&path).unwrap();
            let mut item = job(path);
            item.id = id;
            item.bucket = 0;
            jobs.push(item);
        }
        let service =
            ThumbnailService::new_with_repeated_metadata_ready(root.join("cache"), || {}, false);
        service.set_requests(1, jobs);
        let began = Instant::now();
        // Leave the64-slot metadata channel full until a fallback is visible.
        loop {
            if service
                .shared
                .0
                .lock()
                .unwrap()
                .ready
                .iter()
                .any(|r| r.job.bucket == 0)
                && service.stats().metadata_ready_direct >= 64
            {
                break;
            }
            assert!(began.elapsed() < Duration::from_secs(10));
            thread::sleep(Duration::from_millis(2));
        }
        assert!(service.stats().metadata_ready_direct >= 64);
        let mut ids = HashSet::new();
        while ids.len() < 80 {
            while let Some(meta) = service.poll_metadata() {
                assert_eq!((meta.meta.width, meta.meta.height), (2, 3));
                ids.insert(meta.job.id);
            }
            while let Some(event) = service.poll_metadata_result() {
                assert!(event.result.is_ok());
                ids.insert(event.job.id);
            }
            assert!(began.elapsed() < Duration::from_secs(10));
            thread::sleep(Duration::from_millis(2));
        }
        assert!(service.poll().is_none());
        assert_eq!(std::fs::read_dir(root.join("cache")).unwrap().count(), 0);
    }

    #[test]
    fn viewport_queue_deduplicates_jobs() {
        let root = fixture();
        let path = root.join("source.png");
        image::RgbaImage::new(20, 20).save(&path).unwrap();
        let job = job(path);
        let service = ThumbnailService::new(root.join("cache"), || {});
        service.set_requests(1, vec![job.clone(), job.clone()]);
        let start = Instant::now();
        loop {
            if let Some(event) = service.poll() {
                assert!(event.result.is_ok());
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "worker did not complete"
            );
            thread::sleep(Duration::from_millis(5));
        }
        for _ in 0..8 {
            service.set_requests(1, vec![job.clone(), job.clone()]);
        }
        thread::sleep(Duration::from_millis(100));
        assert!(
            service.poll().is_none(),
            "duplicate or already delivered job ran again"
        );
    }
    #[test]
    fn scaled_jpeg_source_is_reused_until_a_larger_decode_is_needed() {
        let root = fixture();
        let path = root.join("photograph.jpg");
        image::RgbImage::from_fn(2048, 1536, |x, y| {
            image::Rgb([(x % 251) as u8, (y % 251) as u8, 100])
        })
        .save(&path)
        .unwrap();
        let mut j = job(path);
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        state.0.lock().unwrap().generation = j.generation;
        j.bucket = 64;
        let (soft, meta) = decode_preview_cached(&j, &state).unwrap();
        assert_eq!((meta.width, meta.height), (2048, 1536));
        assert_eq!((soft.width(), soft.height()), (64, 48));
        j.bucket = 128;
        let (clear, _) = decode_preview_cached(&j, &state).unwrap();
        assert_eq!((clear.width(), clear.height()), (128, 96));
        assert_eq!(state.0.lock().unwrap().stats.source_decodes, 1);
        assert_eq!(state.0.lock().unwrap().stats.source_reuses, 1);
        j.bucket = 1024;
        let (larger, _) = decode_preview_cached(&j, &state).unwrap();
        assert_eq!((larger.width(), larger.height()), (1024, 768));
        assert_eq!(state.0.lock().unwrap().stats.source_decodes, 2);
        assert_eq!(state.0.lock().unwrap().decoded.len(), 0);
    }

    #[test]
    fn extra_image_drain_respects_viewport_bytes_generation_and_kind() {
        let root = fixture();
        let path = root.join("extra-selection.png");
        image::RgbaImage::new(2, 2).save(&path).unwrap();
        let base = job(path);
        let event = |id, priority, bucket, kind, generation| {
            let mut job = base.clone();
            job.id = id;
            job.priority = priority;
            job.bucket = bucket;
            job.file.kind = kind;
            job.generation = generation;
            ThumbResult {
                key: format!("fixture-{id}"),
                job,
                result: Ok(ThumbPixels {
                    image: image::RgbaImage::new(2, 2),
                    meta: ThumbMeta {
                        animated: false,
                        width: 2,
                        height: 2,
                        duration_ms: None,
                    },
                    cache_hit: false,
                }),
            }
        };
        let mut state = State::default();
        state.generation = 1;
        state.ready = VecDeque::from([
            event(1, 1, 256, MediaKind::Video, 1),
            event(2, 3, 256, MediaKind::Image, 1),
            event(3, 1, 0, MediaKind::Image, 1),
            event(4, 1, 256, MediaKind::Image, 0),
            event(5, 1, 256, MediaKind::Image, 1),
            event(6, 1, 256, MediaKind::Image, 1),
        ]);
        let service = ThumbnailService {
            shared: Arc::new((Mutex::new(state), Condvar::new())),
            metadata: mpsc::sync_channel(1).1,
            workers: Vec::new(),
        };
        assert!(service.poll_visible_image(15, |_| true).is_none());
        assert_eq!(
            service.poll_visible_image(16, |id| id == 6).unwrap().job.id,
            6
        );
        service.shared.0.lock().unwrap().blocked.insert(5);
        assert!(service.poll_visible_image(16, |_| true).is_none());
        assert_eq!(service.shared.0.lock().unwrap().ready.len(), 5);
        assert!(service
            .poll_visible_list_pixels(15, true, |_| true)
            .is_none());
        assert!(service
            .poll_visible_list_pixels(16, true, |id| id == 2)
            .is_none());
        assert_eq!(
            service
                .poll_visible_list_pixels(16, true, |id| id == 1)
                .unwrap()
                .job
                .id,
            1
        );
        assert!(service
            .poll_visible_list_pixels(16, true, |_| true)
            .is_none());
    }

    #[test]
    fn metadata_completion_drain_never_consumes_a_pixel_upload() {
        let root = fixture();
        let path = root.join("metadata-result.png");
        image::RgbaImage::new(1, 1).save(&path).unwrap();
        let base = job(path);
        let event = |id, bucket| {
            let mut job = base.clone();
            job.id = id;
            job.bucket = bucket;
            ThumbResult {
                key: job.key(),
                job,
                result: Err("unused result fixture".into()),
            }
        };
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        shared.0.lock().unwrap().ready =
            VecDeque::from([event(1, 256), event(2, 0), event(3, 256)]);
        let service = ThumbnailService {
            shared,
            metadata: mpsc::sync_channel(1).1,
            workers: Vec::new(),
        };
        assert_eq!(service.poll_metadata_result().unwrap().job.id, 2);
        assert!(service.poll_metadata_result().is_none());
        assert_eq!(service.poll().unwrap().job.id, 1);
        assert_eq!(service.poll().unwrap().job.id, 3);
    }

    #[test]
    fn completed_results_keep_eight_slots_prioritize_viewport_and_cancel_backpressure() {
        let root = fixture();
        let path = root.join("queue.png");
        image::RgbaImage::new(1, 1).save(&path).unwrap();
        let base = job(path);
        let result = |id, priority| {
            let mut job = base.clone();
            job.id = id;
            job.priority = priority;
            ThumbResult {
                key: job.key(),
                job,
                result: Ok(ThumbPixels {
                    image: image::RgbaImage::new(1, 1),
                    meta: ThumbMeta {
                        animated: false,
                        width: 1,
                        height: 1,
                        duration_ms: None,
                    },
                    cache_hit: true,
                }),
            }
        };
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        for id in 1..=8 {
            assert!(send_ready(&shared, result(id, 2), &|| {}));
        }
        let (done_tx, done_rx) = mpsc::channel();
        let producer_shared = shared.clone();
        let urgent = result(9, 1);
        let producer = thread::spawn(move || {
            done_tx
                .send(send_ready(&producer_shared, urgent, &|| {}))
                .unwrap();
        });
        assert!(matches!(
            done_rx.recv_timeout(Duration::from_millis(30)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        assert_eq!(shared.0.lock().unwrap().ready.len(), 8);
        assert_eq!(take_ready(&shared).unwrap().job.id, 1);
        assert!(done_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        producer.join().unwrap();
        assert_eq!(shared.0.lock().unwrap().ready.len(), 8);
        assert_eq!(take_ready(&shared).unwrap().job.id, 9);
        for id in 2..=8 {
            assert_eq!(take_ready(&shared).unwrap().job.id, id);
        }
        assert!(take_ready(&shared).is_none());
        for id in 10..18 {
            assert!(send_ready(&shared, result(id, 2), &|| {}));
        }
        let (done_tx, done_rx) = mpsc::channel();
        let producer_shared = shared.clone();
        let pending = result(18, 0);
        let producer = thread::spawn(move || {
            done_tx
                .send(send_ready(&producer_shared, pending, &|| {}))
                .unwrap();
        });
        assert!(matches!(
            done_rx.recv_timeout(Duration::from_millis(30)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        shared.0.lock().unwrap().stop = true;
        shared.1.notify_all();
        assert!(!done_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        producer.join().unwrap();
        assert_eq!(shared.0.lock().unwrap().ready.len(), 8);
    }
    #[test]
    fn warm_clear_preview_uses_real_bucket_and_corrupt_or_stale_cache_cannot_bypass_checks() {
        let root = fixture();
        let path = root.join("progressive.png");
        image::RgbaImage::from_fn(768, 512, |x, y| {
            image::Rgba([(x % 255) as u8, (y % 255) as u8, 120, 255])
        })
        .save(&path)
        .unwrap();
        let source = std::fs::read(&path).unwrap();
        let clear = job(path.clone());
        let cache = root.join("cache");
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let expected = decode_job(&clear, &cache, &state).unwrap().image;
        let mut preview = clear.clone();
        preview.bucket = 64;
        let receive = |service: &ThumbnailService| {
            let began = Instant::now();
            loop {
                if let Some(event) = service.poll() {
                    return event;
                }
                assert!(began.elapsed() < Duration::from_secs(5));
                thread::sleep(Duration::from_millis(1));
            }
        };
        {
            let service = ThumbnailService::new(cache.clone(), || {});
            service.set_requests(1, vec![preview.clone()]);
            let event = receive(&service);
            assert_eq!(event.job.bucket, clear.bucket);
            assert_eq!(event.key, clear.key());
            let pixels = event.result.unwrap();
            assert!(pixels.cache_hit);
            assert_eq!(pixels.image, expected);
            assert_eq!(service.stats().source_decodes, 0);
            assert_eq!(service.stats().decodes, 0);
        }
        let clear_path = cache.join(format!("{}.png", clear.key()));
        std::fs::write(&clear_path, b"broken generated clear cache").unwrap();
        {
            let service = ThumbnailService::new(cache.clone(), || {});
            service.set_requests(1, vec![preview.clone()]);
            let event = receive(&service);
            assert_eq!(event.job.bucket, 64);
            assert_eq!(event.key, preview.key());
            let pixels = event.result.unwrap();
            assert!(!pixels.cache_hit);
            assert!(pixels.image.width().max(pixels.image.height()) <= 64);
        }
        assert_eq!(std::fs::read(&path).unwrap(), source);
        expected.save(&clear_path).unwrap();
        let mut replaced = source;
        replaced.extend_from_slice(b"new generated source version");
        std::fs::write(&path, replaced).unwrap();
        let service = ThumbnailService::new(cache, || {});
        service.set_requests(1, vec![preview]);
        assert!(receive(&service).result.is_err());
    }
    #[test]
    fn warm_cache_hits_do_not_repeat_full_directory_maintenance() {
        let root = fixture();
        let path = root.join("source.png");
        image::RgbaImage::from_pixel(160, 120, image::Rgba([30, 90, 180, 255]))
            .save(&path)
            .unwrap();
        let mut jobs = Vec::new();
        for n in 0..40 {
            let copy = root.join(format!("source-{n}.png"));
            std::fs::copy(&path, &copy).unwrap();
            let mut j = job(copy);
            j.generation = 1;
            j.id = n + 1;
            jobs.push(j);
        }
        let service = ThumbnailService::new(root.join("cache"), || {});
        let drain = |service: &ThumbnailService| {
            let began = Instant::now();
            let mut hits = 0;
            let mut count = 0;
            while count < 40 {
                if let Some(event) = service.poll() {
                    let result = event.result.unwrap();
                    assert_eq!(result.meta.width, 160);
                    assert_eq!(result.image.get_pixel(0, 0).0, [30, 90, 180, 255]);
                    hits += u64::from(result.cache_hit);
                    count += 1;
                }
                assert!(began.elapsed() < Duration::from_secs(15));
                thread::sleep(Duration::from_millis(1));
            }
            hits
        };
        service.set_requests(1, jobs.clone());
        assert_eq!(drain(&service), 0);
        let began = Instant::now();
        while service.stats().maintenance_runs == 0 {
            assert!(began.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(10));
        }
        let scans = service.stats().maintenance_runs;
        for j in &mut jobs {
            j.generation = 2;
        }
        service.set_requests(2, jobs);
        assert_eq!(drain(&service), 40);
        thread::sleep(Duration::from_millis(2200));
        assert_eq!(service.stats().maintenance_runs, scans);
        assert_eq!(service.stats().decodes, 40);
        assert_eq!(service.stats().cache_hits, 40);
        drop(service);
    }
    #[test]
    fn pruning_is_cancellable_bounded_and_preserves_unowned_files() {
        let root = fixture();
        let keep = root.join("private.png");
        std::fs::write(&keep, b"keep unrelated content").unwrap();
        for n in 0..8 {
            let path = root.join(format!("{n:064x}.png"));
            std::fs::write(&path, vec![n as u8; 1024]).unwrap();
            std::fs::write(path.with_extension("json"), b"metadata").unwrap();
        }
        prune_cache_cancelled(&root, 2048, &|| true).unwrap();
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 17);
        prune_cache(&root, 2048).unwrap();
        assert_eq!(std::fs::read(&keep).unwrap(), b"keep unrelated content");
        let bytes: u64 = std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|e| e.path() != keep && e.path().extension().is_some_and(|e| e == "png"))
            .map(|e| e.metadata().unwrap().len())
            .sum();
        assert!(bytes <= 2048);
    }
    #[test]
    #[ignore = "requires prepared offline media tools and generated samples"]
    fn real_video_cover_and_display_aspect_ratio() {
        let root = fixture();
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        for name in [
            "test-h264-aac.mp4",
            "test-h264-aac.mov",
            "test-mpeg4-mp3.avi",
            "test-h263-aac.3gp",
        ] {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("test-media")
                .join(name);
            assert!(path.exists(), "run tools/make_test_media.ps1");
            let job = job(path);
            let result = decode_job(&job, &root.join("cache"), &state).unwrap();
            assert!(result.image.width() > 0);
            assert!((result.meta.width as f32 / result.meta.height as f32 - 16. / 9.).abs() < 0.03);
            assert!(
                (result.image.width() as f32 / result.image.height() as f32 - 16. / 9.).abs()
                    < 0.03
            );
            assert!(result.meta.duration_ms.unwrap() > 60000);
        }
    }
}
