//! Native GIF render/timing diagnostic. All UI actions are programmatic.
use super::*;
pub(super) struct GifSmoke {
    started: Instant,
    phase: u8,
    opened: bool,
    epoch: Instant,
    last: Option<u64>,
    rows: Vec<serde_json::Value>,
    completed: Vec<serde_json::Value>,
}
impl GifSmoke {
    pub(super) fn new() -> Self {
        Self {
            started: Instant::now(),
            phase: 0,
            opened: false,
            epoch: Instant::now(),
            last: None,
            rows: Vec::new(),
            completed: Vec::new(),
        }
    }
    pub(super) fn tick(&mut self, b: &mut Browser, ctx: &egui::Context) {
        ctx.request_repaint_after(Duration::from_millis(20));
        if self.started.elapsed() > Duration::from_secs(20) {
            self.finish(ctx, Some("GIF diagnostic timed out"));
            return;
        }
        if self.phase >= 3 {
            self.finish(ctx, None);
            return;
        }
        if !self.opened {
            let Some(prepared) = &b.prepared else {
                return;
            };
            if !prepared.snapshot.finished {
                return;
            }
            let name = ["循环.gif", "有限.gif", "单次.gif"][self.phase as usize];
            let Some(record) = prepared
                .snapshot
                .files
                .iter()
                .find(|r| r.file.path.file_name().is_some_and(|n| n == name))
            else {
                self.finish(ctx, Some("GIF sample missing"));
                return;
            };
            b.open_detail(record.id);
            self.epoch = Instant::now();
            self.opened = true;
            self.last = None;
        }
        if let Some(error) = &b.animation.error {
            self.finish(ctx, Some(error));
            return;
        }
        if let Some(sequence) = b.animation.sequence {
            if self.last != Some(sequence) {
                self.rows.push(serde_json::json!({"phase":self.phase,"sequence":sequence,"cycle":b.animation.cycle,
                    "elapsed_ms":self.epoch.elapsed().as_secs_f64()*1000.,"declared_ms":b.animation.last_delay.as_secs_f64()*1000.,
                    "texture_bytes":b.animation.texture_bytes(),"texture_id":b.animation.texture().map(|t|format!("{:?}",t.id()))}));
                self.last = Some(sequence);
                // First-cycle pixels are intentionally blended by the modal's
                // opening fade. Sample full-brightness colors in cycle two.
                if self.phase == 0 && (3..6).contains(&sequence) {
                    b.snapshot_image(ctx, &format!("p3-gif-frame-{}.png", sequence - 3));
                }
            }
        }
        if (self.phase == 0 && b.animation.sequence.is_some_and(|sequence| sequence >= 8))
            || b.animation.ended()
        {
            self.completed.push(serde_json::json!({"phase":self.phase,"last_sequence":b.animation.sequence,"ended":b.animation.ended(),"uploads":b.animation.uploaded}));
            self.phase += 1;
            self.opened = false;
            b.close_detail();
        }
    }
    fn finish(&self, ctx: &egui::Context, error: Option<&str>) {
        let result = serde_json::json!({"scope":"Actual native GIF frame uploads and callback timing; programmatic actions, not physical input","error":error,"frames":self.rows,"completed":self.completed});
        let _ = std::fs::write(
            "artifacts/p3-gif-smoke.json",
            serde_json::to_vec_pretty(&result).unwrap(),
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
