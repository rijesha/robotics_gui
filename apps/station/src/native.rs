//! Desktop entry point: argument parsing, the embedded backend and simulator, scripted
//! demos and recording.

use crate::demo::{self, Demo};
use crate::record::Recorder;
use crate::App;
use eframe::egui;
use rgf_ui::{Layout, StationApp};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

const USAGE: &str = "\
usage: station [options]
  --vehicle NAMES     simulated vehicles, comma separated, from rover6 (default), tractor,
                      skid, boat; e.g. rover6,tractor,skid
  --connect URL       use a backend that is already running (ws://host:9002) instead of
                      starting the built-in backend and simulator
  --theme NAME        dark (default) or light
  --design NAME       classic (default), horizon, workbench, fleet, daylight; each brings its
                      own colours, layout and theme, which --layout and --theme override
  --layout NAME       dock, map (everything floats over the map), focus (map beside one
                      tool) or fleet (a card per vehicle beside the map)
  --phone             open at phone size to try the mobile layout
  --size WxH          window size in points (default 1500x950)
  --show TITLES       panels to bring to the front, comma separated (e.g. Lidar,Power)
  --demo NAME         run a scripted walkthrough: survey, drive, linkloss, themes,
                      phone, fleet, cruise
  --record PATH.gif   record the window to a GIF until the demo ends, then exit
  --fps N             frames per second for --record (default 8)
  --screenshot PATH   save a PNG of the window after --after seconds, then exit
  --after SECONDS     delay for --screenshot (default 12)";

struct Args {
    vehicle: String,
    connect: Option<String>,
    dark: Option<bool>,
    design: usize,
    layout: Option<Layout>,
    size: [f32; 2],
    show: Vec<String>,
    demo: Option<String>,
    record: Option<PathBuf>,
    fps: f64,
    screenshot: Option<PathBuf>,
    after: f64,
}

fn parse_args() -> Args {
    let mut args = Args {
        vehicle: "rover6".into(),
        connect: None,
        dark: None,
        design: 0,
        layout: None,
        size: [1500.0, 950.0],
        show: Vec::new(),
        demo: None,
        record: None,
        fps: 8.0,
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
            "--theme" => {
                args.dark = match value().as_str() {
                    "dark" => Some(true),
                    "light" => Some(false),
                    other => fail(format!("unknown theme '{other}'")),
                }
            }
            "--layout" => {
                args.layout = match value().as_str() {
                    "dock" => Some(Layout::Dock),
                    "map" => Some(Layout::Map),
                    "focus" => Some(Layout::Focus),
                    "fleet" => Some(Layout::Fleet),
                    other => fail(format!("unknown layout '{other}'")),
                }
            }
            "--design" => {
                let name = value();
                args.design = rgf_ui::design::find(&name).unwrap_or_else(|| fail(format!("unknown design '{name}'")));
            }
            "--phone" => args.size = [400.0, 800.0],
            "--size" => {
                let text = value();
                let parsed = text.split_once('x').and_then(|(w, h)| Some([w.parse().ok()?, h.parse().ok()?]));
                args.size = parsed.unwrap_or_else(|| fail(format!("--size wants WIDTHxHEIGHT, got '{text}'")));
            }
            "--show" => args.show = value().split(',').map(str::to_owned).collect(),
            "--demo" => args.demo = Some(value()),
            "--record" => args.record = Some(value().into()),
            "--fps" => args.fps = value().parse().unwrap_or_else(|_| fail("--fps needs a number".into())),
            "--screenshot" => args.screenshot = Some(value().into()),
            "--after" => args.after = value().parse().unwrap_or_else(|_| fail("--after needs a number".into())),
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
    let demo = args.demo.as_deref().map(|name| {
        demo::script(name).map(Demo::new).unwrap_or_else(|| {
            eprintln!("unknown demo '{name}'; available: {}", demo::NAMES);
            std::process::exit(2)
        })
    });
    let backend_url = match &args.connect {
        Some(url) => url.clone(),
        None => {
            let vehicles: Vec<_> = args
                .vehicle
                .split(',')
                .map(|name| {
                    rgf_sim::parse_vehicle(name).unwrap_or_else(|| {
                        eprintln!("unknown vehicle '{name}'\n{USAGE}");
                        std::process::exit(2)
                    })
                })
                .collect();
            // One simulator per vehicle, each on its own port and parked in its own slot.
            // The backend numbers them in this order.
            let addr = |slot: usize| format!("127.0.0.1:{}", 14560 + slot);
            let config = rgf_backend::server::Config {
                vehicle_addrs: (0..vehicles.len()).map(addr).collect(),
                ..Default::default()
            };
            // These threads live as long as the process.
            rgf_backend::server::spawn(config);
            for (slot, vehicle) in vehicles.into_iter().enumerate() {
                rgf_sim::udp::spawn(
                    rgf_sim::Sim::new_at(vehicle, slot as u8),
                    addr(slot),
                    rgf_sim::udp::DEFAULT_GCS.into(),
                    Arc::new(AtomicBool::new(false)),
                );
            }
            "ws://127.0.0.1:9002".to_owned()
        }
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size(args.size).with_title("Robotics Ground Station"),
        ..Default::default()
    };
    let tile_cache = std::env::temp_dir().join("rgf-tile-cache");
    let extras = Extras {
        demo,
        recorder: args.record.map(|path| Recorder::new(path, args.fps)),
        screenshot: args.screenshot,
        after: args.after,
        requested: false,
        finished_at: None,
    };
    eframe::run_native(
        "Robotics Ground Station",
        options,
        Box::new(move |cc| {
            let mut station = StationApp::new(&cc.egui_ctx, backend_url, Some(tile_cache), args.dark.unwrap_or(true));
            if args.design != 0 {
                station.set_design(&cc.egui_ctx, args.design, args.dark);
            }
            if let Some(layout) = args.layout {
                station.workspace.layout = layout;
            }
            for title in &args.show {
                station.workspace.show_panel(title);
            }
            Ok(Box::new(App { station, extras }))
        }),
    )
}

/// Demo playback, recording and screenshot capture.
pub struct Extras {
    demo: Option<Demo>,
    recorder: Option<Recorder>,
    screenshot: Option<PathBuf>,
    after: f64,
    requested: bool,
    finished_at: Option<f64>,
}

impl Extras {
    /// Feed the demo's pointer input into egui.
    pub fn inject_input(&mut self, raw: &mut egui::RawInput, app: &mut StationApp) {
        if let Some(demo) = &mut self.demo {
            demo.tick(raw, app);
        }
    }

    pub fn after_frame(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if let Some(demo) = &self.demo {
            demo.paint(ctx);
        }

        if let Some(recorder) = &mut self.recorder {
            let demo_done = self.demo.as_ref().is_none_or(Demo::finished);
            // Without a demo, --after bounds the recording.
            let stop = if self.demo.is_some() { demo_done } else { now >= self.after };
            if !stop {
                recorder.tick(ctx, now);
            } else if now - *self.finished_at.get_or_insert(now) > 0.6 {
                // The short wait lets the last requested frame arrive.
                match recorder.finish() {
                    Ok(frames) => println!("recorded {frames} frames"),
                    Err(err) => eprintln!("recording failed: {err}"),
                }
                std::process::exit(0);
            }
        }

        let Some(path) = &self.screenshot else { return };
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
}
