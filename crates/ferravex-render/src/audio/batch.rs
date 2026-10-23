//! Bounded presentation-frame batching of caller-accepted events.
//!
//! This is a borrowed adapter over [`GameAudio`], not an event validator or ID
//! deduplicator. Create one batch per presentation frame, collect across actual
//! fixed updates, then flush once. Dropping it cancels pending work. Its exclusive
//! borrow prevents changing/replacing the controller behind queued cues; use
//! [`CueBatch::pause`] / [`CueBatch::reset`] during the borrow. Release the batch
//! before resuming or restarting the owner. Never retain it across host frames.
//! Runtime replacement cancels pending cues using the runtime's weak identity,
//! the same lifetime boundary used by GameAudio's ownership repair.
//!
//! Category IDs are caller-authored, not gameplay event IDs. While a category is
//! pending, its cue, policy and critical flag must agree at every position.
//! Only equal category IDs **and equal positions** coalesce (`None` is distinct
//! from `Some(Vec3::ZERO)`). No spatial averaging, gain summing, hidden volume
//! multiplication or automatic event deduplication occurs. Strongest means the
//! greatest raw gain; equal gains keep the first. Replacement retains the first
//! slot's order. A new critical cue at capacity evicts the oldest noncritical
//! slot and appends; otherwise overflow drops the incoming cue. All-critical
//! overflow therefore keeps the earlier cues, regardless of gains. Critical is
//! solely a storage guarantee against noncritical arrivals, **not** mixer priority
//! or admission. Distinct positions can consume multiple slots of one category.
//!
//! Collection/flush uses only fixed arrays, bounded scans (at most 128 slots),
//! and existing GameAudio submissions. After audio bank/mixer setup these paths
//! allocate nothing. Runtime/bank creation, arbitrary host callbacks and their
//! failures are outside that claim. Flush has no callbacks and consumes every
//! retained request, including suppressed/rejected requests; it never retries.
//! Mute may make admitted voices silent. PCM tests are not listening evidence.
//!
//! Runnable headless recipe: actual Magazine acceptance plus a timed first-party
//! objective, driven by GameSession ticks. The objective completion, rather than
//! a fabricated game result, supplies the terminal cue. No wall time or device is
//! used. For spatial contacts use `Some(event.position)` instead of `None`.
//!
//! ```rust,check
//! use ferravex_core::session::GameSession;
//! use ferravex_play::{gameplay::magazine::Magazine,
//!     objective::{ObjectiveGraph, ObjectiveSpec, ObjectiveKind}};
//! use ferravex_render::audio::{self, Cue, GameAudio,
//!     batch::{CueBatch, CueCategory, Coalesce}};
//! let shot = CueCategory { key: 1, cue: Cue::Fire,
//!     policy: Coalesce::Strongest, critical: false };
//! let terminal = CueCategory { key: 2, cue: Cue::Success,
//!     policy: Coalesce::First, critical: true };
//! let mut session = GameSession::new(64., 4, true);
//! let mut magazine = Magazine::new(3, 0, 1.).unwrap();
//! let spec = ObjectiveSpec {
//!     id: 1, all: 0, any: 0, exclusive_group: 0,
//!     kind: ObjectiveKind::Timer { seconds: 3. / 64. },
//! };
//! let mut objective = ObjectiveGraph::new(1, [spec]).unwrap();
//! let mut owner = GameAudio::new(); // Prepare bank outside collection/flush.
//! {
//!     let mut cues = CueBatch::<2>::new(&mut owner).unwrap();
//!     assert_eq!(session.advance(0.).steps, 0);
//!     assert_eq!(cues.flush().submitted, 0); // Zero-tick/empty frame.
//! }
//! {
//!     let mut cues = CueBatch::<2>::new(&mut owner).unwrap();
//!     let frame = session.advance(4. / 64.);
//!     let mut executed = 0;
//!     for dt in frame.ticks() {
//!         if !session.is_playing() { break; }
//!         executed += 1;
//!         if magazine.fire() { let _ = cues.queue(shot, None, 0.4).unwrap(); }
//!         if objective.advance(dt).unwrap().completed != 0 {
//!             assert!(session.finish(true));
//!             let _ = cues.queue(terminal, None, 0.6).unwrap();
//!         }
//!     }
//!     assert_eq!((frame.steps, executed, session.tick()), (4, 3, 4));
//!     let report = cues.flush(); // One presentation flush for three ticks.
//!     assert_eq!((report.batch.coalesced, report.submitted, report.admitted()), (2, 2, 2));
//!     assert_eq!(cues.flush().submitted, 0); // No replay on an empty frame.
//! }
//! let mut pcm = [[0.; 2]; 256];
//! assert!(audio::render(&mut pcm, audio::SAMPLE_RATE)); // Offline PCM mixing.
//! assert!(pcm.iter().flatten().all(|v| v.is_finite()));
//! {
//!     let mut cues = CueBatch::<2>::new(&mut owner).unwrap();
//!     let _ = cues.queue(shot, None, 0.4).unwrap();
//!     assert_eq!(cues.pause(), 1); // Discard pending work and owned gameplay voices.
//!     assert_eq!(cues.flush().submitted, 0);
//!     cues.reset(); // Preserves pause; no retry of the cancelled shot.
//! }
//! session.restart();
//! magazine = Magazine::new(3, 0, 1.).unwrap();
//! objective = ObjectiveGraph::new(1, [spec]).unwrap();
//! owner.resume(); // Release batch first: no pending work survives resume.
//! assert_eq!(session.tick(), 0);
//! let mut retry = CueBatch::<2>::new(&mut owner).unwrap();
//! assert_eq!(retry.flush().submitted, 0);
//! for dt in session.advance(1. / 64.).ticks() {
//!     if magazine.fire() { let _ = retry.queue(shot, None, 0.4).unwrap(); }
//!     assert_eq!(objective.advance(dt).unwrap().completed, 0);
//! }
//! assert_eq!(retry.flush().submitted, 1); // Fresh accepted shot, no stale cue.
//! ```

