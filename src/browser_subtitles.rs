//! Subtitle controls, with the native file picker off the rendering thread.
use super::*;
impl Browser {
    pub(super) fn apply_subtitle_style(&mut self) {
        if let Some(player) = &self.player {
            let p = player.lock().unwrap();
            for (key, value) in self.subtitles.options() {
                if let Err(e) = p.set(key, &value) {
                    self.subtitle_notice = Some(e);
                    break;
                }
            }
        }
    }
    pub(super) fn pick_subtitle(&mut self) {
        if self.subtitle_picker.is_some() {
            return;
        }
        let Some(id) = self
            .detail
            .as_ref()
            .map(|v| v.id)
            .filter(|id| Some(*id) == self.player_media)
        else {
            return;
        };
        let directory = self.player.as_ref().and_then(|p| {
            p.lock()
                .unwrap()
                .source
                .as_ref()
                .and_then(|p| p.parent().map(Path::to_path_buf))
        });
        let (tx, rx) = std::sync::mpsc::channel();
        self.subtitle_picker = Some(rx);
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let mut dialog = rfd::FileDialog::new().add_filter(
                media_browser::ui_text!(msg_60033a3bf561),
                &["srt", "ass", "ssa", "vtt"],
            );
            if let Some(path) = directory {
                dialog = dialog.set_directory(path);
            }
            let path = dialog.pick_file();
            let _ = tx.send((id, path));
            ctx.request_repaint();
        });
    }
    pub(super) fn subtitle_tick(&mut self) {
        let result = self
            .subtitle_picker
            .as_ref()
            .and_then(|rx| rx.try_recv().ok());
        if let Some((id, path)) = result {
            self.subtitle_picker = None;
            if let Some(path) = path {
                if self.detail.as_ref().map(|v| v.id) != Some(id) || self.player_media != Some(id) {
                    self.subtitle_notice = Some(media_browser::ui_text!(msg_b675f8f8f2c3).into());
                } else if let Some(player) = &self.player {
                    match player.lock().unwrap().add_subtitle(&path) {
                        Ok(()) => {
                            self.subtitles.visible = true;
                            self.subtitle_notice =
                                Some(media_browser::ui_text!(msg_998dfe2d3ab3).into());
                        }
                        Err(e) => self.subtitle_notice = Some(e),
                    }
                }
            }
        }
        if let Some(player) = &self.player {
            if let Some(result) = player.lock().unwrap().take_subtitle_result() {
                self.subtitle_notice = Some(match result {
                    Ok(()) => media_browser::ui_text!(msg_1e511f900d9b).into(),
                    Err(e) => e,
                });
            }
        }
    }
    pub(super) fn subtitle_settings_fields(&mut self, ui: &mut egui::Ui) {
        let old = self.subtitles.clone();

        ui.checkbox(
            &mut self.subtitles.visible,
            media_browser::ui_text!(msg_5267612371d3),
        );
        if ui
            .add_enabled(
                self.detail.is_some()
                    && self.player_media == self.detail.as_ref().map(|v| v.id)
                    && self.subtitle_picker.is_none(),
                egui::Button::new(media_browser::ui_text!(msg_f847abeba3c5)),
            )
            .clicked()
        {
            self.pick_subtitle();
        }
        ui.label(media_browser::ui_text!(msg_e3a898dcd336));
        if let Some(message) = &self.subtitle_notice {
            ui.label(message);
        }
        ui.separator();
        ui.label(media_browser::ui_text!(msg_4d357e8b2912));
        ui.horizontal(|ui| {
            for (index, label) in [
                media_browser::ui_text!(msg_c4871fbefafb),
                media_browser::ui_text!(msg_93a322c085d8),
                media_browser::ui_text!(msg_05f761b248e1),
            ]
            .into_iter()
            .enumerate()
            {
                if ui.button(label).clicked() {
                    self.subtitles = media_browser::subtitles::SubtitleStyle::preset(index);
                }
            }
        });
        ui.add(super::modern_ui::QuietSlider(
            egui::Slider::new(&mut self.subtitles.size, 16.0..=80.0)
                .text(media_browser::ui_text!(msg_f71561b5af0a)),
        ));
        ui.add(super::modern_ui::QuietSlider(
            egui::Slider::new(&mut self.subtitles.position, 0.0..=100.0)
                .text(media_browser::ui_text!(msg_64e74f353f40)),
        ));
        ui.horizontal(|ui| {
            ui.label(media_browser::ui_text!(msg_f21ef4aba5ab));
            ui.color_edit_button_srgb(&mut self.subtitles.color);
        });
        ui.add(super::modern_ui::QuietSlider(
            egui::Slider::new(&mut self.subtitles.outline, 0.0..=5.0)
                .text(media_browser::ui_text!(msg_a8f733d4a18d)),
        ));
        ui.checkbox(
            &mut self.subtitles.background,
            media_browser::ui_text!(msg_8eae259e2d42),
        );
        ui.label(media_browser::ui_text!(msg_71add4aee8d0));
        if self.subtitles != old {
            self.apply_subtitle_style();
        }
    }
}
