//! Debug-build-only visual review hooks. This module is not compiled into
//! release or Wasm builds, so shipped behavior and the public CLI are unchanged.
//!
//! - `RALLY_WINDOW=390x700` sets the native window's logical size.
//! - `RALLY_PAUSE_AT=N` presses pause on render frame `N`.
//! - `RALLY_DRIFT=1` makes autoplay pulse the handbrake with exaggerated steering.
//! - `RALLY_OFFROAD_AT=N` makes autoplay steer onto the grass for 70 frames.
//! - `RALLY_FAKE_BEST=SECONDS` pretends a stored best race existed at the finish.
//! - `RALLY_POND_AT=N` places the car beside the duck pond on frame `N`.
use super::Conf;
use crate::sim::Controls;

fn number<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.parse().ok()
}

pub(super) fn window(conf: &mut Conf) {
    if let Some((w, h)) = std::env::var("RALLY_WINDOW").ok().and_then(|size| {
        let (w, h) = size.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    }) {
        conf.window_width = w;
        conf.window_height = h;
    }
}

pub(super) struct Capture {
    frame: u32,
    pause_at: Option<u32>,
    drift: bool,
    offroad_at: Option<u32>,
    fake_best: Option<f32>,
    pond_at: Option<u32>,
}

impl Capture {
    pub fn from_env() -> Self {
        Self {
            frame: 0,
            pause_at: number("RALLY_PAUSE_AT"),
            drift: std::env::var("RALLY_DRIFT").is_ok(),
            offroad_at: number("RALLY_OFFROAD_AT"),
            fake_best: number("RALLY_FAKE_BEST"),
            pond_at: number("RALLY_POND_AT"),
        }
    }

    pub fn pause_now(&self) -> bool {
        self.pause_at == Some(self.frame)
    }

    pub fn steer(&self, controls: &mut Controls) {
        if self.drift && self.frame % 200 < 45 {
            controls.handbrake = true;
            controls.steer = (controls.steer * 3.).clamp(-1., 1.);
        }
        if let Some(at) = self.offroad_at
            && (at..at + 70).contains(&self.frame)
        {
            controls.steer = 1.;
            controls.throttle = 1.;
            controls.handbrake = false;
        }
    }

    pub fn results(&self, results: &mut crate::hud::Results) {
        if let Some(best) = self.fake_best {
            results.previous_best = Some(best);
            results.previous_style = Some(800);
            results.demo = false;
        }
    }

    pub fn pond(&self, race: &mut crate::sim::Race) {
        if self.pond_at == Some(self.frame) {
            race.car.pose.position = ferravex_render::prelude::vec2(1000., 520.);
            race.car.pose.angle = std::f32::consts::FRAC_PI_2;
            race.car.velocity = ferravex_render::prelude::vec2(0., 140.);
        }
    }

    pub fn end_frame(&mut self) {
        self.frame += 1;
    }
}
