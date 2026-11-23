//! Arranges panels: a dockable tile layout or a map with everything floating over it on
//! wide screens, one panel at a time with a tab bar on phones.

use crate::hud::{Hidden, Hud};
use crate::{Panel, Station};
use egui_tiles::{Container, Linear, LinearDir, Tile, TileId, Tiles, Tree};

/// Below this width the workspace switches to the phone layout.
pub const PHONE_WIDTH: f32 = 700.0;

/// How the built-in desktop layout groups panels, by title.
pub struct DesktopLayout {
    pub left_top: &'static [&'static str],
    pub left_bottom: &'static [&'static str],
    pub center: &'static [&'static str],
    pub right_top: &'static [&'static str],
    pub right_bottom: &'static [&'static str],
}

/// How a wide screen is arranged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Panels docked side by side in tiles.
    Dock,
    /// The map fills the window; the essentials are pinned over it and any other panel
    /// opens as a floating window.
    Map,
}

/// Title of the panel the map layout puts behind everything else.
const MAP_TITLE: &str = "Map";

pub struct Workspace {
    panels: Vec<Box<dyn Panel>>,
    tree: Tree<usize>,
    desktop: DesktopLayout,
    phone_tab: usize,
    pub layout: Layout,
    /// In the map layout, which panels are open as floating windows.
    floating: Vec<bool>,
    hud: Hud,
}

impl Workspace {
    pub fn new(panels: Vec<Box<dyn Panel>>, layout: DesktopLayout) -> Self {
        let tree = build_tree(&panels, &layout);
        let floating = vec![false; panels.len()];
        Self { panels, tree, desktop: layout, phone_tab: 0, layout: Layout::Dock, floating, hud: Hud::default() }
    }

    pub fn reset_layout(&mut self) {
        self.tree = build_tree(&self.panels, &self.desktop);
        self.floating.fill(false);
    }

    fn index_of(&self, title: &str) -> Option<usize> {
        self.panels.iter().position(|p| p.title() == title)
    }

    /// Whether a panel is open as a floating window in the map layout.
    pub fn is_floating(&self, title: &str) -> bool {
        self.index_of(title).is_some_and(|index| self.floating[index])
    }

    /// Open or close a panel's floating window in the map layout.
    pub fn set_floating(&mut self, title: &str, open: bool) {
        if let Some(index) = self.index_of(title).filter(|_| title != MAP_TITLE) {
            self.floating[index] = open;
        }
    }

    /// Bring a panel to the front, by title.
    pub fn show_panel(&mut self, title: &str) {
        let Some(index) = self.index_of(title) else { return };
        self.phone_tab = index;
        if self.layout == Layout::Map {
            self.set_floating(title, true);
        }
        if let Some(tile) = self.tree.tiles.find_pane(&index) {
            self.tree.make_active(|id, _| id == tile);
        }
    }

    /// Titles of all panels, in registration order.
    pub fn titles(&self) -> Vec<String> {
        self.panels.iter().map(|p| p.title().to_owned()).collect()
    }

    pub fn is_phone(ui: &egui::Ui) -> bool {
        ui.ctx().content_rect().width() < PHONE_WIDTH
    }

    /// Title of the panel showing in the phone layout.
    pub fn phone_panel(&self) -> &str {
        self.panels[self.phone_tab].title()
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, station: &mut Station) {
        if Self::is_phone(ui) {
            self.phone_ui(ui, station);
        } else if let (Layout::Map, Some(map)) = (self.layout, self.index_of(MAP_TITLE)) {
            self.map_ui(ui, station, map);
        } else {
            let mut behavior = Behavior { panels: &mut self.panels, station };
            self.tree.ui(&mut behavior, ui);
        }
    }

    fn map_ui(&mut self, ui: &mut egui::Ui, station: &mut Station, map: usize) {
        let area = ui.max_rect();
        self.panels[map].ui(ui, station);

        let hidden = Hidden {
            mission: self.is_floating("Mission"),
            video: self.is_floating("Video"),
            motors: self.is_floating("Motors"),
            drive: self.is_floating("Drive"),
        };
        self.hud.ui(ui, area, station, hidden);
        self.panel_rail(ui, area);

        let p = crate::theme::of(ui);
        let fill = egui::Color32::from_rgba_unmultiplied(p.bg.r(), p.bg.g(), p.bg.b(), 242);
        let frame = egui::Frame::window(ui.style()).fill(fill);
        let mut opened = 0;
        for (index, panel) in self.panels.iter_mut().enumerate() {
            if !self.floating[index] {
                continue;
            }
            // Windows open beside the pinned cards and left of the vehicle in the middle of
            // the map, each a step down and right of the one before.
            let step = 26.0 * opened as f32;
            opened += 1;
            let size = if panel.padded() { egui::vec2(380.0, 540.0) } else { egui::vec2(440.0, 300.0) };
            let padded = panel.padded();
            egui::Window::new(panel.title())
                .id(egui::Id::unique(("floating_panel", index)))
                .frame(frame)
                .open(&mut self.floating[index])
                .default_pos(area.left_top() + egui::vec2(250.0 + step, 56.0 + step))
                .default_size(size)
                .constrain_to(area)
                .show(ui.ctx(), |ui| {
                    if padded {
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| panel.ui(ui, station));
                    } else {
                        panel.ui(ui, station);
                    }
                });
        }
    }

    /// Buttons along the top of the map that open and close each panel's floating window.
    fn panel_rail(&mut self, ui: &mut egui::Ui, area: egui::Rect) {
        let mut rail = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("panel_rail")
                .max_rect(area.shrink(8.0))
                .layout(egui::Layout::right_to_left(egui::Align::Min)),
        );
        // A narrow map has no room for a button per panel beside its own toolbar.
        let compact = area.width() < 1150.0;
        crate::panels::map::toolbar_frame(&rail).show(&mut rail, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let mut toggle = |ui: &mut egui::Ui, index: usize| {
                    let (panel, open) = (&self.panels[index], &mut self.floating[index]);
                    if panel.title() != MAP_TITLE && ui.selectable_label(*open, panel.title()).clicked() {
                        *open = !*open;
                    }
                };
                let count = self.panels.len();
                if compact {
                    ui.menu_button("Panels", |ui| (0..count).for_each(|index| toggle(ui, index)));
                } else {
                    // Laid out from the right edge, so added last to first.
                    (0..count).rev().for_each(|index| toggle(ui, index));
                }
            });
        });
    }

    fn phone_ui(&mut self, ui: &mut egui::Ui, station: &mut Station) {
        egui::Panel::bottom("phone_tabs").show(ui, |ui| {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().button_padding = egui::vec2(12.0, 10.0);
                    for (i, panel) in self.panels.iter().enumerate() {
                        if ui.selectable_label(self.phone_tab == i, panel.title()).clicked() {
                            self.phone_tab = i;
                        }
                    }
                });
            });
        });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(ui.visuals().panel_fill)).show(ui, |ui| {
            // Touch targets need more room than mouse targets.
            ui.spacing_mut().interact_size.y = 32.0;
            ui.spacing_mut().button_padding = egui::vec2(10.0, 8.0);
            show_panel(self.panels[self.phone_tab].as_mut(), ui, station);
        });
    }
}

