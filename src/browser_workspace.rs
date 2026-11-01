//! Categorized native preferences and a keyboard-first local command palette.
use super::icons::{icon_button, Glyph};
use super::modern_ui::{MUTED, RAISED, SURFACE};
use super::*;
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Appearance {
    pub compact: bool,
    pub reduce_motion: bool,
    pub accent_index: usize,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            compact: false,
            reduce_motion: false,
            accent_index: 0,
        }
    }
}
impl Appearance {
    pub fn accent(&self) -> Color32 {
        match self.accent_index {
            1 => Color32::from_rgb(230, 154, 139),
            2 => Color32::from_rgb(160, 183, 204),
            _ => super::modern_ui::PRIMARY,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub(super) enum Category {
    #[default]
    General,
    Appearance,
    Video,
    Subtitles,
    Shortcuts,
    Integration,
}
impl Category {
    pub const ALL: [Self; 6] = [
        Self::General,
        Self::Appearance,
        Self::Video,
        Self::Subtitles,
        Self::Shortcuts,
        Self::Integration,
    ];
    fn label(self) -> &'static str {
        match self {
            Self::General => media_browser::ui_text!(msg_40fae00b7c6d),
            Self::Appearance => media_browser::ui_text!(msg_86a63f23a076),
            Self::Video => media_browser::ui_text!(msg_a2aee80dcb89),
            Self::Subtitles => media_browser::ui_text!(msg_60033a3bf561),
            Self::Shortcuts => media_browser::ui_text!(msg_ee2638183d3e),
            Self::Integration => media_browser::ui_text!(msg_2bc6f890600c),
        }
    }
    fn icon(self) -> Glyph {
        match self {
            Self::General => Glyph::Settings,
            Self::Appearance => Glyph::Palette,
            Self::Video => Glyph::Video,
            Self::Subtitles => Glyph::Subtitles,
            Self::Shortcuts => Glyph::Keyboard,
            Self::Integration => Glyph::Link,
        }
    }
    fn description(self) -> &'static str {
        match self {
            Self::General => media_browser::ui_text!(msg_1157904c1df6),
            Self::Appearance => media_browser::ui_text!(msg_81ad5e58049e),
            Self::Video => media_browser::ui_text!(msg_8690bfce38f7),
            Self::Subtitles => media_browser::ui_text!(msg_8cdc6a51638d),
            Self::Shortcuts => media_browser::ui_text!(msg_cd188968c0b4),
            Self::Integration => media_browser::ui_text!(msg_0aafa1d08850),
        }
    }
}
#[derive(Default)]
pub(super) struct Workspace {
    pub appearance: Appearance,
    pub settings_open: bool,
    pub category: Category,
    pub command_open: bool,
    pub query: String,
    pub selected: usize,
    pub(super) focus_search: bool,
    pub settings_rect: Option<Rect>,
    pub command_rect: Option<Rect>,
    pub category_regions: Vec<(Category, Rect)>,
    pub auto_fold_checkbox: Option<Rect>,
}
impl Workspace {
    pub fn is_open(&self) -> bool {
        self.settings_open || self.command_open
    }
}
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Command {
    Folder,
    Refresh,
    Settings(Category),
    All,
    Images,
    Videos,
    Name,
    Modified,
    Size,
    Reverse,
    Sidebar,
    Fullscreen,
    SelectAll,
    Clear,
    Copy,
    Next,
    Previous,
    Mute,
    Subtitle,
}
#[derive(Clone)]
struct Entry {
    action: Command,
    title: &'static str,
    group: &'static str,
    aliases: &'static str,
    key: &'static str,
    icon: Glyph,
}
fn entries() -> Vec<Entry> {
    use Command::*;
    vec![
        Entry {
            action: Folder,
            title: media_browser::ui_text!(msg_fcf8b4bff0df),
            group: media_browser::ui_text!(msg_433bdcb25776),
            aliases: "open folder dakai wenjianjia",
            key: "",
            icon: Glyph::Folder,
        },
        Entry {
            action: Refresh,
            title: media_browser::ui_text!(msg_6518ae147e1d),
            group: media_browser::ui_text!(msg_433bdcb25776),
            aliases: "refresh reload shuaxin",
            key: "",
            icon: Glyph::Refresh,
        },
        Entry {
            action: All,
            title: media_browser::ui_text!(msg_de08583f4a51),
            group: media_browser::ui_text!(msg_a8c81f93f2e0),
            aliases: "all media quanbu",
            key: "",
            icon: Glyph::Photo,
        },
        Entry {
            action: Images,
            title: media_browser::ui_text!(msg_7e8978c85dc9),
            group: media_browser::ui_text!(msg_a8c81f93f2e0),
            aliases: "images photos tupian",
            key: "",
            icon: Glyph::Photo,
        },
        Entry {
            action: Videos,
            title: media_browser::ui_text!(msg_4dea1c9e0d71),
            group: media_browser::ui_text!(msg_a8c81f93f2e0),
            aliases: "videos shipin",
            key: "",
            icon: Glyph::Video,
        },
        Entry {
            action: Name,
            title: media_browser::ui_text!(msg_70d21cd34437),
            group: media_browser::ui_text!(msg_a96c9a854190),
            aliases: "sort name wenjianming",
            key: "",
            icon: Glyph::Folder,
        },
        Entry {
            action: Modified,
            title: media_browser::ui_text!(msg_7067c19eadb6),
            group: media_browser::ui_text!(msg_a96c9a854190),
            aliases: "sort modified date time shijian",
            key: "",
            icon: Glyph::Refresh,
        },
        Entry {
            action: Size,
            title: media_browser::ui_text!(msg_57b1ad1c1066),
            group: media_browser::ui_text!(msg_a96c9a854190),
            aliases: "sort size daxiao",
            key: "",
            icon: Glyph::Disk,
        },
        Entry {
            action: Reverse,
            title: media_browser::ui_text!(msg_ebaf95ed3adb),
            group: media_browser::ui_text!(msg_a96c9a854190),
            aliases: "reverse ascending descending",
            key: "",
            icon: Glyph::Rotate,
        },
        Entry {
            action: Sidebar,
            title: media_browser::ui_text!(msg_030614ff2508),
            group: media_browser::ui_text!(msg_785d65c70fde),
            aliases: "sidebar folders mulu",
            key: "",
            icon: Glyph::Sidebar,
        },
        Entry {
            action: Fullscreen,
            title: media_browser::ui_text!(msg_aa8b6477c2ce),
            group: media_browser::ui_text!(msg_785d65c70fde),
            aliases: "fullscreen quanping",
            key: "F11",
            icon: Glyph::Expand,
        },
        Entry {
            action: SelectAll,
            title: media_browser::ui_text!(msg_93043aa1f502),
            group: media_browser::ui_text!(msg_c11330b85234),
            aliases: "select all quanxuan",
            key: "Ctrl+A",
            icon: Glyph::Select,
        },
        Entry {
            action: Clear,
            title: media_browser::ui_text!(msg_73335a971612),
            group: media_browser::ui_text!(msg_c11330b85234),
            aliases: "clear selection quxiao",
            key: "Esc",
            icon: Glyph::Close,
        },
        Entry {
            action: Copy,
            title: media_browser::ui_text!(msg_faa4789ec627),
            group: media_browser::ui_text!(msg_c11330b85234),
            aliases: "copy selected paths fuzhi",
            key: "Ctrl+C",
            icon: Glyph::Copy,
        },
        Entry {
            action: Next,
            title: media_browser::ui_text!(msg_63da108a4a62),
            group: media_browser::ui_text!(msg_979a332955c8),
            aliases: "next xiayizhang",
            key: media_browser::ui_text!(msg_f955177ad8b9),
            icon: Glyph::Right,
        },
        Entry {
            action: Previous,
            title: media_browser::ui_text!(msg_d32cd6b87c4a),
            group: media_browser::ui_text!(msg_979a332955c8),
            aliases: "previous shangyizhang",
            key: media_browser::ui_text!(msg_098508d17199),
            icon: Glyph::Left,
        },
        Entry {
            action: Mute,
            title: media_browser::ui_text!(msg_1ba2bd811e63),
            group: media_browser::ui_text!(msg_c3396195e91c),
            aliases: "mute sound jingyin",
            key: "",
            icon: Glyph::Video,
        },
        Entry {
            action: Subtitle,
            title: media_browser::ui_text!(msg_1e9b6406feba),
            group: media_browser::ui_text!(msg_c3396195e91c),
            aliases: "load subtitle srt zimu",
            key: "",
            icon: Glyph::Photo,
        },
    ]
    .into_iter()
    .chain(Category::ALL.into_iter().map(|c| Entry {
        action: Settings(c),
        title: match c {
            Category::General => media_browser::ui_text!(msg_4282571f76d4),
            Category::Appearance => media_browser::ui_text!(msg_5b325bdcdc15),
            Category::Video => media_browser::ui_text!(msg_520ae130476a),
            Category::Subtitles => media_browser::ui_text!(msg_9be63081b9a4),
            Category::Shortcuts => media_browser::ui_text!(msg_8f51b3e7f439),
            Category::Integration => media_browser::ui_text!(msg_3c663afe099f),
        },
        group: media_browser::ui_text!(msg_57907a69fc5a),
        aliases: match c {
            Category::General => "settings general shezhi changgui",
            Category::Appearance => "appearance theme waiguan",
            Category::Video => "settings playback bofang",
            Category::Subtitles => "settings subtitles zimu",
            Category::Shortcuts => "settings keyboard shortcuts kuaijiejian",
            Category::Integration => "settings system integration xitong",
        },
        key: if c == Category::General { "Ctrl+," } else { "" },
        icon: c.icon(),
    }))
    .collect()
}
// Unicode-aware subsequence matching. Exact substrings rank before fragmented aliases.
fn fuzzy_score(query: &str, text: &str) -> Option<usize> {
    let q = query.to_lowercase();
    let q = q.trim();
    let t = text.to_lowercase();
    if q.is_empty() {
        return Some(0);
    }
    if let Some(at) = t.find(q) {
        return Some(t[..at].chars().count());
    }
    let mut hay = t.chars().enumerate();
    let mut score = 80;
    let mut last = None;
    for c in q.chars().filter(|c| !c.is_whitespace()) {
        let (at, _) = hay.find(|(_, v)| *v == c)?;
        score += last.map(|v| at - v - 1).unwrap_or(at);
        last = Some(at);
    }
    Some(score)
}
fn section(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(14.);
    ui.label(egui::RichText::new(title).strong());
    ui.add_space(5.);
    egui::Frame::new()
        .fill(RAISED)
        .corner_radius(9)
        .inner_margin(14.)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            body(ui);
        });
}
fn hint(ui: &mut egui::Ui, key: &str) {
    if !key.is_empty() {
        egui::Frame::new()
            .fill(super::modern_ui::SURFACE)
            .corner_radius(4)
            .inner_margin(egui::Margin::symmetric(5, 2))
            .show(ui, |ui| {
                ui.label(egui::RichText::new(key).size(11.).color(MUTED));
            });
    }
}
impl Browser {
    pub(super) fn pick_media_folder(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            self.path_editing = false;
            self.navigate(path, false);
        }
    }
    pub(super) fn open_commands(&mut self) {
        self.workspace.settings_open = false;
        self.workspace.command_open = true;
        self.workspace.query.clear();
        self.workspace.selected = 0;
        self.workspace.focus_search = true;
        self.ctx.request_repaint();
    }
    pub(super) fn workspace_shortcuts(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if !raw.focused {
            return;
        }
        let protected = egui::Popup::is_any_open(ctx)
            || self.permanent_prompt.is_some()
            || self.rotation_message.is_some()
            || self.rotation.is_some()
            || self.trash_operation.is_some()
            || !self.trash_failures.is_empty();
        raw.events.retain(|e| {
            if let egui::Event::Key {
                key,
                pressed: true,
                repeat,
                modifiers,
                ..
            } = e
            {
                if (modifiers.ctrl || modifiers.command)
                    && !modifiers.alt
                    && !modifiers.shift
                    && !protected
                    && (!ctx.text_edit_focused() || self.workspace.is_open())
                {
                    if *key == egui::Key::K {
                        if !repeat {
                            if self.workspace.command_open {
                                self.workspace.command_open = false;
                            } else {
                                self.open_commands();
                            }
                        }
                        return false;
                    }
                    if *key == egui::Key::Comma {
                        if !repeat {
                            self.workspace.settings_open = !self.workspace.settings_open;
                            self.workspace.command_open = false;
                        }
                        return false;
                    }
                }
            }
            true
        });
    }
    pub(super) fn workspace_ui(&mut self, ctx: &egui::Context) {
        if self.workspace.settings_open {
            self.preferences_ui(ctx);
        }
        if self.workspace.command_open {
            self.command_ui(ctx);
        }
    }
    fn preferences_ui(&mut self, ctx: &egui::Context) {
        self.workspace.category_regions.clear();
        let old = self.workspace.appearance.clone();
        let screen = ctx.content_rect();
        let width = (screen.width() - 40.).clamp(320., 850.);
        let height = (screen.height() - 60.).clamp(260., 610.);
        let modal = egui::Modal::new(egui::Id::unique("preferences-workspace"))
            .frame(
                super::modern_ui::dialog_frame(&ctx.style_of(egui::Theme::Dark)).inner_margin(0.),
            )
            .backdrop_color(Color32::from_black_alpha(135))
            .show(ctx, |ui| {
                ui.set_width(width);
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(18, 12))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (mark, _) =
                                ui.allocate_exact_size(egui::vec2(23., 23.), egui::Sense::hover());
                            media_browser::branding::paint(ui.painter(), mark);
                            ui.label(
                                egui::RichText::new(media_browser::branding::NAME)
                                    .size(13.)
                                    .color(MUTED),
                            );
                            ui.label(
                                egui::RichText::new(media_browser::ui_text!(msg_57907a69fc5a))
                                    .strong()
                                    .size(15.),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if icon_button(
                                        ui,
                                        Glyph::Close,
                                        media_browser::ui_text!(msg_73b55a07a3e3),
                                    )
                                    .clicked()
                                    {
                                        self.workspace.settings_open = false;
                                    }
                                    hint(ui, "Ctrl+,");
                                },
                            );
                        });
                    });
                ui.separator();
                ui.horizontal_top(|ui| {
                    let nav_width = if width < 520. { 105. } else { 156. };
                    egui::Frame::new()
                        .fill(SURFACE)
                        .inner_margin(12.)
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .max_width(nav_width)
                                .id_salt("settings-nav")
                                .max_height((height - 158.).max(60.))
                                .show(ui, |ui| {
                                    ui.with_layout(
                                        egui::Layout::top_down(egui::Align::Min),
                                        |ui| {
                                            ui.set_width(nav_width);
                                            ui.set_min_height(height - 158.);
                                            ui.spacing_mut().item_spacing.y = 5.;
                                            for category in Category::ALL {
                                                let (r, response) = ui.allocate_exact_size(
                                                    egui::vec2(nav_width, 34.),
                                                    egui::Sense::click(),
                                                );
                                                let selected = self.workspace.category == category;
                                                self.workspace.category_regions.push((category, r));
                                                if selected || response.hovered() {
                                                    ui.painter().rect_filled(
                                                        r,
                                                        6.,
                                                        if selected {
                                                            ui.visuals().selection.bg_fill
                                                        } else {
                                                            RAISED
                                                        },
                                                    );
                                                }
                                                if response.has_focus() {
                                                    ui.painter().rect_stroke(
                                                        r,
                                                        6.,
                                                        ui.visuals().selection.stroke,
                                                        egui::StrokeKind::Inside,
                                                    );
                                                }
                                                super::icons::paint(
                                                    ui.painter(),
                                                    Rect::from_min_size(
                                                        r.min + egui::vec2(6., 8.),
                                                        egui::vec2(18., 18.),
                                                    ),
                                                    category.icon(),
                                                    if selected {
                                                        self.workspace.appearance.accent()
                                                    } else {
                                                        MUTED
                                                    },
                                                );
                                                ui.painter().text(
                                                    r.left_center() + egui::vec2(31., 0.),
                                                    egui::Align2::LEFT_CENTER,
                                                    category.label(),
                                                    egui::FontId::proportional(13.),
                                                    if selected { Color32::WHITE } else { MUTED },
                                                );
                                                if response.clicked() {
                                                    self.workspace.category = category;
                                                }
                                            }
                                        },
                                    );
                                });
                        });
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(18, 12))
                        .show(ui, |ui| {
                            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                ui.set_width((width - nav_width - 72.).max(160.));
                                egui::ScrollArea::both()
                                    .max_width((width - nav_width - 72.).max(160.))
                                    .id_salt(("preferences-content", self.workspace.category as u8))
                                    .max_height(height - 158.)
                                    .show(ui, |ui| {
                                        ui.heading(self.workspace.category.label());
                                        ui.add_space(4.);
                                        ui.label(
                                            egui::RichText::new(
                                                self.workspace.category.description(),
                                            )
                                            .color(MUTED),
                                        );
                                        self.preferences_page(ui);
                                    });
                            });
                        });
                });
                ui.separator();
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(18, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.small(media_browser::ui_text!(msg_202faaeaf44d));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    hint(ui, media_browser::ui_text!(msg_595f8dbe1732));
                                },
                            );
                        });
                    });
            });
        self.workspace.settings_rect = Some(modal.response.rect);
        if modal.should_close() {
            self.workspace.settings_open = false;
        }
        if self.workspace.appearance != old {
            super::modern_ui::apply_preferences(ctx, &self.workspace.appearance);
            self.prepare_dirty = true;
            if self.workspace.appearance.reduce_motion {
                self.media_transition = None;
            }
        }
    }
    fn preferences_page(&mut self, ui: &mut egui::Ui) {
        match self.workspace.category {
            Category::General => {
                ui.weak(format!("Glimpse {}", env!("CARGO_PKG_VERSION")));
                self.language_settings(ui);
                section(ui, media_browser::ui_text!(msg_1bd55f256f49), |ui| {
                    ui.checkbox(&mut self.pinned, media_browser::ui_text!(msg_e775e58960cf));
                    let response = ui.checkbox(
                        &mut self.auto_fold_empty,
                        media_browser::ui_text!(msg_auto_fold_empty),
                    );
                    self.workspace.auto_fold_checkbox = Some(response.rect);
                    if response.changed() {
                        self.prepare_dirty = true;
                        self.prepare_after = Instant::now();
                    }
                    ui.weak(media_browser::ui_text!(msg_auto_fold_empty_hint));
                    ui.weak(media_browser::ui_text!(msg_e2ea6c168595));
                });
                section(ui, media_browser::ui_text!(msg_b0fa41e940ff), |ui| {
                    ui.label(if self.root_text.is_empty() {
                        media_browser::ui_text!(msg_8af17333363a)
                    } else {
                        &self.root_text
                    });
                    ui.add_space(5.);
                    if ui
                        .button(media_browser::ui_text!(msg_0d6fad8138e7))
                        .clicked()
                    {
                        self.pick_media_folder();
                    }
                    ui.weak(media_browser::ui_text!(msg_ea7f6c178c19));
                });
            }
            Category::Appearance => {
                section(ui, media_browser::ui_text!(msg_785d65c70fde), |ui| {
                    ui.checkbox(
                        &mut self.workspace.appearance.compact,
                        media_browser::ui_text!(msg_ad2091a9e074),
                    );
                    ui.checkbox(
                        &mut self.workspace.appearance.reduce_motion,
                        media_browser::ui_text!(msg_e4fad4f2893d),
                    );
                    ui.weak(media_browser::ui_text!(msg_61c99adde949));
                });
                section(ui, media_browser::ui_text!(msg_f84e05423960), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (n, label) in [
                            media_browser::ui_text!(msg_c2f43ebe2000),
                            media_browser::ui_text!(msg_d339785fca5a),
                            media_browser::ui_text!(msg_d66d9f0fd779),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            ui.selectable_value(
                                &mut self.workspace.appearance.accent_index,
                                n,
                                label,
                            );
                        }
                    });
                    ui.add_space(8.);
                    let (r, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 38.),
                        egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(r, 7., super::modern_ui::SURFACE);
                    ui.painter().rect_stroke(
                        r,
                        7.,
                        egui::Stroke::new(
                            super::modern_ui::STROKE,
                            self.workspace.appearance.accent().gamma_multiply(0.82),
                        ),
                        egui::StrokeKind::Inside,
                    );
                    ui.painter().text(
                        r.center(),
                        egui::Align2::CENTER_CENTER,
                        media_browser::ui_text!(msg_8b5210fa8b38),
                        egui::FontId::proportional(13.),
                        self.workspace.appearance.accent(),
                    );
                });
            }
            Category::Video => {
                section(ui, media_browser::ui_text!(msg_07a1de47fac0), |ui| {
                    for action in [
                        VideoEndAction::Next,
                        VideoEndAction::Replay,
                        VideoEndAction::Pause,
                    ] {
                        ui.radio_value(&mut self.video_end_action, action, action.label());
                    }
                });
                section(ui, media_browser::ui_text!(msg_473400815c0f), |ui| {
                    let before = (self.muted, self.volume);
                    ui.checkbox(&mut self.muted, media_browser::ui_text!(msg_7db2e189ecb0));
                    ui.add(super::modern_ui::QuietSlider(
                        egui::Slider::new(&mut self.volume, 0.0..=400.)
                            .text(media_browser::ui_text!(msg_8bf8b9780d34)),
                    ));
                    if before != (self.muted, self.volume) {
                        if let Some(p) = &self.player {
                            let p = p.lock().unwrap();
                            let _ = p.set("mute", if self.muted { "yes" } else { "no" });
                            let _ = p.set("volume", &self.volume.to_string());
                        }
                    }
                    ui.weak(media_browser::ui_text!(msg_7efac23005b4));
                });
            }
            Category::Subtitles => {
                section(ui, media_browser::ui_text!(msg_fdb8d493f914), |ui| {
                    self.subtitle_settings_fields(ui)
                });
            }
            Category::Shortcuts => {
                section(ui, media_browser::ui_text!(msg_164b15ab7b65), |ui| {
                    use media_browser::shortcuts::DetailKey;
                    let old = self.shortcuts.clone();
                    for (label, key) in [
                        (
                            media_browser::ui_text!(msg_f8baad25a051),
                            &mut self.shortcuts.next,
                        ),
                        (
                            media_browser::ui_text!(msg_b88f02974ea0),
                            &mut self.shortcuts.previous,
                        ),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(label);
                            egui::ComboBox::from_id_salt(label)
                                .selected_text(key.label())
                                .show_ui(ui, |ui| {
                                    for v in DetailKey::ALL {
                                        ui.selectable_value(key, v, v.label());
                                    }
                                });
                        });
                    }
                    if self.shortcuts.next == self.shortcuts.previous {
                        self.shortcuts = old;
                        ui.colored_label(
                            super::modern_ui::DANGER,
                            media_browser::ui_text!(msg_2008a16a6ace),
                        );
                    }
                    if ui
                        .button(media_browser::ui_text!(msg_dcc045b9aeb2))
                        .clicked()
                    {
                        self.shortcuts = Default::default();
                    }
                });
                section(ui, media_browser::ui_text!(msg_e0782ab38fa3), |ui| {
                    for (action, key) in [
                        (media_browser::ui_text!(msg_4b495d8b87b8), "Ctrl+K"),
                        (media_browser::ui_text!(msg_57907a69fc5a), "Ctrl+,"),
                        (
                            media_browser::ui_text!(msg_f03e2b5312a8),
                            media_browser::ui_text!(msg_f8cfd254b8e4),
                        ),
                        (media_browser::ui_text!(msg_22ea3f47fee8), "F11"),
                        (
                            media_browser::ui_text!(msg_15de0ec4dcfb),
                            media_browser::ui_text!(msg_a6d0103345ea),
                        ),
                        (
                            media_browser::ui_text!(msg_a47b574dfbd9),
                            media_browser::ui_text!(msg_434547728579),
                        ),
                        (
                            media_browser::ui_text!(msg_a69fbfc8da41),
                            media_browser::ui_text!(msg_6123062e7e42),
                        ),
                        (
                            media_browser::ui_text!(msg_4fe7642ac0e6),
                            media_browser::ui_text!(msg_7ae34bbc866b),
                        ),
                        (
                            media_browser::ui_text!(msg_41966b13f0ae),
                            media_browser::ui_text!(msg_e1d2ca63024c),
                        ),
                        (media_browser::ui_text!(msg_361bdf3f36e3), "Delete"),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(action);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    hint(ui, key);
                                },
                            );
                        });
                    }
                });
            }
            Category::Integration => {
                section(ui, media_browser::ui_text!(msg_17abd349b511), |ui| {
                    self.shell_settings(ui);
                });
                section(ui, media_browser::ui_text!(msg_57e88a43ef2b), |ui| {
                    ui.label(media_browser::ui_text!(msg_acc20f834d96));
                    ui.weak(media_browser::ui_text!(msg_67de3be1d6a3));
                });
            }
        }
    }
    fn command_enabled(&self, c: Command) -> bool {
        match c {
            Command::Copy => !self.selection.is_empty(),
            Command::Next | Command::Previous => self.detail.is_some(),
            Command::Subtitle => self
                .detail
                .as_ref()
                .is_some_and(|v| Some(v.id) == self.player_media),
            _ => true,
        }
    }
    fn command_ui(&mut self, ctx: &egui::Context) {
        let mut chosen = None;
        let width = (ctx.content_rect().width() - 48.).clamp(290., 640.);
        let up = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp));
        let down = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
        let enter = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let modal = egui::Modal::new(egui::Id::unique("command-workspace"))
            .frame(
                super::modern_ui::dialog_frame(&ctx.style_of(egui::Theme::Dark)).inner_margin(0.),
            )
            .backdrop_color(Color32::from_black_alpha(145))
            .show(ctx, |ui| {
                ui.set_width(width);
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(16, 13))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (r, _) =
                                ui.allocate_exact_size(egui::vec2(20., 24.), egui::Sense::hover());
                            super::icons::paint(ui.painter(), r, Glyph::Search, MUTED);
                            let edit = ui.add_sized(
                                [ui.available_width() - 55., 30.],
                                egui::TextEdit::singleline(&mut self.workspace.query)
                                    .frame(egui::Frame::NONE)
                                    .font(egui::FontId::proportional(17.))
                                    .hint_text(media_browser::ui_text!(msg_888c641e1b27)),
                            );
                            if self.workspace.focus_search {
                                edit.request_focus();
                                self.workspace.focus_search = false;
                            }
                            if edit.changed() {
                                self.workspace.selected = 0;
                            }
                            hint(ui, "Esc");
                        });
                    });
                ui.separator();
                let mut results: Vec<_> = entries()
                    .into_iter()
                    .enumerate()
                    .filter_map(|(n, e)| {
                        if !self.command_enabled(e.action) {
                            return None;
                        }
                        fuzzy_score(&self.workspace.query, &format!("{} {}", e.title, e.aliases))
                            .map(|score| (score, n, e))
                    })
                    .collect();
                results.sort_by_key(|r| (r.0, r.1));
                if results.is_empty() {
                    egui::Frame::new().inner_margin(28.).show(ui, |ui| {
                        ui.label(media_browser::ui_text!(msg_556513743902));
                        ui.weak(media_browser::ui_text!(msg_6abec1810c86));
                    });
                } else {
                    self.workspace.selected = self.workspace.selected.min(results.len() - 1);
                    if down {
                        self.workspace.selected = (self.workspace.selected + 1) % results.len();
                    }
                    if up {
                        self.workspace.selected =
                            (self.workspace.selected + results.len() - 1) % results.len();
                    }
                    egui::ScrollArea::vertical()
                        .max_height((ctx.content_rect().height() - 210.).clamp(110., 390.))
                        .show(ui, |ui| {
                            egui::Frame::new().inner_margin(7.).show(ui, |ui| {
                                for (index, (_, _, entry)) in results.iter().enumerate() {
                                    let (r, response) = ui.allocate_exact_size(
                                        egui::vec2(width - 14., 46.),
                                        egui::Sense::click(),
                                    );
                                    if response.hovered()
                                        && ui.input(|i| i.pointer.delta() != Vec2::ZERO)
                                    {
                                        self.workspace.selected = index;
                                    }
                                    let selected = self.workspace.selected == index;
                                    if selected {
                                        ui.painter().rect_filled(
                                            r,
                                            7.,
                                            ui.visuals().selection.bg_fill,
                                        );
                                    }
                                    super::icons::paint(
                                        ui.painter(),
                                        Rect::from_min_size(
                                            r.min + egui::vec2(10., 14.),
                                            egui::vec2(20., 20.),
                                        ),
                                        entry.icon,
                                        if selected {
                                            self.workspace.appearance.accent()
                                        } else {
                                            MUTED
                                        },
                                    );
                                    ui.painter().text(
                                        r.min + egui::vec2(43., 9.),
                                        egui::Align2::LEFT_TOP,
                                        entry.title,
                                        egui::FontId::proportional(13.),
                                        super::modern_ui::TEXT,
                                    );
                                    ui.painter().text(
                                        r.min + egui::vec2(43., 27.),
                                        egui::Align2::LEFT_TOP,
                                        entry.group,
                                        egui::FontId::proportional(10.),
                                        MUTED,
                                    );
                                    ui.painter().text(
                                        r.right_center() - egui::vec2(12., 0.),
                                        egui::Align2::RIGHT_CENTER,
                                        match entry.action {
                                            Command::Next => self.shortcuts.next.label(),
                                            Command::Previous => self.shortcuts.previous.label(),
                                            _ => entry.key,
                                        },
                                        egui::FontId::proportional(11.),
                                        MUTED,
                                    );
                                    if selected && (up || down) {
                                        ui.scroll_to_rect(r, Some(egui::Align::Center));
                                    }
                                    if response.clicked() {
                                        chosen = Some(entry.action);
                                    }
                                }
                            });
                        });
                    if enter {
                        chosen = Some(results[self.workspace.selected].2.action);
                    }
                }
                ui.separator();
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(16, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.weak(media_browser::ui_text!(msg_928f87d4507b));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    hint(ui, media_browser::ui_text!(msg_c22c24c89d11));
                                    hint(ui, media_browser::ui_text!(msg_a6560b60a35d));
                                },
                            );
                        });
                    });
            });
        self.workspace.command_rect = Some(modal.response.rect);
        if modal.should_close() {
            self.workspace.command_open = false;
        }
        if let Some(c) = chosen {
            self.workspace.command_open = false;
            self.execute_command(c);
        }
    }
    fn execute_command(&mut self, c: Command) {
        let old = self.settings.clone();
        match c {
            Command::Folder => self.pick_media_folder(),
            Command::Refresh => self.refresh_in_place(),
            Command::Settings(category) => {
                self.workspace.category = category;
                self.workspace.settings_open = true;
            }
            Command::All => self.settings.filter = MediaFilter::All,
            Command::Images => self.settings.filter = MediaFilter::Images,
            Command::Videos => self.settings.filter = MediaFilter::Videos,
            Command::Name => self.settings.sort = SortField::Name,
            Command::Modified => self.settings.sort = SortField::Modified,
            Command::Size => self.settings.sort = SortField::Size,
            Command::Reverse => self.settings.descending = !self.settings.descending,
            Command::Sidebar => {
                self.pinned = !self.pinned;
                self.sidebar_open = self.pinned;
            }
            Command::Fullscreen => {
                self.fullscreen = !self.fullscreen;
                self.ctx
                    .send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
            }
            Command::SelectAll => self.select_all(),
            Command::Clear => {
                self.selection.clear();
                self.selected_folders.clear();
            }
            Command::Copy => self.copy_selected_paths(),
            Command::Next | Command::Previous => {
                if let (Some(v), Some(p)) = (&self.detail, &self.prepared) {
                    if let Some(id) = view::neighbor(
                        &p.view.sequence,
                        v.id,
                        if c == Command::Next { 1 } else { -1 },
                    ) {
                        self.open_detail(id);
                    }
                }
            }
            Command::Mute => {
                self.muted = !self.muted;
                if let Some(p) = &self.player {
                    let _ = p
                        .lock()
                        .unwrap()
                        .set("mute", if self.muted { "yes" } else { "no" });
                }
            }
            Command::Subtitle => self.pick_subtitle(),
        }
        if old != self.settings {
            if old.filter != self.settings.filter {
                self.selection.clear();
                self.selection_anchor = None;
            }
            self.prepare_dirty = true;
            self.prepare_after = Instant::now();
        }
        self.ctx.request_repaint();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fuzzy_chinese_english_and_noncontiguous_search() {
        assert!(fuzzy_score("字幕", "设置 · 字幕 settings subtitles zimu").is_some());
        assert!(fuzzy_score("srt", "load subtitle srt").is_some());
        assert!(fuzzy_score("rfsh", "refresh reload").is_some());
        assert!(fuzzy_score("ZZZZ", "refresh").is_none());
        assert!(
            fuzzy_score("folder", "open folder").unwrap()
                < fuzzy_score("fldr", "open folder").unwrap()
        );
    }
    #[test]
    fn preferences_compatibility_and_safe_catalog() {
        let v: Appearance = serde_json::from_str("{}").unwrap();
        assert!(!v.compact);
        assert_eq!(entries().len(), 24);
        assert!(!entries().iter().any(|e| e.title.contains("删除")));
    }
    #[test]
    fn existing_config_roundtrip_preserves_media_preferences() {
        let old = Config::default();
        let mut json = serde_json::to_value(&old).unwrap();
        json.as_object_mut().unwrap().remove("appearance");
        json["volume"] = serde_json::json!(42.0);
        json["pinned"] = serde_json::json!(true);
        let mut config: Config = serde_json::from_value(json).unwrap();
        assert!(!config.appearance.compact);
        assert_eq!(config.volume, 42.);
        assert!(config.pinned);
        config.appearance.accent_index = 2;
        let restored: Config =
            serde_json::from_slice(&serde_json::to_vec(&config).unwrap()).unwrap();
        assert_eq!(restored.volume, 42.);
        assert_eq!(restored.appearance.accent_index, 2);
        assert!(restored.pinned);
        let old: Config = serde_json::from_str("{}").unwrap();
        assert!(old.auto_fold_empty);
        let mut old = old;
        old.auto_fold_empty = false;
        let roundtrip: Config = serde_json::from_slice(&serde_json::to_vec(&old).unwrap()).unwrap();
        assert!(!roundtrip.auto_fold_empty);
    }
}
