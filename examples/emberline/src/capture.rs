//! Debug-build-only visual review hooks. This module is not compiled into
//! release or Wasm builds, so shipped behavior and the public CLI are unchanged.
//!
//! - `EMBERLINE_WINDOW=390x700` sets the native window's logical size.
//! - `EMBERLINE_PAUSE_AT=N` presses pause on render frame `N`.
//! - `EMBERLINE_STOP_JUMPING_AT=N` makes autoplay stop jumping from frame `N`.
//! - `EMBERLINE_FAKE_BEST=SECONDS` pretends a stored best existed at the finish.
use super::{Conf, Game};

fn number<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.parse().ok()
}

pub(super) fn window(conf: &mut Conf) {
    if let Some((w, h)) = std::env::var("EMBERLINE_WINDOW").ok().and_then(|size| {
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
    stop_jumping_at: Option<u32>,
    fake_best: Option<f64>,
}

impl Capture {
    pub fn from_env() -> Self {
        Self {
            frame: 0,
            pause_at: number("EMBERLINE_PAUSE_AT"),
            stop_jumping_at: number("EMBERLINE_STOP_JUMPING_AT"),
            fake_best: number("EMBERLINE_FAKE_BEST"),
        }
    }

    pub fn pause_now(&self, game: &Game) -> bool {
        self.pause_at == Some(self.frame) && game.session.is_playing()
    }

    pub fn stop_jumping(&self) -> bool {
        self.stop_jumping_at.is_some_and(|at| self.frame >= at)
    }

    pub fn fake_best(&self) -> Option<f64> {
        self.fake_best
    }

    pub fn end_frame(&mut self) {
        self.frame += 1;
    }
}
