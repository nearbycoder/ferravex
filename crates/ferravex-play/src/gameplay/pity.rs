//! Bound unlucky reward droughts without reroll loops or hidden probability changes.
//!
//! [`PityCounter`] retains its immediate, `Copy` behavior. [`PityDelivery`] adds
//! one outstanding reward offer, without changing the legacy counter's storage.
//! Neither API owns or consumes randomness. Callers authorize and deduplicate
//! gameplay events before admission; offer IDs do not authenticate those events.
use super::LoopError;

/// Admission of one gameplay event. Skipped events never advance the drought.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PityAttempt {
    Ineligible,
    Deferred,
    Accepted { natural_success: bool },
}

/// Immediate accepted-attempt result. Natural success wins at the boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PityOutcome {
    Ineligible,
    Deferred,
    Miss,
    Natural,
    Guaranteed,
}

#[derive(Clone, Copy, Debug)]
pub struct PityCounter {
    limit: u16,
    misses: u16,
}
impl PityCounter {
    /// At least one reward in each `limit` accepted attempts, including the first.
    pub fn new(limit: u16) -> Result<Self, LoopError> {
        if limit == 0 {
            return Err(LoopError::Invalid);
        }
        Ok(Self { limit, misses: 0 })
    }
    /// Call only after the host confirms an eligible event. Successful natural
    /// rolls reset the same counter. The returned reward must be applied once.
    pub fn attempt(&mut self, natural_success: bool) -> bool {
        if natural_success || self.misses >= self.limit - 1 {
            self.misses = 0;
            true
        } else {
            self.misses += 1;
            false
        }
    }
    /// Inspect the reason while retaining immediate legacy reward commitment.
    /// Use [`PityDelivery`] if destination insertion can fail.
    ///
    /// ```
    /// use ferravex_play::gameplay::pity::{PityAttempt, PityCounter, PityOutcome};
    /// let mut pity = PityCounter::new(1).unwrap();
    /// assert_eq!(pity.attempt_outcome(PityAttempt::Deferred), PityOutcome::Deferred);
    /// assert_eq!(pity.attempt_outcome(PityAttempt::Accepted { natural_success: true }),
    ///            PityOutcome::Natural);
    /// ```
    pub fn attempt_outcome(&mut self, attempt: PityAttempt) -> PityOutcome {
        let outcome = self.outcome(attempt);
        match outcome {
            PityOutcome::Miss => self.misses += 1,
            PityOutcome::Natural | PityOutcome::Guaranteed => self.misses = 0,
            PityOutcome::Ineligible | PityOutcome::Deferred => {}
        }
        outcome
    }

    fn outcome(&self, attempt: PityAttempt) -> PityOutcome {
        match attempt {
            PityAttempt::Ineligible => PityOutcome::Ineligible,
            PityAttempt::Deferred => PityOutcome::Deferred,
            PityAttempt::Accepted {
                natural_success: true,
            } => PityOutcome::Natural,
            PityAttempt::Accepted {
                natural_success: false,
            } if self.misses >= self.limit - 1 => PityOutcome::Guaranteed,
            PityAttempt::Accepted { .. } => PityOutcome::Miss,
        }
    }
    pub fn misses(&self) -> u16 {
        self.misses
    }
    pub fn reset(&mut self) {
        self.misses = 0;
    }
}

/// Locally issued token: a caller-owned nonzero stream and nonzero sequence.
/// Fields cannot be constructed directly. Tokens are not security credentials.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PityOfferId {
    stream: u64,
    sequence: u64,
}
impl PityOfferId {
    /// Caller-supplied stream namespace, not an automatically unique identity.
    pub fn stream(self) -> u64 {
        self.stream
    }
    /// Monotonic reward-offer sequence in this delivery instance.
    pub fn sequence(self) -> u64 {
        self.sequence
    }
}

/// A reward owed, retaining its original natural/guaranteed reason on retry.
/// The host must retain the corresponding reward payload until settlement/reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PityOffer {
    id: PityOfferId,
    outcome: PityOutcome,
}
impl PityOffer {
    pub fn id(self) -> PityOfferId {
        self.id
    }
    /// Always [`PityOutcome::Natural`] or [`PityOutcome::Guaranteed`].
    pub fn outcome(self) -> PityOutcome {
        self.outcome
    }
}

