//! Client, router and simulated vehicle wired together in memory, one test per behaviour
//! the UI depends on.

use rgf_backend::Router;
use rgf_core::client::label;
use rgf_core::msgs::common::{Mode, VehicleType};
use rgf_core::msgs::mission::MissionAction;
use rgf_core::msgs::simctl::{SimLinkFault, SimSelectVehicle};
use rgf_core::msgs::telemetry::{FixType, NavState};
use rgf_core::reliable::{Failure, TxState};
use rgf_core::{Client, Decoder, LatLon, LocalFrame, Vec2};
use rgf_sim::sim::{home, param};
use rgf_sim::Sim;

const DT: f64 = 0.02;

struct Rig {
    client: Client,
    router: Router,
    sim: Sim,
    now: f64,
    from_vehicle: Decoder,
    from_client: Decoder,
    to_client: Decoder,
}

impl Rig {
    fn new(vehicle: VehicleType) -> Self {
        let mut rig = Self {
            client: Client::new(1),
            router: Router::default(),
            sim: Sim::new(vehicle),
            now: 0.0,
            from_vehicle: Decoder::new(),
            from_client: Decoder::new(),
            to_client: Decoder::new(),
        };
        rig.run(1.0);
        rig
    }

    fn step(&mut self) {
        self.now += DT;
        for frame in self.client.drain() {
            for msg in self.from_client.feed(&frame) {
                self.router.from_ui(msg, self.now);
            }
        }
        self.router.tick(self.now);
        for frame in self.router.drain_to_vehicle() {
            self.sim.receive(&frame);
        }
        self.sim.step(DT);
        for frame in self.sim.drain() {
            for msg in self.from_vehicle.feed(&frame) {
                self.router.from_vehicle(msg, self.now);
            }
        }
        for frame in self.router.drain_to_ui() {
            for msg in self.to_client.feed(&frame) {
                self.client.handle(&msg, self.now);
            }
        }
        self.client.poll(self.now);
    }

    fn run(&mut self, seconds: f64) {
        for _ in 0..(seconds / DT).round() as usize {
            self.step();
        }
    }

    /// Step until the label settles; panics if it never does.
    fn settle(&mut self, label: &str) -> TxState {
        for _ in 0..(10.0 / DT) as usize {
            self.step();
            match self.client.state(label) {
                Some(TxState::Pending { .. }) | None => {}
                Some(done) => return done,
            }
        }
        panic!("'{label}' never settled");
    }

    fn set_fault(&mut self, loss_pct: u8, cut: bool) {
        self.client.send_tracked(label::SIM_FAULT, self.now, |seq| SimLinkFault { seq, loss_pct, cut });
        assert_eq!(self.settle(label::SIM_FAULT), TxState::Confirmed);
    }
}

const ALL: [VehicleType; 4] = [VehicleType::ROVER6, VehicleType::TRACTOR, VehicleType::SKID_STEER, VehicleType::BOAT];

#[test]
fn every_vehicle_describes_itself_and_reports_telemetry() {
    for vehicle in ALL {
        let mut rig = Rig::new(vehicle);
        rig.run(2.5);
        let client = &rig.client;
        assert!(client.vehicle_alive(rig.now), "{vehicle:?}");
        assert_eq!(client.vehicle_type(), Some(vehicle));
        assert_eq!(client.actuators.len(), rig.sim.spec.actuators.len());
        assert!(client.actuators.iter().all(Option::is_some), "{vehicle:?} actuator descriptions");
        assert!(client.motors.iter().all(Option::is_some), "{vehicle:?} motor status");
        assert!(client.pose.is_some() && client.power.is_some() && client.nav.is_some());
        assert!(client.lidar.as_ref().is_some_and(|s| s.ranges_cm_count == 1440), "{vehicle:?} lidar");
        let video = client.video.as_ref().expect("video frame");
        assert_eq!(&video.jpeg[..2], &[0xFF, 0xD8]);
        assert_eq!(&video.jpeg[video.jpeg.len() - 2..], &[0xFF, 0xD9], "{vehicle:?} frame reassembled whole");
        assert!(video.hfov_rad > 1.0 && video.mount_height_m > 1.0);
    }
}

