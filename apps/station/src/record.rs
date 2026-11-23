//! Records the window to an animated GIF, for documentation.
//!
//! Frames are kept in memory and encoded when recording stops. One palette is built
//! for the whole clip, and each frame stores only the rectangle that changed, with
//! unchanged pixels inside it left transparent. A mostly static UI therefore stays small.

use eframe::egui;
use std::borrow::Cow;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

struct Captured {
    width: usize,
    height: usize,
    rgb: Vec<u8>,
}

pub struct Recorder {
    path: PathBuf,
    period: f64,
    next: f64,
    frames: Arc<Mutex<Vec<Captured>>>,
}

/// Palette slot reserved for "same as the previous frame".
const TRANSPARENT: u8 = 255;
/// Summed RGB difference below which a pixel is treated as unchanged.
const NOISE_FLOOR: u32 = 8;

impl Recorder {
    pub fn new(path: PathBuf, fps: f64) -> Self {
        Self { path, period: 1.0 / fps.clamp(1.0, 30.0), next: 0.0, frames: Arc::default() }
    }

    /// Ask for a frame if one is due.
    pub fn tick(&mut self, ctx: &egui::Context, now: f64) {
        if now < self.next {
            return;
        }
        self.next = now + self.period;
        let frames = self.frames.clone();
        ctx.request_screenshot(move |shot| {
            let rgb = shot.pixels.iter().flat_map(|c| [c.r(), c.g(), c.b()]).collect();
            frames.lock().unwrap().push(Captured { width: shot.size[0], height: shot.size[1], rgb });
        });
    }

    /// Encode everything captured so far. Returns the number of frames written.
    pub fn finish(&self) -> Result<usize, String> {
        let frames = std::mem::take(&mut *self.frames.lock().unwrap());
        // The window can settle to its size a frame or two in, so go by the last frame.
        let Some(last) = frames.last() else { return Err("no frames were captured".into()) };
        let (width, height) = (last.width, last.height);
        // A resize mid-recording would break the frame-to-frame comparison.
        let frames: Vec<&Captured> = frames.iter().filter(|f| f.width == width && f.height == height).collect();
        encode(&self.path, &frames, width, height, (self.period * 100.0).round() as u16).map_err(|e| e.to_string())?;
        Ok(frames.len())
    }
}

fn encode(
    path: &Path,
    frames: &[&Captured],
    width: usize,
    height: usize,
    delay_cs: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    // Sample pixels across the whole clip so one palette serves every frame.
    let stride = (frames.len() / 12).max(1);
    let mut samples = Vec::new();
    for frame in frames.iter().step_by(stride) {
        for px in frame.rgb.chunks_exact(3).step_by(7) {
            samples.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
    }
    let quant = color_quant::NeuQuant::new(10, 255, &samples);
    let mut palette = quant.color_map_rgb();
    palette.resize(256 * 3, 0);

    // Nearest-colour search is slow; remember the answer per 6-bit-per-channel colour.
    let mut cache = vec![u8::MAX; 1 << 18];
    let mut index_of = |px: &[u8]| -> u8 {
        let key = ((px[0] as usize >> 2) << 12) | ((px[1] as usize >> 2) << 6) | (px[2] as usize >> 2);
        if cache[key] == u8::MAX {
            cache[key] = quant.index_of(&[px[0], px[1], px[2], 255]) as u8;
        }
        cache[key]
    };

    let mut encoder = gif::Encoder::new(File::create(path)?, width as u16, height as u16, &palette)?;
    encoder.set_repeat(gif::Repeat::Infinite)?;

    // What the viewer currently sees. Pixels that barely differ from it are left alone:
    // sensor noise in the video and lidar views would otherwise dominate the file.
    let mut shown: Vec<u8> = Vec::new();
    let differs = |a: &[u8], b: &[u8]| {
        a[0].abs_diff(b[0]) as u32 + a[1].abs_diff(b[1]) as u32 + a[2].abs_diff(b[2]) as u32 > NOISE_FLOOR
    };
    let mut held = 0u16;
    let mut pending: Option<gif::Frame<'static>> = None;
    for frame in frames {
        let first_frame = shown.is_empty();
        // Bounding box of what changed.
        let (mut x0, mut y0, mut x1, mut y1) = (width, height, 0usize, 0usize);
        if first_frame {
            (x0, y0, x1, y1) = (0, 0, width - 1, height - 1);
        } else {
            for y in 0..height {
                let row = y * width * 3..(y + 1) * width * 3;
                let (a, b) = (&frame.rgb[row.clone()], &shown[row]);
                if a == b {
                    continue;
                }
                let mut pairs = a.chunks_exact(3).zip(b.chunks_exact(3));
                let Some(first) = pairs.position(|(p, q)| differs(p, q)) else { continue };
                let last = a.chunks_exact(3).zip(b.chunks_exact(3)).rposition(|(p, q)| differs(p, q)).unwrap_or(first);
                y0 = y0.min(y);
                y1 = y;
                x0 = x0.min(first);
                x1 = x1.max(last);
            }
        }
        if x1 < x0 {
            // Nothing changed: show the previous frame for longer instead.
            held += delay_cs;
            continue;
        }
        if let Some(mut done) = pending.take() {
            done.delay = delay_cs + held;
            encoder.write_frame(&done)?;
        }
        held = 0;
        if first_frame {
            shown = frame.rgb.clone();
        }

        let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
        let mut buffer = Vec::with_capacity(w * h);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let i = (y * width + x) * 3;
                let px = &frame.rgb[i..i + 3];
                if !first_frame && !differs(px, &shown[i..i + 3]) {
                    buffer.push(TRANSPARENT);
                } else {
                    buffer.push(index_of(px));
                    shown[i..i + 3].copy_from_slice(px);
                }
            }
        }
        pending = Some(gif::Frame {
            delay: delay_cs,
            dispose: gif::DisposalMethod::Keep,
            transparent: (!first_frame).then_some(TRANSPARENT),
            top: y0 as u16,
            left: x0 as u16,
            width: w as u16,
            height: h as u16,
            buffer: Cow::Owned(buffer),
            ..Default::default()
        });
    }
    if let Some(mut done) = pending {
        // Pause on the last frame before the clip loops.
        done.delay = delay_cs + held + 150;
        encoder.write_frame(&done)?;
    }
    Ok(())
}
