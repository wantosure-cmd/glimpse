//! Shared visual vocabulary. Values are logical points and scale with egui DPI.
use super::*;

pub(super) const PRIMARY: Color32 = Color32::from_rgb(177, 160, 231);
pub(super) const CANVAS: Color32 = Color32::from_rgb(19, 20, 23);
pub(super) const SURFACE: Color32 = Color32::from_rgb(26, 27, 31);
pub(super) const RAISED: Color32 = Color32::from_rgb(32, 33, 38);
pub(super) const BORDER: Color32 = Color32::from_rgb(49, 50, 57);
pub(super) const MUTED: Color32 = Color32::from_rgb(145, 147, 158);
pub(super) const TEXT: Color32 = Color32::from_rgb(225, 226, 233);
pub(super) const DANGER: Color32 = Color32::from_rgb(232, 139, 146);
pub(super) const WARNING: Color32 = Color32::from_rgb(211, 183, 122);
pub(super) const OVERLAY: Color32 = Color32::from_rgba_premultiplied(22, 23, 26, 230);
pub(super) const CARD_RADIUS: f32 = 8.;
pub(super) const CONTROL_RADIUS: f32 = 6.;
pub(super) const POPUP_RADIUS: u8 = 10;
pub(super) const STROKE: f32 = 1.;
pub(super) const TYPE_SMALL: f32 = 11.;
pub(super) const TYPE_BODY: f32 = 13.;

pub(super) fn popup_frame(style: &egui::Style) -> egui::Frame {
    egui::Frame::popup(style)
        .fill(RAISED)
        .stroke(egui::Stroke::new(STROKE, BORDER))
        .corner_radius(POPUP_RADIUS)
        .inner_margin(8.)
}
pub(super) fn dialog_frame(style: &egui::Style) -> egui::Frame {
    popup_frame(style).corner_radius(12).inner_margin(20.)
}
pub(super) fn toast_frame(style: &egui::Style) -> egui::Frame {
    popup_frame(style).inner_margin(egui::Margin::symmetric(12, 9))
}
pub(super) fn overlay_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(OVERLAY)
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(8, 4))
}
pub(super) fn toolbar_divider(ui: &mut egui::Ui) {
    ui.add_space(4.);
    let (r, _) = ui.allocate_exact_size(egui::vec2(1., 14.), egui::Sense::hover());
    ui.painter().line_segment(
        [r.center_top(), r.center_bottom()],
        egui::Stroke::new(STROKE, BORDER),
    );
    ui.add_space(4.);
}
pub(super) fn quiet_button(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.add(egui::Button::new(text))
}
pub(super) struct QuietSlider<'a>(pub egui::Slider<'a>);
impl egui::Widget for QuietSlider<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let response = ui
            .scope(|ui| {
                let accent = ui.visuals().selection.stroke.color;
                let v = ui.visuals_mut();
                for (visual, color, expansion) in [
                    (&mut v.widgets.inactive, MUTED, -4.),
                    (&mut v.widgets.hovered, TEXT, -3.),
                    (&mut v.widgets.active, accent, -3.),
                ] {
                    visual.bg_fill = color;
                    visual.fg_stroke = egui::Stroke::NONE;
                    visual.expansion = expansion;
                }
                v.selection.bg_fill = accent.gamma_multiply(0.6);
                ui.add(self.0)
            })
            .inner;
        if response.has_focus() {
            ui.painter().rect_stroke(
                response.rect.shrink(1.),
                CONTROL_RADIUS,
                ui.visuals().selection.stroke,
                egui::StrokeKind::Inside,
            );
        }
        response
    }
}
pub(super) fn loading(p: &egui::Painter, rect: Rect, label: &str) {
    // Static, quiet placeholder: no animation-driven repaint for hundreds of tiles.
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(TYPE_SMALL),
        MUTED,
    );
}
pub(super) fn transport_row<R>(
    ui: &mut egui::Ui,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    egui::Frame::new()
        .fill(Color32::TRANSPARENT)
        .inner_margin(egui::Margin::symmetric(10, 0))
        .show(ui, |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 24.),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_height(24.);
                    body(ui)
                },
            )
            .inner
        })
}
pub(super) fn tracked_transport_icon(
    ui: &mut egui::Ui,
    regions: &mut Vec<Rect>,
    glyph: icons::Glyph,
    label: &str,
) -> egui::Response {
    let response = icons::icon_button_with_size(ui, glyph, label, egui::vec2(24., 24.));
    regions.push(response.rect);
    response
}
pub(super) fn tracked_icon(
    ui: &mut egui::Ui,
    regions: &mut Vec<Rect>,
    glyph: icons::Glyph,
    label: &str,
) -> egui::Response {
    let response = icons::icon_button(ui, glyph, label);
    regions.push(response.rect);
    response
}
pub(super) fn selection_mark(p: &egui::Painter, hit: Rect, selected: bool, accent: Color32) {
    let r = Rect::from_center_size(hit.center(), egui::vec2(16., 16.));
    p.rect_filled(
        Rect::from_center_size(hit.center(), egui::vec2(26., 26.)),
        CONTROL_RADIUS,
        OVERLAY,
    );
    if selected {
        // The official16pt square-check outline spans12pt between its
        // centerlines. Tint only that square, never the surrounding hit pad.
        p.rect_filled(
            Rect::from_center_size(hit.center(), egui::vec2(12., 12.)),
            1.3,
            accent,
        );
    }
    icons::paint(
        p,
        r,
        if selected {
            icons::Glyph::Select
        } else {
            icons::Glyph::Square
        },
        if selected { CANVAS } else { TEXT },
    );
}
pub(super) fn media_badge(ui: &egui::Ui, card: Rect, label: &str) {
    let text =
        ui.painter()
            .layout_no_wrap(label.into(), egui::FontId::proportional(TYPE_SMALL), TEXT);
    let width = (text.size().x + 30.).min((card.width() - 12.).max(30.));
    let r = Rect::from_min_size(
        card.right_bottom() - egui::vec2(width + 7., 25.),
        egui::vec2(width, 19.),
    );
    ui.painter().rect_filled(r, CONTROL_RADIUS, OVERLAY);
    icons::paint(
        ui.painter(),
        Rect::from_center_size(r.left_center() + egui::vec2(11., 0.), egui::vec2(12., 12.)),
        icons::Glyph::Video,
        TEXT,
    );
    ui.painter().galley(
        r.left_center() + egui::vec2(23., -text.size().y / 2.),
        text,
        TEXT,
    );
}
