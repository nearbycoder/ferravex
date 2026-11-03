use crate::{ArcadeGame, Cue, Input, Outcome};
use ferravex_core::session::GameSession;

/// Pure lifecycle: only actually executed ticks consume gameplay commands.
pub(crate) struct Driver<G> {
    pub game: G,
    pub session: GameSession,
    pub ticks: u64,
}
impl<G: ArcadeGame> Driver<G> {
    pub fn new(autoplay: bool) -> Self {
        Self {
            game: G::new(),
            session: GameSession::new(60., 8, autoplay),
            ticks: 0,
        }
    }
    pub fn restart(&mut self) {
        self.game = G::new();
        self.session.restart();
        self.ticks = 0;
    }
    pub fn advance(
        &mut self,
        elapsed: f64,
        mut input: impl FnMut(&G, u64) -> Input,
        mut cue: impl FnMut(Cue, [Option<u8>; 4]),
    ) {
        let batch = self.session.advance(elapsed);
        for _ in 0..batch.steps {
            if !self.session.is_playing() || self.game.outcome() != Outcome::Playing {
                break;
            }
            let command = input(&self.game, self.ticks);
            let feedback = self.game.update(&command);
            cue(feedback, self.game.music_notes());
            self.ticks += 1;
            match self.game.outcome() {
                Outcome::Playing => {}
                Outcome::Won => {
                    self.session.finish(true);
                }
                Outcome::Lost => {
                    self.session.finish(false);
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Hud;
    struct Probe {
        ticks: u32,
        actions: u32,
        terminal: bool,
    }
    impl ArcadeGame for Probe {
        fn new() -> Self {
            Self {
                ticks: 0,
                actions: 0,
                terminal: false,
            }
        }
        fn update(&mut self, i: &Input) -> Cue {
            self.ticks += 1;
            self.actions += u32::from(i.primary);
            if i.secondary {
                self.terminal = true;
            }
            Cue::None
        }
        fn draw(&self) {}
        fn music_notes(&self) -> [Option<u8>; 4] {
            [Some(60 + self.ticks as u8), None, None, None]
        }
        fn hud(&self) -> Hud {
            Hud {
                stage: String::new(),
                objective: String::new(),
                status: String::new(),
                score: self.actions,
            }
        }
        fn outcome(&self) -> Outcome {
            if self.terminal {
                Outcome::Won
            } else {
                Outcome::Playing
            }
        }
        fn summary(&self) -> String {
            String::new()
        }
        fn bot_input(&self, _: u64) -> Input {
            Input::default()
        }
    }
    #[test]
    fn zero_tick_retains_edge_and_catchup_consumes_it_once() {
        let mut driver = Driver::<Probe>::new(true);
        let mut pending = true;
        driver.advance(
            0.,
            |_, _| {
                pending = false;
                Input::default()
            },
            |_, _| {},
        );
        assert!(pending);
        driver.advance(
            3. / 60.,
            |_, _| {
                let primary = std::mem::take(&mut pending);
                Input {
                    primary,
                    ..Input::default()
                }
            },
            |_, _| {},
        );
        assert_eq!((driver.game.ticks, driver.game.actions), (3, 1));
    }
    #[test]
    fn pause_discards_suspension_and_does_not_consume_commands() {
        let mut driver = Driver::<Probe>::new(true);
        driver.session.set_paused(true);
        driver.advance(30., |_, _| panic!("paused input consumed"), |_, _| {});
        assert_eq!(driver.ticks, 0);
        driver.session.set_paused(false);
        driver.advance(1. / 60., |_, _| Input::default(), |_, _| {});
        assert_eq!(driver.ticks, 1);
    }
    #[test]
    fn terminal_tick_stops_remaining_batch_and_cues_only_once() {
        let mut driver = Driver::<Probe>::new(true);
        let mut cues = 0;
        driver.advance(
            4. / 60.,
            |_, _| Input {
                secondary: true,
                ..Input::default()
            },
            |_, _| cues += 1,
        );
        assert_eq!((driver.ticks, cues), (1, 1));
        driver.advance(
            1.,
            |_, _| panic!("terminal input consumed"),
            |_, _| panic!(),
        );
        assert_eq!(driver.game.outcome(), Outcome::Won);
    }
    #[test]
    fn restart_replaces_model_clock_debt_and_execution_count() {
        let mut driver = Driver::<Probe>::new(true);
        driver.advance(
            1.5 / 60.,
            |_, _| Input {
                primary: true,
                ..Input::default()
            },
            |_, _| {},
        );
        driver.restart();
        driver.advance(0., |_, _| panic!(), |_, _| {});
        assert_eq!(
            (driver.game.ticks, driver.game.actions, driver.ticks),
            (0, 0, 0)
        );
    }
    #[test]
    fn render_cadence_does_not_change_executed_updates() {
        for rate in [30, 60, 144] {
            let mut driver = Driver::<Probe>::new(true);
            for _ in 0..rate {
                driver.advance(1. / f64::from(rate), |_, _| Input::default(), |_, _| {});
            }
            assert_eq!(driver.ticks, 60);
        }
    }
    #[test]
    fn musical_events_are_read_once_per_executed_tick_after_update() {
        let mut driver = Driver::<Probe>::new(true);
        let mut notes = Vec::new();
        driver.advance(0., |_, _| Input::default(), |_, n| notes.push(n[0]));
        assert!(notes.is_empty());
        driver.advance(3. / 60., |_, _| Input::default(), |_, n| notes.push(n[0]));
        assert_eq!(notes, vec![Some(61), Some(62), Some(63)]);
    }
}