fn show_panel(panel: &mut dyn Panel, ui: &mut egui::Ui, station: &mut Station) {
    if panel.padded() {
        // Fill the pane: the window behind is not guaranteed to be opaque.
        egui::Frame::new().fill(ui.visuals().panel_fill).inner_margin(10).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| panel.ui(ui, station));
        });
    } else {
        panel.ui(ui, station);
    }
}

struct Behavior<'a> {
    panels: &'a mut Vec<Box<dyn Panel>>,
    station: &'a mut Station,
}

impl egui_tiles::Behavior<usize> for Behavior<'_> {
    fn pane_ui(&mut self, ui: &mut egui::Ui, _tile_id: TileId, pane: &mut usize) -> egui_tiles::UiResponse {
        show_panel(self.panels[*pane].as_mut(), ui, self.station);
        egui_tiles::UiResponse::None
    }

    fn tab_title_for_pane(&mut self, pane: &usize) -> egui::WidgetText {
        self.panels[*pane].title().into()
    }

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        30.0
    }

    fn tab_bar_color(&self, visuals: &egui::Visuals) -> egui::Color32 {
        crate::theme::palette(visuals.dark_mode).surface
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        2.0
    }

    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        // Keep the tab bar on single panels so every panel has a title to drag it by.
        egui_tiles::SimplificationOptions { all_panes_must_have_tabs: true, ..Default::default() }
    }
}

fn build_tree(panels: &[Box<dyn Panel>], layout: &DesktopLayout) -> Tree<usize> {
    let mut tiles = Tiles::default();
    let mut placed = vec![false; panels.len()];
    let mut tabs = |tiles: &mut Tiles<usize>, titles: &[&str]| -> Option<TileId> {
        let ids: Vec<TileId> = titles
            .iter()
            .filter_map(|title| panels.iter().position(|p| p.title() == *title))
            .map(|index| {
                placed[index] = true;
                tiles.insert_pane(index)
            })
            .collect();
        (!ids.is_empty()).then(|| tiles.insert_tab_tile(ids))
    };

    let left_top = tabs(&mut tiles, layout.left_top);
    let left_bottom = tabs(&mut tiles, layout.left_bottom);
    let center = tabs(&mut tiles, layout.center);
    let right_top = tabs(&mut tiles, layout.right_top);
    let right_bottom = tabs(&mut tiles, layout.right_bottom);
    // Panels the layout does not mention still get a home.
    let leftovers: Vec<TileId> = (0..panels.len()).filter(|i| !placed[*i]).map(|i| tiles.insert_pane(i)).collect();
    let extra = (!leftovers.is_empty()).then(|| tiles.insert_tab_tile(leftovers));

    let column = |tiles: &mut Tiles<usize>, parts: Vec<Option<TileId>>, top_share: f32| -> Option<TileId> {
        let children: Vec<TileId> = parts.into_iter().flatten().collect();
        match children.len() {
            0 => None,
            1 => Some(children[0]),
            _ => {
                let mut linear = Linear::new(LinearDir::Vertical, children.clone());
                linear.shares.set_share(children[0], top_share);
                linear.shares.set_share(children[1], 1.0 - top_share);
                Some(tiles.insert_new(Tile::Container(Container::Linear(linear))))
            }
        }
    };
    let left = column(&mut tiles, vec![left_top, left_bottom], 0.5);
    let middle = column(&mut tiles, vec![center, extra], 0.75);
    let right = column(&mut tiles, vec![right_top, right_bottom], 0.36);

    let columns: Vec<(TileId, f32)> = [(left, 0.25), (middle, 0.46), (right, 0.29)]
        .into_iter()
        .filter_map(|(id, share)| Some((id?, share)))
        .collect();
    let mut root = Linear::new(LinearDir::Horizontal, columns.iter().map(|(id, _)| *id).collect());
    for (id, share) in &columns {
        root.shares.set_share(*id, *share);
    }
    let root = tiles.insert_new(Tile::Container(Container::Linear(root)));
    Tree::new(egui::Id::unique("rgf_workspace"), root, tiles)
}
