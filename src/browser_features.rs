//! Local favorites, paint selection, compact telemetry and safe cleanup workflow.
use super::*;
use media_browser::{favorites::Favorites, telemetry::Telemetry, trash::TrashRequest};
pub(super) struct Cleanup {
    prepared: Arc<Prepared>,
    root: PathBuf,
    deadline: Option<Instant>,
    files: Vec<TrashRequest>,
    directories: Vec<PathBuf>,
    started: bool,
    worker: Option<std::sync::mpsc::Receiver<Result<Vec<TrashRequest>, String>>>,
}
pub(super) struct Features {
    pub favorites: Favorites,
    pub only_favorites: bool,
    pub dragging: bool,
    drag_remove: bool,
    visited: HashSet<MediaId>,
    pub(super) previous: Option<Pos2>,
    pub telemetry: Telemetry,
    pub close_rect: Option<Rect>,
    pub close_visual_rect: Option<Rect>,
    pub close_clicked: bool,
    favorites_error: Option<String>,
    pub cleanup: Option<Cleanup>,
}
impl Features {
    pub(super) fn cleanup_blocks_input(&self) -> bool {
        self.cleanup.as_ref().is_some_and(|c| !c.started && c.deadline.is_none())
    }
    pub fn new(dir: &Path) -> Self {
        let result = Favorites::load(&dir.join("favorites.json"));
        let error = result.as_ref().err().cloned();
        Self {
            favorites: result.unwrap_or_default(),
            only_favorites: false,
            dragging: false,
            drag_remove: false,
            visited: HashSet::new(),
            previous: None,
            telemetry: Default::default(),
            close_rect: None,
            close_visual_rect: None,
            close_clicked: false,
            favorites_error: error,
            cleanup: None,
        }
    }
}
pub(super) fn favorite_rect(rect: Rect) -> Rect {
    Rect::from_min_size(
        rect.right_top() + egui::vec2(-34., 4.),
        egui::vec2(30., 30.),
    )
}
impl Browser {
    fn toggle_favorite(&mut self, path: &Path) {
        if self.features.cleanup.as_ref().is_some_and(|c| c.started) {
            return;
        }
        if self.features.favorites_error.is_some() {
            self.status = self.features.favorites_error.clone().unwrap();
            return;
        }
        let mut next = self.features.favorites.clone();
        if !next.paths.remove(path) {
            next.paths.insert(path.to_owned());
        }
        match next.save(&self.data_dir.join("favorites.json")) {
            Ok(()) => {
                self.features.favorites = next;
                if self.features.only_favorites {
                    self.prepare_dirty = true;
                    self.prepare_after = Instant::now();
                }
            }
            Err(e) => self.status = e,
        }
    }
    pub(super) fn favorite_card(&mut self, ui: &mut egui::Ui, record: &MediaRecord, rect: Rect) {
        let r = favorite_rect(rect);
        self.favorite_button(ui, record, r, ui.rect_contains_pointer(rect));
    }
    pub(super) fn favorite_button(
        &mut self,
        ui: &mut egui::Ui,
        record: &MediaRecord,
        r: Rect,
        show: bool,
    ) {
        let selected = self.features.favorites.paths.contains(&record.file.path);
        let response = ui.interact(
            r,
            ui.scope_id().with(("favorite", record.id)),
            egui::Sense::click(),
        );
        if selected || response.hovered() || response.has_focus() || show {
            if !selected && (response.hovered() || response.has_focus()) {
                ui.painter().rect_filled(
                    r.shrink(1.),
                    modern_ui::CONTROL_RADIUS,
                    modern_ui::OVERLAY,
                );
            }
            icons::paint(
                ui.painter(),
                r.shrink(5.),
                if selected {
                    icons::Glyph::StarFilled
                } else {
                    icons::Glyph::Star
                },
                if selected {
                    modern_ui::WARNING
                } else {
                    modern_ui::TEXT
                },
            );
        }
        if response
            .clone()
            .on_hover_text(if selected {
                media_browser::ui_text!(msg_dca60869e7d2)
            } else {
                media_browser::ui_text!(msg_60a53514eb92)
            })
            .clicked()
        {
            self.toggle_favorite(&record.file.path);
        }
        self.control_regions.push(r);
        if response.hovered() {
            self.hovered_video = None;
        }
    }
    pub(super) fn select_favorites(&mut self, folder: Option<&Path>) {
        self.selection.clear();
        self.selected_folders.clear();
        if let Some(p) = &self.prepared {
            self.selection.extend(
                p.view
                    .sequence
                    .iter()
                    .filter(|id| {
                        p.lookup.get(id).is_some_and(|i| {
                            let path = &p.snapshot.files[*i].file.path;
                            self.features.favorites.paths.contains(path)
                                && folder.is_none_or(|root| path.starts_with(root))
                        })
                    })
                    .copied(),
            );
        }
    }
    pub(super) fn favorites_toolbar(&mut self, ui: &mut egui::Ui) {
        if icons::labeled_button(
            ui,
            icons::Glyph::Star,
            media_browser::ui_text!(msg_094e6eafb9a2)
                .trim_start_matches('★')
                .trim(),
            self.features.only_favorites,
        )
        .clicked()
        {
            self.features.only_favorites = !self.features.only_favorites;
            self.prepare_dirty = true;
            self.prepare_after = Instant::now();
            self.selection.clear();
        }
        if ui
            .small_button(media_browser::ui_text!(msg_0dd6971d63de))
            .clicked()
        {
            self.select_favorites(None);
        }
    }
    pub(super) fn favorite_menu(&mut self, ui: &mut egui::Ui, path: &Path) {
        if ui
            .add(
                modern_ui::MenuAction::new(if self.features.favorites.paths.contains(path) {
                    media_browser::ui_text!(msg_dca60869e7d2)
                } else {
                    media_browser::ui_text!(msg_60a53514eb92)
                })
                .icon(icons::Glyph::Star),
            )
            .clicked()
        {
            self.toggle_favorite(path);
            ui.close();
        }
        if ui
            .add(
                modern_ui::MenuAction::new(media_browser::ui_text!(msg_c14a67f6bc2f))
                    .icon(icons::Glyph::Copy)
                    .key("Ctrl+C"),
            )
            .clicked()
        {
            self.copy_selected_files(false);
            ui.close();
        }
        if ui
            .add(
                modern_ui::MenuAction::new(media_browser::ui_text!(msg_a690487833d8))
                    .icon(icons::Glyph::Cut)
                    .key("Ctrl+X"),
            )
            .clicked()
        {
            self.copy_selected_files(true);
            ui.close();
        }
        ui.separator();
    }
    pub(super) fn favorite_folder_menu(&mut self, ui: &mut egui::Ui, path: &Path) {
        if ui
            .button(media_browser::ui_text!(msg_3251048499aa))
            .clicked()
        {
            self.select_favorites(Some(path));
            ui.close();
        }
        if ui
            .add_enabled(
                self.features.cleanup.is_none() && !self.scan_busy,
                egui::Button::new(
                    egui::RichText::new(media_browser::ui_text!(msg_a81d1b3ddfdb))
                        .color(Color32::LIGHT_RED),
                ),
            )
            .clicked()
        {
            self.begin_cleanup(path);
            ui.close();
        }
        ui.separator();
    }
    pub(super) fn copy_selected_files(&mut self, cut: bool) {
        if let Some(p) = &self.prepared {
            let paths: Vec<_> = p
                .view
                .sequence
                .iter()
                .filter(|id| {
                    self.selection.contains(id)
                        || (self.selection.is_empty()
                            && self.detail.as_ref().is_some_and(|v| v.id == **id))
                })
                .filter_map(|id| {
                    p.lookup
                        .get(id)
                        .map(|i| p.snapshot.files[*i].file.path.clone())
                })
                .collect();
            if let Some(smoke) = &mut self.features_smoke {
                // Native event regression inspects payloads without replacing the user's clipboard.
                smoke.clipboard.push((cut, paths));
                return;
            }
            if let Err(e) = crate::file_clipboard::files(&paths, cut) {
                self.status = e;
            }
        }
    }
    pub(super) fn drag_select_card(
        &mut self,
        ui: &egui::Ui,
        id: MediaId,
        rect: Rect,
        check: &egui::Response,
        card: &egui::Response,
    ) {
        if self.workspace.is_open() || self.detail.is_some() {
            return;
        }
        if check.drag_started()
            || (card.drag_started() && !self.selection.is_empty() && self.selection.contains(&id))
        {
            self.features.dragging = true;
            self.features.drag_remove = false;
            self.features.visited.clear();
            self.features.previous = ui.input(|i| i.pointer.press_origin());
        }
        if self.features.dragging
            && ui.input(|i| {
                i.pointer.primary_down() || i.pointer.button_released(egui::PointerButton::Primary)
            })
        {
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                let from = self.features.previous.unwrap_or(pos);
                if segment_hits(rect.intersect(ui.clip_rect()), from, pos)
                    && self.features.visited.insert(id)
                {
                    if self.features.drag_remove {
                        self.selection.remove(&id);
                    } else {
                        self.selection.insert(id);
                    }
                }
            }
        }
    }
    pub(super) fn bottom_status_bar(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().interact_size.y = 20.;
        ui.horizontal(|ui| {
            let stats_width = 250_f32.min(ui.available_width());
            let left_width =
                (ui.available_width() - stats_width - ui.spacing().item_spacing.x).max(0.);
            ui.allocate_ui_with_layout(
                egui::vec2(left_width, 20.),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(left_width);
                    // Long status messages must not paint over the fixed right group.
                    ui.set_clip_rect(ui.clip_rect().intersect(ui.max_rect()));
                    let ready = self.snapshot.as_ref().is_some_and(|s| {
                        s.finished
                            && s.errors.is_empty()
                            && self.status
                                == media_browser::ui_format!(
                                    msg_a2d7df3e4edd,
                                    s.files.len(),
                                    s.elapsed_ms
                                )
                    });
                    let status = if ready {
                        media_browser::ui_text!(status_library_ready)
                    } else {
                        &self.status
                    };
                    ui.add(egui::Label::new(egui::RichText::new(status).small().weak()).truncate())
                        .on_hover_text(&self.status);
                    if let Some(message) = &self.trash_message {
                        ui.label(egui::RichText::new(message).color(modern_ui::PRIMARY));
                    }
                    if let Some(notice) = &self.index_notice {
                        ui.add(
                            egui::Label::new(egui::RichText::new(notice).color(modern_ui::WARNING))
                                .truncate(),
                        )
                        .on_hover_text(notice);
                    }
                    if let Some(warning) = &self.watch_warning {
                        ui.colored_label(modern_ui::WARNING, warning);
                    }
                    if let Some(p) = &self.prepared {
                        ui.label(media_browser::ui_format!(
                            msg_3cfb151cccd6,
                            p.view.shown.images,
                            p.view.shown.videos
                        ));
                    }
                    if !self.failures.is_empty() {
                        ui.colored_label(
                            modern_ui::DANGER,
                            media_browser::ui_format!(msg_55e264f79959, self.failures.len()),
                        );
                    }
                    self.loading_issues_menu(ui);
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(stats_width, 20.),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| self.performance_bar(ui),
            );
        });
    }
    pub(super) fn performance_bar(&mut self, ui: &mut egui::Ui) {
        ui.ctx().request_repaint_after(Duration::from_secs(1));
        self.features.telemetry.tick();
        let t = &self.features.telemetry;
        if ui.available_width() < 210. {
            return;
        }
        let cpu = t
            .cpu
            .map(|n| format!("{n:.0}%"))
            .unwrap_or_else(|| "—".into());
        let mem = t
            .memory
            .map(|n| format!("{}M", n / 1048576))
            .unwrap_or_else(|| "—".into());
        let read = t.read.unwrap_or(0.) / 1048576.;
        let write = t.write.unwrap_or(0.) / 1048576.;
        let stats = ui
            .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for (icon, text) in [
                    (icons::Glyph::Disk, format!("{read:.1}M/s")),
                    (icons::Glyph::Memory, mem.clone()),
                    (icons::Glyph::Cpu, cpu.clone()),
                ] {
                    ui.label(
                        egui::RichText::new(text)
                            .size(modern_ui::TYPE_SMALL)
                            .color(modern_ui::MUTED),
                    );
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(16., 20.), egui::Sense::hover());
                    icons::paint(ui.painter(), rect, icon, modern_ui::MUTED);
                    ui.add_space(5.);
                }
            })
            .response;
        if let Some(smoke) = &mut self.design_smoke {
            smoke.performance_rect = Some(stats.rect);
        }
        stats.on_hover_text(media_browser::ui_format!(
            msg_cb45a2c41952,
            cpu = cpu,
            mem = mem,
            read = read,
            write = write
        ));
    }
    pub(super) fn window_close_button(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let response = icons::window_button(
            ui,
            icons::Glyph::Close,
            media_browser::ui_text!(msg_3fd47edce45b),
        );
        let base = response.rect;
        self.features.close_visual_rect = Some(base);
        let screen = ui.ctx().content_rect();
        let r = Rect::from_min_max(
            egui::pos2(base.left(), screen.top()),
            egui::pos2(screen.right(), base.bottom() + 5.),
        );
        self.features.close_rect = Some(r);
        // Corner convenience is hit-only. Nonoverlapping strips avoid stealing
        // hover/click from the standard button or introducing extra Tab stops.
        let strips = [
            Rect::from_min_max(
                egui::pos2(base.left(), screen.top()),
                egui::pos2(screen.right(), base.top()),
            ),
            Rect::from_min_max(
                egui::pos2(base.right(), base.top()),
                egui::pos2(screen.right(), r.bottom()),
            ),
            Rect::from_min_max(base.left_bottom(), egui::pos2(base.right(), r.bottom())),
        ];
        let mut corner_hovered = false;
        let mut corner_clicked = false;
        for (i, strip) in strips.into_iter().enumerate() {
            if strip.is_positive() {
                let hit = ui.interact(
                    strip,
                    ui.scope_id().with(("close-corner", i)),
                    egui::Sense::CLICK,
                );
                corner_hovered |= hit.hovered();
                corner_clicked |= hit.clicked();
                hit.on_hover_text(media_browser::ui_text!(msg_3fd47edce45b));
            }
        }
        if corner_hovered {
            icons::paint_window_control(ui, base, icons::Glyph::Close, true, response.has_focus());
        }
        if response.clicked() || corner_clicked {
            self.features.close_clicked = true;
        }
        if corner_clicked {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        response
    }
    pub(super) fn language_settings(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(media_browser::ui_text!(msg_121918175edd));
            for (code, label) in [
                ("zh-CN", media_browser::ui_text!(msg_9937e365aaf3)),
                ("en", "English"),
            ] {
                if ui
                    .selectable_value(&mut self.language, code.to_owned(), label)
                    .changed()
                {
                    self.status.clear();
                    self.prepare_dirty = true;
                    self.prepare_after = Instant::now();
                    media_browser::ui_text::set_english(code == "en");
                    self.ctx.request_repaint();
                }
            }
        });
    }
    fn begin_cleanup(&mut self, root: &Path) {
        if self.features.favorites_error.is_some()
            || self.pending_trash.is_some()
            || self.trash_operation.is_some()
            || self.pending_folders.is_some()
            || self.folder_prepare.is_some()
            || self.rotation.is_some()
            || self.permanent_prompt.is_some()
            || !self.trash_failures.is_empty()
        {
            return;
        }
        let Some(p) = self.prepared.clone() else {
            return;
        };
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        let files = snapshot
            .files
            .iter()
            .filter(|m| {
                m.file.path.starts_with(root)
                    && !self.features.favorites.paths.contains(&m.file.path)
            })
            .map(|m| TrashRequest {
                empty_only: false,
                directory: false,
                id: m.id,
                root: snapshot.root.clone(),
                path: m.file.path.clone(),
                fingerprint: m.file.fingerprint.clone(),
            })
            .collect();
        let mut directories: Vec<_> = snapshot
            .directories
            .iter()
            .filter(|d| d.path != root && d.path.starts_with(root))
            .map(|d| d.path.clone())
            .collect();
        directories.sort_by_key(|p| p.components().count());
        self.features.cleanup = Some(Cleanup {
            prepared: p,
            root: root.to_owned(),
            deadline: None,
            files,
            directories,
            started: false,
            worker: None,
        });
    }
    pub(super) fn features_tick(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| {
            i.pointer.primary_down() || i.pointer.button_released(egui::PointerButton::Primary)
        }) {
            self.features.dragging = false;
            self.features.previous = None;
        }
        let Some(mut c) = self.features.cleanup.take() else {
            return;
        };
        if !c.started {
            let mut cancel = false;
            if c.deadline.is_none() {
                egui::Modal::new(egui::Id::unique("favorite-cleanup-confirm"))
                    .frame(modern_ui::dialog_frame(&ctx.style_of(egui::Theme::Dark)))
                    .show(ctx, |ui| {
                        ui.heading(media_browser::ui_text!(msg_f1569bbe1151));
                        ui.label(display_path(&c.root));
                        ui.label(media_browser::ui_format!(msg_a47e44af22a6, c.files.len()));
                        ui.horizontal(|ui| {
                            if ui
                                .button(media_browser::ui_text!(msg_2cd0f3be8738))
                                .clicked()
                            {
                                cancel = true;
                            }
                            if ui
                                .button(media_browser::ui_text!(msg_7ba2d6728fe2))
                                .clicked()
                            {
                                c.deadline = Some(Instant::now() + Duration::from_secs(10));
                            }
                        });
                    });
            } else {
                // Recheck favorites at commit time; changes during countdown are protected.
                let deadline = c.deadline.unwrap();
                if Instant::now() < deadline {
                    egui::Area::new(egui::Id::unique("favorite-cleanup-undo"))
                        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0., -25.))
                        .show(ctx, |ui| {
                            modern_ui::toast_frame(ui.style()).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(media_browser::ui_format!(
                                        msg_821d388a68c0,
                                        deadline
                                            .saturating_duration_since(Instant::now())
                                            .as_secs()
                                            + 1
                                    ));
                                    if ui
                                        .button(media_browser::ui_text!(msg_6b5819895525))
                                        .clicked()
                                    {
                                        cancel = true;
                                    }
                                });
                            });
                        });
                    ctx.request_repaint_after(Duration::from_millis(100));
                } else if self.trash_operation.is_none()
                    && self.permanent_prompt.is_none()
                    && self.rotation.is_none()
                {
                    c.files
                        .retain(|r| !self.features.favorites.paths.contains(&r.path));
                    let requests = std::mem::take(&mut c.files);
                    let ids = requests.iter().map(|r| r.id).collect();
                    c.started = true;
                    if !requests.is_empty() {
                        self.start_trash_requests(c.prepared.clone(), requests, ids);
                    }
                }
            }
            if cancel {
                return;
            }
        } else if self.trash_operation.is_none() && self.permanent_prompt.is_none() {
            // Keep the preceding batch's failure report visible and stop the cleanup.
            if !self.trash_failures.is_empty() {
                return;
            }
            if let Some(rx) = &c.worker {
                match rx.try_recv() {
                    Ok(Ok(requests)) => {
                        c.worker = None;
                        if !requests.is_empty() {
                            self.start_trash_requests(c.prepared.clone(), requests, HashSet::new());
                        }
                    }
                    Ok(Err(e)) => {
                        self.status = e;
                        return;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                    Err(_) => {}
                }
            } else {
                // One directory at a time, deepest first. Never enqueue a folder containing files.
                let Some(path) = c.directories.pop() else {
                    self.refresh_in_place();
                    return;
                };
                let root = c.prepared.snapshot.root.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                c.worker = Some(rx);
                let wake = ctx.clone();
                std::thread::spawn(move || {
                    let result = (|| {
                        if std::fs::read_dir(&path)
                            .map_err(|e| e.to_string())?
                            .next()
                            .is_some()
                        {
                            return Ok(Vec::new());
                        }
                        let request = TrashRequest {
                            empty_only: true,
                            directory: true,
                            id: -1,
                            root,
                            path: path.clone(),
                            fingerprint: Fingerprint::read(&path)?,
                        };
                        media_browser::trash::prepare_folder(&request)?;
                        Ok(vec![request])
                    })();
                    let _ = tx.send(result);
                    wake.request_repaint();
                });
            }
        }
        self.features.cleanup = Some(c);
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}
fn segment_hits(rect: Rect, a: Pos2, b: Pos2) -> bool {
    if !rect.is_positive() {
        return false;
    }
    let d = b - a;
    let (mut lo, mut hi) = (0f32, 1f32);
    for (p, q) in [
        (-d.x, a.x - rect.left()),
        (d.x, rect.right() - a.x),
        (-d.y, a.y - rect.top()),
        (d.y, rect.bottom() - a.y),
    ] {
        if p.abs() < 0.00001 {
            if q < 0. {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0. {
                lo = lo.max(t);
            } else {
                hi = hi.min(t);
            }
            if lo > hi {
                return false;
            }
        }
    }
    true
}

impl Browser {
    pub(super) fn cleanup_diagnostic_for_smoke(&self) -> serde_json::Value {
        serde_json::json!({
            "cleanup": self.features.cleanup.as_ref().map(|c| serde_json::json!({
                "started":c.started,
                "deadline_remaining_ms":c.deadline.map(|t|t.saturating_duration_since(Instant::now()).as_millis()),
                "pending_files":c.files.len(),"pending_directories":c.directories.len(),"worker":c.worker.is_some()
            })),
            "trash":self.trash_diagnostic_for_smoke(),
            "rotation":self.rotation.is_some(),"status":self.status
        })
    }
    pub(super) fn begin_cleanup_for_smoke(&mut self, root: &Path) {
        self.begin_cleanup(root);
    }
    pub(super) fn confirm_cleanup_for_smoke(&mut self) {
        if let Some(c) = &mut self.features.cleanup {
            c.deadline = Some(Instant::now() + Duration::from_secs(10));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paint_selection_segments_do_not_select_beyond_pointer() {
        let r = Rect::from_min_size(egui::pos2(10., 10.), egui::vec2(10., 10.));
        assert!(segment_hits(r, egui::pos2(0., 15.), egui::pos2(30., 15.)));
        assert!(!segment_hits(r, egui::pos2(0., 15.), egui::pos2(5., 15.)));
        assert!(!segment_hits(r, egui::pos2(0., 0.), egui::pos2(30., 0.)));
        assert!(segment_hits(r, r.center(), r.center()));
    }
}
