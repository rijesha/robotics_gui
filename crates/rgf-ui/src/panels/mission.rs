//! Waypoint missions and polygon surveys: plan, send, run.

use crate::station::MapTool;
use crate::widgets::{card, hint, meter, primary_button, segment, tx_status};
use crate::{theme, Panel, Station};
use rgf_core::client::label;
use rgf_core::msgs::mission::MissionAction;
use rgf_core::msgs::telemetry::NavState;
use rgf_core::{LatLon, LocalFrame};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Waypoints,
    Survey,
}

pub struct MissionPanel {
    kind: Kind,
}

impl Default for MissionPanel {
    fn default() -> Self {
        Self { kind: Kind::Waypoints }
    }
}

fn length_m(points: &[LatLon]) -> f64 {
    // Adding zero turns the empty sum's negative zero into plain zero.
    points.windows(2).map(|w| LocalFrame::new(w[0]).to_local(w[1]).length()).sum::<f64>() + 0.0
}

fn distance(metres: f64) -> String {
    if metres >= 1000.0 {
        format!("{:.2} km", metres / 1000.0)
    } else {
        format!("{metres:.0} m")
    }
}

/// How far the vehicle is through its mission.
pub fn progress(ui: &mut egui::Ui, station: &Station) {
    let p = theme::of(ui);
    match &station.client.nav {
        Some(nav) => {
            let (text, color) = match nav.state {
                NavState::NAV_IDLE => ("Idle", p.dim),
                NavState::NAV_RUNNING => ("Running", p.ok),
                NavState::NAV_PAUSED => ("Paused", p.warn),
                NavState::NAV_COMPLETE => ("Complete", p.accent),
            };
            if nav.wp_count > 0 {
                let done = nav.active_wp.min(nav.wp_count);
                let fraction = done as f32 / nav.wp_count as f32;
                meter(ui, fraction, color, format!("{text}: waypoint {done} of {}", nav.wp_count), 20.0);
            } else {
                hint(ui, "No mission on the vehicle.");
            }
            if nav.state == NavState::NAV_RUNNING {
                hint(ui, format!("Next waypoint in {:.1} m. Off track by {:+.2} m.", nav.dist_to_wp_m, nav.xtrack_m));
            }
        }
        None => hint(ui, "No mission status yet."),
    }
}

impl Panel for MissionPanel {
    fn title(&self) -> &str {
        "Mission"
    }