/// Admission result; a reward is only offered, not yet delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PityOfferOutcome {
    Ineligible,
    Deferred,
    Miss,
    Reward(PityOffer),
}

/// Destination refusal retains the exact offer; acceptance commits once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PitySettlement {
    Pending(PityOffer),
    Delivered(PityOffer),
}

/// Rejections leave the delivery instance unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PityDeliveryError {
    InvalidStream,
    Pending,
    IdExhausted,
    StaleOffer,
}

/// Allocation-free delivery with exactly one pending offer and no history list.
///
/// Accepted misses commit immediately. Reward offers freeze the counter until
/// accepted delivery; refusal preserves both counter and offer. While pending,
/// new accepted attempts return `Pending` (even natural successes); ineligible
/// and deferred observations remain harmless. Retry via [`Self::pending`] and
/// [`Self::settle`], never by admitting the gameplay event or rolling again.
/// There is no individual cancellation: [`Self::reset`] explicitly abandons the
/// old run's debt. Payload storage and its cancellation remain caller-owned.
///
/// This type deliberately implements neither `Copy` nor `Clone`. The host must
/// allocate a distinct nonzero stream namespace for every independently routed
/// instance/lifetime, including replacements. Reusing a namespace in a new
/// instance can alias old tokens. Copying a legacy counter creates no unique
/// identity. IDs are local correlation only, with no save/wire/replay encoding.
///
/// Sequences 1 through `u64::MAX` are issued once per reward offer. After the last
/// offer, all further accepted admissions fail closed with `IdExhausted`, even
/// after reset; pending settlement still works. Reset never reuses a sequence.
///
/// Runnable first-party recipe: a confirmed, deduplicated projectile hit admits
/// a reward into an existing bounded inventory. No reward feedback precedes
/// destination acceptance, and capacity retry consumes no additional RNG.
///
/// ```
/// use ferravex_play::gameplay::{
///     inventory::{Inventory, Stack},
///     loot::{RewardRng, RewardTable},
///     pity::{PityAttempt, PityCounter, PityDelivery, PityOfferOutcome, PitySettlement},
///     projectile::{HitResult, ProjectileBudget},
/// };
/// fn main() {
///     let mut hits = ProjectileBudget::<1>::new(0, 0).unwrap();
///     let mut bag = Inventory::<1>::new().unwrap();
///     bag.add(Stack { item: 99, amount: 1, limit: 1 }).unwrap();
///     let mut pity = PityDelivery::new(PityCounter::new(1).unwrap(), 7).unwrap();
///     let mut rng = RewardRng::new(42);
///     let table = RewardTable::new([1, 0]).unwrap();
///     let mut feedback = 0;
///     // The authoritative host has confirmed collision with target 101.
///     assert_eq!(hits.target(101), HitResult::Stop);
///     let natural_success = table.roll(&mut rng) == 1; // One caller-owned roll.
///     let PityOfferOutcome::Reward(offer) = pity.offer(
///         PityAttempt::Accepted { natural_success }).unwrap() else { panic!("limit one") };
///     let reward = Stack { item: 5, amount: 1, limit: 1 }; // Retain with offer.
///     let inserted = bag.add(reward).is_ok();
///     assert!(!inserted);
///     assert_eq!(pity.settle(offer.id(), inserted).unwrap(), PitySettlement::Pending(offer));
///     assert_eq!(feedback, 0);
///     assert_eq!(hits.target(101), HitResult::Ignored); // No second admission/roll.
///     bag.remove(99, 1).unwrap(); // An actual inventory edit frees capacity.
///     if let Some(retry) = pity.pending() {
///         let inserted = bag.add(reward).is_ok();
///         if let PitySettlement::Delivered(_) = pity.settle(retry.id(), inserted).unwrap() {
///             feedback += 1; // Publish only this accepted reward event.
///         }
///     }
///     assert!(pity.pending().is_none());
///     assert_eq!((bag.count(5), feedback), (1, 1));
/// }
/// ```
///
/// Orbit migration handoff: `hit_block` already rolls once on destruction before
/// its `drops.len() < 8` guard. Keep that roll and guard in that order; full
/// capacity is `Deferred`, without counter advancement or a new owed reward.
/// At available capacity, admit the same natural boolean, insert the capsule,
/// then settle and publish reward feedback. Retain any unexpectedly refused
/// offer/payload for retry before admitting another reward attempt. Do not
/// replace the guard with accepted offers while full without an explicit rules
/// version change: that would create new debt and change the old sequence.
#[derive(Debug)]
pub struct PityDelivery {
    counter: PityCounter,
    stream: u64,
    next_sequence: u64, // Zero is exhausted, never issued.
    pending: Option<PityOffer>,
}
impl PityDelivery {
    /// Move an existing counter into a delivery instance. Stream zero is invalid.
    pub fn new(counter: PityCounter, stream: u64) -> Result<Self, PityDeliveryError> {
        if stream == 0 {
            return Err(PityDeliveryError::InvalidStream);
        }
        Ok(Self {
            counter,
            stream,
            next_sequence: 1,
            pending: None,
        })
    }