use super::{Cue, GameAudio, VoiceAdmission, VoiceRejection};
use crate::prelude::Vec3;
use std::rc::{Rc, Weak};

/// Selection among requests at the same category and exact position.
/// See the module recipe for category construction and submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coalesce {
    /// Keep the first gain, even when later gains are stronger.
    First,
    /// Keep the greatest gain; ties retain the first request.
    Strongest,
    /// Keep the latest gain, even when weaker; retain the original slot order.
    Latest,
}

/// Caller-authored category. Configuration must agree while its key is pending.
/// No key value is reserved. See the module recipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CueCategory {
    pub key: u32,
    pub cue: Cue,
    pub policy: Coalesce,
    pub critical: bool,
}

/// One retained raw request, before GameAudio acoustics or mixer admission.
/// Construct through [`CueBatch::queue`]; see the module recipe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BatchedCue {
    pub category: CueCategory,
    pub position: Option<Vec3>,
    pub gain: f32,
}

/// Validation errors leave pending work and counters unchanged, even if the
/// runtime has changed. A subsequent valid operation/flush reconciles runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatchError {
    /// The const capacity must be in `1..=128`.
    Capacity,
    /// Raw gain must be finite and in `0..=4`, including both endpoints.
    Gain,
    /// Every coordinate of a supplied position must be finite.
    Position,
    /// Same pending key, different cue, coalescing policy or critical flag.
    CategoryConflict,
}

/// Immediate storage decision; independent of later voice admission.
#[derive(Clone, Copy, Debug, PartialEq)]
#[must_use]
pub enum QueueOutcome {
    Retained,
    /// One request merged; `replaced` means its gain was selected as replacement.
    Coalesced {
        replaced: bool,
    },
    /// Incoming critical request retained; this older noncritical was evicted.
    Evicted(BatchedCue),
    /// Incoming request lost at capacity; no pending slot changed.
    Dropped,
}

/// Saturating counts since the preceding flush (or construction).
/// `retained` counts insertions, including evicting insertions; `replaced` is a
/// subset of `coalesced`. Dropped arrivals, evictions, explicit cancellations
/// and runtime discards are separate. Current occupancy is [`CueBatch::len`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BatchCounts {
    pub retained: usize,
    pub coalesced: usize,
    pub replaced: usize,
    pub evicted: usize,
    pub dropped: usize,
    pub cancelled: usize,
    pub runtime_discarded: usize,
}

/// Exact result of submitting one retained request through GameAudio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CueOutcome {
    pub request: BatchedCue,
    /// `None` is actual GameAudio pause suppression. `Some` retains the exact
    /// started/stolen/rejection outcome, including acoustic gain validation.
    pub admission: Option<VoiceAdmission>,
}

