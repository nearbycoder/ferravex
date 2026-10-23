//! Task 248: opt-in resistance, transition receipts and legacy compatibility.
use ferravex_play::gameplay::LoopError;
use ferravex_play::gameplay::stagger::{
    Stagger, StaggerAdvance, StaggerHit, StaggerPhase, StaggerRecoveryPolicy,
};

fn resistant() -> Stagger {
    Stagger::with_recovery_policy(
        10.,
        2.,
        0.5,
        StaggerRecoveryPolicy::reject_during_grace(0.25).unwrap(),
    )
    .unwrap()
}

#[test]
fn concentrated_and_spaced_hits_have_different_accepted_onsets() {
    let mut concentrated = resistant();
    assert_eq!(
        concentrated.hit_report(6.).unwrap(),
        StaggerHit::Accumulated
    );
    assert_eq!(concentrated.buildup(), 0.6);
    assert_eq!(concentrated.hit_report(4.).unwrap(), StaggerHit::Onset);
    assert_eq!(concentrated.buildup(), 0.);
    assert_eq!(concentrated.phase(), StaggerPhase::Staggered);
    let mut spaced = resistant();
    assert!(!spaced.hit(6.).unwrap());
    spaced.advance(1.).unwrap();
    assert_eq!(spaced.buildup(), 0.4);
    assert_eq!(spaced.hit_report(4.).unwrap(), StaggerHit::Accumulated);
    spaced.advance(4.).unwrap();
    assert_eq!(spaced.buildup(), 0.);
    assert_eq!(spaced.phase(), StaggerPhase::Idle);
}

#[test]
fn exact_boundaries_and_one_shot_receipts() {
    let mut s = resistant();
    assert_eq!(s.hit_report(10.).unwrap(), StaggerHit::Onset);
    assert_eq!(s.advance_report(0.).unwrap(), StaggerAdvance::default());
    assert_eq!(s.stagger_remaining(), 0.5);
    assert_eq!(s.grace_remaining(), 0.);
    assert_eq!(s.advance_report(0.25).unwrap(), StaggerAdvance::default());
    assert_eq!(s.stagger_remaining(), 0.25);
    assert_eq!(s.hit_report(0.).unwrap(), StaggerHit::RejectedActive);
    assert_eq!(
        s.advance_report(0.25).unwrap(),
        StaggerAdvance {
            recovered_at: Some(0.25),
            grace_ended_at: None,
        }
    );
    assert!(!s.active());
    assert_eq!(s.phase(), StaggerPhase::Grace);
    assert_eq!(s.grace_remaining(), 0.25);
    assert_eq!(s.hit_report(10.).unwrap(), StaggerHit::RejectedGrace);
    assert_eq!(s.advance_report(0.).unwrap(), StaggerAdvance::default());
    assert_eq!(s.advance_report(0.125).unwrap(), StaggerAdvance::default());
    assert_eq!(s.grace_remaining(), 0.125);
    assert_eq!(
        s.advance_report(0.125).unwrap(),
        StaggerAdvance {
            recovered_at: None,
            grace_ended_at: Some(0.125),
        }
    );
    assert_eq!(s.phase(), StaggerPhase::Idle);
    assert_eq!(s.advance_report(0.).unwrap(), StaggerAdvance::default());
    assert_eq!(s.hit_report(0.).unwrap(), StaggerHit::Accumulated);
    assert_eq!(s.hit_report(10.).unwrap(), StaggerHit::Onset);
}

#[test]
fn partial_crossing_begins_grace_at_actual_recovery() {
    let mut s = resistant();
    s.hit(10.).unwrap();
    let report = s.advance_report(0.625).unwrap();
    assert_eq!(report.recovered_at, Some(0.5));
    assert_eq!(report.grace_ended_at, None);
    assert_eq!(s.grace_remaining(), 0.125);
    assert_eq!(s.hit_report(1e6).unwrap(), StaggerHit::RejectedGrace);
    assert_eq!(s.buildup(), 0.);
    assert_eq!(s.advance_report(0.125).unwrap().grace_ended_at, Some(0.125));
    assert_eq!(s.hit_report(9.).unwrap(), StaggerHit::Accumulated);
    assert_eq!(s.buildup(), 0.9); // Rejected hits left no hidden poise.
}

#[test]
fn large_step_crosses_each_boundary_once_and_consumes_idle_remainder() {
    let mut s = resistant();
    s.hit(10.).unwrap();
    assert_eq!(
        s.advance_report(3600.).unwrap(),
        StaggerAdvance {
            recovered_at: Some(0.5),
            grace_ended_at: Some(0.75),
        }
    );
    assert_eq!(s.phase(), StaggerPhase::Idle);
    assert_eq!(s.stagger_remaining(), 0.);
    assert_eq!(s.grace_remaining(), 0.);
    assert_eq!(s.advance_report(3600.).unwrap(), StaggerAdvance::default());
    assert_eq!(s.hit_report(8.).unwrap(), StaggerHit::Accumulated);
    s.advance(3.).unwrap();
    assert_eq!(s.buildup(), 0.2);
    s.advance(1.).unwrap();
    assert_eq!(s.buildup(), 0.);
}

