//! The simulated vehicle: firmware behaviour on top of a vehicle model.

use crate::camera;
use crate::vehicles::{self, Actuator, Body, Spec};
use crate::world::{Rng, World};
use rgf_core::codec::str_to;
use rgf_core::geo::{bearing, heading_vec, wrap_pi};
use rgf_core::msgs::common::{AckResult, ActuatorInfo, CommandAck, Heartbeat, Mode, VehicleInfo, VehicleType};
use rgf_core::msgs::mission::MissionAction;
use rgf_core::msgs::params::ParamValue;
use rgf_core::msgs::sensors::{LidarScan, VideoFrame};
use rgf_core::msgs::telemetry::{FixType, MotorStatus, NavState, NavStatus, Pose, PowerStatus};
use rgf_core::{Decoder, LatLon, LocalFrame, Message, Vec2};

pub const LIDAR_LAYERS: usize = 8;
pub const LIDAR_POINTS_PER_LAYER: usize = 180;
const LIDAR_ELEV_MIN: f64 = -0.35;
const LIDAR_ELEV_STEP: f64 = 0.07;
const LIDAR_MOUNT_M: f64 = 1.2;
const LIDAR_RANGE_M: f64 = 30.0;
/// Streamed drive and motor commands older than this are treated as zero.
const COMMAND_TIMEOUT_S: f64 = 0.5;
/// Largest video payload per frame; keeps datagrams under common UDP limits.
const VIDEO_CHUNK: usize = 8000;

/// Where each kind of vehicle starts.
pub fn home(vehicle_type: VehicleType) -> LatLon {
    match vehicle_type {
        VehicleType::BOAT => LatLon::new(39.0968, -120.0324),
        _ => LatLon::new(41.9000, -93.5500),
    }
}

/// Where the vehicle in each start slot is parked, relative to home. Slots sit in rows
/// far enough apart for the largest vehicle and inside the clearing around home.
pub fn start_slot(slot: u8) -> Vec2 {
    let (column, row) = ((slot % 3) as f64, (slot / 3) as f64);
    // Middle, then right, then left of it.
    let east = [0.0, 4.0, -4.0][column as usize];
    Vec2::new(east, -5.0 * row)
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: &'static str,
    pub value: f32,
    pub min: f32,
    pub max: f32,
}

pub mod param {
    pub const CRUISE_SPEED: usize = 0;
    pub const WP_RADIUS: usize = 1;
    pub const STEER_GAIN: usize = 2;
    pub const TELEM_RATE: usize = 3;
    pub const LIDAR_RATE: usize = 4;
    pub const VIDEO_RATE: usize = 5;
    pub const VIDEO_QUALITY: usize = 6;
    pub const MANUAL_LIMIT: usize = 7;
}

fn default_params(spec: &Spec) -> Vec<Param> {
    let max = spec.max_speed_mps as f32;
    let boat = spec.vehicle_type == VehicleType::BOAT;
    let p = |name, value, min, max| Param { name, value, min, max };
    vec![
        p("CRUISE_SPEED", max * 0.6, 0.1, max),
        p("WP_RADIUS", if boat { 2.5 } else { 1.2 }, 0.3, 10.0),
        p("STEER_GAIN", 1.5, 0.2, 5.0),
        p("TELEM_RATE", 10.0, 1.0, 20.0),
        p("LIDAR_RATE", 5.0, 0.0, 10.0),
        p("VIDEO_RATE", 8.0, 0.0, 15.0),
        p("VIDEO_QUALITY", 60.0, 10.0, 95.0),
        p("MANUAL_LIMIT", 1.0, 0.1, 1.0),
    ]
}

/// Fires at a rate that can change while running.
#[derive(Default)]
struct Timer(f64);

impl Timer {
    fn due(&mut self, dt: f64, rate_hz: f64) -> bool {
        if rate_hz <= 0.0 {
            self.0 = 0.0;
            return false;
        }
        self.0 += dt;
        if self.0 >= 1.0 / rate_hz {
            self.0 = 0.0;
            true
        } else {
            false
        }
    }
}

