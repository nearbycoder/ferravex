//! Debug-build-only visual review hooks. This module is not compiled into
//! release or Wasm builds, so shipped behavior and the public CLI are unchanged.
//!
//! - `MARBLE_WINDOW=390x700` sets the native window's logical size.
//! - `MARBLE_COURSE=N` starts on course `N` (0..=3).
//! - `MARBLE_PAUSE_AT=N` presses pause on render frame `N`.
//! - `MARBLE_PICKER_AT=N` opens the course picker on render frame `N`.
//! - `MARBLE_OVERVIEW_FROM=N` holds the course overview from frame `N`.
//! - `MARBLE_FALL_AT=N` drops the marble off the course on frame `N`.
//! - `MARBLE_SECRET_AT=N` places the marble on the moonstone on frame `N`.
//! - `MARBLE_PUSH_AT=N` launches the marble down the course at speed on frame `N`.
//! - `MARBLE_TOUCH=1` draws the touch layout as if a touch had been seen.
use crate::sim::Run;
use ferravex_physics::d3::Vector;
use ferravex_render::Conf;

fn number<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.parse().ok()
}

pub(crate) fn window(conf: &mut Conf) {
    if let Some((w, h)) = std::env::var("MARBLE_WINDOW").ok().and_then(|size| {
        let (w, h) = size.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    }) {
        conf.window_width = w;
        conf.window_height = h;
    }
}

pub(crate) struct Capture {
    frame: u32,
    pub course: Option<usize>,
    pause_at: Option<u32>,
    picker_at: Option<u32>,
    overview_from: Option<u32>,
    fall_at: Option<u32>,
    secret_at: Option<u32>,
    push_at: Option<u32>,
    pub touch: bool,
}

impl Capture {
    pub fn from_env() -> Self {
        Self {
            frame: 0,
            course: number("MARBLE_COURSE"),
            pause_at: number("MARBLE_PAUSE_AT"),
            picker_at: number("MARBLE_PICKER_AT"),
            overview_from: number("MARBLE_OVERVIEW_FROM"),
            fall_at: number("MARBLE_FALL_AT"),
            secret_at: number("MARBLE_SECRET_AT"),
            push_at: number("MARBLE_PUSH_AT"),
            touch: number::<u32>("MARBLE_TOUCH") == Some(1),
        }
    }

    pub fn pause_now(&self) -> bool {
        self.pause_at == Some(self.frame)
    }

    pub fn picker_now(&self) -> bool {
        self.picker_at == Some(self.frame)
    }

    pub fn overview(&self) -> bool {
        self.overview_from.is_some_and(|at| self.frame >= at)
    }

    /// Debug-only teleports used to stage fall and moonstone screenshots. Each
    /// fires once, on the first simulated frame at or after its request.
    pub fn stage(&mut self, run: &mut Run) {
        let frame = self.frame;
        let due = |at: &mut Option<u32>| at.take_if(|at| *at <= frame).is_some();
        let fall = due(&mut self.fall_at);
        let secret = due(&mut self.secret_at);
        let push = due(&mut self.push_at);
        let body = &mut run.physics.bodies[run.marble];
        if fall {
            body.set_translation(Vector::new(0., run.level.tuning.fall_y - 1., 0.), true);
        }
        if push {
            body.set_linvel(Vector::new(0., 0., -7.5), true);
        }
        if secret && let Some(secret) = run.secret {
            body.set_translation(Vector::from_array(secret.to_array()), true);
            body.set_linvel(Vector::ZERO, true);
        }
    }

    pub fn end_frame(&mut self) {
        self.frame += 1;
    }
}
