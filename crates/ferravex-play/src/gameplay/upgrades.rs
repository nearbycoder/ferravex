//! Capped upgrade ranks and distinct seeded offers. The host defines rank effects.
//!
//! [`Upgrades`] retains the original host-managed spending policy.
//! [`UpgradeChoices`] optionally owns ranks, earned points and one cached offer
//! together. Existing games, including Neon's automatic upgrade, are unchanged.
use super::{LoopError, loot::RewardRng};
#[derive(Clone, Copy, Debug)]
pub struct Upgrades<const N: usize> {
    ranks: [u8; N],
    caps: [u8; N],
}
/// Identity within one caller-scoped choice-session namespace.
///
/// Only a session can issue tokens. Copying a token/snapshot does not reserve a
/// point or make it reusable after acceptance, abandonment or reset. Namespaces
/// are caller-assigned nonzero values, not global uniqueness or security: two
/// sessions created with the same namespace can issue equal tokens. Use a fresh
/// namespace whenever recreating a session whose old callbacks may survive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpgradeOfferId {
    namespace: u64,
    serial: u64,
}

impl UpgradeOfferId {
    /// Caller-assigned namespace; see [`UpgradeChoices::new`].
    pub fn namespace(self) -> u64 {
        self.namespace
    }

    /// Monotonic serial in `1..=u64::MAX`, never reused by a live session.
    pub fn serial(self) -> u64 {
        self.serial
    }
}

/// Read-only fixed-storage offer snapshot. Only its session can accept it.
///
/// Choices are upgrade indices, with trailing `None` for a short eligible pool.
/// Keep the snapshot across redraws and submit its identity with the chosen ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpgradeOffer<const K: usize> {
    id: UpgradeOfferId,
    choices: [Option<usize>; K],
}

impl<const K: usize> UpgradeOffer<K> {
    /// Stable identity, including across repeated [`UpgradeChoices::open`] calls.
    pub fn id(&self) -> UpgradeOfferId {
        self.id
    }

    /// Borrow the fixed array without rerolling, allocating or consuming points.
    pub fn choices(&self) -> &[Option<usize>; K] {
        &self.choices
    }
}

/// Choice-session failures, each leaving ranks, points and pending offer intact.
///
/// These are additive; the legacy [`LoopError`] and upgrade APIs are unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpgradeChoiceError {
    /// Zero namespace, invalid upgrade state, or `K > N` at construction.
    Invalid,
    /// Explicit credit would exceed `u32::MAX`; no partial credit is admitted.
    BalanceOverflow,
    /// All `u64::MAX` offer serials were issued. Reset cannot rewind this limit.
    SerialExhausted,
    /// No pending offer, or the namespace/serial differs from its identity.
    StaleOffer,
    /// No earned point is available.
    NoPoints,
    /// The chosen upgrade index is absent from the pending offer.
    NotOffered,
    /// The offered upgrade is already at its cap.
    Capped,
}

/// Observation of one committed rank and one spent point, not a reward token.
///
/// Emit feedback only from this accepted result. Copying/replaying this receipt
/// grants no additional rank or credit; it has no persistence or wire format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub struct AcceptedUpgrade {
    /// Consumed offer identity; subsequent choices using it are stale.
    pub offer: UpgradeOfferId,
    /// Chosen upgrade index.
    pub upgrade: usize,
    /// Rank immediately before acceptance.
    pub rank_before: u8,
    /// Rank immediately after acceptance, exactly one higher.
    pub rank_after: u8,
    /// Earned balance immediately before acceptance.
    pub points_before: u32,
    /// Earned balance immediately after acceptance, exactly one lower.
    pub points_after: u32,
}

/// Discarded state returned by [`UpgradeChoices::reset`], never an award.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub struct UpgradeReset<const K: usize> {
    /// Points deliberately discarded by the reset.
    pub discarded_points: u32,
    /// Abandoned offer, if any, for host UI/callback cleanup.
    pub pending: Option<UpgradeOffer<K>>,
}

