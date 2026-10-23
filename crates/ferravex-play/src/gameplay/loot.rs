//! Seeded weighted choices with setup-time validation, fixed storage and no floats.
//!
//! Single [`RewardTable::choose`] and [`RewardTable::roll`] retain their legacy
//! mappings. Distinct draws remove **indices**, not caller-defined reward values,
//! from local scratch weights; authored weights never change. Each call starts
//! afresh. Persist the accepted prefix for a menu; do not reroll on redraw.
//!
//! A headless optional reward recipe, using accepted wave clears, actual earned
//! promotions and capped upgrades (the existing games' selection policies stay
//! unchanged). Run with `cargo test -p ferravex-play --doc gameplay::loot
//! --offline --locked -j 2`. This host confirms a defeat after an admitted spawn;
//! a game supplies that fact from its combat system on an actual simulation tick.
//!
//! ```
//! use ferravex_play::gameplay::{
//!     loot::{DistinctDrawStatus, RewardRng, RewardTable},
//!     progression::Progression,
//!     upgrades::Upgrades,
//!     waves::{WaveSequence, WaveSpec},
//! };
//! let mut waves = WaveSequence::new([WaveSpec {
//!     credits: 1, interval: 0.125, rest: 0.0, reward: 10,
//! }], 1).unwrap();
//! let mut xp = Progression::new(10, 0, 4).unwrap();
//! let caps = [1, 1, 1];
//! let mut upgrades = Upgrades::new(caps).unwrap();
//! let mut rng = RewardRng::new(7);
//! let mut points = 0;
//! assert!(waves.admit(1, 0));
//! assert_eq!(waves.advance(0.125, 0.0, 1).unwrap(), None);
//! // The admitted actor has now been defeated; only the clear earns XP.
//! let clear = waves.advance(0.125, 0.0, 0).unwrap().unwrap();
//! points += xp.award(u64::from(clear.reward));
//! let authored = [3, 2, 1];
//! let weights: [u32; 3] = std::array::from_fn(|id| {
//!     if upgrades.rank(id).unwrap() < caps[id] { authored[id] } else { 0 }
//! });
//! // All-capped pools need no table or entropy. Never spend an unearned point.
//! if points > 0 && weights.iter().any(|&w| w > 0) {
//!     let table = RewardTable::new(weights).unwrap();
//!     let mut offers = [usize::MAX; 3];
//!     let report = table.roll_distinct_into(&mut rng, 3, &mut offers, 16).unwrap();
//!     assert_eq!(report.status, DistinctDrawStatus::Complete);
//!     // Keep this prefix across redraws. This headless host chooses its first
//!     // offer; an interactive host uses the player's submitted offered index.
//!     let displayed = &offers[..report.written];
//!     let chosen = displayed[0];
//!     if points > 0 && displayed.contains(&chosen)
//!         && upgrades.rank(chosen).unwrap() < caps[chosen]
//!     {
//!         points -= 1;
//!         upgrades.apply(chosen).unwrap();
//!     }
//!     assert_eq!(upgrades.rank(chosen), Some(1));
//! }
//! assert_eq!(points, 0);
//! assert_eq!(xp.level(), 2);
//! assert_eq!(waves.advance(0.125, 0.0, 0).unwrap(), None);
//! ```
use super::LoopError;

/// Maximum request size for either distinct operation, independent of table size.
pub const MAX_DISTINCT_DRAWS: usize = 256;
/// Maximum number of SplitMix64 outputs consumed by one distinct roll call.
pub const MAX_DISTINCT_SAMPLES: usize = 4096;

/// Preflight failures: neither caller output nor RNG is changed.
///
/// See [`RewardTable::roll_distinct_into`] for precedence and a usage example.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistinctDrawError {
    /// `requested` exceeds [`MAX_DISTINCT_DRAWS`].
    RequestTooLarge { requested: usize },
    /// Output cannot hold `min(requested, positive_weight_entries)` indices.
    OutputTooSmall { required: usize, provided: usize },
    /// `sample_budget` exceeds [`MAX_DISTINCT_SAMPLES`]. Zero is valid.
    SampleBudgetTooLarge { sample_budget: usize },
}