    /// Admit once per accepted gameplay event, using its already-consumed roll.
    /// No state changes on errors, ineligibility or deferral. See the type-level
    /// recipe for delivery and the stream provenance/overflow contract.
    pub fn offer(&mut self, attempt: PityAttempt) -> Result<PityOfferOutcome, PityDeliveryError> {
        let outcome = self.counter.outcome(attempt);
        match outcome {
            PityOutcome::Ineligible => return Ok(PityOfferOutcome::Ineligible),
            PityOutcome::Deferred => return Ok(PityOfferOutcome::Deferred),
            _ => {}
        }
        if self.pending.is_some() {
            return Err(PityDeliveryError::Pending);
        }
        if self.next_sequence == 0 {
            return Err(PityDeliveryError::IdExhausted);
        }
        if outcome == PityOutcome::Miss {
            self.counter.misses += 1;
            return Ok(PityOfferOutcome::Miss);
        }
        let offer = PityOffer {
            id: PityOfferId {
                stream: self.stream,
                sequence: self.next_sequence,
            },
            outcome,
        };
        self.next_sequence = self.next_sequence.checked_add(1).unwrap_or(0);
        self.pending = Some(offer);
        Ok(PityOfferOutcome::Reward(offer))
    }

    /// Read the sole outstanding offer for delivery/retry. Does not reroll.
    pub fn pending(&self) -> Option<PityOffer> {
        self.pending
    }

    /// Record the destination's actual insertion result for the current offer.
    /// `false` must mean no delivery happened; `true` must mean exactly one
    /// delivery happened. This method does not call or roll back external code.
    ///
    /// The host must validate `pending()` before attempting insertion, keep
    /// exclusive ownership through insertion and settlement, and settle before
    /// publishing feedback. Stale, duplicate and foreign-stream tokens return
    /// `StaleOffer` without changing state. This rejects duplicate *settlement*,
    /// not a repeated external insertion performed before validation. Panic,
    /// crash recovery, dishonest results and other external effects are the
    /// caller's responsibility; there is no durable exactly-once guarantee.
    pub fn settle(
        &mut self,
        id: PityOfferId,
        destination_accepted: bool,
    ) -> Result<PitySettlement, PityDeliveryError> {
        let offer = self
            .pending
            .filter(|offer| offer.id == id)
            .ok_or(PityDeliveryError::StaleOffer)?;
        if !destination_accepted {
            return Ok(PitySettlement::Pending(offer));
        }
        self.counter.reset();
        self.pending = None;
        Ok(PitySettlement::Delivered(offer))
    }

    /// Committed misses only; an outstanding reward has not reset them yet.
    pub fn misses(&self) -> u16 {
        self.counter.misses()
    }

