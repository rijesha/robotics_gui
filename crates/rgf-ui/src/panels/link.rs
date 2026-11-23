//! Connection status, and controls for the simulator.

use crate::link::LinkState;
use crate::widgets::{card, chip, hint, primary_button, tx_status, tx_visible};
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
        let p = theme::of(ui);
        let now = station.now;
        // Receive rate, averaged over a second.
        if now - self.rate.0 >= 1.0 {
            let bytes = station.link.rx_bytes;
            self.rate = (now, bytes, (bytes - self.rate.1) as f32 / (now - self.rate.0) as f32);
        }

        card(ui, "Connection", |ui| {
            ui.horizontal_wrapped(|ui| {
                match station.link.state {
                    LinkState::Connected => chip(ui, "Backend connected", p.ok),
                    LinkState::Connecting => chip(ui, "Backend connecting", p.warn),
                    LinkState::Disconnected => chip(ui, "Backend offline", p.bad),
                };
                if station.client.vehicle_alive(now) {
                    chip(ui, "Vehicle heartbeat", p.ok);
                } else if station.client.heartbeat.is_some() {
                    chip(ui, format!("Vehicle lost {:.0} s ago", now - station.client.last_heartbeat), p.bad);
                } else {
                    chip(ui, "No vehicle yet", p.dim);
                }
                ui.label(egui::RichText::new(format!("{:.0} kB/s", self.rate.2 / 1000.0)).color(p.dim));
            });
            ui.horizontal(|ui| {
                let response = ui.add(egui::TextEdit::singleline(&mut station.link.url).desired_width(150.0));
                if (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                    || ui.button("Connect").clicked()
                {
                    station.link.reconnect();
                }
            });
            if let Some(err) = &station.link.last_error {
                ui.colored_label(p.bad, err);
            }
        });
        ui.add_space(4.0);

        card(ui, "Simulated vehicle", |ui| {
            let current = station.client.vehicle_type();
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (index, (vehicle_type, name)) in VEHICLES.into_iter().enumerate() {
                    let response = ui.selectable_label(current == Some(vehicle_type), name);
                    station.anchor(format!("link.vehicle.{index}"), &response);
                    if response.clicked() && current != Some(vehicle_type) {
                        station
                            .client
                            .send_tracked(label::SIM_VEHICLE, now, |seq| SimSelectVehicle { seq, vehicle_type });
                    }
                }
            });
            if tx_visible(station, label::SIM_VEHICLE) {
                ui.horizontal(|ui| tx_status(ui, station, label::SIM_VEHICLE));
            }
        });
        ui.add_space(4.0);

        card(ui, "Simulated radio fault", |ui| {
            hint(ui, "Degrade the link to see how unconfirmed commands are reported.");
            ui.add(egui::Slider::new(&mut self.loss_pct, 0..=90).suffix("% packet loss"));
            let response = ui.checkbox(&mut self.cut, "Cut the link completely");
            station.anchor("link.cut", &response);
            ui.horizontal(|ui| {
                let response = primary_button(ui, true, "Apply");
                station.anchor("link.apply", &response);
                if response.clicked() {
                    let (loss_pct, cut) = (self.loss_pct, self.cut);
                    station.client.send_tracked(label::SIM_FAULT, now, |seq| SimLinkFault { seq, loss_pct, cut });
                }
                tx_status(ui, station, label::SIM_FAULT);
            });
        });
    }
}
