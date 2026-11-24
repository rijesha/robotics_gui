//! What the map layout keeps in front of the map: the vehicles, the mission, the camera,
//! the motors and the drive controls, each as a small card pinned to a corner.
//!
//! The cards are drawn in the map's own layer, after it, so floating panels always open
//! above them.

use crate::panels::drive::{drive_controls, mode_name, mode_row};
use crate::panels::{map, mission, motors, video};
use crate::widgets::{heading, tx_status, tx_visible};
use crate::{design, fleet, theme, Station};
use egui::{Align2, Rect, Sense, Vec2};
use rgf_core::client::label;
use rgf_core::msgs::common::Mode;
use std::collections::HashMap;

const MARGIN: f32 = 8.0;
const GAP: f32 = 6.0;
/// Room left for the map's own toolbar along its top edge.
const BELOW_TOOLBAR: f32 = 52.0;
/// Room left for the map's height legend down its right edge.
const BELOW_LEGEND: f32 = 178.0;
/// Room left for the map's scale bar and imagery credit along its bottom edge.
const ABOVE_SCALE: f32 = 44.0;
const ABOVE_CREDIT: f32 = 28.0;

/// Which cards to leave out because the panel that says the same thing is open.
#[derive(Default, Clone, Copy)]
pub struct Hidden {
    pub vehicles: bool,
    pub mission: bool,
    pub video: bool,
    pub motors: bool,
    pub drive: bool,
}

#[derive(Default)]
pub struct Hud {
    /// Size each card came out at last frame; a card is pinned by a corner, so where it
    /// starts depends on how big it is.
    sizes: HashMap<&'static str, Vec2>,
}

impl Hud {
    /// Draw one card with `corner` of it at `at`. Returns the space it takes up.
    fn card(
        &mut self,
        ui: &mut egui::Ui,
        key: &'static str,
        corner: Align2,
        at: egui::Pos2,
        width: f32,
        add_contents: impl FnOnce(&mut egui::Ui),
    ) -> Rect {
        let known = self.sizes.get(key).copied();
        let rect = corner.anchor_size(at, known.unwrap_or(Vec2::ZERO));
        let mut builder = egui::UiBuilder::new()
            .id_salt(key)
            .max_rect(Rect::from_min_size(rect.min, Vec2::new(width, ui.max_rect().height())));
        match known {
            // Registered before the contents so they stay on top of it: keeps clicks and
            // drags on the card from reaching the map underneath.
            Some(_) => drop(ui.interact(rect, ui.scope_id().with(("hud", key)), Sense::click_and_drag())),
            // Not measured yet: lay it out unseen this once.
            None => builder = builder.invisible(),
        }
        let mut child = ui.new_child(builder);
        let p = theme::of(&child);
        let glass = design::current().glass;
        let fill = egui::Color32::from_rgba_unmultiplied(p.surface.r(), p.surface.g(), p.surface.b(), glass);
        let size =
            map::toolbar_frame(&child).fill(fill).inner_margin(8).show(&mut child, add_contents).response.rect.size();
        if known != Some(size) {
            self.sizes.insert(key, size);
            ui.ctx().request_repaint();
        }
        Rect::from_min_size(rect.min, size)
    }