#[test]
fn every_vehicle_completes_an_uploaded_mission() {
    for vehicle in ALL {
        let mut rig = Rig::new(vehicle);
        let frame = LocalFrame::new(home(vehicle));
        let square = [Vec2::new(0.0, 25.0), Vec2::new(25.0, 25.0), Vec2::new(25.0, 0.0), Vec2::new(0.0, 0.0)];
        let waypoints: Vec<LatLon> = square.iter().map(|p| frame.to_latlon(*p)).collect();

        rig.client.upload_mission(&waypoints, 1.0, rig.now);
        assert_eq!(rig.settle(label::MISSION), TxState::Confirmed, "{vehicle:?} upload");
        rig.client.mission_command(MissionAction::MISSION_START, rig.now);
        assert_eq!(rig.settle(label::MISSION_CMD), TxState::Confirmed, "{vehicle:?} start");
        rig.run(1.0);
        assert_eq!(rig.client.mode(), Some(Mode::AUTO));

        let mut elapsed = 0.0;
        while rig.sim.nav_state() != NavState::NAV_COMPLETE {
            rig.run(1.0);
            elapsed += 1.0;
            assert!(elapsed < 400.0, "{vehicle:?} stuck at waypoint {:?}, pos {:?}", rig.client.nav, rig.sim.body.pos);
        }
        rig.run(1.0);
        let nav = rig.client.nav.as_ref().unwrap();
        assert_eq!(nav.state, NavState::NAV_COMPLETE);
        assert_eq!((nav.active_wp, nav.wp_count), (4, 4));
        assert!(rig.sim.body.pos.length() < 5.0, "{vehicle:?} ended at {:?}", rig.sim.body.pos);
        assert_eq!(rig.client.mode(), Some(Mode::HOLD));
        assert!(rig.client.power.as_ref().unwrap().consumed_wh > 0.01);
    }
}

#[test]
fn setting_is_confirmed_only_by_a_matching_readback() {
    let mut rig = Rig::new(VehicleType::ROVER6);
    rig.client.request_params(rig.now);
    assert_eq!(rig.settle(label::PARAM_LIST), TxState::Confirmed);
    rig.run(0.5);
    assert_eq!(rig.client.params.len(), rig.client.param_count as usize);

    let id = param::CRUISE_SPEED as u16;
    rig.client.set_param(id, 1.1, rig.now);
    assert_eq!(rig.settle(&label::param(id)), TxState::Confirmed);
    assert_eq!(rig.client.params[&id].value, 1.1);
    assert_eq!(rig.sim.params[param::CRUISE_SPEED].value, 1.1);

    // Out of range: the vehicle clamps it, so what it holds is not what was asked for.
    rig.client.set_param(id, 99.0, rig.now);
    assert_eq!(rig.settle(&label::param(id)), TxState::NotSent(Failure::Mismatch));
    assert_eq!(rig.client.params[&id].value, 1.5, "the UI shows what the vehicle really holds");

    rig.client.set_param(200, 1.0, rig.now);
    assert_eq!(rig.settle(&label::param(200)), TxState::NotSent(Failure::Rejected));
}

#[test]
fn cut_link_shows_not_sent_and_recovers() {
    let mut rig = Rig::new(VehicleType::SKID_STEER);
    let id = param::WP_RADIUS as u16;

    rig.set_fault(0, true);
    rig.client.set_param(id, 3.0, rig.now);
    rig.client.set_mode(Mode::MANUAL, rig.now);
    assert_eq!(rig.settle(&label::param(id)), TxState::NotSent(Failure::Timeout));
    assert_eq!(rig.settle(label::MODE), TxState::NotSent(Failure::Timeout));
    assert_ne!(rig.sim.params[param::WP_RADIUS].value, 3.0);
    assert_eq!(rig.sim.mode, Mode::DISARMED);
    rig.run(2.0);
    assert!(!rig.client.vehicle_alive(rig.now), "heartbeats stop when the link is cut");

    rig.set_fault(0, false);
    rig.client.set_param(id, 3.0, rig.now);
    assert_eq!(rig.settle(&label::param(id)), TxState::Confirmed);
    assert_eq!(rig.sim.params[param::WP_RADIUS].value, 3.0);
    rig.run(1.0);
    assert!(rig.client.vehicle_alive(rig.now));
}

#[test]
fn backend_retries_carry_commands_across_a_lossy_link() {
    let mut rig = Rig::new(VehicleType::TRACTOR);
    rig.set_fault(35, false);
    let id = param::STEER_GAIN as u16;
    let mut confirmed = 0;
    for i in 0..40 {
        let value = 1.0 + i as f32 * 0.05;
        rig.client.set_param(id, value, rig.now);
        if rig.settle(&label::param(id)) == TxState::Confirmed {
            confirmed += 1;
            assert_eq!(rig.sim.params[param::STEER_GAIN].value, value);
        }
    }
    // A single try survives 35% loss each way about 42% of the time; four tries about 89%.
    assert!(confirmed >= 30, "only {confirmed}/40 confirmed despite retries");
}

