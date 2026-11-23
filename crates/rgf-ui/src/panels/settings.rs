//! Vehicle settings. A change counts only once the vehicle reads the same value back.

use crate::widgets::tx_status;
use crate::{theme, Panel, Station};
use rgf_core::client::label;

pub struct SettingsPanel;

impl Panel for SettingsPanel {
    fn title(&self) -> &str {
        "Settings"
    }

    fn ui(&mut self, ui: &mut egui::Ui, station: &mut Station) {
        let now = station.now;
        ui.horizontal(|ui| {
            if ui.button("Reload from vehicle").clicked() {
                station.client.request_params(now);
                station.param_edits.clear();
            }
            tx_status(ui, station, label::PARAM_LIST);
            ui.label(format!("{} of {} loaded", station.client.params.len(), station.client.param_count));
        });
        if station.client.params.is_empty() {
            ui.colored_label(theme::DIM, "No settings received yet.");
            return;
        }
        ui.separator();

        let ids: Vec<u16> = station.client.params.keys().copied().collect();
        egui::Grid::new("settings").num_columns(5).striped(true).show(ui, |ui| {
            for heading in ["Setting", "On vehicle", "New value", "", "Delivery"] {
                ui.label(egui::RichText::new(heading).small().color(theme::DIM));
            }
            ui.end_row();
            for id in ids {
                let entry = station.client.params[&id].clone();
                ui.label(&entry.name).on_hover_text(format!("Allowed range {} to {}", entry.min, entry.max));
                ui.monospace(format!("{:.3}", entry.value));

                // An edit stays until the vehicle holds that value, so a send that failed
                // can be retried without retyping.
                if station.param_edits.get(&id).is_some_and(|e| (e - entry.value).abs() <= 1e-6) {
                    station.param_edits.remove(&id);
                }
                let mut edit = station.param_edits.get(&id).copied().unwrap_or(entry.value);
                let step = ((entry.max - entry.min) / 200.0).max(0.001);
                // Deliberately not clamped: the vehicle is the authority on what it accepts,
                // and the readback shows what it actually applied.
                if ui.add(egui::DragValue::new(&mut edit).speed(step).max_decimals(3)).changed() {
                    station.param_edits.insert(id, edit);
                }
                let changed = (edit - entry.value).abs() > 1e-6;
                if ui.add_enabled(changed, egui::Button::new("Send")).clicked() {
                    station.client.set_param(id, edit, now);
                }
                ui.horizontal(|ui| tx_status(ui, station, &label::param(id)));
                ui.end_row();
            }
        });
    }
}
