//! The four simulated vehicle models: what actuators each has, how a drive command is
//! mixed onto them, and how the body moves in response.

use rgf_core::geo::heading_vec;
use rgf_core::msgs::common::{ActuatorKind, KinematicModel, VehicleType};
use rgf_core::Vec2;

#[derive(Debug, Clone)]
pub struct ActuatorSpec {
    pub name: &'static str,
    pub kind: ActuatorKind,
    /// Mount position in the body frame: forward and right of centre, metres.
    pub forward_m: f32,
    pub right_m: f32,
}

#[derive(Debug, Clone)]
pub struct Spec {
    pub vehicle_type: VehicleType,
    pub model: KinematicModel,
    pub name: &'static str,
    pub length_m: f64,
    pub width_m: f64,
    /// Axle spacing, or hull length for the boat.
    pub wheelbase_m: f64,
    /// Wheel track, or thruster spacing for the boat.
    pub track_m: f64,
    pub max_speed_mps: f64,
    pub max_steer_rad: f64,
    pub wheel_radius_m: f64,
    pub battery_v: f64,
    pub battery_wh: f64,
    /// Current one drive motor or thruster draws flat out against a stall.
    pub stall_current_a: f64,
    pub actuators: Vec<ActuatorSpec>,
}

fn act(name: &'static str, kind: ActuatorKind, forward_m: f32, right_m: f32) -> ActuatorSpec {
    ActuatorSpec { name, kind, forward_m, right_m }
}

pub fn spec(vehicle_type: VehicleType) -> Spec {
    use ActuatorKind::*;
    match vehicle_type {
        VehicleType::ROVER6 => Spec {
            vehicle_type,
            model: KinematicModel::CORNER_STEER,
            name: "Six-wheel rover",
            length_m: 1.3,
            width_m: 0.95,
            wheelbase_m: 1.0,
            track_m: 0.8,
            max_speed_mps: 1.5,
            max_steer_rad: 0.6,
            wheel_radius_m: 0.12,
            battery_v: 24.0,
            battery_wh: 480.0,
            stall_current_a: 12.0,
            actuators: vec![
                act("Drive FL", DRIVE, 0.5, -0.4),
                act("Drive FR", DRIVE, 0.5, 0.4),
                act("Drive ML", DRIVE, 0.0, -0.4),
                act("Drive MR", DRIVE, 0.0, 0.4),
                act("Drive RL", DRIVE, -0.5, -0.4),
                act("Drive RR", DRIVE, -0.5, 0.4),
                act("Steer FL", STEER, 0.5, -0.4),
                act("Steer FR", STEER, 0.5, 0.4),
                act("Steer RL", STEER, -0.5, -0.4),
                act("Steer RR", STEER, -0.5, 0.4),
            ],
        },
        VehicleType::TRACTOR => Spec {
            vehicle_type,
            model: KinematicModel::ACKERMANN,
            name: "Four-wheel tractor",
            length_m: 3.6,
            width_m: 1.9,
            wheelbase_m: 2.4,
            track_m: 1.6,
            max_speed_mps: 4.0,
            max_steer_rad: 0.6,
            wheel_radius_m: 0.65,
            battery_v: 48.0,
            battery_wh: 6000.0,
            stall_current_a: 90.0,
            actuators: vec![
                act("Drive RL", DRIVE, -1.2, -0.8),
                act("Drive RR", DRIVE, -1.2, 0.8),
                act("Steering", STEER, 1.2, 0.0),
            ],
        },
        VehicleType::SKID_STEER => Spec {
            vehicle_type,
            model: KinematicModel::SKID,
            name: "Skid-steer rover",
            length_m: 1.0,
            width_m: 0.8,
            wheelbase_m: 0.6,
            track_m: 0.7,
            max_speed_mps: 2.0,
            max_steer_rad: 0.0,
            wheel_radius_m: 0.15,
            battery_v: 24.0,
            battery_wh: 360.0,
            stall_current_a: 15.0,
            actuators: vec![
                act("Drive FL", DRIVE, 0.3, -0.35),
                act("Drive FR", DRIVE, 0.3, 0.35),
                act("Drive RL", DRIVE, -0.3, -0.35),
                act("Drive RR", DRIVE, -0.3, 0.35),
            ],
        },
        VehicleType::BOAT => Spec {
            vehicle_type,
            model: KinematicModel::TWIN_THRUSTER,
            name: "Twin-propeller boat",
            length_m: 2.4,
            width_m: 1.2,
            wheelbase_m: 2.4,
            track_m: 0.8,
            max_speed_mps: 3.0,
            max_steer_rad: 0.6,
            wheel_radius_m: 0.1,
            battery_v: 24.0,
            battery_wh: 1200.0,
            stall_current_a: 30.0,
            actuators: vec![
                act("Port prop", THRUSTER, -1.1, -0.4),
                act("Starboard prop", THRUSTER, -1.1, 0.4),
                act("Rudder", RUDDER, -1.2, 0.0),
            ],
        },
    }
}

