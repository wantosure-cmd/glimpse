//! Opt-in regression using owned fixtures, native egui events and real FBO captures.
use super::*;
use presentation_ui::Mode;
pub(super) struct PresentationSmoke {
    pub caption: Option<Rect>,
    pub path: Option<Rect>,
    pub drag_requests: usize,
    output: PathBuf,
    phase: u8,
    at: Instant,
    started: Instant,
    events: Vec<egui::Event>,
    rows: Vec<serde_json::Value>,
    seen: Vec<MediaId>,
    point: Pos2,
    captured: bool,
}
impl PresentationSmoke {
    pub fn new(output: PathBuf) -> Self {
        Self {
            caption: None,
            path: None,
            drag_requests: 0,
            output,
            phase: 0,
            at: Instant::now(),
            started: Instant::now(),
            events: vec![],
            rows: vec![],
            seen: vec![],
            point: Pos2::ZERO,
            captured: false,
        }
    }
    pub fn inject(&mut self, raw: &mut egui::RawInput) {
        raw.focused = true;
        raw.events.append(&mut self.events);
    }
    fn pointer(&mut self, point: Pos2, pressed: bool) {
        self.events.push(egui::Event::PointerMoved(point));
        self.events.push(egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        });
    }
    fn click(&mut self, point: Pos2) {
        self.pointer(point, true);
        self.pointer(point, false);
    }
    fn space(&mut self) {
        for pressed in [true, false] {
            self.events.push(egui::Event::Key {
                key: egui::Key::Space,
                physical_key: Some(egui::Key::Space),
                pressed,
                repeat: false,
                modifiers: Default::default(),
            });
        }
    }
    fn capture(&mut self, b: &Browser, ctx: &egui::Context, name: &str) {
        self.rows.push(serde_json::json!({"name":name,"tiles":b.presentation.tiles.iter().map(|(id,r)|(id,[r.left(),r.top(),r.width(),r.height()])).collect::<Vec<_>>(),"texture_bytes":b.texture_bytes,"detail":b.detail.as_ref().map(|d|d.id),"playing":b.presentation.playing,"seen":self.seen,"drag_requests":self.drag_requests}));
        b.snapshot_image(ctx, &format!("presentation-{name}.png"));
    }
    fn finish(&mut self, ctx: &egui::Context, error: Option<&str>) {
        let report = serde_json::json!({"checks_pass":error.is_none(),"error":error,"phase":self.phase,"rows":self.rows,"seen":self.seen,"scope":"Native egui pointer/key injection and real rendered FBO. Caption drag request observed; physical OS window movement is not verified."});
        let _ = std::fs::write(&self.output, serde_json::to_vec_pretty(&report).unwrap());
        self.phase = 255;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    pub fn tick(&mut self, b: &mut Browser, ctx: &egui::Context) {
        if self.phase == 255 {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(20));
        if self.started.elapsed() > Duration::from_secs(70) {
            self.finish(ctx, Some("timeout"));
            return;
        }
        let Some(p) = b
            .prepared
            .clone()
            .filter(|p| p.snapshot.finished && p.view.sequence.len() >= 9)
        else {
            return;
        };
        if b.prepare_dirty || self.at.elapsed() < Duration::from_millis(120) {
            return;
        }
        let first = p.view.sequence[0];
        let video = p
            .view
            .sequence
            .iter()
            .find(|id| p.snapshot.files[p.lookup[id]].file.kind == MediaKind::Video)
            .copied()
            .unwrap();
        let mut fail = None;
        match self.phase {
            0 => {
                b.open_detail(first);
                b.set_presentation_mode(Mode::Group);
                b.presentation.playing = false;
                b.presentation.prefs.group_size = 2;
            }
            1..=4 => {
                if !b.presentation.ready || b.presentation.tiles.len() != self.phase as usize + 1 {
                    return;
                }
                if !self.captured {
                    self.capture(b, ctx, &format!("group-{}", self.phase + 1));
                    self.captured = true;
                    self.at = Instant::now();
                    return;
                }
                self.captured = false;
                b.presentation.prefs.group_size = (self.phase as usize + 2).min(5);
            }
            5 => {
                b.presentation.prefs.fill = false;
            }
            6 => {
                self.capture(b, ctx, "contain");
                self.point = b.presentation.tiles[0].1.center();
                self.click(self.point);
            }
            7 => self.click(self.point),
            8 => {
                if b.detail.is_some() {
                    fail = Some("group double click did not close");
                } else {
                    self.capture(b, ctx, "group-return");
                    b.open_detail(first);
                }
            }
            9 => {
                if self.at.elapsed() < Duration::from_millis(450) {
                    return;
                }
                if b.image_region.is_none() {
                    return;
                }
                self.point = b.image_region.unwrap().center();
                self.click(self.point);
            }
            10 => self.click(self.point),
            11 => {
                if b.detail.is_some() {
                    fail = Some("image double click did not close");
                } else {
                    b.open_detail(video);
                }
            }
            12 => {
                if self.at.elapsed() < Duration::from_millis(450) {
                    return;
                }
                if b.player_media != Some(video)
                    || b.player.as_ref().is_none_or(|p| !p.lock().unwrap().ready())
                    || b.video_region.is_none()
                {
                    return;
                }
                let _ = b
                    .player
                    .as_ref()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .set("pause", "no");
                self.point = b.video_region.unwrap().1.center();
                self.click(self.point);
            }
            13 => {
                if b.player
                    .as_ref()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .text("pause")
                    .as_deref()
                    != Some("yes")
                {
                    fail = Some("video single click did not pause");
                }
                self.click(self.point);
            }
            14 => {
                if b.detail.is_some() {
                    fail = Some("video double click did not close");
                } else {
                    b.open_detail(first);
                    b.presentation.prefs.seconds = 1.;
                    b.presentation.prefs.repeat = false;
                    b.set_presentation_mode(Mode::Sequential);
                    self.space();
                }
            }
            15 => {
                if b.presentation.playing {
                    fail = Some("space did not pause slideshow");
                } else {
                    self.capture(b, ctx, "paused");
                }
            }
            16 => {
                if self.at.elapsed() < Duration::from_millis(1400) {
                    return;
                }
                if b.detail.as_ref().map(|d| d.id) != Some(first) {
                    fail = Some("paused slideshow advanced");
                }
                self.space();
            }
            17 => {
                if let Some(d) = &b.detail {
                    if self.seen.last() != Some(&d.id) {
                        self.seen.push(d.id);
                    }
                }
                if b.presentation.playing {
                    return;
                }
                if self.seen != p.view.sequence {
                    fail = Some("mixed slideshow order/EOF transition wrong");
                } else {
                    self.capture(b, ctx, "mixed-finished");
                    b.close_detail();
                }
            }
            18 => {
                self.seen.clear();
                b.open_detail(first);
                b.presentation.prefs.group_size = 3;
                b.set_presentation_mode(Mode::Group);
            }
            19 => {
                if let Some(d) = &b.detail {
                    for id in b.presentation.ids(d.id) {
                        if !self.seen.contains(&id) {
                            self.seen.push(id);
                        }
                    }
                }
                if b.presentation.playing {
                    return;
                }
                if self.seen != p.view.sequence {
                    fail = Some("group slideshow lost/reordered media");
                } else {
                    self.capture(b, ctx, "group-finished");
                    b.close_detail();
                }
            }
            20 => {
                if b.presentation.mode != Mode::Manual || b.presentation.playing {
                    fail = Some("closing detail did not stop slideshow");
                }
                self.point = egui::pos2(
                    self.caption.unwrap().right() - 180.,
                    self.caption.unwrap().center().y,
                );
                self.pointer(self.point, true);
            }
            21 => self
                .events
                .push(egui::Event::PointerMoved(self.point + egui::vec2(-40., 0.))),
            22 => self.pointer(self.point + egui::vec2(-40., 0.), false),
            23 => {
                if self.drag_requests == 0 {
                    fail = Some("unused caption did not request window drag");
                } else {
                    self.click(self.path.unwrap().center());
                }
            }
            24 => {
                if !b.path_editing {
                    fail = Some("caption intercepted path control");
                } else {
                    self.capture(b, ctx, "caption-controls");
                    self.finish(ctx, None);
                    return;
                }
            }
            _ => {}
        }
        if let Some(error) = fail {
            self.finish(ctx, Some(error));
            return;
        }
        self.phase += 1;
        self.at = Instant::now();
    }
}
