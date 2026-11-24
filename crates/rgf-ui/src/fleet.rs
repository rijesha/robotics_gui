//! Choosing between vehicles: a switcher for the status bar, a list for the map layout
//! and a board of cards with each vehicle's state.

use crate::panels::drive::mode_name;
use crate::station::Vehicle;
use crate::theme::{self, Palette};
use crate::widgets::{chip, meter};
use crate::Station;
use egui::text::LayoutJob;
use egui::{Color32, FontId, TextFormat};

fn append(job: &mut LayoutJob, text: &str, size: f32, color: Color32) {
    job.append(text, 0.0, TextFormat { font_id: FontId::proportional(size), color, ..Default::default() });
}

/// "Manual · 82% · 1.2 m/s", or what is wrong when the vehicle is not answering.
fn summary(vehicle: &Vehicle, now: f64, p: &Palette) -> (String, Color32) {
    let client = &vehicle.client;
    if client.heartbeat.is_none() {
        return ("waiting…".to_owned(), p.dim);
    }
    if !client.vehicle_alive(now) {
        return (format!("link lost {:.0} s ago", now - client.last_heartbeat), p.bad);
    }
    if client.heartbeat.as_ref().is_some_and(|h| h.estop) {
        return ("stopped".to_owned(), p.bad);
    }
    let mut parts = vec![client.mode().map_or("?", mode_name).to_owned()];
    if let Some(power) = &client.power {
        parts.push(format!("{:.0}%", power.soc_pct));
    }
    if let Some(pose) = &client.pose {
        parts.push(format!("{:.1} m/s", pose.speed_mps));
    }
    (parts.join(" · "), p.dim)
}

/// One button per vehicle; clicking selects it. `detailed` puts the summary on a second
/// line, otherwise only the name shows and the summary is the tooltip.
fn buttons(ui: &mut egui::Ui, station: &mut Station, detailed: bool) {
    let p = theme::of(ui);
    let now = station.now;
    let active = station.active_index();
    // Applied after the loop: the station is borrowed while its vehicles are listed.
    let mut select = None;
    let mut rects = Vec::new();
    for (index, vehicle) in station.vehicles().iter().enumerate() {
        let (status, status_color) = summary(vehicle, now, &p);
        let troubled = status_color == p.bad;
        let mut job = LayoutJob::default();
        append(&mut job, "● ", 12.0, if troubled { p.bad } else { theme::vehicle_color(index) });
        append(&mut job, &format!("{}  ", index + 1), 12.5, p.dim);
        append(&mut job, &vehicle.name(), 13.5, p.text);
        if detailed {
            append(&mut job, &format!("\n      {status}"), 11.5, status_color);
        }
        let mut button = egui::Button::selectable(index == active, job);
        if detailed {
            button = button.min_size(egui::vec2(ui.available_width(), 0.0));
        }
        let mut response = ui.add(button);
        if !detailed {
            response = response.on_hover_text(status);
        }
        if response.clicked() {
            select = Some(index);
        }
        rects.push((index, response));
    }
    for (index, response) in rects {
        station.anchor(format!("vehicle.{index}"), &response);
    }
    if let Some(index) = select {
        station.select(index);
    }
}

/// A row of vehicles for the status bar.
pub fn switcher(ui: &mut egui::Ui, station: &mut Station) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        buttons(ui, station, false);
    });
}

/// A column of vehicles with what each is doing, for showing over the map.
pub fn list(ui: &mut egui::Ui, station: &mut Station) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        buttons(ui, station, true);
    });
}

/// What the board does once the vehicles are no longer borrowed.
enum Action {
    Select(usize),
    Stop(usize, bool),
}

/// A small labelled value on a vehicle's card.
fn reading(ui: &mut egui::Ui, name: &str, value: String, unit: &str, p: &Palette) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.label(egui::RichText::new(name).size(10.5).color(p.dim));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.label(egui::RichText::new(value).size(17.0).strong().color(p.text));
            ui.label(egui::RichText::new(unit).size(11.0).color(p.dim));
        });
    });
}

