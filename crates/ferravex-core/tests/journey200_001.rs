use ferravex_core::session::{GameSession, SessionConfig, SessionState, SessionStepError};
use ferravex_core::{FixedClockError, FrameStep};

const STEP: f64 = 1.0 / 64.0;

fn assert_same_frame(actual: FrameStep, expected: FrameStep) {
    assert_eq!(actual.steps, expected.steps);
    assert_eq!(actual.dt, expected.dt);
    assert_eq!(actual.alpha, expected.alpha);
    assert_eq!(actual.dropped_seconds, expected.dropped_seconds);
}

fn assert_idle(frame: FrameStep, dt: f32) {
    assert_same_frame(
        frame,
        FrameStep {
            steps: 0,
            dt,
            alpha: 0.0,
            dropped_seconds: 0.0,
        },
    );
}

#[test]
fn checked_settings_report_original_clock_errors_and_validation_order() {
    for autoplay in [false, true] {
        for hz in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::MIN,
            -1.0,
            -0.0,
            0.0,
            0.999999,
            1000.000001,
            f64::MAX,
        ] {
            for max_steps in [0, 1, u32::MAX] {
                for error in [
                    SessionConfig::try_new(hz, max_steps, autoplay).unwrap_err(),
                    GameSession::try_new(hz, max_steps, autoplay).unwrap_err(),
                ] {
                    let FixedClockError::InvalidTickRate { hz: actual } = error else {
                        panic!("tick rate must be validated first: {error:?}");
                    };
                    assert_eq!(actual.to_bits(), hz.to_bits());
                }
            }
        }
        for hz in [1.0, 60.0, 1000.0] {
            assert_eq!(
                SessionConfig::try_new(hz, 0, autoplay).unwrap_err(),
                FixedClockError::ZeroCatchUpBudget
            );
            assert_eq!(
                GameSession::try_new(hz, 0, autoplay).unwrap_err(),
                FixedClockError::ZeroCatchUpBudget
            );
        }
    }
}

#[test]
fn checked_and_configured_sessions_preserve_legacy_timing_and_boundaries() {
    for hz in [1.0, 1.25, 30.0, 60.0, 64.0, 144.0, 999.5, 1000.0] {
        for max_steps in [1, 4, u32::MAX] {
            for autoplay in [false, true] {
                let config = SessionConfig::try_new(hz, max_steps, autoplay).unwrap();
                assert_eq!(config.hz(), hz);
                assert_eq!(config.max_steps(), max_steps);
                assert_eq!(config.autoplay(), autoplay);
                let mut legacy = GameSession::new(hz, max_steps, autoplay);
                let mut checked = GameSession::try_new(hz, max_steps, autoplay).unwrap();
                let mut configured = GameSession::from_config(config);
                assert_eq!(checked.state(), legacy.state());
                assert_eq!(configured.state(), legacy.state());
                assert_eq!(checked.tick(), 0);
                assert_eq!(configured.tick(), 0);
                let expected = legacy.advance(0.5 / hz);
                assert_same_frame(checked.advance(0.5 / hz), expected);
                assert_same_frame(configured.advance(0.5 / hz), expected);
                assert_eq!(checked.start(), legacy.start());
                configured.start();
                for elapsed in [
                    0.0,
                    0.25 / hz,
                    0.5 / hz,
                    3.75 / hz,
                    1.0 / 144.0,
                    f64::NAN,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    -1.0,
                    12.5 / hz,
                    f64::MAX,
                    0.5 / hz,
                ] {
                    let expected = legacy.advance(elapsed);
                    assert_same_frame(checked.advance(elapsed), expected);
                    assert_same_frame(configured.advance(elapsed), expected);
                    assert_eq!(checked.tick(), legacy.tick());
                    assert_eq!(configured.tick(), legacy.tick());
                    assert_eq!(checked.dt(), legacy.dt());
                    assert_eq!(configured.dt(), legacy.dt());
                }
            }
        }
    }
}

