use ferravex_play::gameplay::{
    LoopError,
    shield::{Shield, ShieldChange, ShieldValuePolicy as Policy},
};

fn quiet(change: ShieldChange) {
    assert_eq!(change.value_change(), 0.);
    assert!(!change.depleted());
    assert!(!change.recovered());
    assert!(!change.filled());
}

#[test]
fn axes_exact_delay_empty_hits_and_one_shot_transitions() {
    let mut s = Shield::new(20., 10., 2.).unwrap();
    assert!(s.state().full && !s.state().broken && !s.state().recharging);
    let hit = s.absorb_detailed(30.).unwrap();
    assert_eq!((hit.absorbed, hit.unabsorbed), (20., 10.));
    assert!(hit.change.depleted());
    assert!(s.state().broken && s.state().delayed && !s.state().recharging);
    quiet(s.advance_detailed(1.).unwrap());
    let zero = s.absorb_detailed(0.).unwrap();
    quiet(zero.change);
    assert_eq!(s.delay_remaining(), 1.);
    let empty_hit = s.absorb_detailed(3.).unwrap();
    assert_eq!((empty_hit.absorbed, empty_hit.unabsorbed), (0., 3.));
    quiet(empty_hit.change);
    assert_eq!(s.delay_remaining(), 2.);
    quiet(s.advance_detailed(2.).unwrap());
    assert!(s.state().broken && s.state().recharging && !s.state().delayed);
    quiet(s.advance_detailed(0.).unwrap());
    let recharge = s.advance_detailed(0.5).unwrap();
    assert!(recharge.recovered());
    assert_eq!(recharge.value_change(), 5.);
    assert!(!s.advance_detailed(0.5).unwrap().recovered());
    let capped = s.advance_detailed(3600.).unwrap();
    assert!(capped.filled());
    assert_eq!(capped.value_change(), 10.);
    quiet(s.advance_detailed(3600.).unwrap());
}

#[test]
fn zero_maximum_rate_and_delay_are_independent() {
    for (maximum, rate) in [(0., 0.), (0., 10.), (20., 0.)] {
        let mut s = Shield::new(maximum, rate, 2.).unwrap();
        assert!(!s.state().recharge_enabled);
        assert_eq!(s.state().broken, maximum == 0.);
        assert!(s.state().full);
        s.absorb_detailed(30.).unwrap();
        assert!(s.state().broken && s.state().delayed);
        assert_eq!(s.state().full, maximum == 0.);
        quiet(s.advance_detailed(3600.).unwrap());
        assert!(!s.state().recharging && !s.state().delayed);
        assert_eq!(s.value(), 0.);
    }
    let mut instant = Shield::new(20., 10., 0.).unwrap();
    assert!(instant.absorb_detailed(20.).unwrap().change.depleted());
    assert!(instant.state().broken && instant.state().recharging);
    let change = instant.advance_detailed(3600.).unwrap();
    assert!(change.recovered() && change.filled());
    assert_eq!(change.value_change(), 20.);
}

#[test]
fn damage_routes_conservatively_while_storage_rounding_is_explicit() {
    for maximum in [0., f32::MIN_POSITIVE, 0.1, 20., 1e6] {
        for damage in [0., f32::MIN_POSITIVE, 0.1, 1., 20., 1e6] {
            let mut s = Shield::new(maximum, 0., 0.).unwrap();
            let hit = s.absorb_detailed(damage).unwrap();
            let error =
                (f64::from(hit.absorbed) + f64::from(hit.unabsorbed) - f64::from(damage)).abs();
            assert!(error <= f64::from(damage) * f64::from(f32::EPSILON));
            assert_eq!(hit.absorbed, damage.min(maximum));
            assert_eq!(
                hit.change.value_change(),
                f64::from(s.value()) - f64::from(maximum)
            );
        }
    }
    let mut s = Shield::new(1e6, 1., 0.).unwrap();
    let rounded = s.absorb_detailed(0.01).unwrap();
    assert_eq!(rounded.absorbed, 0.01);
    quiet(rounded.change); // Nominal absorption is below half a storage ULP.
    s.absorb(1.).unwrap();
    quiet(s.advance_detailed(0.01).unwrap());
    assert!(!s.state().full && s.state().recharging);
    let mut tiny = Shield::new(1., f32::from_bits(1), 0.).unwrap();
    tiny.absorb(1.).unwrap();
    quiet(tiny.advance_detailed(f32::from_bits(1)).unwrap());
    assert!(tiny.state().broken && tiny.state().recharging);
}

