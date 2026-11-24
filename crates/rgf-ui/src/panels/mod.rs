//! The built-in panels.

pub mod drive;
pub mod lidar;
pub mod link;
pub mod map;
pub mod mission;
pub mod motors;
pub mod power;
pub mod rtk;
pub mod settings;
pub mod video;

use crate::workspace::DesktopLayout;
use crate::Panel;
use std::path::PathBuf;

/// Every built-in panel. `tile_cache` is where map tiles are kept on disk (native only).
pub fn all(ctx: &egui::Context, tile_cache: Option<PathBuf>) -> Vec<Box<dyn Panel>> {
    vec![
        Box::new(map::MapPanel::new(ctx, tile_cache)),
        Box::new(drive::DrivePanel),
        Box::new(mission::MissionPanel::default()),
        Box::new(video::VideoPanel::default()),
        Box::new(lidar::LidarPanel::default()),
        Box::new(motors::MotorsPanel),
        Box::new(power::PowerPanel),
        Box::new(settings::SettingsPanel),
        Box::new(rtk::RtkPanel::default()),
        Box::new(link::LinkPanel::default()),
    ]
}

pub const DEFAULT_LAYOUT: DesktopLayout = DesktopLayout {
    left_top: &["Drive"],
    left_bottom: &["Motors", "Power"],
    center: &["Map"],
    right_top: &["Video", "Lidar"],
    right_bottom: &["Mission", "Settings", "RTK Base", "Link"],
    right_top_share: 0.36,
};
