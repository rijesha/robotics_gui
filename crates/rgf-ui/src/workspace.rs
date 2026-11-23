//! Arranges panels: a dockable tile layout on wide screens, one panel at a time with a
//! tab bar on phones.

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

pub struct Workspace {
    panels: Vec<Box<dyn Panel>>,
    tree: Tree<usize>,
    layout: DesktopLayout,
    phone_tab: usize,
}

impl Workspace {
    pub fn new(panels: Vec<Box<dyn Panel>>, layout: DesktopLayout) -> Self {
        let tree = build_tree(&panels, &layout);
        Self { panels, tree, layout, phone_tab: 0 }
    }

    pub fn reset_layout(&mut self) {
        self.tree = build_tree(&self.panels, &self.layout);
    }

    /// Bring a panel to the front, by title.
    pub fn show_panel(&mut self, title: &str) {
        let Some(index) = self.panels.iter().position(|p| p.title() == title) else { return };
        self.phone_tab = index;
        if let Some(tile) = self.tree.tiles.find_pane(&index) {
            self.tree.make_active(|id, _| id == tile);
        }
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
        } else {
            let mut behavior = Behavior { panels: &mut self.panels, station };
            self.tree.ui(&mut behavior, ui);
        }
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
        egui::Frame::new().fill(ui.visuals().panel_fill).inner_margin(8).show(ui, |ui| {
            egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| panel.ui(ui, station));
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
    let left = column(&mut tiles, vec![left_top, left_bottom], 0.45);
    let middle = column(&mut tiles, vec![center, extra], 0.75);
    let right = column(&mut tiles, vec![right_top, right_bottom], 0.42);

    let columns: Vec<(TileId, f32)> =
        [(left, 0.23), (middle, 0.5), (right, 0.27)].into_iter().filter_map(|(id, share)| Some((id?, share))).collect();
    let mut root = Linear::new(LinearDir::Horizontal, columns.iter().map(|(id, _)| *id).collect());
    for (id, share) in &columns {
        root.shares.set_share(*id, *share);
    }
    let root = tiles.insert_new(Tile::Container(Container::Linear(root)));
    Tree::new(egui::Id::unique("rgf_workspace"), root, tiles)
}
