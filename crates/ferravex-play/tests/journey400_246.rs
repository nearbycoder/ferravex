use ferravex_play::gameplay::LoopError;
use ferravex_play::gameplay::charge::{ChargeAction, ChargeRelease, ChargeStep};
use ferravex_play::input::{ActionMap, Binding, InputCode};

const FLAGS: [(bool, bool, bool); 8] = [
    (false, false, false),
    (false, false, true),
    (false, true, false),
    (false, true, true),
    (true, false, false),
    (true, false, true),
    (true, true, false),
    (true, true, true),
];

fn state(charge: &ChargeAction) -> (bool, f32, f32) {
    (charge.charging(), charge.power(), charge.cooldown_seconds())
}

fn assert_report(
    report: ChargeStep,
    charge: &ChargeAction,
    release: ChargeRelease,
    charging: bool,
    power: f32,
    cooldown_seconds: f32,
) {
    assert_eq!(
        report,
        ChargeStep {
            release,
            charging,
            power,
            cooldown_seconds,
        }
    );
    assert_eq!(state(charge), (charging, power, cooldown_seconds));
}

#[test]
fn every_flag_combination_from_idle_has_defined_zero_and_positive_time_behavior() {
    use ChargeRelease::{CanceledTooShort, None};
    let expected = [
        None,
        None,
        CanceledTooShort,
        CanceledTooShort,
        None,
        None,
        None,
        None,
    ];
    for dt in [0., 0.125] {
        for ((held, pressed, released), release) in FLAGS.into_iter().zip(expected) {
            let mut charge = ChargeAction::new(0.25, 1., 0.5).unwrap();
            let report = charge.step_edges(dt, held, pressed, released).unwrap();
            assert_report(
                report,
                &charge,
                release,
                held,
                if held { dt } else { 0. },
                0.,
            );
        }
    }
}

#[test]
fn every_flag_combination_resolves_an_old_charge_before_new_held_duration() {
    use ChargeRelease::{CanceledTooShort, Emitted, None};
    // Includes a short old charge: it cannot borrow this snapshot's dt to meet minimum.
    for (old_duration, resolved) in [(0.125, CanceledTooShort), (0.5, Emitted(0.5))] {
        for recovery in [0., 0.25] {
            for dt in [0., 0.125] {
                for (held, pressed, released) in FLAGS {
                    let mut charge = ChargeAction::new(0.25, 1., recovery).unwrap();
                    charge.step(old_duration, true).unwrap();
                    let report = charge.step_edges(dt, held, pressed, released).unwrap();
                    if !held || released {
                        let cooldown = if old_duration == 0.5 { recovery } else { 0. };
                        let charging = held && cooldown == 0.;
                        assert_report(
                            report,
                            &charge,
                            resolved,
                            charging,
                            if charging { dt } else { 0. },
                            cooldown,
                        );
                    } else {
                        assert_report(report, &charge, None, true, old_duration + dt, 0.);
                    }
                }
            }
        }
    }
}

#[test]
fn every_flag_combination_during_recovery_is_gated_until_expiry() {
    use ChargeRelease::{Emitted, None};
    for dt in [0., 0.125, 0.5, 0.75] {
        for (held, pressed, released) in FLAGS {
            let mut charge = ChargeAction::new(0., 1., 0.5).unwrap();
            charge.step(1., true).unwrap();
            charge.step(0., false).unwrap();
            let report = charge.step_edges(dt, held, pressed, released).unwrap();
            let ready = dt >= 0.5;
            let tap = ready && !held && pressed;
            assert_report(
                report,
                &charge,
                if tap { Emitted(0.) } else { None },
                ready && held,
                if ready && held { dt - 0.5 } else { 0. },
                if tap { 0.5 } else { (0.5 - dt).max(0.) },
            );
        }
    }
}

#[test]
fn real_action_map_release_repress_preserves_old_release_and_new_charge() {
    let mut input = ActionMap::default();
    let code = InputCode::Key(32);
    let action = input.try_action(vec![Binding::digital(code)]).unwrap();
    let mut charge = ChargeAction::new(0.25, 1., 0.).unwrap();
    input.feed(code, 1.);
    let first = input.consume_tick(action);
    charge
        .step_edges(0.5, first.value != 0., first.pressed, first.released)
        .unwrap();
    // More than one transition of each kind still yields only independent booleans.
    for value in [0., 1., 0., 1.] {
        input.feed(code, value);
    }
    let snapshot = input.consume_tick(action);
    assert_eq!(
        (snapshot.value, snapshot.pressed, snapshot.released),
        (1., true, true)
    );
    let report = charge
        .step_edges(0.125, true, snapshot.pressed, snapshot.released)
        .unwrap();
    assert_report(
        report,
        &charge,
        ChargeRelease::Emitted(0.5),
        true,
        0.125,
        0.,
    );
    let snapshot = input.consume_tick(action);
    assert_eq!((snapshot.pressed, snapshot.released), (false, false));
    let report = charge
        .step_edges(0.125, true, snapshot.pressed, snapshot.released)
        .unwrap();
    assert_report(report, &charge, ChargeRelease::None, true, 0.25, 0.);
    input.feed(code, 0.);
    let snapshot = input.consume_tick(action);
    assert_eq!(
        charge
            .step_edges(0., false, snapshot.pressed, snapshot.released)
            .unwrap()
            .release,
        ChargeRelease::Emitted(0.25)
    );
}