/// Optional bounded owner of ranks, earned points and one active choice set.
///
/// No `Clone`/`Copy`, mutable rank access, callbacks, event queue or storage I/O.
/// Acceptance commits solely owned state under `&mut self`; it is not an
/// external transaction. Snapshots and [`Self::upgrades`] are observations.
/// The host owns award authority/deduplication: credit only a newly accepted
/// award's promotions, never preview/restore/replayed reports. Check balance
/// capacity before committing an external award, or retain an explicit owed
/// award if credit fails; this adapter cannot roll back external progression.
///
/// Storage is `O(N + K)` with `1 <= N <= 256` and `0 <= K <= N`. Operations
/// allocate nothing. Opening delegates to the existing [`Upgrades::offers`]
/// (at most `N` eligibility visits and `K` draws); it adds no weighted algorithm.
/// Caller-supplied RNG, clocks and event IDs remain outside the session.
///
/// ```
/// use ferravex_play::gameplay::{
///     loot::RewardRng,
///     upgrades::{UpgradeChoices, UpgradeChoiceError, Upgrades},
/// };
/// let mut choices = UpgradeChoices::<3, 2>::new(Upgrades::new([2, 1, 1]).unwrap(), 7)
///     .unwrap();
/// let mut rng = RewardRng::new(42);
/// assert_eq!(choices.open(&mut rng).unwrap(), None); // No earned point.
/// assert_eq!(choices.credit(2).unwrap(), 2); // Host accepted a new award.
/// let offer = choices.open(&mut rng).unwrap().unwrap();
/// assert_eq!(choices.pending(), Some(&offer));
/// assert_eq!(choices.open(&mut rng).unwrap(), Some(offer)); // Redraw, no RNG.
/// assert_eq!((offer.id().namespace(), offer.id().serial()), (7, 1));
/// let id = offer.choices()[0].unwrap();
/// let accepted = choices.choose(offer.id(), id).unwrap();
/// assert_eq!((accepted.rank_before, accepted.rank_after), (0, 1));
/// assert_eq!((accepted.points_before, accepted.points_after), (2, 1));
/// assert_eq!(choices.upgrades().rank(id), Some(1));
/// assert_eq!(choices.choose(offer.id(), id), Err(UpgradeChoiceError::StaleOffer));
/// let next = choices.open(&mut rng).unwrap().unwrap();
/// assert_eq!(choices.abandon(), Some(next)); // Point retained, serial consumed.
/// let pending = choices.open(&mut rng).unwrap().unwrap();
/// let reset = choices.reset();
/// assert_eq!((reset.discarded_points, reset.pending), (1, Some(pending)));
/// assert_eq!((choices.points(), choices.upgrades().rank(id)), (0, Some(0)));
/// choices.credit(1).unwrap();
/// let fresh = choices.open(&mut rng).unwrap().unwrap();
/// assert!(fresh.id().serial() > pending.id().serial());
/// assert_eq!(choices.choose(pending.id(), id), Err(UpgradeChoiceError::StaleOffer));
/// ```
#[derive(Debug)]
pub struct UpgradeChoices<const N: usize, const K: usize> {
    upgrades: Upgrades<N>,
    points: u32,
    namespace: u64,
    next_serial: Option<u64>,
    pending: Option<UpgradeOffer<K>>,
}

impl<const N: usize, const K: usize> UpgradeChoices<N, K> {
    /// Own an existing valid rank set with zero points and serial 1 next.
    ///
    /// Validate `N`, caps/ranks, `K`, and a nonzero namespace before construction.
    /// A namespace must be fresh relative to any surviving callbacks from other
    /// sessions. Reusing it is safe only after all old tokens are unreachable.
    /// No global registry, authenticated token or save/replay format is implied.
    pub fn new(upgrades: Upgrades<N>, namespace: u64) -> Result<Self, UpgradeChoiceError> {
        if N == 0
            || N > 256
            || K > N
            || namespace == 0
            || upgrades.caps.contains(&0)
            || upgrades
                .ranks
                .iter()
                .zip(upgrades.caps)
                .any(|(&r, c)| r > c)
        {
            return Err(UpgradeChoiceError::Invalid);
        }
        Ok(Self {
            upgrades,
            points: 0,
            namespace,
            next_serial: Some(1),
            pending: None,
        })
    }

    /// Observe ranks; no mutable reference or live ownership copy is exposed.
    pub fn upgrades(&self) -> &Upgrades<N> {
        &self.upgrades
    }

    /// Unspent explicitly credited points.
    pub fn points(&self) -> u32 {
        self.points
    }

    /// Inspect the existing offer without touching RNG or balance.
    pub fn pending(&self) -> Option<&UpgradeOffer<K>> {
        self.pending.as_ref()
    }

