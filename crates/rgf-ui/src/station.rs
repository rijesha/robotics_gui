//! Everything the panels share: the link, what the vehicle last said, what the operator
//! is planning, and the histories and maps built up from telemetry.

use crate::link::Link;
use egui::TextureHandle;
use rgf_core::geo::body_to_local;
use rgf_core::kinematics::{predict_path, PathInput, PredictedPath};
use rgf_core::lidar;
use rgf_core::msgs::common::Mode;
use rgf_core::msgs::control::{DriveCommand, MotorCommand};
use rgf_core::survey::{self, SurveyParams};
use rgf_core::terrain::TerrainGrid;
use rgf_core::{Client, Decoder, LatLon, LocalFrame, Message, Vec2};
use std::collections::{BTreeMap, HashMap, VecDeque};

const TRAIL_POINTS: usize = 2000;
const HISTORY_POINTS: usize = 600;
const TERRAIN_CELL_M: f64 = 0.5;
const TERRAIN_MAX_CELLS: usize = 120_000;
const STREAM_PERIOD_S: f64 = 0.1;
/// Height spread within one terrain cell that marks it as an obstacle.
pub const OBSTACLE_M: f32 = 0.5;
/// How far ahead the expected path is drawn.
pub const PATH_HORIZON_S: f64 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapTool {
    Pan,
    Waypoints,
    Polygon,
}

/// The mission being edited on the map, before it is uploaded.
pub struct MissionPlan {
    pub tool: MapTool,
    pub waypoints: Vec<LatLon>,
    pub polygon: Vec<LatLon>,
    pub survey: SurveyParams,
    pub speed_mps: f32,
    /// What the vehicle was last sent, shown while it runs.
    pub uploaded: Vec<LatLon>,
}

impl Default for MissionPlan {
    fn default() -> Self {
        Self {
            tool: MapTool::Pan,
            waypoints: Vec::new(),
            polygon: Vec::new(),
            survey: SurveyParams::default(),
            speed_mps: 0.0,
            uploaded: Vec::new(),
        }
    }
}

impl MissionPlan {
    /// Survey path over the polygon with the current settings.
    pub fn survey_path(&self) -> Vec<LatLon> {
        let Some(first) = self.polygon.first() else { return Vec::new() };
        let frame = LocalFrame::new(*first);
        let local: Vec<Vec2> = self.polygon.iter().map(|p| frame.to_local(*p)).collect();
        survey::plan(&local, &self.survey).into_iter().map(|p| frame.to_latlon(p)).collect()
    }
}

/// Per-actuator history for the motors panel.
#[derive(Default, Clone)]
pub struct MotorHistory {
    pub current_a: VecDeque<f32>,
}

pub struct Station {
    pub client: Client,
    pub link: Link,
    decoder: Decoder,
    /// Seconds since the app started.
    pub now: f64,

    /// Local frame for map overlays, anchored where the vehicle was first seen.
    pub frame: Option<LocalFrame>,
    pub trail: VecDeque<LatLon>,
    pub terrain: TerrainGrid,
    /// Latest lidar scan as body-frame points.
    pub lidar_points: Vec<lidar::Point>,
    /// For each of `lidar_points`, whether it lies on something standing above the ground.
    pub lidar_obstacle: Vec<bool>,
    /// (seconds, watts, volts)
    pub power_history: VecDeque<[f64; 3]>,
    pub motor_history: Vec<MotorHistory>,

    pub plan: MissionPlan,
    /// Operator drive input, -1..1 each: (throttle, steer).
    pub drive_input: (f32, f32),
    /// Sliders of the motor test, one per actuator.
    pub motor_test: Vec<f32>,
    /// Settings edited but not yet sent.
    pub param_edits: BTreeMap<u16, f32>,

    pub video_texture: Option<TextureHandle>,

    /// Screen rectangles of named controls, refreshed every frame. Guided tours and
    /// scripted demos use these to find what to point at.
    pub anchors: HashMap<String, egui::Rect>,
    /// Where the map currently draws the local frame's origin, and its pixels per metre.
    pub map_view: Option<(egui::Pos2, f32)>,

    seen_epoch: u32,
    seen_lidar: u32,
    seen_video: u32,
    next_stream: f64,
    next_param_request: f64,
}

