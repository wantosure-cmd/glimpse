//! Detail viewer: media canvas, playback controls and virtualized filmstrip.
use super::*;
const VIDEO_FOOTER_HEIGHT: f32 = 64.;
fn paint_detail_footer(ui: &egui::Ui, canvas: Rect, screen: Rect) {
    ui.painter().rect_filled(
        Rect::from_min_max(
            egui::pos2(screen.left(), canvas.bottom()),
            screen.right_bottom(),
        ),
        0.,
        Color32::from_black_alpha(160),
    );
}
#[derive(Default)]
pub(super) struct DetailLayout {
    pub canvas: Option<Rect>,
    pub header: Option<Rect>,
    pub close: Option<Rect>,
    pub fullscreen_button: Option<Rect>,
    pub image_viewport: Option<media_browser::long_png::Region>,
    pub image_viewport_ready: bool,
    pub image_full_rect: Option<Rect>,
    pub current_time: Option<Rect>,
    pub total_time: Option<Rect>,
    pub next_image: Option<MediaId>,
    pub next_image_ready: bool,
    current_image_bytes: usize,
    pub seek: Option<Rect>,
    pub volume: Option<Rect>,
    pub play_pause: Option<Rect>,
    pub mute: Option<Rect>,
    pub expand: Option<Rect>,
    pub transport: Option<Rect>,
    pub strip: Option<Rect>,
    pub arrows: Vec<Rect>,
    pub thumbnails: Vec<(MediaId, Rect)>,
}

