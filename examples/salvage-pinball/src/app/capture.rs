//! Debug-build-only visual review hooks. This module is not compiled into
//! release or Wasm builds, so shipped behavior and the public CLI are unchanged.
//!
//! - `SALVAGE_WINDOW=390x700` sets the native window's logical size.
//! - `SALVAGE_PAUSE_FROM=N` holds the table paused from render frame `N`.
//! - `SALVAGE_LAZY_BOT=1` keeps autoplay launching but never flipping, so the
//!   results card arrives within a few hundred frames.
//! - `SALVAGE_FAKE_BEST=SCORE` pretends a stored personal best existed.
//! - `SALVAGE_START=1` leaves Ready on the first frame without launching.
use super::Conf;

fn number<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.parse().ok()
}

pub(super) fn window(conf: &mut Conf) {
    if let Some((w, h)) = std::env::var("SALVAGE_WINDOW").ok().and_then(|size| {
        let (w, h) = size.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    }) {
        conf.window_width = w;
        conf.window_height = h;
    }
}

pub(super) struct Capture {
    frame: u32,
    pause_from: Option<u32>,
    lazy_bot: bool,
    fake_best: Option<u32>,
    start: bool,
}

impl Capture {
    pub fn from_env() -> Self {
        Self {
            frame: 0,
            pause_from: number("SALVAGE_PAUSE_FROM"),
            lazy_bot: number::<u8>("SALVAGE_LAZY_BOT").is_some_and(|v| v > 0),
            fake_best: number("SALVAGE_FAKE_BEST"),
            start: number::<u8>("SALVAGE_START").is_some_and(|v| v > 0),
        }
    }

    pub fn paused(&self) -> bool {
        self.pause_from.is_some_and(|at| self.frame >= at)
    }

    pub fn lazy_bot(&self) -> bool {
        self.lazy_bot
    }

    pub fn fake_best(&self) -> Option<u32> {
        self.fake_best
    }

    pub fn start_now(&self) -> bool {
        self.start && self.frame == 0
    }

    pub fn end_frame(&mut self) {
        self.frame += 1;
    }
}
