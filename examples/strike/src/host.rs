//! Host timing is deliberately separate from match/replay tick authority.
use ferravex_core::{FixedClock, FrameStep, session::GameSession};
use ferravex_fps::{InputFrame, TICK_RATE};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Mode {
    Practice,
    Replay,
    Network,
}

pub(super) struct Host {
    practice: GameSession,
    network: FixedClock,
    replay: FixedClock,
    active: bool,
    has_played: bool,
}

impl Host {
    pub(super) fn new(autoplay: bool) -> Self {
        Self {
            practice: GameSession::new(TICK_RATE as f64, 8, autoplay),
            network: FixedClock::new(TICK_RATE as f64, 8),
            replay: FixedClock::new(TICK_RATE as f64, 8),
            active: autoplay,
            has_played: autoplay,
        }
    }

    pub(super) fn active(&self) -> bool {
        self.active
    }

    pub(super) fn has_played(&self) -> bool {
        self.has_played
    }

    pub(super) fn resume(&mut self) {
        self.active = true;
        self.has_played = true;
        self.practice.start();
        self.practice.set_paused(false);
    }

    pub(super) fn open_menu(&mut self) {
        self.active = false;
        self.practice.set_paused(true);
    }

    /// Called only alongside replacement of the practice world and bots.
    /// Network and playback worlds cannot accidentally be restarted by this UI.
    pub(super) fn restart_practice(&mut self, mode: Mode) -> bool {
        if mode != Mode::Practice {
            return false;
        }
        self.practice.restart();
        self.resume();
        true
    }

    pub(super) fn return_to_briefing(&mut self) {
        self.open_menu();
        self.practice.reset();
        self.has_played = false;
    }

    /// Preserve aim while sending a neutral command from menus/settings.
    /// PlayerInput owns pending edges and is consumed only on actual ticks.
    pub(super) fn command(&self, mut input: InputFrame, available: bool) -> InputFrame {
        if !self.active || !available {
            input.move_x = 0.;
            input.move_z = 0.;
            input.buttons = 0;
        }
        input
    }

    pub(super) fn advance(
        &mut self,
        mode: Mode,
        elapsed: f64,
        available: bool,
        replay_running: bool,
    ) -> FrameStep {
        self.practice
            .set_paused(mode != Mode::Practice || !self.active || !available);
        match mode {
            Mode::Network => self.network.advance(elapsed),
            Mode::Practice => self.practice.advance(elapsed),
            // The recording, including its seek/pause state, still owns all
            // replay ticks. Paused wall time is never queued for playback.
            Mode::Replay => self
                .replay
                .advance(if replay_running { elapsed } else { 0. }),
        }
    }

    pub(super) fn cut_replay(&mut self) {
        self.replay.reset();
        // Do not reset network cadence when the view changes or a menu opens.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_core::session::SessionState;
    use ferravex_fps::BUTTON_FIRE;

    const DT: f64 = 1. / TICK_RATE as f64;

    #[test]
    fn practice_waits_for_start_and_pauses_without_catch_up() {
        let mut host = Host::new(false);
        let mut game = crate::practice();
        assert_eq!(host.practice.state(), SessionState::Ready);
        assert_eq!(host.advance(Mode::Practice, 10., true, true).steps, 0);
        host.resume();
        for _ in 0..host.advance(Mode::Practice, DT, true, true).steps {
            game.step(&[]);
        }
        let hash = game.state_hash();
        host.open_menu();
        for _ in 0..host.advance(Mode::Practice, 60., false, true).steps {
            game.step(&[]);
        }
        assert_eq!(game.state_hash(), hash);
        assert_eq!(host.practice.state(), SessionState::Paused);
        host.resume();
        assert_eq!(host.advance(Mode::Practice, DT, true, true).steps, 1);
        assert_eq!(host.practice.tick(), 2);
    }

    #[test]
    fn menu_and_focus_loss_keep_network_ticking_with_neutral_input() {
        let mut host = Host::new(true);
        let held = InputFrame {
            yaw: 1.,
            pitch: 0.2,
            move_x: 1.,
            move_z: 1.,
            buttons: BUTTON_FIRE,
            ..Default::default()
        };
        host.command(held, true);
        host.open_menu();
        for _ in 0..64 {
            let neutral = host.command(held, false);
            assert_eq!(
                (neutral.move_x, neutral.move_z, neutral.buttons),
                (0., 0., 0)
            );
            assert_eq!((neutral.yaw, neutral.pitch), (1., 0.2));
            assert_eq!(host.advance(Mode::Network, DT, false, false).steps, 1);
        }
        assert_eq!(host.network.tick(), 64);
        host.resume();
        assert_eq!(host.command(InputFrame::default(), true).buttons, 0);
        assert_eq!(host.advance(Mode::Network, DT, false, false).steps, 1);
    }

    #[test]
    fn settings_and_pause_discard_gameplay_commands() {
        let mut host = Host::new(true);
        let fire = InputFrame {
            buttons: BUTTON_FIRE,
            ..Default::default()
        };
        assert_eq!(host.command(fire, true).buttons, BUTTON_FIRE);
        assert_eq!(host.command(fire, false).buttons, 0);
        assert_eq!(host.advance(Mode::Practice, 30., false, true).steps, 0);
        host.open_menu();
        assert_eq!(host.command(fire, true).buttons, 0);
        host.resume();
        assert_eq!(host.command(InputFrame::default(), true).buttons, 0);
        assert_eq!(host.advance(Mode::Practice, DT, true, true).steps, 1);
    }

    #[test]
    fn replay_pause_and_seek_do_not_advance_practice_or_reset_network() {
        let mut host = Host::new(true);
        assert_eq!(host.advance(Mode::Network, DT / 2., true, true).steps, 0);
        host.cut_replay();
        assert_eq!(host.advance(Mode::Network, DT / 2., true, true).steps, 1);
        assert_eq!(host.advance(Mode::Replay, DT, true, true).steps, 1);
        assert_eq!(host.advance(Mode::Replay, 30., true, false).steps, 0);
        host.cut_replay();
        assert_eq!(host.advance(Mode::Replay, DT, true, true).steps, 1);
        host.open_menu();
        assert_eq!(host.advance(Mode::Replay, DT, false, true).steps, 1);
        assert_eq!(host.advance(Mode::Replay, 30., false, false).steps, 0);
        host.resume();
        assert_eq!(host.practice.tick(), 0);
        assert_eq!(host.advance(Mode::Practice, DT, true, true).steps, 1);
        assert_eq!(host.practice.tick(), 1);
    }

    #[test]
    fn retry_is_practice_only_and_briefing_clears_progress() {
        let mut host = Host::new(true);
        host.advance(Mode::Practice, DT, true, true);
        host.open_menu();
        assert!(!host.restart_practice(Mode::Network));
        assert!(!host.restart_practice(Mode::Replay));
        assert!(!host.active());
        assert_eq!(host.practice.tick(), 1);
        assert!(host.restart_practice(Mode::Practice));
        assert!(host.active());
        assert_eq!(host.practice.tick(), 0);
        host.return_to_briefing();
        assert!(!host.has_played());
        assert_eq!(host.practice.state(), SessionState::Ready);
    }
}