#[test]
fn binary_time_partitioning_matches_one_large_step() {
    for total in [0., 0.125, 0.5, 0.625, 0.75, 1., 8.] {
        let mut whole = resistant();
        whole.hit(10.).unwrap();
        let mut split = whole;
        let all = whole.advance_report(total).unwrap();
        let mut elapsed = 0.;
        let mut recovery = None;
        let mut grace_end = None;
        while elapsed < total {
            let dt = (total - elapsed).min(0.125);
            let r = split.advance_report(dt).unwrap();
            if let Some(offset) = r.recovered_at {
                assert!(recovery.is_none());
                recovery = Some(f64::from(elapsed) + offset);
            }
            if let Some(offset) = r.grace_ended_at {
                assert!(grace_end.is_none());
                grace_end = Some(f64::from(elapsed) + offset);
            }
            elapsed += dt;
        }
        assert_eq!(whole, split);
        assert_eq!(all.recovered_at, recovery);
        assert_eq!(all.grace_ended_at, grace_end);
    }
}

#[test]
fn spam_in_both_phases_cannot_extend_timers_or_indefinitely_lock_out() {
    let mut s = resistant();
    for _ in 0..16 {
        assert_eq!(s.hit_report(10.).unwrap(), StaggerHit::Onset);
        for _ in 0..8 {
            let before = s;
            for _ in 0..100 {
                assert_eq!(s.hit_report(1e6).unwrap(), StaggerHit::RejectedActive);
            }
            assert_eq!(s, before);
            s.advance(0.0625).unwrap();
        }
        assert_eq!(s.phase(), StaggerPhase::Grace);
        for _ in 0..4 {
            let before = s;
            for _ in 0..100 {
                assert_eq!(s.hit_report(1e6).unwrap(), StaggerHit::RejectedGrace);
            }
            assert_eq!(s, before);
            s.advance(0.0625).unwrap();
        }
        assert_eq!(s.phase(), StaggerPhase::Idle);
        assert_eq!(s.buildup(), 0.);
    }
}

#[test]
fn legacy_bool_and_unit_wrappers_discard_reports_without_replaying_them() {
    let mut s = resistant();
    assert!(s.hit(10.).unwrap());
    assert_eq!(s.hit_report(10.).unwrap(), StaggerHit::RejectedActive);
    s.advance(0.5).unwrap();
    assert_eq!(s.advance_report(0.).unwrap(), StaggerAdvance::default());
    assert!(!s.hit(10.).unwrap());
    s.advance(0.25).unwrap();
    assert_eq!(s.advance_report(0.).unwrap(), StaggerAdvance::default());
    assert!(s.hit(10.).unwrap());
}

#[test]
fn default_and_explicit_disabled_policy_preserve_original_regression_sequence() {
    for policy in [
        StaggerRecoveryPolicy::default(),
        StaggerRecoveryPolicy::reject_during_grace(0.).unwrap(),
    ] {
        let mut s = Stagger::with_recovery_policy(10., 2., 0.5, policy).unwrap();
        let mut legacy = Stagger::new(10., 2., 0.5).unwrap();
        assert_eq!(s, legacy);
        assert!(!s.hit(6.).unwrap());
        assert!(!legacy.hit(6.).unwrap());
        s.advance(1.).unwrap();
        legacy.advance(1.).unwrap();
        assert_eq!(s.buildup().to_bits(), 0.4_f32.to_bits());
        assert!(s.hit(6.).unwrap());
        assert!(legacy.hit(6.).unwrap());
        s.advance(0.25).unwrap();
        legacy.advance(0.25).unwrap();
        assert!(!s.hit(100.).unwrap());
        assert!(!legacy.hit(100.).unwrap());
        let report = s.advance_report(0.25).unwrap();
        legacy.advance(0.25).unwrap();
        assert_eq!(report.recovered_at, Some(0.25));
        assert_eq!(report.grace_ended_at, None);
        assert_eq!(s.phase(), StaggerPhase::Idle);
        assert_eq!(s, legacy);
        assert!(s.hit(10.).unwrap()); // Immediate re-stagger remains allowed.
        assert!(legacy.hit(10.).unwrap());
        assert_eq!(s, legacy);
    }
}

