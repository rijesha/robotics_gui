//! Positions and the local tangent frame used for planning and sensor fusion.

const EARTH_RADIUS_M: f64 = 6_378_137.0;

/// WGS84 position in degrees.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

impl LatLon {
    pub fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }
}

/// Metres in a local frame: `x` east, `y` north.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }
    pub fn dot(self, o: Vec2) -> f64 {
        self.x * o.x + self.y * o.y
    }
    /// Rotate counter-clockwise by `rad`.
    pub fn rotated(self, rad: f64) -> Vec2 {
        let (s, c) = rad.sin_cos();
        Vec2::new(self.x * c - self.y * s, self.x * s + self.y * c)
    }
}

impl std::ops::Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}
impl std::ops::Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}
impl std::ops::Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, k: f64) -> Vec2 {
        Vec2::new(self.x * k, self.y * k)
    }
}

/// East/north tangent plane around an origin. Accurate to centimetres over the few
/// kilometres a ground vehicle or small boat covers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalFrame {
    pub origin: LatLon,
}

impl LocalFrame {
    pub fn new(origin: LatLon) -> Self {
        Self { origin }
    }

    pub fn to_local(&self, p: LatLon) -> Vec2 {
        let lat0 = self.origin.lat.to_radians();
        Vec2::new(
            (p.lon - self.origin.lon).to_radians() * EARTH_RADIUS_M * lat0.cos(),
            (p.lat - self.origin.lat).to_radians() * EARTH_RADIUS_M,
        )
    }

    pub fn to_latlon(&self, v: Vec2) -> LatLon {
        let lat0 = self.origin.lat.to_radians();
        LatLon::new(
            self.origin.lat + (v.y / EARTH_RADIUS_M).to_degrees(),
            self.origin.lon + (v.x / (EARTH_RADIUS_M * lat0.cos())).to_degrees(),
        )
    }
}

/// Unit vector for a compass heading (0 = north, clockwise positive).
pub fn heading_vec(heading_rad: f64) -> Vec2 {
    Vec2::new(heading_rad.sin(), heading_rad.cos())
}

/// Compass bearing of a local vector.
pub fn bearing(v: Vec2) -> f64 {
    v.x.atan2(v.y)
}

/// Wrap an angle to (-pi, pi].
pub fn wrap_pi(a: f64) -> f64 {
    let mut a = a % std::f64::consts::TAU;
    if a > std::f64::consts::PI {
        a -= std::f64::consts::TAU;
    } else if a <= -std::f64::consts::PI {
        a += std::f64::consts::TAU;
    }
    a
}

/// Place a point given in the body frame (forward, right) into the local frame.
pub fn body_to_local(pos: Vec2, heading_rad: f64, forward: f64, right: f64) -> Vec2 {
    let f = heading_vec(heading_rad);
    let r = Vec2::new(f.y, -f.x);
    pos + f * forward + r * right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_frame_round_trips_and_scales() {
        let frame = LocalFrame::new(LatLon::new(41.9, -93.55));
        let p = Vec2::new(123.4, -567.8);
        let back = frame.to_local(frame.to_latlon(p));
        assert!((back - p).length() < 1e-6);
        // One degree of latitude is about 111.3 km.
        let north = frame.to_local(LatLon::new(42.9, -93.55));
        assert!((north.y - 111_319.5).abs() < 10.0);
    }

    #[test]
    fn heading_conventions() {
        let east = heading_vec(std::f64::consts::FRAC_PI_2);
        assert!((east.x - 1.0).abs() < 1e-12 && east.y.abs() < 1e-12);
        assert!((bearing(Vec2::new(1.0, 0.0)) - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        // Facing north, a point to the right is east.
        let p = body_to_local(Vec2::default(), 0.0, 0.0, 2.0);
        assert!((p.x - 2.0).abs() < 1e-12 && p.y.abs() < 1e-12);
        assert!((wrap_pi(3.5 * std::f64::consts::PI) + std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    }
}
