//! Dark theme, spacing, text styles and bundled monospace font (UI-SPEC).

use eframe::egui::{self, Color32, FontFamily, FontId, TextStyle};

pub const DOMINANT: Color32 = Color32::from_rgb(0x1E, 0x1F, 0x22);
pub const SECONDARY: Color32 = Color32::from_rgb(0x2B, 0x2D, 0x31);
pub const SEPARATOR: Color32 = Color32::from_rgb(0x3A, 0x3C, 0x41);
pub const ACCENT: Color32 = Color32::from_rgb(0x4C, 0x9A, 0xFF);
pub const DESTRUCTIVE: Color32 = Color32::from_rgb(0xF2, 0x55, 0x5A);
pub const TEXT: Color32 = Color32::from_rgb(0xD8, 0xDA, 0xDF);
pub const MUTED: Color32 = Color32::from_rgb(0x8B, 0x8F, 0x98);

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = DOMINANT;
    v.window_fill = DOMINANT;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(0x4C, 0x9A, 0xFF, 89);
    v.selection.stroke = egui::Stroke::new(1.0, ACCENT);
    ctx.set_visuals(v);

    ctx.global_style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 4.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(14.0, FontFamily::Proportional));
        style.text_styles.insert(
            TextStyle::Small,
            FontId::new(12.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(14.0, FontFamily::Monospace),
        );
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(14.0, FontFamily::Proportional),
        );
    });

    #[cfg(feature = "bundled-mono")]
    {
        use std::sync::Arc;
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "jetbrains-mono".to_owned(),
            Arc::new(egui::FontData::from_static(include_bytes!(
                "../assets/fonts/JetBrainsMono-Regular.ttf"
            ))),
        );
        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(0, "jetbrains-mono".to_owned());
        ctx.set_fonts(fonts);
    }
}
