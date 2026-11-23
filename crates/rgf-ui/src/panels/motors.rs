//! Per-motor manual control and status. The rows are built from what the vehicle says
//! it has, so the same panel serves every vehicle.

use crate::widgets::{card, chip, empty_state, hint, primary_button, sparkline, tx_status};
use crate::{theme, Panel, Station};
use egui::{Pos2, Rect, RichText, Sense, Stroke, Vec2};
use rgf_core::client::label;
use rgf_core::codec::str_from;
use rgf_core::msgs::common::{ActuatorKind, Mode};

pub struct MotorsPanel;

const FAULT_NAMES: [(u8, &str); 2] = [(1, "Hot"), (2, "Overcurrent")];

fn is_angle(kind: ActuatorKind) -> bool {
    matches!(kind, ActuatorKind::STEER | ActuatorKind::RUDDER)
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
                        station.client.set_mode(Mode::HOLD, station.now);
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
                        station.client.set_mode(Mode::MOTOR_TEST, station.now);
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

        let peak_a = station
            .client
            .motors
            .iter()
            .flatten()
            .map(|m| m.current_a)
            .fold(1.0f32, f32::max)
            .max(station.motor_history.iter().flat_map(|h| h.current_a.iter().copied()).fold(1.0, f32::max));
        card(ui, "Layout", |ui| diagram(ui, station, peak_a));
        ui.add_space(4.0);

        card(ui, "Status", |ui| {
            egui::Grid::new("motor_rows").num_columns(5).spacing([10.0, 7.0]).show(ui, |ui| {
                for heading in ["Motor", "Output", "Current", "Temp", "Last 30 s"] {
                    ui.label(RichText::new(heading).size(11.0).color(p.dim));
                }
                ui.end_row();
                for i in 0..station.client.actuators.len() {
                    let Some(actuator) = station.client.actuators[i].clone() else { continue };
                    let status = station.client.motors.get(i).cloned().flatten();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(str_from(&actuator.name)).strong());
                        if let Some(status) = &status {
                            for (bit, name) in FAULT_NAMES {
                                if status.faults & bit != 0 {
                                    chip(ui, name, p.bad);
                                }
                            }
                        }
                    });
                    match &status {
                        Some(status) => {
                            if is_angle(actuator.kind) {
                                ui.monospace(format!("{:+5.1}°", status.position_rad.to_degrees()));
                            } else {
                                ui.monospace(format!("{:5.0} rpm", status.rpm));
                            }
                            ui.monospace(format!("{:5.1} A", status.current_a));
                            let color = if status.temp_c > 70.0 { p.bad } else { p.text };
                            ui.label(RichText::new(format!("{:3.0} °C", status.temp_c)).monospace().color(color));
                        }
                        None => {
                            for _ in 0..3 {
                                ui.label(RichText::new("–").color(p.dim));
                            }
                        }
                    }
                    if let Some(history) = station.motor_history.get(i) {
                        sparkline(ui, history.current_a.iter().copied(), peak_a, Vec2::new(84.0, 18.0), p.accent);
                    }
                    ui.end_row();
                }
            });
        });
    }
}