/// Fixed-storage flush result. The first `submitted` entries of `outcomes` are
/// `Some`, in retained insertion order; all remaining entries are `None`.
/// Counts describe submissions, not voices still live after later steals.
#[derive(Clone, Debug, PartialEq)]
pub struct FlushReport<const N: usize> {
    pub batch: BatchCounts,
    pub submitted: usize,
    pub started: usize,
    pub stolen: usize,
    pub suppressed: usize,
    pub rejected_invalid: usize,
    pub rejected_reserved: usize,
    pub rejected_priority: usize,
    pub outcomes: [Option<CueOutcome>; N],
}
impl<const N: usize> FlushReport<N> {
    /// Started plus stolen admissions, including muted/zero-gain voices.
    /// See the module recipe.
    pub fn admitted(&self) -> usize {
        self.started + self.stolen
    }
    /// All mixer rejections, excluding GameAudio `None` suppression.
    /// See [`CueBatch::flush`] and the module recipe.
    pub fn rejected(&self) -> usize {
        self.rejected_invalid + self.rejected_reserved + self.rejected_priority
    }
}

/// Exclusive, thread-bound, one-frame accumulator. No implicit flush on drop.
/// See the module recipe for fixed updates, terminal feedback and lifecycle.
///
/// ```compile_fail
/// use ferravex_render::audio::{GameAudio, batch::CueBatch};
/// let mut owner = GameAudio::new();
/// let mut frame = CueBatch::<5>::new(&mut owner).unwrap();
/// owner.reset(); // Pending work prevents direct lifecycle/owner replacement.
/// frame.flush();
/// ```
///
/// ```compile_fail
/// use ferravex_render::audio::{GameAudio, batch::CueBatch};
/// let mut owner = GameAudio::new();
/// let frame = CueBatch::<5>::new(&mut owner).unwrap();
/// std::thread::scope(|scope| { scope.spawn(move || drop(frame)); });
/// ```
pub struct CueBatch<'a, const N: usize> {
    owner: &'a mut GameAudio,
    runtime: Weak<()>,
    pending: [Option<BatchedCue>; N],
    len: usize,
    counts: BatchCounts,
}

fn runtime_identity() -> Weak<()> {
    super::AUDIO.with(|cell| {
        cell.borrow()
            .as_ref()
            .map_or_else(Weak::new, |audio| Rc::downgrade(&audio.identity))
    })
}

impl<'a, const N: usize> CueBatch<'a, N> {
    /// Borrow a controller for one presentation frame. Does not initialize or
    /// replace the mixer; normally GameAudio construction has prepared its bank.
    /// Rejects capacities outside `1..=128`. See the module recipe.
    pub fn new(owner: &'a mut GameAudio) -> Result<Self, BatchError> {
        if N == 0 || N > 128 {
            return Err(BatchError::Capacity);
        }
        Ok(Self {
            owner,
            runtime: runtime_identity(),
            pending: [None; N],
            len: 0,
            counts: BatchCounts::default(),
        })
    }

    /// Pending slots (may include runtime-stale requests until reconciliation).
    pub fn len(&self) -> usize {
        self.len
    }
    /// Whether no slots are pending. See [`Self::len`].
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// Copy counters without resetting or reconciling the runtime.
    pub fn counts(&self) -> BatchCounts {
        self.counts
    }
    fn reconcile(&mut self, runtime: Weak<()>) {
        if !Weak::ptr_eq(&self.runtime, &runtime) {
            self.counts.runtime_discarded = self.counts.runtime_discarded.saturating_add(self.len);
            self.clear();
            self.runtime = runtime;
        }
    }
    fn clear(&mut self) -> usize {
        let len = self.len;
        self.pending[..len].fill(None);
        self.len = 0;
        len
    }

