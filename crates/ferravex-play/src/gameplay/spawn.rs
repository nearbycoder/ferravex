//! Deterministic safe-spawn selection. Hosts supply authoritative occupancy and
//! line-of-sight eligibility before spending encounter credits.
//!
//! The legacy [`SpawnSelector::choose`] keeps its farthest-first callback
//! pruning. The checked APIs instead explain **every** candidate, rank finite
//! caller scores, and never turn an unsafe result into a fallback position.
//! Scores/availability are authoritative host policy, not client authority.
//!
//! # Admission recipe
//!
//! This uses the real encounter admission gate and one already prepared actor
//! slot. Selection runs on a copy: failed admission does not rotate the live
//! selector. No world mutation occurs between the queries and admission. Once
//! admission spends credits, placing into the prepared slot cannot fail. For a
//! fallible ECS/pool admission, reserve/validate its capacity first and arrange
//! explicit rollback if a later gate can fail; selection is not a reservation.
//!
//! ```
//! use ferravex_math::Vec2;
//! use ferravex_play::{
//!     encounter::{EncounterConfig, EncounterDirector},
//!     gameplay::spawn::{SpawnAssessment, SpawnCheckError, SpawnSelector},
//! };
//!
//! fn attempt(selector: &mut SpawnSelector, director: &mut EncounterDirector,
//!     actor: &mut Option<Vec2>, obstructed: bool) -> Result<bool, SpawnCheckError<()>>
//! {
//!     let points = [Vec2::new(20.0, 0.0), Vec2::new(-20.0, 0.0)];
//!     let mut output = [SpawnAssessment::EMPTY; 2];
//!     let mut pending = *selector;
//!     let chosen = pending.choose_checked_into(&points, Vec2::ZERO, &mut output,
//!         |_| Ok(actor.is_none()), // Prepared storage availability.
//!         |_, distance_squared| Ok(Some(distance_squared)),
//!         |_| Ok(!obstructed))?; // Replace with authoritative geometry/LOS query.
//!     if let Some(index) = chosen {
//!         if director.admit(1, usize::from(actor.is_some())) {
//!             *actor = Some(points[index]); // Guaranteed placement in prepared slot.
//!             *selector = pending;
//!             return Ok(true); // Emit accepted-event feedback here.
//!         }
//!     }
//!     Ok(false) // Defer; no placement or credits spent when unsafe/rejected.
//! }
//! let mut selector = SpawnSelector::new(10.0).unwrap();
//! let mut director = EncounterDirector::new(EncounterConfig {
//!     credits: 2, max_alive: 1, interval: 0.5, first_delay: 0.0,
//!     recovery_seconds: 1.0,
//! }).unwrap();
//! let mut actor = None;
//! assert!(!attempt(&mut selector, &mut director, &mut actor, true).unwrap());
//! assert_eq!(director.credits(), 2);
//! assert_eq!(actor, None);
//! assert!(attempt(&mut selector, &mut director, &mut actor, false).unwrap());
//! assert_eq!(director.credits(), 1);
//! assert_eq!(actor.take(), Some(Vec2::new(20.0, 0.0)));
//! // Pacing rejects another safe selection without accepting its cursor change.
//! assert!(!attempt(&mut selector, &mut director, &mut actor, false).unwrap());
//! assert_eq!(director.credits(), 1);
//! director.advance(0.25, 0.0).unwrap();
//! director.advance(0.25, 0.0).unwrap();
//! assert!(attempt(&mut selector, &mut director, &mut actor, false).unwrap());
//! assert_eq!(actor, Some(Vec2::new(-20.0, 0.0)));
//! ```
use super::LoopError;
use ferravex_math::Vec2;
/// First exclusion in the checked pipeline; later queries are skipped.
/// See the module's admission recipe and [`SpawnSelector::assess_into`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnExclusion {
    /// Nonfinite coordinate or an absolute component greater than 1e6.
    InvalidGeometry,
    /// Squared distance is strictly below the configured minimum squared.
    TooClose,
    /// The availability query returned false (e.g. an unavailable authored slot).
    Unavailable,
    /// The score callback returned `None` under the host's authored policy.
    Authored,
    /// The final clearance/line-of-sight query returned false.
    Obstructed,
}

