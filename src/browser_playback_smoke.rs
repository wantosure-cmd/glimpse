//! Opt-in real-player diagnostics, explicitly not proof of physical input.
use super::*;
pub(super) struct PlaybackSmoke {
    step: u8,
    at: Instant,
    began: Instant,
    first: Option<MediaId>,
    second: Option<MediaId>,
    rows: Vec<serde_json::Value>,
}
impl PlaybackSmoke {
    pub(super) fn new() -> Self {
        Self {
            step: 0,
            at: Instant::now(),
            began: Instant::now(),
            first: None,
            second: None,
            rows: Vec::new(),
        }
    }
    fn capture(&mut self, b: &Browser, phase: &str) {
        let state=b.player.as_ref().map(|p| {
            let p=p.lock().unwrap();
            serde_json::json!({"ready":p.ready(),"source":p.source.as_ref().map(|p|p.display().to_string()),
                "time":p.number("time-pos"),"duration":p.number("duration"),"pause":p.text("pause"),
                "mute":p.text("mute"),"volume":p.number("volume"),"video_codec":p.text("video-codec"),
                "render_timing":p.render_timing_diagnostic()})
        });
        self.rows.push(serde_json::json!({"phase":phase,"player":state,"pending":b.seeks.target(),
            "media":b.player_media,"volume_preference":b.volume,"muted_preference":b.muted,"status":b.status,
            "snapshot_media":b.snapshot.as_ref().map(|s|s.files.len()),
            "prepared_media":b.prepared.as_ref().map(|p|p.snapshot.files.len()),
            "shown":b.prepared.as_ref().map(|p|serde_json::json!({"images":p.view.shown.images,"videos":p.view.shown.videos})),"view_settings":b.settings}));
    }
    pub(super) fn tick(&mut self, b: &mut Browser, ctx: &egui::Context) {
        ctx.request_repaint_after(Duration::from_millis(100));
        if self.began.elapsed() > Duration::from_secs(45) {
            self.finish(ctx, Some("Timed out"));
            return;
        }
        if self.at.elapsed() < Duration::from_secs(2) {
            return;
        }
        match self.step {
            0 => {
                let Some(prepared) = &b.prepared else {
                    return;
                };
                if !prepared.snapshot.finished {
                    return;
                }
                self.first = prepared
                    .snapshot
                    .files
                    .iter()
                    .find(|r| {
                        r.file
                            .path
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("mp4"))
                    })
                    .map(|r| r.id);
                self.second = prepared
                    .snapshot
                    .files
                    .iter()
                    .find(|r| {
                        r.file
                            .path
                            .extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("avi"))
                    })
                    .map(|r| r.id);
                let Some(id) = self.first else {
                    self.finish(ctx, Some("MP4 fixture missing"));
                    return;
                };
                b.open_detail(id);
            }
            1 => {
                if !b.player.as_ref().is_some_and(|p| p.lock().unwrap().ready()) {
                    return;
                }
                self.capture(b, "initial_detail");
                if let Some(p) = &b.player {
                    let _ = p.lock().unwrap().set("pause", "yes");
                }
                for _ in 0..5 {
                    b.request_seek(5., false);
                }
                self.capture(b, "five_pending_seeks");
            }
            2 => {
                self.capture(b, "paused_seek_result");
                b.snapshot_image(ctx, "p3-paused-seek.png");
            }
            3 => {
                b.muted = true;
                b.volume = 23.;
                if let Some(p) = &b.player {
                    let p = p.lock().unwrap();
                    let _ = p.set("mute", "yes");
                    let _ = p.set("volume", "23");
                }
                b.close_detail();
            }
            4 => b.open_detail(self.first.unwrap()),
            5 => {
                self.capture(b, "reopen_preferences_and_position");
                b.snapshot_image(ctx, "p3-detail-preferences.png");
            }
            6 => {
                let Some(id) = self.second else {
                    self.finish(ctx, Some("AVI fixture missing"));
                    return;
                };
                b.open_detail(id);
            }
            7 => {
                self.capture(b, "different_source");
                if let Some(root) = b.snapshot.as_ref().map(|s| s.root.clone()) {
                    b.navigate(root, false);
                }
            }
            8 => {
                if b.prepared.is_none() {
                    return;
                }
                b.open_detail(self.first.unwrap());
            }
            9 => {
                self.capture(b, "same_source_after_stop_and_reload");
                b.snapshot_image(ctx, "p3-reloaded-video.png");
            }
            10 => {
                b.close_detail();
            }
            _ => {
                self.finish(ctx, None);
                return;
            }
        }
        self.step += 1;
        self.at = Instant::now();
    }
    fn finish(&self, ctx: &egui::Context, error: Option<&str>) {
        let _ = std::fs::create_dir_all("artifacts");
        let result = serde_json::json!({"scope":"programmatic real native player actions, not physical input or hover timing proof","error":error,"rows":self.rows});
        let _ = std::fs::write(
            "artifacts/p3-playback-smoke.json",
            serde_json::to_vec_pretty(&result).unwrap(),
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