#[test]
fn estop_blocks_arming_and_rejection_is_reported() {
    let mut rig = Rig::new(VehicleType::BOAT);
    rig.client.set_mode(Mode::MANUAL, rig.now);
    assert_eq!(rig.settle(label::MODE), TxState::Confirmed);
    rig.client.set_estop(true, rig.now);
    assert_eq!(rig.settle(label::ESTOP), TxState::Confirmed);
    rig.run(1.0);
    let hb = rig.client.heartbeat.as_ref().unwrap();
    assert!(hb.estop && hb.mode == Mode::DISARMED);

    rig.client.set_mode(Mode::MANUAL, rig.now);
    assert_eq!(rig.settle(label::MODE), TxState::NotSent(Failure::Rejected));
    // Starting a mission with nothing uploaded is refused too.
    rig.client.set_estop(false, rig.now);
    rig.settle(label::ESTOP);
    rig.client.mission_command(MissionAction::MISSION_START, rig.now);
    assert_eq!(rig.settle(label::MISSION_CMD), TxState::NotSent(Failure::Rejected));
}

#[test]
fn manual_drive_and_motor_test_move_the_right_actuators() {
    use rgf_core::msgs::control::{DriveCommand, MotorCommand};
    let mut rig = Rig::new(VehicleType::ROVER6);
    rig.client.set_mode(Mode::MANUAL, rig.now);
    rig.settle(label::MODE);
    for _ in 0..150 {
        rig.client.send(DriveCommand { throttle: 1.0, steer: 0.0 });
        rig.step();
    }
    assert!(rig.client.pose.as_ref().unwrap().speed_mps > 1.2);
    // The stream stops: the vehicle must stop by itself.
    rig.run(2.5);
    assert!(rig.client.pose.as_ref().unwrap().speed_mps.abs() < 0.05);

    rig.client.set_mode(Mode::MOTOR_TEST, rig.now);
    rig.settle(label::MODE);
    for _ in 0..100 {
        rig.client.send(MotorCommand { index: 3, value: 0.5 });
        rig.step();
    }
    let rpm: Vec<f32> = rig.client.motors.iter().map(|m| m.as_ref().unwrap().rpm).collect();
    assert!(rpm[3] > 20.0, "motor 3 should spin: {rpm:?}");
    assert!(rpm.iter().enumerate().all(|(i, r)| i == 3 || r.abs() < 1.0), "only motor 3: {rpm:?}");
}

#[test]
fn rtk_base_surveys_in_then_rover_reaches_fixed() {
    use rgf_core::msgs::rtk::{BaseConfig, BaseMode};
    let mut rig = Rig::new(VehicleType::TRACTOR);
    rig.client.send_tracked(label::BASE, rig.now, |seq| BaseConfig {
        seq,
        use_fixed: false,
        min_duration_s: 5,
        accuracy_limit_m: 1.0,
        ..Default::default()
    });
    assert_eq!(rig.settle(label::BASE), TxState::Confirmed);
    rig.run(2.0);
    let base = rig.client.base.as_ref().unwrap();
    assert_eq!(base.mode, BaseMode::BASE_SURVEY_IN);
    assert!(base.survey_progress > 0.0 && base.survey_progress < 1.0);
    assert_eq!(rig.client.pose.as_ref().unwrap().fix, FixType::FIX_3D);
    let coarse = rig.client.pose.as_ref().unwrap().h_acc_m;

    rig.run(20.0);
    let base = rig.client.base.as_ref().unwrap();
    assert_eq!(base.mode, BaseMode::BASE_FIXED);
    assert!(base.rtcm_rate_hz > 0.0 && base.rtcm_bytes > 0);
    let pose = rig.client.pose.as_ref().unwrap();
    assert_eq!(pose.fix, FixType::RTK_FIXED);
    assert!(pose.h_acc_m < 0.05 && pose.h_acc_m < coarse / 10.0);
    // The base sits within a short walk of the rover.
    let offset = LocalFrame::new(home(VehicleType::TRACTOR)).to_local(LatLon::new(base.lat, base.lon));
    assert!(offset.length() > 10.0 && offset.length() < 40.0);

    // Bad configuration is refused by the backend itself.
    rig.client.send_tracked(label::BASE, rig.now, |seq| BaseConfig {
        seq,
        use_fixed: true,
        lat: 123.0,
        ..Default::default()
    });
    assert_eq!(rig.settle(label::BASE), TxState::NotSent(Failure::Rejected));
}

#[test]
fn switching_vehicle_resets_what_the_client_knows() {
    let mut rig = Rig::new(VehicleType::ROVER6);
    rig.run(2.5);
    assert_eq!(rig.client.actuators.len(), 10);
    rig.client
        .send_tracked(label::SIM_VEHICLE, rig.now, |seq| SimSelectVehicle { seq, vehicle_type: VehicleType::BOAT });
    assert_eq!(rig.settle(label::SIM_VEHICLE), TxState::Confirmed);
    rig.run(2.5);
    assert_eq!(rig.client.vehicle_type(), Some(VehicleType::BOAT));
    assert_eq!(rig.client.vehicle_epoch, 1);
    assert_eq!(rig.client.actuators.len(), 3);
    assert!(rig.client.actuators.iter().all(Option::is_some));
    let pose = rig.client.pose.as_ref().unwrap();
    assert!((pose.lat - home(VehicleType::BOAT).lat).abs() < 0.01);
}
