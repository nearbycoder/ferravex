//! Task 167: headless scenarios through the shipped race and session paths.
//! No fixture writes race state, skips the countdown, or awards progress.

use crate::race::{
    BoostStatus, Course, GATES, HOVER_HEIGHT, Input, LAPS, RECOVERY_PENALTY, Race, Traction, world,
};
use crate::session::RaceSession;
use ferravex_core::session::SessionState;
use ferravex_play::vehicle::PlanarTelemetry;
use ferravex_render::prelude::Vec2;

const TICK: f64 = 1.0 / 120.0;
const MAX_RACE_TICKS: usize = 120 * 120;
const DRIVE: Input = Input {
    throttle: 1.0,
    steer: 0.0,
    boost: true,
};

/// Observe every accessible race field, plus derived private motor/recovery state.
/// Equality here is exact on this native target, not a cross-platform guarantee.
#[derive(Debug, PartialEq)]
struct Snapshot {
    position: Vec2,
    previous_position: Vec2,
    velocity: Vec2,
    yaw: f32,
    previous_yaw: f32,
    boost: f32,
    boosting: bool,
    boost_status: BoostStatus,
    next_gate: usize,
    laps: u32,
    gates: usize,
    elapsed: f32,
    countdown: f32,
    finished: bool,
    off_track: bool,
    impacts: u32,
    last_gate_timer: f32,
    bank: f32,
    recovery_count: u32,
    recovery_timer: f32,
    recovery_in: Option<f32>,
    penalty: f32,
    driving: f32,
    cooling: f32,
    traction: Traction,
    telemetry: PlanarTelemetry,
}

impl Snapshot {
    fn of(race: &Race) -> Self {
        Self {
            position: race.position,
            previous_position: race.previous_position,
            velocity: race.velocity,
            yaw: race.yaw,
            previous_yaw: race.previous_yaw,
            boost: race.boost,
            boosting: race.boosting,
            boost_status: race.boost_status(),
            next_gate: race.next_gate,
            laps: race.laps,
            gates: race.gates_completed(),
            elapsed: race.elapsed,
            countdown: race.countdown,
            finished: race.finished,
            off_track: race.off_track,
            impacts: race.impacts,
            last_gate_timer: race.last_gate_timer,
            bank: race.bank,
            recovery_count: race.recovery_count,
            recovery_timer: race.recovery_timer,
            recovery_in: race.recovery_in(),
            penalty: race.penalty_seconds(),
            driving: race.driving_seconds(),
            cooling: race.cooling_seconds(),
            traction: race.traction(),
            telemetry: race.telemetry(),
        }
    }
}

fn finish_countdown(trial: &mut RaceSession, course: &Course) {
    let start = trial.race.position;
    for _ in 0..360 {
        if trial.race.countdown == 0.0 {
            break;
        }
        trial.advance(course, Input::default(), false, TICK);
        assert_eq!(trial.race.position, start);
        assert_eq!(trial.race.elapsed, 0.0);
        assert_eq!(trial.race.gates_completed(), 0);
    }
    assert_eq!(trial.race.countdown, 0.0, "countdown must actually expire");
}

fn earn_two_gates(trial: &mut RaceSession, course: &Course) {
    for _ in 0..120 * 15 {
        if trial.race.gates_completed() >= 2 {
            break;
        }
        // Submit ordinary controls, including through the real countdown.
        let input = trial.race.autoplay(course);
        trial.advance(course, input, false, TICK);
    }
    assert_eq!(trial.race.gates_completed(), 2);
    assert!(trial.race.elapsed > 0.0);
    assert!(trial.race.speed() > 0.0);
    assert_eq!(trial.race.recovery_count, 0);
}

