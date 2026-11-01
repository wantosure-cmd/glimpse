//! P0 executable: validates native rendering and decoding before the full browser UI.
use crate::playback::Player;
use eframe::egui;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

pub fn probe_video(path: &Path) -> Result<(), String> {
    let mut player = Player::new(None, None)?;
    player.load(path)?;
    let start = Instant::now();
    while player.number("duration").is_none() && start.elapsed() < Duration::from_secs(15) {
        std::thread::sleep(Duration::from_millis(20));
    }
    player.set("pause", "yes")?;
    let duration = player
        .number("duration")
        .ok_or(media_browser::ui_text!(msg_bbbdf0080375))?;
    let mut rows = Vec::new();
    for target in [5.0_f64, 35., 30., 0.] {
        let target = target.min(duration - 0.1).max(0.);
        let began = Instant::now();
        player.seek(target)?;
        let mut actual = player.number("time-pos");
        while actual.is_none_or(|n| (n - target).abs() > 0.15)
            && began.elapsed() < Duration::from_secs(5)
        {
            std::thread::sleep(Duration::from_millis(10));
            actual = player.number("time-pos");
        }
        rows.push(serde_json::json!({"requested":target,"actual":actual,"elapsed_ms":began.elapsed().as_millis(),"pass":actual.is_some_and(|n|(n-target).abs()<0.15)}));
    }
    println!(
        "{}",
        serde_json::json!({"file":path.display().to_string(),"duration":duration,"video_codec":player.text("video-codec"),"audio_codec":player.text("audio-codec-name"),"mpv":player.text("mpv-version"),"seeks":rows,"scope":"null video/audio output; decoding and seek only, not GPU/audibility proof"})
    );
    Ok(())
}

