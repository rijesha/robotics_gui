//! Height and obstacle grid built up from lidar returns, for the top-down map.

use crate::geo::Vec2;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct Cell {
    pub min_h: f32,
    pub max_h: f32,
    pub hits: u32,
}

impl Cell {
    /// Returns at very different heights in one cell mean something is standing there.
    pub fn is_obstacle(&self, threshold_m: f32) -> bool {
        self.max_h - self.min_h > threshold_m
    }
}

#[derive(Debug, Clone)]
pub struct TerrainGrid {
    pub cell_m: f64,
    pub cells: HashMap<(i32, i32), Cell>,
}

impl TerrainGrid {
    pub fn new(cell_m: f64) -> Self {
        Self { cell_m, cells: HashMap::new() }
    }

    pub fn insert(&mut self, pos: Vec2, height: f32) {
        let key = ((pos.x / self.cell_m).floor() as i32, (pos.y / self.cell_m).floor() as i32);
        self.cells
            .entry(key)
            .and_modify(|c| {
                c.min_h = c.min_h.min(height);
                c.max_h = c.max_h.max(height);
                c.hits += 1;
            })
            .or_insert(Cell { min_h: height, max_h: height, hits: 1 });
    }

    pub fn cell_at(&self, pos: Vec2) -> Option<&Cell> {
        self.cells.get(&((pos.x / self.cell_m).floor() as i32, (pos.y / self.cell_m).floor() as i32))
    }

    /// South-west corner of a cell in the local frame.
    pub fn cell_origin(&self, key: (i32, i32)) -> Vec2 {
        Vec2::new(key.0 as f64 * self.cell_m, key.1 as f64 * self.cell_m)
    }

    /// Lowest and highest ground height seen, for colour scaling.
    pub fn ground_range(&self) -> Option<(f32, f32)> {
        let mut it = self.cells.values().map(|c| c.min_h);
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), h| (lo.min(h), hi.max(h))))
    }

    pub fn clear(&mut self) {
        self.cells.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separates_ground_from_obstacles() {
        let mut grid = TerrainGrid::new(0.5);
        grid.insert(Vec2::new(0.1, 0.1), 1.0);
        grid.insert(Vec2::new(0.2, 0.3), 1.1);
        grid.insert(Vec2::new(5.1, 0.1), 1.0);
        grid.insert(Vec2::new(5.2, 0.1), 3.0);
        grid.insert(Vec2::new(-0.1, -0.1), 0.5);
        assert_eq!(grid.cells.len(), 3);
        assert!(!grid.cells[&(0, 0)].is_obstacle(0.5));
        assert!(grid.cells[&(10, 0)].is_obstacle(0.5));
        assert!(grid.cells.contains_key(&(-1, -1)));
        assert_eq!(grid.ground_range(), Some((0.5, 1.0)));
    }
}
