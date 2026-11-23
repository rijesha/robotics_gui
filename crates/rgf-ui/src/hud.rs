//! What the map layout keeps in front of the map: the vehicles, the mission, the camera,
//! the motors and the drive controls, each as a small card pinned to a corner.
//!
//! The cards are drawn in the map's own layer, after it, so floating panels always open
//! above them.

use crate::panels::drive::{drive_controls, mode_row};
use crate::panels::{map, mission, motors, video};
use crate::widgets::{heading, tx_status, tx_visible};
use crate::{fleet, Station};
use egui::{Align2, Rect, Sense, Vec2};
use rgf_core::client::label;
use rgf_core::msgs::common::Mode;
use std::collections::HashMap;

const MARGIN: f32 = 8.0;
const GAP: f32 = 6.0;
/// Room left for the map's own toolbar along its top edge.
const BELOW_TOOLBAR: f32 = 52.0;
/// Room left for the map's height legend down its right edge.
const BELOW_LEGEND: f32 = 178.0;
/// Room left for the map's scale bar and imagery credit along its bottom edge.
const ABOVE_SCALE: f32 = 44.0;
const ABOVE_CREDIT: f32 = 28.0;

/// Which cards to leave out because the panel that says the same thing is open.
#[derive(Default, Clone, Copy)]
pub struct Hidden {
    pub mission: bool,
    pub video: bool,
    pub motors: bool,
    pub drive: bool,
}

#[derive(Default)]
pub struct Hud {
    /// Size each card came out at last frame; a card is pinned by a corner, so where it
    /// starts depends on how big it is.
    sizes: HashMap<&'static str, Vec2>,
}

impl Hud {
    /// Draw one card with `corner` of it at `at`. Returns the space it takes up.
    fn card(
        &mut self,
        ui: &mut egui::Ui,
        key: &'static str,
        corner: Align2,
        at: egui::Pos2,
        width: f32,
        add_contents: impl FnOnce(&mut egui::Ui),
    ) -> Rect {
        let known = self.sizes.get(key).copied();
        let rect = corner.anchor_size(at, known.unwrap_or(Vec2::ZERO));
        let mut builder = egui::UiBuilder::new()
            .id_salt(key)
            .max_rect(Rect::from_min_size(rect.min, Vec2::new(width, ui.max_rect().height())));
        match known {
            // Registered before the contents so they stay on top of it: keeps clicks and
            // drags on the card from reaching the map underneath.
            Some(_) => drop(ui.interact(rect, ui.scope_id().with(("hud", key)), Sense::click_and_drag())),
            // Not measured yet: lay it out unseen this once.
            None => builder = builder.invisible(),
        }
        let mut child = ui.new_child(builder);
        let p = crate::theme::of(&child);
        let fill = egui::Color32::from_rgba_unmultiplied(p.surface.r(), p.surface.g(), p.surface.b(), 236);
        let size =
            map::toolbar_frame(&child).fill(fill).inner_margin(8).show(&mut child, add_contents).response.rect.size();
        if known != Some(size) {
            self.sizes.insert(key, size);
            ui.ctx().request_repaint();
        }
        Rect::from_min_size(rect.min, size)
    }

    /// Draw every card over `area`, the rectangle the map fills.
    pub fn ui(&mut self, ui: &mut egui::Ui, area: Rect, station: &mut Station, hidden: Hidden) {
        let top_left = area.left_top() + Vec2::new(MARGIN, BELOW_TOOLBAR);
        let vehicles = self.card(ui, "vehicles", Align2::LEFT_TOP, top_left, 230.0, |ui| {
            ui.set_width(214.0);
            heading(ui, "Vehicles");
            fleet::list(ui, station);
        });
        let has_mission = station.client.nav.as_ref().is_some_and(|n| n.wp_count > 0);
        if has_mission && !hidden.mission {
            let at = vehicles.left_bottom() + Vec2::new(0.0, GAP);
            self.card(ui, "mission", Align2::LEFT_TOP, at, 230.0, |ui| {
                ui.set_width(214.0);
                heading(ui, "Mission");
                mission::progress(ui, station);
            });
        }

        if !hidden.video && station.video_texture.is_some() {
            let at = area.right_top() + Vec2::new(-MARGIN, BELOW_LEGEND);
            self.card(ui, "video", Align2::RIGHT_TOP, at, 260.0, |ui| {
                video::picture(ui, station, 236.0);
            });
        }

        if !hidden.motors && !station.client.actuators.is_empty() {
            let at = area.left_bottom() + Vec2::new(MARGIN, -ABOVE_SCALE);
            self.card(ui, "motors", Align2::LEFT_BOTTOM, at, 340.0, |ui| {
                heading(ui, "Motors");
                motors::strip(ui, station);
            });
        }

        if !hidden.drive {
            let at = area.right_bottom() + Vec2::new(-MARGIN, -ABOVE_CREDIT);
            let manual = station.client.mode() == Some(Mode::MANUAL);
            self.card(ui, "drive", Align2::RIGHT_BOTTOM, at, 400.0, |ui| {
                mode_row(ui, station);
                if tx_visible(station, label::MODE) {
                    ui.horizontal(|ui| tx_status(ui, station, label::MODE));
                }
                if manual {
                    ui.horizontal(|ui| {
                        drive_controls(ui, station, 112.0);
                        let (throttle, steer) = station.drive_input;
                        let text = format!("Throttle {:+4.0}%\nSteer    {:+4.0}%", throttle * 100.0, steer * 100.0);
                        ui.label(egui::RichText::new(text).monospace().size(12.0));
                    });
                }
            });
            if !manual {
                station.drive_input = (0.0, 0.0);
            }
        }
    }
}
