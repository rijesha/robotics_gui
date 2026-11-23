//! Connection status, and controls for the simulator.

use crate::link::LinkState;
use crate::widgets::{chip, tx_status};
use crate::{theme, Panel, Station};
use rgf_core::client::label;
use rgf_core::msgs::common::VehicleType;
use rgf_core::msgs::simctl::{SimLinkFault, SimSelectVehicle};

pub struct LinkPanel {
    loss_pct: u8,
    cut: bool,
    rate: (f64, u64, f32),
}

impl Default for LinkPanel {
    fn default() -> Self {
        Self { loss_pct: 0, cut: false, rate: (0.0, 0, 0.0) }
    }
}

const VEHICLES: [(VehicleType, &str); 4] = [
    (VehicleType::ROVER6, "Six-wheel rover"),
    (VehicleType::TRACTOR, "Tractor"),
    (VehicleType::SKID_STEER, "Skid steer"),
    (VehicleType::BOAT, "Boat"),
];

impl Panel for LinkPanel {
    fn title(&self) -> &str {
        "Link"
    }

    fn ui(&mut self, ui: &mut egui::Ui, station: &mut Station) {
        let now = station.now;
        // Receive rate, averaged over a second.
        if now - self.rate.0 >= 1.0 {
            let bytes = station.link.rx_bytes;
            self.rate = (now, bytes, (bytes - self.rate.1) as f32 / (now - self.rate.0) as f32);
        }

        ui.strong("Backend");
        ui.horizontal(|ui| {
            let response = ui.add(egui::TextEdit::singleline(&mut station.link.url).desired_width(170.0));
            if (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                || ui.button("Connect").clicked()
            {
                station.link.reconnect();
            }
        });
        ui.horizontal(|ui| {
            match station.link.state {
                LinkState::Connected => chip(ui, "Connected", theme::OK),
                LinkState::Connecting => chip(ui, "Connecting", theme::WARN),
                LinkState::Disconnected => chip(ui, "Disconnected", theme::BAD),
            };
            ui.label(format!("{:.0} kB/s", self.rate.2 / 1000.0));
        });
        if let Some(err) = &station.link.last_error {
            ui.colored_label(theme::BAD, err);
        }

        ui.add_space(4.0);
        ui.strong("Vehicle");
        ui.horizontal(|ui| {
            if station.client.vehicle_alive(now) {
                chip(ui, "Heartbeat", theme::OK);
            } else if station.client.heartbeat.is_some() {
                chip(ui, "Lost", theme::BAD);
                ui.label(format!("last heard {:.0} s ago", now - station.client.last_heartbeat));
            } else {
                chip(ui, "Not seen", theme::DIM);
            }
        });

        ui.separator();
        ui.strong("Simulator");
        let current = station.client.vehicle_type();
        ui.horizontal_wrapped(|ui| {
            for (vehicle_type, name) in VEHICLES {
                if ui.selectable_label(current == Some(vehicle_type), name).clicked() && current != Some(vehicle_type) {
                    station.client.send_tracked(label::SIM_VEHICLE, now, |seq| SimSelectVehicle { seq, vehicle_type });
                }
            }
        });
        ui.horizontal(|ui| tx_status(ui, station, label::SIM_VEHICLE));

        ui.add_space(4.0);
        ui.label("Radio link fault")
            .on_hover_text("Degrade the simulated vehicle link to see how unconfirmed commands are reported");
        ui.add(egui::Slider::new(&mut self.loss_pct, 0..=90).suffix("% packet loss"));
        ui.checkbox(&mut self.cut, "Cut the link completely");
        ui.horizontal(|ui| {
            if ui.button("Apply fault").clicked() {
                let (loss_pct, cut) = (self.loss_pct, self.cut);
                station.client.send_tracked(label::SIM_FAULT, now, |seq| SimLinkFault { seq, loss_pct, cut });
            }
            tx_status(ui, station, label::SIM_FAULT);
        });
    }
}