    /// Begin a fresh drought and explicitly abandon/return the old pending offer.
    /// Old IDs become stale. Stream, sequence and exhaustion are retained.
    /// The host must discard the associated payload and suppress its feedback.
    pub fn reset(&mut self) -> Option<PityOffer> {
        self.counter.reset();
        self.pending.take()
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn accepted(natural_success: bool) -> PityAttempt {
        PityAttempt::Accepted { natural_success }
    }

    fn reward(delivery: &mut PityDelivery, natural: bool) -> PityOffer {
        let PityOfferOutcome::Reward(offer) = delivery.offer(accepted(natural)).unwrap() else {
            panic!("expected reward")
        };
        offer
    }
    #[test]
    fn drought_is_bounded_and_natural_rewards_reset_it() {
        let mut p = PityCounter::new(3).unwrap();
        assert!(!p.attempt(false));
        assert!(!p.attempt(false));
        assert!(p.attempt(false));
        assert_eq!(p.misses(), 0);
        assert!(!p.attempt(false));
        assert!(p.attempt(true));
        assert!(!p.attempt(false));
        assert!(PityCounter::new(1).unwrap().attempt(false));
        assert!(PityCounter::new(0).is_err());
    }

    #[test]
    fn exact_legacy_sequence_and_copy_storage_are_preserved() {
        fn require_copy<T: Copy>() {}
        require_copy::<PityCounter>();
        assert_eq!(
            std::mem::size_of::<PityCounter>(),
            2 * std::mem::size_of::<u16>()
        );
        for limit in 1..=5 {
            for pattern in 0_u16..1024 {
                let mut legacy = PityCounter::new(limit).unwrap();
                let mut detailed = legacy;
                let mut expected_misses = 0;
                for bit in 0..10 {
                    let natural = pattern & (1 << bit) != 0;
                    let expected = natural || expected_misses >= limit - 1;
                    let outcome = if natural {
                        PityOutcome::Natural
                    } else if expected {
                        PityOutcome::Guaranteed
                    } else {
                        PityOutcome::Miss
                    };
                    expected_misses = if expected { 0 } else { expected_misses + 1 };
                    assert_eq!(legacy.attempt(natural), expected);
                    assert_eq!(detailed.attempt_outcome(accepted(natural)), outcome);
                    assert_eq!(
                        (legacy.misses(), detailed.misses()),
                        (expected_misses, expected_misses)
                    );
                }
                legacy.reset();
                assert_eq!(legacy.misses(), 0);
            }
        }
    }

    #[test]
    fn skips_and_natural_boundary_precedence_include_limit_one() {
        for limit in [1, 3, u16::MAX] {
            let mut counter = PityCounter::new(limit).unwrap();
            for _ in 1..limit {
                assert_eq!(counter.attempt_outcome(accepted(false)), PityOutcome::Miss);
            }
            let misses = counter.misses();
            assert_eq!(
                counter.attempt_outcome(PityAttempt::Ineligible),
                PityOutcome::Ineligible
            );
            assert_eq!(
                counter.attempt_outcome(PityAttempt::Deferred),
                PityOutcome::Deferred
            );
            assert_eq!(counter.misses(), misses);
            let mut delivery = PityDelivery::new(counter, 1).unwrap();
            let offer = reward(&mut delivery, true);
            assert_eq!(offer.outcome(), PityOutcome::Natural);
            assert_eq!(delivery.misses(), misses);
            assert_eq!(
                delivery.settle(offer.id(), false),
                Ok(PitySettlement::Pending(offer))
            );
            assert_eq!(delivery.pending(), Some(offer));
            assert_eq!(
                delivery.settle(offer.id(), true),
                Ok(PitySettlement::Delivered(offer))
            );
            assert_eq!(delivery.misses(), 0);
        }
        let mut delivery = PityDelivery::new(PityCounter::new(1).unwrap(), 1).unwrap();
        assert_eq!(
            reward(&mut delivery, false).outcome(),
            PityOutcome::Guaranteed
        );
    }

    #[test]
    fn guarantee_remains_owed_and_conflicts_cannot_replace_it() {
        let mut delivery = PityDelivery::new(PityCounter::new(3).unwrap(), 8).unwrap();
        for _ in 0..2 {
            assert_eq!(delivery.offer(accepted(false)), Ok(PityOfferOutcome::Miss));
        }
        let offer = reward(&mut delivery, false);
        assert_eq!((offer.id().stream(), offer.id().sequence()), (8, 1));
        assert_eq!(offer.outcome(), PityOutcome::Guaranteed);
        for _ in 0..3 {
            assert_eq!(
                delivery.settle(offer.id(), false),
                Ok(PitySettlement::Pending(offer))
            );
            assert_eq!(
                delivery.offer(accepted(false)),
                Err(PityDeliveryError::Pending)
            );
            assert_eq!(
                delivery.offer(accepted(true)),
                Err(PityDeliveryError::Pending)
            );
            assert_eq!(
                delivery.offer(PityAttempt::Deferred),
                Ok(PityOfferOutcome::Deferred)
            );
            assert_eq!(
                delivery.offer(PityAttempt::Ineligible),
                Ok(PityOfferOutcome::Ineligible)
            );
            assert_eq!(delivery.misses(), 2);
            assert_eq!(delivery.pending(), Some(offer));
        }
        assert_eq!(
            delivery.settle(offer.id(), true),
            Ok(PitySettlement::Delivered(offer))
        );
        for accepted in [false, true] {
            assert_eq!(
                delivery.settle(offer.id(), accepted),
                Err(PityDeliveryError::StaleOffer)
            );
        }
        assert_eq!(delivery.pending(), None);
        assert_eq!(delivery.misses(), 0);
        assert_eq!(delivery.offer(accepted(false)), Ok(PityOfferOutcome::Miss));
    }

    #[test]
    fn reset_and_foreign_ids_cannot_settle_a_new_offer() {
        let counter = PityCounter::new(1).unwrap();
        let mut delivery = PityDelivery::new(counter, 5).unwrap();
        let mut foreign = PityDelivery::new(counter, 6).unwrap();
        let old = reward(&mut delivery, false);
        let other = reward(&mut foreign, false);
        assert_eq!(
            delivery.settle(other.id(), true),
            Err(PityDeliveryError::StaleOffer)
        );
        assert_eq!(delivery.reset(), Some(old));
        assert_eq!(delivery.reset(), None);
        assert_eq!(
            delivery.settle(old.id(), true),
            Err(PityDeliveryError::StaleOffer)
        );
        let new = reward(&mut delivery, true);
        assert_eq!(new.id().sequence(), 2);
        assert_eq!(
            delivery.settle(old.id(), false),
            Err(PityDeliveryError::StaleOffer)
        );
        assert_eq!(delivery.pending(), Some(new));
        assert_eq!(delivery.misses(), 0);
        assert_eq!(
            delivery.settle(new.id(), true),
            Ok(PitySettlement::Delivered(new))
        );
    }

    #[test]
    fn invalid_stream_and_exhaustion_never_wrap_or_reset_ids() {
        let counter = PityCounter::new(2).unwrap();
        assert!(matches!(
            PityDelivery::new(counter, 0),
            Err(PityDeliveryError::InvalidStream)
        ));
        let mut delivery = PityDelivery::new(counter, u64::MAX).unwrap();
        delivery.next_sequence = u64::MAX; // Reach a bounded edge without 2^64 calls.
        assert_eq!(delivery.offer(accepted(false)), Ok(PityOfferOutcome::Miss));
        let last = reward(&mut delivery, false);
        assert_eq!(last.id().sequence(), u64::MAX);
        assert_eq!(delivery.next_sequence, 0);
        assert_eq!(
            delivery.settle(last.id(), false),
            Ok(PitySettlement::Pending(last))
        );
        assert_eq!(
            delivery.settle(last.id(), true),
            Ok(PitySettlement::Delivered(last))
        );
        assert_eq!(
            delivery.offer(accepted(false)),
            Err(PityDeliveryError::IdExhausted)
        );
        assert_eq!(
            delivery.offer(accepted(true)),
            Err(PityDeliveryError::IdExhausted)
        );
        assert_eq!(delivery.misses(), 0);
        assert_eq!(delivery.reset(), None);
        assert_eq!(delivery.next_sequence, 0);
        assert_eq!(
            delivery.offer(accepted(false)),
            Err(PityDeliveryError::IdExhausted)
        );
        assert_eq!(
            delivery.offer(PityAttempt::Deferred),
            Ok(PityOfferOutcome::Deferred)
        );
        assert_eq!(
            delivery.offer(PityAttempt::Ineligible),
            Ok(PityOfferOutcome::Ineligible)
        );
        let invalid = PityOfferId {
            stream: 0,
            sequence: 0,
        };
        assert_eq!(
            delivery.settle(invalid, true),
            Err(PityDeliveryError::StaleOffer)
        );
    }
}
