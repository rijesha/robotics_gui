//! Waypoint missions and polygon surveys: plan, upload, run.

use crate::station::MapTool;
use crate::widgets::tx_status;
use crate::{theme, Panel, Station};
use rgf_core::client::label;
use rgf_core::msgs::mission::MissionAction;
use rgf_core::msgs::telemetry::NavState;
use rgf_core::{LatLon, LocalFrame};

pub struct MissionPanel;

fn length_m(points: &[LatLon]) -> f64 {
    points.windows(2).map(|w| LocalFrame::new(w[0]).to_local(w[1]).length()).sum()
}

impl Panel for MissionPanel {
    fn title(&self) -> &str {
        "Mission"
    }

    fn ui(&mut self, ui: &mut egui::Ui, station: &mut Station) {
        let now = station.now;

        ui.strong("Waypoints");
        ui.horizontal(|ui| {
            let tool = &mut station.plan.tool;
            ui.selectable_value(tool, MapTool::Waypoints, "Add on map");
            if ui.add_enabled(!station.plan.waypoints.is_empty(), egui::Button::new("Clear")).clicked() {
                station.plan.waypoints.clear();
            }
            let n = station.plan.waypoints.len();
            ui.label(format!("{n} points, {:.0} m", length_m(&station.plan.waypoints)));
        });
        ui.horizontal(|ui| {
            ui.label("Speed limit");
            ui.add(egui::DragValue::new(&mut station.plan.speed_mps).range(0.0..=10.0).speed(0.05).suffix(" m/s"))
                .on_hover_text("0 uses the vehicle's cruise speed setting");
        });

        ui.add_space(6.0);
        ui.strong("Survey area");
        ui.horizontal(|ui| {
            let tool = &mut station.plan.tool;
            ui.selectable_value(tool, MapTool::Polygon, "Draw on map");
            if ui.add_enabled(!station.plan.polygon.is_empty(), egui::Button::new("Clear")).clicked() {
                station.plan.polygon.clear();
            }
            ui.label(format!("{} corners", station.plan.polygon.len()));
        });
        let survey = &mut station.plan.survey;
        egui::Grid::new("survey_params").num_columns(2).show(ui, |ui| {
            ui.label("Pass spacing");
            ui.add(egui::DragValue::new(&mut survey.swath_m).range(0.5..=100.0).speed(0.1).suffix(" m"));
            ui.end_row();
            ui.label("Pass direction");
            ui.add(egui::DragValue::new(&mut survey.angle_deg).range(0.0..=180.0).speed(1.0).suffix("°"));
            ui.end_row();
            ui.label("Overshoot");
            ui.add(egui::DragValue::new(&mut survey.overshoot_m).range(0.0..=20.0).speed(0.1).suffix(" m"));
            ui.end_row();
            ui.label("Perimeter lap");
            ui.checkbox(&mut survey.perimeter_lap, "");
            ui.end_row();
        });
        let survey_path = station.plan.survey_path();
        ui.horizontal(|ui| {
            if ui.add_enabled(!survey_path.is_empty(), egui::Button::new("Use survey as mission")).clicked() {
                station.plan.waypoints = survey_path.clone();
                station.plan.tool = MapTool::Pan;
            }
            if !survey_path.is_empty() {
                ui.label(format!("{} points, {:.0} m", survey_path.len(), length_m(&survey_path)));
            }
        });

        ui.separator();
        ui.horizontal(|ui| {
            let ready = !station.plan.waypoints.is_empty() && station.plan.waypoints.len() <= u16::MAX as usize;
            if ui.add_enabled(ready, egui::Button::new("Upload to vehicle")).clicked() {
                let waypoints = station.plan.waypoints.clone();
                station.client.upload_mission(&waypoints, station.plan.speed_mps, now);
                station.plan.uploaded = waypoints;
            }
            tx_status(ui, station, label::MISSION);
        });
        ui.horizontal(|ui| {
            for (action, name) in [
                (MissionAction::MISSION_START, "Start"),
                (MissionAction::MISSION_PAUSE, "Pause"),
                (MissionAction::MISSION_CLEAR, "Clear vehicle"),
            ] {
                if ui.button(name).clicked() {
                    station.client.mission_command(action, now);
                    if action == MissionAction::MISSION_CLEAR {
                        station.plan.uploaded.clear();
                    }
                }
            }
            tx_status(ui, station, label::MISSION_CMD);
        });

        ui.separator();
        match &station.client.nav {
            Some(nav) => {
                let (text, color) = match nav.state {
                    NavState::NAV_IDLE => ("Idle", theme::DIM),
                    NavState::NAV_RUNNING => ("Running", theme::OK),
                    NavState::NAV_PAUSED => ("Paused", theme::WARN),
                    NavState::NAV_COMPLETE => ("Complete", theme::ACCENT),
                };
                ui.horizontal(|ui| {
                    ui.label("Vehicle mission:");
                    ui.colored_label(color, text);
                });
                if nav.wp_count > 0 {
                    let done = nav.active_wp.min(nav.wp_count);
                    ui.add(
                        egui::ProgressBar::new(done as f32 / nav.wp_count as f32)
                            .text(format!("waypoint {done} of {}", nav.wp_count)),
                    );
                }
                if nav.state == NavState::NAV_RUNNING {
                    ui.label(format!(
                        "Next waypoint in {:.1} m, off track by {:+.2} m",
                        nav.dist_to_wp_m, nav.xtrack_m
                    ));
                }
            }
            None => {
                ui.colored_label(theme::DIM, "No mission status yet.");
            }
        }
    }
}