#[test]
fn reused_config_creates_independent_fresh_sessions() {
    for autoplay in [false, true] {
        let config = SessionConfig::try_new(64.0, 1, autoplay).unwrap();
        let mut first = GameSession::from_config(config);
        first.start();
        assert_eq!(first.advance(2.5 * STEP).steps, 1);
        first.set_paused(true);
        first.step_paused().unwrap();
        assert_eq!(first.tick(), 2);

        let mut second = GameSession::from_config(config);
        assert_eq!(second.tick(), 0);
        assert_eq!(second.is_playing(), autoplay);
        second.start();
        let frame = second.advance(0.5 * STEP);
        assert_eq!(frame.steps, 0);
        assert_eq!(frame.alpha, 0.5);
        assert_eq!(first.state(), SessionState::Paused);
        assert_eq!(first.tick(), 2);
    }
}

#[test]
fn stepping_rejects_every_unpaused_state_without_changing_lifecycle_or_time() {
    for state in [
        SessionState::Ready,
        SessionState::Playing,
        SessionState::Won,
        SessionState::Lost,
    ] {
        let mut session = GameSession::new(64.0, 4, false);
        if state != SessionState::Ready {
            session.start();
            assert_eq!(session.advance(2.5 * STEP).steps, 2);
            if state != SessionState::Playing {
                session.finish(state == SessionState::Won);
            }
        }
        let mut control = session.clone();
        for _ in 0..3 {
            let error = session.step_paused().unwrap_err();
            assert_eq!(error, SessionStepError::NotPaused { state });
            assert_eq!(session.state(), state);
            assert_eq!(session.tick(), control.tick());
            assert_eq!(session.dt(), control.dt());
            let message = error.to_string();
            assert!(message.contains("requires a Paused session"));
            assert!(message.contains(&format!("{state:?}")));
            let error: &dyn std::error::Error = &error;
            assert!(error.source().is_none());
        }
        assert_same_frame(session.advance(0.25 * STEP), control.advance(0.25 * STEP));
        assert_same_frame(session.advance(0.25 * STEP), control.advance(0.25 * STEP));
        assert_eq!(session.tick(), control.tick());
        assert_eq!(session.start(), control.start());
        assert_same_frame(session.advance(0.5 * STEP), control.advance(0.5 * STEP));
    }
}

#[test]
fn repeated_paused_steps_preserve_resume_fraction_across_rates_and_budgets() {
    // Non-binary rates and a remainder just below the rounding tolerance catch
    // accidental use of f32 dt as elapsed time or consumption of the remainder.
    for hz in [1.0, 1.25, 30.0, 60.0, 64.0, 144.0, 999.5, 1000.0] {
        for max_steps in [1, 8] {
            for fraction in [0.0, 0.25, 0.5, 1.0 - 2e-9] {
                let mut session = GameSession::try_new(hz, max_steps, true).unwrap();
                assert_eq!(session.advance(fraction / hz).steps, 0);
                let mut control = session.clone();
                assert!(session.set_paused(true));
                for tick in 1..=128 {
                    let frame = session.step_paused().unwrap();
                    assert_eq!(frame.steps, 1);
                    assert_eq!(frame.dt, control.dt());
                    assert_eq!(frame.ticks().collect::<Vec<_>>(), [control.dt()]);
                    assert_eq!(frame.alpha, 1.0);
                    assert_eq!(frame.dropped_seconds, 0.0);
                    assert_eq!(session.tick(), tick);
                    assert_eq!(session.state(), SessionState::Paused);
                    assert!(session.is_started());
                    assert!(!session.is_playing());
                    assert!(!session.start());
                    assert!(!session.set_paused(true));
                }
                for elapsed in [
                    0.0,
                    -1.0,
                    300.0,
                    f64::MAX,
                    f64::NAN,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                ] {
                    assert_idle(session.advance(elapsed), control.dt());
                    assert_eq!(session.tick(), 128);
                }
                assert!(session.set_paused(false));
                for elapsed in [0.0, 0.125 / hz, 0.375 / hz, 0.5 / hz, 10.5 / hz] {
                    assert_same_frame(session.advance(elapsed), control.advance(elapsed));
                    assert_eq!(session.tick(), control.tick() + 128);
                }
            }
        }
    }
}

