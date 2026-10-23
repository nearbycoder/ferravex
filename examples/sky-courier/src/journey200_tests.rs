//! Task 170: fly the authored route and reach failures using actual controls.
//! Fixtures never assign a flight pose, resource, checkpoint, timer or outcome.

use super::*;

const TICK: f64 = 1. / 120.;
const MAX_TICKS: usize = 120 * 100;
const DIVE: Input = Input {
    turn: 0.,
    climb: -1.,
    boost: true,
    slow: false,
};
const LOITER: Input = Input {
    turn: 1.,
    climb: 0.,
    boost: false,
    slow: true,
};

/// Everything the flight owns, including presentation-facing simulation state.
/// The session's tick counter budgets a whole batch, even if a result ends it
/// early; `elapsed` instead counts the updates that actually executed.
#[derive(Debug, PartialEq)]
struct State {
    position: Vec3,
    motor: FlightState,
    next: usize,
    hull: u8,
    remaining: f32,
    outcome: Outcome,
    recovery: f32,
    boost_energy: f32,
    boost_exhausted: bool,
    gate_notice: f32,
    boosting: bool,
    slowing: bool,
    phase: SessionState,
    release_controls: bool,
    elapsed: f64,
}

fn state(run: &FlightRun) -> State {
    let f = &run.flight;
    State {
        position: f.pos,
        motor: f.motor,
        next: f.next,
        hull: f.hull,
        remaining: f.remaining,
        outcome: f.outcome,
        recovery: f.recovery,
        boost_energy: f.boost_energy,
        boost_exhausted: f.boost_exhausted,
        gate_notice: f.gate_notice,
        boosting: f.boosting,
        slowing: f.slowing,
        phase: run.session.state(),
        release_controls: run.release_controls,
        elapsed: run.elapsed,
    }
}

fn started() -> FlightRun {
    let mut run = FlightRun::new(false);
    assert!(run.session.start());
    run.advance(0., Input::default(), false);
    assert!(!run.release_controls);
    run
}

fn executed_ticks(run: &FlightRun) -> usize {
    let ticks = (run.elapsed / f64::from(TICK as f32)).round() as usize;
    assert_eq!(run.elapsed, ticks as f64 * f64::from(TICK as f32));
    ticks
}

fn reach_gate(run: &mut FlightRun, gate: usize) {
    for _ in 0..MAX_TICKS {
        if run.flight.next == gate {
            return;
        }
        assert!(run.session.is_playing());
        run.advance(TICK, Input::default(), true);
    }
    panic!("autopilot failed to reach gate {gate}");
}

struct Record {
    state: State,
    frame: FlightFrame,
}

fn trace(mut run: FlightRun, input: Input, autoplay: bool) -> Vec<Record> {
    let mut records = vec![Record {
        state: state(&run),
        frame: FlightFrame::default(),
    }];
    for _ in 0..MAX_TICKS {
        let frame = run.advance(TICK, input, autoplay);
        assert_eq!(frame.seconds, TICK as f32);
        records.push(Record {
            state: state(&run),
            frame,
        });
        if !run.session.is_playing() {
            return records;
        }
    }
    panic!("scenario must reach a real result within 100 simulated seconds");
}

/// Compare every render-frame sample and its accepted events against the same
/// production coordinator run one tick at a time, not a second physics model.
fn replay_trace(
    mut run: FlightRun,
    input: Input,
    autoplay: bool,
    records: &[Record],
    pattern: &[f64],
) -> (usize, usize) {
    assert_eq!(state(&run), records[0].state);
    let base_tick = executed_ticks(&run);
    let mut cursor = 0;
    let mut zero_frames = 0;
    let mut catch_up_frames = 0;
    for frame_index in 0..MAX_TICKS * 4 {
        let frame = run.advance(pattern[frame_index % pattern.len()], input, autoplay);
        let end = executed_ticks(&run) - base_tick;
        let count = end - cursor;
        assert!(count <= 12);
        zero_frames += usize::from(count == 0);
        catch_up_frames += usize::from(count > 1);
        let expected = &records[cursor + 1..end + 1];
        assert_eq!(frame.gates, expected.iter().map(|r| r.frame.gates).sum());
        assert_eq!(frame.damaged, expected.iter().any(|r| r.frame.damaged));
        assert_eq!(frame.outcome, expected.iter().find_map(|r| r.frame.outcome));
        assert!((frame.seconds - count as f32 * TICK as f32).abs() < 1e-7);
        assert_eq!(state(&run), records[end].state, "frame {frame_index}");
        if run.session.is_playing() {
            assert_eq!(run.session.tick() as usize, base_tick + end);
        } else {
            assert_eq!(end, records.len() - 1);
            // Unexecuted ticks in the terminal batch must not change flight
            // time, position, resources, or replay the terminal feedback.
            let terminal = state(&run);
            let budgeted_ticks = run.session.tick();
            for elapsed in [0., TICK, 30.] {
                let inactive = run.advance(elapsed, DIVE, autoplay);
                assert_eq!(inactive.seconds, 0.);
                assert_eq!(inactive.gates, 0);
                assert!(!inactive.damaged);
                assert_eq!(inactive.outcome, None);
                assert_eq!(state(&run), terminal);
                assert_eq!(run.session.tick(), budgeted_ticks);
            }
            return (zero_frames, catch_up_frames);
        }
        cursor = end;
    }
    panic!("render pattern did not finish the recorded scenario");
}

