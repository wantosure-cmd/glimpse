//! Opt-in timed real detail navigation. Source media stays read-only.
use super::*;

#[derive(Deserialize)]
struct Sources { paths: Vec<PathBuf>, transitions: usize }

pub(super) struct VideoSwitchSmoke {
    output: PathBuf,
    manifest: Option<PathBuf>,
    started: Instant,
    at: Instant,
    phase: u8,
    ids: Vec<MediaId>,
    paths: Vec<PathBuf>,
    count: usize,
    current: usize,
    target: usize,
    forward: bool,
    queued: Option<egui::Key>,
    sent: Option<Instant>,
    rows: Vec<serde_json::Value>,
    initial_ms: Option<f64>,
    loading_ticks: usize,
}

fn normalized(path: &Path) -> String {
    let text = path.to_string_lossy().replace('/', "\\");
    let text = if let Some(unc) = text.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{unc}")
    } else { text.strip_prefix("\\\\?\\").unwrap_or(&text).to_owned() };
    text.to_lowercase()
}

impl VideoSwitchSmoke {
    pub fn new(output: PathBuf, manifest: Option<PathBuf>) -> Self {
        Self { output, manifest, started: Instant::now(), at: Instant::now(), phase: 0,
            ids: vec![], paths: vec![], count: 20, current: 0, target: 0, forward: true,
            queued: None, sent: None, rows: vec![], initial_ms: None, loading_ticks: 0 }
    }
    pub fn inject(&mut self, raw: &mut egui::RawInput) {
        raw.events.clear(); raw.events.push(egui::Event::PointerGone); raw.focused = true;
        if let Some(key) = self.queued.take() {
            self.sent = Some(Instant::now());
            for pressed in [true, false] { raw.events.push(egui::Event::Key {
                key, physical_key: Some(key), pressed, repeat: false, modifiers: Default::default()
            }); }
        }
    }
    fn settled(b: &Browser) -> bool {
        !b.scan_busy && !b.prepare_dirty && b.prepared.as_ref().is_some_and(|p|
            p.snapshot.finished && p.snapshot.generation == b.generation && p.revision == b.revision)
    }
    fn finish(&mut self, b: &Browser, ctx: &egui::Context, error: Option<String>) {
        let viewport = ctx.input(|i| i.content_rect().size());
        let report = serde_json::json!({"checks_pass":error.is_none(), "error":error,
            "version":env!("CARGO_PKG_VERSION"), "rows":self.rows, "expected_transitions":self.count,
            "paths":self.paths, "initial_ready_ms":self.initial_ms, "phase":self.phase,
            "pending_target":self.target, "status":b.status,
            "detail_id":b.detail.as_ref().map(|v|v.id), "player_media":b.player_media,
            "deferred_detail":b.deferred_detail, "resume_pending":b.resume_pending,
            "resume_starting":b.resume_starting, "restore_flight":b.restore_flight,
            "video_region":b.video_region.map(|(id,r)|serde_json::json!({"id":id,"size":[r.width(),r.height()]})),
            "viewport_state":ctx.input(|i|serde_json::json!({"focused":i.focused,"minimized":i.viewport().minimized,"occluded":i.viewport().occluded})),
            "player":b.player.as_ref().map(|p|{let p=p.lock().unwrap();serde_json::json!({"ready":p.ready(),
                "presented":p.has_presented_frame(), "source":p.source,"path":p.text("path"), "error":p.error(),
                "display_size":p.display_size(), "pause":p.text("pause"), "time":p.number("time-pos"),
                "first_ready_render_ms":p.first_ready_render_ms})}),
            "viewport":[viewport.x, viewport.y], "pixels_per_point":ctx.pixels_per_point(),
            "scope":"Injected ArrowRight/Left -> matching source-epoch ready render in native mpv FBO. No physical display/input/audio certification; no PNG readback during timing. Selected-source favorites filter, start positions cleared; source read/hash warms external caches."});
        if let Err(error) = std::fs::write(&self.output, serde_json::to_vec_pretty(&report).unwrap()) {
            eprintln!("Unable to write switch evidence: {error}");
        }
        self.phase = 255; ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    pub fn tick(&mut self, b: &mut Browser, ctx: &egui::Context) {
        if self.phase == 255 { return; }
        ctx.request_repaint_after(Duration::from_millis(5));
        if self.started.elapsed() > Duration::from_secs(90 + self.count as u64 * 21) {
            self.finish(b, ctx, Some(format!("Switch diagnostic timeout at phase {}", self.phase))); return;
        }
        if let Err(error) = self.step(b, ctx) { self.finish(b, ctx, Some(error)); }
    }
    fn step(&mut self, b: &mut Browser, ctx: &egui::Context) -> Result<(), String> {
        match self.phase {
            0 => {
                let owned = std::fs::canonicalize(b.data_dir.parent().ok_or("Missing isolated parent")?).map_err(|e|e.to_string())?;
                let target = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("target")).map_err(|e|e.to_string())?;
                if owned.parent() != Some(target.as_path())
                    || !owned.file_name().is_some_and(|n|n.to_string_lossy().starts_with("switch-owned-"))
                    || std::fs::read(owned.join(".switch-owned")).ok().as_deref() != Some(b"read-only source switch test") {
                    return Err("Refuse diagnostic outside marked isolated data directory".into());
                }
                let manifest = self.manifest.as_ref().ok_or("Missing source manifest")?;
                if manifest.parent() != b.data_dir.parent() { return Err("Manifest must belong to isolated task".into()); }
                if std::fs::metadata(manifest).map_err(|e|e.to_string())?.len() > 65536 { return Err("Oversized manifest".into()); }
                let sources: Sources = serde_json::from_slice(&std::fs::read(manifest).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
                if !(2..=8).contains(&sources.paths.len()) || !(20..=40).contains(&sources.transitions) {
                    return Err("Require 2..8 videos and 20..40 transitions".into());
                }
                if sources.paths.iter().map(|p|normalized(p)).collect::<HashSet<_>>().len() != sources.paths.len() {
                    return Err("Duplicate source path".into());
                }
                self.paths = sources.paths; self.count = sources.transitions; self.phase = 1;
            }
            1 => {
                if !Self::settled(b) { return Ok(()); }
                let p = b.prepared.as_ref().unwrap();
                let mut favorites = HashSet::new();
                for path in &self.paths {
                    let record = p.snapshot.files.iter().find(|r|normalized(&r.file.path) == normalized(path))
                        .ok_or_else(||format!("Selected video not in root: {path:?}"))?;
                    if record.file.kind != MediaKind::Video { return Err("Non-video source".into()); }
                    favorites.insert(record.file.path.clone());
                }
                b.features.favorites.paths = favorites; b.features.only_favorites = true;
                b.settings.filter = MediaFilter::Videos; b.settings.sort = SortField::Name; b.settings.descending = false;
                b.muted = true; b.video_end_action = VideoEndAction::Pause;
                b.prepare_dirty = true; b.prepare_after = Instant::now(); self.phase = 2;
            }
            2 => {
                if !Self::settled(b) { return Ok(()); }
                let p = b.prepared.as_ref().unwrap(); self.ids = p.view.sequence.clone();
                if self.ids.len() != self.paths.len() { return Err("Filtered sequence includes wrong videos".into()); }
                self.paths = self.ids.iter().map(|id|p.snapshot.files[p.lookup[id]].file.path.clone()).collect();
                b.positions.clear(); self.at = Instant::now(); b.open_detail(self.ids[0]); self.phase = 3;
            }
            3 | 5 => {
                let expected = self.ids[self.target];
                let matching = b.detail.as_ref().is_some_and(|v|v.id == expected) && b.player_media == Some(expected);
                let ready = matching && b.resume_pending.is_none() && !b.resume_starting
                    && b.player.as_ref().is_some_and(|p|p.lock().unwrap().has_presented_frame());
                let began = if self.phase == 5 { self.sent } else { Some(self.at) };
                let Some(began) = began else { return Ok(()); };
                if !ready {
                    self.loading_ticks += 1;
                    if began.elapsed() > Duration::from_secs(20) { return Err(format!("No matching rendered frame for transition {}", self.rows.len())); }
                    return Ok(());
                }
                let elapsed = began.elapsed().as_secs_f64() * 1000.;
                let player = b.player.as_ref().unwrap().lock().unwrap();
                if self.phase == 3 { self.initial_ms = Some(elapsed); }
                else { self.rows.push(serde_json::json!({"index":self.rows.len(), "from":self.paths[self.current],
                    "to":self.paths[self.target], "direction":if self.forward {"next"} else {"previous"},
                    "input_to_ready_render_ms":elapsed, "mpv_first_ready_render_ms":player.first_ready_render_ms,
                    "loading_ticks":self.loading_ticks, "time_pos":player.number("time-pos"),
                    "video_codec":player.text("video-codec"), "audio_codec":player.text("audio-codec-name")})); }
                let _ = player.set("pause", "yes"); drop(player);
                self.current = self.target; self.at = Instant::now(); self.phase = 4;
            }
            4 => {
                if self.rows.len() == self.count { self.finish(b, ctx, None); return Ok(()); }
                if self.at.elapsed() < Duration::from_millis(200) { return Ok(()); }
                if self.current == self.ids.len()-1 { self.forward = false; }
                if self.current == 0 { self.forward = true; }
                self.target = if self.forward {self.current+1} else {self.current-1};
                b.positions.remove(&self.ids[self.target]); self.sent = None; self.loading_ticks = 0;
                self.queued = Some(if self.forward {egui::Key::ArrowRight} else {egui::Key::ArrowLeft}); self.phase = 5;
            }
            _ => {}
        }
        Ok(())
    }
}
