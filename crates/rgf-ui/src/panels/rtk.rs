//! RTK ground station: base status, configuration, and the rover's resulting fix.

use crate::widgets::{chip, tx_status};
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

pub fn fix_chip(ui: &mut egui::Ui, fix: FixType) {
    let (text, color) = match fix {
        FixType::NO_FIX => ("No fix", theme::BAD),
        FixType::FIX_3D => ("3D fix", theme::WARN),
        FixType::RTK_FLOAT => ("RTK float", theme::ACCENT),
        FixType::RTK_FIXED => ("RTK fixed", theme::OK),
    };
    chip(ui, text, color);
}

impl Panel for RtkPanel {
    fn title(&self) -> &str {
        "RTK Base"
    }

    fn ui(&mut self, ui: &mut egui::Ui, station: &mut Station) {
        ui.strong("Base station");
        match station.client.base.clone() {
            None => {
                ui.colored_label(theme::DIM, "No base station status yet.");
            }
            Some(base) => {
                ui.horizontal(|ui| match base.mode {
                    BaseMode::BASE_IDLE => {
                        chip(ui, "Idle", theme::DIM);
                    }
                    BaseMode::BASE_SURVEY_IN => {
                        chip(ui, "Surveying in", theme::WARN);
                        ui.label(format!("{} s elapsed", base.survey_elapsed_s));
                    }
                    BaseMode::BASE_FIXED => {
                        chip(ui, "Broadcasting", theme::OK);
                        ui.label(format!(
                            "{:.0} Hz RTCM, {:.1} kB sent",
                            base.rtcm_rate_hz,
                            base.rtcm_bytes as f32 / 1000.0
                        ));
                    }
                });
                if base.mode == BaseMode::BASE_SURVEY_IN {
                    ui.add(
                        egui::ProgressBar::new(base.survey_progress)
                            .text(format!("mean accuracy {:.2} m", base.accuracy_m)),
                    );
                }
                egui::Grid::new("base_status").num_columns(2).show(ui, |ui| {
                    ui.label("Position");
                    ui.monospace(format!("{:.7}, {:.7}", base.lat, base.lon));
                    ui.end_row();
                    ui.label("Accuracy");
                    ui.label(format!("{:.2} m", base.accuracy_m));
                    ui.end_row();
                    ui.label("Satellites");
                    ui.label(base.sats.to_string());
                    ui.end_row();
                    if let Some(rover) = station.vehicle_pos() {
                        let baseline = LocalFrame::new(LatLon::new(base.lat, base.lon)).to_local(rover).length();
                        ui.label("Baseline to rover");
                        ui.label(format!("{baseline:.1} m"));
                        ui.end_row();
                    }
                });
                if ui.button("Copy base position into fixed fields").clicked() {
                    (self.lat, self.lon, self.alt_m) = (base.lat, base.lon, base.alt_m);
                }
            }
        }

        ui.separator();
        ui.strong("Configure");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.use_fixed, false, "Survey in");
            ui.selectable_value(&mut self.use_fixed, true, "Fixed position");
        });
        egui::Grid::new("base_config").num_columns(2).show(ui, |ui| {
            if self.use_fixed {
                ui.label("Latitude");
                ui.add(egui::DragValue::new(&mut self.lat).speed(0.00001).max_decimals(7));
                ui.end_row();
                ui.label("Longitude");
                ui.add(egui::DragValue::new(&mut self.lon).speed(0.00001).max_decimals(7));
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
            if ui.button("Apply").clicked() {
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
            tx_status(ui, station, label::BASE);
        });

        ui.separator();
        ui.strong("Rover");
        match &station.client.pose {
            Some(pose) => {
                ui.horizontal(|ui| {
                    fix_chip(ui, pose.fix);
                    ui.label(format!("±{:.2} m, {} satellites", pose.h_acc_m, pose.sats));
                });
            }
            None => {
                ui.colored_label(theme::DIM, "No position yet.");
            }
        }
    }
}
