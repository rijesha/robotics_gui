//! Camera view with overlays: heading, attitude, expected path and lidar obstacles.

use crate::panels::drive::mode_name;
use crate::widgets::fit_rect;
use crate::{theme, Panel, Station};
use egui::{Align2, Color32, FontId, Mesh, Pos2, Rect, Sense, Shape, Stroke, Vec2};

pub struct VideoPanel {
    show_hud: bool,
    show_path: bool,
    show_lidar: bool,
}

impl Default for VideoPanel {
    fn default() -> Self {
        Self { show_hud: true, show_path: true, show_lidar: true }
    }
}

/// Pinhole projection from the vehicle's body frame into the video image.
struct Camera {
    image: Rect,
    focal_px: f32,
    height_m: f32,
}

impl Camera {
    /// Screen position of a point `forward`, `right` and `up` (above ground) of the vehicle.
    fn project(&self, forward: f32, right: f32, up: f32) -> Option<Pos2> {
        (forward > 0.4)
            .then(|| self.image.center() + Vec2::new(right / forward, -(up - self.height_m) / forward) * self.focal_px)
    }
}

impl Panel for VideoPanel {
    fn title(&self) -> &str {
        "Video"
    }

    fn padded(&self) -> bool {
        false
    }

    fn ui(&mut self, ui: &mut egui::Ui, station: &mut Station) {
        let (area, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
        let painter = ui.painter_at(area);
        painter.rect_filled(area, 0, Color32::BLACK);

        let (Some(texture), Some(video)) = (&station.video_texture, &station.client.video) else {
            painter.text(area.center(), Align2::CENTER_CENTER, "No video", FontId::proportional(16.0), theme::DIM);
            return;
        };
        let size = texture.size_vec2();
        let image = fit_rect(area, size.x / size.y);
        painter.image(texture.id(), image, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        let painter = painter.with_clip_rect(image);
        let camera = Camera {
            image,
            focal_px: image.width() / 2.0 / (video.hfov_rad / 2.0).tan(),
            height_m: video.mount_height_m,
        };

        if self.show_lidar {
            // Mark what the lidar sees standing above the ground; nearer returns are redder.
            let mut mesh = Mesh::default();
            let obstacles =
                station.lidar_points.iter().zip(&station.lidar_obstacle).filter(|(_, o)| **o).map(|(p, _)| p);
            for p in obstacles {
                if let Some(at) = camera.project(p.forward, p.right, p.up) {
                    let range = p.forward.hypot(p.right);
                    let size = (40.0 / range).clamp(2.0, 7.0);
                    let c = theme::ramp(1.0 - range / 25.0);
                    mesh.add_colored_rect(
                        Rect::from_center_size(at, Vec2::splat(size)),
                        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 200),
                    );
                }
            }
            painter.add(Shape::mesh(mesh));
        }

        if self.show_path {
            if let Some(path) = station.expected_path() {
                let half_width = station.client.info.as_ref().map_or(0.4, |i| i.width_m / 2.0);
                let color = if path.nominal { theme::PATH.gamma_multiply(0.45) } else { theme::PATH };
                // Draw the two wheel tracks on the ground, offset sideways along the path normal.
                for side in [-1.0f32, 1.0] {
                    let mut track = Vec::new();
                    for pair in path.points.windows(2) {
                        let (a, b) = (pair[0], pair[1]);
                        let (df, dr) = ((b.0 - a.0) as f32, (b.1 - a.1) as f32);
                        let len = df.hypot(dr).max(1e-6);
                        let (forward, right) =
                            (b.0 as f32 - dr / len * half_width * side, b.1 as f32 + df / len * half_width * side);
                        // Points too close to the lens to project are left out.
                        track.extend(camera.project(forward, right, 0.0));
                    }
                    painter.line(track, Stroke::new(2.5, color));
                }
            }
        }

        if self.show_hud {
            if let Some(pose) = &station.client.pose {
                hud(&painter, image, pose, station);
            }
        }

        // Overlay switches sit on top of the picture.
        let mut bar = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_size(area.left_bottom() + Vec2::new(6.0, -28.0), Vec2::new(260.0, 24.0))),
        );
        bar.horizontal(|ui| {
            ui.toggle_value(&mut self.show_hud, "HUD");
            ui.toggle_value(&mut self.show_path, "Path");
            ui.toggle_value(&mut self.show_lidar, "Lidar");
        });
    }
}

