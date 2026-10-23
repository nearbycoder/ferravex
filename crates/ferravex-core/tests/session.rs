use ferravex_core::session::{GameSession, SessionState};
use ferravex_core::{FixedClock, FrameStep};

const STATES: [SessionState; 5] = [
    SessionState::Ready,
    SessionState::Playing,
    SessionState::Paused,
    SessionState::Won,
    SessionState::Lost,
];
const STEP: f64 = 1.0 / 64.0;

// Active/finished fixtures retain both whole ticks and a fractional remainder,
// making accidental resets during lifecycle changes observable.
fn in_state(state: SessionState) -> GameSession {
    let mut session = GameSession::new(64.0, 4, false);
    if state != SessionState::Ready {
        assert!(session.start());
        assert_eq!(session.advance(2.5 * STEP).steps, 2);
        match state {
            SessionState::Paused => assert!(session.set_paused(true)),
            SessionState::Won => assert!(session.finish(true)),
            SessionState::Lost => assert!(session.finish(false)),
            _ => {}
        }
    }
    assert_eq!(session.state(), state);
    session
}

fn assert_idle(frame: FrameStep, dt: f32) {
    assert_eq!(frame.steps, 0);
    assert_eq!(frame.dt, dt);
    assert_eq!(frame.alpha, 0.0);
    assert_eq!(frame.dropped_seconds, 0.0);
}

fn assert_same_frame(actual: FrameStep, expected: FrameStep) {
    assert_eq!(actual.steps, expected.steps);
    assert_eq!(actual.dt, expected.dt);
    assert_eq!(actual.alpha, expected.alpha);
    assert_eq!(actual.dropped_seconds, expected.dropped_seconds);
}

#[test]
fn construction_and_state_queries_cover_every_state() {
    let ready = GameSession::new(60.0, 8, false);
    let autoplay = GameSession::new(60.0, 8, true);
    assert_eq!(ready.state(), SessionState::Ready);
    assert_eq!(autoplay.state(), SessionState::Playing);
    assert_eq!(ready.tick(), 0);
    assert_eq!(autoplay.tick(), 0);
    assert_eq!(ready.dt(), 1.0_f32 / 60.0);
    assert_eq!(autoplay.dt(), ready.dt());

    for state in STATES {
        let session = in_state(state);
        assert_eq!(session.is_started(), state != SessionState::Ready);
        assert_eq!(session.is_playing(), state == SessionState::Playing);
        assert_eq!(session.is_paused(), state == SessionState::Paused);
        assert_eq!(session.dt(), STEP as f32);
    }
}

#[test]
fn start_only_changes_ready_and_never_resumes_or_retries() {
    for state in STATES {
        let mut session = in_state(state);
        let tick = session.tick();
        assert_eq!(session.start(), state == SessionState::Ready);
        assert_eq!(
            session.state(),
            if state == SessionState::Ready {
                SessionState::Playing
            } else {
                state
            }
        );
        assert_eq!(session.tick(), tick);
        assert!(!session.start());
        if state == SessionState::Playing {
            let frame = session.advance(0.5 * STEP);
            assert_eq!(frame.steps, 1);
            assert_eq!(frame.alpha, 0.0);
        }
    }
}

#[test]
fn set_paused_has_exact_transition_and_idempotence_semantics() {
    for state in STATES {
        for paused in [false, true] {
            let mut session = in_state(state);
            let tick = session.tick();
            let expected = match (state, paused) {
                (SessionState::Playing, true) => SessionState::Paused,
                (SessionState::Paused, false) => SessionState::Playing,
                _ => state,
            };
            assert_eq!(session.set_paused(paused), expected != state);
            assert_eq!(session.state(), expected);
            assert_eq!(session.tick(), tick);
            assert!(!session.set_paused(paused));
            assert_eq!(session.state(), expected);
            if matches!(expected, SessionState::Playing | SessionState::Paused) {
                session.set_paused(false);
                assert_eq!(session.advance(0.5 * STEP).steps, 1);
                assert_eq!(session.tick(), tick + 1);
            }
        }
    }
}