#[derive(Default)]
struct Timers {
    heartbeat: Timer,
    info: Timer,
    pose: Timer,
    motors: Timer,
    power: Timer,
    nav: Timer,
    lidar: Timer,
    video: Timer,
}

/// Injected link trouble, for showing what failed delivery looks like.
#[derive(Debug, Clone, Copy, Default)]
pub struct LinkFault {
    pub loss_pct: u8,
    pub cut: bool,
}

pub struct Sim {
    pub spec: Spec,
    pub world: World,
    pub frame: LocalFrame,
    pub body: Body,
    slot: u8,
    pub acts: Vec<Actuator>,
    pub mode: Mode,
    pub estop: bool,
    pub params: Vec<Param>,
    pub fault: LinkFault,

    time: f64,
    drive: (f64, f64, f64),
    motor_cmds: Vec<(f64, f64)>,

    mission_upload: Vec<Option<(Vec2, f32)>>,
    mission: Vec<Vec2>,
    mission_speeds: Vec<f32>,
    nav: NavState,
    active_wp: usize,
    leg_start: Vec2,
    xtrack: f64,

    consumed_wh: f64,
    bus_v: f64,
    bus_a: f64,
    power_avg_w: f64,

    last_rtcm: f64,
    corrections_since: Option<f64>,
    gnss_sigma: f64,

    timers: Timers,
    frame_id: u16,
    rng: Rng,
    decoder: Decoder,
    outbox: Vec<Vec<u8>>,
}

impl Sim {
    pub fn new(vehicle_type: VehicleType) -> Self {
        Self::new_at(vehicle_type, 0)
    }

    /// A vehicle parked in the given start slot, so several can share one site.
    pub fn new_at(vehicle_type: VehicleType, slot: u8) -> Self {
        let spec = vehicles::spec(vehicle_type);
        let acts = vec![Actuator { temp_c: 25.0, ..Default::default() }; spec.actuators.len()];
        Self {
            world: if vehicle_type == VehicleType::BOAT { World::water() } else { World::land() },
            frame: LocalFrame::new(home(vehicle_type)),
            body: Body { pos: start_slot(slot), ..Default::default() },
            slot,
            motor_cmds: vec![(0.0, f64::NEG_INFINITY); acts.len()],
            acts,
            mode: Mode::DISARMED,
            estop: false,
            params: default_params(&spec),
            fault: LinkFault::default(),
            time: 0.0,
            drive: (0.0, 0.0, f64::NEG_INFINITY),
            mission_upload: Vec::new(),
            mission: Vec::new(),
            mission_speeds: Vec::new(),
            nav: NavState::NAV_IDLE,
            active_wp: 0,
            leg_start: Vec2::default(),
            xtrack: 0.0,
            consumed_wh: 0.0,
            bus_v: spec.battery_v,
            bus_a: 0.0,
            power_avg_w: 0.0,
            last_rtcm: f64::NEG_INFINITY,
            corrections_since: None,
            gnss_sigma: 0.6,
            timers: Timers::default(),
            frame_id: 0,
            rng: Rng(0xC0FFEE ^ vehicle_type as u64 ^ ((slot as u64) << 16)),
            decoder: Decoder::new(),
            outbox: Vec::new(),
            spec,
        }
    }

    pub fn time(&self) -> f64 {
        self.time
    }

    pub fn nav_state(&self) -> NavState {
        self.nav
    }

    fn param(&self, id: usize) -> f64 {
        self.params[id].value as f64
    }

    // ---------------------------------------------------------------- link

