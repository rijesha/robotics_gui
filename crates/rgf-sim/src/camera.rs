//! Forward camera: a column-by-column heightfield renderer over the simulated world.

use crate::world::World;
use rgf_core::geo::heading_vec;
use rgf_core::Vec2;

pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 180;
pub const HFOV_RAD: f32 = 1.4;
/// Lens height above the ground under the vehicle.
pub const MOUNT_HEIGHT_M: f64 = 1.4;
const FAR_M: f64 = 140.0;

/// Render an RGB8 frame looking along `heading` from `pos`.
pub fn render(world: &World, pos: Vec2, heading: f64) -> Vec<u8> {
    let mut rgb = vec![0u8; WIDTH * HEIGHT * 3];
    let cam_z = world.ground(pos) + MOUNT_HEIGHT_M;
    let focal = (WIDTH as f64 / 2.0) / (HFOV_RAD as f64 / 2.0).tan();
    let horizon = HEIGHT as f64 / 2.0;
    let haze: [f64; 3] = if world.water { [176.0, 204.0, 224.0] } else { [186.0, 206.0, 222.0] };

    for col in 0..WIDTH {
        let offset = ((col as f64 + 0.5) - WIDTH as f64 / 2.0).atan2(focal);
        let dir = heading_vec(heading + offset);
        let depth_scale = offset.cos();
        // Lowest row not yet drawn in this column; nearer surfaces hide farther ones.
        let mut y_limit = HEIGHT as i32;
        let mut d = 0.6;
        while d < FAR_M && y_limit > 0 {
            let (h, color) = world.surface(pos + dir * d);
            let y = (horizon - (h - cam_z) / (d * depth_scale) * focal).floor() as i32;
            if y < y_limit {
                let fog = (d / FAR_M).powf(1.5);
                let shade: [u8; 3] = std::array::from_fn(|i| (color[i] as f64 * (1.0 - fog) + haze[i] * fog) as u8);
                for row in y.max(0)..y_limit {
                    let i = (row as usize * WIDTH + col) * 3;
                    rgb[i..i + 3].copy_from_slice(&shade);
                }
                y_limit = y.max(0);
            }
            d += (0.02 * d).max(0.08);
        }
        for row in 0..y_limit.max(0) as usize {
            let t = row as f64 / horizon;
            let i = (row * WIDTH + col) * 3;
            rgb[i] = (96.0 + 90.0 * t).min(haze[0]) as u8;
            rgb[i + 1] = (150.0 + 56.0 * t).min(haze[1]) as u8;
            rgb[i + 2] = (214.0 + 10.0 * t).min(haze[2]) as u8;
        }
    }
    rgb
}

pub fn encode_jpeg(rgb: &[u8], quality: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 * 1024);
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100));
    encoder
        .encode(rgb, WIDTH as u32, HEIGHT as u32, image::ExtendedColorType::Rgb8)
        .expect("encoding an in-memory RGB frame cannot fail");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_has_sky_above_ground_and_encodes() {
        let world = World::land();
        let rgb = render(&world, Vec2::default(), 0.0);
        let px = |x: usize, y: usize| &rgb[(y * WIDTH + x) * 3..(y * WIDTH + x) * 3 + 3];
        let (sky, ground) = (px(WIDTH / 2, 2), px(WIDTH / 2, HEIGHT - 2));
        assert!(sky[2] > sky[1] && sky[2] > 180, "top row should be sky, got {sky:?}");
        assert!(ground[1] > ground[2], "bottom row should be field, got {ground:?}");
        let jpeg = encode_jpeg(&rgb, 60);
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
        assert!(jpeg.len() < 40_000);
    }
}
