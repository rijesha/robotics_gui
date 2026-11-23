//! 3D view of the latest lidar scan. Drag to orbit, scroll to zoom.

use crate::{theme, Panel, Station};
use egui::{Align2, FontId, Mesh, Pos2, Rect, Sense, Shape, Stroke, Vec2};

pub struct LidarPanel {
    yaw: f32,
    pitch: f32,
    distance: f32,
}

impl Default for LidarPanel {
    fn default() -> Self {
        Self { yaw: 0.0, pitch: 0.6, distance: 28.0 }
    }
}

/// Orbit camera looking at the vehicle from behind and above.
struct View {
    center: Pos2,
    focal_px: f32,
    yaw: (f32, f32),
    pitch: (f32, f32),
    distance: f32,
}

impl View {
    /// Project a body-frame point (forward, right, up) to the screen.
    fn project(&self, forward: f32, right: f32, up: f32) -> Option<Pos2> {
        let (sin_y, cos_y) = self.yaw;
        let (sin_p, cos_p) = self.pitch;
        let x = right * cos_y - forward * sin_y;
        let y = right * sin_y + forward * cos_y;
        let depth = self.distance + y * cos_p - up * sin_p;
        let screen_up = y * sin_p + up * cos_p;
        (depth > 0.5).then(|| self.center + Vec2::new(x, -screen_up) * (self.focal_px / depth))
    }
}

impl Panel for LidarPanel {
    fn title(&self) -> &str {
        "Lidar"
    }

    fn padded(&self) -> bool {
        false
    }

    fn ui(&mut self, ui: &mut egui::Ui, station: &mut Station) {
        let (area, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        if response.dragged() {
            let delta = response.drag_delta();
            self.yaw += delta.x * 0.01;
            self.pitch = (self.pitch + delta.y * 0.01).clamp(0.05, std::f32::consts::FRAC_PI_2);
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            self.distance = (self.distance * (1.0 - scroll * 0.002)).clamp(4.0, 90.0);
        }
        if response.double_clicked() {
            *self = Self::default();
        }

        let p = theme::of(ui);
        let painter = ui.painter_at(area);
        painter.rect_filled(area, 0, p.viewport);
        let view = View {
            center: area.center() + Vec2::new(0.0, area.height() * 0.08),
            focal_px: area.height() * 1.1,
            yaw: self.yaw.sin_cos(),
            pitch: self.pitch.sin_cos(),
            distance: self.distance,
        };

        // Range rings on the ground.
        let grid = Stroke::new(1.0, p.viewport_grid);
        for radius in [5.0f32, 10.0, 20.0, 30.0] {
            let ring: Vec<Pos2> = (0..=72)
                .filter_map(|i| {
                    let a = i as f32 * std::f32::consts::TAU / 72.0;
                    view.project(radius * a.cos(), radius * a.sin(), 0.0)
                })
                .collect();
            painter.line(ring, grid);
            if let Some(at) = view.project(0.0, radius, 0.0) {
                painter.text(at, Align2::LEFT_BOTTOM, format!("{radius:.0} m"), FontId::proportional(10.0), p.dim);
            }
        }

        if station.lidar_points.is_empty() {
            painter.text(area.center(), Align2::CENTER_CENTER, "No lidar data", FontId::proportional(16.0), p.dim);
        }
        let mut mesh = Mesh::default();
        for p in &station.lidar_points {
            if let Some(at) = view.project(p.forward, p.right, p.up) {
                if area.contains(at) {
                    mesh.add_colored_rect(
                        Rect::from_center_size(at, Vec2::splat(2.5)),
                        theme::ramp((p.up + 0.5) / 4.0),
                    );
                }
            }
        }
        painter.add(Shape::mesh(mesh));

        // Vehicle footprint and where it is heading.
        if let Some(info) = &station.client.info {
            let (l, w) = (info.length_m / 2.0, info.width_m / 2.0);
            let corners = [(l, -w), (l, w), (-l, w), (-l, -w), (l, -w)];
            let outline: Vec<Pos2> = corners.iter().filter_map(|(f, r)| view.project(*f, *r, 0.0)).collect();
            painter.line(outline, Stroke::new(2.0, p.accent));
            if let (Some(a), Some(b)) = (view.project(l, 0.0, 0.0), view.project(l * 1.8, 0.0, 0.0)) {
                painter.line_segment([a, b], Stroke::new(2.0, p.accent));
            }
        }
        if let Some(path) = station.expected_path() {
            let points: Vec<Pos2> =
                path.points.iter().filter_map(|(f, r)| view.project(*f as f32, *r as f32, 0.0)).collect();
            // Yellow disappears on a light background.
            let path_color = if ui.visuals().dark_mode { theme::PATH } else { p.warn };
            let color = if path.nominal { path_color.gamma_multiply(0.45) } else { path_color };
            painter.line(points, Stroke::new(2.0, color));
        }

        // Height legend.
        let font = FontId::proportional(10.0);
        let legend = Rect::from_min_size(area.left_top() + Vec2::new(8.0, 8.0), Vec2::new(90.0, 8.0));
        let mut bar = Mesh::default();
        for i in 0..30 {
            let slice = Rect::from_min_size(legend.min + Vec2::new(i as f32 * 3.0, 0.0), Vec2::new(3.5, 8.0));
            bar.add_colored_rect(slice, theme::ramp(i as f32 / 29.0));
        }
        painter.add(Shape::mesh(bar));
        painter.text(legend.left_bottom() + Vec2::new(0.0, 2.0), Align2::LEFT_TOP, "ground", font.clone(), p.dim);
        painter.text(legend.right_bottom() + Vec2::new(0.0, 2.0), Align2::RIGHT_TOP, "3.5 m up", font.clone(), p.dim);
        painter.text(
            area.right_bottom() + Vec2::new(-8.0, -6.0),
            Align2::RIGHT_BOTTOM,
            format!("{} returns · drag to orbit · scroll to zoom", station.lidar_points.len()),
            font,
            p.dim,
        );
    }
}