/// One candidate's eligibility, with the host's finite score when eligible.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpawnOutcome {
    Excluded(SpawnExclusion),
    Eligible { score: f32 },
}

/// Read-only diagnostic stored at the original source index in caller output.
/// Indices identify this input slice only; reordering candidates changes identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpawnAssessment {
    pub index: usize,
    /// `None` only for invalid candidate geometry; otherwise uses legacy f32
    /// squared-distance arithmetic in caller-defined logical world units.
    pub distance_squared: Option<f32>,
    pub outcome: SpawnOutcome,
}
impl SpawnAssessment {
    /// Storage initializer, not an assessed candidate. Only the first input
    /// length entries are meaningful after a successful checked call.
    pub const EMPTY: Self = Self {
        index: 0,
        distance_squared: None,
        outcome: SpawnOutcome::Excluded(SpawnExclusion::InvalidGeometry),
    };
}

/// Which external query failed. Query side effects cannot be rolled back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnQuery {
    Availability,
    Score,
    Obstruction,
}

/// Request/numeric/query failures, distinct from individual candidate exclusions.
/// Every reported error preserves the selector and the entire output slice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnCheckError<E> {
    EmptyCandidates,
    CandidateCapacity {
        count: usize,
    },
    InvalidReference,
    OutputTooSmall {
        required: usize,
        provided: usize,
    },
    /// A nonfinite score aborts the request, rather than excluding a candidate.
    InvalidScore {
        index: usize,
    },
    Callback {
        index: usize,
        query: SpawnQuery,
        error: E,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct SpawnSelector {
    minimum: f32,
    cursor: usize,
}
impl SpawnSelector {
    /// Maximum input length for both legacy and checked paths.
    pub const MAX_CANDIDATES: usize = 256;

    pub fn new(minimum: f32) -> Result<Self, LoopError> {
        if !minimum.is_finite() || !(0. ..=1e6).contains(&minimum) {
            return Err(LoopError::Invalid);
        }
        Ok(Self { minimum, cursor: 0 })
    }
    /// Farthest eligible candidate wins, with rotating stable tie breaks.
    /// At most 256 eligibility queries. None means defer spawning, never fallback
    /// into an unsafe location. Callbacks should be read-only world queries.
    pub fn choose(
        &mut self,
        positions: &[Vec2],
        player: Vec2,
        mut eligible: impl FnMut(usize) -> bool,
    ) -> Result<Option<usize>, LoopError> {
        if positions.is_empty()
            || positions.len() > 256
            || !valid(player)
            || positions.iter().any(|p| !valid(*p))
        {
            return Err(LoopError::Invalid);
        }
        let mut best = None;
        let mut best_distance = -1.;
        for offset in 0..positions.len() {
            let i = (self.cursor + offset) % positions.len();
            let d = positions[i].distance_squared(player);
            if d >= self.minimum * self.minimum && d > best_distance && eligible(i) {
                best = Some(i);
                best_distance = d;
            }
        }
        if let Some(i) = best {
            self.cursor = (i + 1) % positions.len();
        }
        Ok(best)
    }

    /// Assess candidates without rotating the selector. Returns the winning
    /// source index, or `None` when all candidates are excluded (defer spawning).
    ///
    /// Requires 1..=256 positions, a valid reference (finite, each absolute
    /// component <= 1e6), and output capacity >= input length, checked in that
    /// order **before any callbacks**. Bad individual positions are exclusions,
    /// unlike the legacy method's whole-request error. The minimum-distance
    /// boundary is inclusive and uses the same arithmetic as [`Self::choose`].
    ///
    /// Starting at the existing cursor modulo input length, visit each source
    /// index once, wrapping. At each index: geometry, minimum distance,
    /// `available`, `score`, `unobstructed`, stopping at the first exclusion.
    /// Each callback is invoked at most once per candidate (at most 256 each,
    /// 768 total); no obstruction query runs after any prior exclusion or invalid
    /// score. Unlike legacy `choose`, eligible lower-scored candidates still get
    /// obstruction queries so that diagnostics describe every candidate.
    ///
    /// `score(index, distance_squared)` returns `None` for authored exclusion,
    /// or a finite score (negative/zero allowed). Higher scores win; exactly equal
    /// scores keep the first in rotating source order, with no random seed.
    /// Return the supplied distance for legacy farthest-distance ranking.
    /// Availability and score callbacks should be cheap read-only checks;
    /// clearance/LOS stays entirely caller-controlled in `unobstructed`.
    ///
    /// On success, output[..positions.len()] is replaced in **source index**
    /// order and the tail is untouched. Errors (including a later query failure
    /// or nonfinite score) leave all output and selector state untouched using
    /// fixed stack scratch storage, with no heap allocation by this method.
    /// External callback effects/allocations/panics cannot be rolled back; no
    /// remaining queries run after a reported failure. Diagnostics are a snapshot,
    /// not a reservation or permission to skip actual admission/revalidation.
    ///
    /// ```
    /// use ferravex_math::Vec2;
    /// use ferravex_play::gameplay::spawn::{SpawnAssessment, SpawnSelector};
    /// let selector = SpawnSelector::new(10.0).unwrap();
    /// let points = [Vec2::new(20.0, 0.0), Vec2::new(-20.0, 0.0)];
    /// let mut output = [SpawnAssessment::EMPTY; 2];
    /// for _ in 0..2 {
    ///     let best = selector.assess_into(&points, Vec2::ZERO, &mut output,
    ///         |_| Ok::<_, ()>(true), |_, d| Ok(Some(d)), |_| Ok(true)).unwrap();
    ///     assert_eq!(best, Some(0)); // Reading does not rotate ties.
    /// }
    /// ```
    pub fn assess_into<E>(
        &self,
        positions: &[Vec2],
        reference: Vec2,
        output: &mut [SpawnAssessment],
        mut available: impl FnMut(usize) -> Result<bool, E>,
        mut score: impl FnMut(usize, f32) -> Result<Option<f32>, E>,
        mut unobstructed: impl FnMut(usize) -> Result<bool, E>,
    ) -> Result<Option<usize>, SpawnCheckError<E>> {
        if positions.is_empty() {
            return Err(SpawnCheckError::EmptyCandidates);
        }
        if positions.len() > Self::MAX_CANDIDATES {
            return Err(SpawnCheckError::CandidateCapacity {
                count: positions.len(),
            });
        }
        if !valid(reference) {
            return Err(SpawnCheckError::InvalidReference);
        }
        if output.len() < positions.len() {
            return Err(SpawnCheckError::OutputTooSmall {
                required: positions.len(),
                provided: output.len(),
            });
        }
        let mut scratch = [SpawnAssessment::EMPTY; Self::MAX_CANDIDATES];
        let mut best: Option<(usize, f32)> = None;
        for offset in 0..positions.len() {
            let i = (self.cursor + offset) % positions.len();
            let assessment = &mut scratch[i];
            assessment.index = i;
            if !valid(positions[i]) {
                continue;
            }
            let distance = positions[i].distance_squared(reference);
            assessment.distance_squared = Some(distance);
            let exclusion = if distance < self.minimum * self.minimum {
                SpawnExclusion::TooClose
            } else if !available(i).map_err(|error| SpawnCheckError::Callback {
                index: i,
                query: SpawnQuery::Availability,
                error,
            })? {
                SpawnExclusion::Unavailable
            } else if let Some(value) =
                score(i, distance).map_err(|error| SpawnCheckError::Callback {
                    index: i,
                    query: SpawnQuery::Score,
                    error,
                })?
            {
                if !value.is_finite() {
                    return Err(SpawnCheckError::InvalidScore { index: i });
                }
                if unobstructed(i).map_err(|error| SpawnCheckError::Callback {
                    index: i,
                    query: SpawnQuery::Obstruction,
                    error,
                })? {
                    assessment.outcome = SpawnOutcome::Eligible { score: value };
                    if best.is_none_or(|(_, previous)| value > previous) {
                        best = Some((i, value));
                    }
                    continue;
                }
                SpawnExclusion::Obstructed
            } else {
                SpawnExclusion::Authored
            };
            assessment.outcome = SpawnOutcome::Excluded(exclusion);
        }
        output[..positions.len()].copy_from_slice(&scratch[..positions.len()]);
        Ok(best.map(|(index, _)| index))
    }

    /// Assess and accept a selection, rotating only on `Ok(Some(index))`.
    /// All query, output and failure contracts are those of [`Self::assess_into`].
    /// This accepts selector state only, not an actor or encounter credit spend.
    /// Use a copied selector and commit it after actual admission if that can
    /// fail; see the module's runnable encounter recipe.
    ///
    /// ```
    /// use ferravex_math::Vec2;
    /// use ferravex_play::gameplay::spawn::{SpawnAssessment, SpawnSelector};
    /// let mut selector = SpawnSelector::new(0.0).unwrap();
    /// let points = [Vec2::X, -Vec2::X];
    /// let mut output = [SpawnAssessment::EMPTY; 2];
    /// for expected in [0, 1] {
    ///     assert_eq!(selector.choose_checked_into(&points, Vec2::ZERO, &mut output,
    ///         |_| Ok::<_, ()>(true), |_, d| Ok(Some(d)), |_| Ok(true)).unwrap(),
    ///         Some(expected));
    /// }
    /// ```
    pub fn choose_checked_into<E>(
        &mut self,
        positions: &[Vec2],
        reference: Vec2,
        output: &mut [SpawnAssessment],
        available: impl FnMut(usize) -> Result<bool, E>,
        score: impl FnMut(usize, f32) -> Result<Option<f32>, E>,
        unobstructed: impl FnMut(usize) -> Result<bool, E>,
    ) -> Result<Option<usize>, SpawnCheckError<E>> {
        let best =
            self.assess_into(positions, reference, output, available, score, unobstructed)?;
        if let Some(i) = best {
            self.cursor = (i + 1) % positions.len();
        }
        Ok(best)
    }

    /// Reset tie rotation to source index zero, retaining the authored minimum.
    /// Reset on a new run or when intentionally restarting candidate identity.
    ///
    /// ```
    /// use ferravex_math::Vec2;
    /// use ferravex_play::gameplay::spawn::SpawnSelector;
    /// let mut selector = SpawnSelector::new(0.0).unwrap();
    /// let points = [Vec2::X, -Vec2::X];
    /// assert_eq!(selector.choose(&points, Vec2::ZERO, |_| true), Ok(Some(0)));
    /// selector.reset();
    /// assert_eq!(selector.choose(&points, Vec2::ZERO, |_| true), Ok(Some(0)));
    /// ```
    pub fn reset(&mut self) {
        self.cursor = 0;
    }
}
fn valid(p: Vec2) -> bool {
    p.is_finite() && p.abs().max_element() <= 1e6
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsafe_candidates_defer_and_equal_distances_rotate() {
        let mut s = SpawnSelector::new(10.).unwrap();
        let p = [Vec2::X * 20., -Vec2::X * 20., Vec2::X];
        assert_eq!(s.choose(&p, Vec2::ZERO, |_| true).unwrap(), Some(0));
        assert_eq!(s.choose(&p, Vec2::ZERO, |_| true).unwrap(), Some(1));
        assert_eq!(s.choose(&p, Vec2::ZERO, |_| false).unwrap(), None);
        assert_eq!(s.choose(&p, Vec2::ZERO, |i| i == 2).unwrap(), None);
        assert!(s.choose(&[], Vec2::ZERO, |_| panic!()).is_err());
    }
}