#[test]
fn toggle_pause_only_changes_playing_and_paused() {
    for state in STATES {
        let mut session = in_state(state);
        let tick = session.tick();
        let expected = match state {
            SessionState::Playing => SessionState::Paused,
            SessionState::Paused => SessionState::Playing,
            _ => state,
        };
        assert_eq!(session.toggle_pause(), expected != state);
        assert_eq!(session.state(), expected);
        assert_eq!(session.tick(), tick);
        assert_eq!(session.toggle_pause(), expected != state);
        assert_eq!(session.state(), state);
        if matches!(state, SessionState::Playing | SessionState::Paused) {
            session.set_paused(false);
            assert_eq!(session.advance(0.5 * STEP).steps, 1);
        }
    }
}

#[test]
fn finish_accepts_playing_and_paused_and_preserves_first_result() {
    for state in STATES {
        for won in [false, true] {
            let mut session = in_state(state);
            let tick = session.tick();
            let can_finish = matches!(state, SessionState::Playing | SessionState::Paused);
            let expected = if can_finish {
                if won {
                    SessionState::Won
                } else {
                    SessionState::Lost
                }
            } else {
                state
            };
            assert_eq!(session.finish(won), can_finish);
            assert_eq!(session.state(), expected);
            assert_eq!(session.tick(), tick);
            assert!(!session.finish(won));
            assert!(!session.finish(!won));
            assert_eq!(session.state(), expected);
            assert_idle(session.advance(600.0), STEP as f32);
            assert_eq!(session.tick(), tick);
        }
    }
}

#[test]
fn restart_clears_ticks_and_fraction_from_every_state() {
    for state in STATES {
        let mut session = in_state(state);
        session.restart();
        assert_eq!(session.state(), SessionState::Playing);
        assert_eq!(session.tick(), 0);
        let frame = session.advance(0.5 * STEP);
        assert_eq!(frame.steps, 0);
        assert_eq!(frame.alpha, 0.5);
        assert_eq!(frame.dt, STEP as f32);
        assert_eq!(session.advance(0.5 * STEP).steps, 1);
        session.restart();
        assert_eq!(session.tick(), 0);
        assert_idle(session.advance(0.0), STEP as f32);
    }
}

#[test]
fn reset_clears_ticks_and_fraction_and_requires_start_from_every_state() {
    for state in STATES {
        let mut session = in_state(state);
        session.reset();
        assert_eq!(session.state(), SessionState::Ready);
        assert!(!session.is_started());
        assert_eq!(session.tick(), 0);
        assert_idle(session.advance(600.0), STEP as f32);
        session.reset();
        assert!(session.start());
        let frame = session.advance(0.5 * STEP);
        assert_eq!(frame.steps, 0);
        assert_eq!(frame.alpha, 0.5);
        assert_eq!(session.advance(0.5 * STEP).steps, 1);
        assert_eq!(session.tick(), 1);
    }
}

#[test]
fn inactive_frames_are_zero_and_never_advance_the_clock() {
    for state in STATES {
        if state == SessionState::Playing {
            continue;
        }
        let mut session = in_state(state);
        let tick = session.tick();
        for elapsed in [
            0.0,
            0.5 * STEP,
            30.0,
            3600.0,
            f64::MAX,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -1.0,
        ] {
            assert_idle(session.advance(elapsed), STEP as f32);
            assert_eq!(session.tick(), tick);
            assert_eq!(session.state(), state);
        }
    }
}

#[test]
fn waiting_for_start_does_not_create_a_catchup_backlog() {
    let mut session = GameSession::new(64.0, 4, false);
    for _ in 0..100 {
        assert_idle(session.advance(3.0), STEP as f32);
    }
    assert!(session.start());
    let frame = session.advance(0.25 * STEP);
    assert_eq!(frame.steps, 0);
    assert_eq!(frame.alpha, 0.25);
    assert_eq!(frame.dropped_seconds, 0.0);
    assert_eq!(session.advance(0.75 * STEP).steps, 1);
    assert_eq!(session.tick(), 1);
}