/// A bounded, independently counted one-tick trace is the batch oracle. Its
/// final index counts updates actually executed, unlike GameSession::tick(),
/// which can include unused ticks issued in the final frame's batch.
fn autoplay_trace(course: &Course) -> Vec<Snapshot> {
    let mut race = Race::new();
    let mut trace = Vec::with_capacity(MAX_RACE_TICKS + 1);
    trace.push(Snapshot::of(&race));
    let mut boost_ticks = 0;
    let mut countdown_ticks = 0;
    let mut gate_ticks = Vec::with_capacity(GATES * LAPS as usize);
    for tick in 1..=MAX_RACE_TICKS {
        let gate = race.next_gate;
        let laps = race.laps;
        let gates = race.gates_completed();
        let counting_down = race.countdown > 0.0;
        race.step(course, race.autoplay(course), TICK as f32);
        countdown_ticks += usize::from(counting_down);
        boost_ticks += usize::from(race.boosting);
        assert!(race.position.is_finite() && race.velocity.is_finite());
        assert!((0.0..=1.0).contains(&race.boost));
        if counting_down {
            assert_eq!(race.position, trace[0].position);
            assert_eq!(race.elapsed, 0.0);
            assert_eq!(race.gates_completed(), 0);
        }
        if race.next_gate != gate {
            assert!(course.gates[gate].crossed(
                world(race.previous_position, HOVER_HEIGHT),
                world(race.position, HOVER_HEIGHT),
            ));
            assert_eq!(race.next_gate, (gate + 1) % GATES);
            assert_eq!(race.gates_completed(), gates + 1);
            assert_eq!(race.laps, laps + u32::from(gate == 0));
            assert_eq!(gate, (gate_ticks.len() + 1) % GATES);
            gate_ticks.push(tick);
        } else {
            assert_eq!(race.gates_completed(), gates);
            assert_eq!(race.laps, laps);
        }
        trace.push(Snapshot::of(&race));
        if race.finished {
            break;
        }
    }
    assert!(
        race.finished,
        "real controls must finish within the scenario budget"
    );
    assert_eq!(gate_ticks.len(), GATES * LAPS as usize);
    assert_eq!(gate_ticks.last().copied(), Some(trace.len() - 1));
    assert_eq!(race.laps, LAPS);
    assert_eq!(race.recovery_count, 0);
    assert_eq!(race.velocity, Vec2::ZERO);
    assert!(!race.boosting);
    assert_eq!(race.boost_status(), BoostStatus::Finished);
    assert!(boost_ticks > 0);
    assert!((336..=337).contains(&countdown_ticks));
    eprintln!(
        "167 reference: countdown={countdown_ticks}, executed={}, gates={}, boost_ticks={boost_ticks}, elapsed={}",
        trace.len() - 1,
        gate_ticks.len(),
        race.elapsed,
    );
    trace
}

#[test]
fn actual_countdown_driving_pause_resume_and_held_restart() {
    let course = Course::new();
    let mut trial = RaceSession::new();
    let initial = Snapshot::of(&trial.race);
    assert_eq!(trial.session.state(), SessionState::Playing);
    assert_eq!(trial.race.boost_status(), BoostStatus::Countdown);
    trial.advance(&course, DRIVE, false, TICK * 1.5);
    assert_eq!(trial.session.tick(), 1);
    assert!(trial.race.countdown < initial.countdown);
    assert_eq!(trial.race.position, initial.position);
    let countdown = Snapshot::of(&trial.race);
    let alpha = trial.alpha;
    assert!(trial.session.set_paused(true));
    trial.advance(&course, DRIVE, false, 600.0);
    assert_eq!(Snapshot::of(&trial.race), countdown);
    assert_eq!(trial.alpha, alpha);
    assert_eq!(trial.session.tick(), 1);
    assert!(trial.session.set_paused(false));
    trial.advance(&course, DRIVE, false, TICK * 0.5);
    assert_eq!(trial.session.tick(), 2);
    assert_eq!(trial.race.elapsed, 0.0);
    earn_two_gates(&mut trial, &course);

    let input = trial.race.autoplay(&course);
    trial.advance(&course, input, false, TICK * 0.5);
    let driving = Snapshot::of(&trial.race);
    let tick = trial.session.tick();
    let alpha = trial.alpha;
    assert!((alpha - 0.5).abs() < 0.00001);
    assert!(trial.session.set_paused(true));
    for elapsed in [0.0, 1.0 / 30.0, 600.0] {
        trial.advance(&course, DRIVE, false, elapsed);
        assert_eq!(Snapshot::of(&trial.race), driving);
        assert_eq!(trial.session.tick(), tick);
        assert_eq!(trial.alpha, alpha);
    }
    assert!(trial.session.set_paused(false));
    trial.advance(&course, input, false, TICK * 0.5);
    assert_eq!(trial.session.tick(), tick + 1);
    assert_eq!(trial.race.elapsed, driving.elapsed + TICK as f32);
    assert_ne!(trial.race.position, driving.position);

    // Restart a dirty, paused race with half a tick of debt and earned gates.
    trial.advance(&course, DRIVE, false, TICK * 0.5);
    trial.session.set_paused(true);
    trial.restart();
    assert_eq!(Snapshot::of(&trial.race), initial);
    assert_eq!(trial.session.state(), SessionState::Playing);
    assert_eq!(trial.session.tick(), 0);
    assert_eq!(trial.alpha, 1.0);
    assert!(trial.waiting_for_controls);
    trial.advance(&course, DRIVE, false, TICK * 0.5);
    assert_eq!(
        trial.session.tick(),
        0,
        "restart must clear old fractional debt"
    );
    for _ in 0..4 * 60 {
        trial.advance(&course, DRIVE, false, 1.0 / 60.0);
    }
    assert_eq!(trial.race.countdown, 0.0);
    assert!(trial.race.elapsed > 1.0);
    assert_eq!(trial.race.position, initial.position);
    assert_eq!(trial.race.velocity, Vec2::ZERO);
    assert_eq!(trial.race.boost, 1.0);
    assert_eq!(trial.race.gates_completed(), 0);
    assert!(trial.waiting_for_controls);
    let released = Snapshot::of(&trial.race);
    trial.advance(&course, Input::default(), false, 0.0);
    assert!(!trial.waiting_for_controls);
    assert_eq!(Snapshot::of(&trial.race), released);
    trial.advance(&course, DRIVE, false, TICK * 0.5);
    assert!(trial.race.speed() > 0.0 && trial.race.boosting);
    trial.restart();
    assert_eq!(Snapshot::of(&trial.race), initial);
}

