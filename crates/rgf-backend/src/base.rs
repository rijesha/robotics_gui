//! RTK base station.

use rgf_core::msgs::rtk::{BaseConfig, BaseMode, BaseStatus};
use rgf_core::{LatLon, LocalFrame, Vec2};

/// A source of base-station status and RTCM corrections. The simulated base below is the
/// only implementation so far; a serial GNSS receiver or an NTRIP client fits the same shape.
pub trait BaseStation: Send {
    /// Apply a new configuration. Returns false if it is not usable.
    fn configure(&mut self, config: &BaseConfig, now: f64) -> bool;
    fn tick(&mut self, now: f64);
    fn status(&self) -> BaseStatus;
    /// RTCM messages produced since the last call.
    fn take_rtcm(&mut self) -> Vec<Vec<u8>>;
    /// Told where the rover is; a real base ignores this.
    fn observe_rover(&mut self, _pos: LatLon, _now: f64) {}
}

/// A base that surveys itself in and then broadcasts placeholder RTCM once a second.
pub struct SimBase {
    /// Where the antenna really is; unknown until a rover has been seen nearby.
    truth: Option<LatLon>,
    mode: BaseMode,
    survey_start: f64,
    min_duration_s: f64,
    accuracy_limit_m: f64,
    accuracy_m: f64,
    progress: f64,
    elapsed_s: f64,
    position: LatLon,
    alt_m: f32,
    next_rtcm: f64,
    rtcm_bytes: u32,
    pending: Vec<Vec<u8>>,
}

const RTCM_LEN: usize = 96;

impl Default for SimBase {
    fn default() -> Self {
        Self {
            truth: None,
            mode: BaseMode::BASE_SURVEY_IN,
            survey_start: 0.0,
            min_duration_s: 20.0,
            accuracy_limit_m: 0.5,
            accuracy_m: 0.0,
            progress: 0.0,
            elapsed_s: 0.0,
            position: LatLon::default(),
            alt_m: 0.0,
            next_rtcm: 0.0,
            rtcm_bytes: 0,
            pending: Vec::new(),
        }
    }
}

impl BaseStation for SimBase {
    fn configure(&mut self, config: &BaseConfig, now: f64) -> bool {
        if config.use_fixed {
            if !(-90.0..=90.0).contains(&config.lat) || !(-180.0..=180.0).contains(&config.lon) {
                return false;
            }
            self.mode = BaseMode::BASE_FIXED;
            self.position = LatLon::new(config.lat, config.lon);
            self.alt_m = config.alt_m;
            self.accuracy_m = 0.0;
            self.progress = 1.0;
        } else {
            if !(config.accuracy_limit_m > 0.0) {
                return false;
            }
            self.mode = BaseMode::BASE_SURVEY_IN;
            self.survey_start = now;
            self.min_duration_s = config.min_duration_s as f64;
            self.accuracy_limit_m = config.accuracy_limit_m as f64;
            self.progress = 0.0;
        }
        true
    }

    fn observe_rover(&mut self, pos: LatLon, now: f64) {
        let frame = LocalFrame::new(pos);
        let far = self.truth.is_some_and(|t| frame.to_local(t).length() > 2000.0);
        if self.truth.is_none() || far {
            // Set the simulated antenna up a short walk from the rover.
            self.truth = Some(frame.to_latlon(Vec2::new(-22.0, -16.0)));
            if self.mode == BaseMode::BASE_SURVEY_IN || far {
                self.mode = BaseMode::BASE_SURVEY_IN;
                self.survey_start = now;
                self.progress = 0.0;
            }
        }
    }

    fn tick(&mut self, now: f64) {
        let Some(truth) = self.truth else { return };
        match self.mode {
            BaseMode::BASE_SURVEY_IN => {
                self.elapsed_s = now - self.survey_start;
                // The averaged position converges quickly at first, then slowly.
                self.accuracy_m = 2.5 * (-self.elapsed_s / 6.0).exp() + 0.03;
                let offset = Vec2::new(0.6, -0.4) * self.accuracy_m;
                self.position = LocalFrame::new(truth).to_latlon(offset);
                let time_part = if self.min_duration_s > 0.0 { self.elapsed_s / self.min_duration_s } else { 1.0 };
                let accuracy_part = self.accuracy_limit_m / self.accuracy_m;
                self.progress = time_part.min(accuracy_part).clamp(0.0, 1.0);
                if self.progress >= 1.0 {
                    self.mode = BaseMode::BASE_FIXED;
                    self.next_rtcm = now;
                }
            }
            BaseMode::BASE_FIXED => {
                if now >= self.next_rtcm {
                    self.next_rtcm = now + 1.0;
                    // Placeholder with an RTCM3 preamble; the simulated rover only needs
                    // corrections to keep arriving.
                    let mut msg = vec![0u8; RTCM_LEN];
                    msg[0] = 0xD3;
                    msg[2] = (RTCM_LEN - 6) as u8;
                    self.rtcm_bytes += msg.len() as u32;
                    self.pending.push(msg);
                }
            }
            BaseMode::BASE_IDLE => {}
        }
    }

    fn status(&self) -> BaseStatus {
        let broadcasting = self.mode == BaseMode::BASE_FIXED && self.truth.is_some();
        BaseStatus {
            mode: if self.truth.is_some() { self.mode } else { BaseMode::BASE_IDLE },
            survey_progress: self.progress as f32,
            survey_elapsed_s: self.elapsed_s as u32,
            accuracy_m: self.accuracy_m as f32,
            lat: self.position.lat,
            lon: self.position.lon,
            alt_m: self.alt_m,
            sats: if self.truth.is_some() { 17 } else { 0 },
            rtcm_rate_hz: if broadcasting { 1.0 } else { 0.0 },
            rtcm_bytes: self.rtcm_bytes,
        }
    }

    fn take_rtcm(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.pending)
    }
}
