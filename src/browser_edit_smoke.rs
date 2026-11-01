//! Opt-in end-to-end diagnostics; edit mode accepts only marked generated fixtures.
use super::*;
pub(super) struct EditSmoke {
    pub events: Vec<egui::Event>,
    output: PathBuf,
    cloud: bool,
    phase: u8,
    at: Instant,
    began: Instant,
    ids: Vec<MediaId>,
    rows: Vec<serde_json::Value>,
    finished: bool,
    controlled_focus: bool,
    requested_source: Option<PathBuf>,
    desired_focus: bool,
    input_observations: Vec<serde_json::Value>,
}
impl EditSmoke {
    pub fn new(output: PathBuf, cloud: bool, controlled_focus: bool) -> Self {
        Self {
            events: vec![],
            output,
            cloud,
            phase: 0,
            at: Instant::now(),
            began: Instant::now(),
            ids: vec![],
            rows: vec![],
            finished: false,
            controlled_focus: controlled_focus && !cloud,
            requested_source: None,
            desired_focus: true,
            input_observations: vec![],
        }
    }
    pub(super) fn with_source(mut self, source: Option<PathBuf>) -> Self {
        self.requested_source = source;
        self
    }
    pub fn inject(&mut self, raw: &mut egui::RawInput) {
        let observed_focus = raw.focused;
        if self.controlled_focus {
            // Explicit opt-in application-input test context, never OS focus proof.
            raw.focused = self.desired_focus;
            // Keep this generated-fixture pixel/input test independent of the
            // real cursor, whose path tooltip can cover the sampled image.
            // No OS cursor movement and no change to ordinary/cloud input.
            raw.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::PointerMoved(_)
                        | egui::Event::PointerButton { .. }
                        | egui::Event::PointerGone
                        | egui::Event::MouseWheel { .. }
                )
            });
            raw.events.push(egui::Event::PointerGone);
        }
        for event in &self.events {
            if let egui::Event::Key {
                key, pressed: true, ..
            } = event
            {
                self.input_observations.push(serde_json::json!({
                    "key":format!("{key:?}"), "observed_os_focus":observed_focus,
                    "applied_input_focus":raw.focused, "controlled":self.controlled_focus,
                    "source":"edit-smoke synthetic event"
                }));
            }
        }
        raw.events.append(&mut self.events);
    }
    fn key(&mut self, key: egui::Key, repeat: bool) {
        self.events.push(egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat,
            modifiers: Default::default(),
        });
        if !repeat {
            self.events.push(egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: false,
                repeat: false,
                modifiers: Default::default(),
            });
        }
    }
    fn finish(&mut self, b: &Browser, ctx: &egui::Context, error: Option<&str>) {
        self.finished = true;
        let _ = std::fs::create_dir_all(&self.output);
        let data = serde_json::json!({"error":error,"cloud":self.cloud,"rows":self.rows,"requested_source":self.requested_source,
            "pointer_mode":if self.controlled_focus { "neutral-no-os-cursor-movement" } else { "observed-os-pointer" },
            "focus_mode":if self.controlled_focus { "controlled_application_input" } else { "observed_os_focus" },
            "input_observations":self.input_observations,
            "scope":"Actual native rendering and file saves; synthetic keys with recorded focus context. Controlled mode includes unfocused negative controls; neither mode proves physical keypad or cloud writes"});
        let _ = std::fs::write(
            self.output.join("report.json"),
            serde_json::to_vec_pretty(&data).unwrap(),
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        let _ = b;
    }
    fn capture(&mut self, b: &Browser, ctx: &egui::Context, name: &str) {
        let record = b.detail.as_ref().and_then(|v| {
            b.prepared
                .as_ref()
                .and_then(|p| p.lookup.get(&v.id).map(|i| &p.snapshot.files[*i]))
        });
        self.rows.push(serde_json::json!({"phase":name,"elapsed_ms":self.began.elapsed().as_secs_f64()*1000.,"detail_id":b.detail.as_ref().map(|v|v.id),"record":record.map(|r|serde_json::json!({"path":r.file.path,"width":r.width,"height":r.height})),"image_region":b.image_region.map(|r|[r.min.x,r.min.y,r.max.x,r.max.y]),"video_region":b.video_region.map(|(_,r)|[r.min.x,r.min.y,r.max.x,r.max.y]),"scale":ctx.pixels_per_point(),"status":b.status,"rotation_message":b.rotation_message,"media_count":b.snapshot.as_ref().map(|s|s.files.len()),"player":b.player.as_ref().map(|p|{let p=p.lock().unwrap();serde_json::json!({"first_ready_render_ms":p.first_ready_render_ms,"ready":p.ready(),"time":p.number("time-pos"),"duration":p.number("duration"),"mute":p.text("mute"),"video":p.text("video-codec"),"audio":p.text("audio-codec-name"),"ao":p.text("current-ao"),"vo":p.text("current-vo")})})}));
        b.snapshot_image(
            ctx,
            &self.output.join(format!("{name}.png")).to_string_lossy(),
        );
    }
    pub fn tick(&mut self, b: &mut Browser, ctx: &egui::Context) {
        if self.finished {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(50));
        if self.began.elapsed() > Duration::from_secs(150) {
            self.finish(b, ctx, Some("Timed out"));
            return;
        }
        if self.at.elapsed() < Duration::from_millis(650) {
            return;
        }
        if self.phase == 0 {
            let Some(p) = &b.prepared else {
                return;
            };
            if !p.snapshot.finished {
                return;
            }
            let _ = std::fs::create_dir_all(&self.output);
            if self.cloud {
                if !media_browser::media_path::virtual_hint(&p.snapshot.root) {
                    self.finish(b, ctx, Some("Expected a virtual-volume path"));
                    return;
                }
                let Some(r) = (if let Some(source) = &self.requested_source {
                    // The runner resolves the explicit source outside the UI thread.
                    // Only a file already present in this authorized scan can be used.
                    p.snapshot
                        .files
                        .iter()
                        .find(|r| r.file.kind == MediaKind::Video && &r.file.path == source)
                } else {
                    p.snapshot
                        .files
                        .iter()
                        .filter(|r| r.file.kind == MediaKind::Video)
                        .min_by_key(|r| r.file.fingerprint.bytes)
                }) else {
                    self.finish(
                        b,
                        ctx,
                        Some("Requested video is absent from scanned directory"),
                    );
                    return;
                };
                self.ids = vec![r.id];
            } else {
                let root = p.snapshot.root.clone();
                let workspace =
                    std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("target"))
                        .unwrap();
                if !root.starts_with(&workspace) || !root.join(".edit-smoke-owned").is_file() {
                    self.finish(b, ctx, Some("Unmarked edit fixture refused"));
                    return;
                }
                self.ids = p.view.sequence.iter().copied().collect();
                if self.ids.len() != 2 {
                    self.finish(b, ctx, Some("Expected two generated images"));
                    return;
                }
                b.shortcuts.previous = media_browser::shortcuts::DetailKey::PageUp;
                b.shortcuts.next = media_browser::shortcuts::DetailKey::PageDown;
            }
            self.capture(b, ctx, "list");
            b.open_detail(self.ids[0]);
        } else if self.cloud {
            match self.phase {
                1 => {
                    if !b
                        .player
                        .as_ref()
                        .is_some_and(|p| p.lock().unwrap().number("time-pos").unwrap_or(0.) > 0.2)
                    {
                        return;
                    }
                    self.capture(b, ctx, "cloud-playing");
                }
                2 => {
                    if let Some(p) = &b.player {
                        let _ = p.lock().unwrap().set("pause", "yes");
                    }
                    b.request_seek(5., false);
                }
                3 => {
                    if b.seeks.target().is_some() || b.seek_flight.is_some() {
                        return;
                    }
                    self.capture(b, ctx, "cloud-seek");
                }
                4 => b.close_detail(),
                5 => self.capture(b, ctx, "cloud-returned"),
                _ => {
                    self.finish(b, ctx, None);
                    return;
                }
            }
        } else {
            match self.phase {
                1 => {
                    if b.image_region.is_none() {
                        return;
                    }
                    self.capture(b, ctx, "before");
                }
                2 => {
                    if self.controlled_focus {
                        self.desired_focus = false;
                        self.phase = 9;
                    }
                    self.key(egui::Key::PageDown, false);
                }
                3 => {
                    if b.detail.as_ref().map(|v| v.id) != Some(self.ids[1]) {
                        self.finish(b, ctx, Some("Configured next key failed"));
                        return;
                    }
                    self.capture(b, ctx, "next");
                }
                4 => {
                    if self.controlled_focus {
                        self.desired_focus = false;
                        self.phase = 11;
                    }
                    self.key(egui::Key::PageUp, false);
                }
                5 => {
                    if b.detail.as_ref().map(|v| v.id) != Some(self.ids[0]) {
                        self.finish(b, ctx, Some("Configured previous key failed"));
                        return;
                    }
                    self.capture(b, ctx, "previous");
                    b.close_detail();
                    b.selection = self.ids.iter().copied().collect();
                    b.begin_rotation(true, None);
                }
                6 => {
                    if b.rotation.is_some() || b.scan_busy || b.prepare_dirty {
                        return;
                    }
                    if !b
                        .rotation_message
                        .as_ref()
                        .is_some_and(|m| m.contains("失败 0 张"))
                    {
                        self.finish(b, ctx, Some("Bulk rotation failed"));
                        return;
                    }
                    self.capture(b, ctx, "saved");
                    b.rotation_message = None;
                    b.open_detail(self.ids[0]);
                }
                7 => {
                    let Some(r) = b
                        .prepared
                        .as_ref()
                        .and_then(|p| p.lookup.get(&self.ids[0]).map(|i| &p.snapshot.files[*i]))
                    else {
                        return;
                    };
                    if r.width != Some(480) || r.height != Some(640) || b.image_region.is_none() {
                        return;
                    }
                    self.capture(b, ctx, "rotated-jpeg");
                }
                8 => b.open_detail(self.ids[1]),
                9 => {
                    let Some(r) = b
                        .prepared
                        .as_ref()
                        .and_then(|p| p.lookup.get(&self.ids[1]).map(|i| &p.snapshot.files[*i]))
                    else {
                        return;
                    };
                    if r.width != Some(480) || r.height != Some(640) || b.image_region.is_none() {
                        return;
                    }
                    self.capture(b, ctx, "rotated-png");
                    self.finish(b, ctx, None);
                    return;
                }
                10 => {
                    if b.detail.as_ref().map(|v| v.id) != Some(self.ids[0]) {
                        self.finish(b, ctx, Some("Unfocused next key was not blocked"));
                        return;
                    }
                    self.capture(b, ctx, "background-next-blocked");
                    self.desired_focus = true;
                    self.key(egui::Key::PageDown, false);
                    self.phase = 2;
                }
                12 => {
                    if b.detail.as_ref().map(|v| v.id) != Some(self.ids[1]) {
                        self.finish(b, ctx, Some("Unfocused previous key was not blocked"));
                        return;
                    }
                    self.capture(b, ctx, "background-previous-blocked");
                    self.desired_focus = true;
                    self.key(egui::Key::PageUp, false);
                    self.phase = 4;
                }
                _ => {
                    self.finish(b, ctx, None);
                    return;
                }
            }
        }
        self.phase += 1;
        self.at = Instant::now();
    }
}