    /// Frames ready to transmit, one per datagram.
    pub fn drain(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.outbox)
    }

    fn link_drops(&mut self) -> bool {
        self.fault.cut || self.rng.next_u32() % 100 < self.fault.loss_pct as u32
    }

    fn emit(&mut self, msg: impl Into<Message>) {
        if !self.link_drops() {
            self.outbox.push(msg.into().encode());
        }
    }

    /// Simulator controls travel outside the simulated radio link, so a cut link can be
    /// restored from the UI.
    fn emit_out_of_band(&mut self, msg: impl Into<Message>) {
        self.outbox.push(msg.into().encode());
    }

    fn ack(&mut self, seq: u16, msg_id: u16, accepted: bool) {
        let result = if accepted { AckResult::ACCEPTED } else { AckResult::REJECTED };
        self.emit(CommandAck { seq, msg_id, result });
    }

    /// Bytes received from the ground station.
    pub fn receive(&mut self, bytes: &[u8]) {
        for msg in self.decoder.feed(bytes) {
            let msg_id = msg.msg_id();
            match msg {
                Message::SimLinkFault(m) => {
                    self.fault = LinkFault { loss_pct: m.loss_pct.min(100), cut: m.cut };
                    self.emit_out_of_band(CommandAck { seq: m.seq, msg_id, result: AckResult::ACCEPTED });
                }
                Message::SimSelectVehicle(m) => {
                    let fault = self.fault;
                    let mut outbox = std::mem::take(&mut self.outbox);
                    *self = Sim::new_at(m.vehicle_type, self.slot);
                    self.fault = fault;
                    outbox.push(Message::from(CommandAck { seq: m.seq, msg_id, result: AckResult::ACCEPTED }).encode());
                    self.outbox = outbox;
                    // Announce the new vehicle right away.
                    self.timers.info.0 = f64::INFINITY;
                    self.timers.heartbeat.0 = f64::INFINITY;
                }
                other => {
                    if !self.link_drops() {
                        self.handle(other, msg_id);
                    }
                }
            }
        }
    }

    fn handle(&mut self, msg: Message, msg_id: u16) {
        match msg {
            Message::SetMode(m) => {
                let ok = self.set_mode(m.mode);
                self.ack(m.seq, msg_id, ok);
            }
            Message::EStop(m) => {
                self.estop = m.engage;
                if m.engage {
                    self.mode = Mode::DISARMED;
                    if self.nav == NavState::NAV_RUNNING {
                        self.nav = NavState::NAV_PAUSED;
                    }
                }
                self.ack(m.seq, msg_id, true);
            }
            Message::DriveCommand(m) => self.drive = (m.throttle as f64, m.steer as f64, self.time),
            Message::MotorCommand(m) => {
                if let Some(slot) = self.motor_cmds.get_mut(m.index as usize) {
                    *slot = (m.value as f64, self.time);
                }
            }
            Message::ParamRequestList(m) => {
                self.ack(m.seq, msg_id, true);
                for id in 0..self.params.len() {
                    self.emit_param(id, 0);
                }
            }
            Message::ParamSet(m) => {
                // Confirmation is the value read back, so the sender can see what was applied.
                if let Some(p) = self.params.get_mut(m.id as usize) {
                    if m.value.is_finite() {
                        p.value = m.value.clamp(p.min, p.max);
                    }
                    self.emit_param(m.id as usize, m.seq);
                } else {
                    self.ack(m.seq, msg_id, false);
                }
            }
            Message::MissionCount(m) => {
                let ok = self.nav != NavState::NAV_RUNNING && m.count > 0;
                if ok {
                    self.mission_upload = vec![None; m.count as usize];
                }
                self.ack(m.seq, msg_id, ok);
            }
            Message::MissionItem(m) => {
                let pos = self.frame.to_local(LatLon::new(m.lat, m.lon));
                let ok = match self.mission_upload.get_mut(m.index as usize) {
                    Some(slot) => {
                        *slot = Some((pos, m.speed_mps));
                        true
                    }
                    None => false,
                };
                self.ack(m.seq, msg_id, ok);
            }
            Message::MissionCommand(m) => {
                let ok = match m.action {
                    MissionAction::MISSION_START => self.start_mission(),
                    MissionAction::MISSION_PAUSE => {
                        let running = self.nav == NavState::NAV_RUNNING;
                        if running {
                            self.nav = NavState::NAV_PAUSED;
                            self.mode = Mode::HOLD;
                        }
                        running
                    }
                    MissionAction::MISSION_CLEAR => {
                        self.mission.clear();
                        self.mission_upload.clear();
                        self.nav = NavState::NAV_IDLE;
                        self.active_wp = 0;
                        if self.mode == Mode::AUTO {
                            self.mode = Mode::HOLD;
                        }
                        true
                    }
                };
                self.ack(m.seq, msg_id, ok);
            }
            Message::RtcmData(_) => self.last_rtcm = self.time,
            _ => {}
        }
    }

    fn set_mode(&mut self, mode: Mode) -> bool {
        if self.estop && mode != Mode::DISARMED {
            return false;
        }
        if mode == Mode::AUTO {
            return self.start_mission();
        }
        if self.nav == NavState::NAV_RUNNING {
            self.nav = NavState::NAV_PAUSED;
        }
        self.mode = mode;
        true
    }

    /// Start the uploaded mission, or resume a paused one.
    fn start_mission(&mut self) -> bool {
        if self.estop {
            return false;
        }
        if self.nav == NavState::NAV_PAUSED && !self.mission.is_empty() && self.upload_matches_mission() {
            self.nav = NavState::NAV_RUNNING;
            self.mode = Mode::AUTO;
            return true;
        }
        if self.mission_upload.is_empty() || self.mission_upload.iter().any(Option::is_none) {
            return false;
        }
        self.mission = self.mission_upload.iter().flatten().map(|(p, _)| *p).collect();
        self.mission_speeds = self.mission_upload.iter().flatten().map(|(_, s)| *s).collect();
        self.active_wp = 0;
        self.leg_start = self.estimated_pos();
        self.nav = NavState::NAV_RUNNING;
        self.mode = Mode::AUTO;
        true
    }

    fn upload_matches_mission(&self) -> bool {
        self.mission_upload.len() == self.mission.len()
            && self.mission_upload.iter().zip(&self.mission).all(|(u, m)| u.map(|(p, _)| p) == Some(*m))
    }

    fn emit_param(&mut self, id: usize, seq: u16) {
        let p = &self.params[id];
        let msg = ParamValue {
            id: id as u16,
            name: str_to(p.name),
            value: p.value,
            min: p.min,
            max: p.max,
            count: self.params.len() as u16,
            seq,
        };
        self.emit(msg);
    }

    // ------------------------------------------------------------ dynamics

    /// GNSS error at the current time: a slowly wandering bias plus a little jitter.
    fn gnss_error(&mut self) -> Vec2 {
        let t = self.time;
        let wander = Vec2::new((t / 7.0 + 1.0).sin() + 0.5 * (t / 2.3).sin(), (t / 11.0).cos() + 0.5 * (t / 3.1).cos());
        let jitter = Vec2::new(self.rng.signed(), self.rng.signed()) * 0.15;
        (wander + jitter) * self.gnss_sigma
    }

    /// Where the vehicle believes it is. Navigation uses this, so RTK visibly tightens
    /// path tracking.
    fn estimated_pos(&self) -> Vec2 {
        let t = self.time;
        let wander = Vec2::new((t / 7.0 + 1.0).sin() + 0.5 * (t / 2.3).sin(), (t / 11.0).cos() + 0.5 * (t / 3.1).cos());
        self.body.pos + wander * self.gnss_sigma
    }

    fn fix(&self) -> FixType {
        match self.corrections_since {
            Some(since) if self.time - since > 8.0 => FixType::RTK_FIXED,
            Some(since) if self.time - since > 2.0 => FixType::RTK_FLOAT,
            _ => FixType::FIX_3D,
        }
    }

    /// Throttle and steer for following the mission.
    fn navigate(&mut self) -> (f64, f64) {
        let pos = self.estimated_pos();
        let radius = self.param(param::WP_RADIUS);
        while self.active_wp < self.mission.len() && (self.mission[self.active_wp] - pos).length() < radius {
            self.leg_start = self.mission[self.active_wp];
            self.active_wp += 1;
        }
        if self.active_wp >= self.mission.len() {
            self.nav = NavState::NAV_COMPLETE;
            self.mode = Mode::HOLD;
            return (0.0, 0.0);
        }
        let target_wp = self.mission[self.active_wp];
        let leg = target_wp - self.leg_start;
        let leg_len = leg.length().max(1e-6);
        let along = leg * (1.0 / leg_len);
        let rel = pos - self.leg_start;
        let progress = rel.dot(along).clamp(0.0, leg_len);
        self.xtrack = rel.x * along.y - rel.y * along.x;

        // Aim at a point ahead on the leg, never past the waypoint.
        let lookahead = (self.body.speed.abs() * 1.5).max(self.spec.wheelbase_m * 1.5).max(2.0);
        let aim = self.leg_start + along * (progress + lookahead).min(leg_len);
        let error = wrap_pi(bearing(aim - pos) - self.body.heading);

        let steer = (self.param(param::STEER_GAIN) * error).clamp(-1.0, 1.0);
        // A waypoint may ask for less than the cruise speed, never more.
        let leg_speed = self.mission_speeds[self.active_wp] as f64;
        let speed = if leg_speed > 0.0 {
            leg_speed.min(self.param(param::CRUISE_SPEED))
        } else {
            self.param(param::CRUISE_SPEED)
        };
        let cruise = speed / self.spec.max_speed_mps;
        // Slow down for sharp corrections; vehicles that cannot pivot keep some way on.
        let min_fraction = match self.spec.vehicle_type {
            VehicleType::SKID_STEER => 0.0,
            VehicleType::BOAT => 0.15,
            _ => 0.3,
        };
        let throttle = cruise * (1.0 - (error.abs() / 1.0).min(1.0)).max(min_fraction);
        (throttle, steer)
    }

    pub fn step(&mut self, dt: f64) {
        self.time += dt;
        let fresh = |stamp: f64, now: f64| now - stamp < COMMAND_TIMEOUT_S;

        let commands: Vec<f64> = match self.mode {
            _ if self.estop => vec![0.0; self.acts.len()],
            Mode::MANUAL if fresh(self.drive.2, self.time) => {
                let limit = self.param(param::MANUAL_LIMIT);
                vehicles::mix(&self.spec, self.drive.0 * limit, self.drive.1)
            }
            Mode::MOTOR_TEST => {
                self.motor_cmds.iter().map(|&(v, t)| if fresh(t, self.time) { v } else { 0.0 }).collect()
            }
            Mode::AUTO if self.nav == NavState::NAV_RUNNING => {
                let (throttle, steer) = self.navigate();
                vehicles::mix(&self.spec, throttle, steer)
            }
            _ => vec![0.0; self.acts.len()],
        };
        for (a, c) in self.acts.iter_mut().zip(&commands) {
            a.command = *c;
        }

        let ahead = self.body.pos + heading_vec(self.body.heading) * 0.5;
        let slope = (self.world.ground(ahead) - self.world.ground(self.body.pos)) / 0.5;
        vehicles::step(&self.spec, &mut self.acts, &mut self.body, slope, dt);
        if self.spec.vehicle_type == VehicleType::BOAT {
            // A light current sets the boat east whether or not it is under power.
            self.body.pos.x += 0.04 * dt;
        }

        // Battery: open-circuit voltage sags with charge and with load.
        let soc = (1.0 - self.consumed_wh / self.spec.battery_wh).max(0.0);
        self.bus_a = self.acts.iter().map(|a| a.current_a).sum::<f64>() + 1.0;
        let drives = self.acts.len() as f64;
        let resistance = 0.06 * self.spec.battery_v / (self.spec.stall_current_a * drives);
        self.bus_v = self.spec.battery_v * (0.88 + 0.17 * soc) - self.bus_a * resistance;
        let power = self.bus_v * self.bus_a;
        self.consumed_wh += power * dt / 3600.0;
        self.power_avg_w += (power - self.power_avg_w) * (dt / 20.0).min(1.0);

        // RTK: corrections must keep flowing; accuracy converges while they do.
        if self.time - self.last_rtcm < 3.0 {
            self.corrections_since.get_or_insert(self.time);
        } else {
            self.corrections_since = None;
        }
        let target_sigma = match self.fix() {
            FixType::RTK_FIXED => 0.01,
            FixType::RTK_FLOAT => 0.15,
            _ => 0.6,
        };
        self.gnss_sigma += (target_sigma - self.gnss_sigma) * (dt / 1.5).min(1.0);

        self.telemetry(dt);
    }

    // ----------------------------------------------------------- telemetry

    fn telemetry(&mut self, dt: f64) {
        if self.timers.heartbeat.due(dt, 2.0) {
            self.emit(Heartbeat {
                uptime_ms: (self.time * 1000.0) as u32,
                mode: self.mode,
                estop: self.estop,
                vehicle_type: self.spec.vehicle_type,
            });
        }
        if self.timers.info.due(dt, 0.5) {
            self.emit_info();
        }
        if self.timers.pose.due(dt, self.param(param::TELEM_RATE)) {
            self.emit_pose();
        }
        if self.timers.motors.due(dt, 5.0) {
            for i in 0..self.acts.len() {
                let a = &self.acts[i];
                let is_angle = !matches!(
                    self.spec.actuators[i].kind,
                    rgf_core::msgs::common::ActuatorKind::DRIVE | rgf_core::msgs::common::ActuatorKind::THRUSTER
                );
                let msg = MotorStatus {
                    index: i as u8,
                    command: a.command as f32,
                    rpm: a.rpm as f32,
                    current_a: a.current_a as f32,
                    voltage_v: (self.bus_v * a.command.abs()) as f32,
                    temp_c: a.temp_c as f32,
                    position_rad: if is_angle { a.output as f32 } else { 0.0 },
                    faults: a.faults,
                    torque_nm: a.torque_nm as f32,
                };
                self.emit(msg);
            }
        }
        if self.timers.power.due(dt, 2.0) {
            let remaining_wh = (self.spec.battery_wh - self.consumed_wh).max(0.0);
            let power = self.bus_v * self.bus_a;
            self.emit(PowerStatus {
                voltage_v: self.bus_v as f32,
                current_a: self.bus_a as f32,
                power_w: power as f32,
                consumed_wh: self.consumed_wh as f32,
                soc_pct: (100.0 * remaining_wh / self.spec.battery_wh) as f32,
                time_remaining_s: (remaining_wh / self.power_avg_w.max(1.0) * 3600.0) as f32,
            });
        }
        if self.timers.nav.due(dt, 2.0) {
            let dist = self.mission.get(self.active_wp).map_or(0.0, |wp| (*wp - self.estimated_pos()).length());
            self.emit(NavStatus {
                state: self.nav,
                active_wp: self.active_wp as u16,
                wp_count: self.mission.len() as u16,
                xtrack_m: if self.nav == NavState::NAV_RUNNING { self.xtrack as f32 } else { 0.0 },
                dist_to_wp_m: dist as f32,
            });
        }
        if self.timers.lidar.due(dt, self.param(param::LIDAR_RATE)) {
            self.emit_lidar();
        }
        if self.timers.video.due(dt, self.param(param::VIDEO_RATE)) {
            self.emit_video();
        }
    }

    fn emit_info(&mut self) {
        let s = &self.spec;
        let info = VehicleInfo {
            vehicle_type: s.vehicle_type,
            model: s.model,
            name: str_to(s.name),
            length_m: s.length_m as f32,
            width_m: s.width_m as f32,
            wheelbase_m: s.wheelbase_m as f32,
            track_m: s.track_m as f32,
            max_speed_mps: s.max_speed_mps as f32,
            max_steer_rad: s.max_steer_rad as f32,
            actuator_count: s.actuators.len() as u8,
        };
        let actuators: Vec<ActuatorInfo> = s
            .actuators
            .iter()
            .enumerate()
            .map(|(i, a)| ActuatorInfo {
                index: i as u8,
                kind: a.kind,
                name: str_to(a.name),
                x_m: a.forward_m,
                y_m: a.right_m,
            })
            .collect();
        self.emit(info);
        for a in actuators {
            self.emit(a);
        }
    }

    fn emit_pose(&mut self) {
        let reported = self.body.pos + self.gnss_error();
        let ll = self.frame.to_latlon(reported);
        let pos = self.body.pos;
        let forward = heading_vec(self.body.heading);
        let right = Vec2::new(forward.y, -forward.x);
        let ground = self.world.ground(pos);
        let half_len = self.spec.length_m / 2.0;
        let half_wid = self.spec.width_m / 2.0;
        let pitch = ((self.world.ground(pos + forward * half_len) - self.world.ground(pos - forward * half_len))
            / self.spec.length_m)
            .atan();
        let roll = ((self.world.ground(pos - right * half_wid) - self.world.ground(pos + right * half_wid))
            / self.spec.width_m)
            .atan();
        let fix = self.fix();
        self.emit(Pose {
            time_ms: (self.time * 1000.0) as u32,
            lat: ll.lat,
            lon: ll.lon,
            alt_m: ground as f32,
            heading_rad: self.body.heading as f32,
            roll_rad: roll as f32,
            pitch_rad: pitch as f32,
            speed_mps: self.body.speed as f32,
            yaw_rate_rps: self.body.yaw_rate as f32,
            steer_rad: self.body.steer as f32,
            fix,
            h_acc_m: (self.gnss_sigma * 1.5) as f32,
            sats: if fix == FixType::FIX_3D { 14 } else { 19 },
        });
    }

    fn emit_lidar(&mut self) {
        let mut scan = LidarScan {
            time_ms: (self.time * 1000.0) as u32,
            layers: LIDAR_LAYERS as u8,
            points_per_layer: LIDAR_POINTS_PER_LAYER as u16,
            elev_min_rad: LIDAR_ELEV_MIN as f32,
            elev_step_rad: LIDAR_ELEV_STEP as f32,
            mount_height_m: LIDAR_MOUNT_M as f32,
            ..Default::default()
        };
        let origin = self.body.pos;
        let z0 = self.world.ground(origin) + LIDAR_MOUNT_M;
        for layer in 0..LIDAR_LAYERS {
            let elevation = LIDAR_ELEV_MIN + layer as f64 * LIDAR_ELEV_STEP;
            let (sin_el, cos_el) = elevation.sin_cos();
            for i in 0..LIDAR_POINTS_PER_LAYER {
                let azimuth = self.body.heading + i as f64 * std::f64::consts::TAU / LIDAR_POINTS_PER_LAYER as f64;
                let h = heading_vec(azimuth);
                let hit = self.world.raycast(origin, z0, (h.x * cos_el, h.y * cos_el, sin_el), LIDAR_RANGE_M);
                let noise = self.rng.signed() * 0.01;
                scan.ranges_cm[layer * LIDAR_POINTS_PER_LAYER + i] =
                    hit.map_or(0, |d| ((d + noise) * 100.0).clamp(1.0, 65535.0) as u16);
            }
        }
        scan.ranges_cm_count = (LIDAR_LAYERS * LIDAR_POINTS_PER_LAYER) as u16;
        self.emit(scan);
    }

    fn emit_video(&mut self) {
        let rgb = camera::render(&self.world, self.body.pos, self.body.heading);
        let jpeg = camera::encode_jpeg(&rgb, self.param(param::VIDEO_QUALITY) as u8);
        self.frame_id = self.frame_id.wrapping_add(1);
        let chunks = jpeg.chunks(VIDEO_CHUNK).count();
        for (i, piece) in jpeg.chunks(VIDEO_CHUNK).enumerate() {
            let mut msg = VideoFrame {
                frame_id: self.frame_id,
                chunk: i as u8,
                chunks: chunks as u8,
                hfov_rad: camera::HFOV_RAD,
                mount_height_m: camera::MOUNT_HEIGHT_M as f32,
                ..Default::default()
            };
            msg.data[..piece.len()].copy_from_slice(piece);
            msg.data_count = piece.len() as u16;
            self.emit(msg);
        }
    }
}
