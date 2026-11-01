//! Instance requests run from App::logic, including while the window is minimized.
use super::*;
pub(super) struct Bridge {
    pub primary: media_browser::instance::Primary,
    pending_root: Option<PathBuf>,
    activate: bool,
    received: u64,
    recorded: u64,
    last_root: Option<PathBuf>,
    rows: Vec<serde_json::Value>,
    report: Option<PathBuf>,
    visibility: Vec<serde_json::Value>,
    last_minimized: Option<bool>,
    minimize_after_open: bool,
    minimized_once: bool,
}
impl Browser {
    pub(crate) fn attach_instance(
        &mut self,
        primary: media_browser::instance::Primary,
        root: Option<PathBuf>,
        report: Option<PathBuf>,
        minimize_after_open: bool,
    ) {
        let ctx = self.ctx.clone();
        primary.set_waker(move || ctx.request_repaint());
        if let Some(root) = root {
            self.root_text = display_path(&root);
            self.initial_root = Some(root);
        }
        self.instance = Some(Bridge {
            primary,
            pending_root: None,
            activate: false,
            received: 0,
            recorded: u64::MAX,
            last_root: None,
            rows: vec![],
            report,
            visibility: vec![],
            last_minimized: None,
            minimize_after_open,
            minimized_once: false,
        });
    }
    pub(super) fn poll_instance(&mut self, ctx: &egui::Context) {
        let Some(bridge) = &mut self.instance else {
            return;
        };
        while let Some(request) = bridge.primary.poll() {
            bridge.received += 1;
            bridge.activate = true;
            if let Some(root) = request.directory {
                bridge.pending_root = Some(root);
            }
        }
        if self.trash_operation.is_some() {
            return;
        }
        if bridge.activate {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            bridge.activate = false;
        }
        if let Some(root) = bridge.pending_root.take() {
            self.initial_root = None;
            self.navigate(root, false);
        }
    }
    pub(super) fn record_instance(&mut self, ctx: &egui::Context) {
        let Some(bridge) = &mut self.instance else {
            return;
        };
        if bridge.report.is_none() {
            return;
        }
        let Some(p) = &self.prepared else {
            return;
        };
        if !p.snapshot.finished || p.snapshot.generation != self.generation {
            return;
        }
        if bridge.recorded == bridge.received && bridge.last_root.as_ref() == Some(&p.snapshot.root)
        {
            return;
        }
        bridge.recorded = bridge.received;
        bridge.last_root = Some(p.snapshot.root.clone());
        bridge.rows.push(serde_json::json!({"root":p.snapshot.root,"media":p.snapshot.files.len(),"received":bridge.received,"generation":self.generation,"focused":ctx.input(|i|i.focused),"minimized":ctx.input(|i|i.viewport().minimized),"elapsed_ms":self.started.elapsed().as_secs_f64()*1000.}));
        persist(bridge);
        if bridge.minimize_after_open && !bridge.minimized_once {
            bridge.minimized_once = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
            ctx.request_repaint();
        }
    }
    pub(super) fn record_instance_visibility(&mut self, ctx: &egui::Context) {
        let Some(bridge) = &mut self.instance else {
            return;
        };
        if bridge.report.is_none() {
            return;
        }
        let minimized = ctx.input(|i| i.viewport().minimized).unwrap_or(false);
        if bridge.last_minimized == Some(minimized) {
            return;
        }
        bridge.last_minimized = Some(minimized);
        bridge.visibility.push(serde_json::json!({"minimized":minimized,"elapsed_ms":self.started.elapsed().as_secs_f64()*1000.}));
        persist(bridge);
    }
}
fn persist(bridge: &Bridge) {
    // Opt-in diagnostics only. No report I/O on normal application frames.
    if let Some(report) = &bridge.report {
        let _=std::fs::write(report,serde_json::to_vec_pretty(&serde_json::json!({"pid":std::process::id(),"pipe":bridge.primary.pipe_name,"scope":"Actual native instance requests, prepared roots and backend minimized state; not physical menu input proof","rows":bridge.rows,"visibility":bridge.visibility})).unwrap());
    }
}
