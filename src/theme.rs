//! Visual identity for Simple embedded.

use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, TextStyle};

pub const BG: Color32 = Color32::from_rgb(12, 15, 20);
pub const GUTTER: Color32 = Color32::from_rgb(6, 8, 12);
pub const PANEL: Color32 = Color32::from_rgb(18, 22, 29);
pub const PANEL_RAISED: Color32 = Color32::from_rgb(24, 29, 38);
pub const SURFACE: Color32 = Color32::from_rgb(29, 35, 45);
pub const HOVER: Color32 = Color32::from_rgb(36, 44, 57);
pub const EDITOR_BG: Color32 = Color32::from_rgb(13, 17, 23);
pub const BORDER: Color32 = Color32::from_rgb(39, 47, 60);
pub const BORDER_SOFT: Color32 = Color32::from_rgb(31, 38, 49);

pub const TEXT: Color32 = Color32::from_rgb(218, 225, 235);
pub const TEXT_DIM: Color32 = Color32::from_rgb(145, 157, 174);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(91, 103, 120);

pub const ACCENT: Color32 = Color32::from_rgb(88, 196, 221);
pub const ACCENT_DARK: Color32 = Color32::from_rgb(29, 108, 128);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(23, 65, 77);
pub const ON_ACCENT: Color32 = Color32::from_rgb(7, 24, 29);

pub const SUCCESS: Color32 = Color32::from_rgb(114, 203, 139);
pub const WARNING: Color32 = Color32::from_rgb(233, 181, 100);
pub const ERROR: Color32 = Color32::from_rgb(239, 112, 118);
pub const INFO: Color32 = Color32::from_rgb(132, 170, 215);
pub const DIR: Color32 = Color32::from_rgb(220, 176, 100);

pub fn install(ctx: &egui::Context) {
    ctx.global_style_mut(|style| {
        style.visuals = egui::Visuals::dark();

        let visuals = &mut style.visuals;
        visuals.panel_fill = BG;
        visuals.window_fill = PANEL;
        visuals.extreme_bg_color = EDITOR_BG;
        visuals.faint_bg_color = SURFACE;
        visuals.code_bg_color = EDITOR_BG;
        visuals.override_text_color = Some(TEXT);
        visuals.selection.bg_fill = Color32::from_rgb(34, 82, 104);
        visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
        visuals.hyperlink_color = ACCENT;
        visuals.warn_fg_color = WARNING;
        visuals.error_fg_color = ERROR;

        visuals.widgets.noninteractive.bg_fill = PANEL;
        visuals.widgets.noninteractive.weak_bg_fill = PANEL;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER_SOFT);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_DIM);

        visuals.widgets.inactive.bg_fill = SURFACE;
        visuals.widgets.inactive.weak_bg_fill = SURFACE;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);

        visuals.widgets.hovered.bg_fill = HOVER;
        visuals.widgets.hovered.weak_bg_fill = HOVER;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT_DARK);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);

        visuals.widgets.active.bg_fill = ACCENT_SOFT;
        visuals.widgets.active.weak_bg_fill = ACCENT_SOFT;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
        visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);

        visuals.widgets.open.bg_fill = SURFACE;
        visuals.widgets.open.weak_bg_fill = SURFACE;
        visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, BORDER);
        visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, TEXT);

        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.interact_size.y = 28.0;
        style.spacing.scroll.bar_width = 9.0;
        style.interaction.resize_grab_radius_side = 7.0;
        style.animation_time = 0.12;

        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(19.0, FontFamily::Proportional),
        );
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(13.5, FontFamily::Proportional));
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(13.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(13.0, FontFamily::Monospace),
        );
    });
}

pub fn primary_button(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(text.into())
            .color(ON_ACCENT)
            .strong()
            .size(13.0),
    )
    .fill(ACCENT)
    .stroke(Stroke::NONE)
    .corner_radius(6)
    .min_size(egui::vec2(0.0, 28.0))
}

pub fn secondary_button(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(text.into()).color(TEXT).size(13.0))
        .fill(SURFACE)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(6)
        .min_size(egui::vec2(0.0, 28.0))
}