    /// Draw every card over `area`, the rectangle the map fills.
    pub fn ui(&mut self, ui: &mut egui::Ui, area: Rect, station: &mut Station, hidden: Hidden) {
        let mut top_left = area.left_top() + Vec2::new(MARGIN, BELOW_TOOLBAR);
        if !hidden.vehicles {
            let vehicles = self.card(ui, "vehicles", Align2::LEFT_TOP, top_left, 230.0, |ui| {
                ui.set_width(214.0);
                heading(ui, "Vehicles");
                fleet::list(ui, station);
            });
            top_left = vehicles.left_bottom() + Vec2::new(0.0, GAP);
        }
        let has_mission = station.client.nav.as_ref().is_some_and(|n| n.wp_count > 0);
        if has_mission && !hidden.mission {
            let at = top_left;
            self.card(ui, "mission", Align2::LEFT_TOP, at, 230.0, |ui| {
                ui.set_width(214.0);
                heading(ui, "Mission");
                mission::progress(ui, station);
            });
        }

        if !hidden.video && station.video_texture.is_some() {
            let at = area.right_top() + Vec2::new(-MARGIN, BELOW_LEGEND);
            // Smaller beside a narrow map, so it does not sit on the vehicle.
            let width = (area.width() * 0.22).clamp(150.0, 236.0);
            self.card(ui, "video", Align2::RIGHT_TOP, at, width + 24.0, |ui| {
                video::picture(ui, station, width);
            });
        }

        if !hidden.motors && !station.client.actuators.is_empty() {
            let at = area.left_bottom() + Vec2::new(MARGIN, -ABOVE_SCALE);
            self.card(ui, "motors", Align2::LEFT_BOTTOM, at, 340.0, |ui| {
                heading(ui, "Motors");
                motors::strip(ui, station);
            });
        }

        if design::current().strip && station.client.heartbeat.is_some() {
            // Beside a narrow map the strip moves to the corner and keeps to the essentials,
            // leaving the other corner to the drive controls.
            let compact = area.width() < 1150.0;
            let (corner, at) = match compact {
                true => (Align2::LEFT_BOTTOM, area.left_bottom() + Vec2::new(MARGIN, -ABOVE_SCALE)),
                false => (Align2::CENTER_BOTTOM, area.center_bottom() + Vec2::new(0.0, -14.0)),
            };
            self.card(ui, "strip", corner, at, 760.0, |ui| strip(ui, station, compact));
        }

        if !hidden.drive {
            let at = area.right_bottom() + Vec2::new(-MARGIN, -ABOVE_CREDIT);
            let manual = station.client.mode() == Some(Mode::MANUAL);
            self.card(ui, "drive", Align2::RIGHT_BOTTOM, at, 400.0, |ui| {
                mode_row(ui, station);
                if tx_visible(station, label::MODE) {
                    ui.horizontal(|ui| tx_status(ui, station, label::MODE));
                }
                if manual {
                    ui.horizontal(|ui| {
                        drive_controls(ui, station, 112.0);
                        let (throttle, steer) = station.drive_input;
                        let text = format!("Throttle {:+4.0}%\nSteer    {:+4.0}%", throttle * 100.0, steer * 100.0);
                        ui.label(egui::RichText::new(text).monospace().size(12.0));
                    });
                }
            });
            if !manual {
                station.drive_input = (0.0, 0.0);
            }
        }
    }
}

/// One big number of the telemetry strip.
fn figure(ui: &mut egui::Ui, name: &str, value: String, unit: &str, color: egui::Color32) {
    let p = theme::of(ui);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        // A fixed width keeps the strip from shifting as digits change.
        ui.set_min_width(74.0);
        ui.label(egui::RichText::new(name.to_uppercase()).size(10.0).color(p.dim).extra_letter_spacing(0.6));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.label(egui::RichText::new(value).size(22.0).strong().color(color));
            ui.label(egui::RichText::new(unit).size(11.5).color(p.dim));
        });
    });
}

/// The selected vehicle at a glance: the few numbers an operator checks while driving.
fn strip(ui: &mut egui::Ui, station: &Station, compact: bool) {
    let p = theme::of(ui);
    let client = &station.client;
    let alive = client.vehicle_alive(station.now);
    let live = if alive { p.text } else { p.dim };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        let mode = client.mode().map_or("–", mode_name);
        let mode_color = match client.mode() {
            _ if !alive => p.bad,
            Some(Mode::AUTO) => p.ok,
            Some(Mode::DISARMED) | None => p.dim,
            Some(Mode::MOTOR_TEST) => p.warn,
            Some(_) => p.accent,
        };
        figure(ui, "Mode", if alive { mode.to_owned() } else { "Link lost".to_owned() }, "", mode_color);
        if let Some(pose) = &client.pose {
            ui.separator();
            figure(ui, "Speed", format!("{:.1}", pose.speed_mps), "m/s", live);
            let heading_deg = pose.heading_rad.to_degrees().rem_euclid(360.0);
            figure(ui, "Heading", format!("{heading_deg:03.0}"), "°", live);
            if !compact {
                figure(ui, "Position", format!("±{:.2}", pose.h_acc_m), "m", live);
            }
        }
        if let Some(power) = &client.power {
            ui.separator();
            let color = if power.soc_pct < 15.0 {
                p.bad
            } else if power.soc_pct < 30.0 {
                p.warn
            } else {
                live
            };
            figure(ui, "Battery", format!("{:.0}", power.soc_pct), "%", color);
            if !compact {
                figure(ui, "Power", format!("{:.0}", power.power_w), "W", live);
            }
        }
        let hottest = client.motors.iter().flatten().map(|m| m.temp_c).fold(f32::NAN, f32::max);
        if hottest.is_finite() && !compact {
            ui.separator();
            let color = if hottest > 80.0 {
                p.bad
            } else if hottest > 70.0 {
                p.warn
            } else {
                live
            };
            figure(ui, "Hottest motor", format!("{hottest:.0}"), "°C", color);
        }
    });
}