/// One card per vehicle with what it is doing, how charged it is and how hot it runs.
/// Clicking a card's name selects the vehicle; each has its own stop button.
pub fn board(ui: &mut egui::Ui, station: &mut Station) {
    let p = theme::of(ui);
    let design = crate::design::current();
    let now = station.now;
    let active = station.active_index();
    let mut action = None;
    let mut rects = Vec::new();
    for (index, vehicle) in station.vehicles().iter().enumerate() {
        let client = &vehicle.client;
        let color = theme::vehicle_color(index);
        let selected = index == active;
        let alive = client.vehicle_alive(now);
        let stopped = client.heartbeat.as_ref().is_some_and(|h| h.estop);
        let outline = if selected { egui::Stroke::new(2.0, p.accent) } else { egui::Stroke::new(1.0, p.border) };
        egui::Frame::new()
            .fill(if selected { theme::mix(p.surface, p.accent, 0.06) } else { p.surface })
            .stroke(outline)
            .corner_radius(design.card_radius)
            .inner_margin(10)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let mut job = LayoutJob::default();
                    append(&mut job, "● ", 13.0, color);
                    append(&mut job, &format!("{}  ", index + 1), 13.0, p.dim);
                    append(&mut job, &vehicle.name(), 14.5, p.text);
                    let response = ui.add(egui::Button::new(job).frame(false)).on_hover_text("Select this vehicle");
                    if response.clicked() {
                        action = Some(Action::Select(index));
                    }
                    rects.push((index, response));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (text, fill) = if stopped { ("Release", p.warn) } else { ("Stop", p.bad) };
                        let label = egui::RichText::new(text).size(12.0).strong().color(fill);
                        let button = egui::Button::new(label).stroke(egui::Stroke::new(1.0, fill));
                        if ui.add(button).clicked() {
                            action = Some(Action::Stop(index, !stopped));
                        }
                    });
                });
                ui.horizontal(|ui| {
                    let (text, text_color) = match client.mode() {
                        _ if client.heartbeat.is_none() => ("Waiting", p.dim),
                        _ if !alive => ("Link lost", p.bad),
                        _ if stopped => ("Stopped", p.bad),
                        Some(mode) => (mode_name(mode), if selected { p.accent } else { p.ok }),
                        None => ("?", p.dim),
                    };
                    chip(ui, text, text_color);
                    if let Some(nav) = client.nav.as_ref().filter(|n| n.wp_count > 0) {
                        let done = nav.active_wp.min(nav.wp_count);
                        ui.label(
                            egui::RichText::new(format!("waypoint {done} of {}", nav.wp_count)).size(12.0).color(p.dim),
                        );
                    }
                });
                ui.columns(3, |columns| {
                    let speed = client.pose.as_ref().map_or("–".to_owned(), |pose| format!("{:.1}", pose.speed_mps));
                    reading(&mut columns[0], "Speed", speed, "m/s", &p);
                    let power = client.power.as_ref().map_or("–".to_owned(), |power| format!("{:.0}", power.power_w));
                    reading(&mut columns[1], "Power", power, "W", &p);
                    let hottest = client.motors.iter().flatten().map(|m| m.temp_c).fold(f32::NAN, f32::max);
                    let hottest = if hottest.is_finite() { format!("{hottest:.0}") } else { "–".to_owned() };
                    reading(&mut columns[2], "Hottest motor", hottest, "°C", &p);
                });
                if let Some(power) = &client.power {
                    let soc = power.soc_pct / 100.0;
                    let fill = if soc < 0.15 {
                        p.bad
                    } else if soc < 0.3 {
                        p.warn
                    } else {
                        p.ok
                    };
                    let left = power.time_remaining_s / 3600.0;
                    meter(ui, soc, fill, format!("{:.0}%   about {left:.1} h left", power.soc_pct), 18.0);
                }
            });
        ui.add_space(2.0);
    }
    for (index, response) in rects {
        station.anchor(format!("vehicle.{index}"), &response);
    }
    match action {
        Some(Action::Select(index)) => station.select(index),
        Some(Action::Stop(index, engage)) => station.set_estop(index, engage),
        None => {}
    }
}
