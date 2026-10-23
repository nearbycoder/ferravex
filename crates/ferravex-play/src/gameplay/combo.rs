//! Timed chains reward continued accepted actions. Hosts own event deduplication.
//!
//! Call an award method only after the game's action is accepted. Inspection
//! never accepts an action, changes state, or refreshes the simulation timer.
//! [`ScoreChain::award_with_receipt`] preserves legacy saturation; [`ScoreChain::try_award`]
//! instead rejects either counter overflow atomically. Display `total_change`,
//! not `nominal_points`, as the amount added to this helper's total.
//!
//! ```
//! use ferravex_play::gameplay::combo::{ChainAdvance, ChainReset, ScoreChain};
//! let mut score = ScoreChain::new(2.0, 2, 3).unwrap();
//! let preview = score.preview_award(100).unwrap();
//! assert_eq!(score.remaining_life(), None);
//! let receipt = score.try_award(100).unwrap().unwrap();
//! assert_eq!(receipt, preview);
//! assert_eq!((receipt.base, receipt.tier, receipt.multiplier), (100, 1, 1));
//! assert_eq!((receipt.nominal_points, receipt.total_change), (100, 100));
//! assert!(!receipt.chain_saturated && !receipt.total_saturated);
//! assert_eq!((receipt.chain, receipt.total_before, receipt.total_after), (1, 0, 100));
//! assert_eq!(score.award_with_receipt(0), None);
//! assert_eq!(score.remaining_life(), Some(2.0));
//! assert_eq!(score.advance_with_outcome(1.0).unwrap(), ChainAdvance::Active);
//! assert_eq!(score.advance_with_outcome(1.0).unwrap(), ChainAdvance::Expired { chain: 1 });
//! assert_eq!(score.advance_with_outcome(0.0).unwrap(), ChainAdvance::Idle);
//! score.reset(ChainReset::Run); // Clear run score, retain best chain.
//! assert_eq!((score.chain(), score.total(), score.best()), (0, 0, 1));
//! score.reset(ChainReset::All);
//! assert_eq!(score.best(), 0);
//! score.award(25);
//! score.reset(ChainReset::ChainOnly);
//! assert_eq!((score.chain(), score.total(), score.best()), (0, 25, 1));
//! ```
use super::{LoopError, seconds};

/// An accepted award, or a forecast returned by [`ScoreChain::preview_award`].
/// A forecast is not feedback evidence; see the module example for acceptance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScoreAward {
    /// Caller-supplied nonzero base points.
    pub base: u32,
    /// One-based tier after this action, before applying the multiplier cap.
    pub tier: u32,
    /// Capped multiplier used for this award.
    pub multiplier: u32,
    /// Chain length after this action, saturating at `u32::MAX`.
    pub chain: u32,
    /// `base * multiplier`, exactly representable in `u64` (maximum multiplier 100).
    pub nominal_points: u64,
    /// Actual increase in the helper's total; can be smaller than nominal points.
    pub total_change: u64,
    /// Total before this award.
    pub total_before: u64,
    /// Total after this award, saturating at `u64::MAX`.
    pub total_after: u64,
    /// Incrementing the chain exceeded `u32::MAX`; merely reaching it is false.
    pub chain_saturated: bool,
    /// Adding nominal points exceeded `u64::MAX`; merely reaching it is false.
    pub total_saturated: bool,
}

/// Counters that would overflow in an atomically rejected [`ScoreChain::try_award`].
/// Both flags can be true. The rejection changes no field, including timing/best.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScoreOverflow {
    /// The chain increment cannot be represented in `u32`.
    pub chain: bool,
    /// The nominal score addition cannot be represented in `u64`.
    pub total: bool,
}

/// Result of advancing simulation time; an expiry is reported exactly once.
/// See the module example. Inspection alone never triggers expiry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainAdvance {
    /// No chain was active.
    Idle,
    /// The chain remains active.
    Active,
    /// This chain length expired and was cleared; total and best were retained.
    Expired { chain: u32 },
}