#[test]
fn tick_counts_issued_batches_and_manual_steps_but_not_dropped_time() {
    let mut session = GameSession::new(64.0, 4, true);
    let frame = session.advance(10.5 * STEP);
    assert_eq!(frame.steps, 4);
    assert_eq!(frame.dropped_seconds, 6.0 * STEP);
    // No simulation has consumed the frame yet; the whole batch is counted.
    assert_eq!(session.tick(), 4);
    session.set_paused(true);
    let frame = session.step_paused().unwrap();
    assert_eq!(frame.steps, 1);
    assert_eq!(session.tick(), 5);
    assert_idle(session.advance(100.0), session.dt());
    assert_eq!(session.tick(), 5);
    session.set_paused(false);
    let frame = session.advance(0.5 * STEP);
    assert_eq!(frame.steps, 1);
    assert_eq!(frame.alpha, 0.0);
    assert_eq!(frame.dropped_seconds, 0.0);
    assert_eq!(session.tick(), 6);
}

#[test]
fn stepped_sessions_can_finish_but_cannot_overwrite_or_step_terminal_results() {
    for won in [false, true] {
        let mut session = GameSession::new(64.0, 1, true);
        session.advance(0.5 * STEP);
        session.set_paused(true);
        session.step_paused().unwrap();
        assert!(session.finish(won));
        let state = if won {
            SessionState::Won
        } else {
            SessionState::Lost
        };
        assert_eq!(session.state(), state);
        assert!(!session.finish(!won));
        assert!(!session.toggle_pause());
        assert!(!session.start());
        assert_eq!(
            session.step_paused().unwrap_err(),
            SessionStepError::NotPaused { state }
        );
        assert_idle(session.advance(10.0), session.dt());
        assert_eq!(session.tick(), 1);
    }
}

#[test]
fn reset_and_restart_clear_manual_ticks_and_fraction_without_changing_settings() {
    for reset in [false, true] {
        for finish in [None, Some(false), Some(true)] {
            let mut session = GameSession::new(64.0, 1, true);
            session.advance(2.5 * STEP);
            session.set_paused(true);
            for _ in 0..3 {
                session.step_paused().unwrap();
            }
            assert_eq!(session.tick(), 4);
            if let Some(won) = finish {
                assert!(session.finish(won));
            }
            if reset {
                session.reset();
                assert_eq!(session.state(), SessionState::Ready);
                assert!(session.step_paused().is_err());
                assert_idle(session.advance(100.0), STEP as f32);
                assert!(session.start());
            } else {
                session.restart();
                assert_eq!(session.state(), SessionState::Playing);
                assert!(session.step_paused().is_err());
            }
            assert_eq!(session.tick(), 0);
            let frame = session.advance(0.5 * STEP);
            assert_eq!(frame.steps, 0);
            assert_eq!(frame.alpha, 0.5);
            assert_eq!(frame.dt, STEP as f32);
            let frame = session.advance(2.5 * STEP);
            assert_eq!(frame.steps, 1);
            assert_eq!(frame.dropped_seconds, 2.0 * STEP);
            assert_eq!(session.tick(), 1);
        }
    }
}

#[test]
fn manual_and_playing_steps_produce_the_same_fixed_simulation_trace() {
    let mut inspected = GameSession::new(64.0, 4, true);
    let mut reference = inspected.clone();
    let mut inspected_position = 0.0_f32;
    let mut reference_position = 0.0_f32;
    let mut inspected_velocity = 1.0_f32;
    let mut reference_velocity = 1.0_f32;
    for tick in 1..=64 {
        let frame = if tick % 3 == 0 {
            inspected.set_paused(true);
            assert_idle(inspected.advance(30.0), inspected.dt());
            inspected.step_paused().unwrap()
        } else {
            inspected.set_paused(false);
            inspected.advance(STEP)
        };
        for dt in frame.ticks() {
            inspected_velocity += 3.0 * dt;
            inspected_position += inspected_velocity * dt;
        }
        for dt in reference.advance(STEP).ticks() {
            reference_velocity += 3.0 * dt;
            reference_position += reference_velocity * dt;
        }
        assert_eq!(inspected.tick(), tick);
        assert_eq!(inspected_position, reference_position);
        assert_eq!(inspected_velocity, reference_velocity);
    }
}
