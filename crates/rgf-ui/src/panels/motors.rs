//! Per-motor manual control and status: current, torque, speed and temperature for each
//! one. The rows are built from what the vehicle says it has, so the same panel serves
//! every vehicle.

use crate::station::{MotorHistory, Vehicle};
use crate::theme::Palette;
use crate::widgets::{card, chip, empty_state, hint, primary_button, trend, tx_status};
use crate::{theme, Panel, Station};
use egui::{Pos2, Rect, RichText, Sense, Stroke, Vec2};
use rgf_core::client::label;
use rgf_core::codec::str_from;
use rgf_core::msgs::common::{ActuatorInfo, ActuatorKind, Mode};
use rgf_core::msgs::telemetry::MotorStatus;
use std::collections::VecDeque;

pub struct MotorsPanel;

const FAULT_OVERCURRENT: u8 = 2;
const FAULT_NAMES: [(u8, &str); 2] = [(1, "Hot"), (FAULT_OVERCURRENT, "Overcurrent")];

/// Motors run warm above this and trip their over-temperature fault at [`TEMP_BAD_C`].
const TEMP_WARN_C: f32 = 70.0;
const TEMP_BAD_C: f32 = 80.0;
/// Below this width the four readings of a motor are stacked two by two.
const WIDE_ROW: f32 = 330.0;

fn is_angle(kind: ActuatorKind) -> bool {
    matches!(kind, ActuatorKind::STEER | ActuatorKind::RUDDER)
}

fn temp_color(temp_c: f32, p: &Palette) -> egui::Color32 {
    if temp_c > TEMP_BAD_C {
        p.bad
    } else if temp_c > TEMP_WARN_C {
        p.warn
    } else {
        p.text
    }
}

fn current_color(status: &MotorStatus, p: &Palette) -> egui::Color32 {
    if status.faults & FAULT_OVERCURRENT != 0 {
        p.bad
    } else {
        p.text
    }
}

/// Largest magnitudes across all motors, now and in the kept history, so that every
/// motor's readings are drawn to the same scale and can be compared.
#[derive(Clone, Copy)]
struct Peaks {
    current_a: f32,
    torque_nm: f32,
    rpm: f32,
    angle_rad: f32,
}

impl Peaks {
    fn of(vehicle: &Vehicle) -> Self {
        let peak = |series: fn(&MotorHistory) -> &VecDeque<f32>, floor: f32| {
            vehicle.motor_history.iter().flat_map(|h| series(h).iter()).fold(floor, |max, v| max.max(v.abs()))
        };
        Self {
            current_a: peak(|h| &h.current_a, 1.0),
            torque_nm: peak(|h| &h.torque_nm, 0.5),
            rpm: peak(|h| &h.rpm, 10.0),
            angle_rad: peak(|h| &h.position_rad, 0.2),
        }
    }
}

/// One reading of a motor: label, value with unit, and its recent history.
fn reading(
    ui: &mut egui::Ui,
    name: &str,
    value: String,
    unit: &str,
    color: egui::Color32,
    history: &VecDeque<f32>,
    range: std::ops::RangeInclusive<f32>,
) {
    let p = theme::of(ui);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.label(RichText::new(name).size(10.5).color(p.dim));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.label(RichText::new(value).monospace().size(13.5).strong().color(color));
            ui.label(RichText::new(unit).size(10.5).color(p.dim));
        });
        let line = if color == p.text { p.accent } else { color };
        trend(ui, history.iter().copied(), range, Vec2::new(ui.available_width(), 16.0), line);
    });
}

