//! Optional mixed-media slideshow. Manual viewing and its EOF policy stay separate.
use super::*;
use media_browser::presentation::{self, Clock, Item, Page, Preferences};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Mode {
    Manual,
    Sequential,
    Group,
}
pub(super) struct Presentation {
    pub prefs: Preferences,
    pub mode: Mode,
    pub playing: bool,
    pub ready: bool,
    pub tiles: Vec<(MediaId, Rect)>,
    plan: Vec<Page>,
    key: Option<(u64, usize)>,
    clock: Clock,
}
impl Presentation {
    pub fn new(mut prefs: Preferences) -> Self {
        prefs.validate();
        Self {
            prefs,
            mode: Mode::Manual,
            playing: false,
            ready: false,
            tiles: vec![],
            plan: vec![],
            key: None,
            clock: Clock::default(),
        }
    }
    pub fn ensure_plan(&mut self, prepared: &Prepared) {
        let size = if self.mode == Mode::Group {
            self.prefs.group_size
        } else {
            1
        };
        if self.key == Some((prepared.revision, size)) {
            return;
        }
        let items: Vec<_> = prepared
            .view
            .sequence
            .iter()
            .filter_map(|id| prepared.lookup.get(id))
            .map(|i| {
                let r = &prepared.snapshot.files[*i];
                Item {
                    id: r.id,
                    kind: r.file.kind,
                    parent: r.file.parent.clone(),
                }
            })
            .collect();
        self.plan = presentation::pages(&items, size);
        self.key = Some((prepared.revision, size));
    }
    pub fn ids(&self, id: MediaId) -> Vec<MediaId> {
        self.plan
            .iter()
            .find(|p| p.ids.contains(&id))
            .map(|p| p.ids.clone())
            .unwrap_or_else(|| vec![id])
    }
    pub fn reset_clock(&mut self) {
        self.clock.reset();
    }
}

impl Browser {
    pub(super) fn detail_target(
        &self,
        prepared: &Prepared,
        id: MediaId,
        delta: i32,
        repeat: bool,
        clamp: bool,
    ) -> Option<MediaId> {
        let groups = self.presentation.mode == Mode::Group;
        let current = if groups {
            self.presentation
                .plan
                .iter()
                .position(|p| p.ids.contains(&id))
        } else {
            prepared.view.sequence.iter().position(|i| *i == id)
        }?;
        let count = if groups {
            self.presentation.plan.len()
        } else {
            prepared.view.sequence.len()
        };
        if count == 0 {
            return None;
        }
        let mut target = current as i64 + delta as i64;
        if repeat {
            target = target.rem_euclid(count as i64);
        } else if clamp {
            target = target.clamp(0, count as i64 - 1);
        } else if target < 0 || target >= count as i64 {
            return None;
        }
        if groups {
            self.presentation.plan[target as usize].ids.first().copied()
        } else {
            prepared.view.sequence.get(target as usize).copied()
        }
    }

