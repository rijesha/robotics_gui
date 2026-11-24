//! Ground station application.
//!
//! On the desktop it starts the backend and a simulated vehicle in the same process, so
//! `cargo run -p station` is a complete system. In the browser (and so on phones) it is
//! the UI alone, connecting to a backend running elsewhere.

use rgf_ui::StationApp;

struct App {
    station: StationApp,
    #[cfg(not(target_arch = "wasm32"))]
    extras: native::Extras,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        self.station.ui(ui);
        #[cfg(not(target_arch = "wasm32"))]
        self.extras.after_frame(ui.ctx());
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn raw_input_hook(&mut self, _ctx: &eframe::egui::Context, raw_input: &mut eframe::egui::RawInput) {
        self.extras.inject_input(raw_input, &mut self.station);
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod demo;
#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
mod record;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    native::run()
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;
    use eframe::web_sys;

    wasm_bindgen_futures::spawn_local(async {
        let window = web_sys::window().expect("no window");
        let document = window.document().expect("no document");
        let canvas = document
            .get_element_by_id("station_canvas")
            .expect("index.html has no #station_canvas")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("#station_canvas is not a canvas");

        // The backend is assumed to run on the machine that served this page, unless the
        // address bar says otherwise with ?backend=ws://host:port. ?theme=light starts light,
        // ?layout=map starts in the map layout, ?design=horizon (or workbench, fleet,
        // daylight) in that design.
        let location = window.location();
        let query = location.search().unwrap_or_default();
        let param = |key: &str| {
            query
                .trim_start_matches('?')
                .split('&')
                .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('=').map(str::to_owned))
        };
        let host = location.hostname().unwrap_or_else(|_| "127.0.0.1".into());
        let backend = param("backend").unwrap_or_else(|| format!("ws://{host}:9002"));
        let dark = param("theme").as_deref() != Some("light");
        let map_layout = param("layout").as_deref() == Some("map");
        let design = param("design").and_then(|name| rgf_ui::design::find(&name));
        let theme_given = param("theme").is_some();

        let started = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(move |cc| {
                    let mut station = StationApp::new(&cc.egui_ctx, backend, None, dark);
                    if let Some(index) = design {
                        station.set_design(&cc.egui_ctx, index, theme_given.then_some(dark));
                    }
                    if map_layout {
                        station.workspace.layout = rgf_ui::Layout::Map;
                    }
                    Ok(Box::new(App { station }))
                }),
            )
            .await;
        if let Err(err) = started {
            web_sys::console::error_1(&err);
        }
    });
}
