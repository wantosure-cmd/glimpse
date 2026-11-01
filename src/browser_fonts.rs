//! Keep bundled emoji fallbacks when installing either stage of Chinese fonts.
use eframe::egui;
use std::sync::Arc;

pub(super) fn definitions(full_chinese: Option<Vec<u8>>) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Chinese".into(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/ui-chinese.ttf"
        ))),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "Chinese".into());
    if let Some(bytes) = full_chinese {
        fonts.font_data.insert(
            "FullChinese".into(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .push("FullChinese".into());
        }
    }
    fonts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_emoji_have_visible_glyphs_before_and_after_chinese_font_upgrade() {
        for full in [
            None,
            Some(include_bytes!("../assets/ui-chinese.ttf").to_vec()),
        ] {
            let ctx = egui::Context::default();
            ctx.set_fonts(definitions(full));
            let mut output = ctx.run_pass(Default::default(), |ctx| {
                ctx.fonts_mut(|fonts| {
                    let font = egui::FontId::proportional(16.);
                    // The pre-fix default_fonts-only build had no woman/rocket
                    // glyphs and silently rendered these filename characters blank.
                    for character in ['照', '片', '👩', '🚀', '📷'] {
                        assert!(fonts.has_glyph(&font, character), "Missing {character}");
                    }
                    let text = fonts.layout_no_wrap("👩‍🚀".into(), font, egui::Color32::WHITE);
                    assert!(text.mesh_bounds.width() > 4. && text.mesh_bounds.height() > 4.);
                });
            });
            // This headless coverage test deliberately does not upload to a GPU.
            output.textures_delta.clear();
        }
    }
}
