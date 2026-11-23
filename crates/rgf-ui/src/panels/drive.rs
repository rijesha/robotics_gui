//! Movement control: mode selection and manual driving.

use crate::widgets::{joystick, stat, tx_status};
use crate::{theme, Panel, Station};
use egui::Key;
use rgf_core::client::label;
use rgf_core::msgs::common::Mode;

pub struct DrivePanel;

pub const MODES: [(Mode, &str); 5] = [
    (Mode::DISARMED, "Disarmed"),
    (Mode::MANUAL, "Manual"),
    (Mode::MOTOR_TEST, "Motor test"),
    (Mode::HOLD, "Hold"),
    (Mode::AUTO, "Auto"),
];

pub fn mode_name(mode: Mode) -> &'static str {
    MODES.iter().find(|(m, _)| *m == mode).map_or("?", |(_, name)| name)
}

/// Throttle and steer from the keyboard, unless the operator is typing into a field.
fn keyboard_input(ui: &egui::Ui) -> (f32, f32) {
    if ui.ctx().egui_wants_keyboard_input() {
        return (0.0, 0.0);
    }
    ui.input(|i| {
        let axis = |plus: [Key; 2], minus: [Key; 2]| {
            let down = |keys: [Key; 2]| keys.iter().any(|k| i.key_down(*k)) as i32 as f32;
            down(plus) - down(minus)
        };
        (
            axis([Key::W, Key::ArrowUp], [Key::S, Key::ArrowDown]),
            axis([Key::D, Key::ArrowRight], [Key::A, Key::ArrowLeft]),
        )
    })
}

/// Joystick plus keyboard; writes the result to the station's drive input.
pub fn drive_controls(ui: &mut egui::Ui, station: &mut Station, size: f32) {
    let (stick_x, stick_y) = joystick(ui, size);
    let (key_throttle, key_steer) = keyboard_input(ui);
    let from_stick = stick_x != 0.0 || stick_y != 0.0;
    station.drive_input = if from_stick { (stick_y, stick_x) } else { (key_throttle, key_steer) };
}

impl Panel for DrivePanel {
    fn title(&self) -> &str {
        "Drive"
    }

    fn ui(&mut self, ui: &mut egui::Ui, station: &mut Station) {
        let current = station.client.mode();
        ui.horizontal_wrapped(|ui| {
            for (mode, name) in MODES {
                if ui.selectable_label(current == Some(mode), name).clicked() {
                    station.client.set_mode(mode, station.now);
                }
            }
        });
        ui.horizontal(|ui| tx_status(ui, station, label::MODE));
        ui.separator();

        let manual = current == Some(Mode::MANUAL);
        ui.horizontal(|ui| {
            ui.add_enabled_ui(manual, |ui| drive_controls(ui, station, 150.0));
            ui.vertical(|ui| {
                let (throttle, steer) = station.drive_input;
                ui.label(format!("Throttle {:+.0}%", throttle * 100.0));
                ui.label(format!("Steer {:+.0}%", steer * 100.0));
                ui.add_space(6.0);
                if manual {
                    ui.colored_label(theme::DIM, "Drag the stick, or use W A S D / arrow keys.");
                } else {
                    ui.colored_label(theme::WARN, "Switch to Manual to drive.");
                }
            });
        });
        if !manual {
            station.drive_input = (0.0, 0.0);
        }
        ui.separator();

        if let Some(pose) = &station.client.pose {
            ui.horizontal_wrapped(|ui| {
                stat(ui, "Speed", format!("{:.2} m/s", pose.speed_mps));
                stat(ui, "Heading", format!("{:03.0}°", pose.heading_rad.to_degrees().rem_euclid(360.0)));
                stat(ui, "Turn rate", format!("{:+.0}°/s", pose.yaw_rate_rps.to_degrees()));
                stat(ui, "Steer / rudder", format!("{:+.0}°", pose.steer_rad.to_degrees()));
            });
        } else {
            ui.colored_label(theme::DIM, "No telemetry yet.");
        }
    }
}
