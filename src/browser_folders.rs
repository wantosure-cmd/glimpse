//! Folder headers, sticky breadcrumb, delayed recycle and progressive previews.
use super::*;
use media_browser::trash::TrashRequest;
use std::sync::mpsc::{self, Receiver};
pub(super) struct PendingFolders {
    prepared: Arc<Prepared>,
    paths: Vec<PathBuf>,
    deadline: Instant,
    receiver: Receiver<Result<Vec<TrashRequest>, String>>,
}
pub(super) type FolderPreparation = PendingFolders;
impl Browser {
    pub(super) fn progressive_job(
        &mut self,
        record: &MediaRecord,
        bucket: u32,
        priority: u8,
        jobs: &mut Vec<ThumbJob>,
    ) -> String {
        let full = self.job(record, bucket, priority);
        let key = full.key();
        if priority <= 1 {
            // Visible posters remain hot while a video FBO covers them as well.
            if let Some(entry) = self.textures.get_mut(&key) {
                entry.last_frame = self.frame;
            }
        }
        if self.textures.contains_key(&key) || self.failures.contains_key(&key) {
            if self.textures.contains_key(&key)
                && record.file.kind == MediaKind::Video
                && record.duration_ms.is_none()
            {
                jobs.push(self.job(record, 0, 4));
            }
            return key;
        }
        let direct_jpeg = priority == 1
            && bucket <= 1024
            && !media_browser::media_path::virtual_hint(&record.file.path)
            && record.file.path.extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg")
            })
            && self
                .benchmark
                .as_ref()
                .is_some_and(|b| b.experimental_jpeg_direct && !b.legacy_jpeg_preview);
        // JPEG's small preview already decodes for the clear target size.
        // Benchmark-only experiment. Ordinary browsing preserves the user's
        // explicit soft-preview-then-clear requirement for JPEG as well.
        if record.file.kind == MediaKind::Image && bucket > 64 && !direct_jpeg {
            let mut preview = self.job(record, 64, priority);
            preview.decode_bucket = bucket;
            if !self.textures.contains_key(&preview.key())
                && !self.failures.contains_key(&preview.key())
            {
                jobs.push(preview);
                return key;
            }
        }
        jobs.push(full);
        key
    }
    pub(super) fn folder_header(
        &mut self,
        ui: &mut egui::Ui,
        header: &media_browser::layout::Header,
        origin: Pos2,
        root: &Path,
    ) {
        let rect = to_rect(header.rect, origin);
        let depth = header
            .directory
            .strip_prefix(root)
            .map(|p| p.components().count())
            .unwrap_or(0);
        if header.kind.is_some() {
            ui.painter().text(
                rect.left_center()
                    + egui::vec2(
                        10. + media_browser::layout::tree_indent(depth, rect.width()),
                        0.,
                    ),
                egui::Align2::LEFT_CENTER,
                &header.title,
                egui::FontId::proportional(14.),
                Color32::GRAY,
            );
            return;
        }
        let indent = media_browser::layout::tree_indent(depth, rect.width());
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("folder-row", &header.directory))
                .max_rect(rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        child.spacing_mut().item_spacing = egui::vec2(3., 2.);
        child.spacing_mut().button_padding = egui::vec2(3., 1.);
        child.spacing_mut().interact_size = egui::vec2(18., 22.);
        ui.painter().rect_filled(
            rect,
            4.,
            if self.selected_folders.contains(&header.directory) {
                ui.visuals().selection.bg_fill
            } else {
                modern_ui::SURFACE
            },
        );
        for level in 0..depth {
            let x = rect.left() + 3. + media_browser::layout::tree_indent(level, rect.width());
            ui.painter().line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(modern_ui::STROKE, modern_ui::BORDER),
            );
        }
        child.add_space(indent + 4.);
        let mut selected = self.selected_folders.contains(&header.directory);
        if icons::selection_checkbox(&mut child, &mut selected).changed() {
            self.selection.clear();
            if selected {
                self.selected_folders.insert(header.directory.clone());
            } else {
                self.selected_folders.remove(&header.directory);
            }
        }
        let (toggle, toggle_response) =
            child.allocate_exact_size(egui::vec2(20., 26.), egui::Sense::click());
        self.folder_toggle_regions
            .insert(header.directory.clone(), toggle);
        if toggle_response.hovered() {
            child.painter().rect_filled(toggle, 4., modern_ui::RAISED);
        }
        icons::paint(
            child.painter(),
            toggle,
            if header.collapsed {
                icons::Glyph::Right
            } else {
                icons::Glyph::Down
            },
            modern_ui::MUTED,
        );
        if toggle_response.clicked() {
            self.toggle_folder_collapse(&header.directory);
        }
        let stats = media_browser::ui_format!(
            msg_032ad5fbb9db,
            header.counts.images,
            header.counts.videos,
            header.direct_folders
        );
        let (title_rect, response) = child.allocate_exact_size(
            egui::vec2(child.available_width().max(30.), 26.),
            egui::Sense::click(),
        );
        let font = egui::FontId::proportional(13.);
        let stats_width = child
            .painter()
            .layout_no_wrap(stats.clone(), font.clone(), Color32::GRAY)
            .size()
            .x;
        let name_rect = Rect::from_min_max(
            title_rect.min,
            egui::pos2(
                (title_rect.right() - stats_width - 10.).max(title_rect.left()),
                title_rect.bottom(),
            ),
        );
        child.painter().with_clip_rect(name_rect).text(
            name_rect.left_center() + egui::vec2(3., 0.),
            egui::Align2::LEFT_CENTER,
            &header.title,
            egui::FontId::proportional(14.),
            modern_ui::TEXT,
        );
        child.painter().with_clip_rect(title_rect).text(
            title_rect.right_center() - egui::vec2(4., 0.),
            egui::Align2::RIGHT_CENTER,
            stats,
            font,
            modern_ui::MUTED,
        );
        let name_width = child
            .painter()
            .layout_no_wrap(
                header.title.clone(),
                egui::FontId::proportional(14.),
                modern_ui::TEXT,
            )
            .size()
            .x
            + 6.;
        let name_hit = Rect::from_min_size(
            title_rect.min,
            egui::vec2(
                name_width.min(name_rect.width()).max(1.),
                title_rect.height(),
            ),
        );
        self.folder_title_regions
            .insert(header.directory.clone(), name_hit);
        if response.clicked() {
            if response
                .interact_pointer_pos()
                .is_some_and(|pos| name_hit.contains(pos))
            {
                self.locate_folder(&header.directory);
            } else {
                self.toggle_folder_collapse(&header.directory);
            }
        }
        if response.secondary_clicked() && !self.selected_folders.contains(&header.directory) {
            self.selected_folders.clear();
            self.selected_folders.insert(header.directory.clone());
        }
        response.context_menu(|ui| self.folder_menu(ui, &header.directory));
        response.on_hover_text(display_path(&header.directory));
    }
    pub(super) fn folder_menu(&mut self, ui: &mut egui::Ui, path: &Path) {
        modern_ui::quiet_menu(ui);
        self.favorite_folder_menu(ui, path);
        if ui
            .add(
                modern_ui::MenuAction::new(media_browser::ui_text!(msg_fcf8b4bff0df))
                    .icon(icons::Glyph::Folder),
            )
            .clicked()
        {
            self.navigate(path.to_owned(), true);
            ui.close();
        }
        if ui
            .add(
                modern_ui::MenuAction::new(media_browser::ui_text!(msg_d10ec81b9024))
                    .icon(icons::Glyph::Folder),
            )
            .clicked()
        {
            if let Err(e) = crate::platform::open_folder(path) {
                self.status = e;
            }
            ui.close();
        }
        if ui
            .add(
                modern_ui::MenuAction::new(media_browser::ui_text!(msg_640ae0a58455))
                    .icon(icons::Glyph::Copy),
            )
            .clicked()
        {
            ui.ctx().copy_text(display_path(path));
            ui.close();
        }
        if ui
            .add(
                modern_ui::MenuAction::new(media_browser::ui_text!(msg_0e17db06bb7a))
                    .icon(icons::Glyph::Search),
            )
            .clicked()
        {
            self.locate_folder(path);
            ui.close();
        }
        ui.separator();
        if ui
            .add(
                modern_ui::MenuAction::new(media_browser::ui_text!(msg_66950a93628f))
                    .icon(icons::Glyph::Select),
            )
            .clicked()
        {
            if !self.selected_folders.remove(path) {
                self.selected_folders.insert(path.to_owned());
            }
            ui.close();
        }
        ui.label(media_browser::ui_text!(msg_b48c9b0467b4));
        if ui
            .add_enabled(
                self.pending_folders.is_none()
                    && self.folder_prepare.is_none()
                    && self.trash_operation.is_none()
                    && self.rotation.is_none(),
                modern_ui::MenuAction::new(
                    egui::RichText::new(media_browser::ui_text!(msg_7276352d766d))
                        .color(modern_ui::DANGER),
                )
                .icon(icons::Glyph::Trash)
                .danger()
                .key("Delete"),
            )
            .clicked()
        {
            self.schedule_folder_delete(path);
            ui.close();
        }
        self.shortcut_settings(ui);
    }
    pub(super) fn toggle_folder_collapse(&mut self, path: &Path) {
        self.folder_location_anchor = None;
        if self.folder_collapsed(path) {
            self.collapsed.remove(path);
            self.auto_expanded.insert(path.to_owned());
            self.pending_anchor = self
                .folder_resume_anchors
                .get(path)
                .filter(|anchor| {
                    self.prepared.as_ref().is_some_and(|p| {
                        p.lookup.contains_key(&anchor.media_id)
                            && p.view.sequence.contains(&anchor.media_id)
                    })
                })
                .cloned();
            self.pending_folder_location = if self.pending_anchor.is_some() {
                None
            } else {
                Some(path.to_owned())
            };
        } else {
            self.auto_expanded.remove(path);
            // Keep the media and its screen offset before hiding its geometry.
            // A folder-title location must not override this bookmark on expand.
            if let Some(prepared) = &self.prepared {
                let anchor = prepared.layout.anchor(self.scroll_y).or_else(|| {
                    prepared
                        .layout
                        .visible_cards(self.scroll_y, self.scroll_y + self.viewport_height)
                        .into_iter()
                        .min_by(|a, b| a.rect.y.total_cmp(&b.rect.y))
                        .map(|card| ScrollAnchor {
                            media_id: card.id,
                            offset: card.rect.y - self.scroll_y,
                        })
                });
                if let Some(anchor) = anchor.filter(|anchor| {
                    prepared
                        .lookup
                        .get(&anchor.media_id)
                        .is_some_and(|i| prepared.snapshot.files[*i].file.parent.starts_with(path))
                }) {
                    self.folder_resume_anchors.insert(path.to_owned(), anchor);
                }
            }
            self.collapsed.insert(path.to_owned());
            self.pending_folder_location = Some(path.to_owned());
        }
        self.prepare_dirty = true;
        self.prepare_after = Instant::now();
    }
    pub(super) fn locate_folder(&mut self, path: &Path) {
        self.folder_location_anchor = Some(path.to_owned());
        let mut unfolded = false;
        for ancestor in path.ancestors().skip(1) {
            if self.folder_collapsed(ancestor) {
                unfolded = true;
                self.collapsed.remove(ancestor);
                self.auto_expanded.insert(ancestor.to_owned());
            }
        }
        if unfolded {
            self.pending_folder_location = Some(path.to_owned());
            self.prepare_dirty = true;
            self.prepare_after = Instant::now();
            return;
        }
        if let Some(p) = &self.prepared {
            if let Some(header) = p
                .layout
                .headers
                .iter()
                .find(|h| h.kind.is_none() && h.directory == path)
            {
                self.scroll_override = Some((header.rect.y - 52.).max(0.));
                self.ctx.request_repaint();
            }
        }
    }
    fn media_jump_target(&self, prepared: &Prepared) -> Option<PathBuf> {
        if self.prepare_dirty || prepared.revision != self.revision {
            return None;
        }
        let limit = self.scroll_y + 52.;
        let current = prepared.layout.headers[..prepared
            .layout
            .headers
            .partition_point(|h| h.rect.y <= limit + 0.25)]
            .iter()
            .rev()
            .find(|h| h.kind.is_none());
        let include = current.is_some_and(|h| {
            self.folder_collapsed(&h.directory) || (self.sticky_jump_up && h.rect.y < limit - 1.)
        });
        prepared
            .media_directories
            .target(
                current.map(|h| h.directory.as_path()),
                self.sticky_jump_up,
                include,
            )
            .map(Path::to_owned)
    }
    fn jump_to_media_folder(&mut self, path: &Path) {
        let root = self.prepared.as_ref().map(|p| p.snapshot.root.clone());
        let mut unfolded = false;
        for ancestor in path
            .ancestors()
            .take_while(|p| root.as_ref().is_some_and(|r| p.starts_with(r)))
        {
            if self.folder_collapsed(ancestor) {
                unfolded = true;
                self.collapsed.remove(ancestor);
                self.auto_expanded.insert(ancestor.to_owned());
            }
        }
        self.folder_location_anchor = Some(path.to_owned());
        if unfolded {
            self.pending_folder_location = Some(path.to_owned());
            self.prepare_dirty = true;
            self.prepare_after = Instant::now();
            self.ctx.request_repaint();
        } else {
            self.locate_folder(path);
        }
    }
    pub(super) fn folder_sticky(
        &mut self,
        ui: &mut egui::Ui,
        prepared: &Arc<Prepared>,
        viewport: Rect,
    ) {
        self.sticky_paths.clear();
        if let Some(smoke)=self.design_smoke.as_mut(){smoke.sticky_stats=None;}
        let Some(active) = prepared.layout.headers[..prepared
            .layout
            .headers
            .partition_point(|h| h.rect.y < self.scroll_y)]
            .iter()
            .rev()
            .find(|h| h.kind.is_none())
        else {
            return;
        };
        let mut chain: Vec<_> = active
            .directory
            .ancestors()
            .take_while(|p| p.starts_with(&prepared.snapshot.root))
            .map(Path::to_owned)
            .collect();
        chain.reverse();
        self.sticky_paths = chain.clone();
        let (media_counts,folders)=prepared.directory_stats.get(&active.directory).copied().unwrap_or_default();
        let stats=media_browser::ui_format!(msg_032ad5fbb9db,media_counts.images,media_counts.videos,folders);
        let target = self.media_jump_target(prepared);
        self.sticky_jump_target = target.clone();
        let ctx = ui.ctx().clone();
        egui::Area::new(egui::Id::unique("sticky-folder-chain"))
            .order(egui::Order::Middle)
            .fixed_pos(viewport.left_top())
            .show(&ctx, |ui| {
                ui.set_width(viewport.width());
                egui::Frame::new()
                    .fill(modern_ui::RAISED)
                    .inner_margin(4.)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let stats_width=ui.painter().layout_no_wrap(stats.clone(),egui::FontId::proportional(12.),modern_ui::MUTED)
                                .size().x.min(ui.available_width()*0.48);
                            let breadcrumb_width = (ui.available_width()-stats_width-32.-2.*ui.spacing().item_spacing.x).max(24.);
                            ui.allocate_ui_with_layout(
                                egui::vec2(breadcrumb_width, 28.),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.set_min_width(breadcrumb_width);
                                    egui::ScrollArea::horizontal()
                                        .id_salt("sticky-overflow")
                                        .max_width(breadcrumb_width)
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = 2.;
                                                for (i, path) in chain.iter().enumerate() {
                                                    if i > 0 {
                                                        ui.add_space(4.);
                                                        ui.label("/");
                                                        ui.add_space(4.);
                                                    }
                                                    let name = path
                                                        .file_name()
                                                        .unwrap_or(path.as_os_str())
                                                        .to_string_lossy();
                                                    let toggle = icons::compact_icon_button(
                                                        ui,
                                                        if self.folder_collapsed(path) {
                                                            icons::Glyph::Right
                                                        } else {
                                                            icons::Glyph::Down
                                                        },
                                                        name.as_ref(),
                                                    );
                                                    self.sticky_toggle_regions
                                                        .insert(path.clone(), toggle.rect);
                                                    if toggle.clicked() {
                                                        self.toggle_folder_collapse(path);
                                                    }
                                                    let response = ui.selectable_label(
                                                        self.selected_folders.contains(path),
                                                        name.as_ref(),
                                                    );
                                                    if response.clicked() {
                                                        self.locate_folder(path);
                                                    }
                                                    if response.secondary_clicked()
                                                        && !self.selected_folders.contains(path)
                                                    {
                                                        self.selected_folders.clear();
                                                        self.selected_folders.insert(path.clone());
                                                    }
                                                    response.context_menu(|ui| {
                                                        self.folder_menu(ui, path)
                                                    });
                                                }
                                                let remaining = ui
                                                    .available_width()
                                                    .clamp(0., viewport.width());
                                                if remaining > 0. {
                                                    let (_, blank) = ui.allocate_exact_size(
                                                        egui::vec2(remaining, 28.),
                                                        egui::Sense::click(),
                                                    );
                                                    if let Some(path) = chain.last() {
                                                        if blank.clicked() {
                                                            self.toggle_folder_collapse(path);
                                                        }
                                                        blank.context_menu(|ui| {
                                                            self.folder_menu(ui, path)
                                                        });
                                                    }
                                                }
                                            });
                                        });
                                },
                            );
                            let label=ui.allocate_ui_with_layout(egui::vec2(stats_width,28.),
                                egui::Layout::left_to_right(egui::Align::Center),|ui| {
                                    ui.add(egui::Label::new(egui::RichText::new(&stats).size(12.).color(modern_ui::MUTED)).truncate())
                                }).inner.on_hover_text(&stats);
                            if let Some(smoke)=self.design_smoke.as_mut(){smoke.sticky_stats=Some((media_counts,folders,label.rect));}
                            let response = ui
                                .add_enabled_ui(target.is_some(), |ui| {
                                    icons::floating_navigation_button(
                                        ui,
                                        if self.sticky_jump_up {
                                            icons::Glyph::Up
                                        } else {
                                            icons::Glyph::ArrowDown
                                        },
                                        if self.sticky_jump_up {
                                            media_browser::ui_text!(msg_sticky_jump_previous)
                                        } else {
                                            media_browser::ui_text!(msg_sticky_jump_next)
                                        },
                                    )
                                })
                                .inner;
                            self.sticky_jump_button = Some(response.rect);
                            if response.clicked() {
                                if let Some(path) = &target {
                                    self.jump_to_media_folder(path);
                                }
                            }
                        });
                    });
                self.control_regions.push(ui.min_rect());
                if ui.rect_contains_pointer(ui.min_rect()) {
                    self.hovered_video = None;
                }
            });
    }
    pub(super) fn schedule_folder_delete(&mut self, fallback: &Path) {
        if self.features.cleanup.is_some()
            || self.pending_trash.is_some()
            || self.pending_folders.is_some()
            || self.folder_prepare.is_some()
            || self.trash_operation.is_some()
            || self.rotation.is_some()
            || self.permanent_prompt.is_some()
        {
            return;
        }
        let Some(prepared) = self.prepared.clone() else {
            return;
        };
        let mut paths: Vec<_> = if self.selected_folders.is_empty() {
            vec![fallback.to_owned()]
        } else {
            self.selected_folders.iter().cloned().collect()
        };
        paths.sort_by(|a, b| view::directory_cmp(a, b));
        let mut roots: Vec<PathBuf> = Vec::new();
        for path in paths {
            if !roots.iter().any(|r| path.starts_with(r)) {
                roots.push(path);
            }
        }
        let (tx, receiver) = mpsc::channel();
        let work = roots.clone();
        let root = self
            .sidebar
            .as_ref()
            .map(|s| s.root.clone())
            .unwrap_or_else(|| prepared.snapshot.root.clone());
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let result = work
                .into_iter()
                .enumerate()
                .map(|(i, path)| {
                    if !path.starts_with(&root) {
                        return Err("Folder outside selected library".into());
                    }
                    let canonical = media_browser::media_path::resolve_read(&path)
                        .map_err(|e| e.to_string())?;
                    if canonical != path {
                        return Err("Folder path changed or is a link".into());
                    }
                    let boundary = if path == root {
                        root.parent()
                            .ok_or("Drive roots cannot be recycled")?
                            .to_owned()
                    } else {
                        root.clone()
                    };
                    let request = TrashRequest {
                        empty_only: false,
                        id: -(i as i64) - 1,
                        directory: true,
                        root: boundary,
                        fingerprint: Fingerprint::read(&path)?,
                        path,
                    };
                    media_browser::trash::prepare_folder(&request)?;
                    Ok(request)
                })
                .collect::<Result<Vec<_>, String>>();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        self.pending_folders = Some(PendingFolders {
            prepared,
            paths: roots,
            deadline: Instant::now() + Duration::from_secs(10),
            receiver,
        });
        self.ctx.request_repaint();
    }
    pub(super) fn folder_tick(&mut self, ctx: &egui::Context) {
        if self
            .pending_folders
            .as_ref()
            .is_some_and(|p| Instant::now() >= p.deadline)
        {
            self.folder_prepare = self.pending_folders.take();
        }
        if self.pending_folders.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.folder_prepare.is_some()
            && (self.trash_operation.is_some()
                || self.rotation.is_some()
                || self.permanent_prompt.is_some())
        {
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        let result = self.folder_prepare.as_ref().map(|p| p.receiver.try_recv());
        match result {
            Some(Ok(Ok(requests))) => {
                // Defer until other destructive operations are finished.
                if self.trash_operation.is_some()
                    || self.rotation.is_some()
                    || self.permanent_prompt.is_some()
                {
                    return;
                }
                let pending = self.folder_prepare.take().unwrap();
                let ids = pending
                    .prepared
                    .snapshot
                    .files
                    .iter()
                    .chain(self.prepared.iter().flat_map(|p| p.snapshot.files.iter()))
                    .filter(|m| pending.paths.iter().any(|p| m.file.path.starts_with(p)))
                    .map(|m| m.id)
                    .collect();
                self.close_detail();
                self.start_trash_requests(pending.prepared, requests, ids);
            }
            Some(Ok(Err(error))) => {
                self.folder_prepare = None;
                self.trash_message = Some(error);
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                self.folder_prepare = None;
                self.trash_message = Some(media_browser::ui_text!(msg_09a714c3b257).into());
            }
            _ => {}
        }
        if self.folder_prepare.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if let Some(rx) = &self.shell_result {
            if let Ok(result) = rx.try_recv() {
                self.shell_result = None;
                match result {
                    Ok(value) => self.shell_state = Some(value),
                    Err(e) => {
                        self.shell_state = Some(false);
                        self.shell_message = Some(e);
                    }
                }
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }
    }
    pub(super) fn folder_undo_ui(&mut self, ctx: &egui::Context) {
        let Some(pending) = &self.pending_folders else {
            return;
        };
        let seconds = pending
            .deadline
            .saturating_duration_since(Instant::now())
            .as_secs_f32()
            .ceil() as u32;
        let count = pending.paths.len();
        let mut undo = false;
        egui::Area::new(egui::Id::unique("folder-undo"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0., -32.))
            .show(ctx, |ui| {
                modern_ui::toast_frame(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(media_browser::ui_format!(
                            msg_10c013c4d012,
                            count = count,
                            seconds = seconds
                        ));
                        undo = ui
                            .button(media_browser::ui_text!(msg_6b5819895525))
                            .clicked();
                    });
                    ui.small(media_browser::ui_text!(msg_a5ca50de174e));
                });
            });
        if undo {
            self.pending_folders = None;
        }
    }
    pub(super) fn shell_settings(&mut self, ui: &mut egui::Ui) {
        #[cfg(windows)]
        {
            if self.shell_state.is_none() && self.shell_result.is_none() {
                self.shell_change(None);
            }
            let mut enabled = self.shell_state.unwrap_or(false);
            if ui
                .add_enabled(
                    self.shell_result.is_none() && self.shell_state.is_some(),
                    egui::Checkbox::new(&mut enabled, media_browser::ui_text!(msg_b1f425e529ca)),
                )
                .changed()
            {
                self.shell_change(Some(enabled));
            }
            ui.small(media_browser::ui_text!(msg_a6e9599d0c14));
            if let Some(message) = &self.shell_message {
                ui.label(
                    egui::RichText::new(message)
                        .small()
                        .color(modern_ui::DANGER),
                );
            }
        }
    }
    #[cfg(windows)]
    fn shell_change(&mut self, change: Option<bool>) {
        self.shell_message = None;
        let (tx, rx) = mpsc::channel();
        self.shell_result = Some(rx);
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                if let Some(enable) = change {
                    if enable {
                        media_browser::shell_menu::register(&exe, None)?;
                    } else {
                        media_browser::shell_menu::unregister(&exe, None)?;
                    }
                }
                media_browser::shell_menu::registered(&exe)
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
}

