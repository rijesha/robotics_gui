//! Coverage planning: turn a polygon into a back-and-forth survey path.

use crate::geo::Vec2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurveyParams {
    /// Distance between adjacent passes.
    pub swath_m: f64,
    /// Compass direction of the passes, degrees.
    pub angle_deg: f64,
    /// How far each pass runs past the polygon edge before turning.
    pub overshoot_m: f64,
    /// Drive one lap just inside the boundary before the passes.
    pub perimeter_lap: bool,
}

impl Default for SurveyParams {
    fn default() -> Self {
        Self { swath_m: 5.0, angle_deg: 0.0, overshoot_m: 0.0, perimeter_lap: false }
    }
}

/// Plan a boustrophedon path over `polygon` (local metres, either winding). Returns the
/// waypoints in driving order; empty if the polygon has fewer than three vertices.
pub fn plan(polygon: &[Vec2], params: &SurveyParams) -> Vec<Vec2> {
    if polygon.len() < 3 || params.swath_m <= 0.01 {
        return Vec::new();
    }
    // Rotate so passes run along +y, sweep across x, then rotate back.
    let rot = params.angle_deg.to_radians();
    let poly: Vec<Vec2> = polygon.iter().map(|p| p.rotated(rot)).collect();
    let min_x = poly.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let max_x = poly.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);

    let mut path = Vec::new();
    if params.perimeter_lap {
        let lap = inset(polygon, params.swath_m / 2.0);
        path.extend(lap.iter().copied());
        path.extend(lap.first().copied());
    }

    // The small bias keeps rotation round-off from adding a pass outside the polygon.
    let passes = ((max_x - min_x) / params.swath_m - 1e-6).ceil().max(1.0) as usize;
    // Centre the passes so the leftover margin is split between both sides.
    let start = min_x + ((max_x - min_x) - (passes - 1) as f64 * params.swath_m) / 2.0;
    let mut upward = true;
    for i in 0..passes {
        let x = start + i as f64 * params.swath_m;
        let mut ys = crossings(&poly, x);
        if ys.len() < 2 {
            continue;
        }
        ys.sort_by(|a, b| a.total_cmp(b));
        // Each pair of crossings is one stretch inside the polygon.
        let mut segments: Vec<(f64, f64)> =
            ys.chunks_exact(2).map(|c| (c[0] - params.overshoot_m, c[1] + params.overshoot_m)).collect();
        if !upward {
            segments.reverse();
        }
        for (lo, hi) in segments {
            let (a, b) = if upward { (lo, hi) } else { (hi, lo) };
            path.push(Vec2::new(x, a).rotated(-rot));
            path.push(Vec2::new(x, b).rotated(-rot));
        }
        upward = !upward;
    }
    path
}

/// Where the vertical line at `x` crosses the polygon boundary.
fn crossings(poly: &[Vec2], x: f64) -> Vec<f64> {
    let mut ys = Vec::new();
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        // Half-open test so a vertex shared by two edges counts once.
        if (a.x <= x) != (b.x <= x) {
            let t = (x - a.x) / (b.x - a.x);
            ys.push(a.y + t * (b.y - a.y));
        }
    }
    ys
}

/// Signed area; positive for counter-clockwise winding.
pub fn signed_area(poly: &[Vec2]) -> f64 {
    let mut sum = 0.0;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        sum += a.x * b.y - b.x * a.y;
    }
    sum / 2.0
}

/// Move every vertex inward along its corner bisector. Good for convex field outlines,
/// which is what a perimeter lap needs.
fn inset(poly: &[Vec2], dist: f64) -> Vec<Vec2> {
    let n = poly.len();
    let sign = if signed_area(poly) >= 0.0 { 1.0 } else { -1.0 };
    let normal = |a: Vec2, b: Vec2| {
        let d = b - a;
        let len = d.length().max(1e-9);
        // Left of the edge is inside for counter-clockwise polygons.
        Vec2::new(-d.y / len, d.x / len) * sign
    };
    (0..n)
        .map(|i| {
            let (prev, cur, next) = (poly[(i + n - 1) % n], poly[i], poly[(i + 1) % n]);
            let (n1, n2) = (normal(prev, cur), normal(cur, next));
            let bisector = n1 + n2;
            let scale = dist / (1.0 + n1.dot(n2)).max(0.2);
            cur + bisector * scale
        })
        .collect()
}

pub fn path_length(path: &[Vec2]) -> f64 {
    path.windows(2).map(|w| (w[1] - w[0]).length()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(w: f64, h: f64) -> Vec<Vec2> {
        vec![Vec2::new(0.0, 0.0), Vec2::new(w, 0.0), Vec2::new(w, h), Vec2::new(0.0, h)]
    }

    #[test]
    fn rectangle_gets_evenly_spaced_alternating_passes() {
        let params = SurveyParams { swath_m: 10.0, ..Default::default() };
        let path = plan(&rect(60.0, 100.0), &params);
        assert_eq!(path.len(), 12, "six passes of two waypoints");
        for (i, pass) in path.chunks(2).enumerate() {
            assert!((pass[0].x - (5.0 + 10.0 * i as f64)).abs() < 1e-9);
            assert!((pass[0].x - pass[1].x).abs() < 1e-9);
            let going_up = pass[1].y > pass[0].y;
            assert_eq!(going_up, i % 2 == 0);
            assert!((pass[0].y - pass[1].y).abs() > 99.9);
        }
        // Six 100 m passes joined by five 10 m turns.
        assert!((path_length(&path) - 650.0).abs() < 1e-6);
    }

    #[test]
    fn rotated_passes_stay_inside_and_follow_the_angle() {
        let params = SurveyParams { swath_m: 8.0, angle_deg: 90.0, ..Default::default() };
        let path = plan(&rect(60.0, 40.0), &params);
        assert_eq!(path.len(), 10, "five east-west passes across 40 m");
        for pass in path.chunks(2) {
            assert!((pass[0].y - pass[1].y).abs() < 1e-6, "passes run east-west");
        }
        for p in &path {
            assert!(p.x > -1e-6 && p.x < 60.0 + 1e-6 && p.y > 0.0 && p.y < 40.0);
        }
    }

    #[test]
    fn concave_polygon_and_winding() {
        // A "U": the notch splits the middle passes in two.
        let u = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(30.0, 0.0),
            Vec2::new(30.0, 30.0),
            Vec2::new(20.0, 30.0),
            Vec2::new(20.0, 10.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(10.0, 30.0),
            Vec2::new(0.0, 30.0),
        ];
        let params = SurveyParams { swath_m: 5.0, ..Default::default() };
        let path = plan(&u, &params);
        assert_eq!(path.len(), 12);
        let mut reversed = u.clone();
        reversed.reverse();
        assert_eq!(plan(&reversed, &params).len(), 12);
        assert!(plan(&u[..2], &params).is_empty());
    }

    #[test]
    fn perimeter_lap_is_inside_the_boundary() {
        let params = SurveyParams { swath_m: 4.0, perimeter_lap: true, ..Default::default() };
        let path = plan(&rect(40.0, 40.0), &params);
        let lap = &path[..5];
        assert_eq!(lap[0], lap[4]);
        for p in lap {
            assert!((p.x - 2.0).abs() < 1e-9 || (p.x - 38.0).abs() < 1e-9);
            assert!((p.y - 2.0).abs() < 1e-9 || (p.y - 38.0).abs() < 1e-9);
        }
    }
}
