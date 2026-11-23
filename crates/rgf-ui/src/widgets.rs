//! Small widgets used across panels.

use crate::theme::{self, Palette};
use crate::Station;
use egui::{Color32, Pos2, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};
use rgf_core::reliable::{Failure, TxState};

/// How long a confirmation stays on screen.
const CONFIRMED_SHOWN_S: f64 = 5.0;

/// Delivery status of the latest send under `label`. Every command and setting in the
/// UI shows one of these, so nothing is assumed to have arrived.
pub fn tx_status(ui: &mut Ui, station: &Station, label: &str) {
    let p = theme::of(ui);
    match station.client.state(label) {
        None => {}
        Some(TxState::Pending { .. }) => {
            ui.add(egui::Spinner::new().size(12.0).color(p.warn));
            ui.colored_label(p.warn, "Sending…");
        }
        Some(TxState::Confirmed) => {
            if station.client.tx.settled_for(label, station.now).is_some_and(|age| age < CONFIRMED_SHOWN_S) {
                ui.colored_label(p.ok, "✔ Confirmed");
            }
        }
        Some(TxState::NotSent(failure)) => {
            let (text, detail) = failure_text(failure);
            ui.colored_label(p.bad, RichText::new(format!("✖ {text}")).strong()).on_hover_text(detail);
        }
    }
}

/// Whether `tx_status` would draw anything for `label` right now.
pub fn tx_visible(station: &Station, label: &str) -> bool {
    match station.client.state(label) {
        None => false,
        Some(TxState::Confirmed) => {
            station.client.tx.settled_for(label, station.now).is_some_and(|age| age < CONFIRMED_SHOWN_S)
        }
        Some(_) => true,
    }
}

pub fn failure_text(failure: Failure) -> (&'static str, &'static str) {
    match failure {
        Failure::Timeout => ("Not sent", "No confirmation came back from the vehicle. It may not have received this."),
        Failure::Rejected => ("Rejected", "The vehicle received this and refused it."),
        Failure::Mismatch => ("Not applied", "The vehicle read back a different value from the one sent."),
    }
}

/// A tinted pill with a status dot.
pub fn chip(ui: &mut Ui, text: impl Into<String>, color: Color32) -> Response {
    let p = theme::of(ui);
    egui::Frame::new()
        .fill(theme::mix(p.bg, color, 0.16))
        .stroke(Stroke::new(1.0, theme::mix(p.bg, color, 0.45)))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(7.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 3.5, color);
                ui.label(RichText::new(text.into()).color(theme::mix(color, p.text, 0.35)).strong().size(12.5));
            });
        })
        .response
}

/// A titled group with a border. The usual container for a block of related controls.
pub fn card<R>(ui: &mut Ui, title: &str, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    let p = theme::of(ui);
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(8)
        .inner_margin(10)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !title.is_empty() {
                heading(ui, title);
                ui.add_space(2.0);
            }
            add_contents(ui)
        })
        .inner
}

/// Small capitals heading for a card or section.
pub fn heading(ui: &mut Ui, title: &str) {
    let p = theme::of(ui);
    ui.label(RichText::new(title.to_uppercase()).size(11.0).strong().color(p.dim).extra_letter_spacing(0.6));
}

/// Secondary explanatory text.
pub fn hint(ui: &mut Ui, text: impl Into<String>) {
    let p = theme::of(ui);
    ui.label(RichText::new(text.into()).color(p.dim).size(12.5));
}

/// A value with its label, for dashboards.
pub fn stat(ui: &mut Ui, label: &str, value: impl Into<String>, unit: &str) {
    let p = theme::of(ui);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.label(RichText::new(label).size(11.0).color(p.dim));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.label(RichText::new(value.into()).size(19.0).strong().color(p.text));
            if !unit.is_empty() {
                ui.label(RichText::new(unit).size(12.0).color(p.dim));
            }
        });
    });
}

/// A row of equally wide stats.
pub fn stats(ui: &mut Ui, items: &[(&str, String, &str)]) {
    ui.columns(items.len(), |columns| {
        for (column, (label, value, unit)) in columns.iter_mut().zip(items) {
            stat(column, label, value.clone(), unit);
        }
    });
}

/// Centred message for a panel that has nothing to show yet.
pub fn empty_state(ui: &mut Ui, title: &str, detail: &str) {
    let p = theme::of(ui);
    ui.add_space(18.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(title).size(14.5).color(p.dim).strong());
        ui.label(RichText::new(detail).size(12.5).color(p.dim));
    });
}

/// Button for the main action of a group.
pub fn primary_button(ui: &mut Ui, enabled: bool, text: &str) -> Response {
    let p = theme::of(ui);
    let label = RichText::new(text).strong().color(if enabled { p.on_accent } else { p.dim });
    let button = egui::Button::new(label).fill(if enabled { p.accent } else { p.raised });
    ui.add_enabled(enabled, button)
}

