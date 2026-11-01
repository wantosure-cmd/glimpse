//! Controlled raw events go through the real application hook/widgets/player.
//! This does NOT move the OS cursor or prove physical mouse-driver behavior.
use super::*;
pub(super) struct InputSmoke {
    started: Instant,
    at: Instant,
    phase: u8,
    first: Option<MediaId>,
    bad: Option<MediaId>,
    root: Option<PathBuf>,
    pointer: Option<Pos2>,
    events: Vec<egui::Event>,
    rows: Vec<serde_json::Value>,
    focused: bool,
}
impl InputSmoke {
    pub(super) fn new() -> Self {
        Self {
            started: Instant::now(),
            at: Instant::now(),
            phase: 0,
            first: None,
            bad: None,
            root: None,
            pointer: None,
            events: Vec::new(),
            rows: Vec::new(),
            focused: true,
        }
    }
    pub(super) fn inject(&mut self, _b: &Browser, raw: &mut egui::RawInput) {
        raw.focused = self.focused; // Controlled signal, not actual OS focus proof.
        if let Some(point) = self.pointer {
            raw.events.push(egui::Event::PointerMoved(point));
        } else {
            raw.events.push(egui::Event::PointerGone);
        }
        raw.events.append(&mut self.events);
    }
    fn side(&mut self, button: egui::PointerButton) {
        for pressed in [true, false] {
            self.events.push(egui::Event::PointerButton {
                pos: self.pointer.unwrap_or(Pos2::ZERO),
                button,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }
    fn wheel(&mut self, delta: f32, shift: bool) {
        self.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0., delta),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: egui::Modifiers {
                shift,
                ..Default::default()
            },
        });
    }
    fn repeated_space(&mut self) {
        for pressed in [true, true, false] {
            self.events.push(egui::Event::Key {
                key: egui::Key::Space,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }
    fn capture(&mut self, b: &Browser, phase: &str) {
        let rect = |r: Rect| [r.left(), r.top(), r.right(), r.bottom()];
        let player=b.player.as_ref().map(|p|{let p=p.lock().unwrap();serde_json::json!({"ready":p.ready(),"time":p.number("time-pos"),"pause":p.text("pause"),"mute":p.text("mute"),"volume":p.number("volume"),"source":p.source.as_ref().map(|s|s.display().to_string()),"error":p.error(),"eof":p.text("eof-reached"),"loop":p.text("loop-file")})});
        let render_timing = b
            .player
            .as_ref()
            .map(|p| p.lock().unwrap().render_timing_diagnostic());
        self.rows.push(serde_json::json!({"phase":phase,"detail_canvas":b.detail_layout.canvas.map(rect),"detail_header":b.detail_layout.header.map(rect),"media_rect":b.video_region.map(|(_,r)|rect(r)),"render_timing":render_timing,"end_action":b.video_end_action,"app_elapsed_ms":b.started.elapsed().as_secs_f64()*1000.,"elapsed_ms":self.started.elapsed().as_secs_f64()*1000.,
            "player":player,"detail":b.detail.as_ref().map(|d|d.id),"preview":b.preview_active,"pending":b.seeks.target(),
                "root":b.snapshot.as_ref().map(|s|s.root.display().to_string()),"scroll":b.scroll_y,
            "hover_delay_ms":b.last_hover_delay_ms,"muted_preference":b.muted,"volume_preference":b.volume,"playback_errors":b.playback_errors.len(),
            "sidebar_open":b.sidebar_open,"hover_candidate":b.hover_candidate.map(|(id,_)|id),"hovered_video":b.hovered_video,"pointer":self.pointer.map(|p|[p.x,p.y]),"text_focused":b.ctx.text_edit_focused(),"selection_count":b.selection.len(),"preview_controls":b.preview_controls_rect.map(|r|[r.left(),r.top(),r.right(),r.bottom()]),"popup":egui::Popup::is_any_open(&b.ctx)}));
    }
    fn ready(b: &Browser) -> bool {
        b.resume_pending.is_none()
            && !b.restore_flight
            && b.player.as_ref().is_some_and(|p| p.lock().unwrap().ready())
    }
    fn pause(b: &Browser) {
        if let Some(p) = &b.player {
            let _ = p.lock().unwrap().set("pause", "yes");
        }
    }
    fn next(&mut self) {
        self.phase += 1;
        self.at = Instant::now();
    }
    fn aim(&mut self, b: &Browser) -> bool {
        let Some((rect, _)) = self.first.and_then(|id| b.visible_videos.get(&id)) else {
            return false;
        };
        if rect.width() < 20. || rect.height() < 20. {
            return false;
        }
        // Aim above the controls, even when the compact card is short.
        self.pointer = Some(egui::pos2(rect.center().x, rect.top() + 44.));
        true
    }
    pub(super) fn tick(&mut self, b: &mut Browser, ctx: &egui::Context) {
        ctx.request_repaint_after(Duration::from_millis(20));
        if self.started.elapsed() > Duration::from_secs(60) {
            self.capture(b, &format!("timeout_phase_{}", self.phase));
            self.finish(ctx, Some("Input diagnostic timeout"));
            return;
        }
        let elapsed = self.at.elapsed();
        match self.phase {
            0 => {
                let Some(p) = &b.prepared else {
                    return;
                };
                if !p.snapshot.finished {
                    return;
                }
                self.first = p
                    .snapshot
                    .files
                    .iter()
                    .find(|r| {
                        r.file.path.file_name().is_some_and(|s| s == "01.mp4") && r.width.is_some()
                    })
                    .map(|r| r.id);
                self.bad = p
                    .snapshot
                    .files
                    .iter()
                    .find(|r| r.file.path.file_name().is_some_and(|s| s == "99损坏.mp4"))
                    .map(|r| r.id);
                if self.first.is_none() {
                    return;
                }
                self.root = Some(p.snapshot.root.clone());
                b.scroll_override = Some(20.);
                self.next();
            }
            1 => {
                if elapsed < Duration::from_millis(100) || !self.aim(b) {
                    return;
                }
                self.next();
            }
            2 => {
                if elapsed < Duration::from_millis(200) {
                    return;
                }
                self.capture(b, "before_400ms");
                self.next();
            }
            3 => {
                if b.preview_active != self.first || !Self::ready(b) {
                    return;
                }
                self.capture(b, "preview_ready");
                Self::pause(b);
                b.snapshot_image(ctx, "p3-input-preview.png");
                self.next();
            }
            4 => {
                if elapsed < Duration::from_millis(250) {
                    return;
                }
                self.capture(b, "wheel_before");
                self.wheel(1., false);
                self.next();
            }
            5 => {
                if elapsed < Duration::from_millis(600) {
                    return;
                }
                self.capture(b, "wheel_after");
                b.snapshot_image(ctx, "p3-input-seek-notice.png");
                self.side(egui::PointerButton::Extra1);
                self.next();
            }
            6 => {
                if elapsed < Duration::from_millis(600) {
                    return;
                }
                self.capture(b, "side_after");
                self.wheel(-1., true);
                self.next();
            }
            7 => {
                if elapsed < Duration::from_millis(250) {
                    return;
                }
                self.capture(b, "shift_page_scroll");
                self.pointer = None;
                self.next();
            }
            8 => {
                if elapsed < Duration::from_millis(200) {
                    return;
                }
                self.capture(b, "left_paused");
                self.next();
            }
            9 => {
                if elapsed < Duration::from_millis(2200) {
                    return;
                }
                self.capture(b, "idle_unloaded");
                b.scroll_override = Some(20.);
                self.next();
            }
            10 => {
                if elapsed < Duration::from_millis(300) || !self.aim(b) {
                    return;
                }
                self.next();
            }
            11 => {
                if b.preview_active != self.first || !Self::ready(b) {
                    return;
                }
                self.capture(b, "preview_resumed");
                self.side(egui::PointerButton::Primary);
                self.next();
            }
            12 => {
                if elapsed < Duration::from_millis(500) {
                    return;
                }
                self.capture(b, "single_click_detail");
                Self::pause(b);
                // The top32pt is now a real title bar. Test actual canvas
                // letterboxing, not an obsolete screen-corner assumption.
                let Some(canvas) = b.detail_layout.canvas else {
                    return;
                };
                let Some((_, media)) = b.video_region else {
                    return;
                };
                let strips = [
                    Rect::from_min_max(canvas.min, egui::pos2(media.left(), canvas.bottom())),
                    Rect::from_min_max(egui::pos2(media.right(), canvas.top()), canvas.max),
                    Rect::from_min_max(canvas.min, egui::pos2(canvas.right(), media.top())),
                    Rect::from_min_max(egui::pos2(canvas.left(), media.bottom()), canvas.max),
                ];
                let Some(blank) = strips
                    .into_iter()
                    .filter(|r| r.width() > 0.5 && r.height() > 0.5)
                    .max_by(|a, b| a.area().total_cmp(&b.area()))
                else {
                    self.finish(
                        ctx,
                        Some("No canvas letterbox exists for blank-input diagnostic"),
                    );
                    return;
                };
                self.pointer = Some(blank.center());
                self.next();
            }
            13 => {
                if elapsed < Duration::from_millis(200) {
                    return;
                }
                self.capture(b, "detail_outside_before");
                self.side(egui::PointerButton::Extra1);
                self.next();
            }
            14 => {
                if elapsed < Duration::from_millis(300) {
                    return;
                }
                self.capture(b, "detail_outside_after");
                self.side(egui::PointerButton::Primary);
                self.next();
            }
            15 => {
                if elapsed < Duration::from_millis(300) {
                    return;
                }
                self.capture(b, "blank_closed_detail");
                b.navigate(self.root.as_ref().unwrap().join("子目录"), true);
                self.next();
            }
            16 => {
                if b.prepared
                    .as_ref()
                    .is_none_or(|p| p.snapshot.root == *self.root.as_ref().unwrap())
                {
                    return;
                }
                self.capture(b, "child_before_back");
                self.pointer = Some(egui::pos2(4., 4.));
                self.side(egui::PointerButton::Extra1);
                self.next();
            }
            17 => {
                if b.prepared
                    .as_ref()
                    .is_none_or(|p| p.snapshot.root != *self.root.as_ref().unwrap())
                {
                    return;
                }
                self.capture(b, "directory_back");
                let Some(id) = self.bad else {
                    self.finish(ctx, Some("Corrupt sample missing"));
                    return;
                };
                b.open_detail(id);
                self.next();
            }
            18 => {
                if !self
                    .bad
                    .is_some_and(|id| b.playback_errors.contains_key(&id))
                {
                    return;
                }
                self.capture(b, "invalid_video_error");
                self.next();
            }
            19 => {
                if elapsed < Duration::from_millis(500) {
                    return;
                }
                b.snapshot_image(ctx, "p3-input-invalid-video.png");
                self.next();
            }
            20 => {
                if elapsed < Duration::from_millis(200) {
                    return;
                }
                b.open_detail(self.first.unwrap());
                self.next();
            }
            21 => {
                if !Self::ready(b) || elapsed < Duration::from_millis(500) {
                    return;
                }
                self.capture(b, "recovered_valid_video");
                Self::pause(b);
                self.next();
            }
            22 => {
                if elapsed < Duration::from_millis(200) {
                    return;
                }
                let Some(rect) = b.control_regions.last() else {
                    return;
                };
                self.pointer = Some(rect.center());
                self.capture(b, "control_wheel_before");
                self.wheel(1., false);
                self.next();
            }
            23 => {
                if elapsed < Duration::from_millis(200) {
                    return;
                }
                self.capture(b, "control_wheel_after");
                self.pointer = b.video_region.map(|(_, rect)| rect.center());
                self.side(egui::PointerButton::Primary);
                self.next();
            }
            24 => {
                if elapsed < Duration::from_millis(200) {
                    return;
                }
                // Exercise manual Space replay with the supported pause-at-end policy.
                // The default Next/Replay/Pause policies have their own detail diagnostic.
                b.video_end_action = VideoEndAction::Pause;
                let duration = b
                    .player
                    .as_ref()
                    .and_then(|p| p.lock().unwrap().number("duration"))
                    .unwrap_or(65.);
                b.request_seek(duration, true);
                // mpv stays paused when seeking; play the last frame to enter EOF.
                if let Some(p) = &b.player {
                    let _ = p.lock().unwrap().set("pause", "no");
                }
                self.next();
            }
            25 => {
                if b.seeks.target().is_some()
                    || !b.player.as_ref().is_some_and(|p| {
                        p.lock().unwrap().text("eof-reached").as_deref() == Some("yes")
                    })
                {
                    return;
                }
                self.capture(b, "end_reached");
                self.repeated_space();
                self.next();
            }
            26 => {
                if elapsed < Duration::from_millis(500) {
                    return;
                }
                self.capture(b, "end_replayed");
                self.repeated_space();
                self.next();
            }
            27 => {
                if elapsed < Duration::from_millis(300) {
                    return;
                }
                self.capture(b, "repeat_space_paused");
                self.next();
            }
            28 => {
                let Some(p) = &b.prepared else {
                    return;
                };
                let Some(other) = p
                    .snapshot
                    .files
                    .iter()
                    .find(|r| r.file.path.file_name().is_some_and(|s| s == "02.avi"))
                    .map(|r| r.id)
                else {
                    return;
                };
                for _ in 0..8 {
                    b.open_detail(self.bad.unwrap());
                    b.open_detail(other);
                    b.open_detail(self.first.unwrap());
                }
                self.next();
            }
            29 => {
                if !Self::ready(b) || elapsed < Duration::from_millis(700) {
                    return;
                }
                self.capture(b, "rapid_switch_final");
                b.close_detail();
                // The earlier blank-area test deliberately visited the sidebar edge.
                // Move out and allow its documented auto-hide delay before re-hovering.
                self.pointer = None;
                self.next();
            }
            30 => {
                if elapsed < Duration::from_millis(600) {
                    return;
                }
                // Closing a modal restores the list scroll position on the next frame.
                // Aim from its current geometry instead of the previous modal frame.
                self.aim(b);
                if b.preview_active != self.first || !Self::ready(b) {
                    return;
                }
                self.capture(b, "focus_before");
                self.focused = false;
                self.next();
            }
            31 => {
                if elapsed < Duration::from_millis(200) {
                    return;
                }
                self.capture(b, "controlled_focus_lost");
                self.focused = true;
                self.pointer = None;
                self.next();
            }
            32 => {
                if elapsed < Duration::from_millis(200) {
                    return;
                }
                self.capture(b, "focus_restored_outside_video");
                self.next();
            }
            33 => {
                self.aim(b);
                if b.preview_active != self.first
                    || !Self::ready(b)
                    || b.preview_controls_rect.is_none()
                    || b.player
                        .as_ref()
                        .unwrap()
                        .lock()
                        .unwrap()
                        .text("mute")
                        .as_deref()
                        != Some("no")
                {
                    return;
                }
                self.capture(b, "list_loop_before");
                let duration = b
                    .player
                    .as_ref()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .number("duration")
                    .unwrap();
                b.request_seek(duration - 0.3, true);
                let _ = b
                    .player
                    .as_ref()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .set("pause", "no");
                self.next();
            }
            34 => {
                if elapsed < Duration::from_millis(1600) {
                    return;
                }
                self.capture(b, "list_loop_after");
                b.snapshot_image(ctx, "ui-list-playing.png");
                let Some(bar) = b.preview_seek_rect else {
                    return;
                };
                self.pointer = Some(egui::pos2(bar.left() + bar.width() * 0.65, bar.center().y));
                self.events.push(egui::Event::PointerButton {
                    pos: self.pointer.unwrap(),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
                self.next();
            }
            35 => {
                if elapsed < Duration::from_millis(600) {
                    return;
                }
                self.events.push(egui::Event::PointerButton {
                    pos: self.pointer.unwrap(),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                });
                self.capture(b, "list_progress_clicked");
                self.events.push(egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL,
                });
                self.events.push(egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL,
                });
                self.next();
            }
            36 => {
                if elapsed < Duration::from_millis(100) {
                    return;
                }
                self.capture(b, "multi_select_all");
                self.aim(b);
                self.side(egui::PointerButton::Secondary);
                self.next();
            }
            37 => {
                if elapsed < Duration::from_millis(200) {
                    return;
                }
                self.capture(b, "right_menu_multi_preserved");
                b.snapshot_image(ctx, "ui-right-menu.png");
                self.next();
            }
            38 => {
                if elapsed < Duration::from_millis(300) {
                    return;
                }
                self.finish(ctx, None);
            }
            _ => {
                self.finish(ctx, None);
            }
        }
    }
    fn finish(&self, ctx: &egui::Context, error: Option<&str>) {
        let value = serde_json::json!({"scope":"Synthetic raw events and controlled focus through actual egui/native player; NOT physical mouse/driver or real OS focus proof","error":error,"rows":self.rows});
        let _ = std::fs::write(
            "artifacts/p3-input-smoke.json",
            serde_json::to_vec_pretty(&value).unwrap(),
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
