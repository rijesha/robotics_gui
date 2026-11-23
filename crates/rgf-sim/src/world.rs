//! The synthetic world the simulated sensors look at. Lidar and camera sample the same
//! surface, so what the video shows is what the lidar measures.

use rgf_core::Vec2;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Obstacle {
    pub pos: Vec2,
    pub radius: f64,
    pub height: f64,
    pub color: [u8; 3],
}

const GRID_M: f64 = 4.0;

pub struct World {
    pub water: bool,
    pub obstacles: Vec<Obstacle>,
    /// Obstacle indices by grid cell, for fast surface lookups.
    grid: HashMap<(i32, i32), Vec<usize>>,
}

/// Small deterministic generator; the simulator must behave the same on every run.
pub struct Rng(pub u64);

impl Rng {
    pub fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }
    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f64 {
        self.next_u32() as f64 / (u32::MAX as f64 + 1.0)
    }
    /// Uniform in [-1, 1).
    pub fn signed(&mut self) -> f64 {
        self.unit() * 2.0 - 1.0
    }
}

impl World {
    /// Rolling farmland with trees, rocks and a shed.
    pub fn land() -> Self {
        let mut rng = Rng(0x5EED_1234_ABCD);
        let mut obstacles = Vec::new();
        while obstacles.len() < 45 {
            let pos = Vec2::new(rng.signed() * 130.0, rng.signed() * 130.0);
            if pos.length() < 8.0 {
                continue;
            }
            obstacles.push(if rng.unit() < 0.6 {
                Obstacle { pos, radius: 0.5 + rng.unit() * 0.4, height: 4.0 + rng.unit() * 3.0, color: [38, 92, 40] }
            } else {
                Obstacle { pos, radius: 0.8 + rng.unit() * 0.8, height: 0.6 + rng.unit() * 0.6, color: [128, 124, 118] }
            });
        }
        obstacles.push(Obstacle { pos: Vec2::new(-28.0, 22.0), radius: 4.5, height: 4.0, color: [150, 52, 44] });
        Self::build(false, obstacles)
    }

    /// Flat water with buoys, rocks and a line of pier posts.
    pub fn water() -> Self {
        let mut rng = Rng(0xB0A7_5EED_77);
        let mut obstacles = Vec::new();
        while obstacles.len() < 25 {
            let pos = Vec2::new(rng.signed() * 150.0, rng.signed() * 150.0);
            if pos.length() < 10.0 {
                continue;
            }
            obstacles.push(if rng.unit() < 0.6 {
                Obstacle { pos, radius: 0.45, height: 1.3, color: [235, 110, 30] }
            } else {
                Obstacle { pos, radius: 1.2 + rng.unit(), height: 0.5 + rng.unit() * 0.5, color: [90, 88, 84] }
            });
        }
        for i in 0..8 {
            obstacles.push(Obstacle {
                pos: Vec2::new(18.0 + i as f64 * 2.5, -25.0),
                radius: 0.3,
                height: 1.8,
                color: [110, 84, 60],
            });
        }
        Self::build(true, obstacles)
    }

    fn build(water: bool, obstacles: Vec<Obstacle>) -> Self {
        let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for (i, o) in obstacles.iter().enumerate() {
            let lo = Self::cell(Vec2::new(o.pos.x - o.radius, o.pos.y - o.radius));
            let hi = Self::cell(Vec2::new(o.pos.x + o.radius, o.pos.y + o.radius));
            for cx in lo.0..=hi.0 {
                for cy in lo.1..=hi.1 {
                    grid.entry((cx, cy)).or_default().push(i);
                }
            }
        }
        Self { water, obstacles, grid }
    }

    fn cell(p: Vec2) -> (i32, i32) {
        ((p.x / GRID_M).floor() as i32, (p.y / GRID_M).floor() as i32)
    }

    /// Ground (or water) height in metres.
    pub fn ground(&self, p: Vec2) -> f64 {
        if self.water {
            return 0.0;
        }
        1.2 * (p.x / 23.0).sin() * (p.y / 31.0).cos() + 0.5 * (p.x / 7.0 + 1.0).sin() + 0.3 * (p.y / 5.0).cos()
    }

    fn obstacle_at(&self, p: Vec2) -> Option<&Obstacle> {
        self.grid.get(&Self::cell(p))?.iter().map(|&i| &self.obstacles[i]).find(|o| (o.pos - p).length() <= o.radius)
    }

    /// Height of whatever is at `p`: the ground, or the top of an obstacle standing on it.
    pub fn top(&self, p: Vec2) -> f64 {
        match self.obstacle_at(p) {
            Some(o) => self.ground(o.pos) + o.height,
            None => self.ground(p),
        }
    }

    /// Height and colour of the surface at `p`, for the camera.
    pub fn surface(&self, p: Vec2) -> (f64, [u8; 3]) {
        if let Some(o) = self.obstacle_at(p) {
            return (self.ground(o.pos) + o.height, o.color);
        }
        let h = self.ground(p);
        let color = if self.water {
            let ripple = ((p.x * 0.9).sin() + (p.y * 1.3 + p.x * 0.4).sin()) * 6.0;
            [(28.0 + ripple) as u8, (84.0 + ripple) as u8, (128.0 + ripple * 1.5) as u8]
        } else {
            // Crop rows running north-south, tinted by elevation.
            let row = if (p.x / 1.5).floor() as i64 % 2 == 0 { 1.0 } else { 0.82 };
            let lift = (h * 9.0).clamp(-18.0, 18.0);
            [((78.0 + lift) * row) as u8, ((122.0 + lift) * row) as u8, ((52.0 + lift * 0.5) * row) as u8]
        };
        (h, color)
    }

    /// Distance along a ray to the first surface it meets, if within `max_range`.
    /// `dir` is a unit vector as (east, north, up).
    pub fn raycast(&self, origin: Vec2, z0: f64, dir: (f64, f64, f64), max_range: f64) -> Option<f64> {
        let at = |t: f64| (Vec2::new(origin.x + dir.0 * t, origin.y + dir.1 * t), z0 + dir.2 * t);
        let mut prev = 0.0;
        let mut t = 0.3;
        while t <= max_range {
            let (p, z) = at(t);
            // Nothing in the world is taller than this; a ray above it and climbing is in the sky.
            if dir.2 > 0.0 && z > 12.0 {
                return None;
            }
            if z <= self.top(p) {
                // Narrow down the crossing between the last clear sample and this one.
                let (mut lo, mut hi) = (prev, t);
                for _ in 0..6 {
                    let mid = (lo + hi) / 2.0;
                    let (p, z) = at(mid);
                    if z <= self.top(p) {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                return Some(hi);
            }
            prev = t;
            t += (0.02 * t).max(0.12);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rays_hit_ground_and_obstacles() {
        let world = World::water();
        // Straight down from 1.2 m above flat water.
        let down = world.raycast(Vec2::default(), 1.2, (0.0, 0.0, -1.0), 30.0).unwrap();
        assert!((down - 1.2).abs() < 0.02);
        // Level ray at the first pier post, 18 m east and 25 m south.
        let target = Vec2::new(18.0, -25.0);
        let dist = target.length();
        let dir = (target.x / dist, target.y / dist, 0.0);
        let hit = world.raycast(Vec2::default(), 1.0, dir, 60.0).unwrap();
        assert!((hit - (dist - 0.3)).abs() < 0.1, "hit at {hit}, post face at {}", dist - 0.3);
        // A climbing ray finds nothing.
        assert!(world.raycast(Vec2::default(), 1.2, (0.0, 0.6, 0.8), 30.0).is_none());
    }
}