    /// Collect only a caller-accepted event. Validation precedes any mutation.
    /// Raw gains are not clamped; GameAudio also validates its resulting acoustic
    /// gain at flush. Distinct positions never coalesce. See the module recipe.
    /// Paused collection is permitted, but flush uses GameAudio's real suppression
    /// (nonpositional Click bypasses pause). No resume API exists on the batch:
    /// release it before resuming, so paused work cannot cross that boundary.
    #[must_use = "handle validation and the storage decision"]
    pub fn queue(
        &mut self,
        category: CueCategory,
        position: Option<Vec3>,
        gain: f32,
    ) -> Result<QueueOutcome, BatchError> {
        if !gain.is_finite() || !(0. ..=4.).contains(&gain) {
            return Err(BatchError::Gain);
        }
        if position.is_some_and(|position| !position.is_finite()) {
            return Err(BatchError::Position);
        }
        let runtime = runtime_identity();
        if Weak::ptr_eq(&self.runtime, &runtime)
            && self.pending[..self.len]
                .iter()
                .flatten()
                .any(|entry| entry.category.key == category.key && entry.category != category)
        {
            return Err(BatchError::CategoryConflict);
        }
        self.reconcile(runtime);
        let incoming = BatchedCue {
            category,
            position,
            gain,
        };
        for entry in self.pending[..self.len].iter_mut().flatten() {
            if entry.category.key == category.key && entry.position == position {
                let replaced = match category.policy {
                    Coalesce::First => false,
                    Coalesce::Strongest => gain > entry.gain,
                    Coalesce::Latest => true,
                };
                if replaced {
                    *entry = incoming;
                    self.counts.replaced = self.counts.replaced.saturating_add(1);
                }
                self.counts.coalesced = self.counts.coalesced.saturating_add(1);
                return Ok(QueueOutcome::Coalesced { replaced });
            }
        }
        let mut outcome = QueueOutcome::Retained;
        if self.len == N {
            let victim = category
                .critical
                .then(|| {
                    self.pending[..self.len]
                        .iter()
                        .position(|entry| !entry.as_ref().unwrap().category.critical)
                })
                .flatten();
            let Some(victim) = victim else {
                self.counts.dropped = self.counts.dropped.saturating_add(1);
                return Ok(QueueOutcome::Dropped);
            };
            outcome = QueueOutcome::Evicted(self.pending[victim].unwrap());
            self.pending.copy_within(victim + 1..self.len, victim);
            self.len -= 1;
            self.counts.evicted = self.counts.evicted.saturating_add(1);
        }
        self.pending[self.len] = Some(incoming);
        self.len += 1;
        self.counts.retained = self.counts.retained.saturating_add(1);
        Ok(outcome)
    }

    /// Discard pending cues without touching any controller's voices/settings.
    /// Returns discarded slots; records `cancelled`. See the module recipe.
    pub fn cancel(&mut self) -> usize {
        let discarded = self.clear();
        self.counts.cancelled = self.counts.cancelled.saturating_add(discarded);
        discarded
    }

    /// Cancel all pending cues, then pause this owner. Already-playing UI clicks
    /// and other owners' voices retain GameAudio semantics. See the module recipe.
    pub fn pause(&mut self) -> usize {
        let discarded = self.cancel();
        self.owner.pause();
        discarded
    }

    /// Cancel all pending cues, then reset this owner, preserving its pause and
    /// acoustics. Call alongside game/session restart. See the module recipe.
    pub fn reset(&mut self) -> usize {
        let discarded = self.cancel();
        self.owner.reset();
        discarded
    }

