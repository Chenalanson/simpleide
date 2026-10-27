//! Visual identity for Simple embedded.
//!
//! The whole application palette is derived from the selected editor theme,
//! so panels, menus, buttons and the native title bar follow the same look.

use std::cell::Cell;

use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, TextStyle};

use crate::codetheme::CodeTheme;

#[derive(Clone, Copy)]
struct Palette {
    dark: bool,
    bg: Color32,
    gutter: Color32,
    panel: Color32,
    panel_raised: Color32,
    surface: Color32,
    hover: Color32,
    editor_bg: Color32,
    border: Color32,
    border_soft: Color32,
    text: Color32,
    text_dim: Color32,
    text_faint: Color32,
    accent: Color32,
    accent_dark: Color32,
    accent_soft: Color32,
    on_accent: Color32,
    success: Color32,
    warning: Color32,
    error: Color32,
    info: Color32,
    dir: Color32,
}

impl Palette {
    const fn dark_default() -> Self {
        Self {
            dark: true,
            bg: Color32::from_rgb(12, 15, 20),
            gutter: Color32::from_rgb(6, 8, 12),
            panel: Color32::from_rgb(18, 22, 29),
            panel_raised: Color32::from_rgb(24, 29, 38),
            surface: Color32::from_rgb(29, 35, 45),
            hover: Color32::from_rgb(36, 44, 57),
            editor_bg: Color32::from_rgb(13, 17, 23),
            border: Color32::from_rgb(39, 47, 60),
            border_soft: Color32::from_rgb(31, 38, 49),
            text: Color32::from_rgb(218, 225, 235),
            text_dim: Color32::from_rgb(145, 157, 174),
            text_faint: Color32::from_rgb(91, 103, 120),
            accent: Color32::from_rgb(88, 196, 221),
            accent_dark: Color32::from_rgb(29, 108, 128),
            accent_soft: Color32::from_rgb(23, 65, 77),
            on_accent: Color32::from_rgb(7, 24, 29),
            success: Color32::from_rgb(114, 203, 139),
            warning: Color32::from_rgb(233, 181, 100),
            error: Color32::from_rgb(239, 112, 118),
            info: Color32::from_rgb(132, 170, 215),
            dir: Color32::from_rgb(220, 176, 100),
        }
    }

    fn from_code_theme(code: &CodeTheme) -> Self {
        let dark = code.dark;
        let bg = code.bg;
        let text = code.text;
        let gutter = if dark {
            blend(bg, Color32::BLACK, 0.42)
        } else {
            blend(bg, text, 0.09)
        };
        let accent = code.keyword;
        let accent_dark = if dark {
            blend(accent, Color32::BLACK, 0.35)
        } else {
            blend(accent, text, 0.30)
        };
        let accent_soft = if dark {
            blend(bg, accent, 0.18)
        } else {
            blend(bg, accent, 0.16)
        };
        let text_dim = blend(text, code.comment, 0.55);
        let text_faint = blend(text, bg, if dark { 0.48 } else { 0.42 });
        let on_accent = if luminance(accent) > 0.62 {
            Color32::from_rgb(8, 20, 24)
        } else {
            Color32::from_rgb(248, 250, 252)
        };
        let (success, warning, error, info, dir) = if dark {
            (
                Color32::from_rgb(114, 203, 139),
                Color32::from_rgb(233, 181, 100),
                Color32::from_rgb(239, 112, 118),
                Color32::from_rgb(132, 170, 215),
                Color32::from_rgb(220, 176, 100),
            )
        } else {
            (
                Color32::from_rgb(47, 125, 79),
                Color32::from_rgb(138, 90, 18),
                Color32::from_rgb(178, 59, 69),
                Color32::from_rgb(61, 111, 158),
                Color32::from_rgb(141, 98, 24),
            )
        };

        Self {
            dark,
            bg,
            gutter,
            panel: blend(bg, text, 0.035),
            panel_raised: blend(bg, text, 0.06),
            surface: blend(bg, text, 0.10),
            hover: blend(bg, text, 0.16),
            editor_bg: bg,
            border: blend(bg, text, 0.22),
            border_soft: blend(bg, text, 0.11),
            text,
            text_dim,
            text_faint,
            accent,
            accent_dark,
            accent_soft,
            on_accent,
            success,
            warning,
            error,
            info,
            dir,
        }
    }
}

thread_local! {
    static CURRENT: Cell<Palette> = const { Cell::new(Palette::dark_default()) };
}

fn current() -> Palette {
    CURRENT.with(Cell::get)
}

macro_rules! palette_colors {
    ($($name:ident => $field:ident),* $(,)?) => {
        $(
            pub fn $name() -> Color32 {
                current().$field
            }
        )*
    };
}

palette_colors! {
    bg => bg,
    gutter => gutter,
    panel => panel,
    panel_raised => panel_raised,
    surface => surface,
    hover => hover,
    editor_bg => editor_bg,
    border => border,
    border_soft => border_soft,
    text => text,
    text_dim => text_dim,
    text_faint => text_faint,
    accent => accent,
    accent_dark => accent_dark,
    accent_soft => accent_soft,
    on_accent => on_accent,
    success => success,
    warning => warning,
    error => error,
    info => info,
    dir => dir,
}

pub fn is_dark() -> bool {
    current().dark
}

