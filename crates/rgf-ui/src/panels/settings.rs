//! Vehicle settings. A change counts only once the vehicle reads the same value back.

use crate::widgets::{card, empty_state, hint, primary_button, tx_status, tx_visible};
use crate::{theme, Panel, Station};
use egui::RichText;
use rgf_core::client::label;

pub struct SettingsPanel;

/// Friendly name, unit and explanation for the parameters this UI knows about. Anything
/// else the vehicle reports is shown under its raw name.
const KNOWN: [(&str, &str, &str, &str); 8] = [
    ("CRUISE_SPEED", "Cruise speed", " m/s", "Speed the vehicle holds while following a mission."),
    ("WP_RADIUS", "Waypoint radius", " m", "How close counts as having reached a waypoint."),
    ("STEER_GAIN", "Steering gain", "", "How sharply Auto steers back towards the route."),
    ("TELEM_RATE", "Telemetry rate", " Hz", "How often position and attitude are reported."),
    ("LIDAR_RATE", "Lidar rate", " Hz", "Lidar scans per second. 0 turns the lidar off."),
    ("VIDEO_RATE", "Video rate", " fps", "Camera frames per second. 0 turns video off."),
    ("VIDEO_QUALITY", "Video quality", " %", "JPEG quality of the video; lower uses less of the link."),
    ("MANUAL_LIMIT", "Manual throttle limit", "", "Scales the throttle in Manual mode, 0.1 to 1."),
];

impl Panel for SettingsPanel {
    fn title(&self) -> &str {
        "Settings"
    }

    fn ui(&mut self, ui: &mut egui::Ui, station: &mut Station) {
        let p = theme::of(ui);
        let now = station.now;
        ui.horizontal(|ui| {
            if ui.button("Reload").on_hover_text("Read every setting from the vehicle again").clicked() {
                station.client.request_params(now);
                station.param_edits.clear();
            }
            tx_status(ui, station, label::PARAM_LIST);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                hint(ui, format!("{} of {} loaded", station.client.params.len(), station.client.param_count));
            });
        });
        if station.client.params.is_empty() {
            empty_state(ui, "No settings yet", "They load automatically once a vehicle is connected.");
            return;
        }
        ui.add_space(2.0);

        let ids: Vec<u16> = station.client.params.keys().copied().collect();
        card(ui, "", |ui| {
            for (row, id) in ids.into_iter().enumerate() {
                if row > 0 {
                    ui.separator();
                }
                let entry = station.client.params[&id].clone();
                let known = KNOWN.iter().find(|(raw, ..)| *raw == entry.name);
                let (name, unit, about) = known.map_or((entry.name.as_str(), "", ""), |(_, n, u, a)| (*n, *u, *a));

                // An edit stays until the vehicle holds that value, so a send that failed
                // can be retried without retyping.
                if station.param_edits.get(&id).is_some_and(|e| (e - entry.value).abs() <= 1e-6) {
                    station.param_edits.remove(&id);
                }
                let mut edit = station.param_edits.get(&id).copied().unwrap_or(entry.value);
                let changed = (edit - entry.value).abs() > 1e-6;

                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.label(name).on_hover_text(format!(
                            "{about}\n\nAllowed: {} to {}{unit}\nParameter: {}",
                            entry.min, entry.max, entry.name
                        ));
                        let now_text = format!("on vehicle: {:.2}{unit}", entry.value);
                        ui.label(RichText::new(now_text).size(11.5).color(p.dim));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let response = primary_button(ui, changed, "Apply");
                        station.anchor(format!("settings.apply.{id}"), &response);
                        if response.clicked() {
                            station.client.set_param(id, edit, now);
                        }
                        let step = ((entry.max - entry.min) / 200.0).max(0.001);
                        // Deliberately not clamped: the vehicle is the authority on what it
                        // accepts, and the readback shows what it actually applied.
                        let response = ui.add(egui::DragValue::new(&mut edit).speed(step).max_decimals(2).suffix(unit));
                        station.anchor(format!("settings.value.{id}"), &response);
                        if response.changed() {
                            station.param_edits.insert(id, edit);
                        }
                    });
                });
                let tx_label = label::param(id);
                if tx_visible(station, &tx_label) {
                    ui.horizontal(|ui| tx_status(ui, station, &tx_label));
                }
            }
        });
        hint(ui, "A change is confirmed only when the vehicle reads the same value back.");
    }
}