    /// Consume retained requests once, in retained order, calling exactly
    /// `cue_with_outcome` for `None` positions and `at_with_outcome` otherwise.
    /// Clears frame counters after reporting, including on an empty flush.
    /// Runtime-stale slots are discarded before any submission. Rejected or
    /// paused requests are not queued for retry. See the module recipe.
    pub fn flush(&mut self) -> FlushReport<N> {
        self.reconcile(runtime_identity());
        let mut report = FlushReport {
            batch: std::mem::take(&mut self.counts),
            submitted: self.len,
            started: 0,
            stolen: 0,
            suppressed: 0,
            rejected_invalid: 0,
            rejected_reserved: 0,
            rejected_priority: 0,
            outcomes: [None; N],
        };
        for (entry, output) in self.pending[..self.len]
            .iter_mut()
            .zip(&mut report.outcomes)
        {
            let request = entry.take().unwrap();
            let admission = match request.position {
                Some(position) => {
                    self.owner
                        .at_with_outcome(request.category.cue, position, request.gain)
                }
                None => self
                    .owner
                    .cue_with_outcome(request.category.cue, request.gain),
            };
            match admission {
                None => report.suppressed += 1,
                Some(VoiceAdmission::Started(_)) => report.started += 1,
                Some(VoiceAdmission::Stolen { .. }) => report.stolen += 1,
                Some(VoiceAdmission::Rejected(reason)) => match reason {
                    VoiceRejection::InvalidParams => report.rejected_invalid += 1,
                    VoiceRejection::ReservedForCritical => report.rejected_reserved += 1,
                    VoiceRejection::NoLowerPriority => report.rejected_priority += 1,
                },
            }
            *output = Some(CueOutcome { request, admission });
        }
        self.len = 0;
        // First submission may initialize a replacement runtime. There can be
        // no interleaved host callbacks in this synchronous, thread-bound flush.
        self.runtime = runtime_identity();
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{self, Bus, Mixer, VoiceParams, VoicePolicy, playing};

    fn category(key: u32, policy: Coalesce, critical: bool) -> CueCategory {
        CueCategory {
            key,
            cue: Cue::Jump,
            policy,
            critical,
        }
    }
    fn fresh(limit: usize) -> GameAudio {
        audio::reset();
        audio::set_muted(false);
        audio::OFFLINE.with(|v| v.set(true));
        let owner = GameAudio::new();
        audio::AUDIO.with(|cell| {
            cell.borrow_mut().as_mut().unwrap().mixer =
                Mixer::new(audio::SAMPLE_RATE, limit).unwrap();
        });
        owner
    }

    #[test]
    fn validation_is_atomic_including_category_conflicts_and_runtime_change() {
        let mut owner = fresh(8);
        assert!(matches!(
            CueBatch::<0>::new(&mut owner),
            Err(BatchError::Capacity)
        ));
        assert!(matches!(
            CueBatch::<129>::new(&mut owner),
            Err(BatchError::Capacity)
        ));
        let mut batch = CueBatch::<2>::new(&mut owner).unwrap();
        let cat = category(1, Coalesce::Strongest, false);
        assert_eq!(batch.queue(cat, None, 0.5), Ok(QueueOutcome::Retained));
        let before = batch.counts();
        for gain in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -0.01,
            4.000001,
            f32::MAX,
        ] {
            assert_eq!(batch.queue(cat, None, gain), Err(BatchError::Gain));
            assert_eq!(batch.counts(), before);
        }
        for position in [Vec3::splat(f32::NAN), Vec3::new(0., f32::INFINITY, 0.)] {
            assert_eq!(
                batch.queue(cat, Some(position), 0.5),
                Err(BatchError::Position)
            );
            assert_eq!(batch.counts(), before);
        }
        for conflict in [
            CueCategory {
                cue: Cue::Fire,
                ..cat
            },
            CueCategory {
                policy: Coalesce::Latest,
                ..cat
            },
            CueCategory {
                critical: true,
                ..cat
            },
        ] {
            assert_eq!(
                batch.queue(conflict, Some(Vec3::ZERO), 0.7),
                Err(BatchError::CategoryConflict)
            );
            assert_eq!(batch.counts(), before);
        }
        assert_eq!(batch.len(), 1);
        assert_eq!(batch.pending[0].unwrap().gain, 0.5);
        audio::reset();
        assert_eq!(batch.queue(cat, None, f32::NAN), Err(BatchError::Gain));
        assert_eq!((batch.len(), batch.counts()), (1, before));
        let report = batch.flush();
        assert_eq!((report.submitted, report.batch.runtime_discarded), (0, 1));
        // A new runtime and fresh valid request can reuse the key/configuration.
        audio::preload_cues();
        assert_eq!(
            batch.queue(
                CueCategory {
                    cue: Cue::Fire,
                    ..cat
                },
                None,
                4.
            ),
            Ok(QueueOutcome::Retained)
        );
        let report = batch.flush();
        assert_eq!((report.submitted, report.admitted()), (1, 1));
        let _ = batch.queue(cat, None, 0.5).unwrap();
        audio::reset();
        audio::preload_cues();
        // Valid collection also reconciles; a stale configuration cannot
        // conflict with a new runtime's category definition.
        assert_eq!(
            batch.queue(
                CueCategory {
                    cue: Cue::Fire,
                    ..cat
                },
                None,
                0.6
            ),
            Ok(QueueOutcome::Retained)
        );
        let report = batch.flush();
        assert_eq!((report.submitted, report.batch.runtime_discarded), (1, 1));
    }

