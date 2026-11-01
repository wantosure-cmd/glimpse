//! Opt-in generated-media diagnostic; no physical input or source writes.
use super::*;
pub(super) struct AnimationSmoke {
    output: PathBuf,
    started: Instant,
    at: Instant,
    phase: u8,
    rows: Vec<serde_json::Value>,
    seen: HashSet<String>,
    root: Option<PathBuf>,
}
impl AnimationSmoke {
    pub(super) fn new(output: PathBuf) -> Self {
        Self {
            output,
            started: Instant::now(),
            at: Instant::now(),
            phase: 0,
            rows: vec![],
            seen: HashSet::new(),
            root: None,
        }
    }
    pub(super) fn inject(&self, raw: &mut egui::RawInput) {
        raw.events.clear();
        raw.hovered_files.clear();
        raw.dropped_files.clear();
        raw.focused = false;
        raw.events.push(egui::Event::PointerGone);
    }
    fn advance(&mut self) {
        self.phase += 1;
        self.at = Instant::now();
    }
    fn capture(&mut self, b: &Browser, ctx: &egui::Context, name: &str) {
        let entries: Vec<_> = b
            .list_animations
            .entries
            .iter()
            .map(|(id, e)| {
                serde_json::json!({"id":id,"sequence":e.sequence,"cycle":e.cycle,"ended":e.ended,
                "error":e.error,"delay_ms":e.delay.as_secs_f64()*1000.,
                "texture_id":e.texture.as_ref().map(|t|format!("{:?}",t.id())),
                "rect":b.list_regions.get(id).map(|r|[r.min.x,r.min.y,r.width(),r.height()])})
            })
            .collect();
        self.rows.push(serde_json::json!({"phase":self.phase,"name":name,
            "elapsed_ms":self.started.elapsed().as_secs_f64()*1000.,"generation":b.generation,
            "list":entries,"stats":b.list_animations.stats(),"released":b.list_animations.released(),
            "list_texture_bytes":b.list_animations.texture_bytes(),"scroll_y":b.scroll_y,
            "ui_width":ctx.content_rect().width(),"pixels_per_point":ctx.pixels_per_point(),
            "focused":ctx.input(|i|i.focused),"pointer":ctx.input(|i|i.pointer.hover_pos()).map(|p|[p.x,p.y]),
            "detail_sequence":b.animation.sequence,"detail_cycle":b.animation.cycle,
            "detail_ended":b.animation.ended(),"detail_error":b.animation.error,
            "detail_texture_bytes":b.animation.texture_bytes()}));
        b.snapshot_image(ctx, &format!("animation-{name}.png"));
        self.at = Instant::now();
    }
    pub(super) fn tick(&mut self, b: &mut Browser, ctx: &egui::Context) {
        if self.phase == 255 {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(10));
        if self.started.elapsed() > Duration::from_secs(75) {
            self.capture(b, ctx, "timeout");
            self.finish(ctx, Some("timeout"));
            return;
        }
        if let Some(error) = b
            .list_animations
            .entries
            .values()
            .find_map(|e| e.error.as_deref())
        {
            self.finish(ctx, Some(error));
            return;
        }
        if let Some(error) = b.animation.error.as_deref() {
            self.finish(ctx, Some(error));
            return;
        }
        let Some(p) = b.prepared.clone().filter(|p| p.snapshot.finished) else {
            return;
        };
        let id = |name: &str| {
            p.snapshot
                .files
                .iter()
                .find(|r| r.file.path.file_name().is_some_and(|s| s == name))
                .map(|r| r.id)
        };
        match self.phase {
            0 => {
                self.root = Some(p.snapshot.root.clone());
                b.pinned = false;
                b.sidebar_open = false;
                b.target_width = 220.;
                self.advance();
            }
            1 => {
                if b.list_animations.entries.len() < 4 {
                    return;
                }
                for (key, e) in &b.list_animations.entries {
                    let marker = format!("list-{key}-{}", e.sequence);
                    if e.texture.is_some() && e.sequence < 9 && self.seen.insert(marker) {
                        self.rows.push(serde_json::json!({"name":"list-frame","id":key,
                            "sequence":e.sequence,"cycle":e.cycle,"delay_ms":e.delay.as_secs_f64()*1000.,
                            "elapsed_ms":self.started.elapsed().as_secs_f64()*1000.,
                            "texture_id":e.texture.as_ref().map(|t|format!("{:?}",t.id()))}));
                    }
                }
                if let Some(e) = id("01-loop.gif").and_then(|id| b.list_animations.entries.get(&id))
                {
                    if e.sequence >= 3 {
                        // PNG screenshot encoding can stall the diagnostic UI.
                        // Capture the next occurrence of each color, rather
                        // than assuming every frame in cycle two was painted.
                        let name = format!("list-color-{}", e.sequence % 3);
                        if self.seen.insert(name.clone()) {
                            self.capture(b, ctx, &name);
                        }
                    }
                }
                let done = [
                    ("01-loop.gif", 8, false),
                    ("02-loop.gif", 8, false),
                    ("03-finite.gif", 5, true),
                    ("04-once.gif", 2, true),
                ]
                .iter()
                .all(|(name, seq, end)| {
                    id(name)
                        .and_then(|id| b.list_animations.entries.get(&id))
                        .is_some_and(|e| {
                            e.sequence >= *seq && e.ended == *end && e.texture.is_some()
                        })
                });
                if done
                    && self.at.elapsed() > Duration::from_millis(250)
                    && (0..3).all(|i| self.seen.contains(&format!("list-color-{i}")))
                {
                    self.capture(b, ctx, "list-auto-complete");
                    b.scroll_override = Some(p.layout.height);
                    self.advance();
                }
            }
            2 if self.at.elapsed() > Duration::from_millis(300) && b.list_animations.released() => {
                self.capture(b, ctx, "offscreen-released");
                b.scroll_override = Some(0.);
                self.advance();
            }
            3 => {
                if id("01-loop.gif")
                    .and_then(|id| b.list_animations.entries.get(&id))
                    .is_some_and(|e| e.sequence >= 2)
                {
                    self.capture(b, ctx, "reentry-playing");
                    if let Some(id) = id("05-loop.png") {
                        b.open_detail(id);
                        self.advance();
                    }
                }
            }
            4 => {
                if let Some(seq) = b.animation.sequence {
                    if seq >= 3 {
                        let name = format!("apng-color-{}", seq % 3);
                        if self.seen.insert(name.clone()) {
                            self.capture(b, ctx, &name);
                        }
                    }
                    if seq >= 8
                        && self.at.elapsed() > Duration::from_millis(250)
                        && (0..3).all(|i| self.seen.contains(&format!("apng-color-{i}")))
                    {
                        self.capture(b, ctx, "apng-infinite");
                        b.close_detail();
                        if let Some(id) = id("06-finite.apng") {
                            b.open_detail(id);
                            self.advance();
                        }
                    }
                }
            }
            5 if b.animation.ended() => {
                if b.animation.sequence != Some(7) {
                    self.finish(ctx, Some("APNG finite count"));
                    return;
                }
                self.capture(b, ctx, "apng-finite");
                b.close_detail();
                if let Some(id) = id("07-cover.png") {
                    b.open_detail(id);
                    self.advance();
                }
            }
            6 if b.animation.ended() => {
                if b.animation.sequence != Some(2) {
                    self.finish(ctx, Some("APNG cover counted as frame"));
                    return;
                }
                self.capture(b, ctx, "apng-cover-excluded");
                b.close_detail();
                if let Some(id) = id("08-static.png") {
                    b.open_detail(id);
                    self.advance();
                }
            }
            7 if self.at.elapsed() > Duration::from_millis(600) => {
                if b.animation.sequence.is_some() || !b.animation.released() {
                    self.finish(ctx, Some("static PNG animated"));
                    return;
                }
                self.capture(b, ctx, "static-png");
                b.close_detail();
                b.navigate(self.root.as_ref().unwrap().join("empty"), false);
                self.advance();
            }
            8 if p.snapshot.root == self.root.as_ref().unwrap().join("empty")
                && b.list_animations.released()
                && b.animation.released() =>
            {
                self.capture(b, ctx, "navigation-released");
                self.finish(ctx, None);
            }
            _ => {}
        }
    }
    fn finish(&mut self, ctx: &egui::Context, error: Option<&str>) {
        let _ = std::fs::create_dir_all(&self.output);
        let _=std::fs::write(self.output.join("report.json"),serde_json::to_vec_pretty(&serde_json::json!({
            "scope":"Native UI/FBO; generated sources; synthetic unfocused PointerGone input, no physical keyboard/mouse claim",
            "error":error,"rows":self.rows})).unwrap());
        self.phase = 255;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