/// Why a distinct call stopped; completion takes precedence over pool exhaustion,
/// which takes precedence over input exhaustion. See [`DistinctDrawReport`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistinctDrawStatus {
    /// Exactly the requested number of indices were written, including zero.
    Complete,
    /// All positive-weight indices were written, fewer than requested.
    TableExhausted,
    /// More offers were possible, but the roll's sample budget was consumed.
    SampleBudgetExhausted,
    /// More offers were possible, but the deterministic ticket slice ended.
    TicketsExhausted,
}

/// Accepted prefix and exact input consumption, including rejected RNG samples.
///
/// Only `out[..written]` is changed. The tail retains its previous values on all
/// outcomes. `samples_used` counts SplitMix64 outputs for rolls, or tickets for
/// ticket draws. Exhaustion is not an error or a rollback: the prefix is valid,
/// RNG consumption is retained, and another call starts with the full table.
/// See the examples on [`RewardTable::roll_distinct_into`] and
/// [`RewardTable::choose_distinct_into`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub struct DistinctDrawReport {
    pub written: usize,
    pub samples_used: usize,
    pub status: DistinctDrawStatus,
}

#[derive(Clone, Copy, Debug)]
pub struct RewardTable<const N: usize> {
    weights: [u32; N],
    total: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct RewardRng(u64);
impl RewardRng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }
    /// SplitMix64; gameplay variety only, never security or matchmaking secrets.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
}
impl<const N: usize> RewardTable<N> {
    pub fn new(weights: [u32; N]) -> Result<Self, LoopError> {
        if N == 0 || N > 256 {
            return Err(LoopError::Invalid);
        }
        let total = weights
            .iter()
            .try_fold(0_u32, |s, w| s.checked_add(*w))
            .ok_or(LoopError::Invalid)?;
        if total == 0 {
            return Err(LoopError::Invalid);
        }
        Ok(Self { weights, total })
    }
    /// Caller can supply a saved/replayed ticket. Zero weight is never selected.
    pub fn choose(&self, ticket: u32) -> usize {
        let mut ticket = ticket % self.total;
        for (i, w) in self.weights.iter().enumerate() {
            if ticket < *w {
                return i;
            }
            ticket -= w;
        }
        unreachable!("validated nonzero total")
    }
    /// Fixed work; multiply-high maps the full 64-bit sample to the weight range.
    /// This legacy mapping has rounding bias for totals that do not divide 2^64;
    /// its results and one-output RNG advancement are preserved.
    pub fn roll(&self, rng: &mut RewardRng) -> usize {
        self.choose(((u128::from(rng.next_u64()) * u128::from(self.total)) >> 64) as u32)
    }

