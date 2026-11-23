//! Colours shared by all panels.

use egui::Color32;

pub const OK: Color32 = Color32::from_rgb(88, 200, 120);
pub const WARN: Color32 = Color32::from_rgb(240, 180, 60);
pub const BAD: Color32 = Color32::from_rgb(235, 84, 84);
pub const ACCENT: Color32 = Color32::from_rgb(90, 170, 255);
pub const PATH: Color32 = Color32::from_rgb(255, 214, 64);
pub const MISSION: Color32 = Color32::from_rgb(255, 255, 255);
pub const SURVEY: Color32 = Color32::from_rgb(120, 230, 255);
pub const DIM: Color32 = Color32::from_rgb(150, 156, 164);

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

pub fn apply(ctx: &egui::Context) {
    // A ground station is used outdoors and at night; one dark theme everywhere keeps
    // the overlay colours readable instead of following the system setting.
    ctx.set_theme(egui::ThemePreference::Dark);
}