#[test]
fn quick_tap_waits_across_no_tick_sampling_and_emits_once_in_catch_up_ticks() {
    let mut input = ActionMap::default();
    let code = InputCode::Key(32);
    let action = input.try_action(vec![Binding::digital(code)]).unwrap();
    let mut charge = ChargeAction::new(0., 1., 0.).unwrap();
    input.feed(code, 1.);
    input.feed(code, 0.);
    // A host frame executing zero ticks must not consume this pending snapshot.
    assert!(input.peek(action).pressed && input.peek(action).released);
    assert_eq!(state(&charge), (false, 0., 0.));
    let mut emitted = Vec::new();
    for _ in 0..4 {
        let snapshot = input.consume_tick(action);
        let report = charge
            .step_edges(
                1. / 64.,
                snapshot.value != 0.,
                snapshot.pressed,
                snapshot.released,
            )
            .unwrap();
        if let ChargeRelease::Emitted(power) = report.release {
            emitted.push(power);
        }
        assert!(!report.charging);
    }
    assert_eq!(emitted, [0.]);
}

#[test]
fn aggregated_final_unheld_resolves_only_old_charge_even_with_a_later_press() {
    for (duration, expected) in [
        (0.125, ChargeRelease::CanceledTooShort),
        (0.5, ChargeRelease::Emitted(0.5)),
    ] {
        let mut charge = ChargeAction::new(0.25, 1., 0.).unwrap();
        charge.step(duration, true).unwrap();
        let report = charge.step_edges(3600., false, true, true).unwrap();
        assert_report(report, &charge, expected, false, 0., 0.);
        assert_eq!(
            charge
                .step_edges(3600., false, false, false)
                .unwrap()
                .release,
            ChargeRelease::None
        );
    }
}

#[test]
fn exact_minimum_tolerance_and_power_cap_match_legacy() {
    let minimum = 0.25_f32;
    for duration in [
        minimum,
        f32::from_bits(minimum.to_bits() - 1),
        f32::from_bits(minimum.to_bits() - 6), // Just inside 1e-7 seconds.
        f32::from_bits(minimum.to_bits() - 7), // Just outside 1e-7 seconds.
        minimum - 0.000_001,
    ] {
        let mut charge = ChargeAction::new(minimum, 1., 0.5).unwrap();
        charge.step_edges(duration, true, true, false).unwrap();
        let report = charge.step_edges(0., false, false, true).unwrap();
        if f64::from(duration) + 1e-7 >= f64::from(minimum) {
            assert_report(
                report,
                &charge,
                ChargeRelease::Emitted(duration),
                false,
                0.,
                0.5,
            );
        } else {
            assert_report(
                report,
                &charge,
                ChargeRelease::CanceledTooShort,
                false,
                0.,
                0.,
            );
        }
    }
    for minimum in [0., 1e-7, f32::from_bits((1e-7_f32).to_bits() + 1)] {
        let mut charge = ChargeAction::new(minimum, 1., 0.).unwrap();
        let report = charge.step_edges(3600., false, true, true).unwrap();
        assert_eq!(
            report.release,
            if f64::from(minimum) <= 1e-7 {
                ChargeRelease::Emitted(0.)
            } else {
                ChargeRelease::CanceledTooShort
            }
        );
    }
    let mut charge = ChargeAction::new(0.25, 1., 0.).unwrap();
    for _ in 0..3 {
        assert_eq!(
            charge.step_edges(3600., true, true, false).unwrap().power,
            1.
        );
    }
    let report = charge.step_edges(0., true, true, true).unwrap();
    assert_report(report, &charge, ChargeRelease::Emitted(1.), true, 0., 0.);
}

#[test]
fn fresh_recovery_is_not_spent_twice_and_later_holds_use_only_its_remainder() {
    let mut charge = ChargeAction::new(0.25, 1., 0.5).unwrap();
    charge.step_edges(0.5, true, true, false).unwrap();
    let report = charge.step_edges(3600., true, true, true).unwrap();
    assert_report(report, &charge, ChargeRelease::Emitted(0.5), false, 0., 0.5);
    let report = charge.step_edges(0.125, true, false, false).unwrap();
    assert_report(report, &charge, ChargeRelease::None, false, 0., 0.375);
    let mut exact = charge;
    let report = exact.step_edges(0.375, true, false, false).unwrap();
    assert_report(report, &exact, ChargeRelease::None, true, 0., 0.);
    let report = charge.step_edges(0.5, true, false, false).unwrap();
    assert_report(report, &charge, ChargeRelease::None, true, 0.125, 0.);
    let report = charge.step_edges(3600., true, false, false).unwrap();
    assert_report(report, &charge, ChargeRelease::None, true, 1., 0.);
}