    #[test]
    fn selection_policies_are_explicit_and_positions_never_merge() {
        let mut owner = fresh(16);
        let mut batch = CueBatch::<8>::new(&mut owner).unwrap();
        for (key, policy) in [
            (1, Coalesce::First),
            (2, Coalesce::Strongest),
            (3, Coalesce::Latest),
        ] {
            let cat = category(key, policy, false);
            assert_eq!(batch.queue(cat, None, 0.5), Ok(QueueOutcome::Retained));
            assert_eq!(
                batch.queue(cat, None, 0.9),
                Ok(QueueOutcome::Coalesced {
                    replaced: policy != Coalesce::First
                })
            );
            assert_eq!(
                batch.queue(cat, None, 0.1),
                Ok(QueueOutcome::Coalesced {
                    replaced: policy == Coalesce::Latest
                })
            );
        }
        let spatial = category(2, Coalesce::Strongest, false);
        for position in [Vec3::ZERO, Vec3::new(-2., 0., 0.), Vec3::new(2., 0., 0.)] {
            assert_eq!(
                batch.queue(spatial, Some(position), 0.4),
                Ok(QueueOutcome::Retained)
            );
            assert_eq!(
                batch.queue(spatial, Some(position), 0.4),
                Ok(QueueOutcome::Coalesced { replaced: false })
            );
        }
        let report = batch.flush();
        assert_eq!((report.submitted, report.admitted()), (6, 6));
        assert_eq!(
            (
                report.batch.retained,
                report.batch.coalesced,
                report.batch.replaced
            ),
            (6, 9, 3)
        );
        let requests: Vec<_> = report
            .outcomes
            .into_iter()
            .flatten()
            .map(|o| o.request)
            .collect();
        assert_eq!(
            [requests[0].gain, requests[1].gain, requests[2].gain],
            [0.5, 0.9, 0.1]
        );
        audio::AUDIO.with(|cell| {
            let state = cell.borrow();
            let mixer = &state.as_ref().unwrap().mixer;
            for outcome in report.outcomes.into_iter().flatten() {
                let params = mixer
                    .params(outcome.admission.unwrap().handle().unwrap())
                    .unwrap();
                assert_eq!(params.gain, outcome.request.gain);
                if let Some(position) = outcome.request.position {
                    assert_eq!(params.position, position.to_array());
                }
            }
        });
        assert_eq!(batch.counts(), BatchCounts::default());
        assert_eq!(batch.flush().submitted, 0);
    }

    #[test]
    fn critical_retention_eviction_all_critical_overflow_and_order() {
        let mut owner = fresh(16);
        let mut batch = CueBatch::<3>::new(&mut owner).unwrap();
        for (key, critical) in [(1, false), (2, true), (3, false)] {
            assert_eq!(
                batch.queue(category(key, Coalesce::First, critical), None, 0.4),
                Ok(QueueOutcome::Retained)
            );
        }
        for (key, evicted) in [(4, 1), (5, 3)] {
            let outcome = batch
                .queue(category(key, Coalesce::First, true), None, 0.4)
                .unwrap();
            assert!(matches!(outcome, QueueOutcome::Evicted(e) if e.category.key == evicted));
        }
        for critical in [false, true] {
            assert_eq!(
                batch.queue(category(6, Coalesce::First, critical), None, 4.),
                Ok(QueueOutcome::Dropped)
            );
        }
        assert_eq!(
            batch.queue(category(4, Coalesce::First, true), None, 4.),
            Ok(QueueOutcome::Coalesced { replaced: false })
        );
        let report = batch.flush();
        assert_eq!(report.batch.coalesced, 1);
        assert_eq!(
            (
                report.batch.retained,
                report.batch.evicted,
                report.batch.dropped
            ),
            (5, 2, 2)
        );
        assert_eq!(
            report.outcomes.map(|o| o.unwrap().request.category.key),
            [2, 4, 5]
        );
        assert_eq!(report.admitted(), 3);
    }

