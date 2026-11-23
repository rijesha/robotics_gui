//! Top-down map on ArcGIS World Imagery: vehicle, expected path, mission editing, and
//! what the lidar has seen.

use crate::station::{MapTool, OBSTACLE_M};
use crate::widgets::segment;
use crate::{theme, Panel, Station};
use egui::{Align2, Color32, FontId, Mesh, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use rgf_core::geo::body_to_local;
use rgf_core::msgs::rtk::BaseMode;
use rgf_core::msgs::telemetry::NavState;
use rgf_core::{LatLon, Vec2 as Local};
use std::path::PathBuf;
use walkers::sources::{Attribution, TileUrls};
use walkers::{lat_lon, HttpOptions, HttpTiles, Map, MapMemory, Projector, TileId};

/// Esri World Imagery. Note the tile path is zoom/row/column.
pub struct ArcGisWorldImagery;

impl TileUrls for ArcGisWorldImagery {
    fn tile_url(&self, tile: TileId) -> String {
        format!(
            "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{}/{}/{}",
            tile.zoom, tile.y, tile.x
        )
    }

    fn attribution(&self) -> Attribution {
        Attribution {
            text: "Esri, Maxar, Earthstar Geographics, and the GIS User Community",
            url: "https://www.esri.com",
            logo_light: None,
            logo_dark: None,
        }
    }

    fn max_zoom(&self) -> u8 {
        19
    }
}

const HANDLE_PX: f32 = 22.0;

pub struct MapPanel {
    tiles: HttpTiles,
    memory: MapMemory,
    show_terrain: bool,
    show_obstacles: bool,
    show_lidar: bool,
    show_trail: bool,
    show_path: bool,
}

impl MapPanel {
    pub fn new(ctx: &egui::Context, tile_cache: Option<PathBuf>) -> Self {
        let options = HttpOptions { cache: tile_cache, ..Default::default() };
        let mut memory = MapMemory::default();
        let _ = memory.set_zoom(20.0);
        Self {
            tiles: HttpTiles::with_options(ArcGisWorldImagery, options, ctx.clone()),
            memory,
            show_terrain: true,
            show_obstacles: true,
            show_lidar: true,
            show_trail: true,
            show_path: true,
        }
    }
}

/// Maps the station's local frame onto the screen. The map is north-up and the area is
/// small, so a scale and an offset are enough.
struct LocalToScreen {
    origin: Pos2,
    px_per_m: f32,
}

impl LocalToScreen {
    fn pos(&self, v: Local) -> Pos2 {
        self.origin + Vec2::new(v.x as f32, -v.y as f32) * self.px_per_m
    }
}

fn project(projector: &Projector, p: LatLon) -> Pos2 {
    projector.project(lat_lon(p.lat, p.lon)).to_pos2()
}

fn unproject(projector: &Projector, pos: Pos2) -> LatLon {
    let p = projector.unproject(pos.to_vec2());
    LatLon::new(p.y(), p.x())
}

/// What the toolbar asked for; applied once the map no longer borrows its state.
#[derive(Default)]
struct Actions {
    follow: bool,
    zoom_in: bool,
    zoom_out: bool,
    clear_terrain: bool,
}

fn toolbar_frame(ui: &egui::Ui) -> egui::Frame {
    let p = theme::of(ui);
    egui::Frame::new().fill(p.surface).stroke(Stroke::new(1.0, p.border)).corner_radius(8).inner_margin(4)
}

impl Panel for MapPanel {
    fn title(&self) -> &str {
        "Map"
    }

    fn padded(&self) -> bool {
        false
    }

    fn ui(&mut self, ui: &mut egui::Ui, station: &mut Station) {
        let Self { tiles, memory, show_terrain, show_obstacles, show_lidar, show_trail, show_path } = self;
        // Until a vehicle reports in, look at the default field.
        let vehicle = station.vehicle_pos().unwrap_or(LatLon::new(41.9, -93.55));
        let following = memory.detached().is_none();

        let map = Map::new(Some(tiles), memory, lat_lon(vehicle.lat, vehicle.lon)).double_click_to_zoom(false);
        let actions = map
            .show(ui, |ui, response, projector, _memory| {
                let clip = projector.map_rect();
                let painter = ui.painter().with_clip_rect(clip);
                station.anchor("map", response);

                let to_screen = station.frame.map(|frame| LocalToScreen {
                    origin: project(projector, frame.origin),
                    px_per_m: projector.scale_pixel_per_meter(lat_lon(frame.origin.lat, frame.origin.lon)),
                });
                station.map_view = to_screen.as_ref().map(|t| (t.origin, t.px_per_m));

                if let (Some(frame), Some(to_screen)) = (station.frame, &to_screen) {
                    if *show_terrain || *show_obstacles {
                        draw_terrain(&painter, clip, station, to_screen, *show_terrain, *show_obstacles);
                    }
                    if *show_lidar {
                        draw_lidar(&painter, clip, station, &frame, to_screen);
                    }
                }
                if *show_trail && station.trail.len() > 1 {
                    let points: Vec<Pos2> = station.trail.iter().map(|p| project(projector, *p)).collect();
                    painter.line(points, Stroke::new(2.0, theme::VEHICLE.gamma_multiply(0.7)));
                }

                draw_base(&painter, projector, station);
                draw_mission(ui, &painter, projector, station);
                if let (Some(frame), Some(to_screen)) = (station.frame, &to_screen) {
                    draw_vehicle(&painter, station, &frame, to_screen, *show_path);
                }

                // Clicks on the map itself add points in the editing tools.
                if response.clicked() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let at = unproject(projector, pos);
                        match station.plan.tool {
                            MapTool::Waypoints => station.plan.waypoints.push(at),
                            MapTool::Polygon => station.plan.polygon.push(at),
                            MapTool::Pan => {}
                        }
                    }
                }

                draw_scale_and_credit(&painter, projector, clip);
                let tool_hint = match station.plan.tool {
                    MapTool::Pan => None,
                    MapTool::Waypoints => Some("Click to add a waypoint  ·  drag to move  ·  right-click to remove"),
                    MapTool::Polygon => {
                        Some("Click to add a corner of the area  ·  drag to move  ·  right-click to remove")
                    }
                };
                if let Some(text) = tool_hint {
                    let galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(12.5), Color32::WHITE);
                    let rect = Rect::from_center_size(clip.center_bottom() - Vec2::new(0.0, 46.0), galley.size());
                    if rect.width() + 24.0 < clip.width() {
                        painter.rect_filled(rect.expand2(Vec2::new(10.0, 5.0)), 12, Color32::from_black_alpha(190));
                        painter.galley(rect.min, galley, Color32::WHITE);
                    }
                }

                let mut actions = Actions::default();
                let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(clip.shrink(8.0)));
                bar.horizontal_wrapped(|ui| {
                    toolbar_frame(ui).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let tool = &mut station.plan.tool;
                            let pan = segment(ui, tool, MapTool::Pan, "Pan").on_hover_text("Drag to move the map");
                            let waypoints = segment(ui, tool, MapTool::Waypoints, "Waypoints")
                                .on_hover_text("Add and edit mission waypoints");
                            let survey = segment(ui, tool, MapTool::Polygon, "Survey area")
                                .on_hover_text("Outline an area to cover");
                            station.anchor("tool.pan", &pan);
                            station.anchor("tool.waypoints", &waypoints);
                            station.anchor("tool.survey", &survey);
                        });
                    });
                    toolbar_frame(ui).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            let follow =
                                ui.selectable_label(following, "Follow").on_hover_text("Keep the vehicle centred");
                            station.anchor("map.follow", &follow);
                            actions.follow = follow.clicked();
                            actions.zoom_in = ui.button("+").on_hover_text("Zoom in").clicked();
                            actions.zoom_out = ui.button("−").on_hover_text("Zoom out").clicked();
                            ui.menu_button("Layers", |ui| {
                                ui.checkbox(show_path, "Expected path");
                                ui.checkbox(show_trail, "Trail");
                                ui.checkbox(show_lidar, "Lidar returns");
                                ui.checkbox(show_obstacles, "Obstacles");
                                ui.checkbox(show_terrain, "Terrain height");
                                ui.separator();
                                actions.clear_terrain = ui.button("Clear lidar map").clicked();
                            });
                        });
                    });
                });
                actions
            })
            .inner;

        if actions.follow {
            self.memory.follow_my_position();
        }
        if actions.zoom_in {
            let _ = self.memory.zoom_in();
        }
        if actions.zoom_out {
            let _ = self.memory.zoom_out();
        }
        if actions.clear_terrain {
            station.terrain.clear();
        }
    }
}