#[test]
fn full_race_matches_every_supplied_30_60_144_hz_frame() {
    let course = Course::new();
    let trace = autoplay_trace(&course);
    let executed = trace.len() - 1;
    for hz in [30_usize, 60, 144] {
        let mut trial = RaceSession::new();
        let mut zero_tick_frames = 0;
        let mut catchup_frames = 0;
        let mut frames = 0;
        for frame in 1..=hz * 120 {
            let previous_tick = trial.session.tick();
            trial.advance(&course, Input::default(), true, 1.0 / hz as f64);
            frames = frame;
            // These schedules never exceed the 16-tick budget. Derive ticks
            // from supplied time independently of the session implementation.
            let issued = frame * 120 / hz;
            assert_eq!(trial.session.tick(), issued as u64, "{hz} Hz frame {frame}");
            zero_tick_frames += usize::from(previous_tick == issued as u64);
            catchup_frames += usize::from(issued as u64 - previous_tick > 1);
            assert_eq!(
                Snapshot::of(&trial.race),
                trace[issued.min(executed)],
                "{hz} Hz frame {frame}, issued={issued}, executed={executed}",
            );
            assert!((0.0..=1.0).contains(&trial.alpha));
            if trial.race.finished {
                assert!(previous_tick < executed as u64);
                assert_eq!(trial.session.state(), SessionState::Won);
                assert_eq!(trial.alpha, 1.0);
                break;
            }
            assert_eq!(trial.session.state(), SessionState::Playing);
        }
        assert_eq!(
            trial.session.state(),
            SessionState::Won,
            "{hz} Hz must finish"
        );
        assert_eq!(trial.race.gates_completed(), 42);
        if hz == 144 {
            assert!(zero_tick_frames > 0);
            assert_eq!(catchup_frames, 0);
        } else {
            assert_eq!(zero_tick_frames, 0);
            assert_eq!(catchup_frames, frames);
        }
        eprintln!(
            "167 {hz} Hz: frames={frames}, zero={zero_tick_frames}, catchup={catchup_frames}, issued={}, executed={executed}",
            trial.session.tick(),
        );
    }
}