/// Explicit reset policy. Every mode retains window, actions per tier and cap.
/// See the module example for resetting alongside a host's session lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChainReset {
    /// Clear chain and remaining time; retain total and best (legacy break).
    ChainOnly,
    /// Clear chain, remaining time and total; retain best.
    Run,
    /// Clear chain, remaining time, total and best.
    All,
}

/// Allocation-free scoring with an authored simulation-time chain window.
/// Expiry occurs on advancement at the inclusive `remaining <= 1e-9` boundary,
/// including a zero-delta advance. A newly awarded sub-epsilon window remains
/// active until advancement. See the module example for receipts and resets.
#[derive(Clone, Copy, Debug)]
pub struct ScoreChain {
    window: f64,
    remaining: f64,
    chain: u32,
    per_tier: u32,
    maximum: u32,
    total: u64,
    best: u32,
}
impl ScoreChain {
    pub fn new(window: f32, per_tier: u32, maximum: u32) -> Result<Self, LoopError> {
        let window = seconds(window)?;
        if window == 0. || per_tier == 0 || maximum == 0 || maximum > 100 {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            window,
            remaining: 0.,
            chain: 0,
            per_tier,
            maximum,
            total: 0,
            best: 0,
        })
    }
    pub fn advance(&mut self, dt: f32) -> Result<(), LoopError> {
        self.advance_with_outcome(dt).map(|_| ())
    }
    /// Advance once per executed simulation tick, reporting expiry once.
    ///
    /// Invalid `dt` (non-finite, negative, or above 3600 seconds) returns
    /// [`LoopError::Invalid`] with every field unchanged, even if idle.
    /// Total and best survive expiry. See the module example.
    pub fn advance_with_outcome(&mut self, dt: f32) -> Result<ChainAdvance, LoopError> {
        let dt = seconds(dt)?;
        let chain = self.chain;
        self.remaining = (self.remaining - dt).max(0.);
        if self.remaining <= 1e-9 {
            self.break_chain();
            return Ok(if chain == 0 {
                ChainAdvance::Idle
            } else {
                ChainAdvance::Expired { chain }
            });
        }
        Ok(ChainAdvance::Active)
    }
    /// Legacy saturating award, returning **nominal** points even at total saturation.
    /// Zero is a no-op. Use [`Self::award_with_receipt`] for actual total changes.
    pub fn award(&mut self, base: u32) -> u64 {
        self.award_with_receipt(base)
            .map_or(0, |award| award.nominal_points)
    }
    /// Forecast the legacy saturating policy without mutating or refreshing time.
    ///
    /// Zero returns `None` without changes. Tier is one-based and uncapped;
    /// multiplier is capped. Overflow flags describe lost increments, rather
    /// than a counter merely reaching its maximum. See the module example.
    pub fn preview_award(&self, base: u32) -> Option<ScoreAward> {
        if base == 0 {
            return None;
        }
        let chain = self.chain.saturating_add(1);
        let tier = 1 + chain.saturating_sub(1) / self.per_tier;
        let multiplier = tier.min(self.maximum);
        let nominal_points = u64::from(base) * u64::from(multiplier);
        let total_after = self.total.saturating_add(nominal_points);
        Some(ScoreAward {
            base,
            tier,
            multiplier,
            chain,
            nominal_points,
            total_change: total_after - self.total,
            total_before: self.total,
            total_after,
            chain_saturated: self.chain == u32::MAX,
            total_saturated: self.total.checked_add(nominal_points).is_none(),
        })
    }
    /// Accept with explicit saturation, sharing the exact policy of [`Self::award`].
    ///
    /// A nonzero base always increments/saturates the chain, updates best and
    /// refreshes the window, even if total cannot increase. Receipt flags expose
    /// each overflow independently. Zero returns `None` and changes nothing.
    /// Emit feedback only from this accepted result, never from a preview.
    /// See the module example; total changes concern this helper only, not any
    /// separate narrower counter maintained by a consumer.
    pub fn award_with_receipt(&mut self, base: u32) -> Option<ScoreAward> {
        let award = self.preview_award(base)?;
        self.apply_award(award);
        Some(award)
    }
    /// Accept only if both chain increment and nominal total addition fit.
    ///
    /// Either overflow returns [`ScoreOverflow`] **before any mutation**.
    /// Zero returns `Ok(None)` even at counter bounds. Successful receipts have
    /// both saturation flags false. Games that already committed an action and
    /// require legacy saturation should use [`Self::award_with_receipt`] instead;
    /// rejection here does not roll back another system's accepted action.
    /// See the module example for usage.
    pub fn try_award(&mut self, base: u32) -> Result<Option<ScoreAward>, ScoreOverflow> {
        let Some(award) = self.preview_award(base) else {
            return Ok(None);
        };
        if award.chain_saturated || award.total_saturated {
            return Err(ScoreOverflow {
                chain: award.chain_saturated,
                total: award.total_saturated,
            });
        }
        self.apply_award(award);
        Ok(Some(award))
    }
    fn apply_award(&mut self, award: ScoreAward) {
        self.chain = award.chain;
        self.best = self.best.max(self.chain);
        self.remaining = self.window;
        self.total = award.total_after;
    }
    pub fn multiplier(&self) -> u32 {
        (1 + self.chain.saturating_sub(1) / self.per_tier).min(self.maximum)
    }
    pub fn chain(&self) -> u32 {
        self.chain
    }
    pub fn best(&self) -> u32 {
        self.best
    }
    pub fn total(&self) -> u64 {
        self.total
    }
    /// Remaining simulation seconds for an active chain, at internal precision.
    /// Never advances or expires the chain, even at/below epsilon. `None` means
    /// no active chain. See the module example.
    pub fn remaining_life(&self) -> Option<f64> {
        (self.chain != 0).then_some(self.remaining)
    }
    /// Apply the explicit clearing/retention policy, retaining authoring settings.
    /// See [`ChainReset`] and the module example for each mode's fields.
    pub fn reset(&mut self, mode: ChainReset) {
        self.chain = 0;
        self.remaining = 0.;
        if mode != ChainReset::ChainOnly {
            self.total = 0;
        }
        if mode == ChainReset::All {
            self.best = 0;
        }
    }
    /// Legacy break: clear chain and remaining time, retain total, best and settings.
    pub fn break_chain(&mut self) {
        self.reset(ChainReset::ChainOnly);
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn state(c: &ScoreChain) -> (f64, f64, u32, u32, u32, u64, u32) {
        (
            c.window,
            c.remaining,
            c.chain,
            c.per_tier,
            c.maximum,
            c.total,
            c.best,
        )
    }

    #[test]
    fn actions_build_capped_multipliers_and_timeout_preserves_total() {
        let mut c = ScoreChain::new(2., 2, 3).unwrap();
        let awards: Vec<_> = (0..7).map(|_| c.award(100)).collect();
        assert_eq!(awards, [100, 100, 200, 200, 300, 300, 300]);
        assert_eq!(c.total(), 1500);
        c.advance(1.).unwrap();
        c.award(0);
        c.advance(1.).unwrap();
        assert_eq!((c.chain(), c.best(), c.multiplier()), (0, 7, 1));
        assert_eq!(c.award(100), 100);
        assert!(c.advance(-1.).is_err());
        assert_eq!(c.chain(), 1);
    }

    #[test]
    fn receipts_follow_post_action_tiers_and_cap_without_preview_mutation() {
        let mut c = ScoreChain::new(2., 2, 3).unwrap();
        for (i, multiplier) in [1, 1, 2, 2, 3, 3, 3, 3, 3].into_iter().enumerate() {
            let before = state(&c);
            let preview = c.preview_award(100).unwrap();
            assert_eq!(state(&c), before);
            assert_eq!(preview.base, 100);
            assert_eq!(preview.chain, i as u32 + 1);
            assert_eq!(preview.tier, 1 + i as u32 / 2);
            assert_eq!(preview.multiplier, multiplier);
            assert_eq!(preview.nominal_points, 100 * u64::from(multiplier));
            assert_eq!(preview.total_change, preview.nominal_points);
            assert_eq!(preview.total_before, before.5);
            assert_eq!(preview.total_after, before.5 + preview.nominal_points);
            assert!(!preview.chain_saturated && !preview.total_saturated);
            assert_eq!(c.try_award(100), Ok(Some(preview)));
            assert_eq!(c.total(), preview.total_after);
            assert_eq!(c.multiplier(), multiplier);
            assert_eq!(c.best(), preview.chain);
            assert_eq!(c.remaining_life(), Some(2.));
        }
        // Constructor/cap bounds and the widest authored tier size.
        let mut widest = ScoreChain::new(3600., u32::MAX, 100).unwrap();
        widest.chain = u32::MAX - 1;
        let last = widest.try_award(u32::MAX).unwrap().unwrap();
        assert_eq!((last.chain, last.tier, last.multiplier), (u32::MAX, 1, 1));
        assert!(!last.chain_saturated);
        for (window, per_tier, maximum) in [
            (0., 1, 1),
            (1., 0, 1),
            (1., 1, 0),
            (1., 1, 101),
            (f32::NAN, 1, 1),
        ] {
            assert!(matches!(
                ScoreChain::new(window, per_tier, maximum),
                Err(LoopError::Invalid)
            ));
        }
    }

    #[test]
    fn zero_and_inspection_never_refresh_even_at_counter_bounds() {
        for bounded in [false, true] {
            let mut c = ScoreChain::new(0.125, 1, 100).unwrap();
            c.award(1);
            c.advance(0.03125).unwrap();
            if bounded {
                c.chain = u32::MAX;
                c.best = u32::MAX;
                c.total = u64::MAX;
            }
            let before = state(&c);
            for _ in 0..3 {
                assert_eq!(c.preview_award(0), None);
                assert_eq!(c.award_with_receipt(0), None);
                assert_eq!(c.try_award(0), Ok(None));
                assert_eq!(c.award(0), 0);
                assert_eq!(c.remaining_life(), Some(0.09375));
                let _ = (
                    c.preview_award(1),
                    c.multiplier(),
                    c.chain(),
                    c.total(),
                    c.best(),
                );
                assert_eq!(state(&c), before);
            }
            assert_eq!(
                c.advance_with_outcome(0.09375),
                Ok(ChainAdvance::Expired { chain: before.2 })
            );
            assert_eq!((c.total(), c.best()), (before.5, before.6));
            assert_eq!(c.remaining_life(), None);
            assert_eq!(c.advance_with_outcome(0.), Ok(ChainAdvance::Idle));
        }
    }

    #[test]
    fn expiry_is_inclusive_at_exact_epsilon_and_f32_neighbors() {
        let below = 1e-9_f32;
        let above = f32::from_bits(below.to_bits() + 1);
        assert!(f64::from(below) < 1e-9);
        assert!(f64::from(above) > 1e-9);
        for window in [f32::from_bits(1), below] {
            let mut c = ScoreChain::new(window, 2, 3).unwrap();
            c.award(1);
            assert_eq!(c.remaining_life(), Some(f64::from(window)));
            assert_eq!(c.chain(), 1); // Queries do not expire tiny windows.
            assert_eq!(
                c.advance_with_outcome(0.),
                Ok(ChainAdvance::Expired { chain: 1 })
            );
        }
        let mut c = ScoreChain::new(above, 2, 3).unwrap();
        c.award(1);
        assert_eq!(c.advance_with_outcome(0.), Ok(ChainAdvance::Active));
        assert_eq!(
            c.advance_with_outcome(below),
            Ok(ChainAdvance::Expired { chain: 1 })
        );
        c.award(1);
        // Internal fixture pins the exact f64 boundary, independent of f32 rounding.
        c.remaining = 1e-9;
        let before = state(&c);
        assert_eq!(c.remaining_life(), Some(1e-9));
        c.preview_award(1).unwrap();
        assert_eq!(state(&c), before);
        assert_eq!(
            c.advance_with_outcome(-0.),
            Ok(ChainAdvance::Expired { chain: 1 })
        );
        c.award(1);
        c.remaining = f64::from_bits(1e-9_f64.to_bits() + 1);
        assert_eq!(c.advance_with_outcome(0.), Ok(ChainAdvance::Active));
        assert_eq!(
            c.advance_with_outcome(3600.),
            Ok(ChainAdvance::Expired { chain: 1 })
        );
        assert_eq!(c.advance_with_outcome(3600.), Ok(ChainAdvance::Idle));
    }

    #[test]
    fn invalid_delta_leaves_every_field_unchanged_active_or_idle() {
        let mut c = ScoreChain::new(3600., 3, 4).unwrap();
        for active in [false, true] {
            if active {
                c.award(100);
                c.advance(0.25).unwrap();
            }
            let before = state(&c);
            for dt in [
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                -0.001,
                f32::from_bits(3600_f32.to_bits() + 1),
            ] {
                assert_eq!(c.advance_with_outcome(dt), Err(LoopError::Invalid));
                assert_eq!(state(&c), before);
                assert_eq!(c.advance(dt), Err(LoopError::Invalid));
                assert_eq!(state(&c), before);
            }
        }
    }

    #[test]
    fn total_boundary_distinguishes_exact_fit_rejection_and_explicit_saturation() {
        let mut c = ScoreChain::new(2., 1, 100).unwrap();
        c.chain = 99;
        c.best = 99;
        let nominal = u64::from(u32::MAX) * 100;
        c.total = u64::MAX - nominal;
        let exact = c.try_award(u32::MAX).unwrap().unwrap();
        assert_eq!((exact.tier, exact.multiplier), (100, 100));
        assert_eq!(
            (exact.nominal_points, exact.total_change),
            (nominal, nominal)
        );
        assert_eq!(exact.total_after, u64::MAX);
        assert!(!exact.total_saturated);

        // One more accepted legacy award changes no total, but refreshes timing.
        c.advance(1.).unwrap();
        let before = state(&c);
        assert_eq!(
            c.try_award(1),
            Err(ScoreOverflow {
                chain: false,
                total: true
            })
        );
        assert_eq!(state(&c), before);
        let receipt = c.award_with_receipt(1).unwrap();
        assert_eq!((receipt.nominal_points, receipt.total_change), (100, 0));
        assert!(receipt.total_saturated && !receipt.chain_saturated);
        assert_eq!(
            (c.chain(), c.best(), c.remaining_life()),
            (101, 101, Some(2.))
        );

        c.total = u64::MAX - 25;
        let before = state(&c);
        let preview = c.preview_award(u32::MAX).unwrap();
        assert_eq!(state(&c), before);
        assert_eq!(
            c.try_award(u32::MAX),
            Err(ScoreOverflow {
                chain: false,
                total: true
            })
        );
        assert_eq!(state(&c), before);
        assert_eq!(c.award_with_receipt(u32::MAX), Some(preview));
        assert_eq!(
            (preview.nominal_points, preview.total_change),
            (nominal, 25)
        );
        assert!(preview.total_saturated);
    }

    #[test]
    fn chain_boundary_and_simultaneous_overflows_are_explicit_and_atomic() {
        let mut c = ScoreChain::new(2., 1, 100).unwrap();
        c.chain = u32::MAX - 1;
        c.best = u32::MAX - 1;
        let exact = c.try_award(1).unwrap().unwrap();
        assert_eq!(
            (exact.chain, exact.tier, exact.multiplier),
            (u32::MAX, u32::MAX, 100)
        );
        assert!(!exact.chain_saturated);
        c.advance(1.).unwrap();
        for (total, total_overflow) in [(c.total(), false), (u64::MAX, true)] {
            c.total = total;
            let before = state(&c);
            let preview = c.preview_award(1).unwrap();
            assert_eq!(state(&c), before);
            assert_eq!(
                c.try_award(1),
                Err(ScoreOverflow {
                    chain: true,
                    total: total_overflow
                })
            );
            assert_eq!(state(&c), before);
            assert_eq!(c.award_with_receipt(1), Some(preview));
            assert!(preview.chain_saturated);
            assert_eq!(preview.total_saturated, total_overflow);
            assert_eq!(preview.total_change, if total_overflow { 0 } else { 100 });
            assert_eq!(
                (c.chain(), c.best(), c.remaining_life()),
                (u32::MAX, u32::MAX, Some(2.))
            );
        }
    }

    #[test]
    fn each_reset_mode_is_idempotent_and_retains_authoring_configuration() {
        let mut initial = ScoreChain::new(0.125, 2, 3).unwrap();
        for _ in 0..9 {
            initial.award(10);
        }
        initial.advance(0.03125).unwrap();
        for (mode, total, best) in [
            (ChainReset::ChainOnly, initial.total(), 9),
            (ChainReset::Run, 0, 9),
            (ChainReset::All, 0, 0),
        ] {
            let mut c = initial;
            for _ in 0..2 {
                c.reset(mode);
                assert_eq!(state(&c), (0.125, 0., 0, 2, 3, total, best));
                assert_eq!(c.multiplier(), 1);
                assert_eq!(c.remaining_life(), None);
                assert_eq!(c.advance_with_outcome(0.), Ok(ChainAdvance::Idle));
            }
            let receipts: Vec<_> = (0..7).map(|_| c.award_with_receipt(10).unwrap()).collect();
            assert_eq!(
                receipts.iter().map(|r| r.multiplier).collect::<Vec<_>>(),
                [1, 1, 2, 2, 3, 3, 3]
            );
            assert_eq!(c.remaining_life(), Some(0.125));
            assert_eq!(c.total(), total + 150);
            assert_eq!(c.best(), best.max(7));
        }
        let mut broken = initial;
        broken.break_chain();
        initial.reset(ChainReset::ChainOnly);
        assert_eq!(state(&broken), state(&initial));
    }

    #[test]
    fn legacy_wrappers_match_original_policy_and_receipt_path_in_all_fields() {
        // Independent copy of the original formulas detects a shared wrapper bug.
        fn original_award(c: &mut ScoreChain, base: u32) -> u64 {
            if base == 0 {
                return 0;
            }
            c.chain = c.chain.saturating_add(1);
            c.best = c.best.max(c.chain);
            c.remaining = c.window;
            let multiplier = (1 + c.chain.saturating_sub(1) / c.per_tier).min(c.maximum);
            let points = u64::from(base) * u64::from(multiplier);
            c.total = c.total.saturating_add(points);
            points
        }
        for (chain, total) in [(0, 0), (u32::MAX - 1, u64::MAX - 1), (u32::MAX, u64::MAX)] {
            let mut seed = ScoreChain::new(0.125, 2, 3).unwrap();
            seed.chain = chain;
            seed.best = chain;
            seed.total = total;
            for base in [0, 100, 100, u32::MAX, 0, 1, 10, 100] {
                let mut original = seed;
                let mut legacy = seed;
                let mut receipt = seed;
                let expected = original_award(&mut original, base);
                assert_eq!(legacy.award(base), expected);
                assert_eq!(
                    receipt
                        .award_with_receipt(base)
                        .map_or(0, |r| r.nominal_points),
                    expected
                );
                assert_eq!(state(&legacy), state(&original));
                assert_eq!(state(&receipt), state(&original));
                for dt in [0., 0.03125, 0.09375, 1e-9_f32, 3600.] {
                    original.remaining = (original.remaining - f64::from(dt)).max(0.);
                    if original.remaining <= 1e-9 {
                        original.chain = 0;
                        original.remaining = 0.;
                    }
                    assert_eq!(legacy.advance(dt), Ok(()));
                    receipt.advance_with_outcome(dt).unwrap();
                    assert_eq!(state(&legacy), state(&original));
                    assert_eq!(state(&receipt), state(&original));
                }
            }
        }
    }
}
