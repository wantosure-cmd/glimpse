use super::*;
pub(super) struct RotationOperation {
    requests: Option<Vec<(MediaId, MediaFile)>>,
    receiver: Option<std::sync::mpsc::Receiver<Vec<(PathBuf, Result<(), String>)>>>,
    clockwise: bool,
    started: Instant,
}
impl Browser {
    pub(super) fn begin_rotation(&mut self, clockwise: bool, single: Option<MediaId>) {
        if self.rotation.is_some() || self.trash_operation.is_some() {
            return;
        }
        let Some(p) = self.prepared.clone() else {
            return;
        };
        let ids = single.map(|id| HashSet::from([id])).unwrap_or_else(|| {
            self.detail
                .as_ref()
                .map(|v| HashSet::from([v.id]))
                .unwrap_or_else(|| self.selection.clone())
        });
        let requests: Vec<_> = p
            .view
            .sequence
            .iter()
            .filter(|id| ids.contains(id))
            .filter_map(|id| p.lookup.get(id).map(|i| &p.snapshot.files[*i]))
            .filter(|r| r.file.kind == MediaKind::Image)
            .map(|r| (r.id, r.file.clone()))
            .collect();
        if requests.is_empty() {
            self.rotation_message = Some(media_browser::ui_text!(msg_4596ce92e46f).into());
            return;
        }
        self.list_animations.clear();
        self.animation.clear();
        self.rotation_ids = requests.iter().map(|r| r.0).collect();
        self.thumbnails.block_sources(&self.rotation_ids);
        self.status = media_browser::ui_format!(msg_e6ed3f80b441, requests.len());
        self.rotation = Some(RotationOperation {
            requests: Some(requests),
            receiver: None,
            clockwise,
            started: Instant::now(),
        });
        self.ctx.request_repaint();
    }
    pub(super) fn poll_rotation(&mut self) {
        if self
            .rotation
            .as_ref()
            .is_some_and(|op| op.requests.is_some())
        {
            if !self.thumbnails.sources_released(&self.rotation_ids)
                || !self.animation.released()
                || !self.list_animations.released()
            {
                if self.rotation.as_ref().unwrap().started.elapsed() > Duration::from_secs(65) {
                    self.rotation = None;
                    self.thumbnails.unblock_sources(&self.rotation_ids);
                    self.rotation_ids.clear();
                    self.rotation_message = Some(media_browser::ui_text!(msg_40abd2b80769).into());
                }
                return;
            }
            // Drain prior-generation decode results before invalidating disk cache.
            while self.thumbnails.poll().is_some() {}
            let op = self.rotation.as_mut().unwrap();
            let requests = op.requests.take().unwrap();
            let clockwise = op.clockwise;
            let (tx, rx) = std::sync::mpsc::channel();
            op.receiver = Some(rx);
            let ctx = self.ctx.clone();
            let cache = self.data_dir.join("thumbnails");
            std::thread::spawn(move || {
                let results = requests
                    .into_iter()
                    .map(|(id, file)| {
                        let result = media_browser::photo_rotate::save(
                            &file.path,
                            &file.fingerprint,
                            clockwise,
                        );
                        if result.is_ok() {
                            // Invalidate all our variants even when a cloud provider rounds
                            // modification timestamps and the edited file size is unchanged.
                            for bucket in [256, 512, 1024, 2048, 4096, 8192] {
                                let key = ThumbJob {
                                    generation: 0,
                                    id,
                                    file: file.clone(),
                                    bucket,
                                    decode_bucket: bucket,
                                    region: None,
                                    priority: 0,
                                }
                                .key();
                                for extension in ["png", "json"] {
                                    let _ = std::fs::remove_file(
                                        cache.join(format!("{key}.{extension}")),
                                    );
                                }
                            }
                        }
                        (file.path, result)
                    })
                    .collect();
                let _ = tx.send(results);
                ctx.request_repaint();
            });
        }
        let result = self
            .rotation
            .as_ref()
            .and_then(|op| op.receiver.as_ref())
            .map(|rx| rx.try_recv());
        let results = match result {
            Some(Ok(r)) => r,
            Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => vec![(
                PathBuf::new(),
                Err(media_browser::ui_text!(msg_d58d9a685f38).into()),
            )],
            _ => return,
        };
        self.rotation = None;
        self.thumbnails.unblock_sources(&self.rotation_ids);
        self.rotation_ids.clear();
        self.textures.clear();
        self.texture_bytes = 0;
        self.failures.clear();
        let success = results.iter().filter(|r| r.1.is_ok()).count();
        let mut message =
            media_browser::ui_format!(msg_68a72e763584, success, results.len() - success);
        for (path, result) in &results {
            if let Err(error) = result {
                message.push_str(&format!("\n{}：{error}", display_path(path)));
            }
        }
        if let Some(v) = &mut self.detail {
            v.zoom = 1.;
            v.pan = Vec2::ZERO;
        }
        self.status = message.lines().next().unwrap_or_default().into();
        self.rotation_message = Some(message);
        self.refresh_in_place();
    }
    pub(super) fn rotation_dialog(&mut self, ctx: &egui::Context) {
        if self.rotation.is_some() && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if self.rotation.is_some() {
            egui::Modal::new(egui::Id::unique("rotation-progress"))
                .frame(modern_ui::dialog_frame(&ctx.style_of(egui::Theme::Dark)))
                .show(ctx, |ui| {
                    ui.heading(media_browser::ui_text!(msg_af57dc5d309d));
                    ui.label(&self.status);
                    ui.spinner();
                });
            ctx.request_repaint_after(Duration::from_millis(100));
        } else if let Some(message) = self.rotation_message.clone() {
            egui::Modal::new(egui::Id::unique("rotation-result"))
                .frame(modern_ui::dialog_frame(&ctx.style_of(egui::Theme::Dark)))
                .show(ctx, |ui| {
                    ui.heading(media_browser::ui_text!(msg_e69ce0833e43));
                    egui::ScrollArea::vertical()
                        .max_height(350.)
                        .show(ui, |ui| {
                            ui.label(message);
                        });
                    if ui
                        .button(media_browser::ui_text!(msg_fac2a67ad878))
                        .clicked()
                    {
                        self.rotation_message = None;
                    }
                });
        }
    }
    pub(super) fn shortcut_settings(&mut self, ui: &mut egui::Ui) {
        if ui
            .add(
                modern_ui::MenuAction::new(media_browser::ui_text!(msg_df3d58c7d84b))
                    .icon(icons::Glyph::Settings)
                    .key("Ctrl+,"),
            )
            .clicked()
        {
            self.workspace.settings_open = true;
            ui.close();
        }
    }
}
