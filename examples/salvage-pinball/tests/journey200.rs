//! Journey200 task 164: real controls and supplied time, without moving bodies
//! or assigning scores, lives, timers, waiting flags, or session outcomes.

use ferravex_core::session::SessionState;
use salvage_pinball::{Controls, PinballFeedback, PinballGame, Table};

const TICK: f64 = 1. / 120.;
const LAUNCH: Controls = Controls {
    left: false,
    right: false,
    launch: true,
};
const ALL: Controls = Controls {
    left: true,
    right: true,
    launch: true,
};
const NONE: Controls = Controls {
    left: false,
    right: false,
    launch: false,
};

/// The clock counts the entire issued batch; these observations describe the
/// table that actually executed. Compare exact floats only within this build.
#[derive(Debug, PartialEq)]
struct TableState {
    position: [f32; 2],
    velocity: [f32; 2],
    angles: [f32; 2],
    flippers: [bool; 2],
    score: u32,
    ball_points: u32,
    last_ball_points: Option<u32>,
    lives: u8,
    waiting: bool,
    launches: u32,
    hits: u32,
    drains: u32,
    saves: u32,
    ball_save: f32,
    save_notice: f32,
    drain_notice: f32,
    bumper_flash: [f32; 3],
    bumper_notice: [f32; 3],
    time: f32,
    trail: [[f32; 2]; 12],
}

fn state(table: &Table) -> TableState {
    TableState {
        position: table.position(),
        velocity: table.world.bodies[table.ball].linvel().to_array(),
        angles: table.angles,
        flippers: table.flipper_pressed,
        score: table.score,
        ball_points: table.ball_points(),
        last_ball_points: table.last_ball_points(),
        lives: table.lives,
        waiting: table.waiting,
        launches: table.launches,
        hits: table.hits,
        drains: table.drains,
        saves: table.saves,
        ball_save: table.ball_save,
        save_notice: table.save_notice,
        drain_notice: table.drain_notice,
        bumper_flash: table.bumper_flash,
        bumper_notice: table.bumper_notice,
        time: table.time,
        trail: table.trail,
    }
}

#[derive(Debug, Default, PartialEq)]
struct Totals {
    points: u32,
    hits: u32,
    launches: u32,
    drains: u32,
    saves: u32,
}

impl Totals {
    fn record(&mut self, events: PinballFeedback, table: &Table) {
        // Multipliers, chains, skill shots, jackpots and the overtime secret
        // add to each contact's 100-point base; nothing scores without a cause.
        assert!(events.points >= 100 * events.hits);
        assert!(events.points == 0 || events.hits + events.jackpots + events.secrets > 0);
        assert!(events.saves <= events.drains);
        self.points += events.points;
        self.hits += events.hits;
        self.launches += events.launches;
        self.drains += events.drains;
        self.saves += events.saves;
        assert_eq!(self.points, table.score, "each point is reported once");
        assert_eq!(self.hits, table.hits);
        assert_eq!(self.launches, table.launches);
        assert_eq!(self.drains, table.drains);
        assert_eq!(self.saves, table.saves);
        assert_eq!(u32::from(table.lives), 3 + self.saves - self.drains);
        assert!(table.position().into_iter().all(f32::is_finite));
        assert!(table.world.bodies[table.ball].linvel().length() <= 34.001);
    }
}

/// Press for half a second, release for half a second, and never flip. All
/// transitions fall on frame boundaries at 30, 60, 120 and 144 Hz. Even launch
/// presses made while the ball is in play must be consumed, not deferred.
fn launch_script(half_second: usize) -> Controls {
    if half_second.is_multiple_of(2) {
        LAUNCH
    } else {
        NONE
    }
}

struct Run {
    game: PinballGame,
    checkpoints: Vec<TableState>,
    totals: Totals,
    zero_frames: usize,
    catch_up_frames: usize,
    last_frame_start: u64,
    last_frame_steps: u64,
    flipper_events: usize,
}

