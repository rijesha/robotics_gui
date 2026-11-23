//! Robotics GUI framework.
//!
//! A front end is a set of [`Panel`]s arranged by a [`Workspace`], all reading and
//! commanding one [`Station`], which holds every vehicle and shows the selected one. [`StationApp`] is the stock arrangement with every
//! built-in panel; build your own `Workspace` to add or replace panels.

pub mod app;
pub mod fleet;
pub mod hud;
pub mod link;
pub mod panels;
pub mod station;
pub mod theme;
pub mod widgets;
pub mod workspace;

pub use app::StationApp;
pub use station::Station;
pub use workspace::{Layout, Workspace};

/// One dockable view. Implement this to add your own.
pub trait Panel {
    fn title(&self) -> &str;

    fn ui(&mut self, ui: &mut egui::Ui, station: &mut Station);

    /// Panels that fill their area themselves (maps, 3D views) return false to get the
    /// whole pane with no margin or scrolling.
    fn padded(&self) -> bool {
        true
    }
}
