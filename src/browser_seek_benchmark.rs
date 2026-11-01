//! Opt-in, generated-fixture-only raw-input to matching GPU screenshot readback.
//! Does not measure physical input, display presentation, or decoder PTS alone.
use super::*;

#[derive(Clone, serde::Deserialize)]
struct Sample {
    target: f64,
    rgb: [u8; 3],
}

pub(super) struct SeekBenchmark {
    output: PathBuf,
    began: Instant,
    at: Instant,
    phase: u8,
    samples: Vec<Sample>,
    rows: Vec<serde_json::Value>,
    pointer: Option<Pos2>,
    events: Vec<egui::Event>,
    requested: Option<Instant>,
    target: f64,
    result: Arc<std::sync::Mutex<Option<serde_json::Value>>>,
    last_readback: Arc<std::sync::Mutex<serde_json::Value>>,
    finished: bool,
}

impl SeekBenchmark {
    pub(super) fn new(output: PathBuf) -> Self {
        Self {
            output,
            began: Instant::now(),
            at: Instant::now(),
            phase: 0,
            samples: vec![],
            rows: vec![],
            pointer: None,
            events: vec![],
            requested: None,
            target: 0.,
            result: Arc::new(std::sync::Mutex::new(None)),
            last_readback: Arc::new(std::sync::Mutex::new(serde_json::Value::Null)),
            finished: false,
        }
    }
    pub(super) fn inject(&mut self, raw: &mut egui::RawInput) {
        if self.finished {
            return;
        }
        raw.focused = true;
        if let Some(pos) = self.pointer {
            raw.events.push(egui::Event::PointerMoved(pos));
        } else {
            raw.events.push(egui::Event::PointerGone);
        }
        if self.requested.is_none()
            && self
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::PointerButton { pressed: true, .. }))
        {
            self.requested = Some(Instant::now());
        }
        raw.events.append(&mut self.events);
    }
    fn finish(&mut self, ctx: &egui::Context, error: Option<&str>) {
        self.finished = true;
        let mut times: Vec<_> = self
            .rows
            .iter()
            .filter_map(|r| r["elapsed_ms"].as_f64())
            .collect();
        times.sort_by(f64::total_cmp);
        let p95 = (times.len() == 20).then(|| times[18]);
        let report = serde_json::json!({"checks_pass":error.is_none() && times.len()==20,
            "error":error,"rows":self.rows,"sample_count":times.len(),"p95_ms":p95,
            "last_readback":self.last_readback.lock().unwrap().clone(),
            "physical_input_verified":false,"physical_presentation_verified":false,
            "scope":"Generated H.264 moving content; synthetic raw slider input through actual widgets, seek intent and libmpv to GPU screenshot readback. Matching segment color, observed position and 11-bit frame counter within two frames required. Includes screenshot overhead, excludes later PNG encoding. Not physical input/display presentation or a performance acceptance certificate."});
        if let Some(parent) = self.output.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(error) =
            std::fs::write(&self.output, serde_json::to_vec_pretty(&report).unwrap())
        {
            eprintln!("Seek benchmark report: {error}");
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    pub(super) fn tick(&mut self, b: &mut Browser, ctx: &egui::Context) {
        if self.finished {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(5));
        if self.began.elapsed() > Duration::from_secs(90)
            || self
                .requested
                .is_some_and(|at| at.elapsed() > Duration::from_secs(5))
        {
            self.finish(ctx, Some("Seek benchmark timed out; no sample discarded"));
            return;
        }
        match self.phase {
            0 => {
                let Some(prepared) = &b.prepared else {
                    return;
                };
                if !prepared.snapshot.finished || prepared.snapshot.files.is_empty() {
                    return;
                }
                let root = &prepared.snapshot.root;
                if !root.join(".seek-benchmark-owned").is_file()
                    || prepared.snapshot.files.len() != 1
                    || prepared.snapshot.files[0].file.kind != MediaKind::Video
                {
                    self.finish(ctx, Some("Dedicated one-video fixture required"));
                    return;
                }
                let samples = std::fs::read(root.join("seek-reference.json"))
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Vec<Sample>>(&bytes).ok());
                let Some(samples) = samples.filter(|s| {
                    s.len() == 20 && s.iter().all(|s| s.target.is_finite() && s.target > 0.)
                }) else {
                    self.finish(
                        ctx,
                        Some("Exactly twenty finite reference targets required"),
                    );
                    return;
                };
                self.samples = samples;
                b.open_detail(prepared.snapshot.files[0].id);
                self.phase = 1;
            }
            1 => {
                let Some(player) = &b.player else {
                    return;
                };
                let p = player.lock().unwrap();
                if !p.ready() || p.number("time-pos").unwrap_or(0.) < 0.2 {
                    return;
                }
                let _ = p.set("pause", "yes");
                self.phase = 2;
                self.at = Instant::now();
            }
            2 => {
                if self.rows.len() == 20 {
                    self.finish(ctx, None);
                    return;
                }
                if self.at.elapsed() < Duration::from_millis(150) {
                    return;
                }
                let Some(rect) = b.detail_layout.seek else {
                    return;
                };
                let Some(duration) = b
                    .player
                    .as_ref()
                    .and_then(|p| p.lock().unwrap().number("duration"))
                else {
                    return;
                };
                let sample = &self.samples[self.rows.len()];
                if sample.target >= duration {
                    self.finish(ctx, Some("Reference target exceeds duration"));
                    return;
                }
                let point = egui::pos2(
                    rect.left() + rect.width() * (sample.target / duration) as f32,
                    rect.center().y,
                );
                self.pointer = Some(point);
                self.result = Arc::new(std::sync::Mutex::new(None));
                self.last_readback = Arc::new(std::sync::Mutex::new(serde_json::Value::Null));
                self.requested = None;
                self.events.push(egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
                self.phase = 3;
            }
            3 => {
                if self.requested.is_none() {
                    return;
                }
                let Some(target) = b.seeks.target() else {
                    return;
                };
                if (target - self.samples[self.rows.len()].target).abs() > 1. {
                    self.finish(ctx, Some("Slider target differs from reference segment"));
                    return;
                }
                self.target = target;
                self.events.push(egui::Event::PointerButton {
                    pos: self.pointer.unwrap(),
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                });
                self.phase = 4;
            }
            _ => {
                // Keep the completion populated so late callbacks for this sample
                // cannot recapture/overwrite its PNG after the row is collected.
                let completed = self.result.lock().unwrap().clone();
                if let Some(row) = completed {
                    if row["screenshot_saved"] != true {
                        self.rows.push(row);
                        self.finish(ctx, Some("Matching frame screenshot could not be saved"));
                        return;
                    }
                    self.rows.push(row);
                    self.requested = None;
                    self.phase = 2;
                    self.at = Instant::now();
                    return;
                }
                let Some((_, rect)) = b.video_region else {
                    return;
                };
                let Some(player) = b.player.clone() else {
                    return;
                };
                let scale = ctx.pixels_per_point();
                let expected = self.samples[self.rows.len()].rgb;
                let desired = self.samples[self.rows.len()].target;
                let target = self.target;
                let began = self.requested.unwrap();
                let result = self.result.clone();
                let last_readback = self.last_readback.clone();
                let path = self
                    .output
                    .with_file_name(format!("seek-{:02}.png", self.rows.len()));
                let wake = ctx.clone();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |pixels| {
                    if result.lock().unwrap().is_some() { return; }
                    let x = (rect.center().x * scale).round() as usize;
                    let y = (rect.center().y * scale).round() as usize;
                    if x<2 || y<2 || x+2>=pixels.size[0] || y+2>=pixels.size[1] { return; }
                    let mut sums=[0u32;3];
                    for yy in y-2..=y+2 { for xx in x-2..=x+2 {
                        let p = pixels.pixels[yy*pixels.size[0]+xx].to_array();
                        for k in 0..3 { sums[k]+=u32::from(p[k]); }
                    }}
                    let observed = sums.map(|value|(value/25) as u8);
                    let mut frame_number = 0u32;
                    for bit in 0..11 {
                        let xx = ((rect.left()+rect.width()*(857.+20.*bit as f32)/1920.)*scale).round() as usize;
                        let yy = ((rect.top()+rect.height()*612./1080.)*scale).round() as usize;
                        if xx<1 || yy<1 || xx+1>=pixels.size[0] || yy+1>=pixels.size[1] { return; }
                        let mut value=0u32;
                        for y in yy-1..=yy+1 { for x in xx-1..=xx+1 {
                            let pixel=pixels.pixels[y*pixels.size[0]+x].to_array();
                            value += u32::from(pixel[0])+u32::from(pixel[1])+u32::from(pixel[2]);
                        }}
                        let level=value/27;
                        if level>208 { frame_number |= 1<<bit; }
                        else if level>=48 { return; }
                    }
                    let frame_time=f64::from(frame_number)/30.;
                    let position = player.lock().unwrap().number("time-pos").unwrap_or(-100.);
                    *last_readback.lock().unwrap() = serde_json::json!({"expected_rgb":expected,
                        "observed_rgb":observed,"observed_time":position,"slider_target":target,
                        "observed_frame_number":frame_number,"observed_frame_time":frame_time,
                        "video_region_pixels":[rect.left()*scale,rect.top()*scale,rect.right()*scale,rect.bottom()*scale],
                        "elapsed_ms":began.elapsed().as_secs_f64()*1000.});
                    if observed.iter().zip(expected).any(|(a,b)|a.abs_diff(b)>18) || (position-target).abs()>0.2
                        || (frame_time-target).abs() > 2./30. + 0.0001 { return; }
                    let elapsed = began.elapsed().as_secs_f64()*1000.;
                    let rgba: Vec<u8> = pixels.pixels.iter().flat_map(|p|p.to_array()).collect();
                    let saved = image::save_buffer(&path,&rgba,pixels.size[0] as u32,pixels.size[1] as u32,image::ColorType::Rgba8).is_ok();
                    *result.lock().unwrap()=Some(serde_json::json!({"elapsed_ms":elapsed,"reference_target":desired,
                        "slider_target":target,"observed_time":position,"expected_rgb":expected,"observed_rgb":observed,
                        "observed_frame_number":frame_number,"observed_frame_time":frame_time,
                        "video_region_pixels":[rect.left()*scale,rect.top()*scale,rect.right()*scale,rect.bottom()*scale],
                        "screenshot":path,"screenshot_saved":saved,"pixels":pixels.size}));
                    wake.request_repaint();
                })));
            }
        }
    }
}