/// Current, torque, speed and temperature of one motor, each with its last 30 s.
fn motor_block(
    ui: &mut egui::Ui,
    actuator: &ActuatorInfo,
    status: Option<&MotorStatus>,
    history: &MotorHistory,
    peaks: Peaks,
) {
    let p = theme::of(ui);
    ui.horizontal(|ui| {
        ui.label(RichText::new(str_from(&actuator.name)).strong());
        let Some(status) = status else {
            ui.label(RichText::new("no data yet").color(p.dim).size(12.0));
            return;
        };
        for (bit, name) in FAULT_NAMES {
            if status.faults & bit != 0 {
                chip(ui, name, p.bad);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(format!("command {:+.0}%", status.command * 100.0)).size(11.0).color(p.dim));
        });
    });
    let Some(status) = status else { return };

    let cells: [&dyn Fn(&mut egui::Ui); 4] = [
        &|ui| {
            let color = current_color(status, &p);
            reading(
                ui,
                "Current",
                format!("{:.1}", status.current_a),
                "A",
                color,
                &history.current_a,
                0.0..=peaks.current_a,
            )
        },
        &|ui| {
            let range = -peaks.torque_nm..=peaks.torque_nm;
            reading(ui, "Torque", format!("{:+.1}", status.torque_nm), "N·m", p.text, &history.torque_nm, range)
        },
        &|ui| {
            if is_angle(actuator.kind) {
                let range = -peaks.angle_rad..=peaks.angle_rad;
                let value = format!("{:+.1}", status.position_rad.to_degrees());
                reading(ui, "Angle", value, "°", p.text, &history.position_rad, range)
            } else {
                let range = -peaks.rpm..=peaks.rpm;
                reading(ui, "Speed", format!("{:.0}", status.rpm), "rpm", p.text, &history.rpm, range)
            }
        },
        &|ui| {
            let color = temp_color(status.temp_c, &p);
            reading(ui, "Temp", format!("{:.0}", status.temp_c), "°C", color, &history.temp_c, 20.0..=90.0)
        },
    ];
    let per_row = if ui.available_width() >= WIDE_ROW { 4 } else { 2 };
    for row in cells.chunks(per_row) {
        ui.columns(per_row, |columns| {
            for (column, cell) in columns.iter_mut().zip(row) {
                cell(column);
            }
        });
    }
}

/// Every motor's current, torque, speed and temperature as a small table, for showing
/// over the map.
pub fn strip(ui: &mut egui::Ui, vehicle: &Vehicle) {
    let p = theme::of(ui);
    let peak_a = Peaks::of(vehicle).current_a;
    // Monospace and padded to one width, so the columns line up without measuring.
    let cell = |ui: &mut egui::Ui, text: String, color: egui::Color32| {
        ui.label(RichText::new(format!("{text:>6}")).monospace().size(12.0).color(color));
    };
    egui::Grid::new("motor_strip").num_columns(5).spacing([8.0, 3.0]).show(ui, |ui| {
        ui.label(RichText::new("MOTOR").size(10.5).color(p.dim));
        for unit in ["A", "N·m", "rpm", "°C"] {
            cell(ui, unit.to_owned(), p.dim);
        }
        ui.end_row();
        for (actuator, status) in vehicle.client.actuators.iter().zip(&vehicle.client.motors) {
            let Some(actuator) = actuator else { continue };
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                // Dot coloured by load, matching the layout sketch in the Motors panel.
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                let load = status.as_ref().map_or(p.border, |s| theme::ramp(s.current_a / peak_a));
                ui.painter().circle_filled(dot.center(), 4.0, load);
                ui.label(RichText::new(str_from(&actuator.name)).size(12.0));
            });
            match status {
                Some(status) => {
                    cell(ui, format!("{:.1}", status.current_a), current_color(status, &p));
                    cell(ui, format!("{:+.1}", status.torque_nm), p.text);
                    if is_angle(actuator.kind) {
                        cell(ui, format!("{:+.0}°", status.position_rad.to_degrees()), p.dim);
                    } else {
                        cell(ui, format!("{:.0}", status.rpm), p.text);
                    }
                    cell(ui, format!("{:.0}", status.temp_c), temp_color(status.temp_c, &p));
                }
                None => (0..4).for_each(|_| cell(ui, "–".to_owned(), p.dim)),
            }
            ui.end_row();
        }
    });
}

