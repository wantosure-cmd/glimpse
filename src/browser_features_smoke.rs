//! Opt-in regression restricted to generated target/features-owned-* fixtures.
use super::*;
pub(super) struct FeaturesSmoke {
    pub clipboard: Vec<(bool, Vec<PathBuf>)>,
    output: PathBuf,
    phase: u8,
    at: Instant,
    started: Instant,
    events: Vec<egui::Event>,
    checks: Vec<String>,
    favorite: Option<PathBuf>,
    ids: Vec<MediaId>,
    paths: Vec<PathBuf>,
    trace: Vec<serde_json::Value>,
    trace_at: Instant,
}
impl FeaturesSmoke {
    pub fn new(output: PathBuf) -> Self {
        Self {
            clipboard: Vec::new(),
            output,
            phase: 0,
            at: Instant::now(),
            started: Instant::now(),
            events: vec![],
            checks: vec![],
            favorite: None,
            ids: vec![],
            paths: vec![],
            trace: vec![],
            trace_at: Instant::now(),
        }
    }
    pub fn inject(&mut self, raw: &mut egui::RawInput) {
        raw.focused = true;
        raw.events.append(&mut self.events);
    }
    fn pointer(&mut self, p: Pos2, pressed: Option<bool>) {
        self.events.push(egui::Event::PointerMoved(p));
        if let Some(pressed) = pressed {
            self.events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
        }
    }
    fn click(&mut self, p: Pos2) {
        self.pointer(p, Some(true));
        self.pointer(p, Some(false));
    }
    fn advance(&mut self, label: &str) {
        self.checks.push(label.into());
        self.phase += 1;
        self.at = Instant::now();
    }
    fn finish(&mut self, ctx: &egui::Context, error: Option<&str>) {
        let _=std::fs::write(&self.output,serde_json::to_vec_pretty(&serde_json::json!({"checks_pass":error.is_none(),"error":error,"checks":self.checks,"trace":self.trace,"scope":"Generated dedicated fixture; actual native GL screenshots and injected egui events, not physical OS input","version":env!("CARGO_PKG_VERSION")})).unwrap());
        self.phase = 255;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    pub fn tick(&mut self, b: &mut Browser, ctx: &egui::Context) {
        if self.phase == 255 {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(30));
        if self.trace_at.elapsed() >= Duration::from_secs(1) {
            self.trace.push(serde_json::json!({"phase":self.phase,"elapsed_ms":self.started.elapsed().as_millis(),"state":b.cleanup_diagnostic_for_smoke()}));
            self.trace_at = Instant::now();
        }
        if self.started.elapsed() > Duration::from_secs(100) {
            self.finish(
                ctx,
                Some(&format!("Timeout phase {} status {}", self.phase, b.status)),
            );
            return;
        }
        if self.at.elapsed() < Duration::from_millis(250)
            && !(self.phase == 17 && b.features.close_clicked)
        {
            return;
        }
        let Some(p) = b.prepared.clone() else {
            return;
        };
        match self.phase {
            0 => {
                if !p.snapshot.finished || p.view.sequence.len() < 4 || b.list_regions.len() < 4 {
                    return;
                }
                let expected = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target");
                let root = std::fs::canonicalize(&p.snapshot.root).unwrap();
                let target = std::fs::canonicalize(expected).unwrap();
                if !root.starts_with(&target)
                    || !root.components().any(|c| {
                        c.as_os_str()
                            .to_string_lossy()
                            .starts_with("features-owned-")
                    })
                    || !root.join("generated-fixture.marker").is_file()
                {
                    self.finish(ctx, Some("Unsafe fixture scope"));
                    return;
                }
                self.ids = p
                    .view
                    .sequence
                    .iter()
                    .filter(|id| b.list_regions.contains_key(id))
                    .take(4)
                    .copied()
                    .collect();
                self.paths = self
                    .ids
                    .iter()
                    .map(|id| p.snapshot.files[p.lookup[id]].file.path.clone())
                    .collect();
                self.favorite = Some(p.snapshot.files[p.lookup[&self.ids[0]]].file.path.clone());
                let rect = b.list_regions[&self.ids[0]];
                self.click(features::favorite_rect(rect).center());
                self.advance("fixture-scope");
            }
            1 => {
                if !b
                    .features
                    .favorites
                    .paths
                    .contains(self.favorite.as_ref().unwrap())
                {
                    self.finish(ctx, Some("Favorite click failed"));
                    return;
                }
                let saved =
                    media_browser::favorites::Favorites::load(&b.data_dir.join("favorites.json"))
                        .unwrap();
                if !saved.paths.contains(self.favorite.as_ref().unwrap()) {
                    self.finish(ctx, Some("Favorite not persisted"));
                    return;
                }
                b.snapshot_image(ctx, "features-zh-list.png");
                b.features.only_favorites = true;
                b.prepare_dirty = true;
                b.prepare_after = Instant::now();
                self.advance("favorite-click-and-persistence");
            }
            2 => {
                if b.prepare_dirty || p.view.sequence.len() != 1 {
                    return;
                }
                b.select_favorites(None);
                if b.selection.len() != 1 {
                    self.finish(ctx, Some("Favorite selection failed"));
                    return;
                }
                b.features.only_favorites = false;
                b.prepare_dirty = true;
                b.prepare_after = Instant::now();
                self.events.push(egui::Event::Copy);
                self.events.push(egui::Event::Cut);
                self.advance("favorite-filter-and-selection");
            }
            3 => {
                if b.prepare_dirty || b.list_regions.len() < 4 {
                    return;
                }
                if self.clipboard.len() != 2
                    || self.clipboard[0].0
                    || !self.clipboard[1].0
                    || self.clipboard.iter().any(|(_, paths)| {
                        paths.len() != 1 || Some(&paths[0]) != self.favorite.as_ref()
                    })
                {
                    self.finish(
                        ctx,
                        Some("Native clipboard events did not target only the selected favorite"),
                    );
                    return;
                }
                b.selection.clear();
                let pos = b.list_regions[&self.ids[0]].min + egui::vec2(18., 18.);
                self.pointer(pos, Some(true));
                self.advance("copy-cut-events-preserve-favorite-selection-and-drag-start");
            }
            4 => {
                self.pointer(b.list_regions[&self.ids[1]].center(), None);
                self.advance("drag-move-one");
            }
            5 => {
                self.pointer(b.list_regions[&self.ids[2]].center(), None);
                self.advance("drag-move-two");
            }
            6 => {
                self.pointer(b.list_regions[&self.ids[3]].center(), Some(false));
                self.advance("drag-release");
            }
            7 => {
                if self.ids.iter().any(|id| !b.selection.contains(id)) || b.detail.is_some() {
                    self.finish(ctx, Some("Paint selection failed or opened detail"));
                    return;
                }
                b.snapshot_image(ctx, "features-drag.png");
                b.language = "en".into();
                b.status.clear();
                media_browser::ui_text::set_english(true);
                b.workspace.settings_open = true;
                b.workspace.category = workspace_ui::Category::General;
                self.advance("paint-selection");
            }
            8 => {
                if media_browser::ui_text!(msg_df3d58c7d84b) != "Settings" {
                    self.finish(ctx, Some("English switch failed"));
                    return;
                }
                b.snapshot_image(ctx, "features-en-settings.png");
                self.advance("english-settings");
            }
            9 => {
                if b.detail.is_none() {
                    b.workspace.settings_open = false;
                    b.open_detail(
                        *p.view
                            .sequence
                            .iter()
                            .find(|id| p.snapshot.files[p.lookup[id]].file.kind == MediaKind::Video)
                            .unwrap(),
                    );

                    return;
                }

                let Some(player) = &b.player else {
                    return;
                };
                let player = player.lock().unwrap();
                if !player.ready() {
                    return;
                }
                drop(player);
                b.volume = 400.;
                b.player
                    .as_ref()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .set("volume", "400")
                    .unwrap();
                self.advance("volume-request-400");
            }
            10 => {
                if b.player.as_ref().unwrap().lock().unwrap().number("volume") != Some(400.) {
                    return;
                }
                if b.detail_layout.transport.is_some_and(|r| {
                    r.left() < ctx.content_rect().left() - 1.
                        || r.right() > ctx.content_rect().right() + 1.
                }) {
                    self.finish(ctx, Some("English playback controls exceed viewport"));
                    return;
                }
                b.snapshot_image(ctx, "features-volume400.png");
                self.advance("mpv-volume-400");
            }
            11 => {
                b.close_detail();
                b.language = "zh-CN".into();
                media_browser::ui_text::set_english(false);

                if b.features.telemetry.memory.is_none() || b.features.telemetry.cpu.is_none() {
                    return;
                }
                let close = b.features.close_rect.unwrap();
                if close.right() != ctx.content_rect().right()
                    || close.top() != ctx.content_rect().top()
                {
                    self.finish(ctx, Some("Close control misses window corner"));
                    return;
                }
                b.begin_cleanup_for_smoke(&p.snapshot.root);
                self.advance("live-process-counters-and-close-corner");
            }
            12 => {
                if b.features.cleanup.is_none() {
                    self.finish(ctx, Some("Cleanup unavailable"));
                    return;
                }
                b.confirm_cleanup_for_smoke();
                self.advance("cleanup-confirm");
            }
            13 => {
                if self.at.elapsed() < Duration::from_secs(2) {
                    return;
                }
                if !self
                    .ids
                    .iter()
                    .all(|id| p.snapshot.files[p.lookup[id]].file.path.is_file())
                {
                    self.finish(ctx, Some("Cleanup ran before undo period"));
                    return;
                }
                b.features.cleanup = None;
                b.begin_cleanup_for_smoke(&p.snapshot.root);
                self.advance("cleanup-undo-keeps-files");
            }
            14 => {
                b.confirm_cleanup_for_smoke();
                self.advance("cleanup-confirm-again");
            }
            15 => {
                if b.trash_operation.is_some() {
                    return;
                }
                if !b.trash_failures.is_empty() || b.permanent_prompt.is_some() {
                    let state = b.cleanup_diagnostic_for_smoke();
                    self.trace.push(serde_json::json!({"phase":self.phase,"elapsed_ms":self.started.elapsed().as_millis(),"state":state}));
                    self.finish(
                        ctx,
                        Some(&format!(
                            "Recycling failed: {}",
                            b.trash_diagnostic_for_smoke()
                        )),
                    );
                    return;
                }
                // A pending manual confirmation keeps the cleanup object alive.
                // Inspect its terminal failure before waiting for that object to clear.
                if b.features.cleanup.is_some() {
                    return;
                }
                let root = &p.snapshot.root;
                if !self.favorite.as_ref().unwrap().is_file()
                    || !root.join("generated-fixture.marker").is_file()
                    || !root.join("with-file/keep.txt").is_file()
                    || root.join("empty").exists()
                {
                    self.finish(
                        ctx,
                        Some("Cleanup did not preserve files or remove empty folders"),
                    );
                    return;
                }
                if self.paths[1..].iter().any(|path| path.exists()) {
                    self.finish(ctx, Some("Non-favorites remain"));
                    return;
                }
                self.advance("cleanup-preserves-favorites-and-nonmedia-removes-empty");
                b.snapshot_image(ctx, "features-cleanup.png");
            }
            16 => {
                self.click(egui::pos2(
                    ctx.content_rect().right() - 0.5,
                    ctx.content_rect().top() + 0.5,
                ));
                self.advance("close-corner-injected-click");
            }
            _ => {
                if !b.features.close_clicked {
                    self.finish(
                        ctx,
                        Some("Top-right corner click did not reach close control"),
                    );
                } else {
                    self.finish(ctx, None);
                }
            }
        }
    }
}
