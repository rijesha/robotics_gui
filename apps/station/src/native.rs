//! Desktop entry point: argument parsing, the embedded backend and simulator, and the
//! scripted demo used for unattended checks.

use crate::App;
use eframe::egui;
use rgf_core::client::label;
use rgf_core::msgs::common::Mode;
use rgf_core::msgs::mission::MissionAction;
use rgf_core::msgs::simctl::SimLinkFault;
use rgf_core::reliable::TxState;
use rgf_core::{LocalFrame, Vec2};
use rgf_ui::station::MapTool;
use rgf_ui::StationApp;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

const USAGE: &str = "\
usage: station [options]
  --vehicle NAME      simulated vehicle: rover6 (default), tractor, skid, boat
  --connect URL       use a backend that is already running (ws://host:9002) instead of
                      starting the built-in backend and simulator
  --phone             open at phone size to try the mobile layout
  --show TITLES       panels to bring to the front, comma separated (e.g. Lidar,Power)
  --demo NAME         run a scripted scenario: survey, drive, linkloss, edit
  --screenshot PATH   save a PNG of the window after --after seconds, then exit
  --after SECONDS     delay for --screenshot (default 12)";

struct Args {
    vehicle: String,
    connect: Option<String>,
    phone: bool,
    show: Vec<String>,
    demo: Option<String>,
    screenshot: Option<PathBuf>,
    after: f64,
}

fn parse_args() -> Args {
    let mut args = Args {
        vehicle: "rover6".into(),
        connect: None,
        phone: false,
        show: Vec::new(),
        demo: None,
        screenshot: None,
        after: 12.0,
    };
    let mut it = std::env::args().skip(1);
    let fail = |message: String| -> ! {
        eprintln!("{message}\n{USAGE}");
        std::process::exit(2)
    };
    while let Some(arg) = it.next() {
        let mut value = || it.next().unwrap_or_else(|| fail(format!("{arg} needs a value")));
        match arg.as_str() {
            "--vehicle" => args.vehicle = value(),
            "--connect" => args.connect = Some(value()),
            "--demo" => args.demo = Some(value()),
            "--show" => args.show = value().split(',').map(str::to_owned).collect(),
            "--screenshot" => args.screenshot = Some(value().into()),
            "--after" => args.after = value().parse().unwrap_or_else(|_| fail("--after needs a number".into())),
            "--phone" => args.phone = true,
            "--help" | "-h" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => fail(format!("unknown option {other}")),
        }
    }
    args
}

pub fn run() -> eframe::Result {
    let args = parse_args();
    let backend_url = match &args.connect {
        Some(url) => url.clone(),
        None => {
            let Some(vehicle) = rgf_sim::parse_vehicle(&args.vehicle) else {
                eprintln!("unknown vehicle '{}'\n{USAGE}", args.vehicle);
                std::process::exit(2);
            };
            // These threads live as long as the process.
            rgf_backend::server::spawn(rgf_backend::server::Config::default());
            rgf_sim::udp::spawn(
                rgf_sim::Sim::new(vehicle),
                rgf_sim::udp::DEFAULT_BIND.into(),
                rgf_sim::udp::DEFAULT_GCS.into(),
                Arc::new(AtomicBool::new(false)),
            );
            "ws://127.0.0.1:9002".to_owned()
        }
    };

    let size = if args.phone { [400.0, 800.0] } else { [1500.0, 950.0] };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size(size).with_title("Robotics Ground Station"),
        ..Default::default()
    };
    let tile_cache = std::env::temp_dir().join("rgf-tile-cache");
    let extras = Extras {
        demo: args.demo,
        screenshot: args.screenshot,
        after: args.after,
        step: 0,
        requested: false,
        script_at: 0,
    };
    eframe::run_native(
        "Robotics Ground Station",
        options,
        Box::new(move |cc| {
            let mut station = StationApp::new(&cc.egui_ctx, backend_url, Some(tile_cache));
            for title in &args.show {
                station.workspace.show_panel(title);
            }
            Ok(Box::new(App { station, extras }))
        }),
    )
}

/// Scripted demo and screenshot capture.
pub struct Extras {
    demo: Option<String>,
    screenshot: Option<PathBuf>,
    after: f64,
    step: u32,
    requested: bool,
    script_at: usize,
}

/// Pointer input for the `edit` demo, at window positions in the default desktop layout:
/// add three waypoints on the map, drag the second, delete the first, then hold the
/// drive stick forward.
const EDIT_SCRIPT: &[(f64, Pointer)] = {
    use Pointer::*;
    &[
        (5.0, Move(600.0, 300.0)),
        (5.1, Left(true)),
        (5.2, Left(false)),
        (5.5, Move(800.0, 300.0)),
        (5.6, Left(true)),
        (5.7, Left(false)),
        (6.0, Move(800.0, 500.0)),
        (6.1, Left(true)),
        (6.2, Left(false)),
        (6.5, Move(800.0, 300.0)),
        (6.6, Left(true)),
        (6.8, Move(850.0, 275.0)),
        (7.0, Move(900.0, 250.0)),
        (7.2, Left(false)),
        (7.5, Move(600.0, 300.0)),
        (7.6, Right(true)),
        (7.7, Right(false)),
        (8.0, Move(83.0, 187.0)),
        (8.1, Left(true)),
        (8.3, Move(83.0, 150.0)),
    ]
};

