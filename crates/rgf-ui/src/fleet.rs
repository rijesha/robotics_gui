//! Choosing between vehicles: a switcher for the status bar and a list for the map layout.

use crate::panels::drive::mode_name;
use crate::station::Vehicle;
use crate::theme::{self, Palette};
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
