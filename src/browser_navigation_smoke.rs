//! Read-only, opt-in native UI regression for tree navigation and media motion.
use super::*;
pub(super) struct NavigationSmoke {
    output: PathBuf,
    phase: u8,
    at: Instant,
    started: Instant,
    root: Option<PathBuf>,
    generation: u64,
    first: Option<MediaId>,
    events: Vec<egui::Event>,
    pointer: Option<Pos2>,
    rows: Vec<serde_json::Value>,
    saw_sizing: bool,
    release_next: Option<Pos2>,
    resume_anchor: Option<ScrollAnchor>,
}
impl NavigationSmoke {
    pub(super) fn new(output: PathBuf) -> Self {
        Self {
            output,
            phase: 0,
            at: Instant::now(),
            started: Instant::now(),
            root: None,
            generation: 0,
            first: None,
            events: vec![],
            pointer: None,
            rows: vec![],
            saw_sizing: false,
            release_next: None,
            resume_anchor: None,
        }
    }
    pub(super) fn inject(&mut self, raw: &mut egui::RawInput) {
        // This diagnostic owns its synthetic input. Real mouse/keyboard events
        // must not be combined with a scripted click in the same frame.
        raw.events.clear();
        raw.hovered_files.clear();
        raw.dropped_files.clear();
        raw.focused = true;
        raw.events
            .push(egui::Event::ModifiersChanged(egui::Modifiers::default()));
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
    fn advance(&mut self) {
        self.phase += 1;
        self.at = Instant::now();
    }
    fn capture(&mut self, b: &Browser, name: &str) {
        let rect = |r: Rect| [r.left(), r.top(), r.width(), r.height()];
        self.rows.push(serde_json::json!({"phase":name,"generation":b.generation,"root":b.snapshot.as_ref().map(|s|&s.root),"scroll":b.scroll_y,"detail":b.detail.as_ref().map(|v|v.id),"motion":b.motion_rect.map(rect),"sticky":b.sticky_paths,"collapsed":b.collapsed,"guides":b.prepared.as_ref().map(|p|p.layout.tree_guides.len()),"columns":b.prepared.as_ref().map(|p|p.layout.columns.len()),"unknown_cards":b.prepared.as_ref().map(|p|p.layout.rectangles.keys().filter(|id|p.lookup.get(id).is_some_and(|i|p.snapshot.files[*i].width.zip(p.snapshot.files[*i].height).is_none() && p.snapshot.files[*i].error.is_none())).count()),"sizing_rows":b.prepared.as_ref().map(|p|p.layout.sizing_rows.len()),"folder_counts":b.prepared.as_ref().map(|p|p.view.groups.iter().map(|g|serde_json::json!({"path":g.directory,"direct_folders":g.direct_folders})).collect::<Vec<_>>())}));
    }
    pub(super) fn tick(&mut self, b: &mut Browser, ctx: &egui::Context) {
        if self.phase == 255 {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(10));
        if self.started.elapsed() > Duration::from_secs(45) {
            self.finish(b, ctx, Some("timeout"));
            return;
        }
        let Some(p) = b
            .prepared
            .clone()
            .filter(|p| p.snapshot.finished && !b.prepare_dirty && p.revision == b.revision)
        else {
            return;
        };
        if self.phase == 0 {
            // Never scan outside the caller's explicit root and never edit/delete any file.
            self.root = Some(p.snapshot.root.clone());
            self.generation = b.generation;
            self.first = p
                .view
                .sequence
                .iter()
                .find(|id| {
                    p.lookup.get(id).is_some_and(|i| {
                        p.snapshot.files[*i].file.kind == MediaKind::Image
                            && p.snapshot.files[*i].width.is_some()
                    })
                })
                .copied();
            if self.first.is_none() {
                if !self.saw_sizing {
                    self.capture(b, "waiting_for_dimensions");
                    b.snapshot_image(ctx, "sizing-before-cells.png");
                    self.saw_sizing = true;
                }
                return;
            }
            b.pinned = true;
            b.locate_folder(&p.snapshot.root.join("123/456"));
            self.advance();
            return;
        }
        let root = self.root.clone().unwrap();
        let target = root.join("123/456");
        let wait = self.at.elapsed();
        match self.phase {
            1 if wait > Duration::from_millis(250) => {
                if let Some(rect) = b.folder_title_regions.get(&target) {
                    self.click(rect.center(), egui::PointerButton::Primary);
                    self.advance();
                }
            }
            2 if wait > Duration::from_millis(180) => {
                if b.generation != self.generation || p.snapshot.root != root {
                    self.finish(b, ctx, Some("folder click changed root"));
                    return;
                }
                self.capture(b, "folder_click_locates");
                b.snapshot_image(ctx, "tree-navigation.png");
                self.phase = 30;
                self.at = Instant::now();
            }
            3 if wait > Duration::from_millis(180) => {
                if let Some(rect) = b.sticky_toggle_regions.get(&target) {
                    let point = rect.center();
                    self.capture(b, "sticky_toggle_requested");
                    self.rows.last_mut().unwrap()["pointer"] =
                        serde_json::json!([point.x, point.y]);
                    self.rows.last_mut().unwrap()["requested_revision"] =
                        serde_json::json!(b.revision);
                    self.rows.last_mut().unwrap()["prepared_revision"] =
                        serde_json::json!(p.revision);
                    self.click(point, egui::PointerButton::Primary);
                    self.advance();
                }
            }
            4 if wait > Duration::from_millis(180) => {
                if !b.collapsed.contains(&target)
                    || p.layout
                        .headers
                        .iter()
                        .any(|h| h.directory == target.join("789"))
                {
                    self.capture(b, "sticky_collapse_failure");
                    b.snapshot_image(ctx, "sticky-collapse-failed.png");
                    self.finish(b, ctx, Some("sticky collapse did not hide descendants"));
                    return;
                }
                self.capture(b, "sticky_collapsed");
                b.locate_folder(&target.join("789"));
                self.advance();
            }
            5 if wait > Duration::from_millis(220) => {
                if b.collapsed.contains(&target)
                    || !p
                        .layout
                        .headers
                        .iter()
                        .any(|h| h.directory == target.join("789"))
                {
                    self.finish(b, ctx, Some("locate did not expand ancestor"));
                    return;
                }
                self.capture(b, "locate_expands_ancestors");
                b.folder_location_anchor = None;
                b.scroll_override = Some(0.);
                self.advance();
            }
            6 if wait > Duration::from_millis(220) => {
                if let Some(rect) = b.list_regions.get(&self.first.unwrap()) {
                    self.click(rect.center(), egui::PointerButton::Primary);
                    self.advance();
                }
            }
            7 if wait > Duration::from_millis(35) => {
                if b.detail.is_none() {
                    return;
                }
                if b.motion_rect.is_none() {
                    self.finish(b, ctx, Some("opening motion not painted"));
                    return;
                }
                self.capture(b, "opening_motion");
                b.snapshot_image(ctx, "tree-opening-motion.png");
                self.advance();
            }
            8 if wait > Duration::from_millis(260) => {
                let Some(rect) = b.image_region else {
                    return;
                };
                self.click(rect.center(), egui::PointerButton::Extra2);
                self.advance();
            }
            9 if wait > Duration::from_millis(140) => {
                if b.detail.as_ref().map(|v| v.id)
                    != view::neighbor(&p.view.sequence, self.first.unwrap(), 1)
                {
                    self.finish(b, ctx, Some("image forward button failed"));
                    return;
                }
                self.capture(b, "side_next");
                self.click(
                    b.image_region.unwrap().center(),
                    egui::PointerButton::Extra1,
                );
                self.advance();
            }
            10 if wait > Duration::from_millis(140) => {
                if b.detail.as_ref().map(|v| v.id) != self.first {
                    self.finish(b, ctx, Some("image back button failed"));
                    return;
                }
                self.capture(b, "side_previous");
                let Some(rect) = b.detail_layout.strip else {
                    return;
                };
                self.pointer = Some(rect.center());
                self.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0., -3.),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: Default::default(),
                });
                self.advance();
            }
            11 if wait > Duration::from_millis(200) => {
                let first = p
                    .view
                    .sequence
                    .iter()
                    .position(|id| Some(*id) == self.first)
                    .unwrap();
                if b.detail.as_ref().map(|v| v.id) != p.view.sequence.get(first + 3).copied() {
                    self.finish(b, ctx, Some("filmstrip wheel did not advance three"));
                    return;
                }
                self.capture(b, "filmstrip_three_steps");
                self.pointer = None;
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
            12 if wait > Duration::from_millis(35) => {
                if b.detail.is_some() || b.motion_rect.is_none() {
                    self.finish(b, ctx, Some("closing motion not painted"));
                    return;
                }
                self.capture(b, "closing_motion");
                b.snapshot_image(ctx, "tree-closing-motion.png");
                self.advance();
            }
            13 if wait > Duration::from_millis(250) => {
                if b.media_transition.is_some() {
                    self.finish(b, ctx, Some("motion did not finish"));
                    return;
                }
                self.capture(b, "returned_to_list");
                let video = p
                    .view
                    .sequence
                    .iter()
                    .find(|id| {
                        p.lookup
                            .get(id)
                            .is_some_and(|i| p.snapshot.files[*i].file.kind == MediaKind::Video)
                    })
                    .copied();
                if let Some(id) = video {
                    self.first = Some(id);
                    if let Some(rect) = p.layout.rectangles.get(&id) {
                        b.scroll_override = Some(rect.y.max(0.));
                    }
                    self.advance();
                } else {
                    self.finish(b, ctx, Some("video fixture missing"));
                }
            }
            14 if wait > Duration::from_millis(350) => {
                if let Some(rect) = b.list_regions.get(&self.first.unwrap()) {
                    self.click(rect.center(), egui::PointerButton::Primary);
                    self.advance();
                }
            }
            15 if wait > Duration::from_millis(35) => {
                if b.detail.is_none() {
                    return;
                }
                if b.motion_rect.is_none() {
                    self.finish(b, ctx, Some("video opening motion not painted"));
                    return;
                }
                self.capture(b, "video_opening_motion");
                b.snapshot_image(ctx, "tree-video-opening-motion.png");
                self.advance();
            }
            16 if wait > Duration::from_millis(700) => {
                if !b.player.as_ref().is_some_and(|p| p.lock().unwrap().ready()) {
                    return;
                }
                self.capture(b, "video_ready");
                self.pointer = None;
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
            17 if wait > Duration::from_millis(35) => {
                if b.detail.is_some() || b.motion_rect.is_none() {
                    self.finish(b, ctx, Some("video closing motion not painted"));
                    return;
                }
                self.capture(b, "video_closing_motion");
                b.snapshot_image(ctx, "tree-video-closing-motion.png");
                self.advance();
            }
            18 if wait > Duration::from_millis(250) => {
                if let Some(rect) = b.preview_size_rect {
                    let pos = rect.right_center() - egui::vec2(1., 0.);
                    self.pointer = Some(pos);
                    self.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    });
                    self.release_next = Some(pos);
                    self.advance();
                }
            }
            19 if wait > Duration::from_millis(300) => {
                if p.layout.columns.len() != 1
                    || b.target_width < media_browser::layout::MAX_PREVIEW_WIDTH
                {
                    self.finish(b, ctx, Some("maximum preview did not produce one column"));
                    return;
                }
                if p.view
                    .groups
                    .iter()
                    .find(|g| g.directory == p.snapshot.root)
                    .map(|g| g.direct_folders)
                    != Some(3)
                {
                    self.finish(
                        b,
                        ctx,
                        Some("direct child count includes descendants or misses empty child"),
                    );
                    return;
                }
                if p.layout.rectangles.keys().any(|id| {
                    p.lookup.get(id).is_some_and(|i| {
                        p.snapshot.files[*i].width.is_none() && p.snapshot.files[*i].error.is_none()
                    })
                }) {
                    self.finish(b, ctx, Some("unknown geometry placed a card"));
                    return;
                }
                self.capture(b, "maximum_single_column");
                b.snapshot_image(ctx, "maximum-single-column.png");
                self.advance();
            }
            20 if wait > Duration::from_millis(120) => {
                self.finish(b, ctx, None);
            }
            30 if wait > Duration::from_millis(100) => {
                if let Some(rect) = b.folder_title_regions.get(&target) {
                    self.click(
                        egui::pos2(rect.right() + 100., rect.center().y),
                        egui::PointerButton::Primary,
                    );
                    self.advance();
                }
            }
            31 if wait > Duration::from_millis(200) => {
                if !b.collapsed.contains(&target)
                    || p.layout
                        .headers
                        .iter()
                        .any(|h| h.directory == target.join("789"))
                {
                    self.finish(
                        b,
                        ctx,
                        Some("folder blank click did not collapse descendants"),
                    );
                    return;
                }
                self.capture(b, "folder_blank_collapsed");
                if let Some(rect) = b.folder_title_regions.get(&target) {
                    self.click(
                        egui::pos2(rect.right() + 100., rect.center().y),
                        egui::PointerButton::Primary,
                    );
                    self.advance();
                }
            }
            32 if wait > Duration::from_millis(200) => {
                if b.collapsed.contains(&target)
                    || !p
                        .layout
                        .headers
                        .iter()
                        .any(|h| h.directory == target.join("789"))
                {
                    self.finish(
                        b,
                        ctx,
                        Some("folder blank click did not expand descendants"),
                    );
                    return;
                }
                self.capture(b, "folder_blank_expanded");
                if let Some(card) = p.layout.rectangles.iter().find(|(id, _)| {
                    p.lookup
                        .get(id)
                        .is_some_and(|i| p.snapshot.files[*i].file.parent == target)
                }) {
                    b.folder_location_anchor = None;
                    b.scroll_override = Some(card.1.y + 80.);
                }
                self.phase = 40;
                self.at = Instant::now();
            }
            40 if wait > Duration::from_millis(220) => {
                self.resume_anchor = p.layout.anchor(b.scroll_y);
                let Some(anchor) = &self.resume_anchor else {
                    self.finish(b, ctx, Some("missing deep folder resume anchor"));
                    return;
                };
                if !p
                    .lookup
                    .get(&anchor.media_id)
                    .is_some_and(|i| p.snapshot.files[*i].file.parent.starts_with(&target))
                {
                    self.finish(
                        b,
                        ctx,
                        Some("resume fixture not scrolled inside target folder"),
                    );
                    return;
                }
                self.capture(b, "folder_resume_before_collapse");
                b.toggle_folder_collapse(&target);
                self.advance();
            }
            41 if wait > Duration::from_millis(220) => {
                if !b.collapsed.contains(&target) {
                    self.finish(b, ctx, Some("resume collapse not applied"));
                    return;
                }
                b.toggle_folder_collapse(&target);
                self.advance();
            }
            42 if wait > Duration::from_millis(220) => {
                let anchor = self.resume_anchor.as_ref().unwrap();
                let error = p
                    .layout
                    .restore_anchor(anchor)
                    .map(|y| (y - b.scroll_y).abs());
                if b.collapsed.contains(&target) || error.is_none_or(|e| e > 1.) {
                    self.finish(b, ctx, Some("folder expand lost reading position"));
                    return;
                }
                self.capture(b, "folder_resume_after_expand");
                self.rows.last_mut().unwrap()["anchor_error"] = serde_json::json!(error);
                b.snapshot_image(ctx, "folder-resume-after-expand.png");
                b.toggle_folder_collapse(&root);
                self.advance();
            }
            43 if wait > Duration::from_millis(220) => {
                if !b.collapsed.contains(&root) {
                    self.finish(b, ctx, Some("root resume collapse not applied"));
                    return;
                }
                b.toggle_folder_collapse(&root);
                self.advance();
            }
            44 if wait > Duration::from_millis(220) => {
                let anchor = self.resume_anchor.as_ref().unwrap();
                let error = p
                    .layout
                    .restore_anchor(anchor)
                    .map(|y| (y - b.scroll_y).abs());
                if b.collapsed.contains(&root) || error.is_none_or(|e| e > 1.) {
                    self.finish(b, ctx, Some("root expand lost descendant reading position"));
                    return;
                }
                self.capture(b, "root_resume_after_expand");
                self.rows.last_mut().unwrap()["anchor_error"] = serde_json::json!(error);
                self.phase = 3;
                self.at = Instant::now();
            }
            _ => {}
        }
    }
    fn finish(&mut self, b: &Browser, ctx: &egui::Context, error: Option<&str>) {
        let report = serde_json::json!({"error":error,"checks_pass":error.is_none(),"rows":self.rows,"generation":b.generation,"target_width":b.target_width,"preview_size_rect":b.preview_size_rect.map(|r|[r.left(),r.top(),r.width(),r.height()]),"scope":"Read-only native egui controlled events and FBO; not physical input or OS window capture"});
        let _ = std::fs::write(&self.output, serde_json::to_vec_pretty(&report).unwrap());
        self.phase = 255;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