fn draw_terrain(
    painter: &egui::Painter,
    clip: Rect,
    station: &Station,
    to_screen: &LocalToScreen,
    heights: bool,
    obstacles: bool,
) {
    let grid = &station.terrain;
    let cell_px = grid.cell_m as f32 * to_screen.px_per_m;
    // Zoomed far out the cells are smaller than a pixel and only add cost.
    if cell_px < 0.75 {
        return;
    }
    let Some((lo, hi)) = grid.ground_range() else { return };
    let span = (hi - lo).max(0.5);
    let mut mesh = Mesh::default();
    for (key, cell) in &grid.cells {
        let corner = to_screen.pos(grid.cell_origin(*key));
        let rect = Rect::from_min_max(Pos2::new(corner.x, corner.y - cell_px), Pos2::new(corner.x + cell_px, corner.y));
        if !clip.intersects(rect) {
            continue;
        }
        if cell.is_obstacle(OBSTACLE_M) {
            if obstacles {
                mesh.add_colored_rect(rect, Color32::from_rgba_unmultiplied(255, 40, 40, 220));
            }
        } else if heights {
            let c = theme::ramp((cell.min_h - lo) / span);
            mesh.add_colored_rect(rect, Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 95));
        }
    }
    painter.add(Shape::mesh(mesh));

    if heights {
        // Legend for the height colours.
        let bar = Rect::from_min_size(clip.right_top() + Vec2::new(-26.0, 56.0), Vec2::new(12.0, 110.0));
        let mut legend = Mesh::default();
        for i in 0..22 {
            let t = i as f32 / 21.0;
            let y = bar.bottom() - bar.height() * (i as f32 + 1.0) / 22.0;
            legend.add_colored_rect(
                Rect::from_min_size(Pos2::new(bar.left(), y), Vec2::new(bar.width(), bar.height() / 22.0 + 0.5)),
                theme::ramp(t),
            );
        }
        painter.rect_filled(bar.expand(3.0), 3, Color32::from_black_alpha(150));
        painter.add(Shape::mesh(legend));
        let font = FontId::proportional(11.0);
        painter.text(
            bar.left_top() + Vec2::new(-6.0, 0.0),
            Align2::RIGHT_TOP,
            format!("{hi:.1} m"),
            font.clone(),
            Color32::WHITE,
        );
        painter.text(
            bar.left_bottom() + Vec2::new(-6.0, 0.0),
            Align2::RIGHT_BOTTOM,
            format!("{lo:.1} m"),
            font,
            Color32::WHITE,
        );
    }
}

