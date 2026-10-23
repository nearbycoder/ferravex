//! Integer experience curves, with at most 1023 promotions per award.
//!
//! Awards saturate the stored `u64` XP **before** subtracting promotion costs.
//! Excess at the level cap is retained within that integer bound. This is not
//! mathematically unbounded XP: saturation can discard part of a request, and
//! partitioning a saturating award can change its final remainder.
//!
//! Preview is advisory; only a newly accepted game event should call
//! [`Progression::award_report`] and grant its promotions. Neither preview nor
//! restore issues rewards. Games own event deduplication and must restore their
//! reward state consistently with progression, or retain a separate reward ledger
//! across rollback. This helper cannot undo external effects or prevent a game
//! from re-awarding a replayed event after loading an older snapshot.
use super::LoopError;

/// Exact result of applying the legacy saturating-add-then-promote rule.
///
/// Preview and commit return the same value if no state changes between them.
/// Copies of reports are observations, not single-use reward tokens. Only the
/// report returned by a new accepted award should drive rewards.
///
/// Using `u128` arithmetic, `xp_before + accepted_xp == xp_spent + xp_after`.
/// `accepted_xp` is the delta admitted by the initial saturating addition, not
/// `xp_after - xp_before`, which may be negative after promotion costs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AwardReport {
    /// XP requested by the caller.
    pub requested_xp: u64,
    /// XP admitted before promotion subtraction; at most `u64::MAX - xp_before`.
    pub accepted_xp: u64,
    /// Requested XP discarded by saturation, even if promotions free room later.
    pub discarded_xp: u64,
    /// Whether any requested XP was discarded. Exactly filling storage is false.
    pub saturated: bool,
    /// Level before this award.
    pub level_before: u16,
    /// Level after this award.
    pub level_after: u16,
    /// New promotions, bounded by 1023; zero when already at the cap.
    pub promotions: u16,
    /// Retained XP before the initial addition.
    pub xp_before: u64,
    /// Total XP subtracted for newly earned levels (not a lifetime total).
    pub xp_spent: u64,
    /// Retained remainder after promotion subtraction; may be large at the cap.
    pub xp_after: u64,
    /// Whether `level_after` equals the authored maximum, including maximum 1.
    pub at_cap: bool,
}

/// Plain state plus the exact authored curve, with no serialization or wire ABI.
///
/// Public fields let a host decode its own format. [`Progression::restore`]
/// validates every field before replacing state; this type alone is untrusted.
/// No fingerprint substitutes for equality of `base`, `growth` and `maximum`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgressionSnapshot {
    /// Positive initial promotion cost.
    pub base: u32,
    /// Per-level additive cost growth, including zero.
    pub growth: u32,
    /// Authored level cap, in `1..=1024`.
    pub maximum: u16,
    /// Current level, in `1..=maximum`.
    pub level: u16,
    /// Remainder, strictly below next cost unless at the cap.
    pub xp: u64,
}

/// First failed restore check, in declaration order. Rejection is atomic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreError {
    /// Snapshot has zero base or a maximum outside `1..=1024`.
    InvalidCurve,
    /// Any authored value differs from the destination's exact configuration.
    CurveMismatch,
    /// Level is zero or above the snapshot's maximum.
    InvalidLevel,
    /// XP could already buy a promotion below the cap: state is not normalized.
    UnsettledXp,
}

/// State-loading observation, deliberately containing no award/promotions.
///
/// An identical repeated restore has `before == after`. An older same-curve
/// snapshot is valid rollback; there is no timestamp, revision or anti-replay
/// guarantee. A snapshot from an old *different* curve is rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestoreReport {
    /// State before a successful load.
    pub before: ProgressionSnapshot,
    /// Validated state installed by the load.
    pub after: ProgressionSnapshot,
}

