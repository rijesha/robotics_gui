//! Small widgets used across panels.

use crate::theme;
use crate::Station;
use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};
use rgf_core::reliable::{Failure, TxState};

/// How long a confirmation stays on screen.
const CONFIRMED_SHOWN_S: f64 = 5.0;

/// Delivery status of the latest send under `label`. Every command and setting in the
/// UI shows one of these, so nothing is assumed to have arrived.
pub fn tx_status(ui: &mut Ui, station: &Station, label: &str) {
    match station.client.state(label) {
        None => {}
        Some(TxState::Pending { .. }) => {
            ui.add(egui::Spinner::new().size(12.0));
            ui.colored_label(theme::WARN, "Sending…");
        }
        Some(TxState::Confirmed) => {
            if station.client.tx.settled_for(label, station.now).is_some_and(|age| age < CONFIRMED_SHOWN_S) {
                ui.colored_label(theme::OK, "✔ Confirmed");
            }
        }
        Some(TxState::NotSent(failure)) => {
            let (text, detail) = failure_text(failure);
            ui.colored_label(theme::BAD, format!("✖ {text}")).on_hover_text(detail);
        }
    }
}

pub fn failure_text(failure: Failure) -> (&'static str, &'static str) {
    match failure {
        Failure::Timeout => ("Not sent", "No confirmation came back from the vehicle. It may not have received this."),
        Failure::Rejected => ("Rejected", "The vehicle received this and refused it."),
        Failure::Mismatch => ("Not applied", "The vehicle read back a different value from the one sent."),
    }
}

/// A coloured pill with text.
pub fn chip(ui: &mut Ui, text: impl Into<String>, color: Color32) -> Response {
    let text = egui::RichText::new(text.into()).color(Color32::BLACK).strong();
    egui::Frame::new()
        .fill(color)
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| ui.label(text))
        .response
}

/// A label over a large value, for dashboards.
pub fn stat(ui: &mut Ui, label: &str, value: impl Into<String>) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(label).small().color(theme::DIM));
        ui.label(egui::RichText::new(value.into()).size(20.0).strong());
    });
}

/// Spring-centred joystick. Returns (x, y) in -1..1 with y up; zero when released.
pub fn joystick(ui: &mut Ui, size: f32) -> (f32, f32) {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::drag());
    let painter = ui.painter_at(rect);
    let radius = size / 2.0 - 4.0;
    let knob_radius = size * 0.16;
    let travel = radius - knob_radius;

    let mut offset = Vec2::ZERO;
    if response.dragged() {
        if let Some(pointer) = response.interact_pointer_pos() {
            offset = pointer - rect.center();
            if offset.length() > travel {
                offset = offset.normalized() * travel;
            }
        }
    }

    let visuals = ui.visuals();
    painter.circle(
        rect.center(),
        radius,
        visuals.extreme_bg_color,
        Stroke::new(1.5, visuals.widgets.inactive.fg_stroke.color),
    );
    painter.line_segment(
        [rect.center() - Vec2::new(travel, 0.0), rect.center() + Vec2::new(travel, 0.0)],
        Stroke::new(1.0, visuals.weak_text_color()),
    );
    painter.line_segment(
        [rect.center() - Vec2::new(0.0, travel), rect.center() + Vec2::new(0.0, travel)],
        Stroke::new(1.0, visuals.weak_text_color()),
    );
    let knob_color = if response.dragged() { theme::ACCENT } else { visuals.widgets.inactive.fg_stroke.color };
    painter.circle_filled(rect.center() + offset, knob_radius, knob_color);

    (offset.x / travel, -offset.y / travel)
}

/// Minimal line chart of recent values, scaled to `max`.
pub fn sparkline(ui: &mut Ui, values: impl ExactSizeIterator<Item = f32>, max: f32, size: Vec2, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2, ui.visuals().extreme_bg_color);
    let n = values.len();
    if n < 2 || max <= 0.0 {
        return;
    }
    let points: Vec<Pos2> = values
        .enumerate()
        .map(|(i, v)| {
            let x = rect.left() + rect.width() * i as f32 / (n - 1) as f32;
            let y = rect.bottom() - 1.0 - (rect.height() - 2.0) * (v / max).clamp(0.0, 1.0);
            Pos2::new(x, y)
        })
        .collect();
    painter.line(points, Stroke::new(1.5, color));
}

/// Largest rect of the given aspect ratio (width / height) centred in `outer`.
pub fn fit_rect(outer: Rect, aspect: f32) -> Rect {
    let size = if outer.width() / outer.height() > aspect {
        Vec2::new(outer.height() * aspect, outer.height())
    } else {
        Vec2::new(outer.width(), outer.width() / aspect)
    };
    Rect::from_center_size(outer.center(), size)
}