    fn ui(&mut self, ui: &mut egui::Ui, station: &mut Station) {
        let p = theme::of(ui);
        let now = station.now;

        card(ui, "1  Plan", |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let waypoints = segment(ui, &mut self.kind, Kind::Waypoints, "Waypoints");
                let survey = segment(ui, &mut self.kind, Kind::Survey, "Survey area");
                station.anchor("mission.kind.waypoints", &waypoints);
                station.anchor("mission.kind.survey", &survey);
                // Choosing what to plan also arms the matching map tool.
                if waypoints.clicked() {
                    station.plan.tool = MapTool::Waypoints;
                }
                if survey.clicked() {
                    station.plan.tool = MapTool::Polygon;
                }
            });
            ui.add_space(2.0);
            match self.kind {
                Kind::Waypoints => {
                    hint(ui, "Click the map to add waypoints.");
                }
                Kind::Survey => {
                    let spacing = egui::Grid::new("survey_params").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                        let survey = &mut station.plan.survey;
                        ui.label("Pass spacing");
                        let spacing = ui
                            .add(egui::DragValue::new(&mut survey.swath_m).range(0.5..=100.0).speed(0.1).suffix(" m"));
                        ui.end_row();
                        ui.label("Pass direction");
                        ui.add(egui::DragValue::new(&mut survey.angle_deg).range(0.0..=180.0).speed(1.0).suffix("°"))
                            .on_hover_text("Compass direction the passes run in");
                        ui.end_row();
                        ui.label("Overshoot");
                        ui.add(egui::DragValue::new(&mut survey.overshoot_m).range(0.0..=20.0).speed(0.1).suffix(" m"))
                            .on_hover_text("How far each pass runs beyond the edge before turning");
                        ui.end_row();
                        ui.label("Perimeter lap");
                        ui.checkbox(&mut survey.perimeter_lap, "Drive the boundary first");
                        ui.end_row();
                        spacing
                    });
                    station.anchor("mission.spacing", &spacing.inner);
                    let survey_path = station.plan.survey_path();
                    ui.horizontal(|ui| {
                        let response = primary_button(ui, !survey_path.is_empty(), "Create mission from area");
                        station.anchor("mission.use_survey", &response);
                        if response.clicked() {
                            station.plan.waypoints = survey_path.clone();
                            station.plan.tool = MapTool::Pan;
                        }
                        if ui.add_enabled(!station.plan.polygon.is_empty(), egui::Button::new("Clear area")).clicked() {
                            station.plan.polygon.clear();
                        }
                    });
                    if station.plan.polygon.len() < 3 {
                        hint(ui, format!("{} of at least 3 corners placed.", station.plan.polygon.len()));
                    } else {
                        hint(
                            ui,
                            format!("{} passes, {} to drive.", survey_path.len() / 2, distance(length_m(&survey_path))),
                        );
                    }
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                let n = station.plan.waypoints.len();
                ui.label(egui::RichText::new(format!("{n} waypoints")).strong());
                ui.label(egui::RichText::new(distance(length_m(&station.plan.waypoints))).color(p.dim));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(n > 0, egui::Button::new("Clear")).clicked() {
                        station.plan.waypoints.clear();
                    }
                    ui.add(
                        egui::DragValue::new(&mut station.plan.speed_mps)
                            .range(0.0..=10.0)
                            .speed(0.05)
                            .prefix("max ")
                            .suffix(" m/s"),
                    )
                    .on_hover_text("Speed limit for this mission. 0 uses the vehicle's cruise speed setting.");
                });
            });
        });
        ui.add_space(4.0);

        card(ui, "2  Send and run", |ui| {
            let count = station.plan.waypoints.len();
            let ready = count > 0 && count <= u16::MAX as usize;
            let in_sync = station.plan.uploaded == station.plan.waypoints;
            let state = station.client.nav.as_ref().map(|n| n.state);
            let running = state == Some(NavState::NAV_RUNNING);
            ui.horizontal(|ui| {
                let text = if count > 0 { format!("Upload {count} waypoints") } else { "Upload".to_owned() };
                let response = primary_button(ui, ready && !in_sync, &text);
                station.anchor("mission.upload", &response);
                if response.clicked() {
                    let waypoints = station.plan.waypoints.clone();
                    let speed_mps = station.plan.speed_mps;
                    station.client.upload_mission(&waypoints, speed_mps, now);
                    station.plan.uploaded = waypoints;
                }
                tx_status(ui, station, label::MISSION);
            });
            if count == 0 {
                hint(ui, "Plan a route first.");
            } else if !in_sync {
                hint(ui, "The plan differs from what the vehicle has.");
            }
            ui.horizontal(|ui| {
                let start_text = if state == Some(NavState::NAV_PAUSED) { "Resume" } else { "Start" };
                let can_start = !running && !station.plan.uploaded.is_empty() && in_sync;
                let response = primary_button(ui, can_start, start_text);
                station.anchor("mission.start", &response);
                if response.clicked() {
                    station.client.mission_command(MissionAction::MISSION_START, now);
                }
                let response = ui.add_enabled(running, egui::Button::new("Pause"));
                station.anchor("mission.pause", &response);
                if response.clicked() {
                    station.client.mission_command(MissionAction::MISSION_PAUSE, now);
                }
                if ui.button("Clear").on_hover_text("Remove the mission from the vehicle").clicked() {
                    station.client.mission_command(MissionAction::MISSION_CLEAR, now);
                    station.plan.uploaded.clear();
                }
                tx_status(ui, station, label::MISSION_CMD);
            });
            progress(ui, station);
        });
    }
}