/// Private, normalized progression state with an immutable authored curve.
#[derive(Clone, Copy, Debug)]
pub struct Progression {
    base: u32,
    growth: u32,
    maximum: u16,
    level: u16,
    xp: u64,
}
impl Progression {
    /// Start at level 1 with no XP. Zero growth and a maximum of 1 are valid.
    ///
    /// ```
    /// use ferravex_play::gameplay::progression::Progression;
    /// let p = Progression::new(10, 5, 3).unwrap();
    /// assert_eq!((p.level(), p.xp(), p.next_cost()), (1, 0, Some(10)));
    /// assert!(Progression::new(0, 5, 3).is_err());
    /// ```
    pub fn new(base: u32, growth: u32, maximum: u16) -> Result<Self, LoopError> {
        if base == 0 || !(1..=1024).contains(&maximum) {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            base,
            growth,
            maximum,
            level: 1,
            xp: 0,
        })
    }
    pub fn level(&self) -> u16 {
        self.level
    }
    pub fn xp(&self) -> u64 {
        self.xp
    }
    pub fn next_cost(&self) -> Option<u64> {
        (self.level < self.maximum)
            .then(|| u64::from(self.base) + u64::from(self.growth) * u64::from(self.level - 1))
    }
    /// Returns earned promotions; hosts apply rewards once from this result.
    /// Excess at the level cap is retained for display, never re-awarded.
    /// Addition saturates at `u64::MAX` before promotion costs are subtracted.
    /// This is equivalent to [`Self::award_report`]'s `promotions` field.
    pub fn award(&mut self, amount: u64) -> u16 {
        self.award_report(amount).promotions
    }

    /// Compute the exact award without changing state or issuing rewards.
    ///
    /// A preview is not a reservation; recompute on commit after other awards or
    /// restores. At most 1023 iterations, with no allocation or callbacks.
    ///
    /// ```
    /// use ferravex_play::gameplay::progression::Progression;
    /// let mut p = Progression::new(10, 5, 3).unwrap();
    /// let preview = p.preview_award(30);
    /// assert_eq!((p.level(), p.xp()), (1, 0));
    /// assert_eq!((preview.promotions, preview.xp_spent, preview.xp_after), (2, 25, 5));
    /// assert!(preview.at_cap);
    /// assert_eq!(p.award_report(30), preview);
    /// ```
    pub fn preview_award(&self, amount: u64) -> AwardReport {
        let mut next = *self;
        next.xp = next.xp.saturating_add(amount);
        let accepted_xp = next.xp - self.xp;
        let mut xp_spent = 0;
        while let Some(cost) = next.next_cost() {
            if next.xp < cost {
                break;
            }
            next.xp -= cost;
            xp_spent += cost;
            next.level += 1;
        }
        let discarded_xp = amount - accepted_xp;
        AwardReport {
            requested_xp: amount,
            accepted_xp,
            discarded_xp,
            saturated: discarded_xp != 0,
            level_before: self.level,
            level_after: next.level,
            promotions: next.level - self.level,
            xp_before: self.xp,
            xp_spent,
            xp_after: next.xp,
            at_cap: next.level == self.maximum,
        }
    }

    /// Commit the same rule as [`Self::preview_award`], returning accepted data.
    ///
    /// Caller owns game-event acceptance and deduplication. Reward only this
    /// call's promotions, once; no lifetime reward ledger is kept here.
    ///
    /// ```
    /// use ferravex_play::gameplay::progression::Progression;
    /// let mut p = Progression::new(10, 0, 2).unwrap();
    /// p.award(9);
    /// let report = p.award_report(u64::MAX);
    /// assert_eq!((report.accepted_xp, report.discarded_xp), (u64::MAX - 9, 9));
    /// assert!(report.saturated);
    /// assert_eq!((report.promotions, p.xp()), (1, u64::MAX - 10));
    /// ```
    pub fn award_report(&mut self, amount: u64) -> AwardReport {
        let report = self.preview_award(amount);
        self.level = report.level_after;
        self.xp = report.xp_after;
        report
    }

    /// Capture state and every authored value, without normalizing or awarding.
    /// See [`Self::restore`] for checked loading and an executable example.
    pub fn snapshot(&self) -> ProgressionSnapshot {
        ProgressionSnapshot {
            base: self.base,
            growth: self.growth,
            maximum: self.maximum,
            level: self.level,
            xp: self.xp,
        }
    }

    /// Atomically load validated state for this exact curve; never issue rewards.
    ///
    /// Checks authoring, exact configuration, level bounds, then XP normalization.
    /// At the cap any `u64` remainder is valid. Older same-curve snapshots are
    /// deliberately accepted for rollback, without undoing external side effects.
    /// Restore corresponding game-owned reward/event state together, or maintain
    /// an external deduplication ledger for effects that must survive rollback.
    ///
    /// ```
    /// use ferravex_play::gameplay::progression::{Progression, RestoreError};
    /// let mut p = Progression::new(10, 5, 3).unwrap();
    /// let start = p.snapshot();
    /// p.award_report(12);
    /// let loaded = p.restore(start).unwrap();
    /// assert_eq!((loaded.before.level, loaded.after.level), (2, 1));
    /// let again = p.restore(start).unwrap();
    /// assert_eq!(again.before, again.after); // No reward to replay.
    /// let mut invalid = start;
    /// invalid.xp = 10; // A pending promotion is not normalized saved state.
    /// assert_eq!(p.restore(invalid), Err(RestoreError::UnsettledXp));
    /// assert_eq!(p.snapshot(), start);
    /// ```
    pub fn restore(
        &mut self,
        snapshot: ProgressionSnapshot,
    ) -> Result<RestoreReport, RestoreError> {
        let mut next = Self::new(snapshot.base, snapshot.growth, snapshot.maximum)
            .map_err(|_| RestoreError::InvalidCurve)?;
        if (next.base, next.growth, next.maximum) != (self.base, self.growth, self.maximum) {
            return Err(RestoreError::CurveMismatch);
        }
        if snapshot.level == 0 || snapshot.level > snapshot.maximum {
            return Err(RestoreError::InvalidLevel);
        }
        next.level = snapshot.level;
        if next.next_cost().is_some_and(|cost| snapshot.xp >= cost) {
            return Err(RestoreError::UnsettledXp);
        }
        next.xp = snapshot.xp;
        let report = RestoreReport {
            before: self.snapshot(),
            after: snapshot,
        };
        *self = next;
        Ok(report)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_and_partitioned_awards_have_identical_levels_and_remainders() {
        let mut a = Progression::new(10, 5, 8).unwrap();
        let mut b = a;
        assert_eq!(a.award(85), 4);
        for _ in 0..85 {
            b.award(1);
        }
        assert_eq!((a.level(), a.xp()), (b.level(), b.xp()));
        assert_eq!((a.level(), a.xp(), a.next_cost()), (5, 15, Some(30)));
        a.award(u64::MAX);
        assert_eq!(a.level(), 8);
        assert_eq!(a.next_cost(), None);
        assert_eq!(a.award(u64::MAX), 0);
        assert!(Progression::new(0, 1, 2).is_err());
    }
}