    /// Weighted sampling without replacement, into caller storage.
    ///
    /// For each attempt, consume exactly one [`RewardRng::next_u64`] output and
    /// use its upper 32 bits as `x`. For remaining total `W`, reject
    /// `x < (2^32 mod W)`; otherwise use `x % W` as a cumulative-weight ticket,
    /// in original index order. Remove the selected index's entire weight before
    /// the next draw. Zero-weight entries never appear; no duplicate retries occur.
    /// Even `W == 1` consumes a sample for an accepted draw.
    ///
    /// With independent uniform 32-bit samples, rejection leaves equally many
    /// representatives of every ticket. Conditional on a next draw succeeding,
    /// its index probability is `weight[index] / W`. A finite budget can censor
    /// the prefix depending on earlier choices, so conditioning on an entire
    /// request completing need not retain that distribution. SplitMix64 is a
    /// deterministic gameplay PRNG, not a claim of independent random entropy or
    /// security. There is no biased fallback on budget exhaustion.
    ///
    /// Preflight checks, in order: `requested <= MAX_DISTINCT_DRAWS`, output has
    /// at least `min(requested, positive_weight_entries)` slots, then
    /// `sample_budget <= MAX_DISTINCT_SAMPLES`. Errors consume no RNG and write
    /// nothing. Zero budget is valid: a nonzero request returns an empty prefix
    /// with `SampleBudgetExhausted`; a zero request completes without samples.
    /// Accepted or rejected attempts each consume one budget unit and RNG output.
    /// Only the accepted prefix is written, including on exhaustion; no rollback.
    ///
    /// At most `N <= 256` scratch weights (1024 bytes), `requested <= 256` writes,
    /// and `sample_budget <= 4096` samples. Work is `O(N + sample_budget +
    /// N * min(requested, positive_weight_entries))`, without allocation, floating
    /// point, callbacks, wall clocks or authored-weight mutation. A fresh call
    /// restarts the pool; do not concatenate partial calls as a continuation.
    ///
    /// ```
    /// use ferravex_play::gameplay::loot::{RewardRng, RewardTable, DistinctDrawStatus};
    /// let table = RewardTable::new([1, 0, 3]).unwrap();
    /// let mut rng = RewardRng::new(0);
    /// let mut out = [99; 3];
    /// let report = table.roll_distinct_into(&mut rng, 3, &mut out, 8).unwrap();
    /// assert_eq!(report.status, DistinctDrawStatus::TableExhausted);
    /// assert_eq!((report.written, report.samples_used), (2, 2));
    /// assert!(out[..2].contains(&0) && out[..2].contains(&2));
    /// assert_eq!(out[2], 99);
    /// ```
    pub fn roll_distinct_into(
        &self,
        rng: &mut RewardRng,
        requested: usize,
        out: &mut [usize],
        sample_budget: usize,
    ) -> Result<DistinctDrawReport, DistinctDrawError> {
        self.preflight_distinct(requested, out.len())?;
        if sample_budget > MAX_DISTINCT_SAMPLES {
            return Err(DistinctDrawError::SampleBudgetTooLarge { sample_budget });
        }
        let mut remaining = RemainingRewards::new(self);
        let mut written = 0;
        let mut samples_used = 0;
        while written < requested && remaining.total > 0 && samples_used < sample_budget {
            let sample = (rng.next_u64() >> 32) as u32;
            samples_used += 1;
            if let Some(ticket) = bounded_ticket(sample, remaining.total) {
                out[written] = remaining.take(ticket);
                written += 1;
            }
        }
        Ok(remaining.report(
            requested,
            written,
            samples_used,
            DistinctDrawStatus::SampleBudgetExhausted,
        ))
    }

    /// Deterministic distinct draws from saved/authored tickets, without RNG.
    ///
    /// Apply each consumed ticket modulo the **remaining** weight total and
    /// remove that selected index. No tickets are read after completion or pool
    /// exhaustion. `samples_used` reports the number consumed, at most 256;
    /// `TicketsExhausted` preserves the accepted prefix if the slice ends early.
    /// This is an explicit replay mapping, not an unbiased conversion of arbitrary
    /// random `u32`s. Independent uniform tickets in each remaining range give
    /// sequential weight-proportional draws. Tickets depend on weights and order;
    /// saved indices still depend on order. Hosts own stable reward identities and
    /// any replay/content versioning.
    ///
    /// Request/capacity preflight, tail preservation and fresh-pool semantics
    /// match [`Self::roll_distinct_into`]. Work is `O(N * (1 + min(requested,
    /// positive_weight_entries, tickets.len())))`; storage is `N` scratch weights,
    /// with no allocation. Even an arbitrarily long ticket slice is not scanned.
    ///
    /// ```
    /// use ferravex_play::gameplay::loot::{RewardTable, DistinctDrawStatus};
    /// let table = RewardTable::new([1, 0, 3]).unwrap();
    /// let mut out = [99; 4];
    /// let report = table.choose_distinct_into(&[3, 0, 7], 3, &mut out).unwrap();
    /// assert_eq!(&out, &[2, 0, 99, 99]);
    /// assert_eq!((report.written, report.samples_used), (2, 2));
    /// assert_eq!(report.status, DistinctDrawStatus::TableExhausted);
    /// ```
    pub fn choose_distinct_into(
        &self,
        tickets: &[u32],
        requested: usize,
        out: &mut [usize],
    ) -> Result<DistinctDrawReport, DistinctDrawError> {
        self.preflight_distinct(requested, out.len())?;
        let mut remaining = RemainingRewards::new(self);
        let mut written = 0;
        while written < requested && remaining.total > 0 && written < tickets.len() {
            out[written] = remaining.take(tickets[written] % remaining.total);
            written += 1;
        }
        Ok(remaining.report(
            requested,
            written,
            written,
            DistinctDrawStatus::TicketsExhausted,
        ))
    }