    #[test]
    fn pause_reset_cancel_drop_and_ui_click_semantics_preserve_other_owners() {
        let mut owner = fresh(16);
        let mut other = GameAudio::new();
        let other_voice = other
            .cue_with_outcome(Cue::Fire, 0.5)
            .unwrap()
            .handle()
            .unwrap();
        let own_voice = owner
            .cue_with_outcome(Cue::Jump, 0.5)
            .unwrap()
            .handle()
            .unwrap();
        let ui_voice = owner
            .cue_with_outcome(Cue::Click, 0.5)
            .unwrap()
            .handle()
            .unwrap();
        let cat = category(1, Coalesce::First, false);
        {
            let mut batch = CueBatch::<3>::new(&mut owner).unwrap();
            let _ = batch.queue(cat, None, 0.5).unwrap();
            assert_eq!(batch.pause(), 1);
            assert!(!playing(own_voice));
            assert!(playing(other_voice) && playing(ui_voice));
            assert_eq!(batch.pause(), 0);
            let click = CueCategory {
                cue: Cue::Click,
                ..cat
            };
            let _ = batch.queue(click, None, 0.5).unwrap();
            let _ = batch.queue(click, Some(Vec3::ZERO), 0.5).unwrap();
            let _ = batch
                .queue(category(2, Coalesce::First, true), None, 0.5)
                .unwrap();
            let report = batch.flush();
            assert_eq!(
                (
                    report.submitted,
                    report.admitted(),
                    report.suppressed,
                    report.rejected()
                ),
                (3, 1, 2, 0)
            );
            assert!(
                report.outcomes[0]
                    .unwrap()
                    .admission
                    .unwrap()
                    .handle()
                    .is_some()
            );
            assert_eq!(report.outcomes[1].unwrap().admission, None);
            assert_eq!(report.outcomes[2].unwrap().admission, None);
            assert_eq!(report.batch.cancelled, 1);
            let _ = batch.queue(cat, None, 0.5).unwrap();
            assert_eq!(batch.reset(), 1);
            assert_eq!(batch.flush().submitted, 0);
        }
        assert!(owner.is_paused());
        owner.resume();
        {
            let mut batch = CueBatch::<3>::new(&mut owner).unwrap();
            let _ = batch.queue(cat, None, 0.5).unwrap();
            assert_eq!(batch.cancel(), 1);
            let _ = batch.queue(cat, None, 0.5).unwrap();
            // Drop cancels without stopping voices or submitting work.
        }
        assert!(playing(other_voice) && playing(ui_voice));
        assert_eq!(audio::stats().active, 3); // Other, original UI, paused-frame UI.
        assert_eq!(CueBatch::<3>::new(&mut owner).unwrap().flush().submitted, 0);
        // Controller replacement after releasing the borrow cannot inherit cues.
        owner = GameAudio::new();
        assert_eq!(CueBatch::<3>::new(&mut owner).unwrap().flush().submitted, 0);
        assert!(playing(other_voice));
    }

    #[test]
    fn runtime_replacement_discards_queue_and_preserves_reused_voice_handles() {
        for operation in 0..3 {
            let mut old = fresh(16);
            let old_voice = old
                .cue_with_outcome(Cue::Jump, 0.5)
                .unwrap()
                .handle()
                .unwrap();
            let mut batch = CueBatch::<2>::new(&mut old).unwrap();
            let _ = batch
                .queue(category(1, Coalesce::First, false), None, 0.5)
                .unwrap();
            audio::reset();
            let mut replacement = GameAudio::new();
            let new_voice = replacement
                .cue_with_outcome(Cue::Fire, 0.5)
                .unwrap()
                .handle()
                .unwrap();
            assert_eq!(old_voice, new_voice); // Raw handle alias, different runtime.
            if operation == 0 {
                let report = batch.flush();
                assert_eq!((report.submitted, report.batch.runtime_discarded), (0, 1));
            } else if operation == 1 {
                assert_eq!(batch.pause(), 1);
            } else {
                assert_eq!(batch.reset(), 1);
            }
            assert!(playing(new_voice));
            // New work after reconciliation belongs to the current runtime.
            let _ = batch
                .queue(category(2, Coalesce::First, true), None, 0.4)
                .unwrap();
            let report = batch.flush();
            assert_eq!(
                (report.admitted(), report.suppressed),
                if operation == 1 { (0, 1) } else { (1, 0) }
            );
            drop(batch);
            drop(old);
            assert!(playing(new_voice));
        }
    }

    #[test]
    fn reports_actual_admission_steals_reservations_and_mute_independently() {
        let mut owner = fresh(2);
        audio::AUDIO.with(|cell| {
            assert!(
                cell.borrow_mut()
                    .as_mut()
                    .unwrap()
                    .mixer
                    .set_voice_policy(Some(VoicePolicy {
                        critical_priority: 5,
                        reserved_critical: 1,
                    }))
            );
        });
        let mut batch = CueBatch::<5>::new(&mut owner).unwrap();
        // Storage critical flags deliberately differ from mixer priorities.
        for (key, cue, critical) in [
            (1, Cue::Jump, false),
            (2, Cue::Success, true),
            (3, Cue::Fire, false),
            (4, Cue::Failure, true),
            (5, Cue::Fire, true),
        ] {
            let _ = batch
                .queue(
                    CueCategory {
                        key,
                        cue,
                        policy: Coalesce::First,
                        critical,
                    },
                    None,
                    0.4,
                )
                .unwrap();
        }
        let report = batch.flush();
        assert_eq!(
            (
                report.started,
                report.stolen,
                report.rejected_reserved,
                report.rejected_priority,
                report.suppressed
            ),
            (2, 1, 1, 1, 0)
        );
        assert_eq!((report.admitted(), report.rejected()), (3, 2));
        let first = report.outcomes[0]
            .unwrap()
            .admission
            .unwrap()
            .handle()
            .unwrap();
        assert_eq!(
            report.outcomes[3].unwrap().admission.unwrap().replaced(),
            Some(first)
        );
        assert!(!playing(first));
        audio::stop_all();
        audio::set_muted(true);
        let _ = batch
            .queue(category(1, Coalesce::First, true), None, 0.5)
            .unwrap();
        let report = batch.flush();
        assert_eq!(
            (report.admitted(), report.suppressed, report.rejected()),
            (1, 0, 0)
        );
        let mut pcm = [[1.; 2]; 512];
        assert!(audio::render(&mut pcm, audio::SAMPLE_RATE));
        assert!(pcm.iter().flatten().all(|v| v.is_finite() && *v == 0.));
        audio::set_muted(false);
        audio::stop_all();
        let _ = batch
            .queue(category(1, Coalesce::First, false), None, 0.)
            .unwrap();
        let _ = batch
            .queue(category(2, Coalesce::First, false), None, 4.)
            .unwrap();
        let report = batch.flush();
        assert_eq!((report.admitted(), report.rejected_reserved), (1, 1));
        assert!(audio::render(&mut pcm, audio::SAMPLE_RATE));
        assert!(pcm.iter().flatten().all(|v| v.is_finite()));
    }

