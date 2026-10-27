//! Orchard-specific commands around the shared session clock. Scene data,
//! navigation, combat and checkpoint encoding remain owned by their existing systems.
use crate::game::{ActionError, Game, Kind, Outcome};
use ferravex_core::session::{GameSession, SessionState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Build(Kind),
    Upgrade,
    Wave,
    Select(Option<usize>),
    Pause,
    Restart,
    /// Toggle 1x/2x simulation speed. Ticks stay fixed; only how many run
    /// per presentation frame changes.
    Speed,
}

#[derive(Default, Debug)]
pub(super) struct FrameEvents {
    pub restarted: bool,
    pub fired: bool,
    pub killed: bool,
    pub escaped: bool,
    pub finished: Option<bool>,
}

pub(super) struct Campaign {
    pub game: Game,
    pub session: GameSession,
    manual_pause: bool,
    /// Fast-forward runs two fixed ticks per supplied tick interval.
    pub fast: bool,
    notice: String,
    notice_time: f32,
    /// One coalesced HUD purchase with its original plot, waiting for a real tick.
    pending: Option<(Action, usize)>,
}

impl Campaign {
    pub fn new(game: Game, autoplay: bool) -> Self {
        Self {
            game,
            session: GameSession::new(60., 8, autoplay),
            manual_pause: false,
            fast: false,
            notice: String::new(),
            notice_time: 0.,
            pending: None,
        }
    }

    /// Overlay/focus suspension must never undo an intentional pause.
    pub fn suspend(&mut self, blocked: bool) {
        self.session.set_paused(self.manual_pause || blocked);
    }

    pub fn notice(&self) -> Option<&str> {
        (self.notice_time > 0.).then_some(self.notice.as_str())
    }

    #[cfg(test)]
    pub fn frame(
        &mut self,
        elapsed: f64,
        blocked: bool,
        autoplay: bool,
        actions: [Option<Action>; 3],
    ) -> FrameEvents {
        self.frame_with_input(elapsed, blocked, autoplay, actions, || None)
    }

    /// Menus/preparation are immediate. During a wave, consume gameplay keys only
    /// on real ticks and retain one HUD command across zero-tick frames. Focus,
    /// pause and restart discard pending commands; duplicate key/UI activation
    /// coalesces on that tick. The callback must drain all gameplay shortcut edges.
    pub fn frame_with_input(
        &mut self,
        elapsed: f64,
        blocked: bool,
        autoplay: bool,
        actions: [Option<Action>; 3],
        mut tick_action: impl FnMut() -> Option<Action>,
    ) -> FrameEvents {
        // Smoke/autoplay runs keep the original focus-independent simulation;
        // explicit pause still works, and blocked human commands are discarded.
        self.suspend(blocked && !autoplay);
        let mut events = FrameEvents::default();
        if blocked {
            self.pending = None;
            tick_action();
        }
        if !blocked {
            // Speed is a local presentation-rate preference, never a gameplay
            // command: it applies immediately and never competes for the tick.
            if actions.contains(&Some(Action::Speed)) {
                self.fast = !self.fast;
                self.notice = if self.fast {
                    "Fast-forward on: the orchard runs at double speed."
                } else {
                    "Normal speed."
                }
                .into();
                self.notice_time = 2.5;
            }
            let actions = actions.map(|action| action.filter(|&a| a != Action::Speed));
            let mut action = if actions.contains(&Some(Action::Restart)) {
                Some(Action::Restart)
            } else if actions.contains(&Some(Action::Pause)) {
                Some(Action::Pause)
            } else {
                actions.into_iter().flatten().next()
            };
            if action == Some(Action::Restart) {
                self.game = Game::with_level(self.game.level.clone());
                self.session.restart();
                self.manual_pause = false;
                self.notice.clear();
                self.notice_time = 0.;
                self.pending = None;
                tick_action();
                events.restarted = true;
                return events;
            }
            if action == Some(Action::Pause) || self.session.is_paused() {
                self.pending = None;
                tick_action();
            } else if !self.game.active || self.game.outcome != Outcome::Playing {
                let keyboard = tick_action();
                action = keyboard.or(action);
            }
            if let Some(action) = action {
                if self.game.active
                    && self.session.is_playing()
                    && matches!(action, Action::Build(_) | Action::Upgrade | Action::Wave)
                {
                    let queued = (action, self.game.selected);
                    if self.pending.is_some_and(|pending| pending != queued) {
                        self.notice = "A command is waiting for the next orchard tick. Try again after it runs.".into();
                        self.notice_time = 4.;
                    } else {
                        self.pending = Some(queued);
                    }
                } else {
                    self.command(action, self.game.selected);
                }
            }
        }
        let rate = if self.fast && !blocked { 2. } else { 1. };
        let step = self.session.advance(elapsed * rate);
        let (fired, kills, escaped) = (self.game.fired, self.game.kills, self.game.escaped);
        for _ in 0..step.steps {
            if !blocked {
                let keyboard = tick_action();
                if let Some((action, plot)) = self
                    .pending
                    .take()
                    .or_else(|| keyboard.map(|action| (action, self.game.selected)))
                {
                    self.command(action, plot);
                }
            }
            if autoplay {
                self.game.autoplay();
            }
            self.game.step(step.dt);
            self.notice_time = (self.notice_time - step.dt).max(0.);
            if self.game.outcome != Outcome::Playing {
                let won = self.game.outcome == Outcome::Won;
                if self.session.finish(won) {
                    events.finished = Some(won);
                }
                break;
            }
        }
        events.fired = self.game.fired > fired;
        events.killed = self.game.kills > kills;
        events.escaped = self.game.escaped > escaped;
        events
    }