    pub(super) fn set_presentation_mode(&mut self, mode: Mode) {
        let restore_animation = self.presentation.mode == Mode::Group && mode != Mode::Group;
        self.presentation.mode = mode;
        self.presentation.playing = mode != Mode::Manual;
        self.presentation.key = None;
        self.presentation.reset_clock();
        if mode == Mode::Group {
            self.animation.clear();
        }
        if let Some(prepared) = self.prepared.clone() {
            if restore_animation {
                if let Some(record) = self
                    .detail
                    .as_ref()
                    .and_then(|v| prepared.lookup.get(&v.id))
                    .map(|i| &prepared.snapshot.files[*i])
                {
                    if record
                        .file
                        .path
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("gif"))
                    {
                        self.animation.open(&record.file);
                    }
                }
            }
            self.presentation.ensure_plan(&prepared);
        }
        if let Some(player) = &self.player {
            if self
                .detail
                .as_ref()
                .is_some_and(|v| Some(v.id) == self.player_media)
                && mode != Mode::Manual
            {
                let _ = player.lock().unwrap().set("pause", "no");
            }
        }
    }

    pub(super) fn presentation_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(media_browser::ui_text!(msg_a5a25e880c5e), |ui| {
            if ui
                .selectable_label(
                    self.presentation.mode == Mode::Sequential,
                    media_browser::ui_text!(msg_e92a0ae71f79),
                )
                .clicked()
            {
                self.set_presentation_mode(Mode::Sequential);
                ui.close();
            }
            if ui
                .selectable_label(
                    self.presentation.mode == Mode::Group,
                    media_browser::ui_text!(msg_33a3a58adb5a),
                )
                .clicked()
            {
                self.set_presentation_mode(Mode::Group);
                ui.close();
            }
            if self.presentation.mode != Mode::Manual {
                if ui
                    .button(if self.presentation.playing {
                        media_browser::ui_text!(msg_f6e186580dd7)
                    } else {
                        media_browser::ui_text!(msg_d97d53f05833)
                    })
                    .clicked()
                {
                    self.toggle_presentation_pause();
                    ui.close();
                }
                if ui
                    .button(media_browser::ui_text!(msg_a27f76e6f97d))
                    .clicked()
                {
                    self.set_presentation_mode(Mode::Manual);
                    ui.close();
                }
            }
            ui.separator();
            ui.add(super::modern_ui::QuietSlider(
                egui::Slider::new(&mut self.presentation.prefs.seconds, 1.0..=60.0)
                    .suffix(media_browser::ui_text!(msg_00fe8dda1fd7))
                    .text(media_browser::ui_text!(msg_cfa694dce5b5)),
            ));
            if ui
                .add(super::modern_ui::QuietSlider(
                    egui::Slider::new(&mut self.presentation.prefs.group_size, 2..=5)
                        .text(media_browser::ui_text!(msg_7e3c63f800d1)),
                ))
                .changed()
            {
                self.presentation.key = None;
                self.presentation.reset_clock();
            }
            ui.checkbox(
                &mut self.presentation.prefs.repeat,
                media_browser::ui_text!(msg_dfca64c65c59),
            );
            ui.checkbox(
                &mut self.presentation.prefs.fill,
                media_browser::ui_text!(msg_b2ae4fd1689b),
            );
            ui.weak(media_browser::ui_text!(msg_fb3471da3c27));
        });
    }

    pub(super) fn toggle_detail_pause(&mut self) {
        self.hidden_pause = None;
        if self.presentation.mode != Mode::Manual {
            self.toggle_presentation_pause();
        } else if let Some(p) = &self.player {
            if self
                .detail
                .as_ref()
                .is_some_and(|v| Some(v.id) == self.player_media)
            {
                let _ = p.lock().unwrap().toggle_pause();
            }
        }
    }

    pub(super) fn toggle_presentation_pause(&mut self) {
        self.presentation.playing = !self.presentation.playing;
        self.presentation.reset_clock();
        if let Some(player) = &self.player {
            if self
                .detail
                .as_ref()
                .is_some_and(|v| Some(v.id) == self.player_media)
            {
                let _ = player.lock().unwrap().set(
                    "pause",
                    if self.presentation.playing {
                        "no"
                    } else {
                        "yes"
                    },
                );
            }
        }
    }

    pub(super) fn presentation_badge(&mut self, ui: &mut egui::Ui, canvas: Rect) {
        if self.presentation.mode == Mode::Manual {
            return;
        }
        let rect = Rect::from_min_size(
            egui::pos2(
                (canvas.right() - 240.).max(canvas.left() + 12.),
                canvas.top() + 72.,
            ),
            egui::vec2(224., 28.),
        );
        let mut overlay = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("slideshow-controls")
                .max_rect(rect),
        );
        let response = modern_ui::overlay_frame().show(&mut overlay, |ui| {
            ui.horizontal(|ui| {
                ui.label(if self.presentation.mode == Mode::Group {
                    media_browser::ui_text!(msg_c7bb9b555b95)
                } else {
                    media_browser::ui_text!(msg_3985eb783ecf)
                });
                if ui
                    .small_button(if self.presentation.playing {
                        media_browser::ui_text!(msg_8d12fc0d4eb2)
                    } else {
                        media_browser::ui_text!(msg_7c9691192f1b)
                    })
                    .clicked()
                {
                    self.toggle_presentation_pause();
                }
                self.presentation_menu(ui);
            });
        });
        self.control_regions.push(response.response.rect);
    }

    pub(super) fn paint_collage(
        &mut self,
        ui: &mut egui::Ui,
        canvas: Rect,
        prepared: &Prepared,
        viewer: &mut Viewer,
        jobs: &mut Vec<ThumbJob>,
        next: &mut Option<MediaId>,
        close: &mut bool,
    ) {
        let current = viewer.id;
        self.presentation.tiles.clear();
        self.presentation.ready = true;
        self.routed_zoom = 0.;
        let records: Vec<_> = self
            .presentation
            .ids(current)
            .iter()
            .filter_map(|id| prepared.lookup.get(id))
            .map(|i| prepared.snapshot.files[*i].clone())
            .collect();
        let aspects: Vec<_> = records
            .iter()
            .map(|r| r.width.unwrap_or(1) as f32 / r.height.unwrap_or(1).max(1) as f32)
            .collect();
        let tiles = presentation::collage(&aspects, canvas.width(), canvas.height(), 4.);
        for (record, tile) in records.iter().zip(tiles) {
            let cell = Rect::from_min_size(
                canvas.min + egui::vec2(tile.x, tile.y),
                egui::vec2(tile.w, tile.h),
            );
            self.presentation.tiles.push((record.id, cell));
            let pixels =
                (cell.width().max(cell.height()) * ui.ctx().pixels_per_point()).ceil() as u32;
            let bucket = [512, 1024, 2048]
                .into_iter()
                .find(|b| *b >= pixels)
                .unwrap_or(2048);
            let job = self.job(record, bucket, 0);
            let key = job.key();
            self.progressive_job(record, bucket, 0, jobs);
            let texture = self.texture(&key).or_else(|| {
                [2048, 1024, 512, 256, 64]
                    .into_iter()
                    .find_map(|b| self.texture(&self.job(record, b, 0).key()))
            });
            if let Some(texture) = texture {
                let size = egui::vec2(
                    record.width.unwrap_or(texture.size()[0] as u32) as f32,
                    record.height.unwrap_or(texture.size()[1] as u32).max(1) as f32,
                );
                let mut uv = Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.));
                let dest = if self.presentation.prefs.fill {
                    let source = size.x / size.y;
                    let target = cell.width() / cell.height();
                    if source > target {
                        let half = target / source * 0.5;
                        uv.min.x = 0.5 - half;
                        uv.max.x = 0.5 + half;
                    } else {
                        let half = source / target * 0.5;
                        uv.min.y = 0.5 - half;
                        uv.max.y = 0.5 + half;
                    }
                    cell
                } else {
                    let fit = (cell.width() / size.x).min(cell.height() / size.y);
                    Rect::from_center_size(cell.center(), size * fit)
                };
                ui.painter()
                    .with_clip_rect(canvas)
                    .image(texture.id(), dest, uv, Color32::WHITE);
            } else {
                let error = self.failures.get(&key);
                self.presentation.ready &= error.is_some();
                ui.painter().text(
                    cell.center(),
                    egui::Align2::CENTER_CENTER,
                    error
                        .map(String::as_str)
                        .unwrap_or(media_browser::ui_text!(msg_4927a53bcc88)),
                    egui::FontId::proportional(14.),
                    Color32::LIGHT_GRAY,
                );
            }
            let response = ui.interact(
                cell,
                ui.scope_id().with(("collage-image", record.id)),
                egui::Sense::click(),
            );
            if response.double_clicked()
                && !self.workspace.is_open()
                && !egui::Popup::is_any_open(ui.ctx())
            {
                *close = true;
            }
            response.context_menu(|ui| {
                if ui
                    .button(media_browser::ui_text!(msg_8fa0661edd77))
                    .clicked()
                {
                    self.set_presentation_mode(Mode::Manual);
                    *next = Some(record.id);
                    ui.close();
                }
                self.detail_context_menu(ui, record, viewer, next, close);
            });
        }
        self.image_region = Some(canvas);
    }

    pub(super) fn presentation_tick(
        &mut self,
        ctx: &egui::Context,
        viewer: &mut Viewer,
        prepared: &Prepared,
        next: &mut Option<MediaId>,
    ) {
        if self.presentation.mode == Mode::Manual {
            return;
        }
        let eligible = self.presentation.playing
            && next.is_none()
            && self.trash_operation.is_none()
            && self.rotation.is_none()
            && !self.workspace.is_open()
            && !egui::Popup::is_any_open(ctx)
            && ctx.input(|i| {
                i.focused
                    && i.viewport().minimized != Some(true)
                    && i.viewport().occluded != Some(true)
            });
        let Some(record) = prepared
            .lookup
            .get(&viewer.id)
            .map(|i| &prepared.snapshot.files[*i])
        else {
            return;
        };
        let video = record.file.kind == MediaKind::Video;
        let eof = video
            && self.player_media == Some(viewer.id)
            && self.resume_pending.is_none()
            && self.seek_flight.is_none()
            && self.seeks.target().is_none()
            && self.player.as_ref().is_some_and(|p| {
                let p = p.lock().unwrap();
                p.ready() && p.text("eof-reached").as_deref() == Some("yes")
            });
        let failed = self.playback_errors.contains_key(&viewer.id);
        let timed = self.presentation.clock.tick(
            Instant::now(),
            eligible && ((!video && self.presentation.ready) || failed),
            self.presentation.prefs.seconds,
        );
        if eligible && (eof || timed) {
            let target = self.detail_target(
                prepared,
                viewer.id,
                1,
                self.presentation.prefs.repeat,
                false,
            );
            self.presentation.reset_clock();
            if let Some(id) = target {
                if video && id == viewer.id {
                    self.request_seek(0., true);
                    if let Some(p) = &self.player {
                        let _ = p.lock().unwrap().set("pause", "no");
                    }
                    viewer.end_handled = false;
                } else if id != viewer.id {
                    self.positions.remove(&id);
                    *next = Some(id);
                }
            } else {
                self.presentation.playing = false;
            }
        } else if eligible && ((!video && self.presentation.ready) || failed) {
            ctx.request_repaint_after(
                self.presentation
                    .clock
                    .remaining(self.presentation.prefs.seconds)
                    .max(Duration::from_millis(16)),
            );
        }
    }
}