    #[test]
    fn minimum_and_maximum_capacity_use_full_bound_and_reset() {
        let mut owner = fresh(128);
        let mut batch = CueBatch::<128>::new(&mut owner).unwrap();
        for key in 0..128 {
            assert_eq!(
                batch.queue(category(key, Coalesce::First, true), None, 0.1),
                Ok(QueueOutcome::Retained)
            );
        }
        assert_eq!(batch.len(), 128);
        assert_eq!(
            batch.queue(category(128, Coalesce::First, true), None, 4.),
            Ok(QueueOutcome::Dropped)
        );
        let report = batch.flush();
        assert_eq!(
            (report.submitted, report.admitted(), report.batch.dropped),
            (128, 128, 1)
        );
        assert!(report.outcomes.iter().all(Option::is_some));
        assert_eq!(batch.reset(), 0);
        assert_eq!(audio::stats().active, 0);
        let mut one = CueBatch::<1>::new(&mut owner).unwrap();
        let _ = one
            .queue(category(1, Coalesce::First, false), None, 0.1)
            .unwrap();
        assert!(matches!(
            one.queue(category(2, Coalesce::First, true), None, 0.1),
            Ok(QueueOutcome::Evicted(_))
        ));
        assert_eq!(one.flush().admitted(), 1);
    }

    #[test]
    fn acoustics_applied_once_and_spatial_click_stays_on_effects_bus() {
        let mut owner = fresh(8);
        assert!(owner.set_acoustics(audio::AcousticProfile {
            gain: 0.5,
            low_pass_hz: 1200.
        }));
        let mut batch = CueBatch::<2>::new(&mut owner).unwrap();
        let cat = CueCategory {
            cue: Cue::Click,
            ..category(1, Coalesce::Strongest, false)
        };
        let _ = batch.queue(cat, None, 0.8).unwrap();
        let _ = batch.queue(cat, Some(Vec3::new(-3., 2., 1.)), 0.8).unwrap();
        let report = batch.flush();
        audio::AUDIO.with(|cell| {
            let state = cell.borrow();
            let mixer = &state.as_ref().unwrap().mixer;
            let ui = mixer
                .params(
                    report.outcomes[0]
                        .unwrap()
                        .admission
                        .unwrap()
                        .handle()
                        .unwrap(),
                )
                .unwrap();
            let spatial = mixer
                .params(
                    report.outcomes[1]
                        .unwrap()
                        .admission
                        .unwrap()
                        .handle()
                        .unwrap(),
                )
                .unwrap();
            assert_eq!((ui.gain, ui.bus, ui.priority), (0.8, Bus::Ui, 2));
            assert_eq!(
                (
                    spatial.gain,
                    spatial.bus,
                    spatial.priority,
                    spatial.low_pass_hz
                ),
                (0.4, Bus::Effects, 3, 1200.)
            );
            assert_eq!(spatial.position, [-3., 2., 1.]);
            assert_ne!(spatial, VoiceParams::default());
        });
        let mut pcm = [[0.; 2]; 512];
        assert!(audio::render(&mut pcm, audio::SAMPLE_RATE));
        assert!(pcm.iter().flatten().all(|v| v.is_finite()));
        assert!(pcm.iter().flatten().any(|v| v.abs() > 0.001));
    }
}
