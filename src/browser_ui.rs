//! Quiet, layered native desktop design tokens; no WebView.
pub(super) use super::components::*;
use super::*;
pub(super) fn quiet_menu(ui: &mut egui::Ui) {
    // Reused context-menu Areas must remain above a detail Modal raised on reopen.
    ui.ctx().move_to_top(ui.layer_id());
    ui.set_width(252.);
    ui.spacing_mut().item_spacing.y = 2.;
    ui.spacing_mut().button_padding = egui::vec2(9., 3.);
    let v = ui.visuals_mut();
    v.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.inactive.bg_fill = Color32::TRANSPARENT;
}
pub(super) struct MenuAction {
    text: egui::WidgetText,
    glyph: super::icons::Glyph,
    key: String,
    danger: bool,
}
impl MenuAction {
    pub fn new(text: impl Into<egui::WidgetText>) -> Self {
        Self {
            text: text.into(),
            glyph: super::icons::Glyph::Right,
            key: String::new(),
            danger: false,
        }
    }
    pub fn icon(mut self, glyph: super::icons::Glyph) -> Self {
        self.glyph = glyph;
        self
    }
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }
}
impl egui::Widget for MenuAction {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let (r, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().max(240.), 30.),
            egui::Sense::click(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), self.text.text())
        });
        let hover = ui.ctx().animate_bool_with_time(
            response.id.with("action-hover"),
            response.hovered() || response.has_focus(),
            ui.style().animation_time,
        );
        if hover > 0. {
            ui.painter().rect_filled(
                r.shrink(1.),
                5.,
                Color32::from_white_alpha((15. * hover) as u8),
            );
        }
        if response.has_focus() {
            ui.painter().rect_stroke(
                r.shrink(1.),
                5.,
                ui.visuals().selection.stroke,
                egui::StrokeKind::Inside,
            );
        }
        let color = if !ui.is_enabled() {
            Color32::from_gray(100)
        } else if self.danger {
            DANGER
        } else {
            ui.visuals().text_color()
        };
        super::icons::paint(
            ui.painter(),
            Rect::from_center_size(r.left_center() + egui::vec2(17., 0.), egui::vec2(16., 16.)),
            self.glyph,
            color.gamma_multiply(0.8),
        );
        let key = ui
            .painter()
            .layout_no_wrap(self.key, egui::FontId::proportional(11.), MUTED);
        let text = self.text.into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            (r.width() - 50. - key.size().x).max(30.),
            egui::TextStyle::Body,
        );
        ui.painter().galley(
            r.left_center() + egui::vec2(36., -text.size().y / 2.),
            text,
            color,
        );
        ui.painter().galley(
            egui::pos2(
                r.right() - 10. - key.size().x,
                r.center().y - key.size().y / 2.,
            ),
            key,
            MUTED,
        );
        response
    }
}
pub(super) fn apply_theme(ctx: &egui::Context) {
    apply_preferences(ctx, &Default::default());
}
pub(super) fn apply_preferences(ctx: &egui::Context, prefs: &super::workspace_ui::Appearance) {
    let mut style = (*ctx.style_of(egui::Theme::Dark)).clone();
    style.animation_time = if prefs.reduce_motion { 0. } else { 0.16 };
    style.spacing.item_spacing = egui::vec2(8., if prefs.compact { 3. } else { 6. });
    style.spacing.button_padding = egui::vec2(7., if prefs.compact { 2. } else { 4. });
    style.spacing.interact_size.y = if prefs.compact { 24. } else { 28. };
    style.spacing.slider_width = 96.;
    style.spacing.slider_rail_height = 3.;
    style.visuals.handle_shape = egui::style::HandleShape::Circle;
    style.visuals.slider_trailing_fill = true;
    style.visuals.panel_fill = CANVAS;
    style.visuals.window_fill = RAISED;
    style.visuals.extreme_bg_color = Color32::from_rgb(17, 17, 20);
    style.visuals.faint_bg_color = SURFACE;
    style.visuals.selection.bg_fill = prefs.accent().gamma_multiply(0.24);
    style.visuals.selection.stroke = egui::Stroke::new(1., prefs.accent());
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.weak_text_color = Some(MUTED);
    style.visuals.window_stroke = egui::Stroke::new(1., BORDER);
    style.visuals.window_corner_radius = egui::CornerRadius::same(12);
    style.visuals.menu_corner_radius = egui::CornerRadius::same(POPUP_RADIUS);
    style.visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 12],
        blur: 32,
        spread: 2,
        color: Color32::from_black_alpha(105),
    };
    style.visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 6],
        blur: 20,
        spread: 0,
        color: Color32::from_black_alpha(110),
    };
    for visual in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        visual.corner_radius = egui::CornerRadius::same(6);
        visual.bg_stroke = egui::Stroke::NONE;
    }
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(STROKE, BORDER);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(56, 57, 64);
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    style.visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(61, 62, 70);
    style.visuals.widgets.hovered.weak_bg_fill = RAISED;
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(70, 67, 82);
    style.visuals.widgets.active.weak_bg_fill = prefs.accent().gamma_multiply(0.22);
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1., prefs.accent());
    style.visuals.widgets.open.bg_fill = RAISED;
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(TYPE_BODY));
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::proportional(TYPE_BODY),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::proportional(TYPE_SMALL),
    );
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(19.));
    ctx.set_style_of(egui::Theme::Dark, style);
}
pub(super) fn tracked_button(
    ui: &mut egui::Ui,
    regions: &mut Vec<Rect>,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    let response = quiet_button(ui, text);
    regions.push(response.rect);
    response
}
impl Browser {
    pub(super) fn places_sidebar(&mut self, ui: &mut egui::Ui) {
        let current = self.watch_root.clone();
        egui::ScrollArea::vertical()
            .id_salt("places")
            .max_height(210.)
            .show(ui, |ui| {
                for (kind, title) in [
                    (0, media_browser::ui_text!(msg_d11e589a9c88)),
                    (1, media_browser::ui_text!(msg_de314b445e07)),
                    (2, media_browser::ui_text!(msg_6fe9a5e2e86b)),
                ] {
                    egui::CollapsingHeader::new(title)
                        .default_open(kind == 0)
                        .icon(|ui, openness, response| icons::collapse(ui, openness, response))
                        .show(ui, |ui| {
                            if kind == 0 {
                                if let Some(path) = &current {
                                    if ui
                                        .small_button(media_browser::ui_text!(msg_7777fbadecb7))
                                        .clicked()
                                    {
                                        self.places.toggle_pin(path);
                                    }
                                }
                            }
                            for place in self.places.list(kind) {
                                let name = place
                                    .path
                                    .file_name()
                                    .unwrap_or(place.path.as_os_str())
                                    .to_string_lossy();
                                let response = ui
                                    .add(egui::Button::new(name).frame(false).truncate())
                                    .on_hover_text(display_path(&place.path));
                                if response.clicked() {
                                    self.navigate(place.path.clone(), false);
                                }
                                response.context_menu(|ui| {
                                    if ui
                                        .button(if place.pinned {
                                            media_browser::ui_text!(msg_20590a6a7b9a)
                                        } else {
                                            media_browser::ui_text!(msg_d11e589a9c88)
                                        })
                                        .clicked()
                                    {
                                        self.places.toggle_pin(&place.path);
                                        ui.close();
                                    }
                                });
                            }
                        });
                }
            });
    }
    pub(super) fn modern_toolbar(&mut self, ui: &mut egui::Ui) {
        use super::icons::{icon_button, Glyph};
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(
                10,
                if self.workspace.appearance.compact {
                    3
                } else {
                    5
                },
            ))
            .show(ui, |ui| {
                // Register the caption behind the widgets, so controls retain
                // their own click/drag behavior while unused header space moves the window.
                let caption = ui.interact(
                    Rect::from_min_size(
                        ui.next_widget_position(),
                        egui::vec2(ui.available_width(), 32.),
                    ),
                    ui.scope_id().with("native-window-caption"),
                    egui::Sense::click_and_drag(),
                );
                if let Some(smoke) = &mut self.presentation_smoke {
                    smoke.caption = Some(caption.rect);
                }
                if caption.drag_started_by(egui::PointerButton::Primary) {
                    if let Some(smoke) = &mut self.presentation_smoke {
                        smoke.drag_requests += 1;
                    }
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if caption.double_clicked() {
                    let maximized = ui.input(|i| i.viewport().maximized == Some(true));
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                }
                ui.horizontal(|ui| {
                    let (brand, response) =
                        ui.allocate_exact_size(egui::vec2(26., 26.), egui::Sense::hover());
                    media_browser::branding::paint(ui.painter(), brand);
                    response.on_hover_text(media_browser::branding::NAME);
                    let brand = ui.add(
                        egui::Label::new(
                            egui::RichText::new(media_browser::ui_text!(msg_76be2a32c205))
                                .size(16.)
                                .italics(),
                        )
                        .sense(egui::Sense::click_and_drag()),
                    );
                    if let Some(smoke) = &mut self.design_smoke {
                        smoke.toolbar_brand = Some(brand.rect);
                    }
                    if brand.drag_started() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if brand.double_clicked() {
                        let max = ui.input(|i| i.viewport().maximized == Some(true));
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
                    }
                    ui.add_enabled_ui(self.history.can_back(), |ui| {
                        if icon_button(ui, Glyph::Left, media_browser::ui_text!(msg_ca034d13c48a))
                            .clicked()
                        {
                            self.history_move(false);
                        }
                    });
                    ui.add_enabled_ui(self.history.can_forward(), |ui| {
                        if icon_button(ui, Glyph::Right, media_browser::ui_text!(msg_18eabe1c6777))
                            .clicked()
                        {
                            self.history_move(true);
                        }
                    });
                    if icon_button(ui, Glyph::Up, media_browser::ui_text!(msg_963f3b5d1c4f))
                        .clicked()
                    {
                        if let Some(parent) = self
                            .watch_root
                            .as_ref()
                            .and_then(|p| p.parent())
                            .map(Path::to_owned)
                        {
                            self.navigate(parent, false);
                        }
                    }
                    ui.add_space(5.);
                    let path_width = (ui.available_width() - 132.).clamp(60., 420.);
                    if self.path_editing {
                        let edit = ui.add_sized(
                            [path_width, 28.],
                            egui::TextEdit::singleline(&mut self.root_text)
                                .hint_text(media_browser::ui_text!(msg_5ca8c9fcb6ec)),
                        );
                        if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            self.path_editing = false;
                            self.navigate(PathBuf::from(&self.root_text), false);
                        }
                    } else {
                        let name = self
                            .watch_root
                            .as_ref()
                            .map(|p| display_path(p))
                            .unwrap_or_else(|| media_browser::ui_text!(msg_4c19b190fb39).into());
                        let r = ui.add_sized(
                            [path_width, 28.],
                            egui::Button::new(
                                egui::RichText::new(name).color(Color32::from_gray(197)),
                            )
                            .frame(false)
                            .truncate(),
                        );
                        if let Some(smoke) = &mut self.presentation_smoke {
                            smoke.path = Some(r.rect);
                        }
                        if r.on_hover_text(&self.root_text).clicked() {
                            self.path_editing = true;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let close = self.window_close_button(ui);
                        if let Some(smoke) = &mut self.design_smoke {
                            smoke.window_close = Some(close.rect);
                        }
                        if close.clicked() {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        let max = ui.input(|i| i.viewport().maximized == Some(true));
                        let maximize = icons::window_button(
                            ui,
                            if max { Glyph::Restore } else { Glyph::Maximize },
                            media_browser::ui_text!(msg_9949b1fb5f67),
                        );
                        if let Some(smoke) = &mut self.design_smoke {
                            smoke.window_maximize = Some(maximize.rect);
                        }
                        if maximize.clicked() {
                            ui.ctx()
                                .send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
                        }
                        let minimize = icons::window_button(
                            ui,
                            Glyph::Minus,
                            media_browser::ui_text!(msg_ac29e57a46f4),
                        );
                        if let Some(smoke) = &mut self.design_smoke {
                            smoke.window_minimize = Some(minimize.rect);
                        }
                        if minimize.clicked() {
                            ui.ctx()
                                .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                    });
                });
                ui.add_space(3.);
                // Keep folder actions and settings together at the right of
                // row two. The central browsing controls can wrap independently.
                ui.horizontal_top(|ui| {
                    let action_width = 4. * 28. + 3. * ui.spacing().item_spacing.x;
                    let controls_width =
                        (ui.available_width() - action_width - ui.spacing().item_spacing.x).max(0.);
                    ui.allocate_ui_with_layout(
                        egui::vec2(controls_width, 28.),
                        egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true),
                        |ui| {
                            ui.set_min_width(controls_width);
                            let sidebar = icon_button(
                                ui,
                                Glyph::Sidebar,
                                media_browser::ui_text!(msg_1bd55f256f49),
                            );
                            if let Some(smoke) = &mut self.design_smoke {
                                smoke.toolbar_sidebar = Some(sidebar.rect);
                            }
                            self.sidebar_button_rect = Some(sidebar.rect);
                            if sidebar.clicked() {
                                if self.pinned {
                                    self.pinned = false;
                                    self.sidebar_open = false;
                                } else {
                                    self.sidebar_open = true;
                                }
                                self.outside_since = None;
                            }
                            toolbar_divider(ui);
                            let old = self.settings.clone();
                            for (filter, label) in [
                                (MediaFilter::All, media_browser::ui_text!(msg_2ee2a6e9a382)),
                                (
                                    MediaFilter::Images,
                                    media_browser::ui_text!(msg_d24c10d37db0),
                                ),
                                (
                                    MediaFilter::Videos,
                                    media_browser::ui_text!(msg_c20f7618d330),
                                ),
                            ] {
                                let response =
                                    ui.selectable_value(&mut self.settings.filter, filter, label);
                                if filter == MediaFilter::All {
                                    if let Some(smoke) = &mut self.design_smoke {
                                        smoke.toolbar_filter = Some(response.rect);
                                    }
                                }
                            }
                            toolbar_divider(ui);
                            self.favorites_toolbar(ui);
                            toolbar_divider(ui);
                            egui::ComboBox::from_id_salt("sort")
                                .width(82.)
                                .selected_text(match self.settings.sort {
                                    SortField::Name => media_browser::ui_text!(msg_a6e48adc26d6),
                                    SortField::Modified => {
                                        media_browser::ui_text!(msg_257bbcc44c83)
                                    }
                                    SortField::Size => media_browser::ui_text!(msg_b9bf6916437a),
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut self.settings.sort,
                                        SortField::Name,
                                        media_browser::ui_text!(msg_a6e48adc26d6),
                                    );
                                    ui.selectable_value(
                                        &mut self.settings.sort,
                                        SortField::Modified,
                                        media_browser::ui_text!(msg_257bbcc44c83),
                                    );
                                    ui.selectable_value(
                                        &mut self.settings.sort,
                                        SortField::Size,
                                        media_browser::ui_text!(msg_b9bf6916437a),
                                    );
                                });
                            if ui
                                .button(if self.settings.descending {
                                    media_browser::ui_text!(msg_4b77c3546823)
                                } else {
                                    media_browser::ui_text!(msg_ca715c0d8d83)
                                })
                                .clicked()
                            {
                                self.settings.descending = !self.settings.descending;
                            }
                            ui.checkbox(
                                &mut self.settings.type_groups,
                                media_browser::ui_text!(msg_4bd9567589ed),
                            );
                            toolbar_divider(ui);
                            ui.weak(media_browser::ui_text!(msg_a623478771b1));
                            let mut scale = media_browser::layout::preview_scale(self.target_width);
                            let size = ui.add(super::modern_ui::QuietSlider(
                                egui::Slider::new(&mut scale, 0.0..=1.0).show_value(false),
                            ));
                            if size.changed() {
                                self.target_width = media_browser::layout::preview_width(scale)
                                    .min(media_browser::layout::MAX_PREVIEW_WIDTH);
                            }
                            self.preview_size_rect = Some(size.rect);
                            if old != self.settings || size.changed() {
                                if old.filter != self.settings.filter {
                                    self.selection.clear();
                                    self.selection_anchor = None;
                                }
                                self.prepare_dirty = true;
                                self.prepare_after = Instant::now() + Duration::from_millis(80);
                                ui.ctx().request_repaint_after(Duration::from_millis(80));
                            }
                            toolbar_divider(ui);
                            if ui
                                .button(if self.selection.is_empty() {
                                    media_browser::ui_text!(msg_3a5040b68abf)
                                } else {
                                    media_browser::ui_text!(msg_74966e20df2e)
                                })
                                .clicked()
                            {
                                if self.selection.is_empty() {
                                    self.select_all();
                                } else {
                                    self.selection.clear();
                                }
                            }
                            if !self.selection.is_empty() {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(media_browser::ui_format!(
                                            msg_1c63eccd9001,
                                            self.selection.len()
                                        ))
                                        .color(self.workspace.appearance.accent()),
                                    )
                                    .wrap_mode(egui::TextWrapMode::Extend),
                                );
                                if icon_button(
                                    ui,
                                    Glyph::Rotate,
                                    media_browser::ui_text!(msg_9bd6e93b350e),
                                )
                                .clicked()
                                {
                                    self.begin_rotation(true, None);
                                }
                            }
                        },
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2(action_width, 28.),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            let search = icon_button(
                                ui,
                                Glyph::Search,
                                media_browser::ui_text!(msg_50ecb467a360),
                            );
                            if let Some(smoke) = &mut self.design_smoke {
                                smoke.toolbar_search = Some(search.rect);
                            }
                            if search.clicked() {
                                self.open_commands();
                            }
                            let refresh = icon_button(
                                ui,
                                Glyph::Refresh,
                                media_browser::ui_text!(msg_ae93d47bc513),
                            );
                            if let Some(smoke) = &mut self.design_smoke {
                                smoke.toolbar_refresh = Some(refresh.rect);
                            }
                            if refresh.clicked() {
                                self.refresh_in_place();
                            }
                            let folder = icon_button(
                                ui,
                                Glyph::Folder,
                                media_browser::ui_text!(msg_fcf8b4bff0df),
                            );
                            if let Some(smoke) = &mut self.design_smoke {
                                smoke.toolbar_folder = Some(folder.rect);
                            }
                            if folder.clicked() {
                                self.pick_media_folder();
                            }
                            let settings = icon_button(
                                ui,
                                Glyph::Settings,
                                media_browser::ui_text!(msg_df3d58c7d84b),
                            );
                            if let Some(smoke) = &mut self.design_smoke {
                                smoke.toolbar_settings = Some(settings.rect);
                            }
                            if settings.clicked() {
                                self.workspace.settings_open = true;
                            }
                        },
                    );
                });
            });
    }
    pub(super) fn select_all(&mut self) {
        if let Some(p) = &self.prepared {
            self.selection = p.view.sequence.iter().copied().collect();
        }
    }
    pub(super) fn toggle_selection(&mut self, id: MediaId, range: bool) {
        self.selected_folders.clear();
        if range {
            if let (Some(anchor), Some(p)) = (self.selection_anchor, &self.prepared) {
                if let (Some(a), Some(b)) = (
                    p.view.sequence.iter().position(|i| *i == anchor),
                    p.view.sequence.iter().position(|i| *i == id),
                ) {
                    self.selection
                        .extend(p.view.sequence[a.min(b)..=a.max(b)].iter().copied());
                    return;
                }
            }
        }
        if !self.selection.remove(&id) {
            self.selection.insert(id);
        }
        self.selection_anchor = Some(id);
    }
    pub(super) fn copy_selected_paths(&self) {
        if let Some(p) = &self.prepared {
            let text = p
                .view
                .sequence
                .iter()
                .filter(|id| self.selection.contains(id))
                .filter_map(|id| {
                    p.lookup
                        .get(id)
                        .map(|i| display_path(&p.snapshot.files[*i].file.path))
                })
                .collect::<Vec<_>>()
                .join("\r\n");
            self.ctx.copy_text(text);
        }
    }
    pub(super) fn media_menu(&mut self, ui: &mut egui::Ui, record: &MediaRecord) {
        quiet_menu(ui);
        self.favorite_menu(ui, &record.file.path);
        ui.set_min_width(230.);
        ui.add(
            egui::Label::new(
                egui::RichText::new(
                    record
                        .file
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy(),
                )
                .size(11.)
                .color(MUTED),
            )
            .truncate(),
        );
        ui.separator();
        if ui
            .add(
                modern_ui::MenuAction::new(media_browser::ui_text!(msg_5f90fbb83585))
                    .icon(icons::Glyph::Play),
            )
            .clicked()
        {
            self.open_detail(record.id);
            ui.close();
        }
        if record.file.kind == MediaKind::Image {
            let target = if self.selection.contains(&record.id) {
                None
            } else {
                Some(record.id)
            };
            if ui
                .add(
                    modern_ui::MenuAction::new(media_browser::ui_text!(msg_bf663a9d7f0c))
                        .icon(icons::Glyph::Rotate),
                )
                .clicked()
            {
                self.begin_rotation(true, target);
                ui.close();
            }
            if ui
                .add(
                    modern_ui::MenuAction::new(media_browser::ui_text!(msg_391cbaf3d139))
                        .icon(icons::Glyph::RotateLeft),
                )
                .clicked()
            {
                self.begin_rotation(false, target);
                ui.close();
            }
            if ui
                .add(
                    modern_ui::MenuAction::new(media_browser::ui_text!(msg_2ace8916ffa1))
                        .icon(icons::Glyph::Expand),
                )
                .clicked()
            {
                self.open_detail(record.id);
                if let Some(v) = &mut self.detail {
                    v.native_pending = true;
                }
                ui.close();
            }
        } else {
            self.video_rotation_menu(ui, record);
            ui.separator();
            if ui
                .add(
                    modern_ui::MenuAction::new(media_browser::ui_text!(msg_3cf0d9e7fb39))
                        .icon(icons::Glyph::Refresh),
                )
                .clicked()
            {
                self.positions.remove(&record.id);
                self.resume_pending = None;
                self.resume_starting = false;
                self.restore_flight = false;
                self.open_detail(record.id);
                if self
                    .player
                    .as_ref()
                    .is_some_and(|p| p.lock().unwrap().ready())
                {
                    self.request_seek(0., true);
                }
                ui.close();
            }
            if ui
                .add(
                    modern_ui::MenuAction::new(if self.muted {
                        media_browser::ui_text!(msg_78524b56a1ad)
                    } else {
                        media_browser::ui_text!(msg_7dfb22e765ee)
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
        }
        ui.separator();
        if ui
            .add(
                modern_ui::MenuAction::new(if self.selection.contains(&record.id) {
                    media_browser::ui_text!(msg_753dbc75f545)
                } else {
                    media_browser::ui_text!(msg_09418a2c64cb)
                })
                .icon(icons::Glyph::Select),
            )
            .clicked()
        {
            self.toggle_selection(record.id, false);
            ui.close();
        }
        if ui
            .add(
                modern_ui::MenuAction::new(media_browser::ui_text!(msg_5bb55ae730fc))
                    .icon(icons::Glyph::Select),
            )
            .clicked()
        {
            if let Some(p) = &self.prepared {
                self.selection.extend(
                    p.view
                        .sequence
                        .iter()
                        .filter(|id| {
                            p.lookup.get(id).is_some_and(|i| {
                                p.snapshot.files[*i].file.parent == record.file.parent
                            })
                        })
                        .copied(),
                );
            }
            ui.close();
        }
        if ui
            .add(
                modern_ui::MenuAction::new(media_browser::ui_text!(msg_40755a95fb42))
                    .icon(icons::Glyph::Select),
            )
            .clicked()
        {
            self.select_all();
            ui.close();
        }
        if !self.selection.is_empty()
            && ui
                .add(
                    modern_ui::MenuAction::new(media_browser::ui_format!(
                        msg_73de815eb68f,
                        self.selection.len()
                    ))
                    .icon(icons::Glyph::Copy),
                )
                .clicked()
        {
            self.copy_selected_files(false);
            ui.close();
        }
        ui.separator();
        if ui
            .add(
                modern_ui::MenuAction::new(media_browser::ui_text!(msg_21e4ce11b88b))
                    .icon(icons::Glyph::Folder),
            )
            .clicked()
        {
            if let Err(e) = crate::platform::reveal_file(&record.file.path) {
                self.status = e;
            }
            ui.close();
        }
        if ui
            .add(
                modern_ui::MenuAction::new(media_browser::ui_text!(msg_ac5a945b9b63))
                    .icon(icons::Glyph::Copy),
            )
            .clicked()
        {
            ui.ctx().copy_text(display_path(&record.file.path));
            ui.close();
        }
        ui.separator();
        if ui
            .add(
                modern_ui::MenuAction::new(
                    egui::RichText::new(if self.detail.is_some() {
                        media_browser::ui_text!(msg_b5f2bf828fc9).into()
                    } else {
                        media_browser::ui_format!(msg_b2f499639394, self.selection.len())
                    })
                    .color(Color32::LIGHT_RED),
                )
                .icon(icons::Glyph::Trash)
                .danger()
                .key("Delete"),
            )
            .clicked()
        {
            self.begin_trash();
            ui.close();
        }
        ui.collapsing(media_browser::ui_text!(msg_bc74d94581c0), |ui| {
            ui.label(media_browser::ui_format!(
                msg_a51617f0545e,
                record.file.fingerprint.bytes as f64 / 1_000_000.
            ));
            if let (Some(w), Some(h)) = (record.width, record.height) {
                ui.label(media_browser::ui_format!(msg_2f2763d91d85, h = h, w = w));
            }
            if let Some(d) = record.duration_ms {
                ui.label(media_browser::ui_format!(msg_054f7df5a21d, format_time(d)));
            }
        });
    }
    pub(super) fn preview_controls(&mut self, ui: &mut egui::Ui, id: MediaId, content: Rect) {
        let visible = content.intersect(ui.clip_rect());
        if visible.width() < 145. || visible.height() < 90. {
            return;
        }
        let bar = Rect::from_min_max(
            egui::pos2(visible.left() + 8., visible.bottom() - 76.),
            visible.right_bottom() - egui::vec2(8., 8.),
        );
        self.control_regions.push(bar);
        self.preview_controls_rect = Some(bar);
        if ui.input(|i| i.pointer.hover_pos().is_some_and(|p| bar.contains(p))) {
            self.hovered_video = Some(id);
        }
        ui.painter().rect_filled(bar, CARD_RADIUS, OVERLAY);
        let Some(player) = self.player.clone() else {
            return;
        };
        let mut seek = None;
        ui.scope_builder(
            egui::UiBuilder::new()
                .id_salt(("video-controls", id))
                .max_rect(bar.shrink(8.))
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |ui| {
                ui.spacing_mut().interact_size.y = 20.;
                ui.spacing_mut().button_padding = egui::vec2(6., 2.);
                ui.spacing_mut().item_spacing = egui::vec2(6., 2.);
                let p = player.lock().unwrap();
                let duration = p.number("duration").unwrap_or(0.);
                let mut time = self
                    .seeks
                    .target()
                    .unwrap_or_else(|| p.number("time-pos").unwrap_or(0.));
                ui.horizontal(|ui| {
                    if icons::icon_button(
                        ui,
                        if p.text("pause").as_deref() == Some("yes") {
                            icons::Glyph::Play
                        } else {
                            icons::Glyph::Pause
                        },
                        if p.text("pause").as_deref() == Some("yes") {
                            media_browser::ui_text!(msg_c3396195e91c)
                        } else {
                            media_browser::ui_text!(msg_8d12fc0d4eb2)
                        },
                    )
                    .clicked()
                    {
                        let _ = p.toggle_pause();
                    }
                    ui.label(
                        egui::RichText::new(if bar.width() < 220. {
                            format_time((time * 1000.) as u64)
                        } else {
                            format!(
                                "{} / {}",
                                format_time((time * 1000.) as u64),
                                format_time((duration * 1000.) as u64)
                            )
                        })
                        .size(11.),
                    );
                    if icons::icon_button(
                        ui,
                        if self.muted {
                            icons::Glyph::Muted
                        } else {
                            icons::Glyph::Volume
                        },
                        if self.muted {
                            media_browser::ui_text!(msg_f9e2ebfa3cbb)
                        } else {
                            media_browser::ui_text!(msg_3d85e020e1e3)
                        },
                    )
                    .clicked()
                    {
                        self.muted = !self.muted;
                        let _ = p.set("mute", if self.muted { "yes" } else { "no" });
                    }
                    if bar.width() > 320. {
                        let volume = ui.add_sized(
                            [65., 18.],
                            super::modern_ui::QuietSlider(
                                egui::Slider::new(&mut self.volume, 0.0..=400.).show_value(false),
                            ),
                        );
                        if volume.changed() {
                            let _ = p.set("volume", &format!("{:.1}", self.volume));
                        }
                    }
                });
                if duration > 0. {
                    ui.spacing_mut().slider_width = bar.width() - 16.;
                    let response = ui.add_sized(
                        [bar.width() - 16., 16.],
                        super::modern_ui::QuietSlider(
                            egui::Slider::new(&mut time, 0.0..=duration).show_value(false),
                        ),
                    );
                    self.preview_seek_rect = Some(response.rect);
                    if response.changed() {
                        seek = Some(time);
                    }
                }
            },
        );
        if let Some(time) = seek {
            self.request_seek(time, true);
        }
    }
}
