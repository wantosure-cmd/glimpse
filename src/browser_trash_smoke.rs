//! Destructive diagnostics refuse any root except a marked, generated workspace fixture.
use super::*;
pub(super) struct TrashSmoke {
    started: Instant,
    at: Instant,
    phase: u8,
    events: Vec<egui::Event>,
    rows: Vec<serde_json::Value>,
    reports: Vec<serde_json::Value>,
    locked: Option<std::fs::File>,
    failure_captured: bool,
    undo_checked: bool,
    undo_captured: bool,
    browsed_pending: bool,
}
impl TrashSmoke {
    pub(super) fn new() -> Self {
        Self {
            started: Instant::now(),
            at: Instant::now(),
            phase: 0,
            events: vec![],
            rows: vec![],
            reports: vec![],
            locked: None,
            failure_captured: false,
            undo_checked: false,
            undo_captured: false,
            browsed_pending: false,
        }
    }
    pub(super) fn inject(&mut self, raw: &mut egui::RawInput) {
        raw.focused = true;
        raw.events.push(egui::Event::PointerGone);
        raw.events.append(&mut self.events);
    }
    fn key(&mut self, pressed: bool) {
        self.events.push(egui::Event::Key {
            key: egui::Key::Delete,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
    }
    fn click_delete(&mut self) {
        self.key(true);
        self.key(false);
    }
    fn next(&mut self) {
        self.phase += 1;
        self.at = Instant::now();
    }
    fn id(b: &Browser, name: &str) -> Result<MediaId, String> {
        b.prepared
            .as_ref()
            .and_then(|p| {
                p.snapshot
                    .files
                    .iter()
                    .find(|r| r.file.path.file_name().is_some_and(|n| n == name))
            })
            .map(|r| r.id)
            .ok_or_else(|| format!("Missing fixture {name}"))
    }
    fn ready(b: &Browser, count: usize, batch: u64) -> bool {
        b.trash_operation.is_none()
            && b.pending_trash.is_none()
            && b.trash_counter == batch
            && b.prepared.as_ref().is_some_and(|p| {
                p.snapshot.generation == b.generation
                    && p.snapshot.finished
                    && p.snapshot.files.len() == count
            })
    }
    fn capture(&mut self, b: &Browser, name: &str) {
        self.rows.push(serde_json::json!({"phase":name,"tree_media":b.sidebar.as_ref().and_then(|s|s.directories.iter().find(|d|d.path==s.root)).map(|d|d.recursive.images+d.recursive.videos),"tree_root":b.sidebar.as_ref().map(|s|&s.root),"elapsed_ms":self.started.elapsed().as_secs_f64()*1000.,"batch":b.trash_counter,"media":b.prepared.as_ref().map(|p|p.snapshot.files.len()),"detail":b.detail.as_ref().map(|v|v.id),"selected":b.selection.iter().copied().collect::<Vec<_>>(),"gif_released":b.animation.released(),"last_trash":b.last_trash}));
    }
    fn finish(&mut self, b: &Browser, ctx: &egui::Context, error: Option<String>) {
        self.phase = 255;
        self.capture(b, "finished");
        let _=std::fs::write("artifacts/p4-trash-ui.json",serde_json::to_vec_pretty(&serde_json::json!({"error":error,"scope":"Synthetic Delete events through native egui and real IFileOperation, only marked dedicated copies; not physical keyboard proof","rows":self.rows,"reports":self.reports})).unwrap());
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    pub(super) fn tick(&mut self, b: &mut Browser, ctx: &egui::Context) {
        if self.phase == 255 {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(20));
        if self.started.elapsed() > Duration::from_secs(140) {
            self.finish(b, ctx, Some(format!("timeout phase {}", self.phase)));
            return;
        }
        let result = self.step(b, ctx);
        if let Err(error) = result {
            self.finish(b, ctx, Some(error));
        }
    }
    fn step(&mut self, b: &mut Browser, ctx: &egui::Context) -> Result<(), String> {
        let elapsed = self.at.elapsed();
        match self.phase {
            100=>{
                if elapsed<Duration::from_secs(3){return Ok(());}
                if b.pending_trash.is_none() || b.trash_operation.is_some() || b.trash_counter!=0 {
                    return Err("Media deletion was submitted before undo grace elapsed".into());
                }
                for name in ["02.jpg","06.jpg"] {
                    let id=Self::id(b,name)?;let p=b.prepared.as_ref().unwrap();
                    if !p.snapshot.files[p.lookup[&id]].file.path.exists(){return Err("File removed before undo deadline".into());}
                }
                self.capture(b,"undo_wait_files_intact");
                b.open_detail(Self::id(b,"03.mp4")?);b.toggle_video_fullscreen();self.next();
            }
            101=>{
                if elapsed<Duration::from_secs(1) || !ctx.input(|i|i.viewport().fullscreen==Some(true)){return Ok(());}
                if !b.player.as_ref().is_some_and(|p|p.lock().unwrap().has_presented_frame()){return Ok(());}
                if b.pending_trash.is_none() || b.trash_counter!=0 {return Err("Undo grace disappeared while browsing".into());}
                if !self.undo_captured {
                    self.undo_captured=true;self.capture(b,"undo_visible_over_playing_video");
                    b.snapshot_image(ctx,"p4-media-undo-countdown.png");self.at=Instant::now();return Ok(());
                }
                if elapsed<Duration::from_millis(550){return Ok(());}
                let pos=b.trash_undo_button.ok_or("Undo control missing above detail")?.center();
                self.events.push(egui::Event::PointerMoved(pos));
                for pressed in [true,false]{self.events.push(egui::Event::PointerButton {pos,button:egui::PointerButton::Primary,pressed,modifiers:Default::default()});}
                self.next();
            }
            102=>{
                if elapsed<Duration::from_millis(200){return Ok(());}
                if b.pending_trash.is_some() || b.trash_operation.is_some() || b.trash_counter!=0
                    || b.video_fullscreen.is_none() || b.detail.as_ref().map(|v|v.id)!=Some(Self::id(b,"03.mp4")?) {
                    return Err("Undo click failed or leaked through the playing detail".into());
                }
                for name in ["01.jpg","02.jpg","03.mp4","04.gif","05.jpg","06.jpg"]{Self::id(b,name)?;}
                self.capture(b,"undo_cancelled_without_submission");
                self.undo_checked=true;b.close_detail();b.selection.clear();self.phase=1;self.at=Instant::now();
            }
            0 => {
                if !Self::ready(b, 6, 0) {
                    return Ok(());
                }
                let root = b.watch_root.as_ref().ok_or("missing root")?;
                let root = std::fs::canonicalize(root).map_err(|e| e.to_string())?;
                if root
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("trash-ui-"))
                {
                    let target =
                        std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("target"))
                            .map_err(|e| e.to_string())?;
                    if root.parent() != Some(target.as_path())
                        || std::fs::read(root.join(".trash-smoke-owned"))
                            .ok()
                            .as_deref()
                            != Some(b"generated recycle UI copies")
                    {
                        return Err("Refuse unowned parent fixture".into());
                    }
                    b.navigate(root.join("media"), true);
                    return Ok(());
                }
                let base = root.parent().ok_or("missing parent")?;
                let target =
                    std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("target"))
                        .map_err(|e| e.to_string())?;
                if base.parent() != Some(target.as_path())
                    || !base
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("trash-ui-"))
                    || root.file_name().is_none_or(|n| n != "media")
                    || std::fs::read(base.join(".trash-smoke-owned"))
                        .ok()
                        .as_deref()
                        != Some(b"generated recycle UI copies")
                {
                    return Err("Refuse destructive smoke outside marked owned fixture".into());
                }
                for name in ["01.jpg", "02.jpg", "03.mp4", "04.gif", "05.jpg", "06.jpg"] {
                    let id = Self::id(b, name)?;
                    let p = b.prepared.as_ref().unwrap();
                    let record = &p.snapshot.files[p.lookup[&id]];
                    if record.file.parent != root {
                        return Err("Refuse nested/unexpected fixture".into());
                    }
                }
                b.selection.clear();
                self.click_delete();
                self.next();
            }
            1 => {
                if elapsed < Duration::from_millis(150) {
                    return Ok(());
                }
                if b.trash_counter != 0 {
                    return Err("Empty selection performed delete".into());
                }
                self.capture(b, "empty_selection_noop");
                b.selection = HashSet::from([Self::id(b, "02.jpg")?, Self::id(b, "06.jpg")?]);
                if !self.undo_checked {
                    self.click_delete();self.phase=100;self.at=Instant::now();return Ok(());
                }
                b.open_detail(Self::id(b, "03.mp4")?);
                self.next();
            }
            2 => {
                if elapsed < Duration::from_millis(200)
                    || !b.player.as_ref().is_some_and(|p| p.lock().unwrap().ready())
                {
                    return Ok(());
                }
                self.capture(b, "video_before_delete");
                self.key(true);
                self.key(true);
                self.next();
            }
            3 => {
                if b.pending_trash.is_some() && !self.browsed_pending {
                    b.open_detail(Self::id(b,"04.gif")?);
                    self.browsed_pending=true;
                    self.capture(b,"browsed_other_media_during_countdown");
                    return Ok(());
                }
                if !Self::ready(b, 5, 1) {
                    return Ok(());
                }
                if b.detail.as_ref().map(|v| v.id) != Some(Self::id(b, "04.gif")?) {
                    return Err("Deleting detail did not select next GIF".into());
                }
                Self::id(b, "02.jpg")?;
                Self::id(b, "06.jpg")?;
                self.reports
                    .push(b.last_trash.clone().ok_or("missing result")?);
                self.capture(b, "video_recycled_next_gif");
                self.key(true);
                self.next();
            }
            4 => {
                if elapsed < Duration::from_millis(250) {
                    return Ok(());
                }
                if b.trash_counter != 1 {
                    return Err("Held Delete repeated into next media".into());
                }
                Self::id(b, "04.gif")?;
                self.capture(b, "held_delete_did_not_delete_next");
                self.key(false);
                self.next();
            }
            5 => {
                if elapsed < Duration::from_millis(100) || b.animation.sequence.is_none() {
                    return Ok(());
                }
                self.capture(b, "gif_before_delete");
                self.click_delete();
                self.next();
            }
            6 => {
                if !Self::ready(b, 4, 2) {
                    return Ok(());
                }
                if b.detail.as_ref().map(|v| v.id) != Some(Self::id(b, "05.jpg")?) {
                    return Err("Deleting GIF did not select next image".into());
                }
                self.reports
                    .push(b.last_trash.clone().ok_or("missing GIF result")?);
                self.capture(b, "gif_recycled_next_image");
                b.close_detail();
                #[cfg(windows)]
                {
                    use std::os::windows::fs::OpenOptionsExt;
                    let path = b.watch_root.as_ref().unwrap().join("06.jpg");
                    self.locked = Some(
                        std::fs::OpenOptions::new()
                            .read(true)
                            .share_mode(0)
                            .open(path)
                            .map_err(|e| e.to_string())?,
                    );
                }
                b.selection = HashSet::from([Self::id(b, "02.jpg")?, Self::id(b, "06.jpg")?]);
                self.click_delete();
                self.next();
            }
            7 => {
                if !Self::ready(b, 3, 3) {
                    return Ok(());
                }
                let report = b.last_trash.clone().ok_or("missing batch result")?;
                if report["success_count"] != 1 || report["failed_count"] != 1 {
                    return Err("Partial recycle result incorrect".into());
                }
                let locked = Self::id(b, "06.jpg")?;
                if !b.selection.contains(&locked) || b.trash_failures.len() != 1 {
                    return Err("Failed item/selection missing".into());
                }
                self.reports.push(report);
                self.capture(b, "list_partial_failure");

                self.click_delete();
                self.next();
            }
            8 => {
                if elapsed < Duration::from_millis(200) {
                    return Ok(());
                }
                if b.trash_counter != 3 {
                    return Err("Delete passed through failure window".into());
                }
                if !self.failure_captured {
                    self.failure_captured = true;
                    b.snapshot_image(ctx, "p4-trash-partial-failure.png");
                    return Ok(());
                }
                if elapsed < Duration::from_millis(550) {
                    return Ok(());
                }
                self.capture(b, "failure_window_delete_blocked");
                self.locked = None;
                b.trash_failures.clear();
                self.click_delete();
                self.next();
            }
            9 => {
                if !Self::ready(b, 2, 4) {
                    return Ok(());
                }
                self.reports
                    .push(b.last_trash.clone().ok_or("missing retry result")?);
                if !b.selection.is_empty() {
                    return Err("Successful retry left selection".into());
                }
                self.capture(b, "retry_success");
                b.snapshot_image(ctx, "p4-trash-final.png");
                self.next();
            }
            10 => {
                if elapsed < Duration::from_millis(200) {
                    return Ok(());
                }
                b.open_detail(Self::id(b, "05.jpg")?);
                self.click_delete();
                self.next();
            }
            11 => {
                if !Self::ready(b, 1, 5) {
                    return Ok(());
                }
                if b.detail.as_ref().map(|v| v.id) != Some(Self::id(b, "01.jpg")?) {
                    return Err("Last media deletion did not fall back to previous".into());
                }
                self.reports.push(
                    b.last_trash
                        .clone()
                        .ok_or("missing previous fallback result")?,
                );
                self.capture(b, "last_detail_falls_back_previous");
                self.click_delete();
                self.next();
            }
            12 => {
                if !Self::ready(b, 0, 6) {
                    return Ok(());
                }
                if b.detail.is_some() {
                    return Err("Empty sequence did not close detail".into());
                }
                self.reports
                    .push(b.last_trash.clone().ok_or("missing empty result")?);
                self.capture(b, "empty_sequence_closes_detail");
                b.snapshot_image(ctx, "p4-trash-final.png");
                self.next();
            }
            13 => {
                if elapsed < Duration::from_millis(200) {
                    return Ok(());
                }
                self.finish(b, ctx, None);
                self.phase = 255;
            }
            _ => {}
        }
        Ok(())
    }
}
