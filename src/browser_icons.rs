//! Official Lucide SVG assets. No per-icon hand-drawn geometry or icon-font fallback.
//! Rasterized once per physical size; tinted by egui for all interaction states.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Glyph {
    Cpu,
    Memory,
    Disk,
    Star,
    StarFilled,
    Minus,
    Left,
    Right,
    Down,
    Up,
    ArrowDown,
    Sidebar,
    Folder,
    Search,
    Refresh,
    Settings,
    Close,
    Video,
    Photo,
    Keyboard,
    Link,
    Palette,
    Rotate,
    RotateLeft,
    Copy,
    Trash,
    Play,
    Pause,
    Volume,
    Muted,
    Select,
    Expand,
    Maximize,
    Restore,
    Pin,
    PinOff,
    Square,
    Cut,
    Subtitles,
}
impl Glyph {
    fn svg(self) -> &'static str {
        match self {
            Self::Cpu => include_str!("../assets/icons/lucide/cpu.svg"),
            Self::Memory => include_str!("../assets/icons/lucide/memory-stick.svg"),
            Self::Disk => include_str!("../assets/icons/lucide/hard-drive.svg"),
            Self::Star | Self::StarFilled => include_str!("../assets/icons/lucide/star.svg"),
            Self::Minus => include_str!("../assets/icons/lucide/minus.svg"),
            Self::Left => include_str!("../assets/icons/lucide/chevron-left.svg"),
            Self::Right => include_str!("../assets/icons/lucide/chevron-right.svg"),
            Self::Down => include_str!("../assets/icons/lucide/chevron-down.svg"),
            Self::Up | Self::ArrowDown => include_str!("../assets/icons/lucide/arrow-up.svg"),
            Self::Sidebar => include_str!("../assets/icons/lucide/panel-left.svg"),
            Self::Folder => include_str!("../assets/icons/lucide/folder.svg"),
            Self::Search => include_str!("../assets/icons/lucide/search.svg"),
            Self::Refresh => include_str!("../assets/icons/lucide/refresh-cw.svg"),
            Self::Settings => include_str!("../assets/icons/lucide/settings-2.svg"),
            Self::Close => include_str!("../assets/icons/lucide/x.svg"),
            Self::Video => include_str!("../assets/icons/lucide/clapperboard.svg"),
            Self::Photo => include_str!("../assets/icons/lucide/image.svg"),
            Self::Keyboard => include_str!("../assets/icons/lucide/keyboard.svg"),
            Self::Link => include_str!("../assets/icons/lucide/link.svg"),
            Self::Palette => include_str!("../assets/icons/lucide/palette.svg"),
            Self::Rotate => include_str!("../assets/icons/lucide/rotate-cw.svg"),
            Self::RotateLeft => include_str!("../assets/icons/lucide/rotate-ccw.svg"),
            Self::Copy => include_str!("../assets/icons/lucide/copy.svg"),
            Self::Trash => include_str!("../assets/icons/lucide/trash.svg"),
            Self::Play => include_str!("../assets/icons/lucide/play.svg"),
            Self::Pause => include_str!("../assets/icons/lucide/pause.svg"),
            Self::Volume => include_str!("../assets/icons/lucide/volume-2.svg"),
            Self::Muted => include_str!("../assets/icons/lucide/volume-x.svg"),
            Self::Select => include_str!("../assets/icons/lucide/square-check.svg"),
            Self::Expand => include_str!("../assets/icons/lucide/maximize.svg"),
            Self::Maximize => include_str!("../assets/icons/lucide/maximize-2.svg"),
            Self::Restore => include_str!("../assets/icons/lucide/panels-top-left.svg"),
            Self::Pin => include_str!("../assets/icons/lucide/pin.svg"),
            Self::PinOff => include_str!("../assets/icons/lucide/pin-off.svg"),
            Self::Square => include_str!("../assets/icons/lucide/square.svg"),
            Self::Cut => include_str!("../assets/icons/lucide/scissors.svg"),
            Self::Subtitles => include_str!("../assets/icons/lucide/captions.svg"),
        }
    }
}
pub(super) fn paint(p: &egui::Painter, rect: Rect, glyph: Glyph, color: Color32) {
    let size = rect.width().min(rect.height()).min(16.);
    let target = Rect::from_center_size(rect.center(), egui::Vec2::splat(size));
    if size <= 0. || !p.is_visible() || !p.clip_rect().intersects(target) {
        return;
    }
    let pixels = (size * p.ctx().pixels_per_point()).ceil().clamp(1., 128.) as u32;
    let id = egui::Id::unique(("lucide-texture-v1", glyph, pixels));
    let cached = p
        .ctx()
        .data_mut(|data| data.get_temp::<egui::TextureHandle>(id));
    let texture = cached.unwrap_or_else(|| {
        // Only trusted, compile-time embedded upstream SVGs reach the parser.
        let mut svg = glyph.svg().replace("currentColor", "#ffffff");
        if glyph == Glyph::StarFilled {
            // Fill the original Lucide star path, preserving its exact outline.
            svg = svg.replace("fill=\"none\"", "fill=\"#ffffff\"");
        }
        let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default())
            .expect("validated bundled Lucide SVG");
        let mut pixmap = resvg::tiny_skia::Pixmap::new(pixels, pixels).expect("bounded icon size");
        let scale = pixels as f32 / 24.;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        if glyph == Glyph::ArrowDown {
            // Rotate the same upstream arrow mask180deg so shaft, weight,
            // centering and proportions match exactly in both directions.
            let flipped: Vec<u8> = pixmap
                .data()
                .chunks_exact(4)
                .rev()
                .flat_map(|p| p.iter().copied())
                .collect();
            pixmap.data_mut().copy_from_slice(&flipped);
        }
        let image = egui::ColorImage::from_rgba_premultiplied([pixels as usize; 2], pixmap.data());
        let texture = p.ctx().load_texture(
            format!("lucide-{glyph:?}-{pixels}"),
            image,
            egui::TextureOptions::LINEAR,
        );
        p.ctx()
            .data_mut(|data| data.insert_temp(id, texture.clone()));
        texture
    });
    p.image(
        texture.id(),
        target,
        Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
        color,
    );
}
/// Same geometry used by custom folder toggles and egui collapsing headers.
pub(super) fn collapse(ui: &egui::Ui, openness: f32, response: &egui::Response) {
    paint(
        ui.painter(),
        response.rect,
        if openness < 0.5 {
            Glyph::Right
        } else {
            Glyph::Down
        },
        ui.visuals().text_color(),
    );
}
pub(super) fn icon_button(ui: &mut egui::Ui, glyph: Glyph, label: &str) -> egui::Response {
    icon_button_with_size(ui, glyph, label, egui::vec2(28., 28.))
}
/// Window controls share one visible rectangle, icon size and focus treatment.
pub(super) fn paint_window_control(
    ui: &egui::Ui,
    r: Rect,
    glyph: Glyph,
    hovered: bool,
    focused: bool,
) {
    if hovered || focused {
        ui.painter().rect_filled(
            r,
            6.,
            if glyph == Glyph::Close {
                Color32::from_rgb(178, 52, 67)
            } else {
                Color32::from_white_alpha(18)
            },
        );
    }
    if focused {
        ui.painter().rect_stroke(
            r,
            6.,
            ui.visuals().selection.stroke,
            egui::StrokeKind::Inside,
        );
    }
    paint(
        ui.painter(),
        r,
        glyph,
        if hovered || focused {
            Color32::WHITE
        } else {
            super::modern_ui::MUTED
        },
    );
}
pub(super) fn window_button(ui: &mut egui::Ui, glyph: Glyph, label: &str) -> egui::Response {
    let (r, response) = ui.allocate_exact_size(egui::vec2(28., 28.), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    paint_window_control(ui, r, glyph, response.hovered(), response.has_focus());
    response.on_hover_text(label)
}
pub(super) fn floating_navigation_button(
    ui: &mut egui::Ui,
    glyph: Glyph,
    label: &str,
) -> egui::Response {
    let (hit, response) = ui.allocate_exact_size(egui::vec2(32., 28.), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    let t = ui.ctx().animate_bool_with_time(
        response.id.with("floating-nav-hover"),
        response.hovered() || response.has_focus(),
        0.12,
    );
    let circle = Rect::from_center_size(hit.center(), egui::vec2(28., 28.));
    let shadow = egui::epaint::Shadow {
        offset: [0, 2],
        blur: 8,
        spread: 0,
        color: Color32::from_black_alpha(if ui.is_enabled() { 100 } else { 35 }),
    };
    ui.painter().add(shadow.as_shape(circle, 14));
    let value = if ui.is_enabled() { 34. + t * 15. } else { 28. };
    ui.painter().rect_filled(
        circle,
        14.,
        Color32::from_rgb(value as u8, (value + 2.) as u8, (value + 8.) as u8),
    );
    ui.painter().rect_stroke(
        circle,
        14.,
        egui::Stroke::new(
            1.,
            Color32::from_white_alpha(if ui.is_enabled() { 40 } else { 15 }),
        ),
        egui::StrokeKind::Inside,
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            circle,
            14.,
            ui.visuals().selection.stroke,
            egui::StrokeKind::Inside,
        );
    }
    paint(
        ui.painter(),
        circle,
        glyph,
        if ui.is_enabled() {
            super::modern_ui::TEXT
        } else {
            super::modern_ui::MUTED.gamma_multiply(0.55)
        },
    );
    response.on_hover_text(label)
}
pub(super) fn compact_icon_button(ui: &mut egui::Ui, glyph: Glyph, label: &str) -> egui::Response {
    icon_button_with_size(ui, glyph, label, egui::vec2(20., 26.))
}
fn icon_button_with_size(
    ui: &mut egui::Ui,
    glyph: Glyph,
    label: &str,
    size: egui::Vec2,
) -> egui::Response {
    let (r, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    let t = ui.ctx().animate_bool_with_time(
        response.id.with("hover"),
        response.hovered() || response.has_focus(),
        ui.style().animation_time,
    );
    if t > 0. {
        ui.painter()
            .rect_filled(r, 6., Color32::from_white_alpha((18. * t) as u8));
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            r,
            6.,
            ui.visuals().selection.stroke,
            egui::StrokeKind::Inside,
        );
    }
    paint(
        ui.painter(),
        r,
        glyph,
        if ui.is_enabled() {
            if response.hovered() {
                Color32::WHITE
            } else {
                super::modern_ui::MUTED
            }
        } else {
            ui.visuals().weak_text_color()
        },
    );
    response.on_hover_text(label)
}

pub(super) fn labeled_button(
    ui: &mut egui::Ui,
    glyph: Glyph,
    label: &str,
    selected: bool,
) -> egui::Response {
    let color = if selected {
        super::modern_ui::TEXT
    } else {
        super::modern_ui::MUTED
    };
    let text = ui
        .painter()
        .layout_no_wrap(label.into(), egui::FontId::proportional(13.), color);
    let (r, response) = ui.allocate_exact_size(
        egui::vec2(text.size().x + 36., ui.spacing().interact_size.y.max(24.)),
        egui::Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    if selected || response.hovered() || response.has_focus() {
        ui.painter().rect_filled(
            r,
            6.,
            if selected {
                ui.visuals().selection.bg_fill
            } else {
                super::modern_ui::RAISED
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
    paint(
        ui.painter(),
        Rect::from_center_size(r.left_center() + egui::vec2(13., 0.), egui::vec2(16., 16.)),
        glyph,
        color,
    );
    ui.painter().galley(
        r.left_center() + egui::vec2(27., -text.size().y / 2.),
        text,
        color,
    );
    response
}

pub(super) fn selection_checkbox(ui: &mut egui::Ui, selected: &mut bool) -> egui::Response {
    let (r, mut response) = ui.allocate_exact_size(egui::vec2(20., 22.), egui::Sense::click());
    if response.clicked() {
        *selected = !*selected;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::CheckBox, ui.is_enabled(), *selected, "")
    });
    if response.hovered() || response.has_focus() {
        ui.painter().rect_filled(r, 4., super::modern_ui::RAISED);
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            r,
            4.,
            ui.visuals().selection.stroke,
            egui::StrokeKind::Inside,
        );
    }
    paint(
        ui.painter(),
        r,
        if *selected {
            Glyph::Select
        } else {
            Glyph::Square
        },
        if *selected {
            ui.visuals().selection.stroke.color
        } else {
            super::modern_ui::MUTED
        },
    );
    response
}

#[cfg(test)]
mod tests {
    #[test]
    fn bundled_lucide_masks_render_at_supported_scales() {
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../assets/icons/lucide/manifest.json")).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icons/lucide");
        for entry in manifest["files"].as_array().unwrap() {
            let name = entry["file"].as_str().unwrap();
            if !name.ends_with(".svg") {
                continue;
            }
            let svg = std::fs::read_to_string(root.join(name))
                .unwrap()
                .replace("currentColor", "#ffffff");
            let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).unwrap();
            assert_eq!(tree.size().width(), 24., "{name}");
            for pixels in [16, 24, 32, 48] {
                let mut mask = resvg::tiny_skia::Pixmap::new(pixels, pixels).unwrap();
                resvg::render(
                    &tree,
                    resvg::tiny_skia::Transform::from_scale(
                        pixels as f32 / 24.,
                        pixels as f32 / 24.,
                    ),
                    &mut mask.as_mut(),
                );
                assert!(
                    mask.data().chunks_exact(4).any(|p| p[3] > 100),
                    "invisible {name} at {pixels}px"
                );
                assert!(
                    mask.data()
                        .chunks_exact(4)
                        .all(|p| p[0] == p[3] && p[1] == p[3] && p[2] == p[3]),
                    "not a tintable white mask: {name}"
                );
            }
        }
    }
}
