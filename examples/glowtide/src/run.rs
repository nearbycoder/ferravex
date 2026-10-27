//! Testable local host: [`Game`] owns rules, [`GameSession`] owns lifecycle/time.
//!
//! ```
//! use glowtide::{run::Run, sim::{RunConfig, bot}};
//! let mut run = Run::new(RunConfig::default(), false);
//! assert_eq!(run.advance(1.0, bot::autoplay), 0);
//! assert!(run.activate());
//! assert_eq!(run.advance(1.0 / 60.0, bot::autoplay), 1);
//! assert!(run.pause());
//! assert_eq!(run.advance(1.0, bot::autoplay), 0);
//! ```
use crate::sim::{Event, Game, Input, RunConfig, TICK_HZ};
use ferravex_core::session::{GameSession, SessionState};

pub struct Run {
    pub game: Game,
    pub session: GameSession,
    pub config: RunConfig,
    /// Interpolation weight between the last two ticks for rendering.
    pub alpha: f32,
}

impl Run {
    pub fn new(config: RunConfig, autoplay: bool) -> Self {
        Self {
            game: Game::new(config),
            session: GameSession::try_new(TICK_HZ, 6, autoplay).expect("authored tick rate"),
            config,
            alpha: 1.,
        }
    }

    /// Start a fresh run with a (possibly different) configuration.
    pub fn begin(&mut self, config: RunConfig) {
        self.config = config;
        self.game = Game::new(config);
        self.session.restart();
        self.alpha = 1.;
    }

    pub fn restart(&mut self) {
        let config = self.config;
        self.begin(config);
    }

    /// Return to the title: a fresh, not-yet-started run.
    pub fn reset(&mut self) {
        self.game = Game::new(self.config);
        self.session.reset();
        self.alpha = 1.;
    }

    /// Start from Ready or explicitly resume from Paused.
    pub fn activate(&mut self) -> bool {
        match self.session.state() {
            SessionState::Ready => self.session.start(),
            SessionState::Paused => self.session.set_paused(false),
            SessionState::Won | SessionState::Lost => {
                self.restart();
                true
            }
            SessionState::Playing => false,
        }
    }

    pub fn pause(&mut self) -> bool {
        self.session.set_paused(true)
    }

    /// Execute the ticks scheduled for `elapsed` seconds. `input` is called once
    /// per **executed** tick, so edges are consumed only when they can act.
    /// A tick that ends the run stops the rest of the catch-up batch.
    pub fn advance(&mut self, elapsed: f64, mut input: impl FnMut(&Game) -> Input) -> u32 {
        let frame = self.session.advance(elapsed);
        if self.session.is_playing() {
            self.alpha = frame.alpha;
        }
        let mut executed = 0;
        for _ in 0..frame.steps {
            let command = input(&self.game);
            self.game.update(command);
            executed += 1;
            if self.game.is_over() {
                self.session
                    .finish(self.game.phase == crate::sim::Phase::Won);
                break;
            }
        }
        executed
    }

    pub fn drain(&mut self, out: &mut Vec<Event>) {
        self.game.drain_events(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{Choice, Phase, bot};

    fn play(run: &mut Run, cadence: &[f64], game_seconds: f32) {
        let mut events = Vec::with_capacity(crate::sim::MAX_EVENTS);
        let mut frame = 0;
        while run.game.time < game_seconds && run.session.is_playing() {
            run.advance(cadence[frame % cadence.len()], bot::autoplay);
            run.drain(&mut events);
            events.clear();
            frame += 1;
        }
    }

    #[test]
    fn same_seed_matches_across_render_cadences_and_pauses() {
        let config = RunConfig {
            seed: 11,
            ..RunConfig::default()
        };
        let mut steady = Run::new(config, true);
        play(&mut steady, &[1. / 60.], 80.);
        let mut ragged = Run::new(config, true);
        ragged.advance(0.05, bot::autoplay);
        assert!(ragged.pause());
        assert_eq!(
            ragged.advance(30., |_| panic!("paused runs take no input")),
            0
        );
        assert!(ragged.activate());
        play(
            &mut ragged,
            &[0.001, 1. / 30., 1. / 144., 0.075, 1. / 240.],
            80.,
        );
        // Fixed ticks: both reach the same tick with identical state.
        while ragged.game.ticks < steady.game.ticks {
            ragged.advance(1. / 60., bot::autoplay);
        }
        while steady.game.ticks < ragged.game.ticks {
            steady.advance(1. / 60., bot::autoplay);
        }
        let a = &steady.game;
        let b = &ragged.game;
        assert_eq!(a.ticks, b.ticks);
        assert_eq!(a.time, b.time);
        assert_eq!(a.player.pos, b.player.pos);
        assert_eq!(a.stats.kills, b.stats.kills);
        assert_eq!(a.stats.damage, b.stats.damage);
        assert_eq!(a.level(), b.level());
        assert_eq!(a.foes.len(), b.foes.len());
    }

    #[test]
    fn edges_are_consumed_once_per_executed_tick() {
        let mut run = Run::new(RunConfig::default(), true);
        let mut dash_pending = true;
        // A zero-tick frame keeps the edge pending; catch-up uses it once.
        assert_eq!(run.advance(0.001, |_| panic!("no tick yet")), 0);
        run.advance(4. / 60., |_| Input {
            dash: std::mem::take(&mut dash_pending),
            ..Input::default()
        });
        assert_eq!(run.game.stats.dashes, 1);
        assert!(!dash_pending);
    }

    #[test]
    fn terminal_tick_stops_the_batch_and_restart_resets_everything() {
        let mut run = Run::new(RunConfig::default(), true);
        run.game.player.hp = 0.;
        let mut inputs = 0;
        run.advance(0.1, |_| {
            inputs += 1;
            Input::default()
        });
        assert_eq!(inputs, 1, "nothing runs after the losing tick");
        assert_eq!(run.session.state(), SessionState::Lost);
        assert_eq!(run.game.phase, Phase::Lost);
        assert_eq!(run.advance(1., |_| panic!("finished")), 0);
        assert!(run.activate());
        assert_eq!(run.game.phase, Phase::Running);
        assert_eq!(run.game.stats.kills, 0);
        assert_eq!(run.session.tick(), 0);
        assert_eq!(
            run.alpha, 1.,
            "restart snaps interpolation to the fresh world"
        );
        // Level-up decisions travel through tick input like everything else.
        run.game.debug_level_up();
        run.advance(1. / 60., |_| Input {
            choice: Choice::Pick(0),
            ..Input::default()
        });
        assert_eq!(run.game.phase, Phase::Running);
        run.alpha = 0.25;
        run.reset();
        assert_eq!(run.session.state(), SessionState::Ready);
        assert_eq!(run.alpha, 1.);
    }
}
