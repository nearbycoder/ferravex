//! Headless scenarios using authored courses and the host's real Trial/Run path.
//! No body, pickup, checkpoint, timer or outcome is assigned to arrange a test.
//! Rendering, audio, persistence and the async host itself are outside this suite.

use crate::{
    courses::COURSES,
    level::{Level, MAX_CHECKPOINTS},
    sim::{Controls, Run},
    trial::Trial,
};
use ferravex_core::session::SessionState;
use ferravex_render::{
    input::{self, PadState, PlayerInput},
    prelude::{Quat, Vec2, Vec3},
};
use std::sync::Arc;

const DT: f64 = 1. / 120.;
const MAX_TICKS: usize = 120 * 180;

#[derive(Debug, PartialEq)]
struct RunState {
    position: Vec3,
    rotation: Quat,
    velocity: Vec3,
    spin: Vec3,
    previous: (Vec3, Quat),
    collected: Vec<bool>,
    checkpoint: usize,
    splits: [Option<u64>; MAX_CHECKPOINTS],
    elapsed: f32,
    falls: u32,
    finished: bool,
    notice: f32,
    recovered: bool,
    sweepers: Vec<(Vec3, Quat)>,
}

impl RunState {
    fn read(run: &Run) -> Self {
        let body = &run.physics.bodies[run.marble];
        Self {
            position: run.position(),
            rotation: run.rotation(),
            velocity: body.linvel(),
            spin: body.angvel(),
            previous: (run.previous_position, run.previous_rotation),
            collected: run.collected.clone(),
            checkpoint: run.checkpoint,
            splits: run.splits,
            elapsed: run.elapsed,
            falls: run.falls,
            finished: run.finished,
            notice: run.notice,
            recovered: run.recovered,
            sweepers: run
                .sweepers
                .iter()
                .map(|&handle| {
                    let body = &run.physics.bodies[handle];
                    (body.translation(), *body.rotation())
                })
                .collect(),
        }
    }
}

#[derive(Debug, PartialEq)]
struct TickState {
    run: RunState,
    countdown: Option<u32>,
    showing_go: bool,
    session: SessionState,
}

#[derive(Debug)]
struct Batch {
    issued: u32,
    executed: u32,
    dropped: f64,
}

struct Scenario {
    trial: Trial,
    run: Run,
    executed: usize,
    physics_ticks: usize,
}

fn idle(_: &mut Trial, _: &mut Run) -> Controls {
    Controls::default()
}

fn autoplay(trial: &mut Trial, run: &mut Run) -> Controls {
    if trial.countdown().is_none() {
        run.autopilot()
    } else {
        Controls::default()
    }
}

impl Scenario {
    fn new(course: usize, picker: bool) -> Self {
        Self {
            trial: Trial::new(picker),
            run: Run::with_level(Arc::new(Level::parse(COURSES[course].source).unwrap())),
            executed: 0,
            physics_ticks: 0,
        }
    }

    fn frame(
        &mut self,
        elapsed: f64,
        blocked: bool,
        controls: impl FnMut(&mut Trial, &mut Run) -> Controls,
    ) -> Batch {
        self.observe_frame(elapsed, blocked, controls, |_| {})
    }

    fn observe_frame(
        &mut self,
        elapsed: f64,
        blocked: bool,
        mut controls: impl FnMut(&mut Trial, &mut Run) -> Controls,
        mut observe: impl FnMut(&Self),
    ) -> Batch {
        let before = self.trial.session.tick();
        let frame = self.trial.advance(elapsed, blocked);
        assert!(frame.steps <= 12);
        assert_eq!(self.trial.session.tick() - before, u64::from(frame.steps));
        let mut executed = 0;
        for _ in 0..frame.steps {
            // Same guard as game(): advance issues the entire batch up front.
            if !self.trial.session.is_playing() {
                break;
            }
            assert!(self.executed < MAX_TICKS, "scenario tick bound exceeded");
            let physics_tick = self.trial.countdown().is_none();
            let input = controls(&mut self.trial, &mut self.run);
            self.trial.step(&mut self.run, input);
            executed += 1;
            self.executed += 1;
            self.physics_ticks += usize::from(physics_tick);
            assert!(self.run.position().is_finite());
            observe(self);
        }
        Batch {
            issued: frame.steps,
            executed,
            dropped: frame.dropped_seconds,
        }
    }

    fn countdown(&mut self) {
        for _ in 0..360 {
            self.frame(DT, false, idle);
        }
        assert_eq!(self.trial.countdown(), None);
        assert!(self.trial.showing_go());
        assert_eq!(self.run.elapsed, 0.);
        assert_eq!(self.physics_ticks, 0);
    }