#[test]
fn preserve_value_clamps_shrink_and_never_heals_growth() {
    let mut s = Shield::new(20., 10., 2.).unwrap();
    s.absorb(5.).unwrap();
    quiet(s.reconfigure(40., 10., 2., Policy::PreserveValue).unwrap());
    assert_eq!(s.value(), 15.);
    let shrink = s.reconfigure(10., 10., 2., Policy::PreserveValue).unwrap();
    assert_eq!(shrink.value_change(), -5.);
    assert!(shrink.after.full);
    assert!(!shrink.filled()); // Capacity change is not a recharge event.
    let disabled = s.reconfigure(0., 10., 2., Policy::PreserveValue).unwrap();
    assert!(disabled.depleted());
    quiet(s.reconfigure(20., 10., 2., Policy::PreserveValue).unwrap());
    assert_eq!(s.value(), 0.);
}

#[test]
fn preserve_ratio_scales_both_directions_and_zero_capacity_is_empty() {
    let mut s = Shield::new(20., 10., 2.).unwrap();
    s.absorb(5.).unwrap();
    assert_eq!(
        s.reconfigure(40., 10., 2., Policy::PreserveRatio)
            .unwrap()
            .value_change(),
        15.
    );
    assert_eq!(s.value(), 30.);
    s.reconfigure(10., 10., 2., Policy::PreserveRatio).unwrap();
    assert_eq!(s.value(), 7.5);
    s.reconfigure(0., 10., 2., Policy::PreserveRatio).unwrap();
    quiet(s.reconfigure(20., 10., 2., Policy::PreserveRatio).unwrap());
    assert_eq!(s.value(), 0.);
    let mut full = Shield::new(20., 10., 2.).unwrap();
    full.reconfigure(40., 10., 2., Policy::PreserveRatio)
        .unwrap();
    assert_eq!(full.value(), 40.);
    let mut tiny = Shield::new(f32::from_bits(1), 1., 0.).unwrap();
    tiny.reconfigure(1e6, 1., 0., Policy::PreserveRatio)
        .unwrap();
    assert_eq!(tiny.value(), 1e6);
}

#[test]
fn edits_cap_pending_delay_without_extending_or_restarting() {
    let mut s = Shield::new(20., 10., 4.).unwrap();
    s.absorb(20.).unwrap();
    s.advance(1.).unwrap();
    for _ in 0..3 {
        quiet(s.reconfigure(20., 20., 8., Policy::PreserveValue).unwrap());
        assert_eq!(s.delay_remaining(), 3.);
    }
    s.reconfigure(20., 0., 2., Policy::PreserveValue).unwrap();
    assert_eq!(s.delay_remaining(), 2.);
    quiet(s.advance_detailed(1.).unwrap());
    s.reconfigure(20., 5., 0.5, Policy::PreserveValue).unwrap();
    assert_eq!(s.delay_remaining(), 0.5);
    let change = s.advance_detailed(1.).unwrap();
    assert!(change.recovered());
    assert_eq!(s.value(), 2.5);
    quiet(s.reconfigure(20., 10., 8., Policy::PreserveValue).unwrap());
    assert_eq!(s.delay_remaining(), 0.);
    s.advance(1.).unwrap();
    assert_eq!(s.value(), 12.5);
    s.absorb(1.).unwrap();
    assert_eq!(s.delay_remaining(), 8.); // Subsequent hits use new authored delay.
    quiet(s.reconfigure(20., 10., 0., Policy::PreserveValue).unwrap());
    assert_eq!(s.delay_remaining(), 0.);
}

#[test]
fn rate_changes_during_recharge_never_add_charge_or_restart_delay() {
    let mut s = Shield::new(20., 10., 1.).unwrap();
    s.absorb(20.).unwrap();
    s.advance(1.5).unwrap();
    assert_eq!(s.value(), 5.);
    quiet(s.reconfigure(20., 0., 8., Policy::PreserveValue).unwrap());
    assert!(!s.state().recharge_enabled && !s.state().delayed);
    quiet(s.advance_detailed(100.).unwrap());
    quiet(s.reconfigure(20., 2., 8., Policy::PreserveValue).unwrap());
    assert!(s.state().recharging);
    assert_eq!(s.advance_detailed(1.).unwrap().value_change(), 2.);
    assert_eq!(s.value(), 7.);
    assert_eq!(s.delay_remaining(), 0.);
}