fn run_cadence(hz: usize, autoplay_input: Option<Controls>, seconds: usize) -> Run {
    let mut run = Run {
        game: PinballGame::new(autoplay_input.is_some()),
        checkpoints: Vec::with_capacity(seconds * 2),
        totals: Totals::default(),
        zero_frames: 0,
        catch_up_frames: 0,
        last_frame_start: 0,
        last_frame_steps: 0,
        flipper_events: 0,
    };
    for half_second in 0..seconds * 2 {
        for _ in 0..hz / 2 {
            run.last_frame_start = run.game.session().tick();
            let controls = autoplay_input.unwrap_or_else(|| launch_script(half_second));
            let events = run.game.advance(1. / hz as f64, controls);
            run.last_frame_steps = run.game.session().tick() - run.last_frame_start;
            run.zero_frames += usize::from(run.last_frame_steps == 0);
            run.catch_up_frames += usize::from(run.last_frame_steps > 1);
            run.flipper_events += events.flipper_presses.into_iter().filter(|p| *p).count();
            if run.last_frame_steps == 0 {
                assert_eq!(events, PinballFeedback::default());
            }
            run.totals.record(events, run.game.table());
            if run.game.table().over() {
                assert_eq!(run.game.session().state(), SessionState::Lost);
                break;
            }
        }
        run.checkpoints.push(state(run.game.table()));
        if run.game.table().over() {
            break;
        }
        assert_eq!(run.game.session().tick(), (half_second as u64 + 1) * 60);
    }
    run
}

fn assert_cadence(reference: &Run, run: &Run, hz: usize) {
    assert_eq!(run.checkpoints, reference.checkpoints, "cadence {hz} Hz");
    assert_eq!(run.totals, reference.totals, "cadence {hz} Hz");
    assert_eq!(run.game.session().state(), reference.game.session().state());
    if reference.game.table().over() {
        // The reference issued one tick at a time, so its final count is also
        // the executed count. A terminal catch-up frame may issue unused ticks.
        let executed = reference.game.session().tick();
        assert!(run.last_frame_start < executed);
        assert!(executed <= run.game.session().tick());
        assert!(run.game.session().tick() - executed < run.last_frame_steps);
        eprintln!(
            "{hz} Hz: executed={executed}, issued={}, terminal batch={}",
            run.game.session().tick(),
            run.last_frame_steps
        );
    } else {
        assert_eq!(run.game.session().tick(), reference.game.session().tick());
    }
    if hz == 144 {
        assert!(run.zero_frames > 0);
        assert_eq!(run.catch_up_frames, 0);
    } else {
        assert_eq!(run.zero_frames, 0);
        assert!(run.catch_up_frames > 0);
    }
}

#[test]
fn natural_three_ball_loss_and_score_breakdown_match_at_30_60_144_hz() {
    let reference = run_cadence(120, None, 30);
    assert_eq!(reference.game.session().state(), SessionState::Lost);
    assert_eq!(reference.totals.launches, 3);
    assert_eq!(reference.totals.drains, 3);
    assert_eq!(reference.totals.saves, 0);
    assert!(
        reference.totals.hits >= 3,
        "actual bumper contacts must score"
    );
    assert_eq!(reference.game.table().ball_points(), 0);
    let mut lives = 3;
    let mut banked = 0;
    for checkpoint in &reference.checkpoints {
        if checkpoint.lives != lives {
            assert_eq!(checkpoint.lives + 1, lives);
            let points = checkpoint.last_ball_points.unwrap();
            assert!(points > 0, "each played life recovered real contact points");
            banked += points;
            lives = checkpoint.lives;
        }
        assert_eq!(banked + checkpoint.ball_points, checkpoint.score);
    }
    assert_eq!(lives, 0);
    assert_eq!(banked, reference.totals.points);
    for hz in [30, 60, 144] {
        assert_cadence(&reference, &run_cadence(hz, None, 30), hz);
    }
}

#[test]
fn real_autoplay_flippers_and_contacts_match_at_30_60_144_hz() {
    let reference = run_cadence(120, Some(NONE), 60);
    assert!(reference.flipper_events > 0);
    assert!(reference.totals.hits >= 2);
    assert!(reference.totals.launches > 0);
    for hz in [30, 60, 144] {
        // Real autopilot must ignore these conflicting external held controls.
        let run = run_cadence(hz, Some(ALL), 60);
        assert!(run.flipper_events > 0);
        assert_cadence(&reference, &run, hz);
    }
}

