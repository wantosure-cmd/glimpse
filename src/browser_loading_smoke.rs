//! Opt-in generated-fixture sharing-lock and unavailable-path recovery checks.
use super::*;
pub(super) struct LoadingSmoke {
    output: PathBuf,
    phase: u8,
    started: Instant,
    at: Instant,
    events: Vec<egui::Event>,
    rows: Vec<serde_json::Value>,
    source: Option<(MediaId, PathBuf, Fingerprint)>,
    generation: u64,
}
impl LoadingSmoke {
    pub fn new(output: PathBuf) -> Self {
        Self {
            output,
            phase: 0,
            started: Instant::now(),
            at: Instant::now(),
            events: vec![],
            rows: vec![],
            source: None,
            generation: 0,
        }
    }
    pub fn inject(&mut self, raw: &mut egui::RawInput) {
        raw.events.clear();
        raw.hovered_files.clear();
        raw.dropped_files.clear();
        raw.focused = true;
        raw.events.append(&mut self.events);
    }
    fn click(&mut self, r: Rect) {
        self.events.push(egui::Event::PointerMoved(r.center()));
        for pressed in [true, false] {
            self.events.push(egui::Event::PointerButton {
                pos: r.center(),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
        }
    }
    fn advance(&mut self) {
        self.phase += 1;
        self.at = Instant::now();
    }
    fn capture(&mut self, b: &Browser, ctx: &egui::Context, name: &str) {
        self.rows.push(serde_json::json!({"name":name,"phase":self.phase,"generation":b.generation,"scan_busy":b.scan_busy,"failures":b.failures.len(),"status":b.status,"scan_error":b.loading.scan_error,"watch_warning":b.watch_warning,"readiness":b.load_readiness_counts(ctx),"selection":b.selection.len(),"issues_button":b.loading.issues_button.map(|r|[r.min.x,r.min.y,r.width(),r.height()]),"retry_button":b.loading.retry_button.map(|r|[r.min.x,r.min.y,r.width(),r.height()])}));
        if let Some(row) = self.rows.last_mut() {
            row["pixels_per_point"] = serde_json::json!(ctx.pixels_per_point());
            row["card_rect"] = serde_json::json!(self
                .source
                .as_ref()
                .and_then(|(id, _, _)| b.list_regions.get(id))
                .map(|r| [r.min.x, r.min.y, r.width(), r.height()]));
        }
        b.snapshot_image(ctx, &format!("recovery-{name}.png"));
    }
    fn marker(&self, name: &str) {
        std::fs::write(self.output.join(name), b"owned diagnostic phase").unwrap();
    }
    fn finished(&mut self, ctx: &egui::Context, error: Option<&str>) {
        std::fs::write(self.output.join("report.json"),serde_json::to_vec_pretty(&serde_json::json!({"checks_pass":error.is_none(),"error":error,"rows":self.rows,"version":env!("CARGO_PKG_VERSION"),"scope":"Owned generated PNG. Win32 sharing-lock released without fingerprint changes; native controlled menu clicks/retry, path-unavailable simulation and restored path. Not physical input or actual drive/SMB disconnection."})).unwrap()).unwrap();
        self.phase = 255;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    fn clear_ready(&self, b: &Browser, ctx: &egui::Context) -> bool {
        let Some((id, path, fp)) = &self.source else {
            return false;
        };
        let (ready, clear, pending) = b.load_readiness_counts(ctx);
        !b.scan_busy
            && b.failures.is_empty()
            && b.loading.scan_error.is_none()
            && b.snapshot.as_ref().is_some_and(|s| {
                s.finished
                    && s.errors.is_empty()
                    && s.files
                        .iter()
                        .any(|r| r.id == *id && r.file.fingerprint == *fp && r.error.is_none())
            })
            && b.selection.contains(id)
            && b.features.favorites.paths.contains(path)
            && ready > 0
            && clear == b.list_regions.len()
            && pending == 0
    }
    pub fn tick(&mut self, b: &mut Browser, ctx: &egui::Context) {
        if self.phase == 255 {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(30));
        if self.started.elapsed() > Duration::from_secs(55) {
            self.capture(b, ctx, "timeout");
            self.finished(ctx, Some("recovery diagnostic timeout"));
            return;
        }
        if self.at.elapsed() < Duration::from_millis(350) {
            return;
        }
        // The real watcher may finish recovery before the queued menu click.
        // Accept only the same strict readiness/identity/selection conditions;
        // do not wait forever for an issue button that correctly disappeared.
        if matches!(self.phase, 6 | 7) && self.clear_ready(b, ctx) {
            self.capture(b, ctx, "path-restored-automatically");
            self.phase = 9;
            self.at = Instant::now();
            return;
        }
        match self.phase {
            0 => {
                if b.scan_busy || b.failures.is_empty() {
                    return;
                }
                let Some(record) = b.snapshot.as_ref().and_then(|s| s.files.first()) else {
                    return;
                };
                self.source = Some((
                    record.id,
                    record.file.path.clone(),
                    record.file.fingerprint.clone(),
                ));
                self.generation = b.generation;
                b.selection.insert(record.id);
                b.features.favorites.paths.insert(record.file.path.clone());
                self.capture(b, ctx, "locked-source-failed");
                self.advance();
            }
            1 => {
                let Some(r) = b.loading.issues_button else {
                    return;
                };
                self.click(r);
                self.advance();
            }
            2 => {
                let Some(r) = b.loading.retry_button else {
                    return;
                };
                self.capture(b, ctx, "failure-reason-menu");
                self.marker("release-request");
                self.phase = 10;
                self.at = Instant::now();
                let _ = r;
            }
            3 => {
                if b.generation <= self.generation || !self.clear_ready(b, ctx) {
                    return;
                }
                self.capture(b, ctx, "same-fingerprint-recovered");
                self.marker("disconnect-request");
                self.advance();
            }
            4 => {
                if !self.output.join("disconnected").exists() {
                    return;
                }
                b.refresh_in_place();
                self.advance();
            }
            5 => {
                if b.scan_busy {
                    return;
                }
                if b.loading.scan_error.is_none()
                    && !b.snapshot.as_ref().is_some_and(|s| !s.errors.is_empty())
                {
                    return;
                }
                if b.loading.issues_button.is_none() {
                    return;
                }
                self.capture(b, ctx, "path-unavailable");
                self.marker("restore-request");
                self.advance();
            }
            6 => {
                if !self.output.join("restored").exists() {
                    return;
                }
                let Some(r) = b.loading.issues_button else {
                    return;
                };
                self.click(r);
                self.advance();
            }
            7 => {
                let Some(r) = b.loading.retry_button else {
                    return;
                };
                self.capture(b, ctx, "path-retry-menu");
                self.click(r);
                self.advance();
            }
            8 => {
                if !self.clear_ready(b, ctx) {
                    return;
                }
                self.capture(b, ctx, "path-restored");
                self.advance();
            }
            9 => self.finished(ctx, None),
            10 => {
                if !self.output.join("released").exists() {
                    return;
                }
                let Some(r) = b.loading.retry_button else {
                    return;
                };
                self.click(r);
                self.phase = 3;
                self.at = Instant::now();
            }
            _ => {}
        }
    }
}
