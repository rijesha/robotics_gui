//! Ground-station client state: what the vehicle last reported, and the delivery status
//! of everything sent to it. No IO and no UI, so it runs headless in tests and inside
//! any front end.

use crate::codec::str_from;
use crate::geo::LatLon;
use crate::msgs::common::{AckResult, ActuatorInfo, Heartbeat, Mode, VehicleInfo, VehicleType};
use crate::msgs::control::{EStop, SetMode};
use crate::msgs::mission::{MissionAction, MissionCommand, MissionCount, MissionItem};
use crate::msgs::params::{ParamRequestList, ParamSet};
use crate::msgs::rtk::BaseStatus;
use crate::msgs::sensors::LidarScan;
use crate::msgs::telemetry::{MotorStatus, NavStatus, Pose, PowerStatus};
use crate::reliable::{Action, Failure, Policy, TxState, TxTracker};
use crate::Message;
use std::collections::{BTreeMap, HashMap};

/// The backend retries towards the vehicle; the client waits out those retries once.
pub const CLIENT_POLICY: Policy = Policy { timeout_s: 3.0, max_attempts: 1 };
/// A vehicle that has not sent a heartbeat for this long is treated as disconnected.
pub const HEARTBEAT_TIMEOUT_S: f64 = 2.5;

#[derive(Debug, Clone)]
pub struct ParamEntry {
    pub name: String,
    /// Last value the vehicle reported.
    pub value: f32,
    pub min: f32,
    pub max: f32,
}

#[derive(Debug, Clone)]
pub struct VideoImage {
    pub jpeg: Vec<u8>,
    pub hfov_rad: f32,
    /// Lens height above the ground under the vehicle.
    pub mount_height_m: f32,
}

pub mod label {
    pub const MODE: &str = "mode";
    pub const ESTOP: &str = "estop";
    pub const MISSION: &str = "mission";
    pub const MISSION_CMD: &str = "mission_cmd";
    pub const PARAM_LIST: &str = "param_list";
    pub const BASE: &str = "base";
    pub const SIM_VEHICLE: &str = "sim_vehicle";
    pub const SIM_FAULT: &str = "sim_fault";
    pub fn param(id: u16) -> String {
        format!("param:{id}")
    }
}

pub struct Client {
    pub tx: TxTracker,
    outbox: Vec<Vec<u8>>,
    pending_params: HashMap<u16, (u16, f32)>,

    pub heartbeat: Option<Heartbeat>,
    pub last_heartbeat: f64,
    pub info: Option<VehicleInfo>,
    pub actuators: Vec<Option<ActuatorInfo>>,
    pub motors: Vec<Option<MotorStatus>>,
    pub pose: Option<Pose>,
    pub power: Option<PowerStatus>,
    pub nav: Option<NavStatus>,
    pub params: BTreeMap<u16, ParamEntry>,
    pub param_count: u16,
    pub base: Option<BaseStatus>,

    pub lidar: Option<Box<LidarScan>>,
    /// Latest complete JPEG frame with the camera's horizontal field of view and height.
    pub video: Option<VideoImage>,
    video_partial: (u16, Vec<u8>, u8),

    /// Bumped whenever a different vehicle shows up, so views can reset.
    pub vehicle_epoch: u32,
    /// Bumped per lidar scan and per completed video frame.
    pub lidar_count: u32,
    pub video_count: u32,
}

impl Client {
    pub fn new(first_seq: u16) -> Self {
        Self {
            tx: TxTracker::with_first_seq(CLIENT_POLICY, first_seq),
            outbox: Vec::new(),
            pending_params: HashMap::new(),
            heartbeat: None,
            last_heartbeat: f64::NEG_INFINITY,
            info: None,
            actuators: Vec::new(),
            motors: Vec::new(),
            pose: None,
            power: None,
            nav: None,
            params: BTreeMap::new(),
            param_count: 0,
            base: None,
            lidar: None,
            video: None,
            video_partial: (0, Vec::new(), 0),
            vehicle_epoch: 0,
            lidar_count: 0,
            video_count: 0,
        }
    }

