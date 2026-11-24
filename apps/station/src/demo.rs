//! Scripted walkthroughs. A demo drives the real UI with synthetic pointer input, so it
//! exercises the same code paths as an operator, and draws a cursor and captions so a
//! recording explains itself.

use eframe::egui::{self, pos2, Color32, Pos2, Vec2};
use rgf_core::client::label;
use rgf_core::reliable::TxState;
use rgf_ui::StationApp;
use std::collections::VecDeque;

pub enum Step {
    Wait(f64),
    Caption(&'static str),
    /// Bring a panel to the front.
    Show(&'static str),
    /// Click a named control.
    Click(&'static str),
    /// Click the map, in metres east and north of where the vehicle started.
    MapClick(f64, f64),
    /// Drag across the map, in the same coordinates.
    MapDrag((f64, f64), (f64, f64)),
    /// Press on a control at an offset from its centre (as a fraction of its half
    /// size), drag to another offset, hold for some seconds, release.
    Drag(&'static str, (f32, f32), (f32, f32), f64),
    /// Wait until the latest send under a label is confirmed or has failed.
    Settled(&'static str),
}

enum Prim {
    Move(Pos2, f64),
    Button(egui::PointerButton, bool),
    Sleep(f64),
}

pub struct Demo {
    steps: VecDeque<Step>,
    prims: VecDeque<Prim>,
    started: Option<f64>,
    move_from: Pos2,
    cursor: Pos2,
    caption: Option<&'static str>,
    last_click: f64,
    waiting_since: Option<f64>,
}

pub const NAMES: &str = "survey, drive, linkloss, themes, phone, fleet, cruise";

pub fn script(name: &str) -> Option<Vec<Step>> {
    use Step::*;
    Some(match name {
        "survey" => vec![
            Wait(3.0),
            Caption("Plan a survey: outline the area on the map"),
            MapDrag((8.0, -6.0), (4.0, -14.0)),
            Click("mission.kind.survey"),
            MapClick(-18.0, 7.0),
            MapClick(22.0, 5.0),
            MapClick(26.0, 33.0),
            MapClick(-14.0, 36.0),
            Wait(0.5),
            Caption("Set the pass spacing, then turn the area into a mission"),
            Drag("mission.spacing", (0.0, 0.0), (1.3, 0.0), 0.3),
            Click("mission.use_survey"),
            Wait(0.6),
            Click("mission.upload"),
            Settled(label::MISSION),
            Wait(0.8),
            Click("mission.start"),
            Caption("The tractor drives the passes while the lidar maps terrain and obstacles"),
            Wait(20.0),
            Show("Lidar"),
            Caption("The same scan in 3D"),
            Wait(4.0),
        ],
        "drive" => vec![
            Wait(3.0),
            Caption("Manual driving: the yellow line is the expected path"),
            MapDrag((8.0, -8.0), (2.0, -12.0)),
            Click("mode.Manual"),
            Settled(label::MODE),
            Drag("drive.stick", (0.0, 0.0), (0.45, -0.7), 5.0),
            Drag("drive.stick", (0.0, 0.0), (-0.6, -0.6), 4.0),
            Caption("Motor test: run each motor on its own"),
            Click("motors.test"),
            Settled(label::MODE),
            Wait(0.6),
            Drag("motors.slider.0", (-0.38, 0.0), (0.25, 0.0), 3.0),
            Drag("motors.slider.2", (-0.38, 0.0), (-0.75, 0.0), 3.0),
            Click("motors.test"),
            Settled(label::MODE),
            Show("Power"),
            Caption("Power draw and where the current goes"),
            Wait(4.0),
        ],
        "linkloss" => vec![
            Wait(3.0),
            Show("Settings"),
            Caption("Every change is verified. First, one that gets through"),
            Drag("settings.value.0", (0.0, 0.0), (-1.2, 0.0), 0.3),
            Click("settings.apply.0"),
            Settled("param:0"),
            Wait(2.0),
            Caption("Now cut the radio link to the vehicle"),
            Show("Link"),
            Click("link.cut"),
            Click("link.apply"),
            Settled(label::SIM_FAULT),
            Wait(1.0),
            Show("Settings"),
            Caption("This change is sent but never confirmed, so it shows as Not sent"),
            Drag("settings.value.1", (0.0, 0.0), (1.5, 0.0), 0.3),
            Click("settings.apply.1"),
            Settled("param:1"),
            Wait(3.0),
            Caption("Restore the link and retry: confirmed"),
            Show("Link"),
            Click("link.cut"),
            Click("link.apply"),
            Settled(label::SIM_FAULT),
            Wait(0.6),
            Show("Settings"),
            Click("settings.apply.1"),
            Settled("param:1"),
            Wait(3.0),
        ],
        "themes" => vec![
            Wait(4.0),
            Caption("Light and dark themes"),
            MapDrag((8.0, -8.0), (4.0, -4.0)),
            Click("mode.Manual"),
            Drag("drive.stick", (0.0, 0.0), (0.5, -0.7), 3.0),
            Click("theme"),
            Wait(1.0),
            Show("Power"),
            Show("Lidar"),
            Drag("drive.stick", (0.0, 0.0), (-0.5, -0.7), 3.0),
            Show("RTK Base"),
            Wait(1.5),
            Click("theme"),
            Wait(2.5),
            Show("Settings"),
            Wait(2.0),
        ],
        "phone" => vec![
            Wait(3.0),
            Caption("Phone layout: one panel at a time"),
            Show("Drive"),
            Wait(1.0),
            Click("mode.Manual"),
            Settled(label::MODE),
            Wait(0.6),
            Show("Map"),
            Wait(0.6),
            Drag("drive.stick", (0.0, 0.0), (0.4, -0.7), 3.5),
            Show("Video"),
            Wait(2.0),
            Show("Motors"),
            Wait(2.0),
            Click("theme"),
            Wait(1.5),
            Show("Mission"),
            Wait(2.0),
            Show("Map"),
            Wait(2.0),
        ],
        // Run with several vehicles, e.g. --vehicle rover6,tractor,skid.
        "fleet" => vec![
            Wait(3.0),
            Caption("Several vehicles on one link. The map layout keeps the map behind everything"),
            Click("layout.map"),
            Wait(1.0),
            // Dragging the map stops it following the vehicle, which keeps a recording small.
            MapDrag((10.0, -8.0), (9.0, -11.0)),
            Click("mode.Manual"),
            Settled(label::MODE),
            Drag("drive.stick", (0.0, 0.0), (0.35, -0.7), 3.5),
            Caption("Pick another vehicle from the list, or click it on the map"),
            Click("vehicle.1"),
            MapDrag((10.0, -8.0), (9.0, -9.0)),
            Click("mode.Manual"),
            Settled(label::MODE),
            Caption("Any panel floats over the map: current, torque, speed and temperature per motor"),
            Show("Motors"),
            Drag("drive.stick", (0.0, 0.0), (0.5, -0.7), 5.0),
            Wait(1.0),
            Caption("STOP ALL stops every vehicle at once"),
            Click("stop_all"),
            Wait(3.0),
        ],
        // No captions and no layout changes: something moving to look at in any design.
        // Run with at least two vehicles.
        "cruise" => vec![
            Wait(3.0),
            Click("mode.Manual"),
            Settled(label::MODE),
            Drag("drive.stick", (0.0, 0.0), (0.4, -0.7), 3.0),
            Click("vehicle.1"),
            Wait(0.5),
            Click("mode.Manual"),
            Settled(label::MODE),
            Drag("drive.stick", (0.0, 0.0), (0.45, -0.7), 14.0),
        ],
        _ => return None,
    })
}

impl Demo {
    pub fn new(steps: Vec<Step>) -> Self {
        let start = pos2(420.0, 360.0);
        Self {
            steps: steps.into(),
            prims: VecDeque::new(),
            started: None,
            move_from: start,
            cursor: start,
            caption: None,
            last_click: f64::NEG_INFINITY,
            waiting_since: None,
        }
    }

    pub fn finished(&self) -> bool {
        self.steps.is_empty() && self.prims.is_empty()
    }

    fn click_at(&mut self, target: Pos2, button: egui::PointerButton) {
        self.prims.extend([
            Prim::Move(target, 0.55),
            Prim::Sleep(0.12),
            Prim::Button(button, true),
            Prim::Sleep(0.08),
            Prim::Button(button, false),
            Prim::Sleep(0.3),
        ]);
    }

    fn drag(&mut self, from: Pos2, to: Pos2, hold: f64) {
        self.prims.extend([
            Prim::Move(from, 0.55),
            Prim::Sleep(0.12),
            Prim::Button(egui::PointerButton::Primary, true),
            Prim::Move(to, 0.6),
            Prim::Sleep(hold),
            Prim::Button(egui::PointerButton::Primary, false),
            Prim::Sleep(0.3),
        ]);
    }

    /// Turn the next step into pointer actions. Returns false if it has to wait.
    fn begin_step(&mut self, now: f64, screen: egui::Rect, app: &mut StationApp) -> bool {
        let Some(step) = self.steps.front() else { return false };
        let station = &app.station;
        let on_map = |east: f64, north: f64| {
            station.map_view.map(|(origin, px_per_m)| origin + Vec2::new(east as f32, -north as f32) * px_per_m)
        };
        match step {
            Step::Wait(seconds) => self.prims.push_back(Prim::Sleep(*seconds)),
            Step::Caption(text) => self.caption = Some(text),
            Step::Show(title) => {
                app.workspace.show_panel(title);
                self.prims.push_back(Prim::Sleep(0.9));
            }
            Step::Click(name) => {
                // The control may not be laid out yet; try again next frame.
                let Some(target) = station.anchors.get(*name).map(|r| r.center()).filter(|c| screen.contains(*c))
                else {
                    return self.patience(now, name);
                };
                self.click_at(target, egui::PointerButton::Primary);
            }
            Step::MapClick(east, north) => {
                let Some(target) = on_map(*east, *north) else { return self.patience(now, "map") };
                self.click_at(target, egui::PointerButton::Primary);
            }
            Step::MapDrag(from, to) => {
                let (Some(a), Some(b)) = (on_map(from.0, from.1), on_map(to.0, to.1)) else {
                    return self.patience(now, "map");
                };
                self.drag(a, b, 0.1);
            }
            Step::Drag(name, from, to, hold) => {
                let Some(rect) = station.anchors.get(*name).filter(|r| screen.contains(r.center())) else {
                    return self.patience(now, name);
                };
                let half = rect.size() / 2.0;
                let at = |o: &(f32, f32)| rect.center() + Vec2::new(o.0 * half.x, o.1 * half.y);
                let (a, b) = (at(from), at(to));
                self.drag(a, b, *hold);
            }
            Step::Settled(tx_label) => {
                let pending = matches!(station.client.state(tx_label), None | Some(TxState::Pending { .. }));
                if pending && now - *self.waiting_since.get_or_insert(now) < 8.0 {
                    return false;
                }
            }
        }
        self.waiting_since = None;
        self.steps.pop_front();
        true
    }

    /// Wait a little for something to appear, then give up on the step.
    fn patience(&mut self, now: f64, what: &str) -> bool {
        if now - *self.waiting_since.get_or_insert(now) < 5.0 {
            return false;
        }
        eprintln!("demo: '{what}' never appeared, skipping the step");
        self.waiting_since = None;
        self.steps.pop_front();
        true
    }

    /// Advance the script and add this frame's pointer events.
    pub fn tick(&mut self, raw: &mut egui::RawInput, app: &mut StationApp) {
        let now = raw.time.unwrap_or(0.0);
        let screen = raw.screen_rect.unwrap_or(egui::Rect::EVERYTHING);
        // Nothing useful can be pointed at before the vehicle is on the map.
        if app.station.map_view.is_none() {
            return;
        }
        loop {
            if self.prims.is_empty() && !self.begin_step(now, screen, app) {
                return;
            }
            let Some(prim) = self.prims.front() else { continue };
            let started = *self.started.get_or_insert(now);
            let done = match prim {
                Prim::Move(target, duration) => {
                    let t = ((now - started) / duration).clamp(0.0, 1.0) as f32;
                    let eased = t * t * (3.0 - 2.0 * t);
                    self.cursor = self.move_from + (*target - self.move_from) * eased;
                    raw.events.push(egui::Event::PointerMoved(self.cursor));
                    t >= 1.0
                }
                Prim::Button(button, pressed) => {
                    raw.events.push(egui::Event::PointerButton {
                        pos: self.cursor,
                        button: *button,
                        pressed: *pressed,
                        modifiers: Default::default(),
                    });
                    if *pressed {
                        self.last_click = now;
                    }
                    true
                }
                Prim::Sleep(seconds) => now - started >= *seconds,
            };
            if !done {
                return;
            }
            self.prims.pop_front();
            self.started = None;
            self.move_from = self.cursor;
            // One input event per frame, so egui sees presses and releases separately.
            return;
        }
    }

    /// Draw the cursor and caption above everything else.
    pub fn paint(&self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Debug, egui::Id::unique("demo_overlay")));
        let screen = ctx.content_rect();

        if let Some(text) = self.caption {
            let wrap = (screen.width() - 80.0).min(620.0);
            let galley = painter.layout(text.to_owned(), egui::FontId::proportional(16.0), Color32::WHITE, wrap);
            // Clear of the tab bar that the phone layout puts along the bottom.
            let lift = if screen.width() < 700.0 { 78.0 } else { 34.0 };
            let rect = egui::Rect::from_center_size(
                pos2(screen.center().x, screen.bottom() - lift - galley.size().y / 2.0),
                galley.size(),
            );
            painter.rect_filled(
                rect.expand2(Vec2::new(16.0, 9.0)),
                10,
                Color32::from_rgba_unmultiplied(16, 20, 28, 235),
            );
            painter.galley(rect.min, galley, Color32::WHITE);
        }

        let since_click = (now - self.last_click) as f32;
        if (0.0..0.45).contains(&since_click) {
            let t = since_click / 0.45;
            let ring = Color32::from_rgba_unmultiplied(255, 200, 40, (220.0 * (1.0 - t)) as u8);
            painter.circle_stroke(self.cursor, 6.0 + 18.0 * t, egui::Stroke::new(3.0, ring));
        }
        // A plain pointer: convex, so it fills cleanly.
        let tip = self.cursor;
        let points = vec![tip, tip + Vec2::new(0.0, 18.0), tip + Vec2::new(12.5, 12.5)];
        painter.add(egui::Shape::convex_polygon(points, Color32::WHITE, egui::Stroke::new(1.5, Color32::BLACK)));
    }
}