#[test]
fn scored_live_ball_survives_pause_then_restart_and_reset_clear_the_run() {
    let mut game = PinballGame::new(false);
    let fresh = state(game.table());
    assert_eq!(game.advance(120., NONE), PinballFeedback::default());
    assert_eq!(state(game.table()), fresh);
    assert_eq!(game.session().state(), SessionState::Ready);
    assert_eq!(game.session().tick(), 0);
    assert!(game.start());
    assert!(!game.start());
    assert!(game.table().waiting);

    let mut reference = PinballGame::new(false);
    assert!(reference.start());
    assert_eq!(game.advance(TICK, LAUNCH), reference.advance(TICK, LAUNCH));
    for _ in 0..600 {
        assert_eq!(game.advance(TICK, NONE), reference.advance(TICK, NONE));
        if game.table().hits > 0 {
            break;
        }
    }
    assert!(game.table().score > 0);
    assert!(game.table().ball_save > 0.);

    // Queue taps and fractional time while the scored ball is still moving.
    assert_eq!(game.advance(0.75 * TICK, ALL), PinballFeedback::default());
    reference.advance(0.75 * TICK, NONE);
    let frozen = state(game.table());
    let tick = game.session().tick();
    assert!(game.set_paused(true));
    assert!(!game.set_paused(true));
    for controls in [ALL, NONE, ALL] {
        assert_eq!(game.advance(60., controls), PinballFeedback::default());
        assert_eq!(state(game.table()), frozen);
        assert_eq!(game.session().tick(), tick);
        assert_eq!(game.session().state(), SessionState::Paused);
    }
    assert!(game.set_paused(false));
    assert!(!game.set_paused(false));
    // Resume retains the pre-pause fraction but discards held/queued actions.
    assert_eq!(
        game.advance(0.25 * TICK, ALL),
        reference.advance(0.25 * TICK, NONE)
    );
    assert_eq!(game.session().tick(), tick + 1);
    assert_eq!(state(game.table()), state(reference.table()));
    assert_eq!(game.advance(0., NONE), PinballFeedback::default());
    let flipped = game.advance(TICK, ALL);
    assert_eq!(flipped.flipper_presses, [true; 2]);
    assert_eq!(flipped.launches, 0, "a mid-play press cannot launch");

    assert_eq!(game.advance(0.75 * TICK, ALL), PinballFeedback::default());
    game.restart();
    assert_eq!(game.session().state(), SessionState::Playing);
    assert_eq!(game.session().tick(), 0);
    assert_eq!(state(game.table()), fresh);
    assert_eq!(game.advance(0.5 * TICK, ALL), PinballFeedback::default());
    assert_eq!(game.session().tick(), 0, "restart drops the old fraction");
    assert_eq!(game.advance(0.5 * TICK, ALL), PinballFeedback::default());
    assert_eq!(game.table().launches, 0);
    assert_eq!(game.table().flipper_pressed, [false; 2]);
    game.advance(0., NONE);
    let relaunched = game.advance(TICK, ALL);
    assert_eq!(relaunched.launches, 1);
    assert_eq!(relaunched.flipper_presses, [true; 2]);
    assert_eq!(game.table().ball_save, 5.);
    game.reset();
    assert_eq!(game.session().state(), SessionState::Ready);
    assert_eq!(game.session().tick(), 0);
    assert_eq!(state(game.table()), fresh);
    assert_eq!(game.advance(60., ALL), PinballFeedback::default());
    assert_eq!(game.session().state(), SessionState::Ready);
    game.advance(0., NONE);
    assert_eq!(game.advance(TICK, LAUNCH).launches, 1);
    assert_eq!(game.session().state(), SessionState::Playing);
}

#[test]
fn sub_tick_taps_coalesce_once_and_match_a_single_tick_press_during_catch_up() {
    let mut game = PinballGame::new(false);
    let fresh = state(game.table());
    for controls in [ALL, NONE, ALL, NONE] {
        assert_eq!(
            game.advance(0.2 * TICK, controls),
            PinballFeedback::default()
        );
        assert_eq!(game.session().tick(), 0);
        assert_eq!(state(game.table()), fresh);
    }
    assert_eq!(game.session().state(), SessionState::Playing);
    let events = game.advance(8.2 * TICK, NONE);
    assert_eq!(game.session().tick(), 9);
    assert_eq!(
        events,
        PinballFeedback {
            launches: 1,
            flipper_presses: [true; 2],
            ..Default::default()
        }
    );
    let mut reference = PinballGame::new(false);
    assert_eq!(reference.advance(TICK, ALL), events);
    for _ in 0..8 {
        assert_eq!(reference.advance(TICK, NONE), PinballFeedback::default());
    }
    assert_eq!(state(game.table()), state(reference.table()));
    assert_eq!(game.table().flipper_pressed, [false; 2]);
    assert_eq!(game.advance(0., NONE), PinballFeedback::default());
    assert_eq!(game.advance(TICK, NONE), reference.advance(TICK, NONE));
    assert_eq!(state(game.table()), state(reference.table()));
}

