//! Opt-in actual browser codec diagnostics. It never routes physical OS input.
use super::*;

pub(super) struct FormatSmoke {
    output: PathBuf,
    began: Instant,
    at: Instant,
    phase: u8,
    id: Option<MediaId>,
    video: bool,
    motion_probe: bool,
    motion_captured: bool,
    expected_seek: Option<f64>,
    rows: Vec<serde_json::Value>,
    finished: bool,
    pub(super) list_region: Option<Rect>,
}
impl FormatSmoke {
    pub(super) fn new(output: PathBuf) -> Self {
        Self {
            output,
            began: Instant::now(),
            at: Instant::now(),
            phase: 0,
            id: None,
            video: false,
            motion_probe: false,
            motion_captured: false,
            expected_seek: None,
            rows: vec![],
            finished: false,
            list_region: None,
        }
    }
    pub(super) fn with_motion_probe(mut self, enabled: bool) -> Self {
        self.motion_probe = enabled;
        self
    }
    pub(super) fn isolate_pointer(&self, raw: &mut egui::RawInput) {
        // Only --format-smoke: preserve actual focus, don't move the OS cursor.
        // Tooltip/hover playback must not cover or replace the pixels sampled
        // by the generated-fixture orientation and frame-motion checks.
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
    fn capture(&mut self, b: &Browser, ctx: &egui::Context, phase: &str) {
        let record = b.prepared.as_ref().and_then(|p| p.snapshot.files.first());
        let player = b.player.as_ref().map(|p| {
            let p = p.lock().unwrap();
            serde_json::json!({
            "first_ready_render_ms":p.first_ready_render_ms,"ready":p.ready(),"time":p.number("time-pos"),"duration":p.number("duration"),
            "pause":p.text("pause"),"mute":p.text("mute"),"audio":p.text("audio-codec-name"),
            "video":p.text("video-codec"),"ao":p.text("current-ao"),"vo":p.text("current-vo"),
            "hwdec":p.text("hwdec-current")})
        });
        let rect = if b.detail.is_none() {
            self.list_region
        } else if self.video {
            b.video_region.map(|(_, r)| r)
        } else {
            b.image_region
        };
        self.rows.push(serde_json::json!({"phase":phase,"elapsed_ms":self.began.elapsed().as_secs_f64()*1000.,
            "record":record.map(|r|serde_json::json!({"path":r.file.path,"width":r.width,"height":r.height,"duration_ms":r.duration_ms})),
            "textures":b.textures.values().map(|t|t.texture.size()).collect::<Vec<_>>(),
            "texture_bytes":b.texture_bytes,"failures":b.failures.values().collect::<Vec<_>>(),
            "region":rect.map(|r|[r.min.x,r.min.y,r.max.x,r.max.y]),"scale":ctx.pixels_per_point(),
            "image_viewport":b.detail_layout.image_viewport,"image_viewport_ready":b.detail_layout.image_viewport_ready,"image_full_rect":b.detail_layout.image_full_rect.map(|r|[r.min.x,r.min.y,r.max.x,r.max.y]),"zoom":b.detail.as_ref().map(|d|d.zoom),"player":player,"seek_target":self.expected_seek,"status":b.status}));
    }
    fn snapshot(&self, b: &Browser, ctx: &egui::Context, name: &str) {
        b.snapshot_image(ctx, &self.output.join(name).to_string_lossy());
    }
    pub(super) fn tick(&mut self, b: &mut Browser, ctx: &egui::Context) {
        if self.finished {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(30));
        if self.began.elapsed() > Duration::from_secs(35) {
            self.finish(b, ctx, Some("Format diagnostic timed out"));
            return;
        }
        if self.at.elapsed() < Duration::from_millis(500) {
            return;
        }
        let long_png = b
            .prepared
            .as_ref()
            .and_then(|p| p.snapshot.files.first())
            .is_some_and(|r| {
                r.file
                    .path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("png"))
                    && r.width.unwrap_or(0).max(r.height.unwrap_or(0)) > 16384
            });
        match self.phase {
            0 => {
                let Some(p) = &b.prepared else {
                    return;
                };
                if !p.snapshot.finished {
                    return;
                }
                if p.snapshot.files.len() != 1 {
                    self.finish(b, ctx, Some("Exactly one media fixture is required"));
                    return;
                }
                let r = &p.snapshot.files[0];
                if !b.failures.is_empty() {
                    self.finish(b, ctx, Some("Thumbnail decoding failed"));
                    return;
                }
                if b.textures.is_empty() || r.width.is_none() || r.height.is_none() {
                    return;
                }
                self.id = Some(r.id);
                self.video = r.file.kind == MediaKind::Video;
                let _ = std::fs::create_dir_all(&self.output);
                self.capture(b, ctx, "list_thumbnail");
                self.snapshot(b, ctx, "list.png");
            }
            1 => b.open_detail(self.id.unwrap()),
            2 => {
                if self.video {
                    if !b.player.as_ref().is_some_and(|p| {
                        let p = p.lock().unwrap();
                        p.ready() && p.number("time-pos").is_some_and(|s| s > 0.5)
                    }) {
                        return;
                    }
                } else if b.image_region.is_none() {
                    return;
                }
                self.capture(b, ctx, "detail_fit");
                self.snapshot(b, ctx, "detail.png");
            }
            3 => {
                if self.video && self.motion_probe && !self.motion_captured {
                    // Capture while still playing, before pause or any seek.
                    self.capture(b, ctx, "playing_pixels");
                    self.snapshot(b, ctx, "playing.png");
                    self.motion_captured = true;
                    self.at = Instant::now();
                    return;
                }
                if self.video {
                    self.capture(b, ctx, "playing_advanced");
                    if let Some(p) = &b.player {
                        let _ = p.lock().unwrap().set("pause", "yes");
                    }
                    b.request_seek(30., false);
                    self.expected_seek = b.seeks.target();
                } else if let Some(v) = &mut b.detail {
                    v.zoom *= 2.;
                    v.native_pending = true;
                }
            }
            4 => {
                if self.video {
                    let Some(target) = self.expected_seek else {
                        self.finish(b, ctx, Some("Seek request missing"));
                        return;
                    };
                    if !b.player.as_ref().is_some_and(|p| {
                        p.lock()
                            .unwrap()
                            .number("time-pos")
                            .is_some_and(|s| (s - target).abs() < 0.2)
                    }) {
                        return;
                    }
                    self.capture(b, ctx, "paused_seek");
                } else {
                    let side = b
                        .prepared
                        .as_ref()
                        .and_then(|p| p.snapshot.files.first())
                        .map(|r| r.width.unwrap_or(0).max(r.height.unwrap_or(0)))
                        .unwrap_or(0) as usize;
                    if if long_png {
                        !b.detail_layout.image_viewport_ready
                    } else {
                        !b.textures
                            .values()
                            .any(|t| t.texture.size().into_iter().max().unwrap_or(0) >= side)
                    } {
                        return;
                    }
                    self.capture(b, ctx, "detail_zoom");
                }
                self.snapshot(b, ctx, "after.png");
            }
            5 => {
                if long_png {
                    let height = b.prepared.as_ref().unwrap().snapshot.files[0]
                        .height
                        .unwrap() as f32;
                    if let (Some(viewer), Some(canvas)) = (&mut b.detail, b.detail_layout.canvas) {
                        viewer.pan.y = (height / ctx.pixels_per_point() - canvas.height()) / 2.;
                    }
                } else {
                    b.close_detail();
                }
            }
            6 => {
                if long_png {
                    if !b.detail_layout.image_viewport_ready
                        || b.detail_layout.image_viewport.is_none_or(|r| r.y != 0)
                    {
                        return;
                    }
                    self.capture(b, ctx, "detail_top");
                    self.snapshot(b, ctx, "top.png");
                    let height = b.prepared.as_ref().unwrap().snapshot.files[0]
                        .height
                        .unwrap() as f32;
                    if let (Some(viewer), Some(canvas)) = (&mut b.detail, b.detail_layout.canvas) {
                        viewer.pan.y = -(height / ctx.pixels_per_point() - canvas.height()) / 2.;
                    }
                } else {
                    self.capture(b, ctx, "returned_list");
                    self.snapshot(b, ctx, "returned.png");
                }
            }
            7 if long_png => {
                let height = b.prepared.as_ref().unwrap().snapshot.files[0]
                    .height
                    .unwrap();
                if !b.detail_layout.image_viewport_ready
                    || b.detail_layout
                        .image_viewport
                        .is_none_or(|r| r.y + r.height != height)
                {
                    return;
                }
                self.capture(b, ctx, "detail_bottom");
                self.snapshot(b, ctx, "bottom.png");
                b.close_detail();
            }
            8 if long_png => {
                self.capture(b, ctx, "returned_list");
                self.snapshot(b, ctx, "returned.png");
            }
            _ => {
                self.finish(b, ctx, None);
                return;
            }
        }
        self.phase += 1;
        self.at = Instant::now();
    }
    fn finish(&mut self, b: &Browser, ctx: &egui::Context, error: Option<&str>) {
        self.finished = true;
        self.capture(b, ctx, "finished");
        let _ = std::fs::create_dir_all(&self.output);
        let data = serde_json::json!({"scope":"Actual native browser thumbnail/detail/render/zoom/player programmatic diagnostics; not physical input or professional color calibration", "pointer_mode":"neutral-no-os-cursor-movement","focus_mode":"observed-os-focus","error":error,"rows":self.rows});
        let _ = std::fs::write(
            self.output.join("report.json"),
            serde_json::to_vec_pretty(&data).unwrap(),
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
