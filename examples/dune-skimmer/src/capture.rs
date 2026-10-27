//! Debug-build-only visual review hooks. This module is not compiled into
//! release or Wasm builds, so shipped behavior and the public CLI are unchanged.
//!
//! - `DUNE_WINDOW=390x700` sets the native window's logical size.
//! - `DUNE_PAUSE_AT=N` pauses on render frame `N`.
//! - `DUNE_FAKE_BEST=SECONDS` pretends a stored best race existed at the finish.
//! - `DUNE_VETERAN=1` hides onboarding hints as if a finish were stored.
use super::Conf;

fn number<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.parse().ok()
}

pub(super) fn window(conf: &mut Conf) {
    if let Some((w, h)) = std::env::var("DUNE_WINDOW").ok().and_then(|size| {
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
    fake_best: Option<f32>,
    pub veteran: bool,
}

impl Capture {
    pub fn from_env() -> Self {
        Self {
            frame: 0,
            pause_at: number("DUNE_PAUSE_AT"),
            fake_best: number("DUNE_FAKE_BEST"),
            veteran: number::<u8>("DUNE_VETERAN").is_some_and(|v| v != 0),
        }
    }

    pub fn pause_now(&self) -> bool {
        self.pause_at == Some(self.frame)
    }

    pub fn fake_best(&self) -> Option<f32> {
        self.fake_best
    }

    pub fn end_frame(&mut self) {
        self.frame += 1;
    }
}