fn draw_lidar(
    painter: &egui::Painter,
    clip: Rect,
    station: &Station,
    frame: &rgf_core::LocalFrame,
    to_screen: &LocalToScreen,
) {
    let Some(pose) = &station.client.pose else { return };
    let origin = frame.to_local(LatLon::new(pose.lat, pose.lon));
    let mut mesh = Mesh::default();
    for p in &station.lidar_points {
        let at = to_screen.pos(body_to_local(origin, pose.heading_rad as f64, p.forward as f64, p.right as f64));
        if clip.contains(at) {
            mesh.add_colored_rect(Rect::from_center_size(at, Vec2::splat(2.5)), theme::ramp(p.up / 3.0));
        }
    }
    painter.add(Shape::mesh(mesh));
}

fn draw_vehicle(
    painter: &egui::Painter,
    station: &Station,
    frame: &rgf_core::LocalFrame,
    to_screen: &LocalToScreen,
    show_path: bool,
) {
    let Some(pose) = &station.client.pose else { return };
    let heading = pose.heading_rad as f64;
    let center = frame.to_local(LatLon::new(pose.lat, pose.lon));
    let at = |forward: f64, right: f64| to_screen.pos(body_to_local(center, heading, forward, right));

    if show_path {
        if let Some(path) = station.expected_path() {
            let points: Vec<Pos2> = path.points.iter().map(|(f, r)| at(*f, *r)).collect();
            if path.nominal {
                // Stopped: show where the current steering would lead, as a hint.
                let stroke = Stroke::new(2.0, theme::PATH.gamma_multiply(0.6));
                painter.extend(Shape::dashed_line(&points[..points.len().min(30)], stroke, 6.0, 5.0));
            } else {
                painter.line(points.clone(), Stroke::new(5.0, Color32::from_black_alpha(120)));
                painter.line(points, Stroke::new(3.0, theme::PATH));
            }
        }
    }

    let (length, width) = station.client.info.as_ref().map_or((1.0, 0.7), |i| (i.length_m as f64, i.width_m as f64));
    // Never draw the vehicle smaller than a fingertip, however far out the map is zoomed.
    let scale = (22.0 / (length as f32 * to_screen.px_per_m)).max(1.0) as f64;
    let (half_l, half_w) = (length * scale / 2.0, width * scale / 2.0);
    let alive = station.client.vehicle_alive(station.now);
    let fill = if alive { theme::VEHICLE } else { Color32::from_gray(150) };
    // An arrowhead: nose, right tail, notch, left tail.
    let (nose, right, notch, left) =
        (at(half_l, 0.0), at(-half_l, half_w), at(-half_l * 0.45, 0.0), at(-half_l, -half_w));
    painter.add(Shape::convex_polygon(vec![nose, right, notch], fill, Stroke::NONE));
    painter.add(Shape::convex_polygon(vec![nose, notch, left], fill, Stroke::NONE));
    painter.add(Shape::closed_line(vec![nose, right, notch, left], Stroke::new(2.0, Color32::WHITE)));
}