pub fn with_alpha(color: Color32, alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

/// Readable foreground for an arbitrary filled button or badge.
pub fn text_on(color: Color32) -> Color32 {
    if luminance(color) > 0.62 {
        Color32::from_rgb(8, 20, 24)
    } else {
        Color32::from_rgb(248, 250, 252)
    }
}

pub fn set_code_theme(code: &CodeTheme) {
    CURRENT.with(|current| current.set(Palette::from_code_theme(code)));
}

pub fn apply(ctx: &egui::Context, code: &CodeTheme) {
    set_code_theme(code);
    install(ctx);
}

pub fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let mix = |a: u8, b: u8| {
        (f32::from(a) + (f32::from(b) - f32::from(a)) * t.clamp(0.0, 1.0)).round() as u8
    };
    Color32::from_rgb(mix(a.r(), b.r()), mix(a.g(), b.g()), mix(a.b(), b.b()))
}

fn luminance(color: Color32) -> f32 {
    (0.2126 * f32::from(color.r()) + 0.7152 * f32::from(color.g()) + 0.0722 * f32::from(color.b()))
        / 255.0
}

pub fn install(ctx: &egui::Context) {
    ctx.set_theme(if is_dark() {
        egui::ThemePreference::Dark
    } else {
        egui::ThemePreference::Light
    });
    ctx.global_style_mut(|style| {
        style.visuals = if is_dark() {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };

        // This is an end-user application, not an egui playground. The debug
        // overlays are enabled by default in debug builds and paint red boxes
        // around widgets whose screen rect happens to move during layout
        // animations. That looks like a rendering bug, so keep them off.
        #[cfg(debug_assertions)]
        {
            style.debug.show_unaligned = false;
            style.debug.warn_if_rect_changes_id = false;
        }

        let visuals = &mut style.visuals;
        visuals.panel_fill = bg();
        visuals.window_fill = panel_raised();
        visuals.window_stroke = Stroke::new(1.0_f32, border());
        visuals.menu_corner_radius = egui::CornerRadius::same(6);
        visuals.popup_shadow = egui::epaint::Shadow {
            offset: [0, 6],
            blur: 16,
            spread: 0,
            color: Color32::from_black_alpha(if is_dark() { 90 } else { 45 }),
        };
        visuals.extreme_bg_color = editor_bg();
        visuals.faint_bg_color = surface();
        visuals.code_bg_color = editor_bg();
        visuals.override_text_color = Some(text());
        visuals.selection.bg_fill = accent_soft();
        visuals.selection.stroke = Stroke::new(1.0_f32, accent());
        visuals.hyperlink_color = accent();
        visuals.warn_fg_color = warning();
        visuals.error_fg_color = error();
        visuals.text_cursor.stroke.color = accent();

        visuals.widgets.noninteractive.bg_fill = panel();
        visuals.widgets.noninteractive.weak_bg_fill = panel();
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, border_soft());
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, text_dim());

        visuals.widgets.inactive.bg_fill = surface();
        visuals.widgets.inactive.weak_bg_fill = surface();
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, border());
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, text());

        visuals.widgets.hovered.bg_fill = hover();
        visuals.widgets.hovered.weak_bg_fill = hover();
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, accent_dark());
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, text());

        visuals.widgets.active.bg_fill = accent_soft();
        visuals.widgets.active.weak_bg_fill = accent_soft();
        visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, accent());
        visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, text());

        visuals.widgets.open.bg_fill = surface();
        visuals.widgets.open.weak_bg_fill = surface();
        visuals.widgets.open.bg_stroke = Stroke::new(1.0_f32, border());
        visuals.widgets.open.fg_stroke = Stroke::new(1.0_f32, text());

        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.interact_size.y = 28.0;
        style.spacing.scroll.bar_width = 9.0;
        style.interaction.resize_grab_radius_side = 7.0;
        style.animation_time = 0.20;

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

pub fn primary_button(label: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(
        egui::RichText::new(label.into())
            .color(on_accent())
            .strong()
            .size(13.0),
    )
    .fill(accent())
    .stroke(Stroke::NONE)
    .corner_radius(6)
    .min_size(egui::vec2(0.0, 28.0))
}

pub fn secondary_button(label: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(egui::RichText::new(label.into()).color(text()).size(13.0))
        .fill(surface())
        .stroke(Stroke::new(1.0_f32, border()))
        .corner_radius(6)
        .min_size(egui::vec2(0.0, 28.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codetheme;

    #[test]
    fn light_code_theme_brightens_the_whole_app_palette() {
        let code = codetheme::by_name("硫酸纸").unwrap();
        set_code_theme(code);
        assert!(!is_dark());
        assert!(bg().r() > 200 && bg().g() > 200 && bg().b() > 200);
        assert!(text().r() < 80);
        assert!(panel().r() < bg().r());
        assert!(gutter().r() < bg().r());
        assert_ne!(accent(), bg());
    }

    #[test]
    fn dark_code_theme_keeps_dark_chrome() {
        let code = codetheme::by_name("示波器原色").unwrap();
        set_code_theme(code);
        assert!(is_dark());
        assert!(bg().r() < 40 && bg().g() < 40 && bg().b() < 40);
        assert!(text().r() > 150);
        assert!(panel().r() >= bg().r());
        assert!(gutter().r() <= bg().r());
    }

    #[test]
    fn contrast_text_flips_on_light_and_dark_fills() {
        assert!(text_on(Color32::from_rgb(240, 240, 240)).r() < 40);
        assert!(text_on(Color32::from_rgb(30, 30, 30)).r() > 200);
        assert_eq!(with_alpha(Color32::WHITE, 40).a(), 40);
    }
}
