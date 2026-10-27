//! Host lifecycle for a trial. Countdown ticks never enter the physics timer;
//! picker, focus and player pauses all stop the shared session clock.
use ferravex_core::{FrameStep, session::GameSession};
use ferravex_render::prelude::{Quat, Vec3};

use crate::sim::{Controls, Run};

const HZ: u32 = 120;
const COUNTDOWN_TICKS: u32 = 3 * HZ;

pub(crate) struct Trial {
    pub session: GameSession,
    pub picker: bool,
    countdown_ticks: u32,
    go_ticks: u32,
    alpha: f32,
    pending_recovery: bool,
}

impl Trial {
    pub fn new(picker: bool) -> Self {
        Self {
            session: GameSession::new(HZ as f64, 12, !picker),
            picker,
            countdown_ticks: COUNTDOWN_TICKS,
            go_ticks: 0,
            alpha: 1.,
            pending_recovery: false,
        }
    }

    /// Use for retries, course changes and accepted content reloads.
    pub fn restart(&mut self) {
        self.session.restart();
        self.picker = false;
        self.countdown_ticks = COUNTDOWN_TICKS;
        self.go_ticks = 0;
        self.alpha = 1.;
        self.pending_recovery = false;
    }

    pub fn toggle_picker(&mut self) {
        self.picker = !self.picker;
        if !self.picker {
            self.session.start();
        }
    }

    pub fn resumable(&self, run: &Run) -> bool {
        self.session.is_started() && !run.finished
    }

    pub fn countdown(&self) -> Option<u32> {
        (self.countdown_ticks > 0).then(|| self.countdown_ticks.div_ceil(HZ))
    }

    /// 0..1 progress through the current countdown second, for presentation.
    pub fn countdown_progress(&self) -> f32 {
        ((HZ - self.countdown_ticks % HZ) % HZ) as f32 / HZ as f32
    }

    pub fn showing_go(&self) -> bool {
        self.go_ticks > 0
    }

    /// The picker is independent of a manual pause; closing it cannot resume one.
    pub fn advance(&mut self, elapsed: f64, blocked: bool) -> FrameStep {
        self.session.set_paused(self.picker || blocked);
        let frame = self.session.advance(elapsed);
        // Inactive sessions report alpha=0. Keep the last rendered pose instead
        // of rewinding the ball and hazards to the previous physics tick.
        if self.session.is_playing() {
            self.alpha = frame.alpha;
        } else {
            self.pending_recovery = false;
        }
        frame
    }

    pub fn step(&mut self, run: &mut Run, input: Controls) {
        if !self.session.is_playing() {
            return;
        }
        if self.countdown_ticks > 0 {
            self.countdown_ticks -= 1;
            if self.countdown_ticks == 0 {
                self.go_ticks = HZ;
            }
            return;
        }
        if std::mem::take(&mut self.pending_recovery) {
            self.recover(run);
        }
        self.go_ticks = self.go_ticks.saturating_sub(1);
        run.step(input, self.session.dt());
        if run.finished {
            self.session.finish(true);
            self.alpha = 1.;
        }
    }

    pub fn can_recover(&self) -> bool {
        self.session.is_playing() && self.countdown_ticks == 0
    }

    /// Queue a pointer or gameplay-edge request for the next actual physics tick.
    pub fn request_recovery(&mut self) {
        if self.can_recover() {
            self.pending_recovery = true;
        }
    }

    pub fn recover(&mut self, run: &mut Run) -> bool {
        if !self.can_recover() || run.finished {
            return false;
        }
        run.recover();
        // Recovery resets both body poses, but does not reset the trial clock.
        self.alpha = 1.;
        true
    }

    pub fn pose(&self, run: &Run) -> (Vec3, Quat) {
        (
            run.previous_position.lerp(run.position(), self.alpha),
            run.previous_rotation.slerp(run.rotation(), self.alpha),
        )
    }