fn draw_base(painter: &egui::Painter, projector: &Projector, station: &Station) {
    let Some(base) = &station.client.base else { return };
    if base.mode == BaseMode::BASE_IDLE {
        return;
    }
    let at = project(projector, LatLon::new(base.lat, base.lon));
    let color = if base.mode == BaseMode::BASE_FIXED { theme::DARK.ok } else { theme::DARK.warn };
    let triangle = vec![at + Vec2::new(0.0, -10.0), at + Vec2::new(9.0, 7.0), at + Vec2::new(-9.0, 7.0)];
    painter.add(Shape::convex_polygon(triangle, color, Stroke::new(1.5, Color32::BLACK)));
    painter.text(at + Vec2::new(0.0, 10.0), Align2::CENTER_TOP, "RTK base", FontId::proportional(11.0), Color32::WHITE);
}

fn draw_mission(ui: &mut egui::Ui, painter: &egui::Painter, projector: &Projector, station: &mut Station) {
    let font = FontId::proportional(11.0);

    // What the vehicle is executing.
    if !station.plan.uploaded.is_empty() {
        let points: Vec<Pos2> = station.plan.uploaded.iter().map(|p| project(projector, *p)).collect();
        painter.line(points.clone(), Stroke::new(5.0, theme::DARK.ok.gamma_multiply(0.55)));
        if let Some(nav) = &station.client.nav {
            if nav.state == NavState::NAV_RUNNING || nav.state == NavState::NAV_PAUSED {
                if let Some(target) = points.get(nav.active_wp as usize) {
                    painter.circle_stroke(*target, 13.0, Stroke::new(2.5, theme::DARK.ok));
                }
            }
        }
    }

    // Survey area and the passes it would produce.
    let polygon: Vec<Pos2> = station.plan.polygon.iter().map(|p| project(projector, *p)).collect();
    if polygon.len() >= 2 {
        painter.add(Shape::closed_line(polygon.clone(), Stroke::new(2.0, theme::SURVEY)));
    }
    if polygon.len() >= 3 {
        let preview: Vec<Pos2> = station.plan.survey_path().iter().map(|p| project(projector, *p)).collect();
        painter.line(preview, Stroke::new(1.5, theme::SURVEY.gamma_multiply(0.75)));
    }
    edit_handles(ui, painter, projector, &mut station.plan.polygon, "poly", |painter, at, _| {
        painter.rect_filled(Rect::from_center_size(at, Vec2::splat(10.0)), 2, theme::SURVEY);
        painter.rect_stroke(
            Rect::from_center_size(at, Vec2::splat(10.0)),
            2,
            Stroke::new(1.0, Color32::BLACK),
            egui::StrokeKind::Outside,
        );
    });

    // The mission being edited.
    let waypoints: Vec<Pos2> = station.plan.waypoints.iter().map(|p| project(projector, *p)).collect();
    if waypoints.len() > 1 {
        painter.line(waypoints.clone(), Stroke::new(2.0, theme::MISSION));
    }
    // Hundreds of survey waypoints would bury the map in labels; number only short missions.
    let numbered = waypoints.len() <= 60;
    edit_handles(ui, painter, projector, &mut station.plan.waypoints, "wp", |painter, at, index| {
        if numbered {
            painter.circle(at, 9.0, Color32::from_rgb(30, 30, 34), Stroke::new(1.5, theme::MISSION));
            painter.text(at, Align2::CENTER_CENTER, index + 1, font.clone(), Color32::WHITE);
        } else {
            painter.circle_filled(at, 3.0, theme::MISSION);
        }
    });
}