#[test]
fn held_and_rejected_launches_do_not_serve_a_naturally_drained_replacement() {
    let mut game = PinballGame::new(false);
    assert_eq!(game.advance(1. / 30., LAUNCH).launches, 1);
    // A new press during the same live ball must not become a future serve.
    game.advance(0., NONE);
    assert_eq!(game.advance(1. / 30., LAUNCH).launches, 0);
    for _ in 0..300 {
        let events = game.advance(1. / 30., LAUNCH);
        assert_eq!(events.launches, 0);
        if events.drains > 0 {
            assert_eq!(events.drains, 1);
            assert_eq!(events.saves, 0);
            break;
        }
    }
    assert_eq!(game.table().drains, 1);
    assert_eq!(game.table().lives, 2);
    assert!(game.table().waiting);
    assert!(game.table().last_ball_points().unwrap() > 0);
    assert_eq!(game.table().ball_points(), 0);
    assert_eq!(game.advance(16. * TICK, LAUNCH), PinballFeedback::default());
    assert!(game.table().waiting);
    assert_eq!(game.table().launches, 1);
    // A fresh tap on sub-tick frames does serve this next life exactly once.
    assert_eq!(game.advance(0.25 * TICK, NONE), PinballFeedback::default());
    assert_eq!(
        game.advance(0.25 * TICK, LAUNCH),
        PinballFeedback::default()
    );
    assert_eq!(game.advance(0.5 * TICK, NONE).launches, 1);
    assert_eq!(game.table().launches, 2);
    assert_eq!(game.table().ball_save, 5.);
    assert!(!game.table().waiting);
}

#[test]
fn terminal_catch_up_executes_only_the_losing_tick_then_retry_requires_a_fresh_press() {
    let reference = run_cadence(120, None, 30);
    assert_eq!(reference.game.session().state(), SessionState::Lost);
    let executed = reference.game.session().tick();
    let mut game = PinballGame::new(false);
    for tick in 0..executed - 1 {
        game.advance(TICK, launch_script(tick as usize / 60));
    }
    assert_eq!(game.table().lives, 1);
    let events = game.advance(16. * TICK, launch_script((executed - 1) as usize / 60));
    assert_eq!(events.drains, 1);
    assert_eq!(events.saves, 0);
    assert_eq!(game.session().state(), SessionState::Lost);
    assert_eq!(
        game.session().tick(),
        executed + 15,
        "the entire batch was issued"
    );
    assert_eq!(
        state(game.table()),
        state(reference.game.table()),
        "only the first tick ran"
    );
    let terminal = state(game.table());
    let issued = game.session().tick();
    assert!(!game.start());
    assert!(!game.set_paused(true));
    for elapsed in [0., TICK, 60.] {
        assert_eq!(game.advance(elapsed, ALL), PinballFeedback::default());
        assert_eq!(state(game.table()), terminal);
        assert_eq!(game.session().tick(), issued);
    }
    game.restart();
    let mut fresh = PinballGame::new(false);
    fresh.start();
    assert_eq!(state(game.table()), state(fresh.table()));
    assert_eq!(game.session().tick(), 0);
    assert_eq!(game.advance(TICK, ALL), fresh.advance(TICK, NONE));
    assert_eq!(state(game.table()), state(fresh.table()));
    game.advance(0., NONE);
    assert_eq!(game.advance(TICK, ALL), fresh.advance(TICK, ALL));
    assert_eq!(state(game.table()), state(fresh.table()));
    assert_eq!(game.table().launches, 1);
}

#[test]
fn a_stall_runs_only_the_bounded_batch_and_discards_catch_up_debt() {
    let mut game = PinballGame::new(false);
    let events = game.advance(60., ALL);
    assert_eq!(game.session().tick(), 16);
    assert_eq!(events.launches, 1);
    assert_eq!(events.flipper_presses, [true; 2]);
    let mut reference = PinballGame::new(false);
    let mut totals = Totals::default();
    for _ in 0..16 {
        let events = reference.advance(TICK, ALL);
        totals.record(events, reference.table());
    }
    assert_eq!(state(game.table()), state(reference.table()));
    assert_eq!(game.advance(0., ALL), PinballFeedback::default());
    assert_eq!(game.session().tick(), 16);
    assert_eq!(state(game.table()), state(reference.table()));
    assert_eq!(game.advance(TICK, NONE), reference.advance(TICK, NONE));
    assert_eq!(game.session().tick(), 17);
    assert_eq!(state(game.table()), state(reference.table()));
}
