//! Reusable visible-card textures; no source I/O or frame decoding on the UI thread.
use eframe::egui;
use media_browser::domain::MediaId;
use media_browser::gif_previews::{Delivery, Event, Request, Service};
use std::collections::HashMap;
use std::time::{Duration, Instant};
pub struct Entry {
    pub texture: Option<egui::TextureHandle>,
    pub sequence: u64,
    pub cycle: u32,
    pub delay: Duration,
    pub ended: bool,
    pub error: Option<String>,
    token: u64,
}
pub struct ListAnimationViewer {
    ctx: egui::Context,
    service: Service,
    pub entries: HashMap<MediaId, Entry>,
    tokens: HashMap<MediaId, u64>,
    pending: Option<Delivery>,
    pub uploaded: u64,
}
impl ListAnimationViewer {
    pub fn new(ctx: egui::Context) -> Self {
        let wake_ctx = ctx.clone();
        Self {
            ctx,
            service: Service::new(move || wake_ctx.request_repaint()),
            entries: HashMap::new(),
            tokens: HashMap::new(),
            pending: None,
            uploaded: 0,
        }
    }
    pub fn submit(&mut self, requests: Vec<Request>) {
        self.tokens = self.service.set_visible(requests).into_iter().collect();
        self.entries
            .retain(|id, e| self.tokens.get(id) == Some(&e.token));
        if self.tokens.is_empty() {
            self.pending = None;
            while self.service.poll().is_some() {}
        }
        if self.tokens.keys().any(|id| {
            self.entries
                .get(id)
                .is_none_or(|e| !e.ended && e.error.is_none())
        }) {
            self.ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
    pub fn clear(&mut self) {
        self.submit(vec![]);
    }
    pub fn tick(&mut self, ctx: &egui::Context) {
        let mut bytes = 0;
        let began = Instant::now();
        for _ in 0..32 {
            if bytes > 0 && began.elapsed() >= Duration::from_millis(2) {
                break;
            }
            let Some(value) = self.pending.take().or_else(|| self.service.poll()) else {
                break;
            };
            if self.tokens.get(&value.id) != Some(&value.token) {
                continue;
            }
            if let Event::Frame { image, .. } = &value.event {
                if bytes > 0 && bytes + image.as_raw().len() > 16 * 1024 * 1024 {
                    self.pending = Some(value);
                    break;
                }
                bytes += image.as_raw().len();
            }
            let entry = self.entries.entry(value.id).or_insert(Entry {
                texture: None,
                sequence: 0,
                cycle: 0,
                delay: Duration::ZERO,
                ended: false,
                error: None,
                token: value.token,
            });
            match value.event {
                Event::Frame {
                    image,
                    sequence,
                    cycle,
                    delay,
                } => {
                    let pixels = egui::ColorImage::from_rgba_unmultiplied(
                        [image.width() as usize, image.height() as usize],
                        image.as_raw(),
                    );
                    if let Some(texture) = &mut entry.texture {
                        texture.set(pixels, egui::TextureOptions::LINEAR);
                    } else {
                        entry.texture = Some(ctx.load_texture(
                            "Visible GIF preview",
                            pixels,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                    entry.sequence = sequence;
                    entry.cycle = cycle;
                    entry.delay = delay;
                    self.uploaded += 1;
                }
                Event::End => entry.ended = true,
                Event::Error(error) => entry.error = Some(error),
            }
        }
        if self.tokens.keys().any(|id| {
            self.entries
                .get(id)
                .is_none_or(|e| !e.ended && e.error.is_none())
        }) {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
    pub fn texture(&self, id: MediaId) -> Option<egui::TextureHandle> {
        self.entries.get(&id)?.texture.clone()
    }
    pub fn texture_bytes(&self) -> usize {
        self.entries
            .values()
            .filter_map(|e| e.texture.as_ref())
            .map(|t| t.size()[0] * t.size()[1] * 4)
            .sum()
    }
    pub fn released(&self) -> bool {
        self.tokens.is_empty() && self.pending.is_none() && self.service.released()
    }
    pub fn stats(&self) -> media_browser::gif_previews::Stats {
        self.service.stats()
    }
}