#[test]
fn cancel_preserves_recovery_reset_clears_it_and_configuration_survives() {
    let mut charge = ChargeAction::new(0.25, 1., 0.5).unwrap();
    charge.step(0.5, true).unwrap();
    charge.cancel();
    charge.cancel();
    assert_eq!(state(&charge), (false, 0., 0.));
    assert_eq!(
        charge.step_edges(0., false, false, true).unwrap().release,
        ChargeRelease::None
    );
    charge.step(0.5, true).unwrap();
    charge.step(0., false).unwrap();
    charge.step_edges(0.125, true, true, false).unwrap();
    charge.cancel();
    assert_eq!(state(&charge), (false, 0., 0.375));
    charge.reset();
    charge.reset();
    assert_eq!(state(&charge), (false, 0., 0.));
    assert_eq!(
        charge.step_edges(0., false, true, true).unwrap().release,
        ChargeRelease::CanceledTooShort
    );
    charge.step_edges(3600., true, true, false).unwrap();
    assert_eq!(
        charge.step_edges(0., false, false, true).unwrap().release,
        ChargeRelease::Emitted(1.)
    );
    assert_eq!(charge.cooldown_seconds(), 0.5);
}

#[test]
fn invalid_time_is_atomic_for_every_edge_combination_and_existing_state() {
    for phase in 0..3 {
        let mut initial = ChargeAction::new(0.25, 1., 0.5).unwrap();
        if phase > 0 {
            initial.step(0.5, true).unwrap();
        }
        if phase == 2 {
            initial.step(0., false).unwrap();
        }
        for (held, pressed, released) in FLAGS {
            for dt in [
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                -0.001,
                3600.5,
                f32::MAX,
            ] {
                let mut charge = initial;
                assert_eq!(
                    charge.step_edges(dt, held, pressed, released),
                    Err(LoopError::Invalid)
                );
                assert_eq!(state(&charge), state(&initial));
                // Check future behavior as well as the public read-only state.
                let mut reference = initial;
                for flags in FLAGS {
                    assert_eq!(
                        charge.step_edges(0.125, flags.0, flags.1, flags.2),
                        reference.step_edges(0.125, flags.0, flags.1, flags.2)
                    );
                }
            }
        }
    }
}

#[test]
fn held_only_api_and_no_edge_api_remain_equivalent_through_long_sequences() {
    // Compare all outcomes plus state across minimum/max/recovery boundaries,
    // zero dt, cancellation, reset, invalid inputs and long recovery remainders.
    for minimum in [0., 0.25, 1.] {
        for recovery in [0., 0.5, 3600.] {
            let mut legacy = ChargeAction::new(minimum, 1., recovery).unwrap();
            let mut edges = legacy;
            for round in 0..16 {
                for dt in [0., 0.125, 0.25, 0.375, 0.5, 1., 3600.] {
                    for held in [true, true, false, false, true] {
                        let old = legacy.step(dt, held).unwrap();
                        let new = edges.step_edges(dt, held, false, false).unwrap();
                        let emitted = match new.release {
                            ChargeRelease::Emitted(power) => Some(power),
                            _ => None,
                        };
                        assert_eq!(old, emitted);
                        assert_eq!(state(&legacy), state(&edges));
                    }
                }
                for dt in [f32::NAN, -1., 3601.] {
                    assert_eq!(legacy.step(dt, false), Err(LoopError::Invalid));
                    assert_eq!(
                        edges.step_edges(dt, false, false, false),
                        Err(LoopError::Invalid)
                    );
                    assert_eq!(state(&legacy), state(&edges));
                }
                if round % 2 == 0 {
                    legacy.cancel();
                    edges.cancel();
                } else {
                    legacy.reset();
                    edges.reset();
                }
            }
        }
    }
}

#[test]
fn constructor_limits_and_valid_time_extremes_are_unchanged() {
    for config in [
        (-1., 1., 0.),
        (0., 0., 0.),
        (2., 1., 0.),
        (0., 3601., 0.),
        (0., 1., -1.),
        (0., 1., 3601.),
        (f32::NAN, 1., 0.),
        (0., f32::INFINITY, 0.),
        (0., 1., f32::NEG_INFINITY),
    ] {
        assert!(matches!(
            ChargeAction::new(config.0, config.1, config.2),
            Err(LoopError::Invalid)
        ));
    }
    for maximum in [f32::from_bits(1), f32::MIN_POSITIVE, 3600.] {
        let mut charge = ChargeAction::new(maximum, maximum, 3600.).unwrap();
        assert_eq!(
            charge.step_edges(maximum, true, true, false).unwrap().power,
            1.
        );
        assert_eq!(
            charge.step_edges(-0., false, false, true).unwrap().release,
            ChargeRelease::Emitted(1.)
        );
        assert_eq!(charge.cooldown_seconds(), 3600.);
    }
}