/// Draw draggable, right-click-deletable handles for a list of positions.
fn edit_handles(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    projector: &Projector,
    points: &mut Vec<LatLon>,
    id: &str,
    draw: impl Fn(&egui::Painter, Pos2, usize),
) {
    let mut remove = None;
    for index in 0..points.len() {
        let at = project(projector, points[index]);
        let rect = Rect::from_center_size(at, Vec2::splat(HANDLE_PX));
        let response = ui.interact(rect, ui.scope_id().with((id, index)), Sense::click_and_drag());
        if response.dragged() {
            if let Some(pointer) = response.interact_pointer_pos() {
                points[index] = unproject(projector, pointer);
            }
        }
        if response.secondary_clicked() {
            remove = Some(index);
        }
        draw(painter, at, index);
    }
    if let Some(index) = remove {
        points.remove(index);
    }
}

fn draw_scale_and_credit(painter: &egui::Painter, projector: &Projector, clip: Rect) {
    let center = projector.unproject(clip.center().to_vec2());
    let px_per_m = projector.scale_pixel_per_meter(center);
    if px_per_m > 0.0 {
        // Largest 1/2/5 x 10^n metres that fits in about 120 pixels.
        let target = 120.0 / px_per_m;
        let magnitude = 10f32.powf(target.log10().floor());
        let nice = [5.0, 2.0, 1.0].into_iter().map(|k| k * magnitude).find(|m| *m <= target).unwrap_or(magnitude);
        let left = clip.left_bottom() + Vec2::new(12.0, -14.0);
        let right = left + Vec2::new(nice * px_per_m, 0.0);
        painter.line_segment([left, right], Stroke::new(4.0, Color32::BLACK));
        painter.line_segment([left, right], Stroke::new(2.0, Color32::WHITE));
        let label = if nice >= 1000.0 { format!("{} km", nice / 1000.0) } else { format!("{nice} m") };
        painter.text(
            left + Vec2::new(0.0, -6.0),
            Align2::LEFT_BOTTOM,
            label,
            FontId::proportional(12.0),
            Color32::WHITE,
        );
    }
    let credit = "Imagery: Esri, Maxar, Earthstar Geographics";
    let galley = painter.layout_no_wrap(credit.to_owned(), FontId::proportional(10.0), Color32::WHITE);
    let rect = Rect::from_min_size(clip.right_bottom() - galley.size() - Vec2::new(6.0, 4.0), galley.size());
    painter.rect_filled(rect.expand(2.0), 2, Color32::from_black_alpha(140));
    painter.galley(rect.min, galley, Color32::WHITE);
}
