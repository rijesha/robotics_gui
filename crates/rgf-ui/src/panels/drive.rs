//! Movement control: mode selection and manual driving.

use crate::widgets::{card, hint, joystick, stats, tx_status, tx_visible};
use crate::{theme, Panel, Station};
use egui::Key;
use rgf_core::client::label;
use rgf_core::msgs::common::Mode;

pub struct DrivePanel;

pub const MODES: [(Mode, &str, &str); 5] = [
    (Mode::DISARMED, "Disarmed", "Motors off"),
    (Mode::MANUAL, "Manual", "Drive with the stick or keyboard"),
    (Mode::MOTOR_TEST, "Motor test", "Run each motor by hand from the Motors panel"),
    (Mode::HOLD, "Hold", "Stay put, ready to resume"),
    (Mode::AUTO, "Auto", "Follow the uploaded mission"),
];

pub fn mode_name(mode: Mode) -> &'static str {
    MODES.iter().find(|(m, ..)| *m == mode).map_or("?", |(_, name, _)| name)
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
    let ((stick_x, stick_y), response) = joystick(ui, size);
    station.anchor("drive.stick", &response);
    let (key_throttle, key_steer) = keyboard_input(ui);
    let from_stick = stick_x != 0.0 || stick_y != 0.0;
    station.drive_input = if from_stick { (stick_y, stick_x) } else { (key_throttle, key_steer) };
}

/// Bar centred on zero for a -1..1 value.
fn signed_bar(ui: &mut egui::Ui, label_text: &str, value: f32) {
    let p = theme::of(ui);
    ui.horizontal(|ui| {
        ui.add_sized([52.0, 16.0], egui::Label::new(egui::RichText::new(label_text).color(p.dim).size(12.0)));
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2((ui.available_width() - 44.0).max(30.0), 8.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 4, p.raised);
        let mid = rect.center().x;
        let end = mid + rect.width() / 2.0 * value.clamp(-1.0, 1.0);
        let bar = egui::Rect::from_x_y_ranges(mid.min(end)..=mid.max(end), rect.y_range());
        ui.painter().rect_filled(bar, 4, p.accent);
        ui.label(egui::RichText::new(format!("{:+.0}%", value * 100.0)).monospace().size(11.5));
    });
}

impl Panel for DrivePanel {
    fn title(&self) -> &str {
        "Drive"
    }

    fn ui(&mut self, ui: &mut egui::Ui, station: &mut Station) {
        let current = station.client.mode();
        card(ui, "Mode", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (mode, name, about) in MODES {
                    let response = ui.selectable_label(current == Some(mode), name).on_hover_text(about);
                    station.anchor(format!("mode.{name}"), &response);
                    if response.clicked() {
                        station.client.set_mode(mode, station.now);
                    }
                }
            });
            if tx_visible(station, label::MODE) {
                ui.horizontal(|ui| tx_status(ui, station, label::MODE));
            }
        });
        ui.add_space(4.0);

        let manual = current == Some(Mode::MANUAL);
        card(ui, "Manual control", |ui| {
            ui.horizontal(|ui| {
                ui.add_enabled_ui(manual, |ui| drive_controls(ui, station, 112.0));
                ui.vertical(|ui| {
                    let (throttle, steer) = station.drive_input;
                    signed_bar(ui, "Throttle", throttle);
                    signed_bar(ui, "Steer", steer);
                    ui.add_space(4.0);
                    if manual {
                        hint(ui, "Drag the stick, or use W A S D or the arrow keys.");
                    } else {
                        hint(ui, "Select Manual mode to drive.");
                    }
                });
            });
        });
        if !manual {
            station.drive_input = (0.0, 0.0);
        }
        ui.add_space(4.0);

        card(ui, "Motion", |ui| match &station.client.pose {
            Some(pose) => {
                stats(
                    ui,
                    &[
                        ("Speed", format!("{:.2}", pose.speed_mps), "m/s"),
                        ("Heading", format!("{:03.0}°", pose.heading_rad.to_degrees().rem_euclid(360.0)), ""),
                    ],
                );
                ui.add_space(4.0);
                stats(
                    ui,
                    &[
                        ("Turn rate", format!("{:+.0}", pose.yaw_rate_rps.to_degrees()), "°/s"),
                        ("Steering", format!("{:+.0}°", pose.steer_rad.to_degrees()), ""),
                    ],
                );
            }
            None => hint(ui, "No telemetry yet."),
        });
    }
}