impl Station {
    pub fn new(url: impl Into<String>) -> Self {
        // Start numbering somewhere random so two UIs on one backend rarely share numbers.
        let first_seq = (run_seed() % 60_000) as u16 + 1;
        Self {
            client: Client::new(first_seq),
            link: Link::new(url),
            decoder: Decoder::new(),
            now: 0.0,
            frame: None,
            trail: VecDeque::new(),
            terrain: TerrainGrid::new(TERRAIN_CELL_M),
            lidar_points: Vec::new(),
            lidar_obstacle: Vec::new(),
            power_history: VecDeque::new(),
            motor_history: Vec::new(),
            plan: MissionPlan::default(),
            drive_input: (0.0, 0.0),
            motor_test: Vec::new(),
            param_edits: BTreeMap::new(),
            video_texture: None,
            anchors: HashMap::new(),
            map_view: None,
            seen_epoch: 0,
            seen_lidar: 0,
            seen_video: 0,
            next_stream: 0.0,
            next_param_request: 0.0,
        }
    }

    /// Record where a named control is on screen. Controls scrolled or clipped out of
    /// view are left out, so an anchor that exists can be pointed at.
    pub fn anchor(&mut self, name: impl Into<String>, response: &egui::Response) {
        if response.interact_rect.contains(response.rect.center()) {
            self.anchors.insert(name.into(), response.rect);
        }
    }

    pub fn vehicle_pos(&self) -> Option<LatLon> {
        self.client.pose.as_ref().map(|p| LatLon::new(p.lat, p.lon))
    }

    /// Where the vehicle is heading given its speed, turn rate and steering.
    pub fn expected_path(&self) -> Option<PredictedPath> {
        let (pose, info) = (self.client.pose.as_ref()?, self.client.info.as_ref()?);
        let input = PathInput {
            model: info.model,
            wheelbase_m: info.wheelbase_m as f64,
            speed_mps: pose.speed_mps as f64,
            yaw_rate_rps: pose.yaw_rate_rps as f64,
            steer_rad: pose.steer_rad as f64,
        };
        Some(predict_path(&input, PATH_HORIZON_S))
    }

    /// Run once per frame before drawing: receive, update histories, send.
    pub fn update(&mut self, ctx: &egui::Context) {
        self.now = ctx.input(|i| i.time);
        let now = self.now;
        // Only controls laid out this frame count as present.
        self.anchors.clear();

        for bytes in self.link.poll(now, ctx) {
            for msg in self.decoder.feed(&bytes) {
                self.client.handle(&msg, now);
                self.observe(&msg);
            }
        }
        self.client.poll(now);

        if self.client.vehicle_epoch != self.seen_epoch {
            self.seen_epoch = self.client.vehicle_epoch;
            self.reset_vehicle_views();
        }
        if self.client.lidar_count != self.seen_lidar {
            self.seen_lidar = self.client.lidar_count;
            self.ingest_lidar();
        }
        if self.client.video_count != self.seen_video {
            self.seen_video = self.client.video_count;
            self.decode_video(ctx);
        }
        let actuators = self.client.actuators.len();
        self.motor_test.resize(actuators, 0.0);
        self.motor_history.resize(actuators, MotorHistory::default());

        // Settings are fetched as soon as there is a vehicle to ask, and again until they all arrive.
        let incomplete = self.client.params.is_empty() || self.client.params.len() < self.client.param_count as usize;
        if self.link.take_opened() {
            self.next_param_request = now;
        }
        if incomplete && self.client.vehicle_alive(now) && now >= self.next_param_request {
            self.next_param_request = now + 4.0;
            self.client.request_params(now);
        }

        if now >= self.next_stream {
            self.next_stream = now + STREAM_PERIOD_S;
            self.stream_commands();
        }

        for frame in self.client.drain() {
            self.link.send(frame);
        }
        self.client.tx.prune();
    }