    fn finish(&mut self) {
        for _ in 0..MAX_TICKS {
            if self.run.finished {
                assert_eq!(self.trial.session.state(), SessionState::Won);
                assert_eq!(self.run.gems(), self.run.level.gems.len());
                return;
            }
            self.frame(DT, false, autoplay);
        }
        panic!(
            "ordinary autoplay did not finish: {:?}",
            self.run.position()
        );
    }

    fn retry(&mut self) {
        // Same simulation reset as the host's pending-course/reload branch.
        self.run = Run::with_level(self.run.level.clone());
        self.trial.restart();
        self.executed = 0;
        self.physics_ticks = 0;
    }

    fn state(&self) -> TickState {
        TickState {
            run: RunState::read(&self.run),
            countdown: self.trial.countdown(),
            showing_go: self.trial.showing_go(),
            session: self.trial.session.state(),
        }
    }
}

#[test]
fn all_authored_courses_match_every_executed_tick_at_30_60_144_hz() {
    for (course, definition) in COURSES.iter().enumerate() {
        // One bounded reference trace; subsequent cadences compare as they run.
        let mut reference = Vec::new();
        for hz in [30, 60, 144] {
            let mut game = Scenario::new(course, true);
            assert_eq!(game.frame(45., false, idle).issued, 0);
            assert_eq!(game.trial.session.state(), SessionState::Ready);
            game.trial.toggle_picker();
            let mut zero_frames = 0;
            let mut catchup_frames = 0;
            let mut terminal = None;
            for _ in 0..hz * 180 {
                let batch = game.observe_frame(1. / f64::from(hz), false, autoplay, |game| {
                    if hz == 30 {
                        reference.push(game.state());
                    } else {
                        assert_eq!(
                            game.state(),
                            reference[game.executed - 1],
                            "{} at {hz} Hz, executed tick {}",
                            definition.id,
                            game.executed,
                        );
                    }
                });
                assert_eq!(batch.dropped, 0.);
                zero_frames += usize::from(batch.issued == 0);
                catchup_frames += usize::from(batch.issued > 1);
                if game.run.finished {
                    terminal = Some(batch);
                    break;
                }
                assert_eq!(batch.executed, batch.issued);
            }
            let terminal = terminal.expect("authored course must finish within 180 seconds");
            assert_eq!(game.executed, reference.len());
            assert_eq!(game.executed - game.physics_ticks, 360);
            assert_eq!(game.trial.session.state(), SessionState::Won);
            assert!(!game.trial.resumable(&game.run));
            assert_eq!(game.run.gems(), game.run.level.gems.len());
            assert!(game.run.splits.iter().any(Option::is_some));
            assert!(
                game.run.position().distance(game.run.level.finish)
                    < game.run.level.tuning.finish_radius
            );
            assert_eq!(
                game.trial.session.tick(),
                game.executed as u64 + u64::from(terminal.issued - terminal.executed),
            );
            if hz == 144 {
                assert!(zero_frames > 0);
            } else {
                assert!(catchup_frames > 0);
            }
            eprintln!(
                "{} {hz} Hz: {} executed / {} physics / {} issued ticks, terminal {}/{}",
                definition.id,
                game.executed,
                game.physics_ticks,
                game.trial.session.tick(),
                terminal.executed,
                terminal.issued,
            );
        }
    }
}