#[test]
fn final_gate_stops_the_issued_batch_and_results_stay_frozen_until_restart() {
    let course = Course::new();
    let trace = autoplay_trace(&course);
    let executed = trace.len() - 1;
    let mut trial = RaceSession::new();
    while trial.session.tick() < (executed - 1) as u64 {
        let steps = (executed - 1 - trial.session.tick() as usize).min(16);
        trial.advance(&course, Input::default(), true, steps as f64 * TICK);
    }
    assert_eq!(Snapshot::of(&trial.race), trace[executed - 1]);
    assert_eq!(trial.race.gates_completed(), 41);
    assert_eq!(trial.race.next_gate, 0);
    assert!(!trial.race.finished);
    trial.advance(&course, Input::default(), true, 16.0 * TICK);
    assert_eq!(trial.session.state(), SessionState::Won);
    assert_eq!(trial.session.tick(), (executed + 15) as u64);
    // Race::step changes previous_position even on a finished race. Comparing
    // history as well as final time catches unwanted calls after the final gate.
    assert_ne!(trace[executed].previous_position, trace[executed].position);
    assert_eq!(Snapshot::of(&trial.race), trace[executed]);
    for autoplay in [false, true] {
        for elapsed in [0.0, TICK, 0.1, 600.0] {
            trial.advance(&course, DRIVE, autoplay, elapsed);
            assert_eq!(Snapshot::of(&trial.race), trace[executed]);
            assert_eq!(trial.session.tick(), (executed + 15) as u64);
            assert_eq!(trial.alpha, 1.0);
        }
    }
    assert!(!trial.session.set_paused(true));
    trial.restart();
    assert_eq!(Snapshot::of(&trial.race), trace[0]);
    assert_eq!(trial.session.tick(), 0);
    for frame in 1..=4 * 60 {
        trial.advance(&course, Input::default(), true, 1.0 / 60.0);
        assert_eq!(Snapshot::of(&trial.race), trace[frame * 2]);
    }
    assert!(trial.race.speed() > 0.0);
}

#[test]
fn held_drive_samples_across_zero_ticks_catchup_and_dropped_time() {
    let course = Course::new();
    let mut trial = RaceSession::new();
    let mut reference = RaceSession::new();
    finish_countdown(&mut trial, &course);
    finish_countdown(&mut reference, &course);
    let ready = Snapshot::of(&trial.race);
    let tick = trial.session.tick();

    // Movement and boost are continuous held controls in the shipped loop,
    // not queued gameplay edges. A complete tap between ticks does no work.
    trial.advance(&course, DRIVE, false, TICK * 0.25);
    trial.advance(&course, Input::default(), false, TICK * 0.25);
    assert_eq!(trial.session.tick(), tick);
    assert_eq!(Snapshot::of(&trial.race), ready);
    trial.advance(&course, Input::default(), false, TICK * 0.5);
    reference.advance(&course, Input::default(), false, TICK);
    assert_eq!(Snapshot::of(&trial.race), Snapshot::of(&reference.race));
    assert_eq!(trial.race.speed(), 0.0);
    assert_eq!(trial.race.boost, 1.0);

    // A held press spanning the next tick drives all eight actual catch-up
    // updates, identically to eight separately submitted fixed ticks.
    trial.advance(&course, DRIVE, false, TICK * 0.25);
    trial.advance(&course, DRIVE, false, TICK * 7.75);
    for _ in 0..8 {
        reference.advance(&course, DRIVE, false, TICK);
    }
    assert_eq!(Snapshot::of(&trial.race), Snapshot::of(&reference.race));
    assert_eq!(trial.session.tick(), tick + 9);
    assert!(trial.race.speed() > 0.0 && trial.race.boosting);
    assert!(trial.race.boost < 1.0);
    let boosting = Snapshot::of(&trial.race);
    trial.advance(&course, Input::default(), false, TICK * 0.25);
    assert_eq!(Snapshot::of(&trial.race), boosting);
    trial.advance(&course, Input::default(), false, TICK * 0.75);
    reference.advance(&course, Input::default(), false, TICK);
    assert_eq!(Snapshot::of(&trial.race), Snapshot::of(&reference.race));
    assert!(!trial.race.boosting);
    assert!(trial.race.boost > boosting.boost);

    let before_stall = trial.session.tick();
    trial.advance(&course, DRIVE, false, 60.0);
    for _ in 0..16 {
        reference.advance(&course, DRIVE, false, TICK);
    }
    assert_eq!(trial.session.tick(), before_stall + 16);
    assert_eq!(Snapshot::of(&trial.race), Snapshot::of(&reference.race));
    let after_stall = Snapshot::of(&trial.race);
    for elapsed in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        trial.advance(&course, DRIVE, false, elapsed);
        assert_eq!(Snapshot::of(&trial.race), after_stall);
        assert_eq!(trial.session.tick(), before_stall + 16);
    }
    trial.advance(&course, Input::default(), false, TICK);
    reference.advance(&course, Input::default(), false, TICK);
    assert_eq!(Snapshot::of(&trial.race), Snapshot::of(&reference.race));
}