    /// Streamed commands go out continuously while their mode is active; the vehicle
    /// stops on its own if they stop arriving.
    fn stream_commands(&mut self) {
        match self.client.mode() {
            Some(Mode::MANUAL) => {
                let (throttle, steer) = self.drive_input;
                self.client.send(DriveCommand { throttle, steer });
            }
            Some(Mode::MOTOR_TEST) => {
                for (index, value) in self.motor_test.clone().into_iter().enumerate() {
                    self.client.send(MotorCommand { index: index as u8, value });
                }
            }
            _ => {
                // Leaving motor test must not leave a slider armed for next time.
                self.motor_test.iter_mut().for_each(|v| *v = 0.0);
            }
        }
    }

    fn observe(&mut self, msg: &Message) {
        match msg {
            Message::Pose(pose) => {
                let here = LatLon::new(pose.lat, pose.lon);
                self.frame.get_or_insert(LocalFrame::new(here));
                let moved = self.trail.back().is_none_or(|last| LocalFrame::new(*last).to_local(here).length() > 0.25);
                if moved {
                    self.trail.push_back(here);
                    if self.trail.len() > TRAIL_POINTS {
                        self.trail.pop_front();
                    }
                }
            }
            Message::PowerStatus(power) => {
                self.power_history.push_back([self.now, power.power_w as f64, power.voltage_v as f64]);
                if self.power_history.len() > HISTORY_POINTS {
                    self.power_history.pop_front();
                }
            }
            Message::MotorStatus(motor) => {
                if let Some(history) = self.motor_history.get_mut(motor.index as usize) {
                    history.current_a.push_back(motor.current_a);
                    if history.current_a.len() > 150 {
                        history.current_a.pop_front();
                    }
                }
            }
            _ => {}
        }
    }

    fn reset_vehicle_views(&mut self) {
        self.frame = None;
        self.trail.clear();
        self.terrain.clear();
        self.lidar_points.clear();
        self.lidar_obstacle.clear();
        self.power_history.clear();
        self.motor_history.clear();
        self.motor_test.clear();
        self.param_edits.clear();
        self.plan = MissionPlan { survey: self.plan.survey, ..Default::default() };
        self.video_texture = None;
        self.next_param_request = self.now + 0.5;
    }

    /// Fold the newest scan into the terrain map at the vehicle's reported pose.
    fn ingest_lidar(&mut self) {
        let (Some(scan), Some(pose), Some(frame)) = (&self.client.lidar, &self.client.pose, &self.frame) else {
            return;
        };
        self.lidar_points = lidar::body_points(scan);
        let origin = frame.to_local(LatLon::new(pose.lat, pose.lon));
        if self.terrain.cells.len() > TERRAIN_MAX_CELLS {
            self.terrain.clear();
        }
        let heading = pose.heading_rad as f64;
        let place = |p: &lidar::Point| body_to_local(origin, heading, p.forward as f64, p.right as f64);
        for p in &self.lidar_points {
            self.terrain.insert(place(p), pose.alt_m + p.up);
        }
        // A return is an obstacle if its cell holds returns at very different heights and
        // this one is not the ground at the foot of it.
        self.lidar_obstacle = self
            .lidar_points
            .iter()
            .map(|p| {
                self.terrain
                    .cell_at(place(p))
                    .is_some_and(|c| c.is_obstacle(OBSTACLE_M) && pose.alt_m + p.up > c.min_h + OBSTACLE_M / 2.0)
            })
            .collect();
    }

    fn decode_video(&mut self, ctx: &egui::Context) {
        let Some(video) = &self.client.video else { return };
        let Ok(decoded) = image::load_from_memory_with_format(&video.jpeg, image::ImageFormat::Jpeg) else {
            return;
        };
        let rgb = decoded.to_rgb8();
        let size = [rgb.width() as usize, rgb.height() as usize];
        let image = egui::ColorImage::from_rgb(size, rgb.as_raw());
        match &mut self.video_texture {
            Some(texture) => texture.set(image, egui::TextureOptions::LINEAR),
            None => self.video_texture = Some(ctx.load_texture("video", image, egui::TextureOptions::LINEAR)),
        }
    }
}

/// A number that differs between runs and between browser tabs.
#[cfg(not(target_arch = "wasm32"))]
fn run_seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64)
}

#[cfg(target_arch = "wasm32")]
fn run_seed() -> u64 {
    (js_sys::Math::random() * 1e9) as u64
}
