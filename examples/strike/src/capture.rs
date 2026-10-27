//! Debug-build-only visual review hooks. This module is not compiled into
//! release or Wasm builds, so shipped behavior and the public CLI are unchanged.
//!
//! - `STRIKE_WINDOW=390x700` sets the native window's logical size.
//! - `STRIKE_AIMBOT=1` aims at the nearest visible bot and fires (practice).
//! - `STRIKE_DUCK=1` aims at the dock duck for the first shots instead.
//! - `STRIKE_MATCH_SECONDS=N` shortens the practice match.
//! - `STRIKE_PAUSE_AT=N` opens the menu on render frame `N`.
//! - `STRIKE_ROSTER_AT=N` holds the scoreboard from render frame `N`.
//! - `STRIKE_FAKE_BEST=FRAGS` pretends a stored best existed at the finish.
//! - `STRIKE_STILL=1` holds a fixed aim without firing (ignores the mouse).
//! - `STRIKE_LEARNED=1` treats onboarding hints as already learned.
//! - `STRIKE_KEEP_FOCUS=1` ignores window focus loss during scripted runs.
use ferravex_fps::{Match, forward};
use ferravex_render::prelude::{Conf, Vec3};

fn number<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.parse().ok()
}

pub(super) fn window(conf: &mut Conf) {
    if let Some((w, h)) = std::env::var("STRIKE_WINDOW").ok().and_then(|size| {
        let (w, h) = size.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    }) {
        conf.window_width = w;
        conf.window_height = h;
    }
}

pub(super) struct Capture {
    pub frame: u64,
    aimbot: bool,
    duck: bool,
    pub match_seconds: Option<u32>,
    pause_at: Option<u64>,
    roster_at: Option<u64>,
    pub fake_best: Option<u64>,
    pub keep_focus: bool,
    still: bool,
    pub learned: bool,
}

impl Capture {
    pub fn from_env() -> Self {
        Self {
            frame: 0,
            aimbot: number::<u8>("STRIKE_AIMBOT").is_some_and(|n| n > 0),
            duck: number::<u8>("STRIKE_DUCK").is_some_and(|n| n > 0),
            match_seconds: number("STRIKE_MATCH_SECONDS"),
            pause_at: number("STRIKE_PAUSE_AT"),
            roster_at: number("STRIKE_ROSTER_AT"),
            fake_best: number("STRIKE_FAKE_BEST"),
            keep_focus: number::<u8>("STRIKE_KEEP_FOCUS").is_some_and(|n| n > 0),
            still: number::<u8>("STRIKE_STILL").is_some_and(|n| n > 0),
            learned: number::<u8>("STRIKE_LEARNED").is_some_and(|n| n > 0),
        }
    }

    pub fn pause_now(&self) -> bool {
        self.pause_at == Some(self.frame)
    }

    pub fn roster(&self) -> bool {
        self.roster_at.is_some_and(|at| self.frame >= at)
    }

    /// Scripted aim for screenshots: (yaw, pitch, fire).
    pub fn aim(&self, game: &Match, duck_found: bool) -> Option<(f32, f32, bool)> {
        let me = game.players.first().filter(|p| p.is_alive())?;
        let eye = me.eye_position();
        let toward = |target: Vec3| -> Option<(f32, f32)> {
            let d = target - eye;
            Some((d.x.atan2(-d.z), (d.y / d.length().max(0.01)).asin()))
        };
        if self.walk() {
            return Some((0., 0., false));
        }
        if self.duck && !duck_found {
            let (yaw, pitch) = toward(crate::scoreboard::DUCK)?;
            return Some((yaw, pitch, self.frame.is_multiple_of(20)));
        }
        if self.still {
            return Some((0., -0.02, false));
        }
        if !self.aimbot {
            return None;
        }
        let target = game
            .players
            .iter()
            .filter(|p| p.id != 0 && p.is_alive())
            .filter(|p| {
                let d = p.eye_position() - eye;
                let distance = d.length();
                game.arena
                    .raycast(eye, d / distance.max(0.01), distance)
                    .is_none()
            })
            .min_by(|a, b| {
                a.position
                    .distance_squared(me.position)
                    .total_cmp(&b.position.distance_squared(me.position))
            });
        match target {
            Some(p) => {
                // Alternate head and chest so headshots and body hits both show.
                let drop = if (self.frame / 90).is_multiple_of(2) {
                    0.05
                } else {
                    0.45
                };
                let (yaw, pitch) = toward(p.eye_position() - Vec3::Y * drop)?;
                debug_assert!(forward(yaw, pitch).length() > 0.5);
                Some((yaw, pitch, true))
            }
            None => Some((me.yaw + 0.02, -0.02, false)),
        }
    }

    /// The duck run first walks out of the spawn so the duck is in view.
    pub fn walk(&self) -> bool {
        self.duck && self.frame < 75
    }

    pub fn end_frame(&mut self) {
        self.frame += 1;
    }
}
