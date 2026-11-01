//! Opt-in read-only generated-fixture test; no physical OS input.
use super::*;
pub(super) struct MediaJumpSmoke {
    output: PathBuf,
    started: Instant,
    at: Instant,
    phase: u8,
    root: Option<PathBuf>,
    events: Vec<egui::Event>,
    pointer: Option<Pos2>,
    rows: Vec<serde_json::Value>,
    old_scroll: f32,
}
impl MediaJumpSmoke {
    pub(super) fn new(output: PathBuf) -> Self {
        Self {
            output,
            started: Instant::now(),
            at: Instant::now(),
            phase: 0,
            root: None,
            events: vec![],
            pointer: None,
            rows: vec![],
            old_scroll: 0.,
        }
    }
    pub(super) fn inject(&mut self, raw: &mut egui::RawInput) {
        raw.events.clear();
        raw.hovered_files.clear();
        raw.dropped_files.clear();
        raw.focused = true;
        raw.events.push(
            self.pointer
                .map(egui::Event::PointerMoved)
                .unwrap_or(egui::Event::PointerGone),
        );
        raw.events.append(&mut self.events);
    }
    fn next(&mut self) {
        self.phase += 1;
        self.at = Instant::now();
    }
    fn wheel(&mut self, b: &Browser, up: bool) {
        let r = b.list_viewport.unwrap();
        self.pointer = Some(egui::pos2(r.right() - 2., r.center().y));
        self.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0., if up { 1. } else { -1. }),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: Default::default(),
        });
    }
    fn click(&mut self, b: &Browser) -> bool {
        let Some(rect) = b.sticky_jump_button else {
            return false;
        };
        self.pointer = Some(rect.center());
        for pressed in [true, false] {
            self.events.push(egui::Event::PointerButton {
                pos: rect.center(),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
        }
        true
    }
    fn gap(&mut self, b: &mut Browser, path: &Path) -> bool {
        let Some(p) = &b.prepared else {
            return false;
        };
        let Some(h) = p
            .layout
            .headers
            .iter()
            .find(|h| h.kind.is_none() && h.directory == path)
        else {
            return false;
        };
        b.folder_location_anchor = None;
        b.scroll_override = Some(h.rect.y + 2.);
        self.pointer = b
            .list_viewport
            .map(|r| egui::pos2(r.right() - 2., r.center().y));
        true
    }
    fn visible(b: &Browser, path: &Path) -> bool {
        b.folder_location_anchor.as_deref() == Some(path)
            && b.prepared.as_ref().is_some_and(|p| {
                p.snapshot
                    .files
                    .iter()
                    .any(|r| r.file.parent == path && b.list_regions.contains_key(&r.id))
            })
    }
    fn capture(&mut self, b: &Browser, ctx: &egui::Context, name: &str) {
        let rect = |r: Rect| [r.left(), r.top(), r.width(), r.height()];
        self.rows.push(serde_json::json!({"name":name,"up":b.sticky_jump_up,"target":b.sticky_jump_target,"button":b.sticky_jump_button.map(rect),"viewport":b.list_viewport.map(rect),"scroll":b.scroll_y,"anchor":b.folder_location_anchor,"sticky":b.sticky_paths,"collapsed":b.collapsed,"filter":format!("{:?}",b.settings.filter),"pixels_per_point":ctx.pixels_per_point()}));
        b.snapshot_image(
            ctx,
            &self.output.join(format!("{name}.png")).to_string_lossy(),
        );
    }
    pub(super) fn tick(&mut self, b: &mut Browser, ctx: &egui::Context) {
        if self.phase == 255 {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(20));
        if self.started.elapsed() > Duration::from_secs(110) {
            self.capture(b, ctx, "timeout");
            self.finish(ctx, Some("media jump timeout"));
            return;
        }
        let Some(p) = b
            .prepared
            .clone()
            .filter(|p| p.snapshot.finished && !b.prepare_dirty && p.revision == b.revision)
        else {
            return;
        };
        if self.at.elapsed() < Duration::from_millis(500) {
            return;
        }
        if self.root.is_none() {
            self.root = Some(p.snapshot.root.clone());
        }
        let root = self.root.as_ref().unwrap().clone();
        let gap = root.join("03-gap/020-empty");
        let image = root.join("04-images");
        let video = root.join("02-video");
        let deep = p
            .snapshot
            .files
            .iter()
            .find(|r| r.file.parent.starts_with(root.join("05-deep")))
            .map(|r| r.file.parent.clone())
            .unwrap();
        match self.phase {
            0 => {
                if b.auto_fold_empty {
                    b.auto_fold_empty = false;
                    b.prepare_dirty = true;
                    b.prepare_after = Instant::now();
                    return;
                }
                b.pinned = false;
                if self.gap(b, &gap) {
                    self.next();
                }
            }
            1 => {
                self.wheel(b, false);
                self.next();
            }
            2 => {
                if b.sticky_jump_up || b.sticky_jump_target.as_deref() != Some(image.as_path()) {
                    self.finish(ctx, Some("down target did not skip empty folders"));
                    return;
                }
                self.capture(b, ctx, "down-empty-gap");
                self.next();
            }
            3 => {
                if self.click(b) {
                    self.next();
                }
            }
            4 if Self::visible(b, &image) => {
                self.capture(b, ctx, "down-image-visible");
                self.wheel(b, true);
                self.next();
            }
            5 => {
                if !b.sticky_jump_up || b.sticky_jump_target.as_deref() != Some(video.as_path()) {
                    self.finish(ctx, Some("up direction/target incorrect"));
                    return;
                }
                self.capture(b, ctx, "up-target-video");
                self.next();
            }
            6 => {
                if self.click(b) {
                    self.next();
                }
            }
            7 if Self::visible(b, &video) => {
                self.capture(b, ctx, "up-video-visible");
                b.collapsed.insert(image.clone());
                b.prepare_dirty = true;
                b.prepare_after = Instant::now();
                self.next();
            }
            8 => {
                if self.gap(b, &gap) {
                    self.wheel(b, false);
                    self.next();
                }
            }
            9 => {
                if b.sticky_jump_target.as_deref() == Some(image.as_path()) && self.click(b) {
                    self.next();
                }
            }
            10 if Self::visible(b, &image) => {
                if b.collapsed.contains(&image) {
                    self.finish(ctx, Some("jump did not unfold destination"));
                    return;
                }
                self.capture(b, ctx, "collapsed-target-unfolded");
                b.settings.filter = MediaFilter::Images;
                b.prepare_dirty = true;
                b.prepare_after = Instant::now();
                self.next();
            }
            11 => {
                if self.gap(b, &gap) {
                    self.wheel(b, true);
                    self.next();
                }
            }
            12 => {
                if b.sticky_jump_target.as_deref() != Some(root.join("00-images").as_path()) {
                    self.finish(ctx, Some("image filter still targeted video"));
                    return;
                }
                self.capture(b, ctx, "images-filter-skips-video");
                if self.click(b) {
                    self.next();
                }
            }
            13 if Self::visible(b, &root.join("00-images")) => {
                b.settings.filter = MediaFilter::Videos;
                b.prepare_dirty = true;
                b.prepare_after = Instant::now();
                self.next();
            }
            14 => {
                if self.gap(b, &gap) {
                    self.wheel(b, false);
                    self.next();
                }
            }
            15 => {
                if b.sticky_jump_target.is_some() {
                    self.finish(ctx, Some("end boundary should have no destination"));
                    return;
                }
                self.capture(b, ctx, "videos-end-disabled");
                self.old_scroll = b.scroll_y;
                self.click(b);
                self.next();
            }
            16 => {
                if (b.scroll_y - self.old_scroll).abs() > 1. {
                    self.finish(ctx, Some("disabled arrow scrolled the view"));
                    return;
                }
                b.settings.filter = MediaFilter::All;
                b.collapsed.insert(root.join("05-deep"));
                b.prepare_dirty = true;
                b.prepare_after = Instant::now();
                self.next();
            }
            17 => {
                if self.gap(b, &root.join("06-tail/020-empty")) {
                    self.wheel(b, true);
                    self.next();
                }
            }
            18 => {
                if b.sticky_jump_target.as_deref() != Some(deep.as_path()) {
                    self.finish(ctx, Some("hidden descendant destination missing"));
                    return;
                }
                if self.click(b) {
                    self.next();
                }
            }
            19 if Self::visible(b, &deep) => {
                self.capture(b, ctx, "collapsed-ancestor-unfolded");
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(400., 660.)));
                self.next();
            }
            20 => {
                b.locate_folder(&deep);
                self.next();
            }
            21 => {
                let (Some(button), Some(view)) = (b.sticky_jump_button, b.list_viewport) else {
                    return;
                };
                if button.right() > view.right() + 1.
                    || button.right() < view.right() - 45.
                    || button.left() < view.left()
                {
                    self.finish(ctx, Some("long breadcrumb pushed arrow out of viewport"));
                    return;
                }
                self.capture(b, ctx, "narrow-long-breadcrumb");
                self.next();
            }
            22 => self.finish(ctx, None),
            _ => {}
        }
    }
    fn finish(&mut self, ctx: &egui::Context, error: Option<&str>) {
        let _ = std::fs::create_dir_all(&self.output);
        let _=std::fs::write(self.output.join("report.json"),serde_json::to_vec_pretty(&serde_json::json!({"checks_pass":error.is_none(),"error":error,"phase":self.phase,"rows":self.rows,"scope":"Native generated-fixture controlled wheel/click input and FBO; empty folders, filters, fold recovery, disabled boundary, narrow breadcrumb. No physical input/performance certification."})).unwrap());
        self.phase = 255;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