#[test]
fn paused_time_is_discarded_but_playing_fraction_survives_resume() {
    let mut session = in_state(SessionState::Paused);
    for _ in 0..100 {
        assert_idle(session.advance(17.75), STEP as f32);
    }
    assert_eq!(session.tick(), 2);
    assert!(session.set_paused(false));
    let frame = session.advance(0.25 * STEP);
    assert_eq!(frame.steps, 0);
    assert_eq!(frame.alpha, 0.75);
    assert_eq!(frame.dropped_seconds, 0.0);
    let frame = session.advance(0.25 * STEP);
    assert_eq!(frame.steps, 1);
    assert_eq!(frame.alpha, 0.0);
    assert_eq!(frame.dropped_seconds, 0.0);
    assert_eq!(session.tick(), 3);
}

#[test]
fn rapid_pause_cycles_do_not_lose_active_time_or_gain_inactive_time() {
    let mut session = GameSession::new(64.0, 4, true);
    for index in 0..1000 {
        let frame = session.advance(0.25 * STEP);
        assert_eq!(frame.steps, u32::from(index % 4 == 3));
        assert!(session.toggle_pause());
        assert_idle(session.advance(0.75 * STEP), STEP as f32);
        assert!(session.toggle_pause());
    }
    assert_eq!(session.tick(), 250);
    assert_idle(session.advance(0.0), STEP as f32);
}

#[test]
fn catchup_drops_only_over_budget_whole_steps_without_deferring_them() {
    let mut session = GameSession::new(64.0, 4, true);
    let frame = session.advance(10.5 * STEP);
    assert_eq!(frame.steps, 4);
    assert_eq!(frame.alpha, 0.5);
    assert_eq!(frame.dropped_seconds, 6.0 * STEP);
    assert_eq!(session.tick(), 4);
    let frame = session.advance(0.0);
    assert_eq!(frame.steps, 0);
    assert_eq!(frame.alpha, 0.5);
    assert_eq!(frame.dropped_seconds, 0.0);
    assert!(session.set_paused(true));
    assert_idle(session.advance(100.0), STEP as f32);
    assert!(session.set_paused(false));
    let frame = session.advance(0.5 * STEP);
    assert_eq!(frame.steps, 1);
    assert_eq!(frame.alpha, 0.0);
    assert_eq!(frame.dropped_seconds, 0.0);
    assert_eq!(session.tick(), 5);
}

#[test]
fn invalid_playing_durations_preserve_fraction_and_allow_recovery() {
    let mut session = GameSession::new(64.0, 4, true);
    assert_eq!(session.advance(0.5 * STEP).alpha, 0.5);
    for elapsed in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -10.0] {
        let frame = session.advance(elapsed);
        assert_eq!(frame.steps, 0);
        assert_eq!(frame.alpha, 0.5);
        assert_eq!(frame.dropped_seconds, 0.0);
        assert_eq!(session.tick(), 0);
    }
    assert_eq!(session.advance(0.5 * STEP).steps, 1);
    assert_eq!(session.tick(), 1);
}

#[test]
fn playing_frames_preserve_fixed_clock_semantics_across_configurations() {
    for hz in [1.0, 30.0, 60.0, 64.0, 144.0, 1000.0] {
        for max_steps in [1, 4, 8] {
            let mut session = GameSession::new(hz, max_steps, true);
            let mut clock = FixedClock::new(hz, max_steps);
            for elapsed in [
                0.0,
                0.25 / hz,
                0.5 / hz,
                3.75 / hz,
                1.0 / 144.0,
                1.0 / 30.0,
                0.0,
                -1.0,
                f64::NAN,
                f64::INFINITY,
                12.5 / hz,
                f64::MAX,
                0.5 / hz,
            ] {
                assert_same_frame(session.advance(elapsed), clock.advance(elapsed));
                assert_eq!(session.tick(), clock.tick());
                assert_eq!(session.dt(), clock.dt());
            }
        }
    }
}

