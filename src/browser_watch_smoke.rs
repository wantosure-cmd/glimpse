//! Opt-in native UI regression. Mutations are restricted to marked generated fixtures.
use super::*;
pub(super) struct WatchSmoke {
    started: Instant,
    at: Instant,
    phase: u8,
    id: MediaId,
    anchor: Option<ScrollAnchor>,
    fingerprint: Option<Fingerprint>,
    generation: u64,
    actor: Option<std::sync::mpsc::Receiver<Result<(), String>>>,
    rows: Vec<serde_json::Value>,
    tree_rows: Vec<serde_json::Value>,
    pub(super) trace: Vec<serde_json::Value>,
    finished: bool,
}
impl WatchSmoke {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            at: Instant::now(),
            phase: 0,
            id: 0,
            anchor: None,
            fingerprint: None,
            generation: 0,
            actor: None,
            rows: vec![],
            tree_rows: vec![],
            trace: vec![],
            finished: false,
        }
    }
    fn next(&mut self) {
        self.phase += 1;
        self.at = Instant::now();
    }
    fn capture(&mut self, b: &Browser, name: &str) {
        let p = b.prepared.as_ref().unwrap();
        let anchor_error = self.anchor.as_ref().and_then(|a| {
            p.layout
                .rectangles
                .get(&a.media_id)
                .map(|r| r.y - b.scroll_y - a.offset)
        });
        self.rows.push(serde_json::json!({"phase":name,"media":p.snapshot.files.len(),"groups":p.view.groups.len(),"generation":b.generation,"prepared_generation":p.snapshot.generation,"detail":b.detail.as_ref().map(|d|d.id),"selected":b.selection.contains(&self.id),"scroll":b.scroll_y,"anchor_error":anchor_error,"watch_warning":b.watch_warning,"elapsed_ms":self.started.elapsed().as_millis()}));
    }
    fn mutate(&mut self, b: &Browser, operation: u8) -> Result<(), String> {
        let root = b
            .sidebar
            .as_ref()
            .map(|s| s.root.clone())
            .or_else(|| b.watch_root.clone())
            .ok_or("Missing tree root")?;
        let ctx = b.ctx.clone();
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        self.actor = Some(rx);
        std::thread::spawn(move || {
            let result = (|| -> Result<(), String> {
                let root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
                let base = root.parent().ok_or("Missing base")?;
                let target =
                    std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("target"))
                        .map_err(|e| e.to_string())?;
                if base.parent() != Some(target.as_path())
                    || !base
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("watch-ui-"))
                    || root.file_name().is_none_or(|n| n != "media")
                    || std::fs::read(base.join(".watch-smoke-owned"))
                        .ok()
                        .as_deref()
                        != Some(b"generated watch UI fixture")
                {
                    return Err("Refuse mutation outside dedicated marked watch UI fixture".into());
                }
                match operation {
                    0 => {
                        std::fs::create_dir_all(root.join("新增目录"))
                            .map_err(|e| e.to_string())?;
                        std::fs::copy(base.join("original.jpg"), root.join("新增目录/新增.jpg"))
                            .map_err(|e| e.to_string())?;
                    }
                    1 => image::RgbImage::from_pixel(120, 360, image::Rgb([70, 140, 20]))
                        .save(root.join("010.jpg"))
                        .map_err(|e| e.to_string())?,
                    2 => {
                        let copy = std::fs::canonicalize(root.join("010.jpg"))
                            .map_err(|e| e.to_string())?;
                        if !copy.starts_with(&root) {
                            return Err("Copy escaped fixture".into());
                        }
                        // External actor, only our known copy. Product Delete is not invoked.
                        std::fs::remove_file(copy).map_err(|e| e.to_string())?;
                    }
                    3 => {
                        for index in 0..40 {
                            std::fs::copy(
                                base.join("original.jpg"),
                                root.join(format!("新增目录/子{index:03}.jpg")),
                            )
                            .map_err(|e| e.to_string())?;
                        }
                        std::fs::create_dir_all(root.join("兄弟目录"))
                            .map_err(|e| e.to_string())?;
                        std::fs::copy(base.join("original.jpg"), root.join("兄弟目录/兄弟.jpg"))
                            .map_err(|e| e.to_string())?;
                        std::fs::copy(
                            Path::new(env!("CARGO_MANIFEST_DIR"))
                                .join("test-media/test-h264-aac.mp4"),
                            root.join("兄弟目录/视频.mp4"),
                        )
                        .map_err(|e| e.to_string())?;
                    }
                    4 => {
                        for path in ["新增目录/子新增.jpg", "兄弟目录/兄弟新增.jpg"]
                        {
                            std::fs::copy(base.join("original.jpg"), root.join(path))
                                .map_err(|e| e.to_string())?;
                        }
                    }
                    5 => {
                        let copy = std::fs::canonicalize(root.join("兄弟目录/兄弟.jpg"))
                            .map_err(|e| e.to_string())?;
                        if !copy.starts_with(&root) {
                            return Err("Sibling copy escaped owned fixture".into());
                        }
                        std::fs::remove_file(copy).map_err(|e| e.to_string())?;
                    }
                    _ => return Err("Unknown fixture operation".into()),
                }
                Ok(())
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        Ok(())
    }
    fn capture_tree(&mut self, b: &Browser, name: &str) -> Result<(), String> {
        let p = b.prepared.as_ref().ok_or("No view")?;
        let tree = b.sidebar.as_ref().ok_or("No tree")?;
        if tree.generation != b.generation || b.watch_source_root.as_ref() != Some(&tree.root) {
            return Err("Stale tree or wrong watched scope".into());
        }
        let counts = |path: &Path| {
            tree.directories
                .iter()
                .find(|d| d.path == path)
                .map(|d| d.recursive)
                .unwrap_or_default()
        };
        let root = counts(&tree.root);
        let current = counts(&p.snapshot.root);
        let sibling = counts(&tree.root.join("兄弟目录"));
        let anchor_error = self.anchor.as_ref().and_then(|a| {
            p.layout
                .rectangles
                .get(&a.media_id)
                .map(|r| r.y - b.scroll_y - a.offset)
        });
        self.tree_rows.push(serde_json::json!({"phase":name,"root":p.snapshot.root,"tree_root":tree.root,"generation":b.generation,"tree_generation":tree.generation,"media":p.snapshot.files.len(),"root_images":root.images,"root_videos":root.videos,"current_images":current.images,"current_videos":current.videos,"sibling_images":sibling.images,"sibling_videos":sibling.videos,"selected":b.selection.contains(&self.id),"scroll":b.scroll_y,"anchor_error":anchor_error}));
        if name.starts_with("child_after")
            && (!b.selection.contains(&self.id) || anchor_error.is_none_or(|e| e.abs() > 1.))
        {
            return Err("Descendant refresh lost selection/anchor".into());
        }
        Ok(())
    }
    fn step(&mut self, b: &mut Browser, ctx: &egui::Context) -> Result<bool, String> {
        if self.started.elapsed() > Duration::from_secs(45) {
            return Err(format!("Watch UI timeout at phase {}", self.phase));
        }
        if let Some(rx) = &self.actor {
            match rx.try_recv() {
                Ok(result) => {
                    result?;
                    self.actor = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(false),
                Err(_) => return Err("Fixture actor disconnected".into()),
            }
        }
        let Some(p) = b.prepared.as_ref().filter(|p| {
            p.snapshot.finished
                && p.snapshot.generation == b.generation
                && !b.scan_busy
                && !b.prepare_dirty
        }) else {
            return Ok(false);
        };
        match self.phase {
            0 if p.snapshot.files.len() == 40 && self.at.elapsed() > Duration::from_secs(1) => {
                let record = p
                    .snapshot
                    .files
                    .iter()
                    .find(|r| r.file.path.file_name().is_some_and(|n| n == "010.jpg"))
                    .ok_or("Missing dedicated copy")?;
                let Some(rect) = p.layout.rectangles.get(&record.id) else {
                    return Ok(false);
                };
                self.id = record.id;
                self.fingerprint = Some(record.file.fingerprint.clone());
                b.selection.insert(self.id);
                b.scroll_override = Some(rect.y + (rect.h * 0.5).min(20.));
                self.next();
            }
            1 if self.at.elapsed() > Duration::from_millis(800) => {
                self.anchor = p.layout.anchor(b.scroll_y);
                if self.anchor.is_none() || b.scroll_y < 1. {
                    // Geometry publication can supersede the diagnostic's queued
                    // scroll before the next draw. Establish a real card-interior
                    // position before testing external mutation; never accept a
                    // missing anchor or a refresh error as a passing observation.
                    if let Some(rect) = p.layout.rectangles.get(&self.id) {
                        b.scroll_override = Some(rect.y + (rect.h * 0.5).min(20.));
                    }
                    return Ok(false);
                }
                self.capture(b, "before_add");
                self.mutate(b, 0)?;
                self.next();
            }
            2 if p.snapshot.files.len() == 41 && self.at.elapsed() > Duration::from_millis(900) => {
                let a = self.anchor.as_ref().unwrap();
                let rect = p
                    .layout
                    .rectangles
                    .get(&a.media_id)
                    .ok_or("Anchor disappeared")?;
                let error = rect.y - b.scroll_y - a.offset;
                let groups = p.view.groups.len();
                self.capture(b, "after_add");
                if error.abs() > 1. || !b.selection.contains(&self.id) || groups != 2 {
                    return Err(format!(
                        "Refresh lost anchor, selection or grouping: anchor_error={error}, selected={}, groups={groups}",
                        b.selection.contains(&self.id)
                    ));
                }
                b.snapshot_image(ctx, "p4-watch-added.png");
                b.open_detail(self.id);
                self.next();
            }
            3 if self.at.elapsed() > Duration::from_millis(600) => {
                self.capture(b, "detail_before_replace");
                self.mutate(b, 1)?;
                self.next();
            }
            4 => {
                let record = p
                    .lookup
                    .get(&self.id)
                    .map(|i| &p.snapshot.files[*i])
                    .ok_or("Replaced copy lost id")?;
                if Some(&record.file.fingerprint) == self.fingerprint.as_ref()
                    || record.width != Some(120)
                    || record.height != Some(360)
                    || self.at.elapsed() < Duration::from_millis(900)
                {
                    return Ok(false);
                }
                if b.detail.as_ref().map(|d| d.id) != Some(self.id)
                    || !b.selection.contains(&self.id)
                {
                    return Err("Same-path replacement lost detail/selection".into());
                }
                self.capture(b, "detail_after_replace");
                b.snapshot_image(ctx, "p4-watch-replaced.png");
                self.mutate(b, 2)?;
                self.next();
            }
            5 if p.snapshot.files.len() == 40
                && !p.lookup.contains_key(&self.id)
                && self.at.elapsed() > Duration::from_millis(900) =>
            {
                if b.detail.is_some() || b.selection.contains(&self.id) {
                    return Err("Removed copy remained in detail/selection".into());
                }
                self.capture(b, "after_external_remove");
                self.generation = b.generation;
                b.refresh_in_place();
                self.next();
            }
            6 if b.generation > self.generation => {
                self.capture(b, "after_manual_refresh");
                self.mutate(b, 3)?;
                self.next();
            }
            7 if p.snapshot.files.len() == 82 && self.at.elapsed() > Duration::from_millis(900) => {
                self.capture_tree(b, "tree_expanded")?;
                let child = p.snapshot.root.join("新增目录");
                b.pinned = true;
                b.navigate(child, true);
                self.next();
            }
            8 if p.snapshot.files.len() == 41
                && p.snapshot.root.file_name().is_some_and(|n| n == "新增目录")
                && self.at.elapsed() > Duration::from_millis(900) =>
            {
                self.id = p
                    .snapshot
                    .files
                    .iter()
                    .find(|r| r.file.path.file_name().is_some_and(|n| n == "子020.jpg"))
                    .ok_or("Missing child copy")?
                    .id;
                b.selection.clear();
                b.selection.insert(self.id);
                let Some(rect) = p.layout.rectangles.get(&self.id) else {
                    return Ok(false);
                };
                b.scroll_override = Some(rect.y + (rect.h * 0.5).min(20.));
                self.next();
            }
            9 if self.at.elapsed() > Duration::from_millis(900) => {
                self.anchor = p.layout.anchor(b.scroll_y);
                if self.anchor.is_none() || b.scroll_y < 1. {
                    if let Some(rect) = p.layout.rectangles.get(&self.id) {
                        b.scroll_override = Some(rect.y + (rect.h * 0.5).min(20.));
                    }
                    return Ok(false);
                }
                self.capture_tree(b, "child_before_changes")?;
                self.mutate(b, 4)?;
                self.next();
            }
            10 if p.snapshot.files.len() == 42
                && self.at.elapsed() > Duration::from_millis(900) =>
            {
                self.capture_tree(b, "child_after_additions")?;
                b.snapshot_image(ctx, "p4-watch-tree-added.png");
                self.mutate(b, 5)?;
                self.next();
            }
            11 if self.at.elapsed() > Duration::from_millis(900)
                && b.sidebar.as_ref().is_some_and(|s| s.files.len() == 83) =>
            {
                self.capture_tree(b, "child_after_sibling_removal")?;
                self.generation = b.generation;
                b.refresh_in_place();
                self.next();
            }
            12 if b.generation > self.generation => {
                self.capture_tree(b, "child_after_manual_refresh")?;
                return Ok(true);
            }
            _ => (),
        }
        Ok(false)
    }
    pub fn tick(&mut self, b: &mut Browser, ctx: &egui::Context) {
        if self.finished {
            return;
        }
        if self.phase >= 7 {
            self.trace.push(serde_json::json!({"kind":"draw","phase":self.phase,
                "scroll":b.scroll_y,"viewport":b.viewport_height,"width":b.last_width,
                "layout":b.prepared.as_ref().map(|p|serde_json::json!({"height":p.layout.height,"cards":p.layout.rectangles.len(),"columns":p.layout.columns.len(),"column_width":p.layout.column_width,"pending":p.layout.sizing_rows.len(),"target":p.layout.rectangles.get(&self.id).map(|r|[r.x,r.y,r.w,r.h]),"generation":p.snapshot.generation}))}));
        }
        ctx.request_repaint_after(Duration::from_millis(20));
        let error = match self.step(b, ctx) {
            Ok(false) => return,
            Ok(true) => None,
            Err(e) => Some(e),
        };
        let result = serde_json::json!({"error":error,"rows":self.rows,"tree_rows":self.tree_rows,"trace":self.trace,
            "final_state":{"phase":self.phase,"scroll":b.scroll_y,"cards":b.prepared.as_ref().map(|p|p.layout.rectangles.len()),"generation":b.generation},
            "scope":"Native UI + actual filesystem actor in marked generated copies; programmatic UI actions, not physical input or product recycle-bin Delete"});
        self.finished = true;
        let _ = std::fs::write(
            "artifacts/p4-watch-ui.json",
            serde_json::to_vec_pretty(&result).unwrap(),
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
