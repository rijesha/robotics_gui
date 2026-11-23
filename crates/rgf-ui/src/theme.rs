//! Look and feel: one palette per theme, applied to egui's style and used directly by
//! custom-painted widgets.

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Theme};

/// Colours for one theme. Panels take these from [`of`] instead of hard-coding colours,
/// so everything follows the light/dark switch.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    /// Window and pane background.
    pub bg: Color32,
    /// Cards and popups.
    pub surface: Color32,
    /// Buttons, inputs and other raised controls.
    pub raised: Color32,
    pub border: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub accent: Color32,
    /// Text on an accent, ok or bad fill.
    pub on_accent: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub bad: Color32,
    /// Background and grid of the 3D lidar view.
    pub viewport: Color32,
    pub viewport_grid: Color32,
}

pub const DARK: Palette = Palette {
    bg: Color32::from_rgb(17, 20, 25),
    surface: Color32::from_rgb(25, 29, 36),
    raised: Color32::from_rgb(37, 43, 53),
    border: Color32::from_rgb(48, 55, 67),
    text: Color32::from_rgb(230, 233, 238),
    dim: Color32::from_rgb(140, 149, 163),
    accent: Color32::from_rgb(66, 142, 255),
    on_accent: Color32::WHITE,
    ok: Color32::from_rgb(64, 190, 116),
    warn: Color32::from_rgb(236, 170, 52),
    bad: Color32::from_rgb(240, 86, 82),
    viewport: Color32::from_rgb(11, 13, 17),
    viewport_grid: Color32::from_rgb(46, 52, 62),
};

pub const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(241, 243, 246),
    surface: Color32::from_rgb(255, 255, 255),
    raised: Color32::from_rgb(232, 235, 240),
    border: Color32::from_rgb(210, 215, 223),
    text: Color32::from_rgb(28, 33, 43),
    dim: Color32::from_rgb(98, 107, 121),
    accent: Color32::from_rgb(26, 106, 232),
    on_accent: Color32::WHITE,
    ok: Color32::from_rgb(24, 132, 66),
    warn: Color32::from_rgb(180, 106, 0),
    bad: Color32::from_rgb(208, 44, 48),
    viewport: Color32::from_rgb(226, 230, 236),
    viewport_grid: Color32::from_rgb(188, 195, 206),
};

// Colours drawn over imagery and video are the same in both themes.
pub const PATH: Color32 = Color32::from_rgb(255, 214, 64);
pub const MISSION: Color32 = Color32::from_rgb(255, 255, 255);
pub const SURVEY: Color32 = Color32::from_rgb(110, 226, 255);
pub const VEHICLE: Color32 = Color32::from_rgb(66, 142, 255);

pub fn palette(dark: bool) -> Palette {
    if dark {
        DARK
    } else {
        LIGHT
    }
}

/// Palette of the theme `ui` is drawn in.
pub fn of(ui: &egui::Ui) -> Palette {
    palette(ui.visuals().dark_mode)
}

/// Colour for a value in 0..1, low to high: blue, cyan, green, yellow, red.
pub fn ramp(t: f32) -> Color32 {
    const STOPS: [[f32; 3]; 5] =
        [[48.0, 84.0, 220.0], [40.0, 190.0, 220.0], [70.0, 210.0, 90.0], [240.0, 220.0, 60.0], [235.0, 70.0, 50.0]];
    let t = t.clamp(0.0, 1.0) * (STOPS.len() - 1) as f32;
    let i = (t.floor() as usize).min(STOPS.len() - 2);
    let f = t - i as f32;
    let c = |k: usize| (STOPS[i][k] + (STOPS[i + 1][k] - STOPS[i][k]) * f) as u8;
    Color32::from_rgb(c(0), c(1), c(2))
}

/// Blend `over` onto `base`; `amount` 0 gives `base`, 1 gives `over`.
pub fn mix(base: Color32, over: Color32, amount: f32) -> Color32 {
    let c = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * amount) as u8;
    Color32::from_rgb(c(base.r(), over.r()), c(base.g(), over.g()), c(base.b(), over.b()))
}

fn visuals(theme: Theme) -> egui::Visuals {
    let dark = theme == Theme::Dark;
    let p = palette(dark);
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    let radius = CornerRadius::same(6);

    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(8);
    v.extreme_bg_color = if dark { Color32::from_rgb(12, 14, 18) } else { Color32::from_rgb(250, 251, 252) };
    v.faint_bg_color = mix(p.surface, p.raised, 0.5);
    v.code_bg_color = p.raised;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.bad;
    v.override_text_color = None;
    v.selection.bg_fill = if dark { mix(p.bg, p.accent, 0.55) } else { mix(p.surface, p.accent, 0.16) };
    v.selection.stroke = Stroke::new(1.0, if dark { Color32::WHITE } else { mix(p.accent, Color32::BLACK, 0.25) });
    v.slider_trailing_fill = true;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.surface;
    w.noninteractive.weak_bg_fill = p.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = p.raised;
    w.inactive.weak_bg_fill = p.raised;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = radius;

    let hover = mix(p.raised, p.text, 0.10);
    w.hovered.bg_fill = hover;
    w.hovered.weak_bg_fill = hover;
    w.hovered.bg_stroke = Stroke::new(1.0, mix(p.border, p.text, 0.25));
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    let active = mix(p.raised, p.text, 0.18);
    w.active.bg_fill = active;
    w.active.weak_bg_fill = active;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.5, p.text);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    w.open.bg_fill = hover;
    w.open.weak_bg_fill = hover;
    w.open.bg_stroke = Stroke::new(1.0, p.border);
    w.open.fg_stroke = Stroke::new(1.0, p.text);
    w.open.corner_radius = radius;
    v
}

/// Install both themes. Call once at start-up; `dark` picks the one to start in.
pub fn install(ctx: &egui::Context, dark: bool) {
    for theme in [Theme::Dark, Theme::Light] {
        ctx.set_visuals_of(theme, visuals(theme));
    }
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.interact_size.y = 26.0;
        style.spacing.slider_width = 120.0;
        style.spacing.menu_margin = egui::Margin::same(8);
        style.text_styles = [
            (TextStyle::Small, FontId::new(11.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(13.5, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(17.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace)),
        ]
        .into();
    });
    set_dark(ctx, dark);
}

pub fn set_dark(ctx: &egui::Context, dark: bool) {
    ctx.set_theme(if dark { egui::ThemePreference::Dark } else { egui::ThemePreference::Light });
}

pub fn is_dark(ctx: &egui::Context) -> bool {
    ctx.theme() == Theme::Dark
}