#[derive(Clone, Copy)]
enum Pointer {
    Move(f32, f32),
    Left(bool),
    Right(bool),
}

impl Extras {
    pub fn before_frame(&mut self, ctx: &egui::Context, app: &mut StationApp) {
        if let Some(demo) = self.demo.clone() {
            self.run_demo(&demo, app);
        }
        let Some(path) = &self.screenshot else { return };
        let now = ctx.input(|i| i.time);
        if now >= self.after && !self.requested {
            self.requested = true;
            let path = path.clone();
            ctx.request_screenshot(move |shot| {
                let [w, h] = shot.size;
                let saved =
                    image::save_buffer(&path, shot.as_raw(), w as u32, h as u32, image::ExtendedColorType::Rgba8);
                match &saved {
                    Ok(()) => println!("saved {}", path.display()),
                    Err(err) => eprintln!("could not save {}: {err}", path.display()),
                }
                std::process::exit(if saved.is_ok() { 0 } else { 1 });
            });
        }
    }

    /// Feed the `edit` demo's pointer script into egui, one event per frame.
    pub fn inject_input(&mut self, raw: &mut egui::RawInput) {
        if self.demo.as_deref() != Some("edit") {
            return;
        }
        let now = raw.time.unwrap_or(0.0);
        let Some(&(at, action)) = EDIT_SCRIPT.get(self.script_at) else { return };
        if now < at {
            return;
        }
        self.script_at += 1;
        let pos = EDIT_SCRIPT[..self.script_at]
            .iter()
            .rev()
            .find_map(|(_, a)| match a {
                Pointer::Move(x, y) => Some(egui::pos2(*x, *y)),
                _ => None,
            })
            .unwrap_or_default();
        let button =
            |button, pressed| egui::Event::PointerButton { pos, button, pressed, modifiers: Default::default() };
        raw.events.push(match action {
            Pointer::Move(..) => egui::Event::PointerMoved(pos),
            Pointer::Left(pressed) => button(egui::PointerButton::Primary, pressed),
            Pointer::Right(pressed) => button(egui::PointerButton::Secondary, pressed),
        });
    }

    /// Each demo is a short list of steps; a step runs once its condition holds.
    fn run_demo(&mut self, demo: &str, app: &mut StationApp) {
        let station = &mut app.station;
        let now = station.now;
        let confirmed = |station: &rgf_ui::Station, l: &str| station.client.state(l) == Some(TxState::Confirmed);
        let Some(here) = station.vehicle_pos() else { return };
        let frame = LocalFrame::new(here);
        match (demo, self.step) {
            ("survey", 0) => {
                let corners = [(-18.0, 6.0), (26.0, 4.0), (30.0, 34.0), (-14.0, 38.0)];
                station.plan.polygon = corners.iter().map(|(x, y)| frame.to_latlon(Vec2::new(*x, *y))).collect();
                station.plan.survey.swath_m = 6.0;
                station.plan.survey.angle_deg = 90.0;
                station.plan.waypoints = station.plan.survey_path();
                station.plan.tool = MapTool::Pan;
                let waypoints = station.plan.waypoints.clone();
                station.client.upload_mission(&waypoints, 0.0, now);
                station.plan.uploaded = waypoints;
                self.step = 1;
            }
            ("survey", 1) if confirmed(station, label::MISSION) => {
                station.client.mission_command(MissionAction::MISSION_START, now);
                self.step = 2;
            }
            ("edit", 0) => {
                station.client.set_mode(Mode::MANUAL, now);
                station.plan.tool = MapTool::Waypoints;
                self.step = 1;
            }
            ("edit", 1) if now > 10.0 => {
                // Report what the scripted clicks produced, in metres east and north of the vehicle.
                let points: Vec<String> = station
                    .plan
                    .waypoints
                    .iter()
                    .map(|p| {
                        let v = frame.to_local(*p);
                        format!("({:.1}, {:.1})", v.x, v.y)
                    })
                    .collect();
                println!("waypoints: {}", points.join(" "));
                println!("drive input: throttle {:.2} steer {:.2}", station.drive_input.0, station.drive_input.1);
                println!("speed: {:.2} m/s", station.client.pose.as_ref().map_or(0.0, |p| p.speed_mps));
                self.step = 2;
            }
            ("drive" | "linkloss", 0) => {
                station.client.set_mode(Mode::MANUAL, now);
                self.step = 1;
            }
            ("drive" | "linkloss", 1) if confirmed(station, label::MODE) => {
                station.drive_input = (0.7, 0.45);
                if demo == "linkloss" && now > 6.0 {
                    station.client.send_tracked(label::SIM_FAULT, now, |seq| SimLinkFault {
                        seq,
                        loss_pct: 0,
                        cut: true,
                    });
                    self.step = 2;
                }
            }
            ("linkloss", 2) if confirmed(station, label::SIM_FAULT) => {
                // With the link cut, neither of these can be confirmed.
                station.client.set_param(0, 0.5, now);
                station.client.set_mode(Mode::HOLD, now);
                self.step = 3;
            }
            _ => {}
        }
    }
}