/// Live state of one actuator.
#[derive(Debug, Clone, Default)]
pub struct Actuator {
    /// Commanded value, -1..1.
    pub command: f64,
    /// Wheel rim speed (m/s) for drives, thrust fraction for thrusters, angle (rad) for
    /// steering and rudder.
    pub output: f64,
    pub rpm: f64,
    pub current_a: f64,
    pub temp_c: f64,
    pub faults: u8,
}

pub const FAULT_OVER_TEMP: u8 = 1;
pub const FAULT_OVER_CURRENT: u8 = 2;

/// Motion state of the vehicle body in the local frame.
#[derive(Debug, Clone, Default)]
pub struct Body {
    pub pos: Vec2,
    /// Compass heading, radians.
    pub heading: f64,
    /// Forward speed, m/s.
    pub speed: f64,
    /// Clockwise positive, rad/s.
    pub yaw_rate: f64,
    /// Sideways slip to the right, m/s (boat only).
    pub sway: f64,
    /// Effective steering or rudder angle, positive right.
    pub steer: f64,
}

/// Turn a drive command (throttle and steer, each -1..1) into per-actuator commands.
pub fn mix(spec: &Spec, throttle: f64, steer: f64) -> Vec<f64> {
    let (throttle, steer) = (throttle.clamp(-1.0, 1.0), steer.clamp(-1.0, 1.0));
    match spec.vehicle_type {
        // Rear wheels steer against the front ones to tighten the turn.
        VehicleType::ROVER6 => {
            let mut cmds = vec![throttle; 6];
            cmds.extend([steer, steer, -steer, -steer]);
            cmds
        }
        VehicleType::TRACTOR => vec![throttle, throttle, steer],
        VehicleType::SKID_STEER => {
            let (left, right) = differential(throttle, steer);
            vec![left, right, left, right]
        }
        VehicleType::BOAT => {
            let (port, starboard) = differential(throttle, steer * 0.5);
            vec![port, starboard, steer]
        }
    }
}

/// Left and right commands for differential steering, scaled to stay within -1..1.
fn differential(throttle: f64, steer: f64) -> (f64, f64) {
    let (left, right) = (throttle + steer, throttle - steer);
    let scale = left.abs().max(right.abs()).max(1.0);
    (left / scale, right / scale)
}

fn approach(value: f64, target: f64, max_step: f64) -> f64 {
    value + (target - value).clamp(-max_step, max_step)
}