/// Top-down sketch of the vehicle with each actuator where it is mounted, coloured by load.
fn diagram(ui: &mut egui::Ui, station: &Station, peak_a: f32) {
    let Some(info) = &station.client.info else { return };
    let p = theme::of(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 132.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let scale = ((rect.height() - 34.0) / info.length_m.max(0.5)).min((rect.width() - 60.0) / info.width_m.max(0.5));
    // Nose up: forward is -y on screen, right is +x.
    let at = |forward: f32, right: f32| rect.center() + Vec2::new(right, -forward) * scale;
    let body = Rect::from_center_size(rect.center(), Vec2::new(info.width_m * 0.62, info.length_m) * scale);
    painter.rect(body, 6, p.raised, Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
    // Nose marker.
    let nose = at(info.length_m / 2.0, 0.0);
    painter.add(egui::Shape::convex_polygon(
        vec![nose + Vec2::new(0.0, -9.0), nose + Vec2::new(6.0, -1.0), nose + Vec2::new(-6.0, -1.0)],
        p.dim,
        Stroke::NONE,
    ));

    for (i, actuator) in station.client.actuators.iter().enumerate() {
        let (Some(actuator), Some(Some(status))) = (actuator, station.client.motors.get(i)) else { continue };
        let center = at(actuator.x_m, actuator.y_m);
        if is_angle(actuator.kind) {
            // A needle showing the steering or rudder angle.
            let (sin, cos) = status.position_rad.sin_cos();
            let dir = Vec2::new(sin, -cos) * 13.0;
            painter.line_segment([center - dir, center + dir], Stroke::new(3.0, p.accent));
        } else {
            let load = theme::ramp(status.current_a / peak_a.max(0.1));
            let size = if actuator.kind == ActuatorKind::THRUSTER { Vec2::splat(12.0) } else { Vec2::new(9.0, 20.0) };
            painter.rect(
                Rect::from_center_size(center, size),
                3,
                load,
                Stroke::new(1.0, p.border),
                egui::StrokeKind::Outside,
            );
            // Bar beside the wheel showing commanded direction and amount.
            let side = if actuator.y_m < 0.0 { -10.0 } else { 10.0 };
            let base = Pos2::new(center.x + side, center.y);
            painter.line_segment([base, base + Vec2::new(0.0, -status.command * 12.0)], Stroke::new(2.5, p.text));
        }
    }
    painter.text(
        rect.right_bottom() - Vec2::new(2.0, 0.0),
        egui::Align2::RIGHT_BOTTOM,
        "colour: current   bar: command",
        egui::FontId::proportional(10.5),
        p.dim,
    );
}

impl Panel for MotorsPanel {
    fn title(&self) -> &str {
        "Motors"
    }

    fn ui(&mut self, ui: &mut egui::Ui, station: &mut Station) {
        let p = theme::of(ui);
        if station.client.actuators.is_empty() {
            empty_state(ui, "No motors yet", "Waiting for the vehicle to describe its actuators.");
            return;
        }
        let testing = station.client.mode() == Some(Mode::MOTOR_TEST);
        card(ui, "Manual motor test", |ui| {
            ui.horizontal(|ui| {
                if testing {
                    let response = primary_button(ui, true, "End test");
                    station.anchor("motors.test", &response);
                    if response.clicked() {
                        let now = station.now;
                        station.client.set_mode(Mode::HOLD, now);
                    }
                    if ui.button("Zero all").clicked() {
                        station.motor_test.iter_mut().for_each(|v| *v = 0.0);
                    }
                    chip(ui, "Test active", p.warn);
                } else {
                    let response =
                        ui.button("Start test").on_hover_text("Switch to Motor test mode and run motors one at a time");
                    station.anchor("motors.test", &response);
                    if response.clicked() {
                        let now = station.now;
                        station.client.set_mode(Mode::MOTOR_TEST, now);
                    }
                }
                tx_status(ui, station, label::MODE);
            });
            if testing {
                // One slider per actuator, -100% to +100% of its range.
                egui::Grid::new("motor_test").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.spacing_mut().slider_width = (ui.available_width() - 150.0).clamp(80.0, 200.0);
                    for i in 0..station.client.actuators.len() {
                        let Some(actuator) = station.client.actuators[i].clone() else { continue };
                        ui.label(str_from(&actuator.name));
                        if let Some(value) = station.motor_test.get_mut(i) {
                            let response = ui.add(egui::Slider::new(value, -1.0..=1.0).fixed_decimals(2));
                            station.anchor(format!("motors.slider.{i}"), &response);
                        }
                        ui.end_row();
                    }
                });
            } else {
                hint(ui, "Start a test to move each motor with its own slider.");
            }
        });
        ui.add_space(4.0);

        let peaks = Peaks::of(station);
        card(ui, "Layout", |ui| diagram(ui, station, peaks.current_a));
        ui.add_space(4.0);

        card(ui, "Status", |ui| {
            let vehicle: &Vehicle = station;
            let mut first = true;
            for (i, actuator) in vehicle.client.actuators.iter().enumerate() {
                let (Some(actuator), Some(history)) = (actuator, vehicle.motor_history.get(i)) else { continue };
                if !std::mem::take(&mut first) {
                    ui.separator();
                }
                let status = vehicle.client.motors.get(i).and_then(Option::as_ref);
                motor_block(ui, actuator, status, history, peaks);
            }
            ui.add_space(2.0);
            hint(ui, "Graphs show the last 30 s, to the same scale for every motor.");
        });
    }
}
