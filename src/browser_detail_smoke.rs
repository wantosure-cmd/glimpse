//! Opt-in controlled egui events and real libmpv EOF; not physical mouse proof.
use super::*;
pub(super) struct DetailSmoke {
    output: PathBuf,
    phase: u8,
    at: Instant,
    started: Instant,
    ids: Vec<MediaId>,
    events: Vec<egui::Event>,
    pointer: Option<Pos2>,
    rows: Vec<serde_json::Value>,
    release_next: Option<Pos2>,
    scrub_attempts: u8,
}
impl DetailSmoke {
    pub(super) fn new(output: PathBuf) -> Self {
        Self {
            output,
            phase: 0,
            at: Instant::now(),
            started: Instant::now(),
            ids: vec![],
            events: vec![],
            pointer: None,
            rows: vec![],
            release_next: None,
            scrub_attempts: 0,
        }
    }
    pub(super) fn inject(&mut self, raw: &mut egui::RawInput) {
        raw.focused = true; // Controlled signal, not OS focus evidence.
        raw.events.push(
            self.pointer
                .map(egui::Event::PointerMoved)
                .unwrap_or(egui::Event::PointerGone),
        );
        raw.events.append(&mut self.events);
        if let Some(pos) = self.release_next.take() {
            self.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            });
        }
    }
    fn click(&mut self, pos: Pos2, button: egui::PointerButton) {
        self.pointer = Some(pos);
        for pressed in [true, false] {
            self.events.push(egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: Default::default(),
            });
        }
    }
    fn scrub(&mut self, pos: Pos2) {
        self.pointer = Some(pos);
        self.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        });
        self.release_next = Some(pos);
        self.scrub_attempts += 1;
    }
    pub(super) fn hidden_tick(&mut self, ctx: &egui::Context) {
        if self.phase != 255 && self.started.elapsed() > Duration::from_secs(70) {
            self.finish(ctx, Some("test window minimized or occluded"));
        }
    }
    fn advance(&mut self) {
        self.phase += 1;
        self.at = Instant::now();
    }
    fn capture(&mut self, b: &Browser, label: &str) {
        let p = b.player.as_ref().map(|p| { let p=p.lock().unwrap(); serde_json::json!({"ready":p.ready(),"time":p.number("time-pos"),"duration":p.number("duration"),"pause":p.text("pause"),"eof":p.text("eof-reached"),"error":p.error(),"sub_text":p.text("sub-text"),"sid":p.text("sid"),"sub_size":p.number("sub-font-size"),"sub_pos":p.number("sub-pos"),"sub_color":p.text("sub-color"),"sub_border_style":p.text("sub-border-style"),"sub_back_color":p.text("sub-back-color")}) });
        let rect = |r: Rect| [r.left(), r.top(), r.width(), r.height()];
        self.rows.push(serde_json::json!({"phase":label,"detail":b.detail.as_ref().map(|v|v.id),"player":p,"canvas":b.detail_layout.canvas.map(rect),"header":b.detail_layout.header.map(rect),"current_time":b.detail_layout.current_time.map(rect),"total_time":b.detail_layout.total_time.map(rect),"next_image":b.detail_layout.next_image,"next_image_ready":b.detail_layout.next_image_ready,"video_prefetch":b.video_prefetch.status(),"seek":b.detail_layout.seek.map(rect),"transport":b.detail_layout.transport.map(rect),"subtitle_notice":b.subtitle_notice,"subtitle_style":b.subtitles,"strip":b.detail_layout.strip.map(rect),"thumbnails":b.detail_layout.thumbnails.len(),"arrows":b.detail_layout.arrows.len(),"current_thumbnail_x":b.detail.as_ref().and_then(|v| b.detail_layout.thumbnails.iter().find(|(id,_)|*id==v.id).map(|(_,r)|r.center().x)),"popup":egui::Popup::is_any_open(&b.ctx)}));
    }
    fn near_end(b: &mut Browser) -> bool {
        let Some(player) = b.player.clone() else {
            return false;
        };
        let p = player.lock().unwrap();
        if !p.ready() || b.resume_pending.is_some() || b.seek_flight.is_some() {
            return false;
        }
        let Some(duration) = p.number("duration") else {
            return false;
        };
        if let Some(v) = &mut b.detail {
            v.end_handled = false;
        }
        p.seek((duration - 0.5).max(0.)).is_ok() && p.set("pause", "no").is_ok()
    }
    fn finish(&mut self, ctx: &egui::Context, error: Option<&str>) {
        if let Some(parent) = self.output.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let result = serde_json::json!({"error":error,"phase":self.phase,"ids":self.ids,"rows":self.rows,"input":"controlled egui raw events, not OS input"});
        let _ = std::fs::write(&self.output, serde_json::to_vec_pretty(&result).unwrap());
        self.phase = 255;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    pub(super) fn tick(&mut self, b: &mut Browser, ctx: &egui::Context) {
        if self.phase == 255 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(30));
        if self.started.elapsed() > Duration::from_secs(70) {
            self.capture(b, "timeout");
            self.finish(ctx, Some("timeout"));
            return;
        }
        if self.at.elapsed() < Duration::from_millis(500) {
            return;
        }
        let current = b.detail.as_ref().map(|v| v.id);
        match self.phase {
            0 => {
                let Some(p) = &b.prepared else {
                    return;
                };
                if !p.snapshot.finished {
                    return;
                }
                for name in ["01.jpg", "02.mp4", "03.jpg", "04.mov", "05.avi"] {
                    let Some(r) = p
                        .snapshot
                        .files
                        .iter()
                        .find(|r| r.file.path.file_name().is_some_and(|n| n == name))
                    else {
                        return;
                    };
                    self.ids.push(r.id);
                }
                b.open_detail(self.ids[0]);
                self.advance();
            }
            1 => {
                self.capture(b, "image_hidden_arrows");
                b.snapshot_image(ctx, "detail-redesign-image.png");
                let Some((_, r)) = b
                    .detail_layout
                    .thumbnails
                    .iter()
                    .find(|(id, _)| *id == self.ids[1])
                else {
                    return;
                };
                self.click(r.center(), egui::PointerButton::Primary);
                self.advance();
            }
            2 => {
                if current != Some(self.ids[1])
                    || !b
                        .player
                        .as_ref()
                        .is_some_and(|p| p.lock().unwrap().number("duration").is_some())
                {
                    return;
                }
                self.capture(b, "thumbnail_open_video");
                b.snapshot_image(ctx, "detail-redesign-video.png");
                if Self::near_end(b) {
                    self.advance();
                }
            }
            3 => {
                if current != Some(self.ids[3]) {
                    return;
                }
                self.capture(b, "eof_next_skips_image");
                b.video_end_action = VideoEndAction::Replay;
                if Self::near_end(b) {
                    self.advance();
                }
            }
            4 => {
                let replayed = b.player.as_ref().is_some_and(|p| {
                    let p = p.lock().unwrap();
                    p.number("time-pos").is_some_and(|t| t < 3.)
                        && p.text("pause").as_deref() == Some("no")
                });
                if !replayed || self.at.elapsed() < Duration::from_millis(1200) {
                    return;
                }
                self.capture(b, "eof_replay");
                b.video_end_action = VideoEndAction::Pause;
                if Self::near_end(b) {
                    self.advance();
                }
            }
            5 => {
                if !b.player.as_ref().is_some_and(|p| {
                    let p = p.lock().unwrap();
                    p.text("eof-reached").as_deref() == Some("yes")
                        && p.text("pause").as_deref() == Some("yes")
                }) {
                    return;
                }
                self.capture(b, "eof_pause");
                b.video_end_action = VideoEndAction::Next;
                b.open_detail(self.ids[4]);
                self.advance();
            }
            6 => {
                if Self::near_end(b) {
                    self.advance();
                }
            }
            7 => {
                if !b.player.as_ref().is_some_and(|p| {
                    let p = p.lock().unwrap();
                    p.text("eof-reached").as_deref() == Some("yes")
                        && p.text("pause").as_deref() == Some("yes")
                }) {
                    return;
                }
                self.capture(b, "last_video_stops");
                let Some(r) = b.detail_layout.canvas else {
                    return;
                };
                self.click(r.center(), egui::PointerButton::Secondary);
                self.advance();
            }
            8 => {
                self.capture(b, "video_context_menu");
                b.snapshot_image(ctx, "detail-redesign-menu.png");
                for pressed in [true, false] {
                    self.events.push(egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: Default::default(),
                    });
                }
                self.advance();
            }
            9 => {
                b.open_detail(self.ids[0]);
                self.pointer = b.detail_layout.canvas.map(|r| r.center());
                self.advance();
            }
            10 => {
                self.capture(b, "image_hover_arrows");
                b.snapshot_image(ctx, "detail-redesign-hover.png");
                let Some(r) = b.detail_layout.arrows.get(1) else {
                    return;
                };
                self.click(r.center(), egui::PointerButton::Primary);
                self.advance();
            }
            11 => {
                if current != Some(self.ids[1]) {
                    return;
                }
                self.capture(b, "arrow_open_video");
                if let Some(p) = &b.player {
                    let _ = p.lock().unwrap().set("pause", "yes");
                }
                let Some(r) = b.detail_layout.seek else {
                    return;
                };
                self.scrub(r.center());
                self.advance();
            }
            12 => {
                let scrubbed = b.player.as_ref().is_some_and(|p| {
                    let p = p.lock().unwrap();
                    p.number("time-pos")
                        .is_some_and(|t| (30.0..35.0).contains(&t))
                        && p.text("pause").as_deref() == Some("yes")
                });
                if !scrubbed || b.seeks.target().is_some() {
                    if self.at.elapsed() > Duration::from_secs(2) && self.scrub_attempts < 3 {
                        if let Some(r) = b.detail_layout.seek {
                            self.scrub(r.center());
                            self.at = Instant::now();
                        }
                    }
                    return;
                }
                self.capture(b, "wide_slider_paused_seek");
                let Some(r) = b.detail_layout.canvas else {
                    return;
                };
                self.click(r.min + egui::vec2(2., 2.), egui::PointerButton::Primary);
                self.advance();
            }
            13 => {
                self.capture(b, "video_blank_returns");
                b.open_detail(self.ids[0]);
                self.advance();
            }
            14 => {
                let Some(r) = b.detail_layout.canvas else {
                    return;
                };
                self.click(r.center(), egui::PointerButton::Secondary);
                self.advance();
            }
            15 => {
                self.capture(b, "image_context_menu");
                b.snapshot_image(ctx, "detail-redesign-image-menu.png");
                for pressed in [true, false] {
                    self.events.push(egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: Default::default(),
                    });
                }
                self.advance();
            }
            16 => {
                let Some(id) = b
                    .prepared
                    .as_ref()
                    .and_then(|p| p.view.sequence.last())
                    .copied()
                else {
                    return;
                };
                b.open_detail(id);
                self.pointer = None;
                self.advance();
            }
            17 => {
                self.capture(b, "last_thumbnail_centered");
                b.snapshot_image(ctx, "detail-redesign-last-thumbnail.png");
                if std::env::args().any(|a| a == "--subtitle-smoke") {
                    b.open_detail(self.ids[1]);
                    self.advance();
                } else {
                    self.phase = 26;
                    self.at = Instant::now();
                }
            }
            18 => {
                let loaded = b.player.as_ref().is_some_and(|p| {
                    p.lock()
                        .unwrap()
                        .text("sub-text")
                        .is_some_and(|t| t.contains("AUTO SRT"))
                });
                if !loaded {
                    return;
                }
                self.capture(b, "subtitle_auto");
                b.snapshot_image(ctx, "subtitle-auto.png");
                let Some((_, rect)) = b.video_region else {
                    return;
                };
                self.click(rect.center(), egui::PointerButton::Primary);
                self.advance();
            }
            19 => {
                if !b
                    .player
                    .as_ref()
                    .is_some_and(|p| p.lock().unwrap().text("pause").as_deref() == Some("yes"))
                {
                    return;
                }
                self.capture(b, "video_click_paused");
                b.subtitles = media_browser::subtitles::SubtitleStyle::preset(1);
                b.subtitles.size = 48.;
                b.subtitles.position = 65.;
                b.apply_subtitle_style();
                self.advance();
            }
            20 => {
                self.capture(b, "subtitle_gold");
                b.snapshot_image(ctx, "subtitle-gold.png");
                let path = b
                    .player
                    .as_ref()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .source
                    .as_ref()
                    .unwrap()
                    .with_file_name("manual.srt");
                b.player
                    .as_ref()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .add_subtitle(&path)
                    .unwrap();
                self.advance();
            }
            21 => {
                if !b.player.as_ref().is_some_and(|p| {
                    p.lock()
                        .unwrap()
                        .text("sub-text")
                        .is_some_and(|t| t.contains("MANUAL SRT"))
                }) {
                    return;
                }
                self.capture(b, "subtitle_manual");
                b.subtitles = media_browser::subtitles::SubtitleStyle::preset(2);
                b.apply_subtitle_style();
                self.advance();
            }
            22 => {
                self.capture(b, "subtitle_box");
                b.snapshot_image(ctx, "subtitle-box.png");
                let path = b
                    .player
                    .as_ref()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .source
                    .as_ref()
                    .unwrap()
                    .with_file_name("does-not-exist.srt");
                b.player
                    .as_ref()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .add_subtitle(&path)
                    .unwrap();
                self.advance();
            }
            23 => {
                if !b
                    .subtitle_notice
                    .as_ref()
                    .is_some_and(|s| s.contains("失败"))
                {
                    return;
                }
                self.capture(b, "subtitle_failed_keeps_video");
                let Some((_, rect)) = b.video_region else {
                    return;
                };
                self.click(rect.center(), egui::PointerButton::Primary);
                self.advance();
            }
            24 => {
                if !b
                    .player
                    .as_ref()
                    .is_some_and(|p| p.lock().unwrap().text("pause").as_deref() == Some("no"))
                {
                    return;
                }
                self.capture(b, "video_click_resumed");
                b.open_detail(self.ids[3]);
                self.advance();
            }
            25 => {
                let ready = b.player.as_ref().is_some_and(|p| p.lock().unwrap().ready());
                if !ready {
                    return;
                }
                self.capture(b, "subtitle_no_leak_next_video");
                self.advance();
            }
            26 => {
                let Some(rect) = b.detail_layout.header else {
                    return;
                };
                self.click(
                    rect.min + egui::vec2(100., 15.),
                    egui::PointerButton::Secondary,
                );
                self.advance();
            }
            27 => {
                self.capture(b, "header_context_menu");
                b.snapshot_image(ctx, "detail-overlay-menu.png");
                for pressed in [true, false] {
                    self.events.push(egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: Default::default(),
                    });
                }
                self.advance();
            }
            28 => {
                let Some(rect) = b.detail_layout.header else {
                    return;
                };
                self.click(
                    rect.min + egui::vec2(25., 18.),
                    egui::PointerButton::Primary,
                );
                self.advance();
            }
            29 => {
                self.capture(b, "overlay_back_returns");
                self.finish(ctx, None);
            }
            _ => {}
        }
    }
}
