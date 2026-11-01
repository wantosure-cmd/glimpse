//! Glimpse's own geometric mark. Drawn in logical points for crisp DPI scaling.
use eframe::egui::{self, Color32, Rect};
pub const NAME: &str = "Glimpse";
pub fn paint(p: &egui::Painter, r: Rect) {
    p.rect_filled(r, r.width() * 0.226, Color32::from_rgb(28, 27, 35));
    for (points, color) in [
        (
            [(48., 112.), (174., 48.), (210., 80.), (84., 144.)],
            Color32::from_rgb(186, 164, 245),
        ),
        (
            [(48., 176.), (174., 112.), (210., 144.), (84., 208.)],
            Color32::from_rgb(126, 103, 186),
        ),
    ] {
        p.add(egui::Shape::convex_polygon(
            points
                .into_iter()
                .map(|(x, y)| r.min + egui::vec2(x * r.width() / 256., y * r.height() / 256.))
                .collect(),
            color,
            egui::Stroke::NONE,
        ));
    }
}
pub fn icon() -> egui::IconData {
    let image = image::load_from_memory(include_bytes!("../assets/brand/glimpse.png"))
        .expect("bundled brand PNG")
        .into_rgba8();
    egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}