#[test]
fn ordered_delivery_matches_every_sample_at_30_60_and_144_hz() {
    let records = trace(started(), Input::default(), true);
    let visits: Vec<_> = records
        .windows(2)
        .filter(|pair| pair[1].frame.gates > 0)
        .collect();
    assert_eq!(visits.len(), ROUTE.len());
    for (index, pair) in visits.iter().enumerate() {
        let before = &pair[0].state;
        let after = &pair[1].state;
        assert_eq!((before.next, after.next), (index, index + 1));
        assert_eq!(pair[1].frame.gates, 1);
        let radius = if index == GATES { 8. } else { 8.5 };
        assert!(after.position.distance(ROUTE[index]) <= radius);
        assert!(after.position.distance(before.position) <= 35. * TICK as f32);
        assert_eq!(after.hull, 3);
        assert_eq!(after.gate_notice, 2.);
    }
    let terminal = &records.last().unwrap().state;
    assert_eq!(terminal.phase, SessionState::Won);
    assert_eq!(terminal.outcome, Outcome::Delivered);
    assert!(terminal.motor.speed <= 18.);
    assert!(terminal.remaining > 0.);
    assert!(!records.iter().any(|r| r.frame.damaged));
    for hz in [30, 60, 144] {
        let (zero, catch_up) = replay_trace(
            started(),
            Input::default(),
            true,
            &records,
            &[1. / f64::from(hz)],
        );
        if hz == 144 {
            assert!(zero > 0);
        } else {
            assert!(catch_up > 0);
        }
    }
    let (zero, catch_up) = replay_trace(
        started(),
        Input::default(),
        true,
        &records,
        &[0., TICK / 2., 0.08, 1. / 144.],
    );
    assert!(zero > 0 && catch_up > 0);
}

#[test]
fn actual_checkpoint_crashes_recover_then_lose_at_each_render_rate() {
    fn at_checkpoint() -> FlightRun {
        let mut run = started();
        reach_gate(&mut run, 1);
        assert_eq!(run.flight.hull, 3);
        run
    }
    let records = trace(at_checkpoint(), DIVE, false);
    let impacts: Vec<_> = records
        .windows(2)
        .filter(|pair| pair[1].frame.damaged)
        .collect();
    assert_eq!(impacts.len(), 3);
    for (index, pair) in impacts.iter().enumerate() {
        let before = &pair[0].state;
        let after = &pair[1].state;
        assert_eq!(after.hull, 2 - index as u8);
        assert_eq!(after.next, 1);
        assert_eq!(after.remaining, (before.remaining - TICK as f32) - 5.);
        if after.hull > 0 {
            assert_eq!(after.position, ROUTE[0]);
            assert_eq!(after.motor.pitch, 0.);
            assert_eq!(after.motor.bank, 0.);
            assert_eq!(after.motor.turn_rate, 0.);
            assert_eq!(after.motor.speed, 24.);
            assert_eq!(after.recovery, 2.5);
            assert_eq!(after.outcome, Outcome::Flying);
        }
    }
    for pair in records.windows(2) {
        if pair[0].state.recovery > TICK as f32 {
            assert!(!pair[1].frame.damaged, "recovery must protect the hull");
        }
    }
    let terminal = &records.last().unwrap().state;
    assert_eq!(terminal.phase, SessionState::Lost);
    assert_eq!(terminal.outcome, Outcome::Lost);
    assert_eq!(terminal.hull, 0);
    assert!(terminal.remaining > 0.);
    for hz in [30, 60, 144] {
        replay_trace(
            at_checkpoint(),
            DIVE,
            false,
            &records,
            &[1. / f64::from(hz)],
        );
    }
}

#[test]
fn continuous_loiter_reaches_real_timeout_at_each_render_rate() {
    let records = trace(started(), LOITER, false);
    assert!(records.iter().all(|r| r.state.hull == 3));
    assert!(records.iter().all(|r| r.state.next == 0));
    let terminal = &records.last().unwrap().state;
    assert_eq!(terminal.phase, SessionState::Lost);
    assert_eq!(terminal.outcome, Outcome::Lost);
    assert_eq!(terminal.remaining, 0.);
    assert!((terminal.elapsed - 90.).abs() < 0.05);
    assert!(records[records.len() - 2].state.remaining > 0.);
    for hz in [30, 60, 144] {
        replay_trace(started(), LOITER, false, &records, &[1. / f64::from(hz)]);
    }
}

