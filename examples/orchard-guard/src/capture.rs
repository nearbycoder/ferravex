//! Debug-build-only visual review hooks. This module is not compiled into
//! release or Wasm builds, so shipped behavior and the public CLI are unchanged.
//!
//! - `ORCHARD_WINDOW=390x700` sets the native window's logical size.
//! - `ORCHARD_PAUSE_AT=N` presses Pause on render frame `N` while playing.
//! - `ORCHARD_SPEED_AT=N` toggles fast-forward on render frame `N`.
//! - `ORCHARD_TURBO=K` multiplies supplied frame time (still fixed ticks).
//! - `ORCHARD_DEFENSELESS=1` stops autoplay building; waves are still sent.
//! - `ORCHARD_LAZY_FROM=N` does the same from render frame `N` onward.
//! - `ORCHARD_SELECT=P` selects plot `P` (0-based) on frame 5.
//! - `ORCHARD_FAKE_BEST=SCORE` pretends an earlier best existed at the finish.
use super::{Action, Campaign, Conf, Outcome, save::Best};

fn number<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.parse().ok()
}

pub(super) fn window(conf: &mut Conf) {
    if let Some((w, h)) = std::env::var("ORCHARD_WINDOW").ok().and_then(|size| {
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
    speed_at: Option<u32>,
    turbo: f64,
    defenseless_from: Option<u32>,
    select: Option<usize>,
    fake_best: Option<u64>,
}

impl Capture {
    pub fn from_env() -> Self {
        Self {
            frame: 0,
            pause_at: number("ORCHARD_PAUSE_AT"),
            speed_at: number("ORCHARD_SPEED_AT"),
            turbo: number("ORCHARD_TURBO").unwrap_or(1.),
            defenseless_from: number::<u8>("ORCHARD_DEFENSELESS")
                .filter(|v| *v != 0)
                .map(|_| 0)
                .or_else(|| number("ORCHARD_LAZY_FROM")),
            select: number("ORCHARD_SELECT"),
            fake_best: number("ORCHARD_FAKE_BEST"),
        }
    }

    pub fn action(&self, campaign: &Campaign) -> Option<Action> {
        if self.pause_at == Some(self.frame) && campaign.session.is_playing() {
            return Some(Action::Pause);
        }
        if self.speed_at == Some(self.frame) {
            return Some(Action::Speed);
        }
        if self.frame == 5 && self.select.is_some() {
            return Some(Action::Select(self.select));
        }
        (self.defenseless()
            && !campaign.game.active
            && campaign.game.outcome == Outcome::Playing
            && self.frame > 30)
            .then_some(Action::Wave)
    }

    pub fn turbo(&self) -> f64 {
        self.turbo
    }

    pub fn defenseless(&self) -> bool {
        self.defenseless_from.is_some_and(|from| self.frame >= from)
    }

    pub fn best(&self, best: Option<Best>, kills: u32) -> Option<Best> {
        match (self.fake_best, best) {
            (Some(score), Some(_)) => Some(Best::compare(Some((score, None)), kills.into(), None)),
            _ => best,
        }
    }

    pub fn end_frame(&mut self) {
        self.frame += 1;
    }
}