#[test]
fn earned_progress_survives_picker_and_manual_pause_then_retry_matches_fresh() {
    let mut game = Scenario::new(2, true); // Real moving Clockwork sweepers.
    let initial = game.state();
    assert!(!game.trial.resumable(&game.run));
    assert_eq!(game.frame(60., false, autoplay).executed, 0);
    assert_eq!(game.state(), initial);
    game.trial.toggle_picker();
    game.countdown();

    let mut uninterrupted = Scenario::new(2, false);
    uninterrupted.countdown();
    for _ in 0..120 * 90 {
        game.frame(DT, false, autoplay);
        uninterrupted.frame(DT, false, autoplay);
        if game.run.checkpoint > 0 && game.run.gems() > 0 {
            break;
        }
    }
    assert!(game.run.checkpoint > 0 && game.run.gems() > 0);
    assert!(game.run.splits[game.run.checkpoint - 1].is_some());
    assert_eq!(game.state(), uninterrupted.state());
    let saved = RunState::read(&game.run);
    let pose = game.trial.pose(&game.run);
    let visual_time = game.trial.visual_time(&game.run);
    let tick = game.trial.session.tick();

    game.trial.request_recovery();
    game.trial.toggle_picker();
    assert_eq!(game.frame(60., false, autoplay).executed, 0);
    assert!(game.trial.resumable(&game.run));
    game.trial.toggle_picker();
    assert_eq!(game.frame(60., true, autoplay).executed, 0);
    assert_eq!(game.trial.session.state(), SessionState::Paused);
    assert_eq!(game.trial.session.tick(), tick);
    assert_eq!(RunState::read(&game.run), saved);
    assert_eq!(game.trial.pose(&game.run), pose);
    assert_eq!(game.trial.visual_time(&game.run), visual_time);
    game.frame(DT, false, autoplay);
    uninterrupted.frame(DT, false, autoplay);
    assert_eq!(game.state(), uninterrupted.state());

    game.trial.request_recovery();
    let level = game.run.level.clone();
    game.retry();
    assert!(Arc::ptr_eq(&game.run.level, &level));
    let mut fresh = Scenario::new(2, false);
    assert_eq!(game.state(), fresh.state());
    assert_eq!(game.trial.session.tick(), 0);
    assert_eq!(game.trial.pose(&game.run).0, level.spawn);
    for _ in 0..360 + 120 {
        game.frame(DT, false, autoplay);
        fresh.frame(DT, false, autoplay);
        assert_eq!(game.state(), fresh.state());
    }
    assert_eq!(game.run.falls, 0, "retry must discard the queued recovery");
}

#[test]
fn rolling_off_the_course_recovers_earned_checkpoint_and_still_allows_a_win() {
    let mut game = Scenario::new(0, false);
    game.countdown();
    for _ in 0..120 * 60 {
        game.frame(DT, false, autoplay);
        if game.run.checkpoint == 1 {
            break;
        }
    }
    assert_eq!(game.run.checkpoint, 1);
    assert!(game.run.gems() >= 2);
    let collected = game.run.collected.clone();
    let splits = game.run.splits;
    let elapsed = game.run.elapsed;
    let falls = game.run.falls;
    let mut fell_below_platform = false;
    for _ in 0..120 * 15 {
        game.frame(DT, false, |_, _| Controls {
            roll: Vec2::X,
            brake: false,
        });
        fell_below_platform |= game.run.position().y < -1.;
        if game.run.falls > falls {
            break;
        }
    }
    assert!(
        fell_below_platform,
        "the marble must actually leave its platform"
    );
    assert_eq!(game.run.falls, falls + 1);
    assert_eq!(game.run.checkpoint, 1);
    assert_eq!(game.run.collected, collected);
    assert_eq!(game.run.splits, splits);
    assert!(game.run.elapsed > elapsed);
    assert!(game.run.recovered);
    assert_eq!(game.run.notice, 2.);
    assert_eq!(game.run.position(), game.run.level.checkpoints[0].position);
    assert_eq!(game.run.previous_position, game.run.position());
    assert_eq!(
        game.run.physics.bodies[game.run.marble].linvel(),
        Vec3::ZERO
    );
    assert_eq!(
        game.run.physics.bodies[game.run.marble].angvel(),
        Vec3::ZERO
    );
    assert_eq!(game.trial.session.state(), SessionState::Playing);
    game.finish();
    assert_eq!(game.run.falls, falls + 1);
    assert_eq!(game.run.splits[0], splits[0]);
    assert!(game.run.splits[1].unwrap() > splits[0].unwrap());
}

fn pad_frame(game: &mut Scenario, player: &mut PlayerInput, elapsed: f64, down: bool) -> Batch {
    input::host_frame(elapsed as f32, 640, 480, || {
        let mut pad = PadState {
            connected: true,
            ..Default::default()
        };
        pad.buttons[4] = f32::from(down);
        assert!(input::gamepad_sample(0, pad));
        player.sample(!game.trial.picker && !player.paused() && !game.run.finished);
        game.frame(elapsed, !player.gameplay_active(), |trial, _| {
            if player.pressed("recover") {
                trial.request_recovery();
            }
            Controls::default()
        })
    })
}