/// Advance actuators and body by `dt`. `slope` is the ground gradient along the heading
/// (rise over run), which loads the drive motors.
pub fn step(spec: &Spec, acts: &mut [Actuator], body: &mut Body, slope: f64, dt: f64) {
    for (a, s) in acts.iter_mut().zip(&spec.actuators) {
        let cmd = a.command.clamp(-1.0, 1.0);
        match s.kind {
            ActuatorKind::DRIVE => {
                let target = cmd * spec.max_speed_mps;
                a.output += (target - a.output) * (dt / 0.35).min(1.0);
                a.rpm = a.output / (std::f64::consts::TAU * spec.wheel_radius_m) * 60.0;
                let effort = (target - a.output) / spec.max_speed_mps;
                let rolling = 0.22 * a.output.abs() / spec.max_speed_mps;
                let climbing = 1.5 * slope * a.output.signum() * (a.output.abs() / spec.max_speed_mps).min(1.0);
                a.current_a = 0.25 + spec.stall_current_a * (effort.abs() * 0.9 + rolling + climbing).clamp(0.0, 1.0);
            }
            ActuatorKind::THRUSTER => {
                a.output += (cmd - a.output) * (dt / 0.5).min(1.0);
                a.rpm = a.output * 3200.0;
                a.current_a = 0.2 + spec.stall_current_a * a.output.abs().powf(1.5);
            }
            ActuatorKind::STEER | ActuatorKind::RUDDER => {
                let target = cmd * spec.max_steer_rad;
                let before = a.output;
                a.output = approach(a.output, target, 0.9 * dt);
                a.rpm = 0.0;
                a.current_a = if (a.output - before).abs() > 1e-6 { 1.6 } else { 0.12 };
            }
        }
        // Heating settles at about 90 C when held at stall current.
        let heat = 65.0 / 45.0 * (a.current_a / spec.stall_current_a).powi(2);
        a.temp_c += dt * (heat - (a.temp_c - 25.0) / 45.0);
        a.faults = 0;
        if a.temp_c > 80.0 {
            a.faults |= FAULT_OVER_TEMP;
        }
        if a.current_a > 0.9 * spec.stall_current_a {
            a.faults |= FAULT_OVER_CURRENT;
        }
    }

    let out = |i: usize| acts[i].output;
    match spec.vehicle_type {
        VehicleType::ROVER6 => {
            body.speed = (0..6).map(out).sum::<f64>() / 6.0;
            body.steer = (out(6) + out(7) - out(8) - out(9)) / 4.0;
            body.yaw_rate = body.speed * 2.0 * body.steer.tan() / spec.wheelbase_m;
        }
        VehicleType::TRACTOR => {
            body.speed = (out(0) + out(1)) / 2.0;
            body.steer = out(2);
            body.yaw_rate = body.speed * body.steer.tan() / spec.wheelbase_m;
        }
        VehicleType::SKID_STEER => {
            let (left, right) = ((out(0) + out(2)) / 2.0, (out(1) + out(3)) / 2.0);
            body.speed = (left + right) / 2.0;
            body.steer = 0.0;
            // Tyres scrub in a skid turn, so the body turns slower than the wheels suggest.
            body.yaw_rate = 0.7 * (left - right) / spec.track_m;
        }
        VehicleType::BOAT => {
            let (port, starboard, rudder) = (out(0), out(1), out(2));
            let thrust = (port + starboard) / 2.0;
            let vmax = spec.max_speed_mps;
            // Thrust against quadratic drag; settles at vmax * sqrt(thrust).
            body.speed += dt * (thrust * vmax * vmax - body.speed * body.speed.abs()) / (vmax * 1.6);
            body.steer = rudder;
            // The rudder bites in proportion to water flow over it: hull speed plus prop wash.
            let flow = body.speed / vmax + 0.35 * thrust;
            let target = 0.9 * (port - starboard) + 1.1 * rudder.sin() * flow;
            body.yaw_rate += dt * (target - body.yaw_rate) / 1.2;
            // The hull slides outward in a turn.
            body.sway += dt * (-0.3 * body.yaw_rate * body.speed - body.sway) / 1.5;
        }
    }

    body.heading = rgf_core::geo::wrap_pi(body.heading + body.yaw_rate * dt);
    let forward = heading_vec(body.heading);
    let right = Vec2::new(forward.y, -forward.x);
    body.pos = body.pos + forward * (body.speed * dt) + right * (body.sway * dt);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(vehicle_type: VehicleType, throttle: f64, steer: f64, seconds: f64) -> (Spec, Body, Vec<Actuator>) {
        let spec = spec(vehicle_type);
        let mut acts = vec![Actuator { temp_c: 25.0, ..Default::default() }; spec.actuators.len()];
        let mut body = Body::default();
        let cmds = mix(&spec, throttle, steer);
        for _ in 0..(seconds / 0.02) as usize {
            for (a, c) in acts.iter_mut().zip(&cmds) {
                a.command = *c;
            }
            step(&spec, &mut acts, &mut body, 0.0, 0.02);
        }
        (spec, body, acts)
    }

    const ALL: [VehicleType; 4] =
        [VehicleType::ROVER6, VehicleType::TRACTOR, VehicleType::SKID_STEER, VehicleType::BOAT];

    #[test]
    fn full_throttle_goes_straight_north_at_top_speed() {
        for vt in ALL {
            let (spec, body, _) = drive(vt, 1.0, 0.0, 20.0);
            assert!((body.speed - spec.max_speed_mps).abs() < 0.05 * spec.max_speed_mps, "{vt:?} speed {}", body.speed);
            assert!(body.pos.x.abs() < 0.01 && body.pos.y > 10.0, "{vt:?} at {:?}", body.pos);
        }
    }

    #[test]
    fn right_steer_turns_clockwise() {
        for vt in ALL {
            let (_, body, _) = drive(vt, 0.5, 0.6, 3.0);
            assert!(body.yaw_rate > 0.05, "{vt:?} yaw rate {}", body.yaw_rate);
            assert!(body.pos.x > 0.0, "{vt:?} should drift east, at {:?}", body.pos);
        }
    }

    #[test]
    fn skid_and_boat_pivot_but_steered_wheels_do_not() {
        for (vt, pivots) in [(VehicleType::SKID_STEER, true), (VehicleType::BOAT, true), (VehicleType::TRACTOR, false)]
        {
            let (_, body, _) = drive(vt, 0.0, 1.0, 4.0);
            assert_eq!(body.yaw_rate.abs() > 0.2, pivots, "{vt:?} yaw rate {}", body.yaw_rate);
        }
    }

    #[test]
    fn mixing_matches_actuator_count_and_draws_current() {
        for vt in ALL {
            let (spec, _, acts) = drive(vt, 0.8, 0.2, 5.0);
            assert_eq!(mix(&spec, 0.8, 0.2).len(), spec.actuators.len());
            assert!(acts[0].current_a > 1.0 && acts[0].rpm.abs() > 10.0, "{vt:?} {:?}", acts[0]);
            assert!(acts[0].temp_c > 25.0);
        }
    }
}