#[test]
fn invalid_hits_and_time_are_atomic_in_every_phase_including_future_behavior() {
    let mut idle = resistant();
    idle.hit(6.).unwrap();
    let mut active = resistant();
    active.hit(10.).unwrap();
    let mut grace = active;
    grace.advance(0.5).unwrap();
    for initial in [idle, active, grace] {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1., 1_000_001.] {
            let mut changed = initial;
            assert_eq!(changed.hit_report(invalid), Err(LoopError::Invalid));
            assert_eq!(changed, initial);
            assert_eq!(changed.hit(invalid), Err(LoopError::Invalid));
            assert_eq!(changed, initial);
            assert_same_future(changed, initial);
        }
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1., 3600.25] {
            let mut changed = initial;
            assert_eq!(changed.advance_report(invalid), Err(LoopError::Invalid));
            assert_eq!(changed, initial);
            assert_eq!(changed.advance(invalid), Err(LoopError::Invalid));
            assert_eq!(changed, initial);
            assert_same_future(changed, initial);
        }
    }
}

fn assert_same_future(mut left: Stagger, mut right: Stagger) {
    for _ in 0..16 {
        assert_eq!(left.hit_report(6.), right.hit_report(6.));
        assert_eq!(left.advance_report(0.125), right.advance_report(0.125));
        assert_eq!(left, right);
    }
}

#[test]
fn configuration_validates_all_bounds_without_an_in_place_edit_path() {
    let policy = StaggerRecoveryPolicy::reject_during_grace(0.25).unwrap();
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1., 3600.25] {
        assert_eq!(
            StaggerRecoveryPolicy::reject_during_grace(invalid),
            Err(LoopError::Invalid)
        );
        assert_eq!(policy.grace_seconds(), 0.25);
    }
    for (threshold, decay, duration) in [
        (0., 2., 0.5),
        (0.009, 2., 0.5),
        (1_000_001., 2., 0.5),
        (f32::NAN, 2., 0.5),
        (10., -1., 0.5),
        (10., 1_000_001., 0.5),
        (10., f32::INFINITY, 0.5),
        (10., 2., 0.),
        (10., 2., -1.),
        (10., 2., 3600.25),
        (10., 2., f32::NAN),
    ] {
        assert_eq!(
            Stagger::with_recovery_policy(threshold, decay, duration, policy),
            Err(LoopError::Invalid)
        );
        assert_eq!(
            Stagger::new(threshold, decay, duration),
            Err(LoopError::Invalid)
        );
    }
}

#[test]
fn reset_clears_poise_active_and_grace_silently_and_preserves_tuning() {
    let clean = resistant();
    let mut building = clean;
    building.hit(6.).unwrap();
    let mut active = clean;
    active.hit(10.).unwrap();
    let mut grace = active;
    grace.advance(0.5).unwrap();
    for mut s in [building, active, grace] {
        s.reset();
        assert_eq!(s, clean);
        assert_eq!(s.advance_report(0.).unwrap(), StaggerAdvance::default());
        assert_same_future(s, clean);
        assert!(s.hit(10.).unwrap());
        s.advance(0.5).unwrap();
        assert_eq!(s.grace_remaining(), 0.25);
        s.reset();
        s.reset();
        assert_eq!(s, clean);
    }
}

#[test]
fn finite_extremes_tiny_durations_and_decay_remain_bounded() {
    let tiny = f32::from_bits(1);
    let policy = StaggerRecoveryPolicy::reject_during_grace(tiny).unwrap();
    let mut s = Stagger::with_recovery_policy(0.01, 1e6, tiny, policy).unwrap();
    assert_eq!(s.hit_report(1e6).unwrap(), StaggerHit::Onset);
    let r = s.advance_report(tiny).unwrap();
    assert_eq!(r.recovered_at, Some(f64::from(tiny)));
    assert_eq!(r.grace_ended_at, None);
    assert_eq!(s.hit_report(1e6).unwrap(), StaggerHit::RejectedGrace);
    assert_eq!(
        s.advance_report(tiny).unwrap().grace_ended_at,
        Some(f64::from(tiny))
    );
    assert_eq!(s.hit_report(0.005).unwrap(), StaggerHit::Accumulated);
    s.advance(3600.).unwrap();
    assert_eq!(s.buildup(), 0.);
    let max_policy = StaggerRecoveryPolicy::reject_during_grace(3600.).unwrap();
    let mut s = Stagger::with_recovery_policy(1e6, 1e6, 3600., max_policy).unwrap();
    assert!(!s.hit(999_999.).unwrap());
    s.advance(3600.).unwrap();
    assert_eq!(s.buildup(), 0.);
    assert!(s.hit(1e6).unwrap());
    assert_eq!(s.advance_report(3600.).unwrap().recovered_at, Some(3600.));
    assert_eq!(s.grace_remaining(), 3600.);
    assert_eq!(s.advance_report(3600.).unwrap().grace_ended_at, Some(3600.));
    let mut no_decay = Stagger::new(1e6, 0., 0.5).unwrap();
    no_decay.hit(999_999.).unwrap();
    no_decay.advance(3600.).unwrap();
    assert_eq!(no_decay.buildup(), 999_999. / 1e6);
    assert!(no_decay.hit(1.).unwrap());
}
