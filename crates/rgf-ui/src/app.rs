//! The stock application: a status bar over a workspace holding every built-in panel.

use crate::link::LinkState;
use crate::panels::drive::{drive_controls, mode_name};
use crate::panels::rtk::fix_chip;
use crate::panels::{self, DEFAULT_LAYOUT};
use crate::theme::{self, Palette};
use crate::widgets::{battery_icon, chip, tx_status};
use crate::{Station, Workspace};
use egui::{Margin, RichText};
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
    pub fn new(ctx: &egui::Context, backend_url: impl Into<String>, tile_cache: Option<PathBuf>, dark: bool) -> Self {
        theme::install(ctx, dark);
        Self {
            station: Station::new(backend_url),
            workspace: Workspace::new(panels::all(ctx, tile_cache), DEFAULT_LAYOUT),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.station.update(ui.ctx());
        // Telemetry arrives continuously; keep drawing even when the pointer is still.
        ui.ctx().request_repaint_after(Duration::from_millis(33));

        let p = theme::of(ui);
        let phone = Workspace::is_phone(ui);
        let bar = egui::Frame::new().fill(p.surface).inner_margin(Margin::symmetric(12, 7));
        egui::Panel::top("status_bar").frame(bar).show(ui, |ui| self.status_bar(ui, phone, &p));
        self.banners(ui, &p);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg)).show(ui, |ui| {
            self.workspace.ui(ui, &mut self.station);
        });

        // On a phone the map fills the screen, so manual driving gets a stick on top of it.
        if phone && self.workspace.phone_panel() == "Map" && self.station.client.mode() == Some(Mode::MANUAL) {
            egui::Area::new(egui::Id::unique("map_stick"))
                .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -72.0))
                .show(ui.ctx(), |ui| drive_controls(ui, &mut self.station, 130.0));
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui, phone: bool, p: &Palette) {
        let station = &mut self.station;
        let now = station.now;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let alive = station.client.vehicle_alive(now);
            let name = station.client.info.as_ref().map(|i| str_from(&i.name));
            match name {
                Some(name) => ui.label(RichText::new(name).size(15.0).strong()),
                None => ui.label(RichText::new("No vehicle").size(15.0).strong().color(p.dim)),
            };

            if alive {
                if let Some(heartbeat) = &station.client.heartbeat {
                    let color = match heartbeat.mode {
                        Mode::DISARMED => p.dim,
                        Mode::AUTO => p.ok,
                        Mode::MOTOR_TEST => p.warn,
                        _ => p.accent,
                    };
                    chip(ui, mode_name(heartbeat.mode), color).on_hover_text("Vehicle mode");
                }
                // A phone has room for the mode and the battery only.
                if let (Some(pose), false) = (&station.client.pose, phone) {
                    fix_chip(ui, pose.fix).on_hover_text(format!("Position accuracy ±{:.2} m", pose.h_acc_m));
                }
                if let Some(power) = &station.client.power {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;
                        battery_icon(ui, power.soc_pct / 100.0, p);
                        ui.label(format!("{:.0}%", power.soc_pct));
                        if !phone {
                            ui.label(RichText::new(format!("{:.0} W", power.power_w)).color(p.dim));
                        }
                    })
                    .response
                    .on_hover_text("Battery charge and present power draw");
                }
                if let (Some(pose), false) = (&station.client.pose, phone) {
                    ui.label(RichText::new(format!("{:.1} m/s", pose.speed_mps)).color(p.dim));
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let engaged = station.client.heartbeat.as_ref().is_some_and(|h| h.estop);
                let text =
                    RichText::new(if engaged { "RELEASE" } else { "STOP" }).strong().size(14.0).color(p.on_accent);
                let button =
                    egui::Button::new(text).fill(if engaged { p.warn } else { p.bad }).min_size(egui::vec2(78.0, 30.0));
                let response = ui.add(button).on_hover_text(if engaged {
                    "Release the emergency stop"
                } else {
                    "Emergency stop: cut every motor now"
                });
                station.anchor("stop", &response);
                if response.clicked() {
                    station.client.set_estop(!engaged, now);
                }
                tx_status(ui, station, label::ESTOP);

                let dark = theme::is_dark(ui.ctx());
                let response = ui.button(if dark { "☀" } else { "🌙" }).on_hover_text(if dark {
                    "Switch to the light theme"
                } else {
                    "Switch to the dark theme"
                });
                station.anchor("theme", &response);
                if response.clicked() {
                    theme::set_dark(ui.ctx(), !dark);
                }
                if !phone {
                    ui.menu_button("View", |ui| {
                        for title in self.workspace.titles() {
                            if ui.button(&title).clicked() {
                                self.workspace.show_panel(&title);
                            }
                        }
                        ui.separator();
                        if ui.button("Reset layout").clicked() {
                            self.workspace.reset_layout();
                        }
                    });
                }
            });
        });
    }

    /// Full-width notices for conditions the operator must not miss.
    fn banners(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let station = &mut self.station;
        let now = station.now;
        let banner = |ui: &mut egui::Ui, id: &str, color: egui::Color32, text: String| {
            let frame = egui::Frame::new().fill(theme::mix(p.bg, color, 0.22)).inner_margin(Margin::symmetric(12, 5));
            egui::Panel::top(id.to_owned()).frame(frame).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("●").color(color));
                    ui.label(RichText::new(text).strong());
                });
            });
        };
        if station.link.state != LinkState::Connected {
            banner(
                ui,
                "banner_backend",
                p.bad,
                format!("Not connected to the backend at {}. Retrying…", station.link.url),
            );
        } else if station.client.heartbeat.is_none() {
            banner(ui, "banner_wait", p.warn, "Connected. Waiting for a vehicle…".to_owned());
        } else if !station.client.vehicle_alive(now) {
            let age = now - station.client.last_heartbeat;
            banner(ui, "banner_lost", p.bad, format!("Vehicle link lost. Showing data from {age:.0} s ago."));
        } else if station.client.heartbeat.as_ref().is_some_and(|h| h.estop) {
            banner(ui, "banner_estop", p.bad, "Emergency stop engaged. All motors are cut.".to_owned());
        }
    }
}