type Proc = Arc<dyn Fn(&std::ffi::CStr) -> *const std::ffi::c_void + Send + Sync>;
pub struct Prototype {
    started: Instant,
    first_ui: bool,
    proc: Proc,
    player: Option<Arc<Mutex<Player>>>,
    video_path: String,
    heic_path: String,
    status: String,
    detail: bool,
    texture: Option<egui::TextureHandle>,
    decoded: mpsc::Receiver<Result<(String, image::RgbaImage), String>>,
    tx: mpsc::Sender<Result<(String, image::RgbaImage), String>>,
    paused: bool,
    volume: f64,
    pending_seek: Option<f64>,
    smoke: bool,
    smoke_step: u8,
    smoke_at: Instant,
    smoke_rows: Vec<serde_json::Value>,
    screenshot_at: Option<Instant>,
    quit_after: Option<Duration>,
}
impl Prototype {
    pub fn new(cc: &eframe::CreationContext<'_>, args: Vec<String>, started: Instant) -> Self {
        cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
        // Small OFL subset, independent of system codecs/fonts and fast to parse.
        {
            let bytes = include_bytes!("../assets/ui-chinese.ttf").to_vec();
            let mut fonts = egui::FontDefinitions::default();
            fonts.font_data.insert(
                "Chinese".into(),
                Arc::new(egui::FontData::from_owned(bytes)),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "Chinese".into());
            cc.egui_ctx.set_fonts(fonts);
        }
        let (tx, decoded) = mpsc::channel();
        let arg = |key: &str| {
            args.iter()
                .position(|s| s == key)
                .and_then(|i| args.get(i + 1))
                .cloned()
                .unwrap_or_default()
        };
        let quit_after = arg("--quit-after")
            .parse::<u64>()
            .ok()
            .map(Duration::from_millis);
        Self {
            started,
            first_ui: false,
            proc: cc
                .get_proc_address
                .clone()
                .expect(media_browser::ui_text!(msg_eecb8317e320)),
            player: None,
            video_path: arg("--video"),
            heic_path: arg("--heic"),
            status: media_browser::ui_text!(msg_bc358517f3b8).into(),
            detail: false,
            texture: None,
            decoded,
            tx,
            paused: false,
            volume: 70.,
            pending_seek: None,
            smoke: args.iter().any(|s| s == "--smoke"),
            smoke_step: 0,
            smoke_at: Instant::now(),
            smoke_rows: Vec::new(),
            screenshot_at: None,
            quit_after,
        }
    }
    fn load_video(&mut self, ctx: &egui::Context) {
        let result = (|| {
            if self.player.is_none() {
                self.player = Some(Arc::new(Mutex::new(Player::new(
                    Some(self.proc.clone()),
                    Some(ctx.clone()),
                )?)));
            }
            let mut p = self.player.as_ref().unwrap().lock().unwrap();
            p.set("mute", if self.detail { "no" } else { "yes" })?;
            p.set("volume", &self.volume.to_string())?;
            p.load(Path::new(&self.video_path))?;
            p.set("pause", "no")?;
            Ok::<_, String>(())
        })();
        self.status = match result {
            Ok(()) => media_browser::ui_text!(msg_c1bf241a7e07).into(),
            Err(e) => e,
        };
        self.paused = false;
    }
    fn load_heic(&mut self, ctx: &egui::Context) {
        let path = self.heic_path.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.status = media_browser::ui_text!(msg_24a7767a9251).into();
        std::thread::spawn(move || {
            let result = crate::heif::Heif::new()
                .and_then(|d| d.decode(Path::new(&path)).map(|img| (d.version(), img)));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    fn seek(&mut self, delta: f64) {
        if let Some(p) = &self.player {
            let p = p.lock().unwrap();
            let duration = p.number("duration").unwrap_or(0.);
            let base = self
                .pending_seek
                .unwrap_or_else(|| p.number("time-pos").unwrap_or(0.));
            let target = (base + delta).clamp(0., duration);
            self.pending_seek = Some(target);
            if let Err(e) = p.seek(target) {
                self.status = e;
            } else {
                self.status = media_browser::ui_format!(msg_8e1cbda762c9,
                    delta = delta,
                    target = target
                );
            }
        }
    }
    fn snapshot(&mut self, ctx: &egui::Context, name: &str) {
        let path = PathBuf::from("artifacts").join(name);
        let _ = std::fs::create_dir_all("artifacts");
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
            egui::ScreenshotCallback::new(move |image| {
                let rgba: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                let _ = image::save_buffer(
                    &path,
                    &rgba,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                );
            }),
        ));
    }
    fn toggle_detail(&mut self) {
        self.detail = !self.detail;
        if let Some(p) = &self.player {
            let _ = p
                .lock()
                .unwrap()
                .set("mute", if self.detail { "no" } else { "yes" });
        }
    }
    fn smoke(&mut self, ctx: &egui::Context) {
        if !self.smoke {
            return;
        }
        let elapsed = self.smoke_at.elapsed().as_secs_f32();
        if elapsed < 2. {
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        match self.smoke_step {
            0 => {
                self.load_video(ctx);
                if !self.heic_path.is_empty() {
                    self.load_heic(ctx);
                }
            }
            1 => {
                self.snapshot(ctx, "p0-preview.png");
            }
            2 => {
                self.toggle_detail();
            }
            3 => {
                self.seek(30.);
            }
            4 => {
                self.seek(-5.);
            }
            5 => {
                self.snapshot(ctx, "p0-detail.png");
            }
            6 => {
                self.toggle_detail();
            }
            7 => self.snapshot(ctx, "p0-preview-return.png"),
            _ => {}
        }
        if let Some(p) = &self.player {
            let p = p.lock().unwrap();
            self.smoke_rows.push(serde_json::json!({"step":self.smoke_step,"detail":self.detail,"time":p.number("time-pos"),"duration":p.number("duration"),"mute":p.text("mute"),"audio":p.text("audio-codec-name"),"audio_device":p.text("current-ao"),"video":p.text("video-codec"),"hwdec":p.text("hwdec-current"),"video_output":p.text("current-vo"),"status":self.status}));
        }
        if self.smoke_step == 8 {
            let json = serde_json::json!({"first_ui_note":"first_ui_ms is internal callback timing, not external visible-frame measurement","steps":self.smoke_rows});
            let _ = std::fs::write(
                "artifacts/p0-gpu-smoke.json",
                serde_json::to_vec_pretty(&json).unwrap(),
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.smoke_step += 1;
        self.smoke_at = Instant::now();
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

impl eframe::App for Prototype {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if !self.first_ui {
            self.first_ui = true;
            let _ = std::fs::create_dir_all("artifacts");
            let _ = std::fs::write(
                "artifacts/p0-first-ui.json",
                format!(
                    "{{\"first_ui_ms\":{},\"scope\":\"internal UI callback, not external visibility\"}}",
                    self.started.elapsed().as_millis()
                ),
            );
        }
        while let Ok(result) = self.decoded.try_recv() {
            match result {
                Ok((version, img)) => {
                    let size = [img.width() as usize, img.height() as usize];
                    self.texture = Some(ctx.load_texture(
                        "HEIC",
                        egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()),
                        egui::TextureOptions::LINEAR,
                    ));
                    self.status = media_browser::ui_format!(msg_d78886106a03,
                        size[0],
                        size[1],
                        version = version
                    );
                }
                Err(e) => self.status = e,
            }
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading(media_browser::ui_text!(msg_696f70feda43));
            ui.label(media_browser::ui_text!(msg_e7dbf4664f29));
            ui.horizontal(|ui| {
                ui.label(media_browser::ui_text!(msg_c20f7618d330));
                ui.text_edit_singleline(&mut self.video_path);
                if ui
                    .button(media_browser::ui_text!(msg_202ea26f0b99))
                    .clicked()
                {
                    self.load_video(&ctx);
                }
            });
            ui.horizontal(|ui| {
                ui.label("HEIC");
                ui.text_edit_singleline(&mut self.heic_path);
                if ui
                    .button(media_browser::ui_text!(msg_0e794723614f))
                    .clicked()
                {
                    self.load_heic(&ctx);
                }
            });
            ui.horizontal(|ui| {
                if ui
                    .button(if self.detail {
                        media_browser::ui_text!(msg_e6910fbecb30)
                    } else {
                        media_browser::ui_text!(msg_55cbb6002f98)
                    })
                    .clicked()
                {
                    self.toggle_detail();
                }
                if ui
                    .button(if self.paused {
                        media_browser::ui_text!(msg_c3396195e91c)
                    } else {
                        media_browser::ui_text!(msg_8d12fc0d4eb2)
                    })
                    .clicked()
                {
                    self.paused = !self.paused;
                    if let Some(p) = &self.player {
                        let _ = p
                            .lock()
                            .unwrap()
                            .set("pause", if self.paused { "yes" } else { "no" });
                    }
                }
                for delta in [-30., -5., 5., 30.] {
                    if ui
                        .button(media_browser::ui_format!(msg_5fc98360fb7d,
                            delta = delta
                        ))
                        .clicked()
                    {
                        self.seek(delta);
                    }
                }
                if ui
                    .button(media_browser::ui_text!(msg_c95dc99afe57))
                    .clicked()
                {
                    self.snapshot(&ctx, "p0-manual.png");
                }
            });
            ui.label(&self.status);
            if let Some(p) = self.player.clone() {
                let size = if self.detail {
                    egui::vec2(ui.available_width(), ui.available_height().max(200.) - 70.)
                } else {
                    egui::vec2(540., 305.)
                };
                let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                ui.painter().rect_filled(rect, 0., egui::Color32::BLACK);
                let p2 = p.clone();
                ui.painter().add(egui::PaintCallback {
                    rect,
                    callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |info, painter| {
                        let v = info.viewport_in_pixels();
                        if let Err(e) = p2.lock().unwrap().paint(
                            painter.gl(),
                            v.width_px,
                            v.height_px,
                            [v.left_px, v.from_bottom_px, v.width_px, v.height_px],
                            painter.intermediate_fbo(),
                        ) {
                            eprintln!("{e}");
                        }
                    })),
                });
                // Drawn after the callback: this must visibly overlap the video.
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(
                        rect.min + egui::vec2(12., 12.),
                        egui::vec2(195., 32.),
                    ),
                    4.,
                    egui::Color32::from_rgba_unmultiplied(0, 80, 40, 235),
                );
                ui.painter().text(
                    rect.min + egui::vec2(22., 28.),
                    egui::Align2::LEFT_CENTER,
                    media_browser::ui_text!(msg_7d6f32539855),
                    egui::FontId::proportional(15.),
                    egui::Color32::WHITE,
                );
                if response.clicked() {
                    self.toggle_detail();
                }
                if response.hovered() {
                    let events = ctx.input(|i| i.events.clone());
                    for event in events {
                        match event {
                            egui::Event::MouseWheel { delta, .. } if delta.y != 0. => {
                                self.seek(if delta.y > 0. { 30. } else { -30. })
                            }
                            egui::Event::PointerButton {
                                button: egui::PointerButton::Extra1,
                                pressed: true,
                                ..
                            } => self.seek(-5.),
                            egui::Event::PointerButton {
                                button: egui::PointerButton::Extra2,
                                pressed: true,
                                ..
                            } => self.seek(5.),
                            _ => {}
                        }
                    }
                }
                let p = p.lock().unwrap();
                let time = p.number("time-pos").unwrap_or(0.);
                let duration = p.number("duration").unwrap_or(0.);
                if self
                    .pending_seek
                    .is_some_and(|target| (target - time).abs() < 0.5)
                {
                    self.pending_seek = None;
                }
                ui.label(media_browser::ui_format!(msg_6b2eacd72d84,
                    p.text("current-vo").unwrap_or_default(),
                    p.text("hwdec-current").unwrap_or_default(),
                    duration = duration,
                    time = time
                ));
            }
            if !self.detail {
                if let Some(texture) = &self.texture {
                    ui.add(egui::Image::new(texture).max_size(egui::vec2(350., 200.)));
                }
            }
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.detail {
            self.toggle_detail();
        }
        self.smoke(&ctx);
        if let Some(after) = self.quit_after {
            if self.started.elapsed() >= after {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.request_repaint_after(after - self.started.elapsed());
            }
        }
        if self
            .screenshot_at
            .is_some_and(|at| at.elapsed() > Duration::from_secs(1))
        {
            self.screenshot_at = None;
        }
    }
    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        if let (Some(p), Some(gl)) = (&self.player, gl) {
            p.lock().unwrap().release_gl(gl);
        }
    }
}
