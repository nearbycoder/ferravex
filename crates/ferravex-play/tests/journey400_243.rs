use ferravex_play::gameplay::{
    LoopError,
    progression::{AwardReport, Progression, ProgressionSnapshot, RestoreError},
};

fn conservation(report: AwardReport) {
    assert_eq!(
        u128::from(report.requested_xp),
        u128::from(report.accepted_xp) + u128::from(report.discarded_xp)
    );
    assert_eq!(
        u128::from(report.xp_before) + u128::from(report.accepted_xp),
        u128::from(report.xp_spent) + u128::from(report.xp_after)
    );
    assert_eq!(report.saturated, report.discarded_xp != 0);
    assert_eq!(report.promotions, report.level_after - report.level_before);
}

#[test]
fn preview_commit_and_legacy_agree_and_stale_preview_is_only_advisory() {
    for (base, growth, maximum) in [(10, 5, 8), (1, 0, 1024), (u32::MAX, u32::MAX, 1024)] {
        let mut p = Progression::new(base, growth, maximum).unwrap();
        for amount in [0, 1, 9, 10, 85, 1000, u64::MAX, 0, 1, u64::MAX] {
            let before = p.snapshot();
            let preview = p.preview_award(amount);
            assert_eq!(p.snapshot(), before);
            assert_eq!(p.preview_award(amount), preview);
            let mut legacy = p;
            assert_eq!(p.award_report(amount), preview);
            assert_eq!(legacy.award(amount), preview.promotions);
            assert_eq!(p.snapshot(), legacy.snapshot());
            assert_eq!((p.level(), p.xp()), (preview.level_after, preview.xp_after));
            assert_eq!(preview.at_cap, p.next_cost().is_none());
            conservation(preview);
        }
    }
    let mut p = Progression::new(10, 0, 3).unwrap();
    let old = p.preview_award(10);
    p.award(1);
    let committed = p.award_report(10);
    assert_ne!(old, committed);
    assert_eq!(committed.xp_after, 1);
}

#[test]
fn ordinary_partitioning_preserves_state_cost_and_promotions() {
    for total in [0, 9, 10, 24, 25, 85, 175, 1000] {
        for chunk in [1, 3, 13, 100] {
            let mut once = Progression::new(10, 5, 8).unwrap();
            let mut partitioned = once;
            let whole = once.award_report(total);
            let (mut promotions, mut spent, mut accepted) = (0, 0, 0);
            let mut remaining = total;
            while remaining != 0 {
                let amount = remaining.min(chunk);
                let part = partitioned.award_report(amount);
                conservation(part);
                promotions += part.promotions;
                spent += part.xp_spent;
                accepted += part.accepted_xp;
                remaining -= amount;
            }
            assert_eq!(partitioned.snapshot(), once.snapshot());
            assert_eq!(
                (promotions, spent, accepted),
                (whole.promotions, whole.xp_spent, total)
            );
        }
    }
}

#[test]
fn exact_boundary_zero_awards_and_cap_retain_excess() {
    let mut p = Progression::new(10, 5, 3).unwrap();
    assert_eq!(p.award_report(0).promotions, 0);
    assert_eq!(p.award_report(9).promotions, 0);
    let level_up = p.award_report(1);
    assert_eq!(
        level_up,
        AwardReport {
            requested_xp: 1,
            accepted_xp: 1,
            discarded_xp: 0,
            saturated: false,
            level_before: 1,
            level_after: 2,
            promotions: 1,
            xp_before: 9,
            xp_spent: 10,
            xp_after: 0,
            at_cap: false,
        }
    );
    assert_eq!(p.next_cost(), Some(15));
    let cap = p.award_report(20);
    assert_eq!(
        (cap.promotions, cap.xp_spent, cap.xp_after, cap.at_cap),
        (1, 15, 5, true)
    );
    assert_eq!(p.next_cost(), None);
    let extra = p.award_report(100);
    assert_eq!(
        (
            extra.promotions,
            extra.xp_spent,
            extra.accepted_xp,
            extra.xp_after
        ),
        (0, 0, 100, 105)
    );
    let zero = p.award_report(0);
    assert_eq!(
        (zero.xp_before, zero.xp_after, zero.promotions),
        (105, 105, 0)
    );
    assert!(!zero.saturated);
}

#[test]
fn saturation_precedes_costs_and_saturating_partitioning_is_not_equivalent() {
    let mut p = Progression::new(10, 0, 3).unwrap();
    p.award(9);
    let r = p.award_report(u64::MAX);
    assert_eq!(r.accepted_xp, u64::MAX - 9);
    assert_eq!(r.discarded_xp, 9);
    assert_eq!(
        (r.promotions, r.xp_spent, r.xp_after),
        (2, 20, u64::MAX - 20)
    );
    assert!(r.saturated && r.at_cap);
    conservation(r);

    let mut parts = Progression::new(10, 0, 3).unwrap();
    parts.award(9);
    parts.award(1); // A cost is subtracted before the large second addition.
    parts.award(u64::MAX - 1);
    assert_eq!(parts.level(), p.level());
    assert_eq!(parts.xp(), u64::MAX - 11);
    assert_ne!(parts.xp(), p.xp());

    // At cap, an exact fill is not reported as discarded XP.
    let fill = p.award_report(20);
    assert_eq!(
        (fill.accepted_xp, fill.xp_after, fill.saturated),
        (20, u64::MAX, false)
    );
    let full = p.award_report(u64::MAX);
    assert_eq!(
        (full.accepted_xp, full.discarded_xp, full.xp_spent),
        (0, u64::MAX, 0)
    );
    assert!(full.saturated);
    assert!(!p.award_report(0).saturated);
    conservation(full);
}

