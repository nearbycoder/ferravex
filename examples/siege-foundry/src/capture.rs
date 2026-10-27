//! Debug-build-only visual review hooks. This module is not compiled into
//! release or Wasm builds, so shipped behavior and the public CLI are unchanged.
//!
//! - `SIEGE_WINDOW=390x700` sets the native window's logical size.
//! - `SIEGE_PAUSE_AT=N` pauses on render frame `N` and holds the pause.
//! - `SIEGE_PLAN=bell,miss,miss` makes autoplay's first shots aim at the
//!   lunch bell or wide of the towers before its normal targeting resumes.
//! - `SIEGE_FAKE_BEST=POINTS` pretends a stored best existed at the result.
use super::Conf;

fn number<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.parse().ok()
}

pub(super) fn window(conf: &mut Conf) {
    if let Some((w, h)) = std::env::var("SIEGE_WINDOW").ok().and_then(|size| {
        let (w, h) = size.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    }) {
        conf.window_width = w;
        conf.window_height = h;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Aim {
    Bell,
    Miss,
}

pub(super) struct Capture {
    frame: u32,
    pause_at: Option<u32>,
    held: bool,
    plan: Vec<Aim>,
    fake_best: Option<u64>,
}

impl Capture {
    pub fn from_env() -> Self {
        let plan = std::env::var("SIEGE_PLAN")
            .unwrap_or_default()
            .split(',')
            .filter_map(|step| match step.trim() {
                "bell" => Some(Aim::Bell),
                "miss" => Some(Aim::Miss),
                _ => None,
            })
            .collect();
        Self {
            frame: 0,
            pause_at: number("SIEGE_PAUSE_AT"),
            held: false,
            plan,
            fake_best: number("SIEGE_FAKE_BEST"),
        }
    }

    /// True from the pause frame onward, once play has started.
    pub fn hold(&mut self, playing: bool) -> bool {
        self.held |= playing && self.pause_at.is_some_and(|at| self.frame >= at);
        self.held
    }

    /// The scripted aim for autoplay's `shot`-th shot (0-based), if any.
    pub fn aim(&self, shot: u8) -> Option<Aim> {
        self.plan.get(usize::from(shot)).copied()
    }

    pub fn fake_best(&self) -> Option<u64> {
        self.fake_best
    }

    pub fn end_frame(&mut self) {
        self.frame += 1;
    }
}
