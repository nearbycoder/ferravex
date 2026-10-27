//! Debug-build-only visual review hooks (not compiled into release or Wasm).
//!
//! - `GLOW_WINDOW=1600x1000` native window logical size.
//! - `GLOW_SCREEN=title|select|shrine` start on a menu instead of autoplay.
//! - `GLOW_SKIP=SECONDS` fast-forward the autoplay run headlessly before frame 0.
//! - `GLOW_CHAR=N`, `GLOW_QUICK=1`, `GLOW_SEED=N` choose the run.
//! - `GLOW_PAUSE_AT=N` pause on render frame N.
//! - `GLOW_LEVELUP_AT=N` / `GLOW_CHEST_AT=N` open those modals on frame N.
//! - `GLOW_OFFERS=4` reviews the largest upgrade-card arrangement.
//! - `GLOW_END_AT=N` surrender on frame N (results screen); `GLOW_WIN=1` wins instead.
//! - `GLOW_GLIMMER=N` pretend the save holds N Glimmer.
//! - `GLOW_HOLD=1` stop the bot from choosing in modals (they stay open).
use crate::Conf;
use crate::run::Run;
use crate::sim::{Event, Game, Input, MAX_EVENTS, Phase, bot};

fn number<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.parse().ok()
}

pub fn window(conf: &mut Conf) {
    if let Some((w, h)) = std::env::var("GLOW_WINDOW").ok().and_then(|size| {
        let (w, h) = size.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    }) {
        conf.window_width = w;
        conf.window_height = h;
    }
}

pub struct Capture {
    frame: u32,
    pub screen: Option<String>,
    skip: Option<f32>,
    pub character: Option<u8>,
    pub quick: bool,
    pub seed: Option<u64>,
    pause_at: Option<u32>,
    levelup_at: Option<u32>,
    chest_at: Option<u32>,
    end_at: Option<u32>,
    win: bool,
    pub glimmer: Option<u64>,
    hold: bool,
}

impl Capture {
    pub fn from_env() -> Self {
        Self {
            frame: 0,
            screen: std::env::var("GLOW_SCREEN").ok(),
            skip: number("GLOW_SKIP"),
            character: number("GLOW_CHAR"),
            quick: number::<u8>("GLOW_QUICK").is_some_and(|v| v > 0),
            seed: number("GLOW_SEED"),
            pause_at: number("GLOW_PAUSE_AT"),
            levelup_at: number("GLOW_LEVELUP_AT"),
            chest_at: number("GLOW_CHEST_AT"),
            end_at: number("GLOW_END_AT"),
            win: number::<u8>("GLOW_WIN").is_some_and(|v| v > 0),
            glimmer: number("GLOW_GLIMMER"),
            hold: number::<u8>("GLOW_HOLD").is_some_and(|v| v > 0),
        }
    }

    pub fn pause_now(&self) -> bool {
        self.pause_at == Some(self.frame)
    }

    /// Fast-forward once, headlessly, with the demo pilot.
    pub fn prepare(&mut self, run: &mut Run) {
        if let Some(seconds) = self.skip.take() {
            let mut events = Vec::with_capacity(MAX_EVENTS);
            let ticks = (seconds * 60.) as u32;
            for _ in 0..ticks {
                let input = bot::autoplay(&run.game);
                run.game.update(input);
                run.game.drain_events(&mut events);
                events.clear();
                if run.game.is_over() {
                    break;
                }
            }
        }
        let game = &mut run.game;
        if self.levelup_at == Some(self.frame) && game.phase == Phase::Running {
            game.debug_level_up();
            if let Some(count) = number::<u8>("GLOW_OFFERS") {
                game.offer.count = count.clamp(1, 4);
            }
        }
        if self.chest_at == Some(self.frame) && game.phase == Phase::Running {
            game.debug_chest();
        }
        if self.end_at == Some(self.frame) && !game.is_over() {
            game.debug_end(self.win);
        }
    }

    pub fn input(&self, game: &Game) -> Input {
        if self.hold && matches!(game.phase, Phase::LevelUp | Phase::Chest) {
            return Input::default();
        }
        bot::autoplay(game)
    }

    pub fn end_frame(&mut self) {
        self.frame += 1;
    }

    #[allow(dead_code)]
    pub fn events_hint(_: &[Event]) {}
}