#[test]
fn fixed_simulation_matches_at_30_60_and_144_fps() {
    for hz in [30.0, 60.0, 64.0, 144.0] {
        let mut reference = None;
        for fps in [30, 60, 144] {
            let mut session = GameSession::new(hz, 8, true);
            let mut position = 0.0_f64;
            let mut velocity = 0.0_f64;
            for _ in 0..fps * 10 {
                let frame = session.advance(1.0 / fps as f64);
                assert_eq!(frame.dropped_seconds, 0.0);
                for _ in 0..frame.steps {
                    velocity += 3.0 * frame.dt as f64;
                    position += velocity * frame.dt as f64;
                }
            }
            assert_eq!(session.tick(), (hz * 10.0) as u64);
            let result = (session.tick(), position.to_bits(), velocity.to_bits());
            if let Some(expected) = reference {
                assert_eq!(result, expected);
            } else {
                reference = Some(result);
            }
        }
    }
}

#[test]
fn irregular_frames_and_pauses_match_one_tick_frames_without_losing_time() {
    let mut irregular = GameSession::new(64.0, 8, true);
    let mut regular = GameSession::new(64.0, 8, true);
    let mut irregular_trace = Vec::new();
    let mut regular_trace = Vec::new();
    let mut elapsed_quarters = 0_u64;
    let mut position = 0_i64;
    let mut velocity = 0_i64;

    for cycle in 0..100 {
        for quarters in [1, 0, 7, 2, 16, 3, 11] {
            let frame = irregular.advance(quarters as f64 * STEP / 4.0);
            elapsed_quarters += quarters;
            assert_eq!(irregular.tick(), elapsed_quarters / 4);
            assert_eq!(frame.alpha, (elapsed_quarters % 4) as f32 / 4.0);
            assert_eq!(frame.dropped_seconds, 0.0);
            for _ in 0..frame.steps {
                velocity += 3;
                position += velocity;
                irregular_trace.push((position, velocity));
            }
            assert!(irregular.set_paused(true));
            assert_idle(irregular.advance(cycle as f64 + 0.125), STEP as f32);
            assert!(irregular.set_paused(false));
        }
    }
    position = 0;
    velocity = 0;
    for _ in 0..1000 {
        let frame = regular.advance(STEP);
        assert_eq!(frame.steps, 1);
        velocity += 3;
        position += velocity;
        regular_trace.push((position, velocity));
    }
    assert_eq!(irregular.tick(), 1000);
    assert_eq!(irregular.tick(), regular.tick());
    assert_eq!(irregular_trace, regular_trace);
}

#[test]
fn restarting_replays_the_same_irregular_frame_sequence() {
    let mut session = GameSession::new(64.0, 4, true);
    let elapsed_frames = [0.25, 0.5, 2.75, 10.5, 0.0, 0.25, 1.75];
    let first: Vec<_> = elapsed_frames
        .iter()
        .map(|elapsed| session.advance(elapsed * STEP))
        .collect();
    let first_tick = session.tick();
    assert!(session.finish(false));
    assert_idle(session.advance(999.0), STEP as f32);
    session.restart();
    for (elapsed, expected) in elapsed_frames.iter().zip(first) {
        assert_same_frame(session.advance(elapsed * STEP), expected);
    }
    assert_eq!(session.tick(), first_tick);
}

#[test]
fn complete_lifecycle_can_finish_while_paused_then_reset_and_play_again() {
    let mut session = GameSession::new(64.0, 4, false);
    assert!(!session.finish(true));
    assert!(!session.toggle_pause());
    assert!(session.start());
    assert_eq!(session.advance(2.25 * STEP).steps, 2);
    assert!(session.set_paused(true));
    assert!(session.finish(false));
    assert!(!session.set_paused(false));
    assert!(!session.start());
    session.restart();
    assert_eq!(session.advance(STEP).steps, 1);
    assert!(session.finish(true));
    assert_eq!(session.state(), SessionState::Won);
    session.reset();
    assert!(!session.is_started());
    assert_eq!(session.tick(), 0);
    assert!(session.start());
    let frame = session.advance(0.75 * STEP);
    assert_eq!(frame.steps, 0);
    assert_eq!(frame.alpha, 0.75);
}

#[test]
fn invalid_configuration_retains_fixed_clock_constructor_panics() {
    for hz in [0.0, -1.0, 1000.01, f64::NAN, f64::INFINITY] {
        assert!(std::panic::catch_unwind(|| GameSession::new(hz, 8, false)).is_err());
    }
    assert!(std::panic::catch_unwind(|| GameSession::new(60.0, 0, true)).is_err());
}