    /// Frames to put on the link.
    pub fn drain(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.outbox)
    }

    pub fn vehicle_alive(&self, now: f64) -> bool {
        now - self.last_heartbeat < HEARTBEAT_TIMEOUT_S
    }

    pub fn mode(&self) -> Option<Mode> {
        self.heartbeat.as_ref().map(|h| h.mode)
    }

    pub fn vehicle_type(&self) -> Option<VehicleType> {
        self.info.as_ref().map(|i| i.vehicle_type)
    }

    // ---------------------------------------------------------------- sending

    /// Send something that needs no confirmation (streamed commands).
    pub fn send(&mut self, msg: impl Into<Message>) {
        self.outbox.push(msg.into().encode());
    }

    /// Send one message of a batch under `label`; call `tx.begin(label)` first.
    pub fn track<M: Into<Message>>(&mut self, label: &str, now: f64, build: impl FnOnce(u16) -> M) -> u16 {
        let seq = self.tx.next_seq();
        let msg: Message = build(seq).into();
        let frame = msg.encode();
        self.tx.track(label, seq, msg.msg_id(), frame.clone(), now);
        self.outbox.push(frame);
        seq
    }

    /// Send a message whose delivery is tracked under `label`, replacing the label's
    /// previous outcome.
    pub fn send_tracked<M: Into<Message>>(&mut self, label: &str, now: f64, build: impl FnOnce(u16) -> M) -> u16 {
        self.tx.begin(label);
        self.track(label, now, build)
    }

    pub fn set_mode(&mut self, mode: Mode, now: f64) {
        self.send_tracked(label::MODE, now, |seq| SetMode { seq, mode });
    }

    pub fn set_estop(&mut self, engage: bool, now: f64) {
        self.send_tracked(label::ESTOP, now, |seq| EStop { seq, engage });
    }

    pub fn request_params(&mut self, now: f64) {
        self.send_tracked(label::PARAM_LIST, now, |seq| ParamRequestList { seq });
    }

    /// Ask the vehicle to change a setting. It counts as delivered only when the vehicle
    /// reads back the same value.
    pub fn set_param(&mut self, id: u16, value: f32, now: f64) {
        let seq = self.send_tracked(&label::param(id), now, |seq| ParamSet { seq, id, value });
        self.pending_params.insert(seq, (id, value));
    }

    pub fn upload_mission(&mut self, waypoints: &[LatLon], speed_mps: f32, now: f64) {
        self.tx.begin(label::MISSION);
        self.track(label::MISSION, now, |seq| MissionCount { seq, count: waypoints.len() as u16 });
        for (index, wp) in waypoints.iter().enumerate() {
            self.track(label::MISSION, now, |seq| MissionItem {
                seq,
                index: index as u16,
                lat: wp.lat,
                lon: wp.lon,
                speed_mps,
            });
        }
    }

    pub fn mission_command(&mut self, action: MissionAction, now: f64) {
        self.send_tracked(label::MISSION_CMD, now, |seq| MissionCommand { seq, action });
    }

    // -------------------------------------------------------------- receiving

    /// Time out anything that has waited too long.
    pub fn poll(&mut self, now: f64) {
        for action in self.tx.poll(now) {
            match action {
                Action::Resend { frame, .. } => self.outbox.push(frame),
                Action::Failed { seq, .. } => {
                    self.pending_params.remove(&seq);
                }
            }
        }
    }

    pub fn handle(&mut self, msg: &Message, now: f64) {
        match msg {
            Message::Heartbeat(m) => {
                self.heartbeat = Some((**m).clone());
                self.last_heartbeat = now;
            }
            Message::VehicleInfo(m) => {
                if self.info.as_ref().is_some_and(|old| old.vehicle_type != m.vehicle_type) {
                    self.reset_vehicle();
                }
                let count = m.actuator_count as usize;
                self.actuators.resize(count, None);
                self.motors.resize(count, None);
                self.info = Some((**m).clone());
            }
            Message::ActuatorInfo(m) => {
                if let Some(slot) = self.actuators.get_mut(m.index as usize) {
                    *slot = Some((**m).clone());
                }
            }
            Message::MotorStatus(m) => {
                if let Some(slot) = self.motors.get_mut(m.index as usize) {
                    *slot = Some((**m).clone());
                }
            }
            Message::Pose(m) => self.pose = Some((**m).clone()),
            Message::PowerStatus(m) => self.power = Some((**m).clone()),
            Message::NavStatus(m) => self.nav = Some((**m).clone()),
            Message::BaseStatus(m) => self.base = Some((**m).clone()),
            Message::LidarScan(m) => {
                self.lidar = Some(m.clone());
                self.lidar_count = self.lidar_count.wrapping_add(1);
            }
            Message::VideoFrame(m) => {
                let (frame_id, data, next) = &mut self.video_partial;
                if m.chunk == 0 {
                    *frame_id = m.frame_id;
                    data.clear();
                    *next = 0;
                }
                // A lost chunk leaves a gap; drop the frame rather than show a torn image.
                if m.frame_id == *frame_id && m.chunk == *next {
                    data.extend_from_slice(&m.data[..(m.data_count as usize).min(m.data.len())]);
                    *next += 1;
                    if *next == m.chunks {
                        self.video = Some(VideoImage {
                            jpeg: std::mem::take(data),
                            hfov_rad: m.hfov_rad,
                            mount_height_m: m.mount_height_m,
                        });
                        self.video_count = self.video_count.wrapping_add(1);
                    }
                }
            }
            Message::CommandAck(m) => {
                let result = match m.result {
                    AckResult::ACCEPTED => Ok(()),
                    AckResult::REJECTED => Err(Failure::Rejected),
                    AckResult::TIMEOUT => Err(Failure::Timeout),
                };
                self.tx.resolve(m.seq, result, now);
                self.pending_params.remove(&m.seq);
            }
            Message::ParamValue(m) => {
                self.param_count = m.count;
                self.params
                    .insert(m.id, ParamEntry { name: str_from(&m.name), value: m.value, min: m.min, max: m.max });
                if let Some((id, wanted)) = self.pending_params.remove(&m.seq) {
                    let matches = id == m.id && (m.value - wanted).abs() <= 1e-5 * wanted.abs().max(1.0);
                    self.tx.resolve(m.seq, if matches { Ok(()) } else { Err(Failure::Mismatch) }, now);
                }
            }
            _ => {}
        }
    }

    fn reset_vehicle(&mut self) {
        self.actuators.clear();
        self.motors.clear();
        self.params.clear();
        self.param_count = 0;
        self.pose = None;
        self.power = None;
        self.nav = None;
        self.lidar = None;
        self.video = None;
        self.vehicle_epoch += 1;
    }

    pub fn state(&self, label: &str) -> Option<TxState> {
        self.tx.state(label)
    }
}