    fn preflight_distinct(
        &self,
        requested: usize,
        capacity: usize,
    ) -> Result<(), DistinctDrawError> {
        if requested > MAX_DISTINCT_DRAWS {
            return Err(DistinctDrawError::RequestTooLarge { requested });
        }
        let positive = self.weights.iter().filter(|&&weight| weight > 0).count();
        let required = requested.min(positive);
        if capacity < required {
            return Err(DistinctDrawError::OutputTooSmall {
                required,
                provided: capacity,
            });
        }
        Ok(())
    }
}

// Discard the incomplete modulo cycle at the bottom of the u32 domain.
// `total` is nonzero, and may be u32::MAX; never compute 2^32 in a u32.
fn bounded_ticket(sample: u32, total: u32) -> Option<u32> {
    let threshold = total.wrapping_neg() % total;
    (sample >= threshold).then(|| sample % total)
}

struct RemainingRewards<const N: usize> {
    weights: [u32; N],
    total: u32,
}
impl<const N: usize> RemainingRewards<N> {
    fn new(table: &RewardTable<N>) -> Self {
        Self {
            weights: table.weights,
            total: table.total,
        }
    }

    // ticket < total; each accepted draw removes one whole positive entry.
    fn take(&mut self, mut ticket: u32) -> usize {
        for (index, weight) in self.weights.iter_mut().enumerate() {
            if ticket < *weight {
                self.total -= *weight;
                *weight = 0;
                return index;
            }
            ticket -= *weight;
        }
        unreachable!("ticket within remaining nonzero total")
    }

    fn report(
        &self,
        requested: usize,
        written: usize,
        samples_used: usize,
        input_exhausted: DistinctDrawStatus,
    ) -> DistinctDrawReport {
        DistinctDrawReport {
            written,
            samples_used,
            status: if written == requested {
                DistinctDrawStatus::Complete
            } else if self.total == 0 {
                DistinctDrawStatus::TableExhausted
            } else {
                input_exhausted
            },
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_mapping_rejects_only_the_incomplete_modulo_cycle() {
        for total in [1, 2, 3, 7, 256, 0x8000_0001, u32::MAX] {
            let domain = 1_u64 << 32;
            let threshold = (domain % u64::from(total)) as u32;
            assert_eq!((domain - u64::from(threshold)) % u64::from(total), 0);
            if threshold > 0 {
                assert_eq!(bounded_ticket(0, total), None);
                assert_eq!(bounded_ticket(threshold - 1, total), None);
            }
            assert_eq!(bounded_ticket(threshold, total), Some(threshold % total));
            assert_eq!(bounded_ticket(u32::MAX, total), Some(u32::MAX % total));
        }
        assert_eq!(bounded_ticket(0x7fff_fffe, 0x8000_0001), None);
        assert_eq!(bounded_ticket(0x7fff_ffff, 0x8000_0001), Some(0x7fff_ffff));
        assert_eq!(bounded_ticket(0x8000_0001, 0x8000_0001), Some(0));
        assert_eq!(bounded_ticket(0, u32::MAX), None);
        assert_eq!(bounded_ticket(u32::MAX, u32::MAX), Some(0));
    }

    #[test]
    fn tickets_respect_weights_and_seeds_reproduce_sequences() {
        let t = RewardTable::new([1, 0, 3]).unwrap();
        assert_eq!(
            (0..4).map(|n| t.choose(n)).collect::<Vec<_>>(),
            [0, 2, 2, 2]
        );
        let mut a = RewardRng::new(0);
        let mut b = a;
        for _ in 0..100 {
            assert_eq!(t.roll(&mut a), t.roll(&mut b));
        }
        assert!(RewardTable::new([0, 0]).is_err());
        assert!(RewardTable::new([u32::MAX, 1]).is_err());
    }
}
