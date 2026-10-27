//! Debug-build-only visual review hooks. This module is not compiled into
//! release or Wasm builds, so shipped behavior and the public CLI are unchanged.
//!
//! - `NEON_WINDOW=390x700` sets the native window's logical size.
//! - `NEON_PAUSE_AT=N` pauses on render frame `N`.
//! - `NEON_IDLE_AT=N` makes autoplay stop all input from frame `N` (to lose).
//! - `NEON_FAKE_BEST=SCORE` pretends a stored best existed at the finish.
//! - `NEON_DISCO=1` lights every wall lamp at the start of a run.
//! - `NEON_HEALTH=HP` sets health once on the first playing frame.
use crate::Conf;
use crate::game::{Game, Input};

fn number<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.parse().ok()
}

pub(crate) fn window(conf: &mut Conf) {
    if let Some((w, h)) = std::env::var("NEON_WINDOW").ok().and_then(|size| {
        let (w, h) = size.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    }) {
        conf.window_width = w;
        conf.window_height = h;
    }
}

pub(crate) struct Capture {
    frame: u32,
    pause_at: Option<u32>,
    idle_at: Option<u32>,
    fake_best: Option<u64>,
    disco: bool,
    health: Option<i32>,
}

impl Capture {
    pub fn from_env() -> Self {
        Self {
            frame: 0,
            pause_at: number("NEON_PAUSE_AT"),
            idle_at: number("NEON_IDLE_AT"),
            fake_best: number("NEON_FAKE_BEST"),
            disco: number::<u8>("NEON_DISCO").is_some_and(|v| v > 0),
            health: number("NEON_HEALTH"),
        }
    }

    pub fn pause_now(&self) -> bool {
        self.pause_at == Some(self.frame)
    }

    pub fn input(&self, game: &Game) -> Input {
        if self.idle_at.is_some_and(|at| self.frame >= at) {
            Input::default()
        } else {
            game.autoplay()
        }
    }

    pub fn fake_best(&self) -> Option<u64> {
        self.fake_best
    }

    pub fn prepare(&mut self, game: &mut Game) {
        if self.disco {
            game.style.beacons = 0b1111;
        }
        if let Some(hp) = self.health.take() {
            game.health = hp;
        }
    }

    pub fn end_frame(&mut self) {
        self.frame += 1;
    }
}