impl Browser {
    pub(super) fn exit_video_fullscreen(&mut self) {
        if let Some(previous) = self.video_fullscreen.take() {
            self.fullscreen = previous;
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(previous));
            self.ctx.request_repaint();
        }
    }
    pub(super) fn toggle_video_fullscreen(&mut self) {
        if self.video_fullscreen.is_some() { self.exit_video_fullscreen(); }
        else {
            self.video_fullscreen = Some(self.fullscreen);
            self.video_fullscreen_activity = Instant::now();
            self.fullscreen = true;
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
            self.ctx.request_repaint();
        }
    }
    fn video_fullscreen_button(&mut self, ui: &mut egui::Ui, header: Rect) {
        let rect = Rect::from_min_size(egui::pos2(header.right()-104.,header.top()+2.),egui::vec2(60.,28.));
        let mut child=ui.new_child(egui::UiBuilder::new().id_salt("video-fullscreen-button").max_rect(rect));
        let button=child.add_sized(rect.size(),egui::Button::new(media_browser::ui_text!(msg_22ea3f47fee8)))
            .on_hover_text(format!("{} · Esc",media_browser::ui_text!(msg_aa8b6477c2ce)));
        self.detail_layout.fullscreen_button=Some(button.rect);
        self.control_regions.push(button.rect);
        self.control_focused |= button.has_focus();
        if button.clicked() { self.toggle_video_fullscreen(); }
    }
    fn detail_close_corner(&mut self, ui: &mut egui::Ui, header: Rect, visible: bool) -> bool {
        // The hit rectangle touches both viewport edges; the glyph stays compact.
        let hit=Rect::from_min_max(egui::pos2(header.right()-40.,header.top()),header.right_bottom());
        let mut child=ui.new_child(egui::UiBuilder::new().id_salt("detail-close").max_rect(hit)
            .layout(egui::Layout::left_to_right(egui::Align::Center)));
        let response=if visible {
            icons::icon_button_with_size(&mut child,icons::Glyph::Close,media_browser::ui_text!(msg_3b97d30d35dd),hit.size())
        } else {child.allocate_exact_size(hit.size(),egui::Sense::click()).1};
        self.detail_layout.close=Some(response.rect);
        self.control_regions.push(response.rect);
        self.control_focused |= response.has_focus();
        response.clicked()
    }
    fn fullscreen_video_detail(&mut self, ctx:&egui::Context, jobs:&mut Vec<ThumbJob>, prepared:&Arc<Prepared>, mut viewer:Viewer, record:&MediaRecord) {
        let screen=ctx.content_rect();
        if ctx.input(|i| i.pointer.delta()!=Vec2::ZERO || i.pointer.any_pressed()) {
            self.video_fullscreen_activity=Instant::now();
        }
        let chrome=self.video_fullscreen_activity.elapsed()<Duration::from_secs(2);
        if chrome {ctx.request_repaint_after(Duration::from_secs(2).saturating_sub(self.video_fullscreen_activity.elapsed()));}
        let popup=egui::Popup::is_any_open(ctx) || self.workspace.is_open();
        let mut next=None;let mut close=false;
        egui::Modal::new(egui::Id::unique("fullscreen-video"))
            .frame(egui::Frame::new().fill(Color32::BLACK).inner_margin(0.))
            .show(ctx,|ui| {
                ui.spacing_mut().item_spacing=Vec2::ZERO;
                let (canvas,response)=ui.allocate_exact_size(screen.size(),egui::Sense::click_and_drag());
                self.detail_layout.canvas=Some(canvas);
                let content=contain(canvas,self.video_size(record));
                self.video_region=Some((viewer.id,content));
                self.paint_video(ui,content);
                response.context_menu(|ui|self.detail_context_menu(ui,record,&mut viewer,&mut next,&mut close));
                if chrome {
                    self.detail_navigation(ui,canvas,canvas.center().y,viewer.id,prepared,&mut next);
                    self.video_fullscreen_button(ui,Rect::from_min_size(canvas.min,egui::vec2(canvas.width(),32.)));
                }
                close |= self.detail_close_corner(ui,Rect::from_min_size(canvas.min,egui::vec2(canvas.width(),32.)),chrome);
                if !popup && !egui::Popup::is_any_open(ctx) {
                    let over_control=ctx.input(|i|i.pointer.interact_pos().is_some_and(|p|self.control_regions.iter().any(|r|r.contains(p))));
                    if !over_control && response.double_clicked() {close=true;}
                    else if !over_control && response.clicked() && response.interact_pointer_pos().is_some_and(|p|content.contains(p)) {
                        self.toggle_detail_pause();
                    }
                }
                self.detail_preload(ctx,prepared,viewer.id,canvas,jobs);
            });
        if next.is_some(){close=false;}
        if !close {self.detail_end_action(&mut viewer,prepared,&mut next);}
        self.detail=Some(viewer);
        if close {self.close_detail();} else if let Some(id)=next {self.open_detail(id);}
    }
    pub(super) fn detail(&mut self, ctx: &egui::Context, jobs: &mut Vec<ThumbJob>) {
        let Some(mut viewer) = self.detail.clone() else {
            return;
        };
        let Some(prepared) = self.prepared.clone() else {
            return;
        };
        let Some(index) = prepared.lookup.get(&viewer.id) else {
            self.close_detail();
            return;
        };
        let record = prepared.snapshot.files[*index].clone();
        if self.presentation.mode != presentation_ui::Mode::Manual {
            self.presentation.ensure_plan(&prepared);
        }
        self.presentation.ready = false;
        self.presentation.tiles.clear();
        self.detail_layout = DetailLayout::default();
        let screen = ctx.content_rect();
        if record.file.kind == MediaKind::Video && self.video_fullscreen.is_some() {
            self.fullscreen_video_detail(ctx, jobs, &prepared, viewer, &record);
            return;
        }
        let mut close = false;
        let mut next = None;
        let mut slider_seek = None;
        let mut toggle_pause = false;
        let popup_was_open = egui::Popup::is_any_open(ctx) || self.workspace.is_open();
        let modal = egui::Modal::new(egui::Id::unique("viewer"))
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(10, 10, 12))
                    .inner_margin(0.),
            )
            .show(ctx, |ui| {
                ui.set_width(screen.width().max(300.));
                ui.spacing_mut().item_spacing = egui::vec2(6., 3.);
                ui.spacing_mut().interact_size.y = 22.;
                ui.set_height(screen.height().max(200.));
                let (header, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 32.),
                    egui::Sense::hover(),
                );
                // Reserve stable space for playback controls and the virtualized filmstrip.
                // Arrows share the image viewport's centre for every media type.
                // Video transport height must not move the navigation targets.
                let navigation_height = (ui.available_height() - 76.).max(40.);
                let canvas_size = egui::vec2(
                    ui.available_width(),
                    (ui.available_height()
                        - if record.file.kind == MediaKind::Video {
                            VIDEO_FOOTER_HEIGHT
                        } else {
                            76.
                        })
                    .max(40.),
                );
                let (canvas, response) =
                    ui.allocate_exact_size(canvas_size, egui::Sense::click_and_drag());
                self.detail_layout.canvas = Some(canvas);
                ui.painter().rect_filled(canvas, 8., Color32::BLACK);
                response.context_menu(|ui| {
                    self.detail_context_menu(ui, &record, &mut viewer, &mut next, &mut close);
                });
                if record.file.kind == MediaKind::Image
                    && self.presentation.mode == presentation_ui::Mode::Group
                {
                    self.paint_collage(
                        ui,
                        canvas,
                        &prepared,
                        &mut viewer,
                        jobs,
                        &mut next,
                        &mut close,
                    );
                } else if record.file.kind == MediaKind::Image {
                    let animated = self.is_animated(&record);
                    if animated
                        && !self.trash_releasing()
                        && !self.trash_blocks_media(record.id)
                        && self.rotation.is_none()
                        && self.permanent_prompt.is_none()
                    {
                        if self.list_animations.released() {
                            self.animation.open(&record.file);
                        } else {
                            ctx.request_repaint_after(Duration::from_millis(10));
                        }
                    }
                    let source = egui::vec2(
                        record.width.unwrap_or(1920) as f32,
                        record.height.unwrap_or(1080) as f32,
                    );
                    let fit = (canvas.width() / source.x).min(canvas.height() / source.y);
                    let wanted = if viewer.native_pending {
                        8192.
                    } else {
                        source.x.max(source.y) * fit * viewer.zoom * ctx.pixels_per_point()
                    };
                    let bucket = [512, 1024, 2048, 4096, 8192]
                        .into_iter()
                        .find(|b| *b as f32 >= wanted)
                        .unwrap_or(8192);
                    let job = self.job(&record, bucket, 0);
                    let key = job.key();
                    if !animated {
                        self.progressive_job(&record, bucket, 0, jobs);
                    }
                    let texture = (if animated {
                        self.animation.texture()
                    } else {
                        self.texture(&key)
                    })
                    .or_else(|| {
                        [4096, 2048, 1024, 512, 256, 64]
                            .into_iter()
                            .find_map(|bucket| self.texture(&self.job(&record, bucket, 0).key()))
                    });
                    if let Some(error) = &self.animation.error {
                        ui.colored_label(modern_ui::DANGER, error);
                    }
                    self.presentation.ready = texture.is_some() || self.failures.contains_key(&key);
                    if let Some(texture) = texture {
                        self.detail_layout.current_image_bytes =
                            texture.size()[0] * texture.size()[1] * 4;
                        let source = egui::vec2(
                            record.width.unwrap_or(texture.size()[0] as u32) as f32,
                            record.height.unwrap_or(texture.size()[1] as u32) as f32,
                        );
                        let fit = (canvas.width() / source.x).min(canvas.height() / source.y);
                        if viewer.native_pending {
                            viewer.zoom = 1. / (fit * ctx.pixels_per_point());
                            viewer.pan = Vec2::ZERO;
                            viewer.native_pending = false;
                        }
                        let mut scale = fit * viewer.zoom;
                        if response.hovered() {
                            let delta = std::mem::take(&mut self.routed_zoom);
                            if delta != 0. {
                                let old = scale;
                                viewer.zoom =
                                    (viewer.zoom * (delta * 0.002).exp()).clamp(0.05, 100.);
                                scale = fit * viewer.zoom;
                                if let Some(pointer) = ctx.input(|i| i.pointer.hover_pos()) {
                                    let local = (pointer - canvas.center() - viewer.pan) / old;
                                    viewer.pan = pointer - canvas.center() - local * scale;
                                }
                            }
                        }
                        if response.dragged() {
                            viewer.pan += ctx.input(|i| i.pointer.delta());
                        }
                        let rect =
                            Rect::from_center_size(canvas.center() + viewer.pan, source * scale);
                        self.image_region = Some(rect.intersect(canvas));
                        self.detail_layout.image_full_rect = Some(rect);
                        let painted = self.opening_motion_rect(ctx, rect);
                        ui.painter()
                            // A zoomed image extends beneath the translucent
                            // bars; their controls keep their own hit regions.
                            .with_clip_rect(screen)
                            .image(
                                texture.id(),
                                painted,
                                Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
                                Color32::WHITE,
                            );
                        // The overview stays visible while a bounded, full-detail crop loads.
                        // Only very long PNGs need this: existing image paths are unchanged.
                        if !animated
                            && record
                                .file
                                .path
                                .extension()
                                .is_some_and(|e| e.eq_ignore_ascii_case("png"))
                            && source.x.max(source.y) > 16384.
                            && scale * ctx.pixels_per_point() > 8192. / source.x.max(source.y)
                        {
                            let visible = rect.intersect(screen);
                            if visible.is_positive() {
                                let align = 256.;
                                let origin = (visible.min - rect.min) / scale;
                                let end = (visible.max - rect.min) / scale;
                                let x = ((origin.x / align).floor() * align).max(0.) as u32;
                                let y = ((origin.y / align).floor() * align).max(0.) as u32;
                                let right = (((end.x / align).ceil() * align).min(source.x)) as u32;
                                let bottom =
                                    (((end.y / align).ceil() * align).min(source.y)) as u32;
                                let region = media_browser::long_png::Region {
                                    x,
                                    y,
                                    width: right.saturating_sub(x),
                                    height: bottom.saturating_sub(y),
                                };
                                if region.width > 0 && region.height > 0 {
                                    let wanted = region.width.max(region.height) as f32
                                        * scale
                                        * ctx.pixels_per_point();
                                    let bucket = [512, 1024, 2048, 4096]
                                        .into_iter()
                                        .find(|n| *n as f32 >= wanted)
                                        .unwrap_or(4096);
                                    let mut crop = self.job(&record, bucket, 0);
                                    crop.region = Some(region);
                                    let key = crop.key();
                                    self.detail_layout.image_viewport = Some(region);
                                    if let Some(texture) = self.texture(&key) {
                                        self.detail_layout.image_viewport_ready = true;
                                        self.detail_layout.current_image_bytes +=
                                            texture.size()[0] * texture.size()[1] * 4;
                                        let ratio = painted.size() / source;
                                        let dest = Rect::from_min_size(
                                            painted.min
                                                + egui::vec2(region.x as f32, region.y as f32)
                                                    * ratio,
                                            egui::vec2(region.width as f32, region.height as f32)
                                                * ratio,
                                        );
                                        ui.painter().with_clip_rect(screen).image(
                                            texture.id(),
                                            dest,
                                            Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
                                            Color32::WHITE,
                                        );
                                    } else if !self.failures.contains_key(&key) {
                                        jobs.push(crop);
                                    }
                                }
                            }
                        }
                    } else {
                        ui.painter().text(
                            canvas.center(),
                            egui::Align2::CENTER_CENTER,
                            self.failures
                                .get(&key)
                                .map(String::as_str)
                                .unwrap_or(media_browser::ui_text!(msg_4836a49a67cd)),
                            egui::FontId::proportional(16.),
                            Color32::LIGHT_GRAY,
                        );
                    }
                } else {
                    // A cloud thumbnail may still be pending. Use the active
                    // decoder's aspect/rotation rather than a 16:9 hit rectangle.
                    let content = contain(canvas, self.video_size(&record));
                    self.video_region = Some((viewer.id, content));
                    let painted = self.opening_motion_rect(ctx, content);
                    let opening_poster = if painted != content
                        && !self
                            .player
                            .as_ref()
                            .is_some_and(|p| p.lock().unwrap().ready())
                    {
                        self.media_transition.as_ref().and_then(|m| m.poster())
                    } else {
                        None
                    };
                    if let Some(texture) = opening_poster {
                        ui.painter().image(
                            texture.id(),
                            painted,
                            Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
                            Color32::WHITE,
                        );
                    } else {
                        self.paint_video(ui, painted);
                    }
                    paint_detail_footer(ui, canvas, screen);
                    if let Some(player) = self.player.clone() {
                        let p = player.lock().unwrap();
                        let mut time = self
                            .seeks
                            .target()
                            .unwrap_or_else(|| p.number("time-pos").unwrap_or(0.));
                        let duration = p.number("duration").unwrap_or(0.);
                        let controls = modern_ui::transport_row(ui, |ui| {
                            ui.spacing_mut().interact_size.y = 24.;
                            let compact = ui.available_width() < 460.;
                            ui.spacing_mut().item_spacing.x = if compact { 4. } else { 6. };
                            let volume_width = if compact { 40. } else { 72. };
                            let clock_font = egui::FontId::proportional(modern_ui::TYPE_SMALL);
                            let elapsed_text = format_time((time * 1000.) as u64);
                            let total_text = format_time((duration * 1000.) as u64);
                            let time_width = [&elapsed_text, &total_text]
                                .into_iter()
                                .map(|text| {
                                    ui.painter()
                                        .layout_no_wrap(
                                            text.clone(),
                                            clock_font.clone(),
                                            modern_ui::TEXT,
                                        )
                                        .size()
                                        .x
                                })
                                .fold(0_f32, f32::max)
                                + 4.;
                            let seek_width = (ui.available_width()
                                - 3. * 24.
                                - 2. * time_width
                                - volume_width
                                - 6. * ui.spacing().item_spacing.x)
                                .max(24.);
                            let failed = self.playback_errors.contains_key(&viewer.id);
                            let paused = p.paused_intent();
                            let play = ui
                                .add_enabled_ui(failed || p.ready(), |ui| {
                                    modern_ui::tracked_transport_icon(
                                        ui,
                                        &mut self.control_regions,
                                        if failed {
                                            icons::Glyph::Refresh
                                        } else if paused {
                                            icons::Glyph::Play
                                        } else {
                                            icons::Glyph::Pause
                                        },
                                        if failed {
                                            media_browser::ui_text!(msg_b8784c8dd563)
                                        } else if paused {
                                            media_browser::ui_text!(msg_c3396195e91c)
                                        } else {
                                            media_browser::ui_text!(msg_8d12fc0d4eb2)
                                        },
                                    )
                                })
                                .inner;
                            self.detail_layout.play_pause = Some(play.rect);
                            self.control_focused |= play.has_focus();
                            if play.clicked() {
                                if failed {
                                    next = Some(viewer.id);
                                } else {
                                    toggle_pause = true;
                                }
                            }
                            let elapsed = ui.add_sized(
                                egui::vec2(time_width, 24.),
                                egui::Label::new(
                                    egui::RichText::new(elapsed_text)
                                        .font(clock_font.clone())
                                        .color(modern_ui::TEXT),
                                ),
                            );
                            self.detail_layout.current_time = Some(elapsed.rect);
                            ui.spacing_mut().slider_width = seek_width;
                            let seek = ui.add_enabled(
                                duration > 0.,
                                super::modern_ui::QuietSlider(
                                    egui::Slider::new(&mut time, 0.0..=duration.max(0.001))
                                        .show_value(false),
                                ),
                            );
                            self.detail_layout.seek = Some(seek.rect);
                            self.control_regions.push(seek.rect);
                            self.control_focused |= seek.has_focus();
                            if seek.changed() {
                                slider_seek = Some(time);
                            }
                            let total = ui.add_sized(
                                egui::vec2(time_width, 24.),
                                egui::Label::new(
                                    egui::RichText::new(total_text)
                                        .font(clock_font)
                                        .color(modern_ui::MUTED),
                                ),
                            );
                            self.detail_layout.total_time = Some(total.rect);
                            let mute = modern_ui::tracked_transport_icon(
                                ui,
                                &mut self.control_regions,
                                if self.muted {
                                    icons::Glyph::Muted
                                } else {
                                    icons::Glyph::Volume
                                },
                                if self.muted {
                                    media_browser::ui_text!(msg_8d01732e7477)
                                } else {
                                    media_browser::ui_text!(msg_7db2e189ecb0)
                                },
                            );
                            self.detail_layout.mute = Some(mute.rect);
                            self.control_focused |= mute.has_focus();
                            if mute.clicked() {
                                self.muted = !self.muted;
                                let _ = p.set("mute", if self.muted { "yes" } else { "no" });
                            }
                            ui.spacing_mut().slider_width = volume_width;
                            let volume_response = ui.add(super::modern_ui::QuietSlider(
                                egui::Slider::new(&mut self.volume, 0.0..=400.).show_value(false),
                            ));
                            self.detail_layout.volume = Some(volume_response.rect);
                            self.control_focused |= volume_response.has_focus();
                            self.control_regions.push(volume_response.rect);
                            volume_response
                                .clone()
                                .on_hover_text(media_browser::ui_format!(
                                    msg_f1c807b2b743,
                                    self.volume
                                ));
                            if volume_response.changed() {
                                let _ = p.set("volume", &format!("{:.1}", self.volume));
                            }
                            let expand = modern_ui::tracked_transport_icon(
                                ui,
                                &mut self.control_regions,
                                icons::Glyph::Expand,
                                media_browser::ui_text!(msg_5745c2638d37),
                            );
                            self.detail_layout.expand = Some(expand.rect);
                            self.control_focused |= expand.has_focus();
                            if expand.clicked() {
                                self.toggle_video_fullscreen();
                            }
                        });
                        self.detail_layout.transport = Some(controls.response.rect);
                        self.control_regions.push(controls.response.rect);
                    }
                }
                if record.file.kind == MediaKind::Image {
                    paint_detail_footer(ui, canvas, screen);
                }
                // Paint chrome after media so zoomed pixels remain visible
                // through the bars without changing canvas/control hit areas.
                self.detail_header(
                    ui,
                    header,
                    &record,
                    &prepared,
                    &mut viewer,
                    &mut next,
                    &mut close,
                );
                self.presentation_badge(ui, canvas);
                self.detail_preload(ctx, &prepared, viewer.id, canvas, jobs);
                self.detail_navigation(
                    ui,
                    canvas,
                    canvas.top() + navigation_height / 2.,
                    viewer.id,
                    &prepared,
                    &mut next,
                );
                if ctx.input(|i| {
                    i.pointer
                        .button_double_clicked(egui::PointerButton::Primary)
                }) && !popup_was_open
                    && !egui::Popup::is_any_open(ctx)
                    && ctx
                        .input(|i| i.pointer.interact_pos())
                        .is_some_and(|point| {
                            canvas.contains(point)
                                && (self.image_region.is_some_and(|r| r.contains(point))
                                    || self.video_region.is_some_and(|(_, r)| r.contains(point)))
                                && !self.control_regions.iter().any(|r| r.contains(point))
                        })
                {
                    close = true;
                }
                // Only the painted video handles playback clicks; letterbox blank still returns.
                if record.file.kind == MediaKind::Video
                    && response.clicked()
                    && !response.double_clicked()
                    && !popup_was_open
                    && !egui::Popup::is_any_open(ctx)
                    && response.interact_pointer_pos().is_some_and(|point| {
                        self.video_region
                            .is_some_and(|(_, rect)| rect.contains(point))
                            && !self.control_regions.iter().any(|rect| rect.contains(point))
                    })
                {
                    self.toggle_detail_pause();
                }

                if record.file.kind == MediaKind::Image { ui.separator(); }
                self.detail_thumbnails(ui, &prepared, &mut viewer, jobs, &mut next);
            });
        if toggle_pause {
            self.toggle_detail_pause();
        }
        let blank_clicked = ctx.input(|i| {
            i.pointer.primary_clicked()
                && i.pointer.interact_pos().is_some_and(|p| {
                    !self.video_region.is_some_and(|(_, r)| r.contains(p))
                        && !self.image_region.is_some_and(|r| r.contains(p))
                        && !self.control_regions.iter().any(|r| r.contains(p))
                })
        }) && !popup_was_open
            && !egui::Popup::is_any_open(ctx);
        if self.frame > self.detail_open_frame
            && (blank_clicked || modal.backdrop_response.clicked())
        {
            close = true;
        }
        if let Some(time) = slider_seek {
            self.request_seek(time, true);
        }
        if self.detail.is_none() {
            return;
        }
        if next.is_some() {
            close = false;
        }
        if !close {
            self.detail_end_action(&mut viewer, &prepared, &mut next);
        }
        self.detail = Some(viewer);
        if close {
            self.close_detail();
        } else if let Some(id) = next {
            self.open_detail(id);
        }
    }

    fn detail_header(
        &mut self,
        ui: &mut egui::Ui,
        canvas: Rect,
        record: &MediaRecord,
        prepared: &Prepared,
        viewer: &mut Viewer,
        next: &mut Option<MediaId>,
        close: &mut bool,
    ) {
        let index = prepared
            .view
            .sequence
            .iter()
            .position(|id| *id == record.id)
            .unwrap_or(0)
            + 1;
        let count = format!("{index} / {}", prepared.view.sequence.len());
        let name = record
            .file
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let name = if record.file.kind == MediaKind::Video && (self.playback_speed - 1.).abs() > 0.001 {
            format!("{name} · {:.2}×", self.playback_speed)
        } else { name.into_owned() };
        let folder = record
            .file
            .parent
            .file_name()
            .unwrap_or(record.file.parent.as_os_str())
            .to_string_lossy();
        ui.painter()
            .rect_filled(canvas, 0., Color32::from_black_alpha(160));
        ui.painter().line_segment(
            [canvas.left_bottom(), canvas.right_bottom()],
            egui::Stroke::new(1., modern_ui::BORDER),
        );
        let mut bar = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("detail-title-bar")
                .max_rect(Rect::from_min_max(
                    canvas.min + egui::vec2(6., 0.),
                    canvas.max - egui::vec2(if record.file.kind == MediaKind::Video {108.} else {40.}, 0.),
                ))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        bar.set_clip_rect(bar.max_rect().intersect(canvas));
        bar.spacing_mut().item_spacing = egui::vec2(6., 0.);
        let back = icons::icon_button(
            &mut bar,
            icons::Glyph::Left,
            media_browser::ui_text!(msg_3b97d30d35dd),
        );
        if back.clicked() {
            *close = true;
        }
        back.context_menu(|ui| self.detail_context_menu(ui, record, viewer, next, close));
        let (favorite, _) = bar.allocate_exact_size(egui::vec2(28., 28.), egui::Sense::hover());
        self.favorite_button(&mut bar, record, favorite, true);
        let progress_width = bar
            .painter()
            .layout_no_wrap(
                count.clone(),
                egui::FontId::proportional(11.),
                modern_ui::MUTED,
            )
            .size()
            .x
            + 8.;
        let available = (bar.available_width() - progress_width - 12.).max(40.);
        let folder_width = (available * 0.28).clamp(50., 240.);
        let title_width = (available - folder_width - 6.).max(20.);
        let title = bar
            .allocate_ui_with_layout(
                egui::vec2(title_width, 28.),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(title_width);
                    ui.add(
                        egui::Label::new(egui::RichText::new(name.as_str()).size(13.))
                            .halign(egui::Align::Min)
                            .truncate()
                            .sense(egui::Sense::click_and_drag()),
                    )
                },
            )
            .inner;
        if title.drag_started() {
            bar.ctx()
                .send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        title.clone().on_hover_text(name.as_str());
        title.context_menu(|ui| self.detail_context_menu(ui, record, viewer, next, close));
        {
            let directory = bar
                .allocate_ui_with_layout(
                    egui::vec2(folder_width, 28.),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_min_width(folder_width);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(folder.as_ref())
                                    .size(11.)
                                    .color(modern_ui::MUTED),
                            )
                            .halign(egui::Align::Min)
                            .truncate()
                            .sense(egui::Sense::click()),
                        )
                    },
                )
                .inner;
            directory
                .clone()
                .on_hover_text(display_path(&record.file.parent));
            directory.context_menu(|ui| self.detail_context_menu(ui, record, viewer, next, close));
        }
        bar.add_sized(
            egui::vec2(progress_width, 28.),
            egui::Label::new(egui::RichText::new(count).size(11.).color(modern_ui::MUTED)),
        );
        if record.file.kind == MediaKind::Video {
            self.video_fullscreen_button(ui, canvas);
        }
        *close |= self.detail_close_corner(ui,canvas,true);
        self.detail_layout.header = Some(canvas);
        self.control_regions.push(canvas);
    }

    fn detail_preload(
        &mut self,
        ctx: &egui::Context,
        prepared: &Prepared,
        current: MediaId,
        canvas: Rect,
        jobs: &mut Vec<ThumbJob>,
    ) {
        if self.trash_operation.is_some() || self.rotation.is_some() {
            return;
        }
        // Give the current cloud video's open/first-frame read precedence.
        // After A->B, do not start warming C while B is still opening/seeking.
        let current_is_video = prepared.lookup.get(&current)
            .is_some_and(|index| prepared.snapshot.files[*index].file.kind == MediaKind::Video);
        if current_is_video && (self.player_media != Some(current)
            || self.resume_pending.is_some() || self.restore_flight
            || !self.player.as_ref().is_some_and(|p| p.lock().unwrap().has_presented_frame()))
        {
            self.video_prefetch.cancel();
            return;
        }
        if self.presentation.mode == presentation_ui::Mode::Group {
            if let Some(next) =
                self.detail_target(prepared, current, 1, self.presentation.prefs.repeat, false)
            {
                for id in self.presentation.ids(next) {
                    if let Some(record) = prepared
                        .lookup
                        .get(&id)
                        .map(|i| &prepared.snapshot.files[*i])
                    {
                        if record.file.kind == MediaKind::Image
                            && self.texture_bytes < 220 * 1024 * 1024
                        {
                            jobs.push(self.job(record, 1024, 2));
                        }
                    }
                }
            }
        }
        let sequence = &prepared.view.sequence;
        let Some(index) = sequence.iter().position(|id| *id == current) else {
            return;
        };
        if let Some(record) = sequence
            .get(index + 1)
            .and_then(|id| prepared.lookup.get(id))
            .map(|i| &prepared.snapshot.files[*i])
        {
            if record.file.kind == MediaKind::Image
                && self.presentation.mode != presentation_ui::Mode::Group
            {
                let w = record.width.unwrap_or(1920) as f32;
                let h = record.height.unwrap_or(1080) as f32;
                let fit =
                    (canvas.width() / w).min((ctx.content_rect().height() - 76.).max(40.) / h);
                let wanted = w.max(h) * fit * ctx.pixels_per_point();
                let bucket = [512, 1024, 2048, 4096, 8192]
                    .into_iter()
                    .find(|b| *b as f32 >= wanted)
                    .unwrap_or(8192)
                    .min(2048);
                let job = self.job(record, bucket, 2);
                let key = job.key();
                self.detail_layout.next_image = Some(record.id);
                self.detail_layout.next_image_ready = self.texture(&key).is_some();
                // Keep the active large photo in the shared 256 MiB cache. A 2048
                // prefetch can need 16 MiB; do not make an 8192 photo churn.
                let has_room = self.detail_layout.current_image_bytes <= 240 * 1024 * 1024;
                if has_room
                    && !self.detail_layout.next_image_ready
                    && !self.failures.contains_key(&key)
                {
                    jobs.push(job);
                }
            }
        }
        // The next video can follow an image: the EOF Next policy skips images.
        let next_video = sequence
            .iter()
            .skip(index + 1)
            .filter_map(|id| prepared.lookup.get(id))
            .map(|i| &prepared.snapshot.files[*i])
            .find(|r| r.file.kind == MediaKind::Video);
        if let Some(record) = next_video {
            let ctx = ctx.clone();
            self.video_prefetch.request(
                record.file.path.clone(),
                record.file.fingerprint.clone(),
                media_browser::media_path::virtual_hint(&prepared.snapshot.root),
                move || ctx.request_repaint(),
            );
        } else {
            self.video_prefetch.cancel();
        }
    }

    fn detail_navigation(
        &mut self,
        ui: &mut egui::Ui,
        canvas: Rect,
        navigation_center_y: f32,
        id: MediaId,
        prepared: &Prepared,
        next: &mut Option<MediaId>,
    ) {
        let hovered = ui
            .ctx()
            .input(|i| i.pointer.hover_pos().is_some_and(|p| canvas.contains(p)));
        if !hovered || self.workspace.is_open() || egui::Popup::is_any_open(ui.ctx()) {
            return;
        }
        for (delta, glyph, x) in [
            (-1, icons::Glyph::Left, canvas.left() + 12.),
            (1, icons::Glyph::Right, canvas.right() - 76.),
        ] {
            let target = self.detail_target(prepared, id, delta, false, false);
            let rect = Rect::from_min_size(
                egui::pos2(x, navigation_center_y - 44.),
                egui::vec2(64., 88.),
            );
            self.detail_layout.arrows.push(rect);
            // Overlay children must not move the main layout cursor back into the canvas.
            let mut overlay = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(("detail-arrow", delta))
                    .max_rect(rect),
            );
            let (hit, response) = overlay.allocate_exact_size(rect.size(), egui::Sense::click());
            let visible = Rect::from_center_size(hit.center(), egui::vec2(48., 64.));
            overlay
                .painter()
                .rect_filled(visible, modern_ui::CONTROL_RADIUS, modern_ui::OVERLAY);
            if response.hovered() || response.has_focus() {
                overlay.painter().rect_stroke(
                    visible,
                    modern_ui::CONTROL_RADIUS,
                    egui::Stroke::new(
                        modern_ui::STROKE,
                        if response.has_focus() {
                            self.workspace.appearance.accent()
                        } else {
                            modern_ui::BORDER
                        },
                    ),
                    egui::StrokeKind::Inside,
                );
            }
            icons::paint_sized(overlay.painter(), visible, glyph, modern_ui::TEXT,28.);
            self.control_regions.push(response.rect);
            self.control_focused |= response.has_focus();
            if response
                .on_hover_text(if delta < 0 {
                    media_browser::ui_text!(msg_bcf200709dbc)
                } else {
                    media_browser::ui_text!(msg_dcd4eb027369)
                })
                .clicked()
            {
                *next = target;
            }
        }
    }

    fn detail_thumbnails(
        &mut self,
        ui: &mut egui::Ui,
        prepared: &Prepared,
        viewer: &mut Viewer,
        jobs: &mut Vec<ThumbJob>,
        next: &mut Option<MediaId>,
    ) {
        let sequence = &prepared.view.sequence;
        let current = sequence.iter().position(|id| *id == viewer.id).unwrap_or(0);
        let compact = prepared.lookup.get(&viewer.id)
            .is_some_and(|i| prepared.snapshot.files[*i].file.kind == MediaKind::Video);
        let stride = if compact {44.} else {84.};
        let image_height = if compact {24.} else {52.};
        let viewport_width = ui.available_width();
        let padding = ((viewport_width - stride) / 2.).max(0.);
        let mut strip = egui::ScrollArea::horizontal()
            .id_salt("detail-filmstrip")
            .wheel_scroll_multiplier(egui::vec2(0., 0.))
            .auto_shrink([false, false])
            .max_height(if compact {30.} else {64.});
        if viewer.center_thumbnail {
            strip = strip.horizontal_scroll_offset(current as f32 * stride);
            viewer.center_thumbnail = false;
        }
        let output = strip.show_viewport(ui, |ui, viewport| {
            ui.set_min_size(egui::vec2(
                (2. * padding + sequence.len() as f32 * stride).max(viewport_width),
                image_height + 4.,
            ));
            let origin = ui.min_rect().min;
            let start = ((viewport.left() - padding) / stride).floor().max(0.) as usize;
            let end =
                (((viewport.right() - padding) / stride).ceil() as usize + 1).min(sequence.len());
            for (index, id) in sequence.iter().enumerate().take(end).skip(start) {
                let Some(record_index) = prepared.lookup.get(id) else {
                    continue;
                };
                let record = &prepared.snapshot.files[*record_index];
                let rect = Rect::from_min_size(
                    origin + egui::vec2(padding + index as f32 * stride + 4., 2.),
                    egui::vec2(stride - 8., image_height),
                );
                let response = ui.allocate_rect(rect, egui::Sense::click());
                self.detail_layout.thumbnails.push((*id, rect));
                self.control_regions.push(rect);
                self.control_focused |= response.has_focus();
                ui.painter()
                    .rect_filled(rect, modern_ui::CONTROL_RADIUS, modern_ui::SURFACE);
                let job = self.job(record, 256, 1);
                let key = job.key();
                if let Some(texture) = self.texture(&key) {
                    let content = contain(rect.shrink(3.), texture.size_vec2());
                    ui.painter().image(
                        texture.id(),
                        content,
                        Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
                        Color32::WHITE,
                    );
                } else if let Some(error) = self.failures.get(&key) {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        media_browser::ui_text!(msg_b825ccc758e9),
                        egui::FontId::proportional(12.),
                        Color32::GRAY,
                    );
                    response.clone().on_hover_text(error);
                } else {
                    jobs.push(job);
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "…",
                        egui::FontId::proportional(16.),
                        Color32::GRAY,
                    );
                }
                if record.file.kind == MediaKind::Video {
                    ui.painter().rect_filled(
                        Rect::from_min_size(
                            rect.left_bottom() + egui::vec2(3., if compact {-16.} else {-23.}),
                            if compact {egui::vec2(16.,14.)} else {egui::vec2(22.,22.)},
                        ),
                        4.,
                        Color32::from_black_alpha(160),
                    );
                    icons::paint(
                        ui.painter(),
                        Rect::from_min_size(
                            rect.left_bottom() + if compact {egui::vec2(5.,-15.)} else {egui::vec2(6.,-20.)},
                            Vec2::splat(if compact {12.} else {16.}),
                        ),
                        icons::Glyph::Video,
                        Color32::WHITE,
                    );
                }
                if *id == viewer.id {
                    ui.painter().rect_stroke(
                        rect,
                        5.,
                        egui::Stroke::new(2., self.workspace.appearance.accent()),
                        egui::StrokeKind::Inside,
                    );
                }
                if response.hovered() && *id != viewer.id {
                    ui.painter().rect_stroke(
                        rect,
                        5.,
                        egui::Stroke::new(1., Color32::from_white_alpha(70)),
                        egui::StrokeKind::Inside,
                    );
                }
                if response
                    .on_hover_text(
                        record
                            .file
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy(),
                    )
                    .clicked()
                    && *id != viewer.id
                {
                    *next = Some(*id);
                }
            }
        });
        // The entire strip, including gaps and scrollbar, is a control, never a backdrop.
        self.detail_layout.strip = Some(output.inner_rect);
        self.control_regions.push(output.inner_rect.expand(8.));
    }

    pub(super) fn detail_context_menu(
        &mut self,
        ui: &mut egui::Ui,
        record: &MediaRecord,
        viewer: &mut Viewer,
        next: &mut Option<MediaId>,
        close: &mut bool,
    ) {
        ui.set_min_width(220.);
        modern_ui::quiet_menu(ui);
        self.favorite_menu(ui, &record.file.path);
        self.shortcut_settings(ui);
        self.presentation_menu(ui);
        ui.separator();
        for (delta, label) in [
            (-1, media_browser::ui_text!(msg_bcf200709dbc)),
            (1, media_browser::ui_text!(msg_dcd4eb027369)),
        ] {
            let target = self
                .prepared
                .as_ref()
                .and_then(|p| self.detail_target(p, record.id, delta, false, false));
            if ui
                .add_enabled(
                    target.is_some(),
                    modern_ui::MenuAction::new(label)
                        .icon(if delta < 0 {
                            icons::Glyph::Left
                        } else {
                            icons::Glyph::Right
                        })
                        .key(if delta < 0 {
                            self.shortcuts.previous.label()
                        } else {
                            self.shortcuts.next.label()
                        }),
                )
                .clicked()
            {
                *next = target;
                ui.close();
            }
        }
        if ui
            .add(
                modern_ui::MenuAction::new(media_browser::ui_text!(msg_e9d5ca6c1406))
                    .icon(icons::Glyph::Left),
            )
            .clicked()
        {
            *close = true;
            ui.close();
        }
        if record.file.kind == MediaKind::Image {
            if ui
                .add(
                    modern_ui::MenuAction::new(media_browser::ui_text!(msg_f8d01c881e34))
                        .icon(icons::Glyph::Expand),
                )
                .clicked()
            {
                viewer.zoom = 1.;
                viewer.pan = Vec2::ZERO;
                ui.close();
            }
            if ui
                .add(
                    modern_ui::MenuAction::new(media_browser::ui_text!(msg_2ace8916ffa1))
                        .icon(icons::Glyph::Expand),
                )
                .clicked()
            {
                viewer.native_pending = true;
                ui.close();
            }
            for (clockwise, label) in [
                (true, media_browser::ui_text!(msg_bf663a9d7f0c)),
                (false, media_browser::ui_text!(msg_391cbaf3d139)),
            ] {
                if ui
                    .add(modern_ui::MenuAction::new(label).icon(if clockwise {
                        icons::Glyph::Rotate
                    } else {
                        icons::Glyph::RotateLeft
                    }))
                    .clicked()
                {
                    self.begin_rotation(clockwise, Some(record.id));
                    ui.close();
                }
            }
        } else {
            self.video_rotation_menu(ui, record);
            ui.separator();
            if ui
                .add(
                    modern_ui::MenuAction::new(if self.muted {
                        media_browser::ui_text!(msg_d10a1eff9182)
                    } else {
                        media_browser::ui_text!(msg_7db2e189ecb0)
                    })
                    .icon(if self.muted {
                        icons::Glyph::Muted
                    } else {
                        icons::Glyph::Volume
                    }),
                )
                .clicked()
            {
                self.muted = !self.muted;
                if let Some(p) = &self.player {
                    let _ = p
                        .lock()
                        .unwrap()
                        .set("mute", if self.muted { "yes" } else { "no" });
                }
                ui.close();
            }
            if ui
                .add(
                    modern_ui::MenuAction::new(media_browser::ui_text!(msg_f03e2b5312a8))
                        .icon(icons::Glyph::Play)
                        .key("Space"),
                )
                .clicked()
            {
                self.toggle_detail_pause();
                ui.close();
            }
            if ui
                .add(
                    modern_ui::MenuAction::new(media_browser::ui_text!(msg_3cf0d9e7fb39))
                        .icon(icons::Glyph::Refresh),
                )
                .clicked()
            {
                self.positions.remove(&record.id);
                self.resume_pending = None;
                *next = Some(record.id);
                self.request_seek(0., true);
                if let Some(p) = &self.player {
                    let _ = p.lock().unwrap().set("pause", "no");
                }
                ui.close();
            }
            ui.menu_button(media_browser::ui_text!(msg_fb57dd24c4e5), |ui| {
                for action in [
                    VideoEndAction::Next,
                    VideoEndAction::Replay,
                    VideoEndAction::Pause,
                ] {
                    ui.selectable_value(&mut self.video_end_action, action, action.label());
                }
            });
        }
        ui.separator();
        if ui
            .add(
                modern_ui::MenuAction::new(media_browser::ui_text!(msg_3e11925eb6cf))
                    .icon(icons::Glyph::Copy),
            )
            .clicked()
        {
            ui.ctx().copy_text(display_path(&record.file.path));
            ui.close();
        }
        if ui
            .add(
                modern_ui::MenuAction::new(media_browser::ui_text!(msg_9e3fc11213b9))
                    .icon(icons::Glyph::Folder),
            )
            .clicked()
        {
            if let Err(e) = crate::platform::reveal_file(&record.file.path) {
                self.status = e;
            }
            ui.close();
        }
        let delete_action = ui
            .add(
                modern_ui::MenuAction::new(
                    egui::RichText::new(media_browser::ui_text!(msg_5954acf17787))
                        .color(modern_ui::DANGER),
                )
                .icon(icons::Glyph::Trash)
                .danger()
                .key("Delete"),
            );
        if let Some(smoke)=&mut self.daily_smoke {smoke.delete_action=Some(delete_action.rect);}
        if delete_action.clicked() {
            self.begin_trash();
            ui.close();
        }
        ui.separator();
        let size = format!(
            "{:.2} MB",
            record.file.fingerprint.bytes as f64 / 1_000_000.
        );
        let metadata = if let Some((w, h)) = record.width.zip(record.height) {
            format!("{size}  ·  {w} × {h}")
        } else {
            size
        };
        ui.label(
            egui::RichText::new(metadata)
                .size(10.)
                .color(modern_ui::MUTED),
        );
        if let Some(smoke) = self.design_smoke.as_mut() {
            smoke.menu_rect = Some(ui.min_rect());
        }
    }

    fn detail_end_action(
        &mut self,
        viewer: &mut Viewer,
        prepared: &Prepared,
        next: &mut Option<MediaId>,
    ) {
        if self.presentation.mode != presentation_ui::Mode::Manual {
            let ctx = self.ctx.clone();
            self.presentation_tick(&ctx, viewer, prepared, next);
            return;
        }
        if next.is_some()
            || self.trash_releasing()
            || self.trash_blocks_media(viewer.id)
            || self.rotation.is_some()
            || self.player_media != Some(viewer.id)
            || self.playback_errors.contains_key(&viewer.id)
            || self.resume_pending.is_some()
            || self.seek_flight.is_some()
            || self.seeks.target().is_some()
            || egui::Popup::is_any_open(&self.ctx)
            || !self.ctx.input(|i| {
                i.focused
                    && i.viewport().minimized != Some(true)
                    && i.viewport().occluded != Some(true)
            })
        {
            return;
        }
        let Some(player) = self.player.clone() else {
            return;
        };
        let p = player.lock().unwrap();
        if !p.ready() {
            return;
        }
        if p.text("eof-reached").as_deref() != Some("yes") {
            viewer.end_handled = false;
            return;
        }
        if viewer.end_handled {
            return;
        }
        viewer.end_handled = true;
        match self.video_end_action {
            VideoEndAction::Next => {
                let position = prepared
                    .view
                    .sequence
                    .iter()
                    .position(|id| *id == viewer.id);
                *next = position.and_then(|pos| {
                    prepared.view.sequence[pos + 1..]
                        .iter()
                        .find(|id| {
                            prepared.lookup.get(id).is_some_and(|i| {
                                prepared.snapshot.files[*i].file.kind == MediaKind::Video
                            })
                        })
                        .copied()
                });
                if let Some(id) = next {
                    self.positions.remove(id);
                } else {
                    let _ = p.set("pause", "yes");
                    self.status = media_browser::ui_text!(msg_9bab400616ec).into();
                }
            }
            VideoEndAction::Replay => {
                if let Err(error) = p.seek(0.).and_then(|_| p.set("pause", "no")) {
                    self.status = error;
                }
            }
            VideoEndAction::Pause => {
                let _ = p.set("pause", "yes");
            }
        }
    }
}
