//! Debug-build-only visual review hooks. Not compiled into release or Wasm
//! builds, so shipped behavior and the public CLI are unchanged.
//!
//! - `SKY_WINDOW=390x700` sets the native window's logical size.
//! - `SKY_PAUSE_AT=N` presses pause on render frame `N`.
//! - `SKY_CRASH_AT=N` replaces autopilot with a boosted dive from frame `N`.
//! - `SKY_TOUR=1` flies the hidden-ring route with ordinary steering inputs.
//! - `SKY_BOOST=1` holds boost on the way to each gate (autopilot or tour).
//! - `SKY_FAKE_BEST=TIPS` pretends a stored best existed at the finish.
use super::{Conf, Flight, FlightRun, Input};

fn number<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.parse().ok()
}

pub(super) fn window(conf: &mut Conf) {
    if let Some((w, h)) = std::env::var("SKY_WINDOW").ok().and_then(|size| {
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
    crash_at: Option<u32>,
    tour: bool,
    boost: bool,
    fake_best: Option<u32>,
}

impl Capture {
    pub(super) fn from_env() -> Self {
        Self {
            frame: 0,
            pause_at: number("SKY_PAUSE_AT"),
            crash_at: number("SKY_CRASH_AT"),
            tour: std::env::var("SKY_TOUR").is_ok_and(|v| v == "1"),
            boost: std::env::var("SKY_BOOST").is_ok_and(|v| v == "1"),
            fake_best: number("SKY_FAKE_BEST"),
        }
    }

    pub(super) fn pause_now(&self, run: &FlightRun) -> bool {
        self.pause_at == Some(self.frame) && run.session.is_playing()
    }

    /// Ordinary inputs replacing autopilot for a review scenario, if any.
    pub(super) fn input(&self, flight: &Flight) -> Option<Input> {
        if self.crash_at.is_some_and(|at| self.frame >= at) {
            return Some(Input {
                climb: -1.,
                boost: true,
                ..Input::default()
            });
        }
        let mut input = if self.tour {
            flight.scenic_tour()
        } else if self.boost {
            flight.autopilot()
        } else {
            return None;
        };
        input.boost = self.boost && flight.next < super::GATES;
        Some(input)
    }

    pub(super) fn fake_best(&self) -> Option<u32> {
        self.fake_best
    }

    pub(super) fn end_frame(&mut self) {
        self.frame += 1;
    }
}
