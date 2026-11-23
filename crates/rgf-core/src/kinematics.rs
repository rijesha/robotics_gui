//! Expected-path prediction from vehicle velocity and wheel or rudder position.
//!
//! All four vehicle types reduce to a path curvature. The measured curvature
//! (yaw rate / speed) is where the vehicle is turning now; the steering geometry gives
//! where it is settling to. The prediction relaxes from the first to the second.

use crate::msgs::common::KinematicModel;

#[derive(Debug, Clone, Copy)]
pub struct PathInput {
    pub model: KinematicModel,
    /// Axle spacing for wheeled vehicles, hull length for boats.
    pub wheelbase_m: f64,
    pub speed_mps: f64,
    /// Clockwise positive, like compass heading.
    pub yaw_rate_rps: f64,
    /// Steering or rudder angle, positive turns right.
    pub steer_rad: f64,
}

#[derive(Debug, Clone, Default)]
pub struct PredictedPath {
    /// Body-frame points as (forward, right) metres, starting at the vehicle.
    pub points: Vec<(f64, f64)>,
    /// True when the vehicle is stopped and the path shows where the current steering
    /// would take it at a nominal speed.
    pub nominal: bool,
}

const STOPPED_MPS: f64 = 0.1;
const NOMINAL_MPS: f64 = 1.0;

/// Curvature (1/m, positive right) the steering geometry settles to, if the model has one.
pub fn steady_curvature(model: KinematicModel, wheelbase_m: f64, steer_rad: f64) -> Option<f64> {
    let l = wheelbase_m.max(0.1);
    match model {
        KinematicModel::ACKERMANN => Some(steer_rad.tan() / l),
        // Front and rear axles steer in opposition about the middle axle.
        KinematicModel::CORNER_STEER => Some(2.0 * steer_rad.tan() / l),
        // A rudder turns the hull over roughly two boat lengths per radian of deflection.
        KinematicModel::TWIN_THRUSTER => Some(steer_rad.sin() / (2.0 * l)),
        // Skid steering has no steering angle; only the measured yaw rate says anything.
        KinematicModel::SKID => None,
    }
}

/// Seconds for the turn rate to settle after a steering change.
fn settle_time(model: KinematicModel) -> f64 {
    match model {
        KinematicModel::TWIN_THRUSTER => 2.0,
        _ => 0.4,
    }
}

pub fn predict_path(input: &PathInput, horizon_s: f64) -> PredictedPath {
    let steady = steady_curvature(input.model, input.wheelbase_m, input.steer_rad);
    let moving = input.speed_mps.abs() > STOPPED_MPS;
    let speed = if moving { input.speed_mps } else { NOMINAL_MPS };
    let measured = if moving { input.yaw_rate_rps / input.speed_mps } else { steady.unwrap_or(0.0) };
    // Differential thrust or skid steering can hold a turn the steering geometry does
    // not explain, so without a steering deflection keep the measured curvature.
    let target = match steady {
        Some(k) if input.model != KinematicModel::TWIN_THRUSTER || input.steer_rad.abs() > 0.02 => k,
        _ => measured,
    };
    let tau = settle_time(input.model);

    let dt = 0.1;
    let steps = (horizon_s / dt).ceil() as usize;
    let mut points = Vec::with_capacity(steps + 1);
    let (mut fwd, mut right, mut psi) = (0.0f64, 0.0f64, 0.0f64);
    points.push((fwd, right));
    for i in 0..steps {
        let t = i as f64 * dt;
        let curvature = target + (measured - target) * (-t / tau).exp();
        let ds = speed * dt;
        psi += curvature * ds;
        fwd += ds * psi.cos();
        right += ds * psi.sin();
        points.push((fwd, right));
    }
    PredictedPath { points, nominal: !moving }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ackermann_follows_the_closed_form_circle() {
        let (l, steer, v) = (2.5f64, 0.3f64, 2.0f64);
        let radius = l / steer.tan();
        let input = PathInput {
            model: KinematicModel::ACKERMANN,
            wheelbase_m: l,
            speed_mps: v,
            yaw_rate_rps: v / radius,
            steer_rad: steer,
        };
        let path = predict_path(&input, 5.0);
        assert!(!path.nominal);
        // Every point sits on the circle centred `radius` to the right of the vehicle.
        for &(f, r) in &path.points {
            let d = (f * f + (r - radius) * (r - radius)).sqrt();
            assert!((d - radius).abs() < 0.15, "off circle by {}", d - radius);
        }
        let travelled: f64 = path.points.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).sum();
        assert!((travelled - 10.0).abs() < 0.05);
    }

    #[test]
    fn straight_and_reverse() {
        let mut input = PathInput {
            model: KinematicModel::SKID,
            wheelbase_m: 1.0,
            speed_mps: 1.0,
            yaw_rate_rps: 0.0,
            steer_rad: 0.0,
        };
        let end = *predict_path(&input, 3.0).points.last().unwrap();
        assert!((end.0 - 3.0).abs() < 1e-6 && end.1.abs() < 1e-9);
        input.speed_mps = -1.0;
        assert!(predict_path(&input, 3.0).points.last().unwrap().0 < -2.9);
    }

    #[test]
    fn stopped_vehicle_previews_its_steering() {
        let input = PathInput {
            model: KinematicModel::CORNER_STEER,
            wheelbase_m: 1.2,
            speed_mps: 0.0,
            yaw_rate_rps: 0.0,
            steer_rad: -0.4,
        };
        let path = predict_path(&input, 3.0);
        assert!(path.nominal);
        assert!(path.points.last().unwrap().1 < -0.5, "left steer should curve left");
    }
}