/// One option of a row of mutually exclusive choices.
pub fn segment<T: PartialEq>(ui: &mut Ui, current: &mut T, value: T, text: &str) -> Response {
    let selected = *current == value;
    let response = ui.selectable_label(selected, text);
    if response.clicked() {
        *current = value;
    }
    response
}

/// A horizontal bar showing `fraction` of full, with text on top.
pub fn meter(ui: &mut Ui, fraction: f32, color: Color32, text: impl Into<String>, height: f32) -> Response {
    let p = theme::of(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 4, p.raised);
    let mut fill = rect;
    fill.set_width(rect.width() * fraction.clamp(0.0, 1.0));
    if fill.width() > 0.5 {
        painter.rect_filled(fill, 4, color);
    }
    let text: String = text.into();
    if !text.is_empty() {
        painter.text(
            rect.left_center() + Vec2::new(8.0, 0.0),
            egui::Align2::LEFT_CENTER,
            text,
            egui::FontId::proportional(12.0),
            p.text,
        );
    }
    response
}

/// Battery outline filled to the state of charge.
pub fn battery_icon(ui: &mut Ui, soc: f32, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(24.0, 12.0), Sense::hover());
    let painter = ui.painter();
    let body = Rect::from_min_max(rect.min, rect.max - Vec2::new(3.0, 0.0));
    painter.rect_stroke(body, 3, Stroke::new(1.2, p.dim), StrokeKind::Inside);
    painter.rect_filled(
        Rect::from_center_size(rect.right_center() - Vec2::new(1.5, 0.0), Vec2::new(2.0, 5.0)),
        1,
        p.dim,
    );
    let color = if soc < 0.15 {
        p.bad
    } else if soc < 0.3 {
        p.warn
    } else {
        p.ok
    };
    let mut fill = body.shrink(2.5);
    fill.set_width(fill.width() * soc.clamp(0.0, 1.0));
    painter.rect_filled(fill, 1, color);
}

/// Spring-centred joystick. Returns (x, y) in -1..1 with y up; zero when released.
pub fn joystick(ui: &mut Ui, size: f32) -> ((f32, f32), Response) {
    let p = theme::of(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::drag());
    let painter = ui.painter_at(rect);
    let radius = size / 2.0 - 3.0;
    let knob_radius = size * 0.17;
    let travel = radius - knob_radius - 3.0;

    let mut offset = Vec2::ZERO;
    if response.dragged() {
        if let Some(pointer) = response.interact_pointer_pos() {
            offset = pointer - rect.center();
            if offset.length() > travel {
                offset = offset.normalized() * travel;
            }
        }
    }

    let enabled = ui.is_enabled();
    let fade = if enabled { 1.0 } else { 0.45 };
    painter.circle(rect.center(), radius, p.raised.gamma_multiply(fade), Stroke::new(1.0, p.border));
    painter.circle_stroke(rect.center(), radius * 0.5, Stroke::new(1.0, p.border.gamma_multiply(fade)));
    for dir in [Vec2::X, Vec2::Y] {
        painter.line_segment(
            [rect.center() - dir * (radius - 6.0), rect.center() + dir * (radius - 6.0)],
            Stroke::new(1.0, p.border.gamma_multiply(fade)),
        );
    }
    let knob = if response.dragged() { p.accent } else { theme::mix(p.raised, p.text, 0.55) };
    painter.circle_filled(rect.center() + offset + Vec2::new(0.0, 1.5), knob_radius, Color32::from_black_alpha(50));
    painter.circle(rect.center() + offset, knob_radius, knob.gamma_multiply(fade), Stroke::new(1.0, p.border));

    ((offset.x / travel, -offset.y / travel), response)
}

/// Minimal line chart of recent values, scaled to `max`.
pub fn sparkline(ui: &mut Ui, values: impl ExactSizeIterator<Item = f32>, max: f32, size: Vec2, color: Color32) {
    let p = theme::of(ui);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3, p.raised);
    let n = values.len();
    if n < 2 || max <= 0.0 {
        return;
    }
    let points: Vec<Pos2> = values
        .enumerate()
        .map(|(i, v)| {
            let x = rect.left() + rect.width() * i as f32 / (n - 1) as f32;
            let y = rect.bottom() - 2.0 - (rect.height() - 4.0) * (v / max).clamp(0.0, 1.0);
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

/// Text with a dark outline, readable over imagery and video in either theme.
pub fn outlined_text(
    painter: &egui::Painter,
    pos: Pos2,
    anchor: egui::Align2,
    text: &str,
    font: egui::FontId,
    color: Color32,
) {
    for offset in [Vec2::new(1.0, 1.0), Vec2::new(-1.0, 1.0), Vec2::new(1.0, -1.0), Vec2::new(-1.0, -1.0)] {
        painter.text(pos + offset, anchor, text, font.clone(), Color32::from_black_alpha(170));
    }
    painter.text(pos, anchor, text, font, color);
}