    /// Explicit checked earned-point credit, returning the new total.
    ///
    /// Zero is a no-op; overflow changes nothing, including the offer. There is
    /// no award deduplication: repeated successful credits intentionally add
    /// again. Authority, replay/rollback consistency and event identity belong
    /// to the caller. Credit never rerolls or replaces a pending offer.
    pub fn credit(&mut self, points: u32) -> Result<u32, UpgradeChoiceError> {
        let total = self
            .points
            .checked_add(points)
            .ok_or(UpgradeChoiceError::BalanceOverflow)?;
        self.points = total;
        Ok(total)
    }

    /// Return the cached offer or generate one using [`Upgrades::offers`].
    ///
    /// Exact order: cached offer first; then zero points, `K == 0` or all ranks
    /// capped return `Ok(None)`; then check serial exhaustion; then select and
    /// publish. Every earlier return consumes zero RNG outputs and no serial.
    /// A new offer consumes exactly `min(K, eligible_count)` existing RNG
    /// outputs (including a sole eligible choice), and exactly one serial.
    /// Short offers have trailing `None`; empty offers are never published.
    /// Errors leave RNG and session intact. Constructor rejection of `K > N`
    /// occurs before any RNG can be supplied. Redraws must retain this offer;
    /// explicit [`Self::abandon`] permits a new draw on the next open.
    pub fn open(
        &mut self,
        rng: &mut RewardRng,
    ) -> Result<Option<UpgradeOffer<K>>, UpgradeChoiceError> {
        if let Some(offer) = self.pending {
            return Ok(Some(offer));
        }
        if self.points == 0 || K == 0 || self.upgrades.ranks == self.upgrades.caps {
            return Ok(None);
        }
        let serial = self
            .next_serial
            .ok_or(UpgradeChoiceError::SerialExhausted)?;
        let choices = self
            .upgrades
            .offers(rng)
            .map_err(|_| UpgradeChoiceError::Invalid)?;
        let offer = UpgradeOffer {
            id: UpgradeOfferId {
                namespace: self.namespace,
                serial,
            },
            choices,
        };
        self.next_serial = serial.checked_add(1);
        self.pending = Some(offer);
        Ok(Some(offer))
    }

    /// Accept an offered upgrade, spending one point and adding one rank.
    ///
    /// Check identity (including no offer), point balance, membership, then cap,
    /// in that order. Every rejection changes nothing. On acceptance the entire
    /// offer is consumed, even with points/other choices left. Repeated clicks,
    /// keypresses and delayed callbacks must submit the original token; they
    /// then fail stale, even after a new offer opens. No implicit next draw.
    /// Owned ranks cannot concurrently change through the public session API.
    pub fn choose(
        &mut self,
        offer: UpgradeOfferId,
        upgrade: usize,
    ) -> Result<AcceptedUpgrade, UpgradeChoiceError> {
        let pending = self
            .pending
            .as_ref()
            .filter(|pending| pending.id == offer)
            .ok_or(UpgradeChoiceError::StaleOffer)?;
        let points_after = self
            .points
            .checked_sub(1)
            .ok_or(UpgradeChoiceError::NoPoints)?;
        if !pending.choices.contains(&Some(upgrade)) {
            return Err(UpgradeChoiceError::NotOffered);
        }
        let rank_before = self
            .upgrades
            .rank(upgrade)
            .ok_or(UpgradeChoiceError::NotOffered)?;
        let rank_after = self
            .upgrades
            .apply(upgrade)
            .map_err(|_| UpgradeChoiceError::Capped)?;
        let accepted = AcceptedUpgrade {
            offer,
            upgrade,
            rank_before,
            rank_after,
            points_before: self.points,
            points_after,
        };
        self.points = points_after;
        self.pending = None;
        Ok(accepted)
    }

    /// Explicitly abandon the offer, returning it for host cleanup.
    ///
    /// No point is spent/refunded and the serial is never rewound. Any later
    /// open is a deliberate new draw; old callbacks are stale.
    pub fn abandon(&mut self) -> Option<UpgradeOffer<K>> {
        self.pending.take()
    }

