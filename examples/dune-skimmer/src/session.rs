//! Headless host lifecycle: fixed vehicle ticks, pause, finish and input rearming.
use ferravex_core::session::GameSession;

use crate::race::{Course, Input, Race};

pub(super) struct RaceSession {
    pub race: Race,
    pub session: GameSession,
    pub alpha: f32,
    pub waiting_for_controls: bool,
}

impl RaceSession {
    pub fn new() -> Self {
        Self {
            race: Race::new(),
            // Ordinary play also starts directly in the authored countdown.
            session: GameSession::new(120.0, 16, true),
            alpha: 1.0,
            waiting_for_controls: false,
        }
    }

    pub fn restart(&mut self) {
        self.race = Race::new();
        self.session.restart();
        self.alpha = 1.0;
        self.waiting_for_controls = true;
    }

    pub fn advance(&mut self, course: &Course, input: Input, autoplay: bool, elapsed: f64) {
        let mut input = input;
        if self.waiting_for_controls && !autoplay {
            // Analog sticks get a small neutral tolerance. Holding any drive
            // control across a restart/teleport cannot move the fresh craft.
            if input.throttle.abs() <= 0.05 && input.steer.abs() <= 0.05 && !input.boost {
                self.waiting_for_controls = false;
            }
            input = Input::default();
        }
        if !self.session.is_playing() {
            // Keep the last interpolated pose when paused; inactive FrameStep
            // alpha is zero and would otherwise move the craft backwards.
            self.session.advance(elapsed);
            return;
        }
        let frame = self.session.advance(elapsed);
        self.alpha = frame.alpha;
        for _ in 0..frame.steps {
            let recoveries = self.race.recovery_count;
            self.race.step(
                course,
                if autoplay {
                    self.race.autoplay(course)
                } else {
                    input
                },
                frame.dt,
            );
            if self.race.recovery_count != recoveries {
                self.waiting_for_controls = !autoplay;
                input = Input::default();
                self.alpha = 1.0;
            }
            if self.race.finished {
                self.session.finish(true);
                self.alpha = 1.0;
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_core::session::SessionState;
    use ferravex_render::prelude::{Vec2, vec2};

    const DRIVE: Input = Input {
        throttle: 1.0,
        steer: 0.4,
        boost: true,
    };

    #[test]
    fn pause_freezes_countdown_time_and_interpolation_without_catchup() {
        let course = Course::new();
        let mut trial = RaceSession::new();
        trial.advance(&course, DRIVE, false, 1.5 / 120.0);
        let countdown = trial.race.countdown;
        let alpha = trial.alpha;
        let tick = trial.session.tick();
        trial.session.set_paused(true);
        trial.advance(&course, DRIVE, false, 600.0);
        assert_eq!(trial.race.countdown, countdown);
        assert_eq!(trial.alpha, alpha);
        assert_eq!(trial.session.tick(), tick);
        trial.race.countdown = 0.0;
        trial.advance(&course, DRIVE, false, 600.0);
        assert_eq!(trial.race.elapsed, 0.0);
        trial.session.set_paused(false);
        trial.advance(&course, DRIVE, false, 1.0 / 120.0);
        assert_eq!(trial.session.tick(), tick + 1);
        assert!((trial.race.elapsed - 1.0 / 120.0).abs() < 0.00001);
    }

    #[test]
    fn restart_clears_clock_race_feedback_and_requires_control_release() {
        let course = Course::new();
        let mut trial = RaceSession::new();
        trial.race.countdown = 0.0;
        trial.advance(&course, DRIVE, false, 0.1);
        trial.race.recovery_timer = 2.0;
        trial.race.last_gate_timer = 0.8;
        trial.session.set_paused(true);
        trial.restart();
        assert_eq!(trial.session.state(), SessionState::Playing);
        assert_eq!(trial.session.tick(), 0);
        assert_eq!(trial.race.elapsed, 0.0);
        assert_eq!(trial.race.boost, 1.0);
        assert_eq!(trial.race.recovery_timer, 0.0);
        assert_eq!(trial.race.last_gate_timer, 0.0);
        assert_eq!(trial.race.velocity, Vec2::ZERO);
        assert_eq!(trial.race.bank, 0.0);
        assert_eq!(trial.race.previous_position, trial.race.position);
        trial.race.countdown = 0.0;
        let position = trial.race.position;
        for _ in 0..60 {
            trial.advance(&course, DRIVE, false, 1.0 / 60.0);
        }
        assert_eq!(trial.race.position, position);
        assert!(!trial.race.boosting);
        trial.advance(&course, Input::default(), false, 0.0);
        trial.advance(&course, DRIVE, false, 1.0 / 60.0);
        assert!(trial.race.speed() > 0.0);
        assert!(trial.race.boosting);
    }

    #[test]
    fn recovery_neutralizes_remaining_catchup_ticks_and_held_input() {
        let course = Course::new();
        let mut trial = RaceSession::new();
        trial.race.countdown = 0.0;
        trial.race.position = vec2(500.0, 500.0);
        trial.advance(&course, DRIVE, false, 0.1);
        assert_eq!(trial.race.recovery_count, 1);
        assert!(trial.waiting_for_controls);
        assert_eq!(trial.race.speed(), 0.0);
        assert!(!trial.race.boosting);
        assert!(!trial.race.off_track);
        let position = trial.race.position;
        trial.advance(&course, DRIVE, false, 0.1);
        assert_eq!(trial.race.position, position);
        assert_eq!(trial.race.next_gate, 1);
    }

    #[test]
    fn autoplay_finishes_session_freezes_result_and_can_restart() {
        let course = Course::new();
        let mut trial = RaceSession::new();
        trial.restart();
        for _ in 0..120 * 60 {
            trial.advance(&course, Input::default(), true, 1.0 / 60.0);
            if !trial.session.is_playing() {
                break;
            }
        }
        assert_eq!(trial.session.state(), SessionState::Won);
        assert_eq!(trial.race.gates_completed(), 42);
        assert_eq!(trial.race.recovery_count, 0);
        let elapsed = trial.race.elapsed;
        let tick = trial.session.tick();
        let position = trial.race.position;
        trial.advance(&course, DRIVE, true, 30.0);
        assert_eq!(trial.race.elapsed, elapsed);
        assert_eq!(trial.race.position, position);
        assert_eq!(trial.session.tick(), tick);
        trial.restart();
        assert_eq!(trial.session.state(), SessionState::Playing);
        assert_eq!(trial.session.tick(), 0);
        assert_eq!(trial.race.gates_completed(), 0);
        assert_eq!(trial.race.countdown, 2.8);
    }
}