    fn command(&mut self, action: Action, plot: usize) {
        if let Err(error) = self.apply(action, plot) {
            self.notice = error.to_string();
        }
        self.notice_time = 4.;
    }

    fn apply(&mut self, action: Action, plot: usize) -> Result<(), ActionError> {
        if action == Action::Pause {
            if matches!(
                self.session.state(),
                SessionState::Playing | SessionState::Paused
            ) {
                self.manual_pause = !self.manual_pause;
                self.session.set_paused(self.manual_pause);
                self.notice = if self.manual_pause {
                    "Orchard paused."
                } else {
                    "Orchard resumed."
                }
                .into();
            }
            return Ok(());
        }
        if self.session.is_paused() {
            return Err(ActionError::Paused);
        }
        if self.game.outcome != Outcome::Playing {
            return Err(ActionError::Finished);
        }
        match action {
            Action::Build(kind) => {
                self.game.build_status(plot, kind)?;
                self.game.build(plot, kind);
                self.notice = format!(
                    "{} planted. Select its plot to see stats and upgrades.",
                    kind.name()
                );
            }
            Action::Upgrade => {
                self.game.upgrade_status(plot)?;
                self.game.upgrade(plot);
                self.notice = "Tower upgraded: more damage, range and faster reload.".into();
            }
            Action::Wave => {
                self.game.wave_status()?;
                self.game.launch();
                self.session.start();
                self.notice = format!(
                    "Wave {} sent. You can build and upgrade during the wave.",
                    self.game.wave
                );
            }
            Action::Select(plot) => {
                let plot = plot
                    .filter(|&p| p < self.game.towers.len())
                    .ok_or(ActionError::InvalidPlot)?;
                self.game.selected = plot;
                self.notice = if self.game.towers[plot].is_some() {
                    format!(
                        "Plot {} selected. Choose Upgrade to improve this tower.",
                        plot + 1
                    )
                } else {
                    format!(
                        "Plot {} selected. Choose Seed sling or Frost hive below.",
                        plot + 1
                    )
                };
            }
            Action::Pause | Action::Restart | Action::Speed => {
                unreachable!("handled before gameplay actions")
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(c: &mut Campaign, action: Action) -> FrameEvents {
        c.frame(0., false, false, [Some(action), None, None])
    }

    #[test]
    fn combat_commands_wait_for_ticks_and_coalesce_ui_and_keyboard() {
        let mut c = Campaign::new(Game::new(), false);
        frame(&mut c, Action::Build(Kind::Seed));
        frame(&mut c, Action::Wave);
        c.game.money = 1000;
        let mut key = Some(Action::Upgrade);
        c.frame_with_input(0., false, false, [None; 3], || {
            panic!("zero ticks must retain keys")
        });
        assert_eq!(c.game.towers[0].unwrap().level, 0);
        c.frame_with_input(0., false, false, [Some(Action::Upgrade); 3], || {
            panic!("HUD waits too")
        });
        c.frame_with_input(0.1, false, false, [None; 3], || key.take());
        assert_eq!(c.game.towers[0].unwrap().level, 1);
        assert_eq!(
            c.game.money, 935,
            "catch-up and duplicate sources spend once"
        );
        assert!(key.is_none());
        assert_eq!(c.session.tick(), 6);
    }

    #[test]
    fn queued_hud_command_keeps_its_plot_and_cancels_at_lifecycle_boundaries() {
        let mut c = Campaign::new(Game::new(), false);
        frame(&mut c, Action::Wave);
        frame(&mut c, Action::Build(Kind::Seed));
        frame(&mut c, Action::Select(Some(1)));
        frame(&mut c, Action::Build(Kind::Frost));
        assert!(
            c.notice()
                .unwrap()
                .contains("waiting for the next orchard tick")
        );
        c.frame(1. / 60., false, false, [None; 3]);
        assert!(c.game.towers[0].is_some());
        assert!(c.game.towers[1].is_none());
        assert_eq!(c.game.selected, 1);
        for boundary in [Action::Pause, Action::Restart] {
            frame(&mut c, Action::Build(Kind::Frost));
            frame(&mut c, boundary);
            if boundary == Action::Pause {
                frame(&mut c, Action::Pause);
            }
            c.frame(0.1, false, false, [None; 3]);
            assert!(c.game.towers[1].is_none());
        }
        frame(&mut c, Action::Wave);
        frame(&mut c, Action::Build(Kind::Frost));
        c.frame(0., true, false, [None; 3]);
        c.frame(0.1, false, false, [None; 3]);
        assert!(
            c.game.towers.iter().all(Option::is_none),
            "focus loss clears pending HUD input"
        );
    }

    #[test]
    fn preparation_has_no_ticks_and_duplicate_shortcut_ui_spends_once() {
        let mut c = Campaign::new(Game::new(), false);
        c.frame(30., false, false, [None; 3]);
        assert_eq!(c.game.time, 0.);
        c.frame(0., false, false, [Some(Action::Build(Kind::Seed)); 3]);
        assert_eq!(c.game.money, 140);
        c.game.money = 1000;
        c.frame(0., false, false, [Some(Action::Upgrade); 3]);
        assert_eq!(c.game.money, 935);
        assert_eq!(c.game.towers[0].unwrap().level, 1);
        c.frame(0.1, false, false, [Some(Action::Wave); 3]);
        assert_eq!(c.game.wave, 1);
        assert_eq!(c.game.pests.len(), 1);
        assert!(c.session.is_playing());
    }

    #[test]
    fn pause_freezes_spawn_movement_cooldown_and_economy_without_catch_up() {
        let mut c = Campaign::new(Game::new(), false);
        frame(&mut c, Action::Build(Kind::Seed));
        frame(&mut c, Action::Wave);
        c.frame(0.1, false, false, [None; 3]);
        frame(&mut c, Action::Pause);
        let before = (
            c.game.time,
            c.game.money,
            c.game.remaining,
            c.session.tick(),
        );
        let pos = c.game.pests[0].pos;
        let cooldown = c.game.towers[0].unwrap().cooldown;
        c.frame(
            60.,
            false,
            false,
            [Some(Action::Upgrade), Some(Action::Wave), None],
        );
        assert_eq!(
            (
                c.game.time,
                c.game.money,
                c.game.remaining,
                c.session.tick()
            ),
            before
        );
        assert_eq!(c.game.pests[0].pos, pos);
        assert_eq!(c.game.towers[0].unwrap().cooldown, cooldown);
        assert!(c.notice().unwrap().contains("Resume"));
        c.frame(60., true, false, [None; 3]);
        c.frame(60., false, false, [None; 3]);
        assert!(
            c.session.is_paused(),
            "closing settings preserves manual pause"
        );
        frame(&mut c, Action::Pause);
        c.frame(1. / 60., false, false, [None; 3]);
        assert_eq!(c.session.tick(), before.3 + 1);
        assert!(c.game.pests[0].pos != pos);
    }

    #[test]
    fn overlay_discards_commands_and_restart_discards_same_frame_spending() {
        let mut c = Campaign::new(Game::new(), true);
        c.frame(0.1, false, true, [None; 3]);
        let time = c.game.time;
        c.frame(
            120.,
            true,
            false,
            [Some(Action::Restart), Some(Action::Upgrade), None],
        );
        assert_eq!(c.game.time, time);
        c.frame(0., false, false, [None; 3]);
        assert!(c.session.is_playing());
        frame(&mut c, Action::Pause);
        let event = c.frame(
            0.1,
            false,
            true,
            [
                Some(Action::Build(Kind::Seed)),
                Some(Action::Restart),
                Some(Action::Pause),
            ],
        );
        assert!(event.restarted);
        assert!(c.session.is_playing());
        assert_eq!((c.session.tick(), c.game.wave, c.game.time), (0, 0, 0.));
        assert_eq!(
            (c.game.money, c.game.lives),
            (c.game.level.money, c.game.level.lives)
        );
        assert!(c.game.towers.iter().all(Option::is_none));
        assert!(c.game.pests.is_empty() && c.game.shots.is_empty());
        assert_eq!((c.game.fired, c.game.kills, c.game.escaped), (0, 0, 0));
        assert!(c.notice().is_none());
    }

    #[test]
    fn plot_rejections_and_pause_priority_do_not_spend_or_retarget() {
        let mut c = Campaign::new(Game::new(), false);
        frame(&mut c, Action::Select(Some(3)));
        frame(&mut c, Action::Select(None));
        assert_eq!(c.game.selected, 3);
        assert_eq!(c.game.money, c.game.level.money);
        assert!(c.notice().unwrap().contains("round plot"));
        frame(&mut c, Action::Build(Kind::Frost));
        frame(&mut c, Action::Wave);
        let money = c.game.money;
        c.frame(
            1.,
            false,
            false,
            [Some(Action::Upgrade), Some(Action::Pause), None],
        );
        assert!(c.session.is_paused());
        assert_eq!(c.game.money, money);
        assert_eq!(c.game.towers[3].unwrap().level, 0);
        frame(&mut c, Action::Pause);
        c.frame(0.1, false, false, [None; 3]);
        assert_eq!(
            c.game.money, money,
            "rejected input must not leak into resume"
        );
    }

    #[test]
    fn autoplay_win_and_defenseless_loss_finish_once_and_retry_cleanly() {
        for autoplay in [true, false] {
            let mut c = Campaign::new(Game::new(), autoplay);
            let mut finishes = 0;
            for _ in 0..24000 {
                let action = (!autoplay && !c.game.active).then_some(Action::Wave);
                let event = c.frame(1. / 60., false, autoplay, [action, None, None]);
                finishes += usize::from(event.finished.is_some());
                if c.game.outcome != Outcome::Playing {
                    break;
                }
            }
            assert_eq!(finishes, 1);
            assert_eq!(
                c.session.state(),
                if autoplay {
                    SessionState::Won
                } else {
                    SessionState::Lost
                }
            );
            let frozen = (c.game.money, c.game.time, c.session.tick());
            let event = c.frame(60., false, autoplay, [Some(Action::Wave), None, None]);
            assert_eq!(event.finished, None);
            assert_eq!((c.game.money, c.game.time, c.session.tick()), frozen);
            assert!(frame(&mut c, Action::Restart).restarted);
            assert!(c.session.is_playing());
            assert_eq!(c.game.wave, 0);
        }
    }

    #[test]
    fn unattended_autoplay_ignores_focus_but_respects_explicit_pause() {
        let mut c = Campaign::new(Game::new(), true);
        c.frame(0.1, true, true, [Some(Action::Restart), None, None]);
        assert!(c.game.time > 0.);
        assert_eq!(c.game.wave, 1);
        frame(&mut c, Action::Pause);
        let time = c.game.time;
        c.frame(30., true, true, [None; 3]);
        assert_eq!(c.game.time, time);
        assert!(c.session.is_paused());
    }

    #[test]
    fn fast_forward_runs_the_same_fixed_ticks_twice_as_often_and_never_while_blocked() {
        let mut normal = Campaign::new(Game::new(), false);
        let mut fast = Campaign::new(Game::new(), false);
        for c in [&mut normal, &mut fast] {
            frame(c, Action::Build(Kind::Seed));
        }
        // Speed applies with a same-frame command and is not a purchase.
        fast.frame(
            0.,
            false,
            false,
            [Some(Action::Speed), Some(Action::Wave), None],
        );
        frame(&mut normal, Action::Wave);
        assert!(fast.fast && fast.game.active);
        assert_eq!(fast.game.money, normal.game.money);
        for _ in 0..90 {
            normal.frame(1. / 60., false, false, [None; 3]);
        }
        for _ in 0..45 {
            fast.frame(1. / 60., false, false, [None; 3]);
        }
        assert_eq!(normal.session.tick(), 90);
        assert_eq!(fast.session.tick(), 90);
        assert_eq!(fast.game.time, normal.game.time);
        assert_eq!(fast.game.fired, normal.game.fired);
        let positions = |c: &Campaign| c.game.pests.iter().map(|p| p.pos).collect::<Vec<_>>();
        assert_eq!(positions(&fast), positions(&normal));
        let tick = fast.session.tick();
        fast.frame(1. / 60., true, false, [Some(Action::Speed), None, None]);
        assert!(fast.fast, "blocked frames ignore speed taps");
        assert_eq!(fast.session.tick(), tick);
        // Restart keeps the player's speed preference but nothing else.
        frame(&mut fast, Action::Restart);
        assert!(fast.fast && fast.game.wave == 0);
        frame(&mut fast, Action::Speed);
        assert!(!fast.fast);
    }
}
