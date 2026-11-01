//! Opt-in diagnostic driver. UI callback intervals are not display presentation timestamps.
use eframe::egui;
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Serialize)]
pub struct Sample {
    elapsed_ms: f64,
    app_elapsed_ms: f64,
    interval_ms: f64,
    scroll_y: f32,
    viewport_points: [f32; 2],
    pixels_per_point: f32,
    native_pixels_per_point: Option<f32>,
    fullscreen: Option<bool>,
    monitor_points: Option<[f32; 2]>,
    texture_bytes: usize,
    textures: usize,
    decoded: u64,
    cache_hits: u64,
    failures: usize,
}

pub struct ScrollBenchmark {
    output: PathBuf,
    duration: Duration,
    created: Instant,
    ready_since: Option<Instant>,
    began: Option<Instant>,
    previous: Option<Instant>,
    offset: f32,
    direction: f32,
    samples: Vec<Sample>,
    finished: bool,
    controlled_4k: bool,
    fling: bool,
    pub hover_video: bool,
    pub legacy_sizing_priority: bool,
    pub legacy_geometry_priority: bool,
    pub legacy_near_geometry_priority: bool,
    pub legacy_jpeg_preview: bool,
    pub experimental_jpeg_direct: bool,
    pub legacy_preparation: bool,
    jump_to_bottom: bool,
    jump_target: Option<f32>,
    screenshot_worker: bool,
    screenshot_timing: Arc<Mutex<serde_json::Value>>,
}

