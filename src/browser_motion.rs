//! A short shared-media transition. Video stays in the existing GPU player.
use super::*;
pub(super) struct MediaTransition {
    id: MediaId,
    from: Rect,
    started: Option<Instant>,
    closing: bool,
    video: bool,
    texture: Option<egui::TextureHandle>,
}
impl MediaTransition {
    pub(super) fn poster(&self) -> Option<egui::TextureHandle> {
        self.texture.clone()
    }
}
fn interpolate(from: Rect, to: Rect, progress: f32) -> Rect {
    let t = progress.clamp(0., 1.);
    let eased = 1. - (1. - t).powi(3);
    Rect::from_min_max(from.min.lerp(to.min, eased), from.max.lerp(to.max, eased))
}
impl Browser {
    pub(super) fn begin_media_motion(&mut self, id: MediaId, closing: bool) {
        if self.workspace.appearance.reduce_motion {
            self.media_transition = None;
            return;
        }
        let from = if closing {
            self.image_region
                .or(self.video_region.map(|(_, rect)| rect))
        } else {
            self.list_regions.get(&id).copied()
        };
        let Some(from) = from else {
            self.media_transition = None;
            return;
        };
        let Some(record) = self
            .prepared
            .as_ref()
            .and_then(|p| p.lookup.get(&id).map(|i| p.snapshot.files[*i].clone()))
        else {
            return;
        };
        let texture = [4096, 2048, 1024, 512, 256, 64]
            .into_iter()
            .find_map(|bucket| self.texture(&self.job(&record, bucket, 0).key()));
        self.media_transition = Some(MediaTransition {
            id,
            from,
            started: None,
            closing,
            video: record.file.kind == MediaKind::Video,
            texture,
        });
        self.ctx.request_repaint();
    }
    pub(super) fn opening_motion_rect(&mut self, ctx: &egui::Context, destination: Rect) -> Rect {
        let Some(motion) = self.media_transition.as_mut() else {
            return destination;
        };
        if motion.closing {
            return destination;
        }
        let progress = motion
            .started
            .get_or_insert_with(Instant::now)
            .elapsed()
            .as_secs_f32()
            / 0.20;
        if progress >= 1. {
            self.media_transition = None;
            return destination;
        }
        ctx.request_repaint();
        // Match source aspect so the transition enlarges the media without stretching it.
        let from = contain(motion.from, destination.size());
        let painted = interpolate(from, destination, progress);
        self.motion_rect = Some(painted);
        painted
    }
    pub(super) fn paint_closing_motion(&mut self, ctx: &egui::Context) {
        let Some(motion) = self.media_transition.as_mut() else {
            return;
        };
        if !motion.closing {
            return;
        }
        let progress = motion
            .started
            .get_or_insert_with(Instant::now)
            .elapsed()
            .as_secs_f32()
            / 0.18;
        if progress >= 1. {
            self.media_transition = None;
            return;
        }
        let destination = self
            .list_regions
            .get(&motion.id)
            .copied()
            .unwrap_or_else(|| {
                Rect::from_center_size(motion.from.center(), motion.from.size() * 0.8)
            });
        let rect = interpolate(
            motion.from,
            contain(destination, motion.from.size()),
            progress,
        );
        self.motion_rect = Some(rect);
        let texture = motion.texture.clone();
        let video = motion.video && self.player_media == Some(motion.id);
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::unique("media-close-motion"))
            .order(egui::Order::Tooltip)
            .fixed_pos(screen.min)
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_min_size(screen.size());
                ui.painter().rect_filled(
                    screen,
                    0.,
                    Color32::from_black_alpha((150. * (1. - progress)) as u8),
                );
                if video {
                    self.paint_video(ui, rect);
                } else if let Some(texture) = texture {
                    ui.painter().image(
                        texture.id(),
                        rect,
                        Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
                        Color32::WHITE,
                    );
                }
            });
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn motion_has_exact_endpoints_and_keeps_intermediate_rect_finite() {
        let from = Rect::from_min_size(egui::pos2(20., 90.), egui::vec2(100., 160.));
        let to = Rect::from_min_size(Pos2::ZERO, egui::vec2(600., 960.));
        assert_eq!(interpolate(from, to, 0.), from);
        assert_eq!(interpolate(from, to, 1.), to);
        let halfway = interpolate(from, to, 0.5);
        assert!(halfway.width() > from.width() && halfway.width() < to.width());
        assert!(halfway.is_finite());
    }
}