/// Opt-in product-code exercise; records native FBO screenshots, not physical input proof.
pub(super) struct FolderSmoke {
    output: PathBuf,
    partial: bool,
    preview_only: bool,
    events: Vec<egui::Event>,
    lock: Option<std::fs::File>,
    media_before: usize,
    files_before: usize,
    locked_path: Option<PathBuf>,
    phase: u8,
    at: Instant,
    started: Instant,
    rows: Vec<serde_json::Value>,
}
impl FolderSmoke {
    pub(super) fn new(output: PathBuf) -> Self {
        Self {
            output,
            partial: false,
            preview_only: false,
            events: Vec::new(),
            lock: None,
            media_before: 0,
            files_before: 0,
            locked_path: None,
            phase: 0,
            at: Instant::now(),
            started: Instant::now(),
            rows: vec![],
        }
    }
    pub(super) fn with_partial(mut self, value: bool) -> Self {
        self.partial = value;
        self
    }
    pub(super) fn with_preview_only(mut self, value: bool) -> Self {
        self.preview_only = value;
        self
    }
    pub(super) fn inject(&mut self, raw: &mut egui::RawInput) {
        if self.partial {
            raw.focused = true;
            raw.events.append(&mut self.events);
        }
    }
    fn click(&mut self, pos: Pos2) {
        self.events.push(egui::Event::PointerMoved(pos));
        for pressed in [true, false] {
            self.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
        }
    }
    fn advance(&mut self) {
        self.phase += 1;
        self.at = Instant::now();
    }
    pub(super) fn tick(&mut self, b: &mut Browser, ctx: &egui::Context) {
        if self.phase == 255 {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(16));
        if self.started.elapsed() > Duration::from_secs(60) {
            self.finish(b, ctx, Some("timeout"));
            return;
        }
        let Some(p) = b
            .prepared
            .clone()
            .filter(|p| p.snapshot.finished && !b.prepare_dirty)
        else {
            return;
        };
        // Never run deletion diagnostics against a user media directory.
        if !p.snapshot.root.components().any(|c| {
            c.as_os_str()
                .to_string_lossy()
                .starts_with("folder-smoke-owned-")
        }) {
            self.finish(
                b,
                ctx,
                Some("requires dedicated folder-smoke-owned fixture"),
            );
            return;
        }
        if self.partial {
            let project = std::fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
            if !p.snapshot.root.starts_with(project.join("target"))
                || std::fs::read(p.snapshot.root.join(".permanent-smoke-owned"))
                    .ok()
                    .as_deref()
                    != Some(b"generated media only")
            {
                self.finish(
                    b,
                    ctx,
                    Some("permanent diagnostic requires marked owned workspace fixture"),
                );
                return;
            }
        }
        match self.phase {
            0 if b.progressive_preview_paints > 0
                && b.textures.keys().any(|k| {
                    p.snapshot.files.iter().any(|r| {
                        *k == b
                            .job(
                                r,
                                thumbnail_bucket(p.layout.column_width * ctx.pixels_per_point()),
                                0,
                            )
                            .key()
                    })
                }) =>
            {
                self.rows.push(serde_json::json!({"step":"progressive","preview_paints":b.progressive_preview_paints,"hierarchy":p.view.groups.iter().map(|g|&g.directory).collect::<Vec<_>>()}));
                b.snapshot_image(ctx, "folder-progressive-clear.png");
                if self.preview_only {
                    self.finish(b, ctx, None);
                    return;
                }
                self.advance();
            }
            1..=3 => {
                let suffix = ["123", "123/456", "123/456/789"][(self.phase - 1) as usize];
                let path = p.snapshot.root.join(suffix);
                if self.at.elapsed() < Duration::from_millis(80) {
                    if let Some(h) = p
                        .layout
                        .headers
                        .iter()
                        .find(|h| h.directory == path && h.kind.is_none())
                    {
                        b.scroll_override = Some(h.rect.y + 1.);
                    }
                } else if b.sticky_paths.last() == Some(&path) {
                    self.rows
                        .push(serde_json::json!({"step":"sticky","paths":b.sticky_paths}));
                    b.snapshot_image(ctx, &format!("folder-sticky-{}.png", self.phase));
                    self.advance();
                }
            }
            4 => {
                let path = p.snapshot.root.join("123/456");
                b.locate_folder(&path);
                self.rows.push(serde_json::json!({"step":"locate","offset":b.scroll_override,"expected":p.layout.headers.iter().find(|h|h.directory==path&&h.kind.is_none()).map(|h|(h.rect.y-38.).max(0.))}));
                b.selected_folders =
                    HashSet::from([p.snapshot.root.join("123"), p.snapshot.root.join("123/456")]);
                b.schedule_folder_delete(&path);
                self.advance();
            }
            5 if self.at.elapsed() > Duration::from_millis(1000) => {
                let pending = b.pending_folders.take().unwrap();
                self.rows.push(serde_json::json!({"step":"undo","deduplicated":pending.paths.len()==1,"before_deadline":Instant::now()<pending.deadline,"files_present":p.snapshot.root.join("123/456/789/photo.jpg").exists()}));
                self.advance();
            }
            6 if self.at.elapsed() > Duration::from_millis(100) => {
                b.selected_folders = HashSet::from([p.snapshot.root.join("delete-me")]);
                b.schedule_folder_delete(&p.snapshot.root.join("delete-me"));
                self.advance();
            }
            7 if b.last_trash.is_some() && b.trash_operation.is_none() => {
                self.rows.push(serde_json::json!({"step":"commit","elapsed_seconds":self.at.elapsed().as_secs_f64(),"result":b.last_trash,"original_undo_folder_exists":p.snapshot.root.join("123/456/789/photo.jpg").exists(),"delete_folder_exists":p.snapshot.root.join("delete-me").exists(),"confirmation_prompt":b.permanent_prompt.is_some()}));
                if b.permanent_prompt.is_some() {
                    b.snapshot_image(ctx, "folder-permanent-confirmation.png");
                    self.advance();
                } else {
                    self.finish(b, ctx, None);
                }
            }
            8 if self.at.elapsed() > Duration::from_millis(600) => {
                self.rows.push(serde_json::json!({"step":"confirmation_wait","prompt_still_open":b.permanent_prompt.is_some(),"source_still_present":p.snapshot.root.join("delete-me/notes.txt").exists()}));
                b.snapshot_image(ctx, "folder-permanent-confirmation.png");
                self.advance();
            }
            9 if self.partial && self.at.elapsed() > Duration::from_millis(160) => {
                let Some((ack, _, enabled)) = b.permanent_controls else {
                    return;
                };
                if enabled {
                    self.finish(
                        b,
                        ctx,
                        Some("confirmation unexpectedly enabled before checkbox"),
                    );
                    return;
                }
                let folder = p.snapshot.root.join("delete-me");
                let files: Vec<_> = std::fs::read_dir(&folder)
                    .unwrap()
                    .map(|r| r.unwrap().path())
                    .filter(|p| p.is_file())
                    .collect();
                self.files_before = files.len();
                self.media_before = files.iter().filter(|p| media_kind(p).is_some()).count();
                let locked = files.last().unwrap().clone();
                #[cfg(windows)]
                {
                    use std::os::windows::fs::OpenOptionsExt;
                    self.lock = Some(
                        std::fs::OpenOptions::new()
                            .read(true)
                            .share_mode(0)
                            .open(&locked)
                            .unwrap(),
                    );
                }
                self.locked_path = Some(locked);
                self.click(ack.center());
                self.advance();
            }
            10 if self.partial && self.at.elapsed() > Duration::from_millis(120) => {
                let Some((_, button, enabled)) = b.permanent_controls else {
                    return;
                };
                if !enabled {
                    return;
                }
                self.rows.push(serde_json::json!({"step":"checkbox_enabled","enabled":enabled,"input":"controlled egui pointer events, not physical mouse"}));
                self.click(button.center());
                self.advance();
            }
            11 if self.partial
                && b.trash_operation.is_none()
                && b.last_trash
                    .as_ref()
                    .is_some_and(|r| r["permanent"] == true) =>
            {
                let folder = p.snapshot.root.join("delete-me");
                let remaining = std::fs::read_dir(&folder).unwrap().count();
                self.rows.push(serde_json::json!({"step":"permanent_partial_result","before":self.files_before,"remaining":remaining,"result":b.last_trash,"locked_survivor":self.locked_path.as_ref().is_some_and(|p|p.exists())}));
                self.advance();
            }
            12 if self.partial && self.at.elapsed() > Duration::from_millis(650) => {
                b.snapshot_image(ctx, "permanent-partial-failure.png");
                self.lock = None;
                b.refresh_in_place();
                self.advance();
            }
            13 if self.partial && self.at.elapsed() > Duration::from_millis(500) => {
                let remaining_media = std::fs::read_dir(p.snapshot.root.join("delete-me"))
                    .unwrap()
                    .filter_map(|r| r.ok())
                    .filter(|r| media_kind(&r.path()).is_some())
                    .count();
                self.rows.push(serde_json::json!({"step":"partial_refresh","remaining_media":remaining_media,"files_in_snapshot":p.snapshot.files.len(),"media_before":self.media_before,"expected_snapshot_files":35-self.media_before+remaining_media}));
                self.finish(b, ctx, None);
            }
            9 if self.at.elapsed() > Duration::from_millis(160) => {
                b.permanent_prompt = None; // Controlled cancellation, never confirms deletion.
                self.finish(b, ctx, None);
            }
            _ => {}
        }
    }
    fn finish(&mut self, b: &Browser, ctx: &egui::Context, error: Option<&str>) {
        let data = serde_json::json!({"scope":"controlled product actions and native FBO, not OS physical input", "error":error,"exe_sha256":std::env::current_exe().ok().and_then(|p|std::fs::read(p).ok()).map(|v|{use sha2::Digest;format!("{:x}",sha2::Sha256::digest(v))}),"steps":self.rows,"preview_paints":b.progressive_preview_paints});
        let _ = std::fs::write(&self.output, serde_json::to_vec_pretty(&data).unwrap());
        self.phase = 255;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