#[test]
fn constructor_limits_and_largest_curve_keep_bounded_exact_costs() {
    for (base, maximum) in [(0, 2), (1, 0), (1, 1025), (1, u16::MAX)] {
        assert!(matches!(
            Progression::new(base, 0, maximum),
            Err(LoopError::Invalid)
        ));
    }
    let mut capped = Progression::new(1, u32::MAX, 1).unwrap();
    assert_eq!(capped.next_cost(), None);
    let r = capped.award_report(u64::MAX);
    assert_eq!(
        (r.promotions, r.xp_spent, r.xp_after, r.saturated),
        (0, 0, u64::MAX, false)
    );
    assert_eq!(capped.award_report(1).discarded_xp, 1);

    let mut largest = Progression::new(u32::MAX, u32::MAX, 1024).unwrap();
    let r = largest.award_report(u64::MAX);
    let costs = u64::from(u32::MAX) * (1023 * 1024 / 2);
    assert_eq!(
        (r.promotions, r.xp_spent, r.xp_after),
        (1023, costs, u64::MAX - costs)
    );
    assert!(!r.saturated);
    conservation(r);
    let mut near = largest.snapshot();
    near.level = 1023;
    near.xp = 0;
    largest.restore(near).unwrap();
    assert_eq!(largest.next_cost(), Some(u64::from(u32::MAX) * 1023));
}

fn invalid_restore(p: &mut Progression, snapshot: ProgressionSnapshot, error: RestoreError) {
    let before = p.snapshot();
    let future = p.preview_award(u64::MAX);
    assert_eq!(p.restore(snapshot), Err(error));
    assert_eq!(p.snapshot(), before);
    assert_eq!(p.preview_award(u64::MAX), future);
}

#[test]
fn malformed_and_other_curve_snapshots_are_rejected_atomically() {
    let mut p = Progression::new(10, 5, 3).unwrap();
    p.award(12);
    let good = p.snapshot();
    for bad in [
        ProgressionSnapshot { base: 0, ..good },
        ProgressionSnapshot { maximum: 0, ..good },
        ProgressionSnapshot {
            maximum: 1025,
            ..good
        },
    ] {
        invalid_restore(&mut p, bad, RestoreError::InvalidCurve);
    }
    // Equality uses every authored value, even if today's cost/state agrees.
    for bad in [
        ProgressionSnapshot { base: 11, ..good },
        ProgressionSnapshot { growth: 6, ..good },
        ProgressionSnapshot { maximum: 4, ..good },
    ] {
        invalid_restore(&mut p, bad, RestoreError::CurveMismatch);
    }
    for level in [0, 4, u16::MAX] {
        invalid_restore(
            &mut p,
            ProgressionSnapshot { level, ..good },
            RestoreError::InvalidLevel,
        );
    }
    for xp in [15, 16, u64::MAX] {
        invalid_restore(
            &mut p,
            ProgressionSnapshot { xp, ..good },
            RestoreError::UnsettledXp,
        );
    }
    // No fingerprint or coincident next cost identifies a curve.
    let old_curve = Progression::new(10, 6, 3).unwrap().snapshot();
    assert_eq!(Progression::new(10, 6, 3).unwrap().next_cost(), Some(10));
    invalid_restore(&mut p, old_curve, RestoreError::CurveMismatch);
    let mut boundary = good;
    boundary.xp = 14;
    p.restore(boundary).unwrap();
    assert_eq!(p.award_report(1).promotions, 1);
}

#[test]
fn every_normalized_level_and_capped_remainder_round_trips() {
    let mut p = Progression::new(3, 2, 1024).unwrap();
    for level in 1..=1024 {
        let xp = if level == 1024 {
            u64::MAX
        } else {
            3 + 2 * u64::from(level - 1) - 1
        };
        let snapshot = ProgressionSnapshot {
            level,
            xp,
            ..p.snapshot()
        };
        assert_eq!(p.restore(snapshot).unwrap().after, snapshot);
        let preview = p.preview_award(1);
        assert_eq!(p.award_report(1), preview);
        assert_eq!(preview.promotions, u16::from(level != 1024));
    }
}

#[test]
fn repeat_load_rollback_and_reset_do_not_award_and_preserve_future_rules() {
    let mut p = Progression::new(10, 5, 4).unwrap();
    let start = p.snapshot();
    p.award(30);
    let saved = p.snapshot();
    let mut expected = p;
    p.award(u64::MAX);
    let loaded = p.restore(saved).unwrap();
    assert_eq!(loaded.before.level, 4);
    assert_eq!(loaded.after, saved);
    for _ in 0..3 {
        let again = p.restore(saved).unwrap();
        assert_eq!(again.before, again.after);
    }
    for amount in [0, 9, 20, 100, u64::MAX, 1] {
        assert_eq!(p.award_report(amount), expected.award_report(amount));
    }
    p.restore(start).unwrap(); // Reset by loading the initial same-curve state.
    let mut fresh = Progression::new(10, 5, 4).unwrap();
    assert_eq!(p.award_report(85), fresh.award_report(85));
    assert_eq!(p.snapshot(), fresh.snapshot());
}
