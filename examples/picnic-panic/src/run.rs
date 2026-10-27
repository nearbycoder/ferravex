//! Testable local host: [`Game`] owns the rules, [`GameSession`] owns
//! lifecycle and fixed time, and the ghost recorder samples accepted poses.
//!
//! ```
//! use picnic_panic::{run::Run, sim::{Input, RunConfig}};
//! let mut run = Run::new(RunConfig::default(), false);
//! assert_eq!(run.advance(1.0, |_| Input::default()), 0); // Ready: no ticks.
//! assert!(run.activate());
//! assert_eq!(run.advance(1.0 / 60.0, |_| Input::default()), 1);
//! assert!(run.pause());
//! assert_eq!(run.advance(5.0, |_| Input::default()), 0); // Paused time is discarded.
//! ```
use crate::meta::{GHOST_STEP, Ghost};
use crate::sim::{Event, Game, Input, Phase, RunConfig, TICK_HZ};
use ferravex_core::session::{GameSession, SessionState};

pub struct Run {
    pub game: Game,
    pub session: GameSession,
    pub config: RunConfig,
    /// Interpolation weight between the last two ticks for rendering.
    pub alpha: f32,
    /// This run's route, sampled for a future speedrun ghost.
    pub ghost: Ghost,
}

impl Run {
    pub fn new(config: RunConfig, autoplay: bool) -> Self {
        let mut ghost = Ghost::default();
        ghost.push(crate::sim::park::Park::authored().spawn, 0.);
        ghost.clear();
        Self {
            game: Game::new(config),
            session: GameSession::try_new(TICK_HZ, 6, autoplay).expect("authored tick rate"),
            config,
            alpha: 1.,
            ghost,
        }
    }

    pub fn begin(&mut self, config: RunConfig) {
        self.config = config;
        self.game = Game::new(config);
        self.session.restart();
        self.alpha = 1.;
        self.ghost.clear();
    }

    pub fn reset(&mut self) {
        self.game = Game::new(self.config);
        self.session.reset();
        self.alpha = 1.;
        self.ghost.clear();
    }

    /// Start from Ready or explicitly resume from Paused.
    pub fn activate(&mut self) -> bool {
        match self.session.state() {
            SessionState::Ready => self.session.start(),
            SessionState::Paused => self.session.set_paused(false),
            SessionState::Won | SessionState::Lost => {
                let config = self.config;
                self.begin(config);
                true
            }
            SessionState::Playing => false,
        }
    }

    pub fn pause(&mut self) -> bool {
        self.session.set_paused(true)
    }

    /// Execute the ticks scheduled for `elapsed` seconds. `input` is called
    /// once per **executed** tick, so edges are consumed only when they act.
    /// The winning tick ends the batch.
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
            if self.game.tick.is_multiple_of(GHOST_STEP) {
                let (p, yaw) = self.game.coon_pose(1.);
                self.ghost.push(p, yaw);
            }
            if self.game.phase == Phase::Won {
                self.session.finish(true);
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
    use crate::sim::bot::Bot;

    fn play(run: &mut Run, bot: &mut Bot, cadence: &[f64], ticks: u64) {
        let mut events = Vec::with_capacity(crate::sim::MAX_EVENTS);
        let mut frame = 0;
        while run.game.tick < ticks && run.session.is_playing() {
            let step = cadence[frame % cadence.len()];
            run.advance(step, |g| bot.input(g));
            run.drain(&mut events);
            events.clear();
            frame += 1;
        }
    }

    #[test]
    fn same_seed_matches_across_render_cadences_and_pauses() {
        let config = RunConfig {
            seed: 11,
            completed: 0,
        };
        let mut steady = Run::new(config, true);
        let mut bot_a = Bot::new(&steady.game);
        play(&mut steady, &mut bot_a, &[1. / 60.], 1500);
        let mut ragged = Run::new(config, true);
        let mut bot_b = Bot::new(&ragged.game);
        ragged.advance(0.05, |g| bot_b.input(g));
        assert!(ragged.pause());
        assert_eq!(
            ragged.advance(30., |_| panic!("paused runs take no input")),
            0
        );
        assert!(ragged.activate());
        play(
            &mut ragged,
            &mut bot_b,
            &[0.001, 1. / 30., 1. / 144., 0.075, 1. / 240.],
            1500,
        );
        while ragged.game.tick < steady.game.tick {
            ragged.advance(1. / 60., |g| bot_b.input(g));
        }
        while steady.game.tick < ragged.game.tick {
            steady.advance(1. / 60., |g| bot_a.input(g));
        }
        assert_eq!(steady.game.tick, ragged.game.tick);
        assert_eq!(steady.game.state_hash(), ragged.game.state_hash());
        assert_eq!(steady.game.completed(), ragged.game.completed());
        assert_eq!(steady.ghost, ragged.ghost, "identical ghost routes");
    }

    #[test]
    fn edges_are_consumed_once_per_executed_tick() {
        let mut run = Run::new(RunConfig::default(), true);
        let mut pending = true;
        assert_eq!(run.advance(0.001, |_| panic!("no tick yet")), 0);
        run.advance(4. / 60., |_| Input {
            chitter: std::mem::take(&mut pending),
            ..Input::default()
        });
        assert_eq!(run.game.stats.chitters, 1);
        assert!(!pending);
    }

    #[test]
    fn winning_tick_stops_the_batch_and_restart_resets_everything() {
        let mut run = Run::new(RunConfig::default(), true);
        run.game.phase = Phase::Won;
        let mut inputs = 0;
        run.advance(0.1, |_| {
            inputs += 1;
            Input::default()
        });
        assert_eq!(inputs, 1, "nothing runs after the final tick");
        assert_eq!(run.session.state(), SessionState::Won);
        assert_eq!(run.advance(1., |_| panic!("finished")), 0);
        assert!(run.activate());
        assert_eq!(run.game.phase, Phase::Playing);
        assert_eq!(run.session.tick(), 0);
        assert!(run.ghost.is_empty());
    }
}
