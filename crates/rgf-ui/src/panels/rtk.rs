//! RTK ground station: base status, configuration, and the rover's resulting fix.

use crate::widgets::{card, chip, hint, meter, primary_button, segment, tx_status};
use crate::{theme, Panel, Station};
use rgf_core::client::label;
use rgf_core::msgs::rtk::{BaseConfig, BaseMode};
use rgf_core::msgs::telemetry::FixType;
use rgf_core::{LatLon, LocalFrame};

pub struct RtkPanel {
    use_fixed: bool,
    min_duration_s: u32,
    accuracy_limit_m: f32,
    lat: f64,
    lon: f64,
    alt_m: f32,
}

impl Default for RtkPanel {
    fn default() -> Self {
        Self { use_fixed: false, min_duration_s: 20, accuracy_limit_m: 0.5, lat: 0.0, lon: 0.0, alt_m: 0.0 }
    }
}

pub fn fix_chip(ui: &mut egui::Ui, fix: FixType) -> egui::Response {
    let p = theme::of(ui);
    let (text, color) = match fix {
        FixType::NO_FIX => ("No fix", p.bad),
        FixType::FIX_3D => ("3D fix", p.warn),
        FixType::RTK_FLOAT => ("RTK float", p.accent),
        FixType::RTK_FIXED => ("RTK fixed", p.ok),
    };
    chip(ui, text, color)
}

impl Panel for RtkPanel {
    fn title(&self) -> &str {
        "RTK Base"
    }

    fn ui(&mut self, ui: &mut egui::Ui, station: &mut Station) {
        let p = theme::of(ui);
        card(ui, "Base station", |ui| match station.client.base.clone() {
            None => hint(ui, "No base station status yet."),
            Some(base) => {
                ui.horizontal(|ui| match base.mode {
                    BaseMode::BASE_IDLE => {
                        chip(ui, "Idle", p.dim);
                    }
                    BaseMode::BASE_SURVEY_IN => {
                        chip(ui, "Surveying in", p.warn);
                        ui.label(format!("{} s elapsed", base.survey_elapsed_s));
                    }
                    BaseMode::BASE_FIXED => {
                        chip(ui, "Broadcasting", p.ok);
                        ui.label(format!(
                            "{:.0} Hz RTCM, {:.1} kB sent",
                            base.rtcm_rate_hz,
                            base.rtcm_bytes as f32 / 1000.0
                        ));
                    }
                });
                if base.mode == BaseMode::BASE_SURVEY_IN {
                    let text = format!("Averaging position, ±{:.2} m so far", base.accuracy_m);
                    meter(ui, base.survey_progress, theme::mix(p.raised, p.warn, 0.7), text, 20.0);
                }
                egui::Grid::new("base_status").num_columns(2).spacing([14.0, 5.0]).show(ui, |ui| {
                    let key = |ui: &mut egui::Ui, text: &str| {
                        ui.label(egui::RichText::new(text).color(p.dim));
                    };
                    key(ui, "Position");
                    ui.monospace(format!("{:.7}, {:.7}", base.lat, base.lon));
                    ui.end_row();
                    key(ui, "Accuracy");
                    ui.label(format!("±{:.2} m", base.accuracy_m));
                    ui.end_row();
                    key(ui, "Satellites");
                    ui.label(base.sats.to_string());
                    ui.end_row();
                    if let Some(rover) = station.vehicle_pos() {
                        let baseline = LocalFrame::new(LatLon::new(base.lat, base.lon)).to_local(rover).length();
                        key(ui, "Distance to rover");
                        ui.label(format!("{baseline:.1} m"));
                        ui.end_row();
                    }
                });
            }
        });
        ui.add_space(4.0);

        card(ui, "Rover", |ui| match &station.client.pose {
            Some(pose) => {
                ui.horizontal(|ui| {
                    fix_chip(ui, pose.fix);
                    ui.label(format!("±{:.2} m, {} satellites", pose.h_acc_m, pose.sats));
                });
            }
            None => hint(ui, "No position yet."),
        });
        ui.add_space(4.0);

        card(ui, "Setup", |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                segment(ui, &mut self.use_fixed, false, "Survey in")
                    .on_hover_text("Let the base average its own position");
                segment(ui, &mut self.use_fixed, true, "Known position")
                    .on_hover_text("Enter a surveyed position for the base");
            });
            egui::Grid::new("base_config").num_columns(2).spacing([14.0, 6.0]).show(ui, |ui| {
                if self.use_fixed {
                    ui.label("Latitude");
                    ui.add(egui::DragValue::new(&mut self.lat).speed(0.00001).max_decimals(7).suffix("°"));
                    ui.end_row();
                    ui.label("Longitude");
                    ui.add(egui::DragValue::new(&mut self.lon).speed(0.00001).max_decimals(7).suffix("°"));
                    ui.end_row();
                    ui.label("Altitude");
                    ui.add(egui::DragValue::new(&mut self.alt_m).speed(0.1).suffix(" m"));
                    ui.end_row();
                } else {
                    ui.label("Minimum time");
                    ui.add(egui::DragValue::new(&mut self.min_duration_s).range(0..=3600).suffix(" s"));
                    ui.end_row();
                    ui.label("Required accuracy");
                    ui.add(egui::DragValue::new(&mut self.accuracy_limit_m).range(0.05..=5.0).speed(0.01).suffix(" m"));
                    ui.end_row();
                }
            });
            ui.horizontal(|ui| {
                let response = primary_button(ui, true, if self.use_fixed { "Set position" } else { "Start survey" });
                station.anchor("rtk.apply", &response);
                if response.clicked() {
                    let config =
                        (self.use_fixed, self.min_duration_s, self.accuracy_limit_m, self.lat, self.lon, self.alt_m);
                    station.client.send_tracked(label::BASE, station.now, |seq| BaseConfig {
                        seq,
                        use_fixed: config.0,
                        min_duration_s: config.1,
                        accuracy_limit_m: config.2,
                        lat: config.3,
                        lon: config.4,
                        alt_m: config.5,
                    });
                }
                if self.use_fixed {
                    if let Some(base) = &station.client.base {
                        if ui
                            .button("Use current")
                            .on_hover_text("Copy the base's present position into the fields")
                            .clicked()
                        {
                            (self.lat, self.lon, self.alt_m) = (base.lat, base.lon, base.alt_m);
                        }
                    }
                }
                tx_status(ui, station, label::BASE);
            });
        });
    }
}
