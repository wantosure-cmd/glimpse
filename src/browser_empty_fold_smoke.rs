//! Native generated-fixture preference, automatic-fold and scrollbar interaction probe.
use super::*;
pub(super) struct EmptyFoldSmoke {
    output: PathBuf,
    disabled: bool,
    phase: u8,
    started: Instant,
    at: Instant,
    pointer: Option<Pos2>,
    events: Vec<egui::Event>,
    rows: Vec<serde_json::Value>,
    old_scroll: f32,
}
impl EmptyFoldSmoke {
    pub(super) fn new(output: PathBuf, disabled: bool) -> Self {
        Self {
            output,
            disabled,
            phase: 0,
            started: Instant::now(),
            at: Instant::now(),
            pointer: None,
            events: vec![],
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
    fn click(&mut self, p: Pos2) {
        self.pointer = Some(p);
        for pressed in [true, false] {
            self.events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
        }
    }
    fn capture(&mut self, b: &Browser, ctx: &egui::Context, name: &str) {
        let rect = |r: Rect| [r.left(), r.top(), r.width(), r.height()];
        self.rows.push(serde_json::json!({"name":name,"enabled":b.auto_fold_empty,"automatic_count":b.automatic_collapsed.len(),"manual_expanded":b.auto_expanded,"manual_collapsed":b.collapsed,"headers":b.prepared.as_ref().map(|p|p.layout.headers.iter().filter(|h|h.kind.is_none()).map(|h|serde_json::json!({"path":h.directory,"collapsed":h.collapsed})).collect::<Vec<_>>()),"scroll":b.scroll_y,"track":b.list_scrollbar.map(rect),"viewport":b.list_viewport.map(rect),"pixels_per_point":ctx.pixels_per_point(),"content_height":b.prepared.as_ref().map(|p|p.layout.height),"configured_handle_min":44}));
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
        if self.started.elapsed() > Duration::from_secs(120) {
            self.capture(b, ctx, "timeout");
            self.finish(ctx, Some("automatic fold/scrollbar timeout"));
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
        let root = p.snapshot.root.clone();
        let empty = root.join("01-empty-app");
        let child = empty.join("_internal");
        let leaf = child.join("vendored/requests/packages/certifi");
        let media_parent = root.join("00-media");
        let has = |path: &Path| {
            p.layout
                .headers
                .iter()
                .any(|h| h.kind.is_none() && h.directory == path)
        };
        if self.disabled {
            if b.auto_fold_empty || !b.automatic_collapsed.is_empty() || !has(&leaf) {
                self.finish(ctx, Some("disabled preference did not persist"));
                return;
            }
            self.capture(b, ctx, "persisted-disabled");
            self.disabled = false;
            self.phase = 14;
            self.at = Instant::now();
            return;
        }
        match self.phase {
            0 => {
                if !b.auto_fold_empty
                    || !b.automatic_collapsed.contains(&empty)
                    || has(&child)
                    || !has(&media_parent)
                    || b.folder_collapsed(&media_parent)
                {
                    self.finish(
                        ctx,
                        Some("default empty fold or media-ancestor preservation failed"),
                    );
                    return;
                }
                if !p.layout.sizing_rows.is_empty() {
                    b.scroll_override = Some(p.layout.height);
                    return;
                }
                b.scroll_override = Some(0.);
                self.next();
            }
            1 => {
                self.capture(b, ctx, "default-folded-scrollbar");
                self.old_scroll = b.scroll_y;
                self.next();
            }
            2 => {
                let Some(track) = b.list_scrollbar else {
                    return;
                };
                let pos = egui::pos2(track.center().x, track.top() + 22.);
                self.pointer = Some(pos);
                self.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                });
                self.next();
            }
            3 => {
                let track = b.list_scrollbar.unwrap();
                self.pointer = Some(egui::pos2(track.center().x, track.top() + 220.));
                self.next();
            }
            4 => {
                let pos = self.pointer.unwrap();
                self.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                });
                self.next();
            }
            5 => {
                if b.scroll_y <= self.old_scroll + 100. {
                    self.finish(ctx, Some("wide scrollbar handle drag failed"));
                    return;
                }
                self.capture(b, ctx, "scrollbar-dragged");
                b.locate_folder(&empty);
                self.next();
            }
            6 => {
                let Some(r) = b.folder_toggle_regions.get(&empty) else {
                    return;
                };
                self.click(r.center());
                self.next();
            }
            7 => {
                if b.folder_collapsed(&empty) || !b.auto_expanded.contains(&empty) || !has(&child) {
                    self.finish(ctx, Some("manual expansion of automatic fold failed"));
                    return;
                }
                self.capture(b, ctx, "manual-empty-expanded");
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(820., 720.)));
                self.next();
            }
            8 => {
                if b.folder_collapsed(&empty) || !has(&child) {
                    self.finish(ctx, Some("resize reset manual expansion"));
                    return;
                }
                self.capture(b, ctx, "manual-expansion-survives-resize");
                b.workspace.category = workspace_ui::Category::General;
                b.workspace.settings_open = true;
                self.next();
            }
            9 => {
                let Some(r) = b.workspace.auto_fold_checkbox else {
                    return;
                };
                self.click(r.center());
                self.next();
            }
            10 => {
                if b.auto_fold_empty {
                    self.finish(
                        ctx,
                        Some("settings checkbox did not disable automatic folds"),
                    );
                    return;
                }
                b.workspace.settings_open = false;
                self.next();
            }
            11 => {
                if !b.automatic_collapsed.is_empty() || !has(&leaf) {
                    self.finish(
                        ctx,
                        Some("turning off preference did not restore empty descendants"),
                    );
                    return;
                }
                self.capture(b, ctx, "automatic-fold-disabled");
                self.phase = 14;
                self.at = Instant::now();
            }
            14 => self.finish(ctx, None),
            _ => {}
        }
    }
    fn finish(&mut self, ctx: &egui::Context, error: Option<&str>) {
        let _ = std::fs::create_dir_all(&self.output);
        let _=std::fs::write(self.output.join("report.json"),serde_json::to_vec_pretty(&serde_json::json!({"checks_pass":error.is_none(),"error":error,"phase":self.phase,"rows":self.rows,"scope":"Native controlled input/FBO, generated media only; default empty-subtree folding, manual override/resize, real settings checkbox, saved preference and real scrollbar drag. Not physical input/full performance certification."})).unwrap());
        self.phase = 255;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
