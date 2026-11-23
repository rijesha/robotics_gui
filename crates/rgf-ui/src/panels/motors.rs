//! Per-motor manual control and status. The rows are built from what the vehicle says
//! it has, so the same panel serves every vehicle.

use crate::widgets::{chip, sparkline, tx_status};
use crate::{theme, Panel, Station};
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};
use rgf_core::client::label;
use rgf_core::codec::str_from;
use rgf_core::msgs::common::{ActuatorKind, Mode};

pub struct MotorsPanel;

const FAULT_NAMES: [(u8, &str); 2] = [(1, "Over temperature"), (2, "Over current")];

fn is_angle(kind: ActuatorKind) -> bool {
    matches!(kind, ActuatorKind::STEER | ActuatorKind::RUDDER)
}

/// Top-down sketch of the vehicle with each actuator where it is mounted, coloured by load.
fn diagram(ui: &mut egui::Ui, station: &Station, peak_a: f32) {
    let Some(info) = &station.client.info else { return };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().min(260.0), 150.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let scale = (rect.height() - 30.0) / info.length_m.max(0.5);
    // Nose up: forward is -y on screen, right is +x.
    let at = |forward: f32, right: f32| rect.center() + Vec2::new(right, -forward) * scale;
    let body = Rect::from_center_size(rect.center(), Vec2::new(info.width_m * 0.7, info.length_m) * scale);
    painter.rect_stroke(body, 4, Stroke::new(1.5, theme::DIM), egui::StrokeKind::Inside);
    painter.line_segment(
        [at(info.length_m / 2.0, 0.0), at(info.length_m / 2.0 + 0.15 * info.length_m, 0.0)],
        Stroke::new(2.0, theme::DIM),
    );

    for (i, actuator) in station.client.actuators.iter().enumerate() {
        let (Some(actuator), Some(Some(status))) = (actuator, station.client.motors.get(i)) else { continue };
        let center = at(actuator.x_m, actuator.y_m);
        let load = theme::ramp(status.current_a / peak_a.max(0.1));
        if is_angle(actuator.kind) {
            // A needle showing the steering or rudder angle.
            let (sin, cos) = status.position_rad.sin_cos();
            let dir = Vec2::new(sin, -cos) * 14.0;
            painter.line_segment([center - dir, center + dir], Stroke::new(3.0, theme::PATH));
        } else {
            let size =
                if actuator.kind == ActuatorKind::THRUSTER { Vec2::new(10.0, 10.0) } else { Vec2::new(9.0, 20.0) };
            painter.rect_filled(Rect::from_center_size(center, size), 3, load);
            // Bar beside the wheel showing commanded direction and amount.
            let bar = Vec2::new(0.0, -status.command * 12.0);
            let side = if actuator.y_m < 0.0 { -9.0 } else { 9.0 };
            let base = Pos2::new(center.x + side, center.y);
            painter.line_segment([base, base + bar], Stroke::new(2.0, Color32::WHITE));
        }
    }
}

impl Panel for MotorsPanel {
    fn title(&self) -> &str {
        "Motors"
    }

    fn ui(&mut self, ui: &mut egui::Ui, station: &mut Station) {
        if station.client.actuators.is_empty() {
            ui.colored_label(theme::DIM, "Waiting for the vehicle to describe its motors.");
            return;
        }
        let testing = station.client.mode() == Some(Mode::MOTOR_TEST);
        ui.horizontal(|ui| {
            if testing {
                chip(ui, "MOTOR TEST", theme::WARN);
                if ui.button("Zero all").clicked() {
                    station.motor_test.iter_mut().for_each(|v| *v = 0.0);
                }
                if ui.button("Leave test").clicked() {
                    station.client.set_mode(Mode::HOLD, station.now);
                }
            } else if ui.button("Enter motor test").on_hover_text("Drive each motor by hand").clicked() {
                station.client.set_mode(Mode::MOTOR_TEST, station.now);
            }
            tx_status(ui, station, label::MODE);
        });

        let peak_a = station
            .client
            .motors
            .iter()
            .flatten()
            .map(|m| m.current_a)
            .fold(1.0f32, f32::max)
            .max(station.motor_history.iter().flat_map(|h| h.current_a.iter().copied()).fold(1.0, f32::max));
        diagram(ui, station, peak_a);
        ui.separator();

        for i in 0..station.client.actuators.len() {
            let Some(actuator) = station.client.actuators[i].clone() else { continue };
            let status = station.client.motors.get(i).cloned().flatten();
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(96.0);
                    ui.strong(str_from(&actuator.name));
                    if let Some(status) = &status {
                        if is_angle(actuator.kind) {
                            ui.label(format!("{:+.1}°", status.position_rad.to_degrees()));
                        } else {
                            ui.label(format!("{:.0} rpm", status.rpm));
                        }
                    }
                });
                ui.vertical(|ui| {
                    if let Some(status) = &status {
                        ui.horizontal(|ui| {
                            ui.label(format!("{:5.1} A", status.current_a));
                            ui.label(format!("{:4.1} V", status.voltage_v));
                            let hot = status.temp_c > 70.0;
                            ui.colored_label(
                                if hot { theme::BAD } else { ui.visuals().text_color() },
                                format!("{:3.0} °C", status.temp_c),
                            );
                            for (bit, name) in FAULT_NAMES {
                                if status.faults & bit != 0 {
                                    chip(ui, name, theme::BAD);
                                }
                            }
                        });
                    } else {
                        ui.colored_label(theme::DIM, "no status");
                    }
                    ui.horizontal(|ui| {
                        if let Some(history) = station.motor_history.get(i) {
                            sparkline(
                                ui,
                                history.current_a.iter().copied(),
                                peak_a,
                                Vec2::new(90.0, 18.0),
                                theme::ACCENT,
                            );
                        }
                        ui.spacing_mut().slider_width = 70.0;
                        if let Some(value) = station.motor_test.get_mut(i) {
                            ui.add_enabled(
                                testing,
                                egui::Slider::new(value, -1.0..=1.0).show_value(true).fixed_decimals(2),
                            );
                        }
                    });
                });
            });
            ui.add_space(2.0);
        }
    }
}