    /// Reset all ranks and points, returning discarded points and pending offer.
    ///
    /// Retains caps, namespace and monotonic serial state, including exhaustion.
    /// It neither resets the caller's RNG nor credits a reward. A deterministic
    /// restart reseeds the caller RNG separately; reproduced choices still get
    /// fresh serials. Recreate with a fresh namespace to recover from exhaustion.
    pub fn reset(&mut self) -> UpgradeReset<K> {
        let reset = UpgradeReset {
            discarded_points: self.points,
            pending: self.pending.take(),
        };
        self.points = 0;
        self.upgrades.ranks = [0; N];
        reset
    }
}
impl<const N: usize> Upgrades<N> {
    pub fn new(caps: [u8; N]) -> Result<Self, LoopError> {
        if N == 0 || N > 256 || caps.contains(&0) {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            ranks: [0; N],
            caps,
        })
    }
    pub fn rank(&self, id: usize) -> Option<u8> {
        self.ranks.get(id).copied()
    }
    /// Only call after spending the host's earned upgrade point.
    pub fn apply(&mut self, id: usize) -> Result<u8, LoopError> {
        let cap = *self.caps.get(id).ok_or(LoopError::Invalid)?;
        if self.ranks[id] >= cap {
            return Err(LoopError::Unavailable);
        }
        self.ranks[id] += 1;
        Ok(self.ranks[id])
    }
    /// Fixed scratch, no duplicate or maxed offers. Fewer eligible choices leave
    /// trailing None entries. Persist chosen IDs, not RNG state, in run records.
    pub fn offers<const K: usize>(
        &self,
        rng: &mut RewardRng,
    ) -> Result<[Option<usize>; K], LoopError> {
        if K > N {
            return Err(LoopError::Invalid);
        }
        let mut eligible = [0; N];
        let mut count = 0;
        for id in 0..N {
            if self.ranks[id] < self.caps[id] {
                eligible[count] = id;
                count += 1;
            }
        }
        let mut offers = [None; K];
        for slot in &mut offers {
            if count == 0 {
                break;
            }
            let index = (rng.next_u64() % count as u64) as usize;
            *slot = Some(eligible[index]);
            count -= 1;
            eligible[index] = eligible[count];
        }
        Ok(offers)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offers_exclude_maxed_ranks_and_never_duplicate() {
        let mut u = Upgrades::new([1, 2, 1]).unwrap();
        u.apply(0).unwrap();
        let offers = u.offers::<3>(&mut RewardRng::new(7)).unwrap();
        assert_eq!(offers[2], None);
        assert_ne!(offers[0], offers[1]);
        assert!(!offers.contains(&Some(0)));
        assert_eq!(u.apply(0), Err(LoopError::Unavailable));
        u.apply(1).unwrap();
        u.apply(1).unwrap();
        u.apply(2).unwrap();
        assert_eq!(u.offers::<2>(&mut RewardRng::new(1)).unwrap(), [None, None]);
    }

    #[test]
    fn choice_constructor_rejects_invalid_private_state_and_configuration() {
        assert!(matches!(
            UpgradeChoices::<0, 0>::new(
                Upgrades {
                    ranks: [],
                    caps: []
                },
                1
            ),
            Err(UpgradeChoiceError::Invalid)
        ));
        assert!(matches!(
            UpgradeChoices::<257, 0>::new(
                Upgrades {
                    ranks: [0; 257],
                    caps: [1; 257]
                },
                1
            ),
            Err(UpgradeChoiceError::Invalid)
        ));
        for (ranks, caps) in [([0], [0]), ([2], [1])] {
            assert!(matches!(
                UpgradeChoices::<1, 1>::new(Upgrades { ranks, caps }, 1),
                Err(UpgradeChoiceError::Invalid)
            ));
        }
        assert!(matches!(
            UpgradeChoices::<1, 2>::new(Upgrades::new([1]).unwrap(), 1),
            Err(UpgradeChoiceError::Invalid)
        ));
        assert!(matches!(
            UpgradeChoices::<1, 1>::new(Upgrades::new([1]).unwrap(), 0),
            Err(UpgradeChoiceError::Invalid)
        ));
    }

    #[test]
    fn defensive_choice_preflight_order_and_atomicity() {
        let mut choices = UpgradeChoices::<2, 1>::new(Upgrades::new([1, 1]).unwrap(), 1).unwrap();
        choices.credit(1).unwrap();
        let offer = choices.open(&mut RewardRng::new(7)).unwrap().unwrap();
        let id = offer.choices()[0].unwrap();
        // Public ownership forbids concurrent rank edits and point debits. Fault
        // injection here verifies the final guards without exposing such edits.
        choices.upgrades.apply(id).unwrap();
        choices.points = 0;
        let stale = UpgradeOfferId {
            namespace: 2,
            serial: offer.id().serial(),
        };
        assert_eq!(
            choices.choose(stale, usize::MAX),
            Err(UpgradeChoiceError::StaleOffer)
        );
        assert_eq!(
            choices.choose(offer.id(), usize::MAX),
            Err(UpgradeChoiceError::NoPoints)
        );
        assert_eq!(
            choices.choose(offer.id(), id),
            Err(UpgradeChoiceError::NoPoints)
        );
        assert_eq!(choices.points(), 0);
        assert_eq!(choices.pending(), Some(&offer));
        assert_eq!(
            choices.upgrades().ranks,
            [u8::from(id == 0), u8::from(id == 1)]
        );
        choices.credit(1).unwrap();
        assert_eq!(
            choices.choose(offer.id(), usize::MAX),
            Err(UpgradeChoiceError::NotOffered)
        );
        assert_eq!(
            choices.choose(offer.id(), id),
            Err(UpgradeChoiceError::Capped)
        );
        assert_eq!(choices.points(), 1);
        assert_eq!(choices.pending(), Some(&offer));
        assert_eq!(choices.upgrades().rank(id), Some(1));
    }

    #[test]
    fn last_serial_is_usable_once_and_reset_never_rewinds_exhaustion() {
        let mut choices = UpgradeChoices::<1, 1>::new(Upgrades::new([2]).unwrap(), 11).unwrap();
        choices.next_serial = Some(u64::MAX);
        choices.credit(2).unwrap();
        let mut rng = RewardRng::new(7);
        let last = choices.open(&mut rng).unwrap().unwrap();
        assert_eq!(last.id().serial(), u64::MAX);
        let mut untouched = rng;
        assert_eq!(choices.open(&mut rng), Ok(Some(last))); // Cached beats exhausted.
        assert_eq!(rng.next_u64(), untouched.next_u64());
        let accepted = choices.choose(last.id(), 0).unwrap();
        assert_eq!((accepted.rank_after, accepted.points_after), (1, 1));
        let mut untouched = rng;
        assert_eq!(
            choices.open(&mut rng),
            Err(UpgradeChoiceError::SerialExhausted)
        );
        assert_eq!(rng.next_u64(), untouched.next_u64());
        assert_eq!(choices.points(), 1);
        assert_eq!(choices.upgrades().rank(0), Some(1));
        let discarded = choices.reset();
        assert_eq!(discarded.discarded_points, 1);
        assert_eq!(choices.open(&mut rng), Ok(None)); // No points precedes exhaustion.
        choices.credit(1).unwrap();
        assert_eq!(
            choices.open(&mut rng),
            Err(UpgradeChoiceError::SerialExhausted)
        );
        assert_eq!(
            choices.choose(last.id(), 0),
            Err(UpgradeChoiceError::StaleOffer)
        );

        let mut fresh = UpgradeChoices::<1, 1>::new(Upgrades::new([2]).unwrap(), 12).unwrap();
        fresh.credit(1).unwrap();
        let offer = fresh.open(&mut rng).unwrap().unwrap();
        assert_eq!(offer.id().serial(), 1);
        assert_eq!(
            fresh.choose(last.id(), 0),
            Err(UpgradeChoiceError::StaleOffer)
        );
        assert_eq!(fresh.pending(), Some(&offer));
    }

    #[test]
    fn exhausted_but_empty_or_zero_width_open_does_not_touch_entropy() {
        let mut ranks = Upgrades::new([1]).unwrap();
        ranks.apply(0).unwrap();
        let mut capped = UpgradeChoices::<1, 1>::new(ranks, 1).unwrap();
        let mut zero = UpgradeChoices::<1, 0>::new(Upgrades::new([1]).unwrap(), 2).unwrap();
        capped.next_serial = None;
        zero.next_serial = None;
        capped.credit(1).unwrap();
        zero.credit(1).unwrap();
        let mut rng = RewardRng::new(9);
        let mut untouched = rng;
        assert_eq!(capped.open(&mut rng), Ok(None));
        assert_eq!(zero.open(&mut rng), Ok(None));
        assert_eq!(rng.next_u64(), untouched.next_u64());
        assert_eq!((capped.points(), zero.points()), (1, 1));
    }
}
