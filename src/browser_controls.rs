//! Browser's raw-input adapter and the single active-player controller.
use super::*;

impl Browser {
    pub(super) fn video_turn(&self, record: &MediaRecord) -> u16 {
        self.video_turns
            .get(&record.id)
            .filter(|(fp, _)| fp == &record.file.fingerprint)
            .map(|(_, angle)| *angle)
            .unwrap_or(0)
    }
    pub(super) fn rotate_video(&mut self, record: &MediaRecord, clockwise: bool) {
        if record.file.kind != MediaKind::Video {
            return;
        }
        let angle = (self.video_turn(record) + if clockwise { 90 } else { 270 }) % 360;
        if self.player_media == Some(record.id) {
            if let Some(player) = &self.player {
                if let Err(error) = player
                    .lock()
                    .unwrap()
                    .set("video-rotate", &angle.to_string())
                {
                    self.status = error;
                    return;
                }
            }
        }
        self.video_turns
            .insert(record.id, (record.file.fingerprint.clone(), angle));
        self.status = media_browser::ui_format!(msg_b44b9173d254, angle = angle);
        self.ctx.request_repaint();
    }
    pub(super) fn video_size(&self, record: &MediaRecord) -> Vec2 {
        if let Some((mut w, mut h)) = record.width.zip(record.height) {
            if self.video_turn(record) % 180 == 90 {
                std::mem::swap(&mut w, &mut h);
            }
            egui::vec2(w as f32, h as f32)
        } else {
            self.player
                .as_ref()
                .and_then(|p| p.lock().unwrap().display_size())
                .map(|(w, h)| egui::vec2(w, h))
                .unwrap_or(egui::vec2(16., 9.))
        }
    }
    pub(super) fn video_rotation_menu(&mut self, ui: &mut egui::Ui, record: &MediaRecord) {
        for (clockwise, label, glyph) in [
            (
                false,
                media_browser::ui_text!(msg_104b0e8a6eca),
                icons::Glyph::RotateLeft,
            ),
            (
                true,
                media_browser::ui_text!(msg_82751aa3ca5f),
                icons::Glyph::Rotate,
            ),
        ] {
            let response = ui.add(modern_ui::MenuAction::new(label).icon(glyph));
            if let Some(smoke) = self.design_smoke.as_mut() {
                smoke.rotation_regions.insert(clockwise, response.rect);
            }
            if response
                .on_hover_text(media_browser::ui_text!(msg_55b564bc16bb))
                .clicked()
            {
                self.rotate_video(record, clockwise);
                ui.close();
            }
        }
    }
    pub(super) fn remember_position(&mut self) {
        let Some(id) = self.player_media else {
            return;
        };
        let Some(player) = &self.player else {
            return;
        };
        let p = player.lock().unwrap();
        if !p.ready() {
            return;
        }
        // Reopening a completed video should start from zero rather than
        // immediately trigger the detail viewer's end-of-file action again.
        let position = if p.text("eof-reached").as_deref() == Some("yes") {
            Some(0.)
        } else {
            self.seeks.target().or_else(|| p.number("time-pos"))
        };
        let Some(position) = position else {
            return;
        };
        if let Some(record) = self
            .snapshot
            .as_ref()
            .and_then(|s| s.files.iter().find(|r| r.id == id))
        {
            self.positions
                .insert(id, (record.file.fingerprint.clone(), position));
        }
    }

    pub(super) fn activate_video(
        &mut self,
        record: &MediaRecord,
        detail: bool,
    ) -> Result<(), String> {
        if self.trash_releasing() || self.trash_blocks_media(record.id) {
            return Err(media_browser::ui_text!(msg_21e9b02cc82e).into());
        }
        if self.player_media != Some(record.id) {
            self.remember_position();
        }
        if self.player.is_none() {
            self.player = Some(Arc::new(Mutex::new(Player::new(
                Some(self.proc.clone()),
                Some(self.ctx.clone()),
            )?)));
        }
        let mut p = self.player.as_ref().unwrap().lock().unwrap();
        let changed = self.player_media != Some(record.id)
            || p.source.is_none()
            || self.player_version.as_ref() != Some(&record.file.fingerprint)
            || self.playback_errors.contains_key(&record.id)
            || p.error().is_some();
        self.playback_errors.remove(&record.id);
        if changed {
            self.resume_pause_override=None;
            self.subtitle_notice = None;
            self.seek_notice = None;
            self.load_started = Some(Instant::now());
            p.set("pause", "yes")?;
            for (key, value) in self.subtitles.options() {
                p.set(key, &value)?;
            }
            p.set("video-rotate", &self.video_turn(record).to_string())?;
            p.load(&record.file.path)?;
            self.seeks.begin_load(record.id);
            self.seek_flight = None;
            self.resume_pending = self
                .positions
                .get(&record.id)
                .filter(|(fingerprint, _)| fingerprint == &record.file.fingerprint)
                .map(|(_, position)| *position)
                .filter(|position| *position > 0.);
            self.player_media = Some(record.id);
            self.player_version = Some(record.file.fingerprint.clone());
            self.resume_starting = self.resume_pending.is_some();
            self.restore_flight = self.resume_pending.is_some();
        }
        p.set("loop-file", if detail { "no" } else { "inf" })?;
        p.set("mute", if self.muted { "yes" } else { "no" })?;
        p.set("volume", &format!("{:.1}", self.volume))?;
        if self.resume_pending.is_none() {
            let paused = detail
                && self.presentation.mode != presentation_ui::Mode::Manual
                && !self.presentation.playing;
            p.set("pause", if paused { "yes" } else { "no" })?;
        }
        self.preview_active = if detail { None } else { Some(record.id) };
        self.preview_idle = None;
        Ok(())
    }