#[test]
fn validation_is_atomic_for_every_field_damage_and_timestep() {
    let invalid = [-1., f32::NEG_INFINITY, f32::INFINITY, f32::NAN, f32::MAX];
    let mut s = Shield::new(20., 10., 4.).unwrap();
    s.absorb(3.).unwrap();
    s.advance(1.).unwrap();
    let before = s.state();
    let config = (s.maximum(), s.recharge_rate(), s.recharge_delay());
    for bad in invalid {
        for policy in [Policy::PreserveValue, Policy::PreserveRatio] {
            for (max, rate, delay) in [(bad, 0., 0.), (40., bad, 0.), (40., 20., bad)] {
                assert_eq!(
                    s.reconfigure(max, rate, delay, policy),
                    Err(LoopError::Invalid)
                );
                assert_eq!(s.state(), before);
                assert_eq!((s.maximum(), s.recharge_rate(), s.recharge_delay()), config);
                assert!(Shield::new(max, rate, delay).is_err());
            }
        }
        assert_eq!(s.absorb_detailed(bad), Err(LoopError::Invalid));
        assert_eq!(s.absorb(bad), Err(LoopError::Invalid));
        assert_eq!(s.advance_detailed(bad), Err(LoopError::Invalid));
        assert_eq!(s.advance(bad), Err(LoopError::Invalid));
        assert_eq!(s.state(), before);
    }
    assert_eq!(s.advance_detailed(3600.0002), Err(LoopError::Invalid));
    assert_eq!(
        s.reconfigure(1_000_000.1, 0., 0., Policy::PreserveValue),
        Err(LoopError::Invalid)
    );
    assert_eq!(
        s.reconfigure(0., 1_000_000.1, 0., Policy::PreserveValue),
        Err(LoopError::Invalid)
    );
    assert_eq!(
        s.reconfigure(0., 0., 3600.0002, Policy::PreserveValue),
        Err(LoopError::Invalid)
    );
    assert_eq!(s.absorb_detailed(1_000_000.1), Err(LoopError::Invalid));
    assert_eq!(s.state(), before);
    assert!(Shield::new(1e6, 1e6, 3600.).is_ok());
    assert!(Shield::new(-0., -0., -0.).is_ok());
}

#[test]
fn partitioned_recharge_uses_documented_float_tolerance() {
    let mut whole = Shield::new(100., 3.7, 0.3).unwrap();
    whole.absorb(100.).unwrap();
    let mut parts = whole;
    whole.advance(10.).unwrap();
    for _ in 0..1000 {
        parts.advance(0.01).unwrap();
    }
    // 1000 per-call f32 roundings, plus represented dt and delay error. This is
    // a test-specific tolerance, not a general bitwise partition guarantee.
    assert!((whole.value() - parts.value()).abs() < 0.002);
    assert_eq!(whole.delay_remaining(), parts.delay_remaining());
    assert_eq!(whole.state().recharging, parts.state().recharging);
    assert_eq!(whole.state().broken, parts.state().broken);
}

#[test]
fn legacy_arithmetic_matches_original_model_including_signed_zero() {
    // Frozen pre-task arithmetic: compare both wrappers and detailed paths to
    // an independent model rather than merely comparing the new wrappers.
    for (maximum, rate, delay) in [
        (20., 10., 2.),
        (0., 0., 0.),
        (-0., -0., -0.),
        (f32::from_bits(1), f32::from_bits(1), 0.),
        (1e6, 3.7, 0.3),
        (0.1, 1e6, 3600.),
    ] {
        let mut legacy = Shield::new(maximum, rate, delay).unwrap();
        let mut detailed = legacy;
        let mut value = maximum;
        let mut remaining = 0_f64;
        for i in 0..500 {
            let damage: f32 = [0., -0., 0.01, 0.1, 20., 1e6][i % 6];
            let expected = if damage == 0. {
                0.
            } else {
                remaining = f64::from(delay);
                let absorbed = damage.min(value);
                value -= absorbed;
                damage - absorbed
            };
            assert_eq!(legacy.absorb(damage).unwrap().to_bits(), expected.to_bits());
            assert_eq!(
                detailed
                    .absorb_detailed(damage)
                    .unwrap()
                    .unabsorbed
                    .to_bits(),
                expected.to_bits()
            );
            let dt = [0., -0., 0.01, 0.3, 2., 3600.][i % 6];
            let dt64 = f64::from(dt);
            let recovery = (dt64 - remaining).max(0.);
            remaining = (remaining - dt64).max(0.);
            value = (f64::from(value) + f64::from(rate) * recovery).min(f64::from(maximum)) as f32;
            legacy.advance(dt).unwrap();
            detailed.advance_detailed(dt).unwrap();
            assert_eq!(legacy.value().to_bits(), value.to_bits());
            assert_eq!(detailed.value().to_bits(), value.to_bits());
            assert_eq!(legacy.delay_remaining().to_bits(), remaining.to_bits());
            assert_eq!(detailed.state(), legacy.state());
        }
    }
}

#[test]
fn reset_replaces_charge_and_timer_without_changing_copy_storage() {
    fn assert_copy<T: Copy>() {}
    assert_copy::<Shield>();
    #[allow(dead_code)]
    struct OldLayout {
        maximum: f32,
        value: f32,
        rate: f32,
        delay: f64,
        remaining: f64,
    }
    assert_eq!(
        std::mem::size_of::<Shield>(),
        std::mem::size_of::<OldLayout>()
    );
    let mut s = Shield::new(20., 8., 4.).unwrap();
    s.absorb(30.).unwrap();
    assert!(s.state().broken && s.state().delayed);
    s = Shield::new(20., 8., 4.).unwrap();
    assert!(s.state().full && !s.state().delayed);
    assert_eq!(s.value(), 20.);
    assert_eq!(s.delay_remaining(), 0.);
}