#[test]
fn earned_progress_survives_a_real_sand_recovery_and_held_catchup() {
    let course = Course::new();
    let mut trial = RaceSession::new();
    earn_two_gates(&mut trial, &course);
    // Stop steering and actually drive off the circuit. Never teleport or
    // change a timer to manufacture the recovery boundary.
    let mut saw_sand = false;
    for _ in 0..120 * 30 {
        if trial
            .race
            .recovery_in()
            .is_some_and(|left| left < TICK as f32 * 2.0)
        {
            break;
        }
        trial.advance(&course, DRIVE, false, TICK);
        saw_sand |= trial.race.off_track;
        assert_eq!(
            trial.race.recovery_count, 0,
            "must observe the real grace period"
        );
    }
    assert!(saw_sand);
    assert_eq!(trial.race.traction(), Traction::Sand);
    assert!(!trial.race.boosting);
    assert!(
        trial
            .race
            .recovery_in()
            .is_some_and(|left| left < TICK as f32 * 2.0)
    );
    let before = Snapshot::of(&trial.race);
    let tick = trial.session.tick();
    trial.session.set_paused(true);
    trial.advance(&course, DRIVE, false, 600.0);
    assert_eq!(Snapshot::of(&trial.race), before);
    assert_eq!(trial.session.tick(), tick);
    trial.session.set_paused(false);
    trial.advance(&course, DRIVE, false, TICK * 16.0);
    assert_eq!(trial.session.tick(), tick + 16);
    assert_eq!(trial.race.recovery_count, 1);
    assert_eq!(trial.race.penalty_seconds(), RECOVERY_PENALTY);
    assert!(
        (trial.race.elapsed - before.elapsed - RECOVERY_PENALTY - 16.0 * TICK as f32).abs()
            < 0.0001
    );
    assert!((trial.race.driving_seconds() - before.driving - 16.0 * TICK as f32).abs() < 0.0001);
    assert_eq!(trial.race.gates_completed(), before.gates);
    assert_eq!(trial.race.next_gate, before.next_gate);
    assert_eq!(trial.race.laps, before.laps);
    let gate = course.gates[(before.next_gate + GATES - 1) % GATES];
    assert_eq!(trial.race.position, gate.position + gate.direction);
    assert_eq!(trial.race.previous_position, trial.race.position);
    assert_eq!(trial.race.previous_yaw, trial.race.yaw);
    assert_eq!(trial.race.velocity, Vec2::ZERO);
    assert_eq!(trial.race.telemetry().turn_rate, 0.0);
    assert_eq!(trial.race.bank, 0.0);
    assert!(!trial.race.off_track && !trial.race.boosting);
    assert_eq!(trial.race.recovery_in(), None);
    assert!(trial.race.recovery_timer > 2.0 && trial.race.recovery_timer < 2.4);
    assert!(trial.waiting_for_controls);
    assert_eq!(trial.alpha, 1.0);
    for _ in 0..60 {
        trial.advance(&course, DRIVE, false, 1.0 / 60.0);
        assert_eq!(trial.race.position, gate.position + gate.direction);
        assert_eq!(trial.race.recovery_count, 1);
        assert_eq!(trial.race.gates_completed(), before.gates);
        assert!(!trial.race.boosting);
    }
    trial.advance(&course, Input::default(), false, 0.0);
    assert!(!trial.waiting_for_controls);
    trial.advance(&course, DRIVE, false, TICK);
    assert!(trial.race.speed() > 0.0 && trial.race.boosting);
    assert_eq!(trial.race.penalty_seconds(), RECOVERY_PENALTY);
    eprintln!(
        "167 recovery: tick={}, retained_gates={}, penalty={RECOVERY_PENALTY}",
        tick + 16,
        before.gates
    );
}
