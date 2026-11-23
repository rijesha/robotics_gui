//! The stock application: a status bar over a workspace holding every built-in panel.

use crate::link::LinkState;
use crate::panels::drive::{drive_controls, mode_name};
use crate::panels::rtk::fix_chip;
use crate::panels::{self, DEFAULT_LAYOUT};
use crate::widgets::{chip, tx_status};
use crate::{theme, Station, Workspace};
use rgf_core::client::label;
use rgf_core::codec::str_from;
use rgf_core::msgs::common::Mode;
use std::path::PathBuf;
use std::time::Duration;

pub struct StationApp {
    pub station: Station,
    pub workspace: Workspace,
}

impl StationApp {
    /// `backend_url` is the backend's WebSocket address; `tile_cache` an optional
    /// directory for map tiles (ignored in the browser, which caches by itself).
    pub fn new(ctx: &egui::Context, backend_url: impl Into<String>, tile_cache: Option<PathBuf>) -> Self {
        theme::apply(ctx);
        Self {
            station: Station::new(backend_url),
            workspace: Workspace::new(panels::all(ctx, tile_cache), DEFAULT_LAYOUT),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.station.update(ui.ctx());
        // Telemetry arrives continuously; keep drawing even when the pointer is still.
        ui.ctx().request_repaint_after(Duration::from_millis(33));

        let phone = Workspace::is_phone(ui);
        egui::Panel::top("status_bar").show(ui, |ui| self.status_bar(ui, phone));
        egui::CentralPanel::default().frame(egui::Frame::new().fill(ui.visuals().panel_fill)).show(ui, |ui| {
            self.workspace.ui(ui, &mut self.station);
        });

        // On a phone the map fills the screen, so manual driving gets a stick on top of it.
        if phone && self.workspace.phone_panel() == "Map" && self.station.client.mode() == Some(Mode::MANUAL) {
            egui::Area::new(egui::Id::unique("map_stick"))
                .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -64.0))
                .show(ui.ctx(), |ui| drive_controls(ui, &mut self.station, 130.0));
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui, phone: bool) {
        let station = &mut self.station;
        let now = station.now;
        ui.horizontal_wrapped(|ui| {
            ui.add_space(2.0);
            let alive = station.client.vehicle_alive(now);
            match (&station.client.info, station.link.state) {
                (_, LinkState::Disconnected | LinkState::Connecting) => {
                    chip(ui, "No backend", theme::BAD);
                }
                (Some(info), _) if alive => {
                    ui.strong(str_from(&info.name));
                }
                (Some(_), _) => {
                    chip(ui, "Vehicle link lost", theme::BAD);
                }
                (None, _) => {
                    chip(ui, "Waiting for vehicle", theme::WARN);
                }
            }

            if let Some(heartbeat) = &station.client.heartbeat {
                let color = match heartbeat.mode {
                    Mode::DISARMED => theme::DIM,
                    Mode::AUTO => theme::OK,
                    Mode::MOTOR_TEST => theme::WARN,
                    _ => theme::ACCENT,
                };
                chip(ui, mode_name(heartbeat.mode), color);
            }
            if let Some(pose) = &station.client.pose {
                fix_chip(ui, pose.fix);
                if !phone {
                    ui.label(format!("{:.1} m/s", pose.speed_mps));
                }
            }
            if let Some(power) = &station.client.power {
                let color = if power.soc_pct < 15.0 {
                    theme::BAD
                } else if power.soc_pct < 30.0 {
                    theme::WARN
                } else {
                    ui.visuals().text_color()
                };
                ui.colored_label(color, format!("{:.0}%", power.soc_pct));
                if !phone {
                    ui.label(format!("{:.0} W", power.power_w));
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let engaged = station.client.heartbeat.as_ref().is_some_and(|h| h.estop);
                let (text, fill) = if engaged { ("RELEASE STOP", theme::WARN) } else { ("STOP", theme::BAD) };
                let button =
                    egui::Button::new(egui::RichText::new(text).strong().color(egui::Color32::BLACK)).fill(fill);
                if ui.add(button).on_hover_text("Emergency stop: cuts every motor").clicked() {
                    station.client.set_estop(!engaged, now);
                }
                tx_status(ui, station, label::ESTOP);
                if !phone && ui.button("Reset layout").clicked() {
                    self.workspace.reset_layout();
                }
            });
        });
    }
}