impl ScrollBenchmark {
    pub fn from_args(args: &[String]) -> Option<Self> {
        let index = args.iter().position(|s| s == "--benchmark-scroll")?;
        let seconds: f64 = args.get(index + 1)?.parse().ok()?;
        if !seconds.is_finite() || !(5.0..=600.0).contains(&seconds) {
            return None;
        }
        let output = args
            .iter()
            .position(|s| s == "--benchmark-output")
            .and_then(|i| args.get(i + 1))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("artifacts/p2-scroll.json"));
        Some(Self {
            output,
            duration: Duration::from_secs_f64(seconds),
            created: Instant::now(),
            ready_since: None,
            began: None,
            previous: None,
            offset: 0.,
            direction: 1.,
            samples: Vec::new(),
            finished: false,
            controlled_4k: args.iter().any(|s| s == "--benchmark-surface-4k"),
            fling: args.iter().any(|s| s == "--benchmark-scroll-fling"),
            hover_video: args.iter().any(|s| s == "--benchmark-hover-video"),
            legacy_sizing_priority: args
                .iter()
                .any(|s| s == "--benchmark-legacy-sizing-priority"),
            legacy_geometry_priority: args
                .iter()
                .any(|s| s == "--benchmark-legacy-geometry-priority"),
            legacy_near_geometry_priority: args
                .iter()
                .any(|s| s == "--benchmark-legacy-near-geometry-priority"),
            legacy_jpeg_preview: args.iter().any(|s| s == "--benchmark-legacy-jpeg-preview"),
            experimental_jpeg_direct: args
                .iter()
                .any(|s| s == "--benchmark-experimental-jpeg-direct"),
            jump_to_bottom: args.iter().any(|s| s == "--benchmark-jump-to-bottom"),
            jump_target: None,
            screenshot_worker: args.iter().any(|s| s == "--benchmark-screenshot-worker"),
            screenshot_timing: Arc::new(Mutex::new(serde_json::Value::Null)),
            legacy_preparation: args.iter().any(|s| s == "--benchmark-legacy-preparation"),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn tick(
        &mut self,
        ctx: &egui::Context,
        ready: bool,
        media: usize,
        height: f32,
        viewport_height: f32,
        scroll_y: f32,
        texture_bytes: usize,
        textures: usize,
        decoded: u64,
        cache_hits: u64,
        failures: usize,
        app_elapsed_ms: f64,
    ) -> Option<f32> {
        if self.finished {
            return None;
        }
        if self.controlled_4k {
            let scale_changed = (ctx.pixels_per_point() - 1.).abs() > 0.001;
            let size_changed = (ctx.content_rect().width() - 3840.).abs() > 1.
                || (ctx.content_rect().height() - 2160.).abs() > 1.;
            if scale_changed || size_changed {
                if self.began.is_some() {
                    self.finish(
                        ctx,
                        media,
                        Some("Controlled viewport or DPI changed during run"),
                    );
                    return None;
                }
                self.ready_since = None;
                if scale_changed {
                    // Normalize only this diagnostic. Wait for the next input
                    // frame before requesting a physical4K window, avoiding an
                    // initial8K allocation on a200%-scaled desktop.
                    ctx.set_pixels_per_point(1.);
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
                        3840., 2160.,
                    )));
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        3840., 2160.,
                    )));
                }
                ctx.request_repaint();
                return None;
            }
        }
        ctx.request_repaint();
        let now = Instant::now();
        if self.began.is_none() {
            if ready {
                let ready_since = self.ready_since.get_or_insert(now);
                // Exclude initial enumeration/layout and give the first screen ten seconds.
                if now.duration_since(*ready_since) >= Duration::from_secs(10) {
                    self.began = Some(now);
                    self.previous = Some(now);
                    self.offset = if self.jump_to_bottom {
                        (height - viewport_height).max(0.)
                    } else {
                        0.
                    };
                    self.jump_target = self.jump_to_bottom.then_some(self.offset);
                    return Some(self.offset);
                }
            } else {
                self.ready_since = None;
            }
            if self.created.elapsed() < Duration::from_secs(90) {
                return None;
            }
            self.finish(ctx, media, Some("No complete layout within 90 seconds"));
            return None;
        }
        let began = self.began.unwrap();
        let interval = now
            .duration_since(self.previous.replace(now).unwrap())
            .as_secs_f64();
        let viewport = ctx.content_rect().size();
        let (fullscreen, monitor_points) = ctx.input(|i| {
            (
                i.viewport().fullscreen,
                i.viewport().monitor_size.map(|s| [s.x, s.y]),
            )
        });
        self.samples.push(Sample {
            elapsed_ms: now.duration_since(began).as_secs_f64() * 1000.,
            app_elapsed_ms,
            interval_ms: interval * 1000.,
            scroll_y,
            viewport_points: [viewport.x, viewport.y],
            pixels_per_point: ctx.pixels_per_point(),
            native_pixels_per_point: ctx.native_pixels_per_point(),
            fullscreen,
            monitor_points,
            texture_bytes,
            textures,
            decoded,
            cache_hits,
            failures,
        });
        if self.samples.len() == 120 {
            let path = self.output.with_extension("png");
            let worker = self.screenshot_worker;
            let timing = self.screenshot_timing.clone();
            let requested = Instant::now();
            *timing.lock().unwrap() = serde_json::json!({"requested_app_ms":app_elapsed_ms,
                "sample_number":120,"worker":worker,"complete":false});
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                egui::ScreenshotCallback::new(move |pixels| {
                    let callback_delay_ms = requested.elapsed().as_secs_f64()*1000.;
                    let encode = move || {
                        let began = Instant::now();
                        let rgba: Vec<u8> = pixels.pixels.iter().flat_map(|p| p.to_array()).collect();
                        let conversion_ms = began.elapsed().as_secs_f64()*1000.;
                        let png_began = Instant::now();
                        let result = image::save_buffer(&path, &rgba, pixels.size[0] as u32,
                            pixels.size[1] as u32, image::ColorType::Rgba8);
                        *timing.lock().unwrap() = serde_json::json!({"requested_app_ms":app_elapsed_ms,
                            "sample_number":120,"worker":worker,"complete":true,
                            "callback_delay_ms":callback_delay_ms,"conversion_ms":conversion_ms,
                            "png_encode_write_ms":png_began.elapsed().as_secs_f64()*1000.,
                            "encode_total_ms":began.elapsed().as_secs_f64()*1000.,
                            "width":pixels.size[0],"height":pixels.size[1],
                            "error":result.err().map(|e|e.to_string())});
                    };
                    if worker { std::thread::spawn(encode); } else { encode(); }
                }),
            ));
        }
        if now.duration_since(began) >= self.duration {
            self.finish(ctx, media, None);
            return None;
        }
        let max = (height - viewport_height).max(0.);
        // One jump, then normal browser anchors handle async geometry.
        // Chasing the growing maximum would conceal anchor defects.
        if self.jump_to_bottom {
            return None;
        }
        if self.fling {
            // Deterministic fast swipe followed by a reading pause. Derive the
            // position from time so slow frames cannot shorten the swipe.
            let elapsed = now.duration_since(began).as_secs_f64();
            let travel = |time: f64| {
                let active = (time - 2.).max(0.);
                (active / 3.).floor() * 5400. + (active % 3.).min(0.3) * 18000.
            };
            let distance = (travel(elapsed) - travel(elapsed - interval)).max(0.) as f32;
            // Hold the actual stop position while reading. Recomputing an
            // absolute triangle against growing layout height moves a paused
            // viewport and would measure the driver rather than the product.
            if max > 0. && distance > 0. {
                let previous = if self.direction > 0. {
                    self.offset
                } else {
                    2. * max - self.offset
                };
                let phase = (previous + distance) % (2. * max);
                self.offset = phase.min(2. * max - phase);
                self.direction = if phase <= max { 1. } else { -1. };
            }
            return Some(self.offset);
        }
        self.offset += interval as f32 * 1800. * self.direction;
        if self.offset >= max {
            self.offset = max;
            self.direction = -1.;
        }
        if self.offset <= 0. {
            self.offset = 0.;
            self.direction = 1.;
        }
        Some(self.offset)
    }

    fn finish(&mut self, ctx: &egui::Context, media: usize, error: Option<&str>) {
        self.finished = true;
        let mut intervals: Vec<_> = self.samples.iter().map(|s| s.interval_ms).collect();
        intervals.sort_by(f64::total_cmp);
        let percentile = |p: f64| {
            intervals
                .get(((intervals.len() as f64 * p).ceil() as usize).saturating_sub(1))
                .copied()
        };
        let result = serde_json::json!({
            "scope": "release native UI callback intervals during programmatic scrolling; not physical input or display presentation proof",
            "error": error, "media": media,
            "pattern":if self.jump_to_bottom {"jump_to_current_bottom"} else if self.fling {"fling_then_read"} else {"continuous"},
            "jump_target_scroll_y":self.jump_target,
            "speed_points_per_second":if self.jump_to_bottom {0} else if self.fling {18000} else {1800},
            "fling_initial_pause_seconds":if self.fling {Some(2.)} else {None},
            "fling_cycle_seconds":if self.fling {Some(3.)} else {None},
            "fling_motion_seconds":if self.fling {Some(0.3)} else {None},
            "input_mode":if self.hover_video {"controlled-visible-video-hover"} else {"pointer-neutral-unfocused"},
            "sizing_priority_scope":if self.legacy_sizing_priority {"label-only"} else {"possible-next-card-column-span"},
            "view_preparation_mode":if self.legacy_preparation {"rebuild-view"} else {"reuse-validated-view"},
            "visible_geometry_priority":if self.legacy_geometry_priority {2} else {1},
            "near_geometry_priority":if self.legacy_near_geometry_priority {4} else {3},
            "list_jpeg_preview_mode":if self.experimental_jpeg_direct && !self.legacy_jpeg_preview {"direct-clear"} else {"progressive"},
            "warmup_seconds":10, "requested_seconds":self.duration.as_secs_f64(),
            "interval_p50_ms":percentile(0.5),"interval_p95_ms":percentile(0.95),"interval_p99_ms":percentile(0.99),
            "samples":self.samples,
            "screenshot_encoding_mode":if self.screenshot_worker {"one-shot-diagnostic-worker"} else {"legacy-ui-callback"},
            "screenshot_timing":*self.screenshot_timing.lock().unwrap(),
        });
        if let Some(parent) = self.output.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_vec(&result)
            .map_err(|e| e.to_string())
            .and_then(|data| std::fs::write(&self.output, data).map_err(|e| e.to_string()))
        {
            Ok(()) => {}
            Err(error) => eprintln!("benchmark write failed: {error}"),
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

/// Controlled load benchmark input only; never modifies OS focus or cursor.
pub fn isolate_scroll_benchmark_input(raw: &mut egui::RawInput) {
    raw.focused = false;
    raw.events.clear();
    raw.events
        .push(egui::Event::ModifiersChanged(egui::Modifiers::default()));
    raw.events.push(egui::Event::PointerGone);
    raw.events.push(egui::Event::WindowFocused(false));
    raw.dropped_files.clear();
    raw.hovered_files.clear();
}

#[cfg(test)]
mod controlled_input_tests {
    #[test]
    fn scroll_load_protocol_cannot_inherit_pointer_or_focus() {
        let mut raw = eframe::egui::RawInput::default();
        raw.focused = true;
        raw.events
            .push(eframe::egui::Event::PointerMoved(eframe::egui::pos2(
                400., 600.,
            )));
        raw.events.push(eframe::egui::Event::WindowFocused(true));
        super::isolate_scroll_benchmark_input(&mut raw);
        assert!(!raw.focused);
        assert_eq!(
            raw.events,
            vec![
                eframe::egui::Event::ModifiersChanged(eframe::egui::Modifiers::default()),
                eframe::egui::Event::PointerGone,
                eframe::egui::Event::WindowFocused(false)
            ]
        );
    }
}