#[test]
fn fast_real_delivery_pass_is_rejected_before_a_slow_return_succeeds() {
    let mut run = started();
    reach_gate(&mut run, GATES);
    let mut entered = false;
    let mut exited = false;
    for _ in 0..120 * 15 {
        let mut input = run.flight.autopilot();
        input.slow = false;
        let frame = run.advance(TICK, input, false);
        assert_eq!(run.flight.next, GATES);
        assert_eq!(run.flight.outcome, Outcome::Flying);
        assert_eq!(run.flight.hull, 3);
        assert_eq!(frame.gates, 0);
        assert_eq!(frame.outcome, None);
        assert!(run.flight.motor.speed > 18.);
        if run.flight.pos.distance(ROUTE[GATES]) < 8. {
            entered = true;
        } else if entered {
            exited = true;
            break;
        }
    }
    assert!(
        entered && exited,
        "must actually fly through the delivery area"
    );
    assert!(run.flight.navigation().unwrap().bearing.abs() > 1.);
    let records = trace(run, Input::default(), true);
    assert_eq!(records.iter().map(|r| r.frame.gates).sum::<usize>(), 1);
    let terminal = &records.last().unwrap().state;
    assert_eq!(terminal.phase, SessionState::Won);
    assert_eq!(terminal.outcome, Outcome::Delivered);
    assert_eq!(terminal.hull, 3);
    assert!(terminal.motor.speed <= 18.);
}

#[test]
fn held_controls_across_zero_and_catch_up_frames_follow_executed_ticks() {
    let mut run = started();
    let mut reference = started();
    let initial = state(&run);
    let held = Input {
        turn: 0.5,
        climb: 0.25,
        boost: true,
        slow: false,
    };
    // These are held controls in the real host, not queued gameplay edges.
    // A press and release entirely between ticks should not boost or steer.
    assert_eq!(run.advance(TICK / 4., held, false).seconds, 0.);
    assert_eq!(state(&run), initial);
    assert_eq!(run.advance(TICK / 4., Input::default(), false).seconds, 0.);
    run.advance(TICK / 2., Input::default(), false);
    reference.advance(TICK, Input::default(), false);
    assert_eq!(state(&run), state(&reference));
    for input in [held, Input { slow: true, ..held }] {
        let before = executed_ticks(&run);
        run.advance(12. * TICK, input, false);
        for _ in 0..12 {
            reference.advance(TICK, input, false);
        }
        assert_eq!(executed_ticks(&run) - before, 12);
        assert_eq!(state(&run), state(&reference));
    }
    assert!(run.flight.slowing && !run.flight.boosting);
    assert_eq!(run.advance(TICK / 2., held, false).seconds, 0.);
    run.restart();
    reference.restart();
    assert_eq!(run.advance(TICK / 2., held, false).seconds, 0.);
    run.advance(11.5 * TICK, held, false);
    for _ in 0..12 {
        reference.advance(TICK, held, false);
    }
    assert_eq!(state(&run), state(&reference));
    assert_eq!(run.flight.motor.yaw, 0.);
    assert_eq!(run.flight.motor.pitch, 0.);
    assert_eq!(run.flight.motor.speed, 24.);
    assert_eq!(run.flight.boost_energy, 1.);
    // A neutral zero-tick sample releases retry protection; a subsequent held
    // input is accepted only when the next real tick executes.
    run.advance(0., Input::default(), false);
    reference.advance(0., Input::default(), false);
    run.advance(4. * TICK, held, false);
    for _ in 0..4 {
        reference.advance(TICK, held, false);
    }
    assert_eq!(state(&run), state(&reference));
    assert!(run.flight.motor.speed > 24.);
    assert!(run.flight.boost_energy < 1.);
}

fn present(
    run: &mut FlightRun,
    presentation: &mut FlightPresentation,
    elapsed: f64,
    input: Input,
    autoplay: bool,
    cues: &mut Vec<Cue>,
) -> (FlightFrame, Pose3) {
    let frame = run.advance(elapsed, input, autoplay);
    let (view, _) = presentation.update(run, &frame, elapsed as f32, |cue, _| cues.push(cue));
    (frame, view)
}

