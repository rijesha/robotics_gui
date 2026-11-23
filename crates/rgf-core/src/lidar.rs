//! Turning a `LidarScan` into points.

use crate::msgs::sensors::LidarScan;

/// A lidar return in the vehicle's level body frame, in metres. `up` is measured from
/// the ground under the vehicle.
#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub forward: f32,
    pub right: f32,
    pub up: f32,
}

pub fn body_points(scan: &LidarScan) -> Vec<Point> {
    let per_layer = scan.points_per_layer as usize;
    let count = (scan.ranges_cm_count as usize).min(scan.layers as usize * per_layer);
    let mut points = Vec::with_capacity(count);
    if per_layer == 0 {
        return points;
    }
    let az_step = std::f32::consts::TAU / per_layer as f32;
    for (i, &range_cm) in scan.ranges_cm[..count].iter().enumerate() {
        if range_cm == 0 {
            continue;
        }
        let range = range_cm as f32 * 0.01;
        let elevation = scan.elev_min_rad + (i / per_layer) as f32 * scan.elev_step_rad;
        let azimuth = (i % per_layer) as f32 * az_step;
        let horizontal = range * elevation.cos();
        points.push(Point {
            forward: horizontal * azimuth.cos(),
            right: horizontal * azimuth.sin(),
            up: scan.mount_height_m + range * elevation.sin(),
        });
    }
    points
}