#[test]
fn recovery_taps_cross_zero_ticks_go_and_context_changes_without_replaying() {
    let mut game = Scenario::new(0, false);
    let mut player = PlayerInput::transient("journey200-166");
    crate::trial_actions(&mut player);
    pad_frame(&mut game, &mut player, 0., false); // Connect neutral.
    for _ in 0..359 {
        pad_frame(&mut game, &mut player, DT, false);
    }
    assert_eq!(game.trial.countdown(), Some(1));
    assert_eq!(pad_frame(&mut game, &mut player, DT / 4., true).issued, 0);
    assert_eq!(pad_frame(&mut game, &mut player, DT / 4., false).issued, 0);
    assert_eq!(
        pad_frame(&mut game, &mut player, 2.5 * DT, false).executed,
        3
    );
    assert_eq!(game.trial.countdown(), None);
    assert_eq!(game.physics_ticks, 2);
    assert_eq!(game.run.falls, 0, "a countdown tap cannot carry across GO");

    // Two quick taps coalesce into the journal's single pending action.
    for down in [true, false, true, false] {
        assert_eq!(pad_frame(&mut game, &mut player, DT / 8., down).issued, 0);
    }
    assert_eq!(game.run.falls, 0);
    assert_eq!(
        pad_frame(&mut game, &mut player, 11.5 * DT, false).executed,
        12
    );
    assert_eq!(game.run.falls, 1);
    pad_frame(&mut game, &mut player, 12. * DT, false);
    assert_eq!(game.run.falls, 1);

    for picker in [true, false] {
        pad_frame(&mut game, &mut player, 0., true);
        game.trial.request_recovery(); // Pointer and journal requests both clear.
        if picker {
            game.trial.toggle_picker();
        } else {
            player.set_paused(true);
        }
        assert_eq!(pad_frame(&mut game, &mut player, 30., true).issued, 0);
        if picker {
            game.trial.toggle_picker();
        } else {
            player.set_paused(false);
        }
        pad_frame(&mut game, &mut player, 3. * DT, true);
        assert_eq!(game.run.falls, 1, "held recovery cannot leak out of a menu");
        pad_frame(&mut game, &mut player, 0., false);
    }

    pad_frame(&mut game, &mut player, 0., true);
    game.trial.request_recovery();
    game.retry();
    player.set_paused(true);
    player.set_paused(false); // Host retry discards journal edges too.
    for _ in 0..361 {
        pad_frame(&mut game, &mut player, DT, true);
    }
    assert_eq!(game.run.falls, 0);
    pad_frame(&mut game, &mut player, 0., false);
    pad_frame(&mut game, &mut player, 0., true);
    pad_frame(&mut game, &mut player, 0., false);
    pad_frame(&mut game, &mut player, DT, false);
    assert_eq!(game.run.falls, 1, "a fresh post-retry tap must still work");
}

#[test]
fn terminal_catchup_counts_issued_ticks_separately_and_never_advances_the_run() {
    let mut reference = Scenario::new(0, false);
    reference.finish();
    let mut game = Scenario::new(0, false);
    for _ in 0..reference.executed - 1 {
        game.frame(DT, false, autoplay);
    }
    assert!(!game.run.finished);
    let batch = game.frame(12. * DT, false, autoplay);
    assert_eq!((batch.issued, batch.executed), (12, 1));
    assert_eq!(game.executed, reference.executed);
    assert_eq!(game.physics_ticks, reference.physics_ticks);
    assert_eq!(
        game.trial.session.tick(),
        reference.trial.session.tick() + 11
    );
    assert_eq!(game.state(), reference.state());
    assert_eq!(
        game.trial.pose(&game.run),
        (game.run.position(), game.run.rotation())
    );
    let tick = game.trial.session.tick();
    for blocked in [false, true, false] {
        game.trial.request_recovery();
        assert_eq!(game.frame(60., blocked, autoplay).issued, 0);
        assert!(!game.trial.recover(&mut game.run));
        // Even accidental calls for already-budgeted ticks cannot mutate a win.
        game.trial.step(&mut game.run, Controls::default());
        assert_eq!(game.state(), reference.state());
        assert_eq!(game.trial.session.tick(), tick);
    }
    game.retry();
    game.finish();
    assert_eq!(game.state(), reference.state());
    assert_eq!(game.executed, reference.executed);
}

#[test]
fn invalid_elapsed_and_over_budget_frames_never_charge_unsimulated_time() {
    let mut game = Scenario::new(2, false);
    let mut reference = Scenario::new(2, false);
    game.countdown();
    reference.countdown();
    for elapsed in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1., 0.] {
        assert_eq!(game.frame(elapsed, false, autoplay).issued, 0);
        assert_eq!(game.state(), reference.state());
    }
    let batch = game.frame(1., false, autoplay);
    assert_eq!((batch.issued, batch.executed), (12, 12));
    assert!((batch.dropped - 0.9).abs() < 1e-12);
    for _ in 0..12 {
        reference.frame(DT, false, autoplay);
    }
    assert_eq!(game.state(), reference.state());
    assert_eq!(game.frame(0., false, autoplay).issued, 0);
    assert_eq!(game.state(), reference.state());
}