fn hud(painter: &egui::Painter, image: Rect, pose: &rgf_core::msgs::telemetry::Pose, station: &Station) {
    let white = Color32::from_white_alpha(230);
    let shadow = Color32::from_black_alpha(160);
    let text = |pos: Pos2, anchor: Align2, s: String, size: f32| {
        painter.text(pos + Vec2::splat(1.0), anchor, &s, FontId::monospace(size), shadow);
        painter.text(pos, anchor, s, FontId::monospace(size), white);
    };

    // Boresight mark where the camera is pointing.
    let center = image.center();
    for dir in [Vec2::X, Vec2::Y] {
        painter.line_segment([center + dir * 5.0, center + dir * 14.0], Stroke::new(1.5, white));
        painter.line_segment([center - dir * 5.0, center - dir * 14.0], Stroke::new(1.5, white));
    }
    text(
        image.left_bottom() + Vec2::new(8.0, -34.0),
        Align2::LEFT_BOTTOM,
        format!("roll {:+3.0}°  pitch {:+3.0}°", pose.roll_rad.to_degrees(), pose.pitch_rad.to_degrees()),
        12.0,
    );

    // Heading tape: a tick every 10 degrees, labelled every 30.
    let heading = pose.heading_rad.to_degrees().rem_euclid(360.0);
    let px_per_deg = image.width() / 90.0;
    let top = image.top() + 8.0;
    let first = ((heading - 45.0) / 10.0).ceil() as i32;
    for step in first..=((heading + 45.0) / 10.0).floor() as i32 {
        let degrees = step * 10;
        let x = image.center().x + (degrees as f32 - heading) * px_per_deg;
        let major = degrees.rem_euclid(30) == 0;
        painter.line_segment(
            [Pos2::new(x, top), Pos2::new(x, top + if major { 10.0 } else { 5.0 })],
            Stroke::new(1.5, white),
        );
        if major {
            let label = match degrees.rem_euclid(360) {
                0 => "N".to_owned(),
                90 => "E".to_owned(),
                180 => "S".to_owned(),
                270 => "W".to_owned(),
                d => d.to_string(),
            };
            text(Pos2::new(x, top + 12.0), Align2::CENTER_TOP, label, 11.0);
        }
    }
    painter.add(Shape::convex_polygon(
        vec![
            Pos2::new(image.center().x, top + 1.0),
            Pos2::new(image.center().x - 5.0, top - 6.0),
            Pos2::new(image.center().x + 5.0, top - 6.0),
        ],
        theme::PATH,
        Stroke::NONE,
    ));

    text(image.left_top() + Vec2::new(8.0, 34.0), Align2::LEFT_TOP, format!("{:4.1} m/s", pose.speed_mps), 15.0);
    text(image.left_top() + Vec2::new(8.0, 54.0), Align2::LEFT_TOP, format!("{:03.0}°", heading), 13.0);
    if let Some(mode) = station.client.mode() {
        text(image.right_top() + Vec2::new(-8.0, 34.0), Align2::RIGHT_TOP, mode_name(mode).to_uppercase(), 13.0);
    }
    if let Some(power) = &station.client.power {
        text(
            image.right_top() + Vec2::new(-8.0, 52.0),
            Align2::RIGHT_TOP,
            format!("{:.0}%  {:.0} W", power.soc_pct, power.power_w),
            12.0,
        );
    }
    if !station.client.vehicle_alive(station.now) {
        painter.text(image.center(), Align2::CENTER_CENTER, "LINK LOST", FontId::proportional(26.0), theme::BAD);
    }
}