    pub fn visual_time(&self, run: &Run) -> f32 {
        (run.elapsed - self.session.dt() * (1. - self.alpha)).max(0.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{courses::COURSES, level::Level};
    use ferravex_core::session::SessionState;
    use ferravex_physics::d3::Vector;
    use std::sync::Arc;

    fn frame(trial: &mut Trial, run: &mut Run, seconds: f64, blocked: bool) {
        let frame = trial.advance(seconds, blocked);
        for _ in 0..frame.steps {
            trial.step(run, Controls::default());
        }
    }

    fn countdown(trial: &mut Trial, run: &mut Run) {
        for _ in 0..180 {
            frame(trial, run, 1. / 60., false);
        }
        assert_eq!(trial.countdown(), None);
        assert_eq!(run.elapsed, 0.);
    }

    #[test]
    fn ready_countdown_and_pause_never_charge_trial_time_or_move_sweepers() {
        let mut run = Run::with_level(Arc::new(Level::parse(COURSES[2].source).unwrap()));
        let mut trial = Trial::new(true);
        let spawn = run.position();
        let sweeper = *run.physics.bodies[run.sweepers[0]].rotation();
        frame(&mut trial, &mut run, 100., false);
        assert_eq!(trial.session.state(), SessionState::Ready);
        assert_eq!(trial.session.tick(), 0);
        trial.toggle_picker();
        for _ in 0..60 {
            frame(&mut trial, &mut run, 1. / 60., false);
        }
        assert_eq!(trial.countdown(), Some(2));
        frame(&mut trial, &mut run, 100., true);
        assert_eq!(trial.countdown(), Some(2));
        assert_eq!(trial.session.tick(), 120);
        assert_eq!(run.elapsed, 0.);
        assert_eq!(run.position(), spawn);
        assert_eq!(*run.physics.bodies[run.sweepers[0]].rotation(), sweeper);
        for _ in 0..120 {
            frame(&mut trial, &mut run, 1. / 60., false);
        }
        assert_eq!(trial.countdown(), None);
        assert!(trial.showing_go());
        assert_eq!(run.elapsed, 0.);
        frame(&mut trial, &mut run, 1. / 60., false);
        assert!((run.elapsed - 1. / 60.).abs() < 1e-6);
    }

    #[test]
    fn picker_resumes_countdown_and_preserves_an_independent_player_pause() {
        let mut run = Run::new();
        let mut trial = Trial::new(false);
        frame(&mut trial, &mut run, 0.1, false);
        assert!(trial.resumable(&run)); // elapsed is still zero during countdown.
        let tick = trial.session.tick();
        trial.toggle_picker();
        frame(&mut trial, &mut run, 60., false);
        trial.toggle_picker();
        frame(&mut trial, &mut run, 60., true);
        assert!(trial.session.is_paused());
        assert_eq!(trial.session.tick(), tick);
        frame(&mut trial, &mut run, 1. / 120., false);
        assert_eq!(trial.session.tick(), tick + 1);
    }

    #[test]
    fn paused_pose_timer_feedback_and_hazards_stay_on_the_same_frame() {
        let mut run = Run::with_level(Arc::new(Level::parse(COURSES[2].source).unwrap()));
        let mut trial = Trial::new(false);
        countdown(&mut trial, &mut run);
        run.physics.bodies[run.marble].set_linvel(Vector::new(2., 0., 0.), true);
        run.notice = 2.;
        frame(&mut trial, &mut run, 1.5 / 120., false);
        let pose = trial.pose(&run);
        let visual_time = trial.visual_time(&run);
        let elapsed = run.elapsed;
        let notice = run.notice;
        let tick = trial.session.tick();
        let sweeper = *run.physics.bodies[run.sweepers[0]].rotation();
        frame(&mut trial, &mut run, 60., true);
        assert_eq!(trial.pose(&run), pose);
        assert_eq!(trial.visual_time(&run), visual_time);
        assert_eq!(run.elapsed, elapsed);
        assert_eq!(run.notice, notice);
        assert_eq!(trial.session.tick(), tick);
        assert_eq!(*run.physics.bodies[run.sweepers[0]].rotation(), sweeper);
        assert!(!trial.recover(&mut run));
        frame(&mut trial, &mut run, 1. / 120., false);
        assert!((run.elapsed - elapsed - 1. / 120.).abs() < 1e-6);
    }

    #[test]
    fn recovery_and_retry_reset_pose_without_rewinding_a_live_trial() {
        let mut run = Run::new();
        let mut trial = Trial::new(false);
        assert!(!trial.recover(&mut run));
        countdown(&mut trial, &mut run);
        frame(&mut trial, &mut run, 0.1, false);
        run.checkpoint = 1;
        run.collected[0] = true;
        let tick = trial.session.tick();
        let elapsed = run.elapsed;
        assert!(trial.recover(&mut run));
        assert_eq!(trial.session.tick(), tick);
        assert_eq!(run.elapsed, elapsed);
        assert_eq!(run.gems(), 1);
        assert_eq!(run.falls, 1);
        assert_eq!(trial.pose(&run).0, run.level.checkpoints[0].position);
        run = Run::with_level(run.level.clone());
        trial.restart();
        assert_eq!(trial.session.tick(), 0);
        assert_eq!(trial.countdown(), Some(3));
        assert_eq!(
            (run.elapsed, run.notice, run.falls, run.gems()),
            (0., 0., 0, 0)
        );
        assert_eq!(trial.pose(&run).0, run.level.spawn);
    }

    #[test]
    fn all_courses_complete_through_session_countdown_and_ordinary_autoplay() {
        let mut trial = Trial::new(false);
        for course in &COURSES {
            let mut run = Run::with_level(Arc::new(Level::parse(course.source).unwrap()));
            trial.restart();
            for _ in 0..60 * 180 {
                let frame = trial.advance(1. / 60., false);
                for _ in 0..frame.steps {
                    let input = if trial.countdown().is_none() {
                        run.autopilot()
                    } else {
                        Controls::default()
                    };
                    trial.step(&mut run, input);
                }
                if run.finished {
                    break;
                }
            }
            assert!(run.finished, "{} failed to finish", course.id);
            assert_eq!(trial.session.state(), SessionState::Won);
            assert!(!trial.resumable(&run));
            let elapsed = run.elapsed;
            frame(&mut trial, &mut run, 60., false);
            assert_eq!(run.elapsed, elapsed);
            assert!(!trial.recover(&mut run));
        }
    }

    #[test]
    fn queued_pointer_recovery_waits_for_tick_and_clears_on_pause_or_restart() {
        let mut run = Run::new();
        let mut trial = Trial::new(false);
        trial.request_recovery(); // Countdown cannot bank a recovery.
        countdown(&mut trial, &mut run);
        assert_eq!(run.falls, 0);
        trial.request_recovery();
        frame(&mut trial, &mut run, 0., false);
        assert_eq!(run.falls, 0);
        let elapsed = run.elapsed;
        frame(&mut trial, &mut run, 3. / 120., false);
        assert_eq!(run.falls, 1);
        assert!((run.elapsed - elapsed - 3. / 120.).abs() < 1e-6);
        frame(&mut trial, &mut run, 3. / 120., false);
        assert_eq!(run.falls, 1);
        trial.request_recovery();
        frame(&mut trial, &mut run, 1., true);
        frame(&mut trial, &mut run, 1. / 120., false);
        assert_eq!(run.falls, 1);
        trial.request_recovery();
        trial.restart();
        run = Run::new();
        countdown(&mut trial, &mut run);
        frame(&mut trial, &mut run, 1. / 120., false);
        assert_eq!(run.falls, 0);
    }

    #[test]
    fn actual_player_tap_survives_zero_tick_frames_and_fires_once_in_catchup() {
        use ferravex_render::input::{self, PadState, PlayerInput};
        let mut player = PlayerInput::transient("marble-ticks");
        crate::trial_actions(&mut player);
        let mut run = Run::new();
        let mut trial = Trial::new(false);
        countdown(&mut trial, &mut run);
        let mut pad = PadState {
            connected: true,
            ..Default::default()
        };
        let mut pump = |seconds: f64, pad: PadState, enabled: bool| {
            input::host_frame(seconds as f32, 640, 480, || {
                input::gamepad_sample(0, pad);
                player.sample(enabled);
                let ticks = trial.advance(seconds, !player.gameplay_active());
                for _ in 0..ticks.steps {
                    if player.pressed("recover") {
                        trial.request_recovery();
                    }
                    trial.step(&mut run, Controls::default());
                }
                run.falls
            })
        };
        pump(0., pad, true); // Connect neutral; reconnect suppression must remain.
        pad.buttons[4] = 1.;
        assert_eq!(pump(0., pad, true), 0);
        pad.buttons[4] = 0.;
        assert_eq!(pump(0., pad, true), 0);
        assert_eq!(pump(3. / 120., pad, true), 1);
        assert_eq!(pump(3. / 120., pad, true), 1);
        pad.buttons[4] = 1.;
        pump(0., pad, true);
        pump(0., pad, false); // A menu invalidates the pending gameplay edge.
        pad.buttons[4] = 0.;
        pump(0., pad, false);
        assert_eq!(pump(3. / 120., pad, true), 1);
    }
}
