//! Debug-build-only visual review hooks for native screenshots.
//!
//! Environment variables are read only in native debug builds. Release and
//! Wasm builds compile every hook to an inert default, so shipped behavior,
//! the browser ABI and the public CLI are unchanged.
//!
//! - `GOLF_WINDOW=390x700` sets the native window's logical size.
//! - `GOLF_TURBO=4` multiplies frame time (more fixed ticks per frame).
//! - `GOLF_HOLE=N` starts the round on hole `N` (1-based).
//! - `GOLF_SKIP_WELCOME=1` starts on the course instead of the title card.
//! - `GOLF_SHOT=DEG,POWER` fires one scripted putt at the first aiming tick.
//! - `GOLF_PAUSE_AT=N` / `GOLF_CARD_AT=N` open pause or the card on frame `N`.
//! - `GOLF_FAKE_BEST=STROKES` pretends a stored full-round best existed.
//! - `GOLF_FOCUS=1` keeps a capture running when the window loses focus.
//! - `GOLF_PLAYER_CARD=1` shows the demo's results card as a player's (unsaved).
use ferravex_math::Vec2;
use ferravex_render::prelude::Conf;

/// A scripted menu action for one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Pause,
    Card,
}

#[derive(Clone, Debug, Default)]
pub struct Capture {
    frame: u32,
    pub hole: Option<usize>,
    pub skip_welcome: bool,
    pub fake_best: Option<u16>,
    /// Show the demonstration's results card as a player would see it.
    pub player_card: bool,
    /// Treat the window as focused even when another window steals focus.
    pub force_focus: bool,
    shot: Option<(f32, f32)>,
    pause_at: Option<u32>,
    card_at: Option<u32>,
}

#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
fn number<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.parse().ok()
}

/// Apply `GOLF_WINDOW` to a native debug window; no-op elsewhere.
pub fn window(conf: &mut Conf) {
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    if let Some((w, h)) = std::env::var("GOLF_WINDOW").ok().and_then(|size| {
        let (w, h) = size.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    }) {
        conf.window_width = w;
        conf.window_height = h;
    }
    let _ = conf;
}

/// Frame-time multiplier for fast review runs; always 1 outside debug native.
pub fn turbo() -> f32 {
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    if let Some(t) = number::<f32>("GOLF_TURBO") {
        return t.clamp(0.1, 8.);
    }
    1.
}

impl Capture {
    pub fn from_env() -> Self {
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        {
            Self {
                frame: 0,
                hole: number::<usize>("GOLF_HOLE").map(|h| h.clamp(1, 12) - 1),
                skip_welcome: std::env::var("GOLF_SKIP_WELCOME").is_ok(),
                fake_best: number("GOLF_FAKE_BEST"),
                player_card: std::env::var("GOLF_PLAYER_CARD").is_ok(),
                force_focus: std::env::var("GOLF_FOCUS").is_ok(),
                shot: std::env::var("GOLF_SHOT").ok().and_then(|s| {
                    let (a, p) = s.split_once(',')?;
                    Some((a.parse().ok()?, p.parse().ok()?))
                }),
                pause_at: number("GOLF_PAUSE_AT"),
                card_at: number("GOLF_CARD_AT"),
            }
        }
        #[cfg(not(all(debug_assertions, not(target_arch = "wasm32"))))]
        Self::default()
    }

    /// The scripted putt, consumed once.
    pub fn scripted_shot(&mut self) -> Option<(Vec2, f32)> {
        let (degrees, power) = self.shot.take()?;
        Some((
            Vec2::from_angle(degrees.to_radians()),
            power.clamp(0.05, 1.),
        ))
    }

    pub fn frame_actions(&self) -> Option<Action> {
        if self.pause_at == Some(self.frame) {
            Some(Action::Pause)
        } else if self.card_at == Some(self.frame) {
            Some(Action::Card)
        } else {
            None
        }
    }

    pub fn end_frame(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_capture_is_inert() {
        let mut c = Capture::default();
        assert_eq!(c.scripted_shot(), None);
        assert_eq!(c.frame_actions(), None);
        c.end_frame();
        assert!(c.hole.is_none() && !c.skip_welcome && c.fake_best.is_none());
    }
}