    pub(super) fn request_seek(&mut self, delta: f64, absolute: bool) {
        let Some(player) = self.player.clone() else {
            return;
        };
        let p = player.lock().unwrap();
        let ready = p.ready();
        let duration = if ready { p.number("duration") } else { None };
        let seekable = p.text("seekable").as_deref() != Some("no");
        let result = if absolute {
            self.seeks.absolute(delta, duration, seekable)
        } else {
            self.seeks.relative(
                delta,
                p.number("time-pos").unwrap_or(0.),
                duration,
                seekable,
            )
        };
        match result {
            Ok(target) => {
                self.status = media_browser::ui_format!(
                    msg_1206f03da8b6,
                    if absolute {
                        media_browser::ui_text!(msg_49726f09c812).to_owned()
                    } else {
                        format!("{delta:+.0}s")
                    },
                    format_time((target * 1000.) as u64)
                )
            }
            Err(error) => self.status = error.into(),
        }
        self.ctx.request_repaint();
        if let Some(id) = self.player_media {
            self.seek_notice = Some((id, self.status.clone(), Instant::now()));
        }
    }

    pub(super) fn playback_tick(&mut self, ctx: &egui::Context) {
        let Some(player) = self.player.clone() else {
            return;
        };
        let p = player.lock().unwrap();
        let ready = p.ready();
        let load_error =
            p.error().or_else(|| {
                (!ready
                    && self.load_started.is_some_and(|since| {
                        since.elapsed()
                            > Duration::from_secs(
                                if self.snapshot.as_ref().is_some_and(|s| {
                                    media_browser::media_path::virtual_hint(&s.root)
                                }) {
                                    60
                                } else {
                                    12
                                },
                            )
                    }))
                .then(|| media_browser::ui_text!(msg_fe4d09f0da6a).to_owned())
            });
        if let Some(error) = load_error {
            self.load_started = None;
            if let (Some(id), Some(version)) = (self.player_media, self.player_version.clone()) {
                if !self.playback_errors.contains_key(&id) {
                    self.playback_errors.insert(id, (version, error.clone()));
                    self.status = error;
                    self.preview_active = None;
                    self.hover_candidate = None;
                    self.preview_idle = Some(Instant::now());
                }
            }
            self.seeks.cancel();
            self.seek_flight = None;
            self.resume_pending = None;
            self.resume_starting = false;
            self.restore_flight = false;
            return;
        }
        if !ready && self.load_started.is_some() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if ready {
            self.load_started = None;
            if let Some(position) = self.resume_pending.take() {
                if let Some(duration) = p.number("duration") {
                    let _ = self.seeks.absolute(
                        position,
                        Some(duration),
                        p.text("seekable").as_deref() != Some("no"),
                    );
                } else {
                    self.resume_pending = Some(position);
                }
            }
            if let Some((request, sent)) = self.seek_flight {
                if self
                    .seeks
                    .acknowledge(request, p.number("time-pos").unwrap_or(-10.))
                {
                    self.seek_flight = None;
                    self.restore_flight = false;
                } else if sent.elapsed() > Duration::from_secs(5) {
                    self.seeks.cancel();
                    self.seek_flight = None;
                    self.status = media_browser::ui_text!(msg_d116b1d83be8).into();
                    self.restore_flight = false;
                }
            }
            if let Some(request) = self.seeks.due(self.started.elapsed().as_millis() as u64) {
                match p.seek(request.target) {
                    Ok(()) => {
                        self.seek_flight = Some((request, Instant::now()));
                        // Restoring a saved position starts paused to avoid flashing frame zero.
                        if self.resume_starting {
                            let default_paused = self.detail.is_some()
                                && self.presentation.mode != presentation_ui::Mode::Manual
                                && !self.presentation.playing;
                            let paused=self.resume_pause_override.take().unwrap_or(default_paused);
                            let _ = p.set("pause", if paused { "yes" } else { "no" });
                            self.resume_starting = false;
                        }
                    }
                    Err(error) => {
                        self.status = error;
                        self.seeks.cancel();
                        self.restore_flight = false;
                    }
                }
            }
        }
        if self.seeks.target().is_some() || self.resume_pending.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    pub(super) fn hover_tick(&mut self, ctx: &egui::Context) {
        if self.detail.is_some() || self.trash_releasing() {
            self.hover_candidate = None;
            return;
        }
        let on_controls = ctx.input(|i| {
            i.pointer
                .hover_pos()
                .is_some_and(|p| self.preview_controls_rect.is_some_and(|r| r.contains(p)))
        });
        let allowed = ctx.input(|i| {
            i.focused
                && i.viewport().minimized != Some(true)
                && i.viewport().occluded != Some(true)
                && (!i.pointer.any_down() || on_controls)
                && !i.is_scrolling()
                && !i
                    .events
                    .iter()
                    .any(|e| matches!(e, egui::Event::MouseWheel { .. }))
        }) && !self.workspace.is_open()
            && !egui::Popup::is_any_open(ctx)
            && !ctx.text_edit_focused()
            && (self.pinned || !self.sidebar_open);
        let candidate = if allowed {
            self.hovered_video.filter(|id| {
                if self.trash_blocks_media(*id) { return false; }
                !self.playback_errors.get(id).is_some_and(|(version, _)| {
                    self.prepared.as_ref().and_then(|p| {
                        p.lookup
                            .get(id)
                            .map(|i| &p.snapshot.files[*i].file.fingerprint)
                    }) == Some(version)
                })
            })
        } else {
            None
        };
        if candidate != self.preview_active && self.preview_active.is_some() {
            self.resume_starting = false;
            self.remember_position();
            if let Some(p) = &self.player {
                let p = p.lock().unwrap();
                let _ = p.set("pause", "yes");
                let _ = p.set("mute", "yes");
            }
            self.preview_active = None;
            self.video_region = None;
            self.preview_idle = Some(Instant::now());
        }
        if candidate != self.hover_candidate.map(|(id, _)| id) {
            self.hover_candidate = candidate.map(|id| (id, Instant::now()));
        }
        if let Some((id, since)) = self.hover_candidate {
            if self.preview_active != Some(id) {
                if since.elapsed() >= Duration::from_millis(400) {
                    self.last_hover_delay_ms = Some(since.elapsed().as_secs_f64() * 1000.);
                    let record = self
                        .prepared
                        .as_ref()
                        .and_then(|p| p.lookup.get(&id).map(|i| p.snapshot.files[*i].clone()));
                    if let Some(record) = record {
                        if let Err(error) = self.activate_video(&record, false) {
                            self.status = error;
                            self.hover_candidate =
                                Some((id, Instant::now() + Duration::from_secs(2)));
                        }
                        ctx.request_repaint();
                    }
                } else {
                    ctx.request_repaint_after(
                        Duration::from_millis(400).saturating_sub(since.elapsed()),
                    );
                }
            }
        }
        if let Some(idle) = self.preview_idle {
            if idle.elapsed() >= Duration::from_secs(2) {
                self.remember_position();
                if let Some(p) = &self.player {
                    let _ = p.lock().unwrap().unload();
                }
                self.player_media = None;
                self.seeks.bind(None);
                self.seek_flight = None;
                self.resume_pending = None;
                self.resume_starting = false;
                self.restore_flight = false;
                self.preview_idle = None;
            } else {
                ctx.request_repaint_after(Duration::from_secs(2) - idle.elapsed());
            }
        }
    }

    pub(super) fn raw_controls(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.workspace_shortcuts(ctx, raw);
        if raw.focused && !self.was_focused && self.watch_root.is_some() {
            self.refresh_pending = true;
        }
        self.was_focused = raw.focused;
        self.routed_zoom = 0.;
        let mut pointer = ctx.input(|i| i.pointer.hover_pos());
        let blocked = self.workspace.is_open()
            || self.features.cleanup_blocks_input()
            || !raw.focused
            || ctx.text_edit_focused()
            || egui::Popup::is_any_open(ctx)
            || self.rotation.is_some()
            || self.rotation_message.is_some()
            || self.permanent_prompt.is_some()
            || !self.trash_failures.is_empty();
        if !raw.focused {
            self.key_latch.clear();
        }
        if blocked {
            self.input_router.reset();
        }
        let events = std::mem::take(&mut raw.events);
        for mut event in events {
            if self.detail.is_none()
                && matches!(
                    &event,
                    egui::Event::MouseWheel { .. }
                        | egui::Event::PointerButton { pressed: true, .. }
                        | egui::Event::Key { pressed: true, .. }
                )
            {
                self.folder_location_anchor = None;
            }
            if matches!(event, egui::Event::WindowFocused(false)) {
                self.key_latch.clear();
            }
            let first_key = if let egui::Event::Key {
                key,
                pressed,
                repeat,
                ..
            } = &event
            {
                self.key_latch.transition(*key as u32, *pressed, *repeat)
            } else {
                true
            };
            if let egui::Event::PointerMoved(position) = &event {
                pointer = Some(*position);
            }
            if let egui::Event::PointerButton { pos, .. } = &event {
                pointer = Some(*pos);
            }
            if matches!(&event, egui::Event::PointerGone) {
                pointer = None;
            }
            // egui-winit translates actual Ctrl+C / Ctrl+X to semantic events.
            // Leave text editors and dialogs to handle their own clipboard.
            if !blocked
                && matches!(event, egui::Event::Copy | egui::Event::Cut)
                && (!self.selection.is_empty() || self.detail.is_some())
            {
                self.copy_selected_files(matches!(event, egui::Event::Cut));
                continue;
            }
            if !blocked && self.detail.is_none() {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = &event
                {
                    if modifiers.ctrl || modifiers.command {
                        if *key == egui::Key::A {
                            if first_key {
                                self.select_all();
                            }
                            continue;
                        }
                        if matches!(*key, egui::Key::C | egui::Key::X) && !self.selection.is_empty()
                        {
                            if first_key {
                                self.copy_selected_files(*key == egui::Key::X);
                            }
                            continue;
                        }
                    }
                    if *key == egui::Key::Escape && !self.selection.is_empty() {
                        if first_key {
                            self.selection.clear();
                        }
                        continue;
                    }
                }
            }
            if let egui::Event::Key {
                key: egui::Key::Delete,
                pressed: true,
                ..
            } = &event
            {
                if !blocked {
                    if first_key {
                        if self.detail.is_none() && !self.selected_folders.is_empty() {
                            if let Some(path) = self.selected_folders.iter().next().cloned() {
                                self.schedule_folder_delete(&path);
                            }
                        } else {
                            self.begin_trash();
                        }
                    }
                    continue;
                }
            }
            let image_detail = self.detail.as_ref().is_some_and(|viewer| {
                self.prepared
                    .as_ref()
                    .and_then(|p| {
                        p.lookup
                            .get(&viewer.id)
                            .map(|i| p.snapshot.files[*i].file.kind == MediaKind::Image)
                    })
                    .unwrap_or(false)
            });
            let hit = if self.detail.is_some()
                && self
                    .detail_layout
                    .strip
                    .is_some_and(|r| pointer.is_some_and(|p| r.contains(p)))
            {
                Hit::Filmstrip
            } else if self
                .control_regions
                .iter()
                .any(|rect| pointer.is_some_and(|p| rect.contains(p)))
                || self
                    .video_exclusion
                    .is_some_and(|r| pointer.is_some_and(|p| r.contains(p)))
            {
                Hit::Control
            } else if let Some((id, rect)) = self.video_region.filter(|(id, rect)| {
                pointer.is_some_and(|p| rect.contains(p))
                    && (self.detail.as_ref().is_some_and(|d| d.id == *id)
                        || self.preview_active == Some(*id))
            }) {
                let _ = rect;
                Hit::Video(id)
            } else if self
                .image_region
                .is_some_and(|r| pointer.is_some_and(|p| r.contains(p)))
            {
                Hit::Image
            } else {
                Hit::Other
            };
            let mut context = InputContext {
                detail: self.detail.is_some(),
                hit: if image_detail && hit == Hit::Other {
                    Hit::Image
                } else {
                    hit
                },
                blocked,
                shift: ctx.input(|i| i.modifiers.shift),
            };
            self.input_router.update_context(context);
            let input = match &event {
                egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } if delta.y != 0. => {
                    context.shift = modifiers.shift;
                    Some(Input::Wheel {
                        delta: if *unit == egui::MouseWheelUnit::Page {
                            delta.y * 100.
                        } else {
                            delta.y
                        },
                        pixels: *unit != egui::MouseWheelUnit::Line,
                    })
                }
                egui::Event::PointerButton {
                    button: egui::PointerButton::Extra1,
                    pressed: true,
                    ..
                } => Some(Input::Back),
                egui::Event::PointerButton {
                    button: egui::PointerButton::Extra2,
                    pressed: true,
                    ..
                } => Some(Input::Forward),
                egui::Event::Key {
                    key,
                    modifiers,
                    pressed: true,
                    ..
                } if self.detail.is_some()
                    && !modifiers.any()
                    && self
                        .shortcuts
                        .next
                        .matches(*key, media_browser::shortcuts::numpad_down) =>
                {
                    Some(Input::Next)
                }
                egui::Event::Key {
                    key,
                    modifiers,
                    pressed: true,
                    ..
                } if self.detail.is_some()
                    && !modifiers.any()
                    && self
                        .shortcuts
                        .previous
                        .matches(*key, media_browser::shortcuts::numpad_down) =>
                {
                    Some(Input::Previous)
                }
                egui::Event::Key {
                    key, pressed: true, ..
                } => match key {
                    egui::Key::ArrowLeft => Some(Input::Previous),
                    egui::Key::ArrowRight => Some(Input::Next),
                    egui::Key::Space => Some(Input::TogglePause),
                    egui::Key::F11 => Some(Input::Fullscreen),
                    egui::Key::Escape => Some(Input::Escape),
                    _ => None,
                },
                _ => None,
            };
            let Some(input) = input else {
                raw.events.push(event);
                continue;
            };
            if matches!(input, Input::Previous | Input::Next | Input::TogglePause)
                && self.control_focused
            {
                context.blocked = true;
            }
            if !first_key
                && matches!(
                    input,
                    Input::Previous
                        | Input::Next
                        | Input::TogglePause
                        | Input::Fullscreen
                        | Input::Escape
                )
            {
                if context.blocked {
                    raw.events.push(event);
                }
                continue;
            }
            let action = self.input_router.route(context, input);
            match action {
                Action::Pass => {
                    if !context.detail
                        && !context.blocked
                        && pointer
                            .is_some_and(|p| self.list_viewport.is_some_and(|r| r.contains(p)))
                        && !pointer.is_some_and(|p| {
                            self.sidebar_overlay_rect.is_some_and(|r| r.contains(p))
                        })
                    {
                        if let egui::Event::MouseWheel { delta, .. } = &event {
                            if delta.y != 0. {
                                self.sticky_jump_up = delta.y > 0.;
                            }
                        }
                    }
                    if !context.detail && !context.blocked && context.shift {
                        if let egui::Event::MouseWheel { modifiers, .. } = &mut event {
                            // egui interprets Shift-wheel as horizontal scrolling;
                            // our specified list escape gesture is vertical paging.
                            modifiers.shift = false;
                        }
                    }
                    raw.events.push(event);
                }
                Action::Consume => {}
                Action::Zoom(delta) => self.routed_zoom += delta,
                Action::Seek { id, seconds } => {
                    if self.player_media == Some(id) {
                        self.request_seek(seconds, false);
                    }
                }
                Action::History { forward } => self.history_move(forward),
                Action::Previous | Action::Next => {
                    if let (Some(detail), Some(prepared)) = (&self.detail, &self.prepared) {
                        let delta = if matches!(action, Action::Next) {
                            1
                        } else {
                            -1
                        };
                        if let Some(id) =
                            self.detail_target(prepared, detail.id, delta, false, false)
                        {
                            self.open_detail(id);
                        }
                    }
                }
                Action::Navigate(delta) => {
                    if let (Some(detail), Some(prepared)) = (&self.detail, &self.prepared) {
                        if let Some(id) =
                            self.detail_target(prepared, detail.id, delta, false, true)
                        {
                            if id != detail.id {
                                self.open_detail(id);
                            }
                        }
                    }
                }
                Action::TogglePause => self.toggle_detail_pause(),
                Action::Fullscreen => {
                    let video = self.detail.as_ref().is_some_and(|d| self.prepared.as_ref()
                        .and_then(|p| p.lookup.get(&d.id).map(|i| p.snapshot.files[*i].file.kind)) == Some(MediaKind::Video));
                    if video { self.toggle_video_fullscreen(); }
                    else {
                        self.fullscreen = !self.fullscreen;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
                    }
                }
                Action::Escape => {
                    if self.video_fullscreen.is_some() {
                        self.exit_video_fullscreen();
                    } else if self.fullscreen {
                        self.fullscreen = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                    } else {
                        self.close_detail();
                    }
                }
            }
        }
    }
}