#[test]
fn real_lifecycle_and_retries_reset_flight_and_rearm_accepted_feedback() {
    let mut run = FlightRun::new(false);
    let mut presentation = FlightPresentation::new();
    let mut cues = Vec::new();
    let ready = state(&run);
    let (_, initial_view) = present(&mut run, &mut presentation, 30., DIVE, false, &mut cues);
    assert_eq!(run.session.state(), SessionState::Ready);
    assert_eq!(state(&run), ready);
    assert_eq!(run.session.tick(), 0);
    assert!(cues.is_empty());
    assert!(run.session.start());
    assert!(!run.session.start());
    run.advance(0., Input::default(), false);
    for _ in 0..MAX_TICKS {
        present(
            &mut run,
            &mut presentation,
            TICK,
            Input::default(),
            true,
            &mut cues,
        );
        if run.flight.next == 1 {
            break;
        }
    }
    assert_eq!(run.flight.next, 1);
    assert_eq!(cues, [Cue::Pickup]);
    for _ in 0..120 * 10 {
        let (frame, _) = present(&mut run, &mut presentation, TICK, DIVE, false, &mut cues);
        if frame.damaged {
            break;
        }
    }
    assert_eq!(run.flight.hull, 2);
    assert_eq!(run.flight.recovery, 2.5);
    assert!(run.flight.boost_energy < 1.);
    assert!(!presentation.exhaust.is_empty());
    assert_eq!(run.advance(TICK / 2., Input::default(), false).seconds, 0.);
    assert!(run.session.set_paused(true));
    let paused = state(&run);
    let tick = run.session.tick();
    let particle_count = presentation.exhaust.len();
    let cue_count = cues.len();
    let (_, paused_view) = present(&mut run, &mut presentation, 0., DIVE, false, &mut cues);
    for elapsed in [TICK, 5., 30.] {
        let (frame, view) = present(&mut run, &mut presentation, elapsed, DIVE, false, &mut cues);
        assert_eq!(frame.seconds, 0.);
        assert_eq!(state(&run), paused);
        assert_eq!(run.session.tick(), tick);
        assert_eq!(view, paused_view);
        assert_eq!(presentation.exhaust.len(), particle_count);
        assert_eq!(cues.len(), cue_count);
    }
    assert!(run.session.set_paused(false));
    assert_eq!(run.advance(0., DIVE, false).seconds, 0.);
    let (resumed, _) = present(
        &mut run,
        &mut presentation,
        TICK / 2.,
        DIVE,
        false,
        &mut cues,
    );
    assert_eq!(resumed.seconds, TICK as f32);
    assert_eq!(run.session.tick(), tick + 1);
    assert_eq!(run.flight.recovery, 2.5 - TICK as f32);
    assert_eq!(run.flight.remaining, paused.remaining - TICK as f32);
    for _ in 0..120 * 20 {
        present(&mut run, &mut presentation, TICK, DIVE, false, &mut cues);
        if !run.session.is_playing() {
            break;
        }
    }
    assert_eq!(run.session.state(), SessionState::Lost);
    assert_eq!(run.flight.hull, 0);
    assert_eq!(
        cues.iter().filter(|c| matches!(c, Cue::Impact(_))).count(),
        3
    );
    assert_eq!(cues.iter().filter(|c| **c == Cue::Failure).count(), 1);
    assert_eq!(cues.len(), 5);

    // Retry first from an actual hull loss, then from an actual delivery. Reuse
    // the same presentation owner to catch feedback IDs that survive reset.
    for _ in 0..2 {
        run.restart();
        presentation.reset(run.flight.pos);
        assert!(run.release_controls);
        assert_eq!(run.session.tick(), 0);
        assert_eq!(run.elapsed, 0.);
        assert_eq!(presentation.propeller_time, 0.);
        assert!(presentation.wing_trails.iter().all(Trail::is_empty));
        assert!(presentation.exhaust.is_empty());
        cues.clear();
        let (zero, reset_view) = present(
            &mut run,
            &mut presentation,
            0.,
            Input::default(),
            false,
            &mut cues,
        );
        assert_eq!(zero.seconds, 0.);
        assert_eq!(reset_view, initial_view);
        assert_eq!(state(&run), state(&started()));
        assert!(cues.is_empty());
        for _ in 0..30 * 90 {
            present(
                &mut run,
                &mut presentation,
                1. / 30.,
                Input::default(),
                true,
                &mut cues,
            );
            if !run.session.is_playing() {
                break;
            }
        }
        assert_eq!(run.session.state(), SessionState::Won);
        assert_eq!(run.flight.hull, 3);
        assert_eq!(&cues[..GATES + 1], [Cue::Pickup; GATES + 1]);
        assert_eq!(cues.last(), Some(&Cue::Success));
        assert_eq!(cues.len(), GATES + 2);
        let terminal = state(&run);
        for elapsed in [0., TICK, 30.] {
            present(&mut run, &mut presentation, elapsed, DIVE, false, &mut cues);
            assert_eq!(state(&run), terminal);
            assert_eq!(cues.len(), GATES + 2);
        }
    }
}
