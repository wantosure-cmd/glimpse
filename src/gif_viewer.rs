//! UI owns one reusable texture; frame decoding and disposal composition stay in a worker.
use eframe::egui;
use media_browser::{
    animation::{AnimationEvent, AnimationService},
    domain::{Fingerprint, MediaFile},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

pub struct GifViewer {
    service: AnimationService,
    source: Option<(PathBuf, Fingerprint)>,
    token: u64,
    texture: Option<egui::TextureHandle>,
    deadline: Option<Instant>,
    ended: bool,
    suspended: Option<Instant>,
    pub error: Option<String>,
    pub sequence: Option<u64>,
    pub cycle: u32,
    pub last_delay: Duration,
    pub uploaded: u64,
}
impl GifViewer {
    pub fn new(ctx: egui::Context) -> Self {
        let service = AnimationService::new(move || ctx.request_repaint());
        Self {
            service,
            source: None,
            token: 0,
            texture: None,
            deadline: None,
            ended: false,
            suspended: None,
            error: None,
            sequence: None,
            cycle: 0,
            last_delay: Duration::ZERO,
            uploaded: 0,
        }
    }
    pub fn open(&mut self, file: &MediaFile) {
        if self.source.as_ref().is_some_and(|(path, fingerprint)| {
            path == &file.path && fingerprint == &file.fingerprint
        }) {
            return;
        }
        self.clear();
        self.source = Some((file.path.clone(), file.fingerprint.clone()));
        self.token = self.service.open(file.clone());
    }
    pub fn clear(&mut self) {
        self.service.cancel();
        self.source = None;
        self.texture = None;
        self.deadline = None;
        self.ended = false;
        self.suspended = None;
        self.error = None;
        self.sequence = None;
        self.cycle = 0;
        self.uploaded = 0;
    }
    pub fn released(&self) -> bool {
        self.source.is_none() && self.service.released()
    }
    pub fn tick(&mut self, ctx: &egui::Context) {
        if self.source.is_none() || self.ended || self.error.is_some() {
            return;
        }
        let now = Instant::now();
        if ctx
            .input(|i| i.viewport().minimized == Some(true) || i.viewport().occluded == Some(true))
        {
            self.suspended.get_or_insert(now);
            return;
        }
        if let Some(since) = self.suspended.take() {
            self.deadline = self
                .deadline
                .map(|deadline| deadline + now.duration_since(since));
        }
        // A hidden window may receive no UI callbacks. Resume instead of trying
        // to decode hours of animation to catch up on its first visible frame.
        if self.deadline.is_some_and(|deadline| {
            now.saturating_duration_since(deadline) > Duration::from_millis(100)
        }) {
            self.deadline = Some(now);
        }
        let mut chosen = None;
        // Catch up after a slow UI frame without uploading every skipped frame.
        for _ in 0..8 {
            if self.deadline.is_some_and(|deadline| deadline > now) {
                break;
            }
            let Some(delivery) = self.service.poll() else {
                break;
            };
            if delivery.token != self.token {
                continue;
            }
            match delivery.event {
                AnimationEvent::Frame(frame) => {
                    self.deadline = Some(self.deadline.unwrap_or(now) + frame.delay);
                    self.sequence = Some(frame.sequence);
                    self.cycle = frame.cycle;
                    self.last_delay = frame.delay;
                    chosen = Some(frame.image);
                }
                AnimationEvent::End => {
                    self.ended = true;
                    break;
                }
                AnimationEvent::Error(error) => {
                    self.error = Some(error);
                    break;
                }
            }
        }
        let advanced = chosen.is_some();
        if let Some(pixels) = chosen {
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [pixels.width() as usize, pixels.height() as usize],
                pixels.as_raw(),
            );
            if let Some(texture) = &mut self.texture {
                texture.set(image, egui::TextureOptions::LINEAR);
            } else {
                self.texture =
                    Some(ctx.load_texture("GIF detail", image, egui::TextureOptions::LINEAR));
            }
            self.uploaded += 1;
        }
        if !self.ended && self.error.is_none() {
            if let Some(deadline) = self.deadline {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if !remaining.is_zero() {
                    ctx.request_repaint_after(remaining);
                } else if advanced {
                    ctx.request_repaint_after(Duration::from_millis(5));
                }
            }
        }
    }
    pub fn texture(&self) -> Option<egui::TextureHandle> {
        self.texture.clone()
    }
    pub fn texture_bytes(&self) -> usize {
        self.texture
            .as_ref()
            .map(|t| t.size()[0] * t.size()[1] * 4)
            .unwrap_or(0)
    }
    pub fn ended(&self) -> bool {
        self.ended
    }
}
