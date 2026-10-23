//! Bounded, opt-in sensor occupancy derived from accepted [`ContactEvent`]s.
//!
//! A physical pair maps to **one caller-selected direction**: a logical sensor
//! and actor. Both keys may represent a collider, body, or a game-owned ID. For
//! sensor/sensor pairs the caller chooses a direction (or uses a second tracker
//! for the reverse direction). Solid contacts must explicitly resolve to
//! [`StartResolution::Ignore`]; this module never detects or classifies contacts.
//!
//! Feed each valid outer step's events once, in world order, before removing the
//! objects needed to resolve starts. Starts cache full generational handles and
//! logical keys until the matching stop, even after explicit actor removal.
//! Duplicate starts for a retained pair do not resolve again or add references;
//! unknown/duplicate stops do nothing and report [`PairUpdate::UntrackedStop`].
//! Reversed handle order is equivalent. Different handle generations are not.
//!
//! # Input loss and reconciliation
//!
//! An incomplete batch freezes updates **before** consuming any of its events.
//! [`SensorOccupancyTracker::mark_incomplete`] does the same for a separately
//! reported loss. Retained state and queued events remain available as historical
//! observations, but occupancy is no longer authoritative. No timeout, missing
//! pair, or truncated input invents an exit. Recover with a complete caller-owned
//! [`SensorOccupancyTracker::reconcile`] snapshot, or [`SensorOccupancyTracker::reset`]
//! at a known empty baseline. Neither operation synthesizes gameplay events.
//!
//! Capacity/resolution errors reject the current event without mutating tracker
//! state. A batch may already have accepted a prefix: retry its rejected suffix
//! before later input, or mark the stream incomplete. Drain output to recover
//! event capacity. Pair capacity requires eventual stops or reconciliation;
//! explicit removal retains mappings and therefore does not free pair capacity.
//! Resolver side effects are outside this atomicity guarantee.
//!
//! The caller must report missing or reordered input. Unknown stops can also be
//! ignored solid contacts, so they cannot automatically establish loss. A stale
//! same-generation start after its stop is indistinguishable from a real reentry;
//! this tracker is not a replay/sequence-number deduplicator. Sensor classification
//! changes without lifecycle events require explicit reconciliation as well.
//!
//! All storage is inline. Construction, updates, queries, removal, reconciliation
//! and output draining allocate nothing. Pair lookup/counting is linear in
//! `PAIRS`; removal, distinct occupancy iteration and reconciliation are at most
//! quadratic. Choose modest capacities; reconciliation uses another `PAIRS`-slot
//! array on the stack. There is no wall clock, polling or simulation stepping here.
//! Caller callbacks and key comparisons own their allocation/panic behavior;
//! keys must implement stable, lawful equality. Panics do not promise rollback.
//!
//! # Runnable first-party sensor/compound recipe
//!
//! This runs with either dimensional frontend. Replace the event assertions with
//! checkpoint/pickup rules and route feedback only from accepted occupancy events.
//!
//! ```
//! # #[cfg(feature = "dim2")]
//! # use ferravex_physics::d2::*;
//! # #[cfg(all(not(feature = "dim2"), feature = "dim3"))]
//! # use ferravex_physics::d3::*;
//! use ferravex_physics::triggers::{
//!     InputCompleteness, Occupancy, OccupancyEvent, SensorOccupancyTracker,
//!     StartResolution,
//! };
//! # #[cfg(any(feature = "dim2", feature = "dim3"))] {
//! let mut world = PhysicsWorld::default();
//! let actor = world.insert_compound(RigidBodyBuilder::fixed(), [
//!     ColliderBuilder::ball(1.).translation(-Vector::X),
//!     ColliderBuilder::ball(1.).translation(Vector::X),
//! ]).unwrap();
//! let child = world.bodies[actor].colliders()[0];
//! let (_, sensor) = world.insert(
//!     RigidBodyBuilder::fixed(), ColliderBuilder::ball(3.).sensor(true),
//! );
//! let occupant = Occupancy { sensor, actor };
//! let mut tracker = SensorOccupancyTracker::<8, 4>::new();
//! world.step(1. / 120.); // Actual accepted world events, once per valid step.
//! tracker.consume_batch(world.contact_events(), InputCompleteness::Complete, |a, b| {
//!     let other = if a == sensor { b } else if b == sensor { a } else {
//!         return StartResolution::Ignore;
//!     };
//!     // This authored sensor is explicitly classified, not inferred from Started.
//!     assert!(world.colliders[sensor].is_sensor());
//!     match world.colliders.get(other).and_then(|c| c.parent()) {
//!         Some(actor) => StartResolution::Track(Occupancy { sensor, actor }),
//!         None => StartResolution::Unresolved,
//!     }
//! }).unwrap();
//! assert_eq!(tracker.active_pairs(occupant), 2);
//! assert_eq!(tracker.pop_event(), Some(OccupancyEvent::Entered(occupant)));
//! assert_eq!(tracker.pop_event(), None);
//! world.remove_collider(child).unwrap();
//! world.step(1. / 120.);
//! tracker.consume_batch(world.contact_events(), InputCompleteness::Complete,
//!     |_, _| panic!("stops use cached identities")).unwrap();
//! assert_eq!(tracker.active_pairs(occupant), 1);
//! assert_eq!(tracker.pop_event(), None);
//! assert!(world.remove_body(actor));
//! world.step(1. / 120.); // Stop still has the removed child's original generation.
//! tracker.consume_batch(world.contact_events(), InputCompleteness::Complete,
//!     |_, _| panic!("removed bodies need no lookup")).unwrap();
//! assert_eq!(tracker.pop_event(), Some(OccupancyEvent::Exited(occupant)));
//! assert_eq!(tracker.pair_count(), 0);
//! # }
//! ```

use crate::{ColliderHandle, ContactEvent, RigidBodyHandle};

/// Caller-resolved, stable logical identity. Defaults use generational physics
/// handles. See the module recipe; custom keys should include their own lifetime
/// generation if IDs can be reused before old stops arrive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Occupancy<S = ColliderHandle, A = RigidBodyHandle> {
    pub sensor: S,
    pub actor: A,
}

/// An accepted logical occupancy transition, in consumption order. Explicit
/// removal also emits exits; reset/reconciliation/input loss never do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OccupancyEvent<S = ColliderHandle, A = RigidBodyHandle> {
    Entered(Occupancy<S, A>),
    Exited(Occupancy<S, A>),
}

/// Explicit policy for a previously unknown start. Resolution is never requested
/// for stops or retained duplicate starts. Use `Unresolved` when identity or
/// classification is unavailable, rather than silently ignoring a sensor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartResolution<S = ColliderHandle, A = RigidBodyHandle> {
    Ignore,
    Track(Occupancy<S, A>),
    Unresolved,
}

/// Whether the supplied batch/snapshot contains all relevant records. `Complete`
/// is a caller assertion, not something this adapter can discover.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputCompleteness {
    Complete,
    Incomplete,
}

/// Physical pair and its single logical association for baseline reconciliation.
/// Input may use either collider order; duplicate pairs are rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedPair<S = ColliderHandle, A = RigidBodyHandle> {
    pub collider1: ColliderHandle,
    pub collider2: ColliderHandle,
    pub occupancy: Occupancy<S, A>,
}

/// Result of one accepted input event. A tracked start/stop need not produce a
/// logical transition if other children still occupy the same sensor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairUpdate {
    TrackedStart,
    DuplicateStart,
    IgnoredStart,
    TrackedStop,
    UntrackedStop,
}

/// Rejected operation. Unless documented for an incomplete batch, state and
/// pending output are unchanged; no accepted event is silently dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerError {
    PairCapacity,
    EventCapacity,
    UnresolvedStart,
    IncompleteInput,
    InvalidPair,
    DuplicatePair,
}
impl std::fmt::Display for TriggerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sensor occupancy: {self:?}")
    }
}
impl std::error::Error for TriggerError {}

/// Batch failure and the exact accepted prefix length. Retry `events[consumed..]`
/// before later input, or explicitly freeze/reconcile the tracker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatchError {
    pub consumed: usize,
    pub error: TriggerError,
}
impl std::fmt::Display for BatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} after {} events", self.error, self.consumed)
    }
}
impl std::error::Error for BatchError {}

#[derive(Clone, Copy, Debug)]
struct Entry<S, A> {
    pair: (ColliderHandle, ColliderHandle),
    occupancy: Occupancy<S, A>,
    active: bool,
}

/// Inline pair mappings and a bounded FIFO of logical events. `PAIRS` counts
/// cached physical pairs (including explicitly removed occupants awaiting stops).
/// `EVENTS` counts undrained output. Zero capacities are supported. See the module
/// recipe for the default generational-handle keys.
#[derive(Clone, Debug)]
pub struct SensorOccupancyTracker<
    const PAIRS: usize,
    const EVENTS: usize,
    S = ColliderHandle,
    A = RigidBodyHandle,
> {
    pairs: [Option<Entry<S, A>>; PAIRS],
    output: [Option<OccupancyEvent<S, A>>; EVENTS],
    output_head: usize,
    output_len: usize,
    complete: bool,
}

impl<const P: usize, const E: usize, S: Copy + Eq, A: Copy + Eq>
    SensorOccupancyTracker<P, E, S, A>
{
    /// Empty, authoritative baseline. Create before observing the world's first
    /// step, or reconcile existing overlaps before consuming subsequent deltas.
    pub const fn new() -> Self {
        Self {
            pairs: [None; P],
            output: [None; E],
            output_head: 0,
            output_len: 0,
            complete: true,
        }
    }

    /// False after reported input loss, until reset or complete reconciliation.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Cached pairs, including inactive mappings awaiting their stop events.
    pub fn pair_count(&self) -> usize {
        self.pairs.iter().flatten().count()
    }

    /// Number of accepted active child pairs for this logical occupant. After
    /// input loss this is historical retained state, not authoritative occupancy.
    pub fn active_pairs(&self, occupancy: Occupancy<S, A>) -> usize {
        self.pairs
            .iter()
            .flatten()
            .filter(|entry| entry.active && entry.occupancy == occupancy)
            .count()
    }

    /// Distinct active logical occupants in first retained slot order, without
    /// allocation. Historical only when `is_complete()` is false.
    pub fn occupants(&self) -> impl Iterator<Item = Occupancy<S, A>> + '_ {
        self.pairs.iter().enumerate().filter_map(|(i, entry)| {
            let entry = entry.as_ref().filter(|entry| entry.active)?;
            (!self.pairs[..i]
                .iter()
                .flatten()
                .any(|previous| previous.active && previous.occupancy == entry.occupancy))
            .then_some(entry.occupancy)
        })
    }

    /// Undrained logical output count.
    pub fn pending_events(&self) -> usize {
        self.output_len
    }

    /// Borrow output as copied events in FIFO order. Reading does not acknowledge
    /// it; use `pop_event()` or `clear_events()` to free output capacity.
    pub fn events(&self) -> impl ExactSizeIterator<Item = OccupancyEvent<S, A>> + '_ {
        (0..self.output_len)
            .map(|i| self.output[(self.output_head + i) % E].expect("occupied output prefix"))
    }

    /// Acknowledge and return the oldest event, freeing one output slot.
    pub fn pop_event(&mut self) -> Option<OccupancyEvent<S, A>> {
        if self.output_len == 0 {
            return None;
        }
        let event = self.output[self.output_head].take();
        self.output_head = (self.output_head + 1) % E;
        self.output_len -= 1;
        event
    }

    /// Acknowledge all output without changing occupancy or completeness.
    pub fn clear_events(&mut self) {
        self.output.fill(None);
        self.output_head = 0;
        self.output_len = 0;
    }

    /// Report dropped/truncated/reordered input. Freeze future updates/removals
    /// without clearing mappings, emitting exits, or discarding accepted output.
    pub fn mark_incomplete(&mut self) {
        self.complete = false;
    }

    /// Clear all state and queued output without emitting exits. Caller must
    /// establish a known empty baseline (for example, a scene restart).
    pub fn reset(&mut self) {
        self.pairs.fill(None);
        self.clear_events();
        self.complete = true;
    }

    /// Consume one actual world event. Resolve only unknown starts, using
    /// canonical collider order. Stops use cached keys, including after removal.
    /// One physical pair cannot change logical identity while retained.
    /// Errors leave the tracker unchanged; retry this event before later input
    /// or call `mark_incomplete()`. The callback may run before a capacity error.
    pub fn consume(
        &mut self,
        event: ContactEvent,
        resolve: impl FnOnce(ColliderHandle, ColliderHandle) -> StartResolution<S, A>,
    ) -> Result<PairUpdate, TriggerError> {
        self.require_complete()?;
        let (a, b) = match event {
            ContactEvent::Started(a, b) | ContactEvent::Stopped(a, b) => (a, b),
        };
        let pair = canonical_pair(a, b)?;
        let found = self
            .pairs
            .iter()
            .position(|entry| entry.is_some_and(|entry| entry.pair == pair));
        match event {
            ContactEvent::Started(_, _) => {
                if found.is_some() {
                    return Ok(PairUpdate::DuplicateStart);
                }
                let occupancy = match resolve(pair.0, pair.1) {
                    StartResolution::Ignore => return Ok(PairUpdate::IgnoredStart),
                    StartResolution::Track(occupancy) => occupancy,
                    StartResolution::Unresolved => return Err(TriggerError::UnresolvedStart),
                };
                let slot = self
                    .pairs
                    .iter()
                    .position(Option::is_none)
                    .ok_or(TriggerError::PairCapacity)?;
                let enter = self.active_pairs(occupancy) == 0;
                if enter && self.output_len == E {
                    return Err(TriggerError::EventCapacity);
                }
                self.pairs[slot] = Some(Entry {
                    pair,
                    occupancy,
                    active: true,
                });
                if enter {
                    self.push_event(OccupancyEvent::Entered(occupancy));
                }
                Ok(PairUpdate::TrackedStart)
            }
            ContactEvent::Stopped(_, _) => {
                let Some(slot) = found else {
                    return Ok(PairUpdate::UntrackedStop);
                };
                let entry = self.pairs[slot].expect("found pair");
                let exit = entry.active && self.active_pairs(entry.occupancy) == 1;
                if exit && self.output_len == E {
                    return Err(TriggerError::EventCapacity);
                }
                self.pairs[slot] = None;
                if exit {
                    self.push_event(OccupancyEvent::Exited(entry.occupancy));
                }
                Ok(PairUpdate::TrackedStop)
            }
        }
    }

    /// Consume a chronological batch, reporting its accepted prefix on error.
    /// Incomplete input freezes the tracker and consumes **zero** events, even
    /// when the supplied prefix looks usable. Complete empty batches do nothing.
    pub fn consume_batch(
        &mut self,
        events: &[ContactEvent],
        completeness: InputCompleteness,
        mut resolve: impl FnMut(ColliderHandle, ColliderHandle) -> StartResolution<S, A>,
    ) -> Result<usize, BatchError> {
        if completeness == InputCompleteness::Incomplete {
            self.mark_incomplete();
        }
        self.require_complete()
            .map_err(|error| BatchError { consumed: 0, error })?;
        for (consumed, &event) in events.iter().enumerate() {
            self.consume(event, &mut resolve)
                .map_err(|error| BatchError { consumed, error })?;
        }
        Ok(events.len())
    }

    /// Explicitly remove an actor's currently active occupants, emitting one exit
    /// per sensor. Preflight all output capacity before mutation. Retain inactive
    /// mappings until stops, so duplicate starts cannot resurrect removed pairs.
    /// New pairs may enter again; this is not a permanent actor ban.
    pub fn remove_actor(&mut self, actor: A) -> Result<usize, TriggerError> {
        self.remove_matching(|occupancy| occupancy.actor == actor)
    }

    /// Sensor equivalent of `remove_actor`; returns logical exits emitted.
    pub fn remove_sensor(&mut self, sensor: S) -> Result<usize, TriggerError> {
        self.remove_matching(|occupancy| occupancy.sensor == sensor)
    }

    /// Atomically replace the baseline with a caller-asserted complete snapshot
    /// of relevant physical pairs. No collision detection is performed. Success
    /// clears pending output, restores completeness and emits no events: reconcile
    /// consumer state explicitly through `occupants()`. Reject incomplete,
    /// over-capacity, self-pair or duplicate-pair snapshots without changing
    /// anything, including output and the previous completeness flag.
    ///
    /// ```
    /// use ferravex_physics::triggers::{
    ///     BatchError, InputCompleteness, SensorOccupancyTracker, StartResolution,
    ///     TriggerError,
    /// };
    /// // Game-owned keys are supported without a physics/frontend dependency.
    /// let mut tracker = SensorOccupancyTracker::<8, 4, u16, u32>::new();
    /// let result = tracker.consume_batch(&[], InputCompleteness::Incomplete,
    ///     |_, _| StartResolution::Ignore);
    /// assert_eq!(result, Err(BatchError {
    ///     consumed: 0, error: TriggerError::IncompleteInput,
    /// }));
    /// assert!(!tracker.is_complete());
    /// // This caller knows its relevant overlap set is empty. For a nonempty
    /// // baseline, supply every relevant pair as a ResolvedPair instead.
    /// tracker.reconcile(&[], InputCompleteness::Complete).unwrap();
    /// assert!(tracker.is_complete());
    /// assert_eq!(tracker.occupants().count(), 0);
    /// assert_eq!(tracker.pending_events(), 0); // No synthetic recovery events.
    /// tracker.reset(); // Appropriate for a known-empty scene restart, too.
    /// ```
    pub fn reconcile(
        &mut self,
        pairs: &[ResolvedPair<S, A>],
        completeness: InputCompleteness,
    ) -> Result<(), TriggerError> {
        if completeness != InputCompleteness::Complete {
            return Err(TriggerError::IncompleteInput);
        }
        if pairs.len() > P {
            return Err(TriggerError::PairCapacity);
        }
        let mut replacement: [Option<Entry<S, A>>; P] = [None; P];
        for (i, resolved) in pairs.iter().enumerate() {
            let pair = canonical_pair(resolved.collider1, resolved.collider2)?;
            if replacement[..i]
                .iter()
                .flatten()
                .any(|entry| entry.pair == pair)
            {
                return Err(TriggerError::DuplicatePair);
            }
            replacement[i] = Some(Entry {
                pair,
                occupancy: resolved.occupancy,
                active: true,
            });
        }
        self.pairs = replacement;
        self.clear_events();
        self.complete = true;
        Ok(())
    }

    fn require_complete(&self) -> Result<(), TriggerError> {
        if self.complete {
            Ok(())
        } else {
            Err(TriggerError::IncompleteInput)
        }
    }

    fn push_event(&mut self, event: OccupancyEvent<S, A>) {
        debug_assert!(self.output_len < E);
        self.output[(self.output_head + self.output_len) % E] = Some(event);
        self.output_len += 1;
    }

    fn remove_matching(
        &mut self,
        matches: impl Fn(Occupancy<S, A>) -> bool,
    ) -> Result<usize, TriggerError> {
        self.require_complete()?;
        let exits = self
            .occupants()
            .filter(|&occupancy| matches(occupancy))
            .count();
        if exits > E - self.output_len {
            return Err(TriggerError::EventCapacity);
        }
        for i in 0..P {
            if let Some(entry) =
                self.pairs[i].filter(|entry| entry.active && matches(entry.occupancy))
            {
                self.pairs[i].as_mut().expect("matched pair").active = false;
                if self.active_pairs(entry.occupancy) == 0 {
                    self.push_event(OccupancyEvent::Exited(entry.occupancy));
                }
            }
        }
        Ok(exits)
    }
}

impl<const P: usize, const E: usize, S: Copy + Eq, A: Copy + Eq> Default
    for SensorOccupancyTracker<P, E, S, A>
{
    fn default() -> Self {
        Self::new()
    }
}

fn canonical_pair(
    a: ColliderHandle,
    b: ColliderHandle,
) -> Result<(ColliderHandle, ColliderHandle), TriggerError> {
    if a == b {
        Err(TriggerError::InvalidPair)
    } else if a < b {
        Ok((a, b))
    } else {
        Ok((b, a))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Tracker<const P: usize = 4, const E: usize = 4> = SensorOccupancyTracker<P, E, u8, u16>;

    const ONE: Occupancy<u8, u16> = Occupancy {
        sensor: 1,
        actor: 10,
    };
    const TWO: Occupancy<u8, u16> = Occupancy {
        sensor: 2,
        actor: 10,
    };
    const OTHER: Occupancy<u8, u16> = Occupancy {
        sensor: 1,
        actor: 20,
    };

    fn handle(index: usize, generation: u32) -> ColliderHandle {
        ColliderHandle { index, generation }
    }

    fn start(n: usize) -> ContactEvent {
        ContactEvent::Started(handle(0, 0), handle(n, 0))
    }

    fn stop(n: usize) -> ContactEvent {
        ContactEvent::Stopped(handle(0, 0), handle(n, 0))
    }

    fn no_resolution(_: ColliderHandle, _: ColliderHandle) -> StartResolution<u8, u16> {
        panic!("must not resolve retained pairs or stops")
    }

    fn resolved(n: usize, occupancy: Occupancy<u8, u16>) -> ResolvedPair<u8, u16> {
        ResolvedPair {
            collider1: handle(0, 0),
            collider2: handle(n, 0),
            occupancy,
        }
    }

    #[test]
    fn two_children_duplicates_and_reversed_pairs_emit_one_enter_and_final_exit() {
        let mut tracker = Tracker::<4, 4>::new();
        assert_eq!(
            tracker.consume(start(1), |_, _| StartResolution::Track(ONE)),
            Ok(PairUpdate::TrackedStart)
        );
        assert_eq!(
            tracker.consume(start(2), |_, _| StartResolution::Track(ONE)),
            Ok(PairUpdate::TrackedStart)
        );
        assert_eq!(
            tracker.consume(start(1), no_resolution),
            Ok(PairUpdate::DuplicateStart)
        );
        assert_eq!(
            tracker.consume(
                ContactEvent::Started(handle(2, 0), handle(0, 0)),
                no_resolution
            ),
            Ok(PairUpdate::DuplicateStart)
        );
        assert_eq!(tracker.active_pairs(ONE), 2);
        assert_eq!(tracker.occupants().collect::<Vec<_>>(), [ONE]);
        assert_eq!(tracker.pop_event(), Some(OccupancyEvent::Entered(ONE)));
        assert_eq!(tracker.pop_event(), None);
        tracker.consume(stop(1), no_resolution).unwrap();
        assert_eq!(
            tracker.consume(stop(1), no_resolution),
            Ok(PairUpdate::UntrackedStop)
        );
        assert_eq!(tracker.pending_events(), 0);
        tracker
            .consume(
                ContactEvent::Stopped(handle(2, 0), handle(0, 0)),
                no_resolution,
            )
            .unwrap();
        assert_eq!(tracker.pop_event(), Some(OccupancyEvent::Exited(ONE)));
        assert_eq!(tracker.pair_count(), 0);
        assert_eq!(tracker.active_pairs(ONE), 0);
    }

    #[test]
    fn full_generations_distinguish_late_stops_and_new_actor_instances() {
        let mut tracker = SensorOccupancyTracker::<4, 4>::new();
        let sensor = handle(0, 0);
        let old = handle(1, 3);
        let new = handle(1, 4);
        let previous = Occupancy {
            sensor,
            actor: RigidBodyHandle {
                index: 1,
                generation: 5,
            },
        };
        let next = Occupancy {
            sensor,
            actor: RigidBodyHandle {
                index: 1,
                generation: 6,
            },
        };
        tracker
            .consume(ContactEvent::Started(old, sensor), |a, b| {
                assert_eq!((a, b), (sensor, old));
                StartResolution::Track(previous)
            })
            .unwrap();
        // World order: new starts can precede removed-generation stops.
        tracker
            .consume(ContactEvent::Started(sensor, new), |_, _| {
                StartResolution::Track(next)
            })
            .unwrap();
        tracker
            .consume(ContactEvent::Stopped(sensor, old), |_, _| {
                panic!("stop lookup")
            })
            .unwrap();
        assert_eq!(tracker.active_pairs(previous), 0);
        assert_eq!(tracker.active_pairs(next), 1);
        assert_eq!(
            tracker.consume(ContactEvent::Stopped(old, sensor), |_, _| panic!(
                "stale stop"
            )),
            Ok(PairUpdate::UntrackedStop)
        );
        assert_eq!(
            tracker.events().collect::<Vec<_>>(),
            [
                OccupancyEvent::Entered(previous),
                OccupancyEvent::Entered(next),
                OccupancyEvent::Exited(previous),
            ]
        );
        tracker.clear_events();
        tracker
            .consume(ContactEvent::Stopped(new, sensor), |_, _| {
                panic!("stop lookup")
            })
            .unwrap();
        assert_eq!(tracker.pop_event(), Some(OccupancyEvent::Exited(next)));
    }

    #[test]
    fn unresolved_and_invalid_pairs_reject_without_mutation_or_silent_classification() {
        let mut tracker = Tracker::<0, 0>::new();
        assert_eq!(
            tracker.consume(start(1), |_, _| StartResolution::Ignore),
            Ok(PairUpdate::IgnoredStart)
        );
        assert_eq!(
            tracker.consume(stop(1), no_resolution),
            Ok(PairUpdate::UntrackedStop)
        );
        assert_eq!(
            tracker.consume(start(1), |_, _| StartResolution::Unresolved),
            Err(TriggerError::UnresolvedStart)
        );
        for event in [
            ContactEvent::Started(handle(1, 0), handle(1, 0)),
            ContactEvent::Stopped(handle(1, 0), handle(1, 0)),
        ] {
            assert_eq!(
                tracker.consume(event, no_resolution),
                Err(TriggerError::InvalidPair)
            );
        }
        assert!(tracker.is_complete());
        assert_eq!(tracker.pair_count(), 0);
        assert_eq!(tracker.events().len(), 0);
        assert_eq!(tracker.pop_event(), None);
        assert_eq!(tracker.remove_actor(10), Ok(0));
        tracker.clear_events();
    }

    #[test]
    fn zero_capacities_reject_tracking_and_keep_a_retryable_empty_baseline() {
        let mut no_pairs = Tracker::<0, 2>::new();
        assert_eq!(
            no_pairs.consume(start(1), |_, _| StartResolution::Track(ONE)),
            Err(TriggerError::PairCapacity)
        );
        let mut no_events = Tracker::<2, 0>::new();
        assert_eq!(
            no_events.consume(start(1), |_, _| StartResolution::Track(ONE)),
            Err(TriggerError::EventCapacity)
        );
        assert_eq!(no_pairs.pair_count(), 0);
        assert_eq!(no_events.pair_count(), 0);
        // A reconciled baseline needs no events and can retain a pair at E = 0.
        no_events
            .reconcile(&[resolved(1, ONE)], InputCompleteness::Complete)
            .unwrap();
        assert_eq!(
            no_events.consume(stop(1), no_resolution),
            Err(TriggerError::EventCapacity)
        );
        assert_eq!(no_events.remove_actor(10), Err(TriggerError::EventCapacity));
        assert_eq!(no_events.active_pairs(ONE), 1);
    }

    #[test]
    fn output_pressure_rejects_only_transitions_and_retry_preserves_counts() {
        let mut tracker = Tracker::<4, 1>::new();
        tracker
            .consume(start(1), |_, _| StartResolution::Track(ONE))
            .unwrap();
        // Adding/stopping a nonfinal child needs no output slot.
        tracker
            .consume(start(2), |_, _| StartResolution::Track(ONE))
            .unwrap();
        tracker.consume(stop(2), no_resolution).unwrap();
        assert_eq!(
            tracker.consume(start(3), |_, _| StartResolution::Track(OTHER)),
            Err(TriggerError::EventCapacity)
        );
        assert_eq!(
            tracker.consume(stop(1), no_resolution),
            Err(TriggerError::EventCapacity)
        );
        assert_eq!(tracker.active_pairs(ONE), 1);
        assert_eq!(tracker.active_pairs(OTHER), 0);
        assert_eq!(tracker.pair_count(), 1);
        assert_eq!(tracker.pop_event(), Some(OccupancyEvent::Entered(ONE)));
        tracker.consume(stop(1), no_resolution).unwrap();
        assert_eq!(tracker.pop_event(), Some(OccupancyEvent::Exited(ONE)));
        tracker
            .consume(start(3), |_, _| StartResolution::Track(OTHER))
            .unwrap();
        assert_eq!(tracker.pop_event(), Some(OccupancyEvent::Entered(OTHER)));
    }

    #[test]
    fn pair_overflow_rejects_atomically_and_reuses_a_stopped_slot() {
        let mut tracker = Tracker::<1, 4>::new();
        tracker
            .consume(start(1), |_, _| StartResolution::Track(ONE))
            .unwrap();
        assert_eq!(
            tracker.consume(start(2), |_, _| StartResolution::Track(ONE)),
            Err(TriggerError::PairCapacity)
        );
        assert_eq!(
            tracker.events().collect::<Vec<_>>(),
            [OccupancyEvent::Entered(ONE)]
        );
        assert_eq!(tracker.active_pairs(ONE), 1);
        tracker.consume(stop(1), no_resolution).unwrap();
        tracker
            .consume(start(2), |_, _| StartResolution::Track(ONE))
            .unwrap();
        assert_eq!(
            tracker.events().collect::<Vec<_>>(),
            [
                OccupancyEvent::Entered(ONE),
                OccupancyEvent::Exited(ONE),
                OccupancyEvent::Entered(ONE)
            ]
        );
        assert_eq!(tracker.active_pairs(ONE), 1);
    }

    #[test]
    fn batch_failure_reports_the_accepted_prefix_and_supports_suffix_retry() {
        let mut tracker = Tracker::<4, 1>::new();
        let events = [start(1), start(2)];
        let failure = tracker
            .consume_batch(&events, InputCompleteness::Complete, |_, b| {
                StartResolution::Track(if b.index == 1 { ONE } else { OTHER })
            })
            .unwrap_err();
        assert_eq!(
            failure,
            BatchError {
                consumed: 1,
                error: TriggerError::EventCapacity
            }
        );
        assert!(tracker.is_complete());
        assert_eq!(tracker.pair_count(), 1);
        assert_eq!(tracker.pop_event(), Some(OccupancyEvent::Entered(ONE)));
        assert_eq!(
            tracker.consume_batch(
                &events[failure.consumed..],
                InputCompleteness::Complete,
                |_, _| StartResolution::Track(OTHER)
            ),
            Ok(1)
        );
        assert_eq!(tracker.active_pairs(OTHER), 1);
        assert_eq!(tracker.pop_event(), Some(OccupancyEvent::Entered(OTHER)));
        assert_eq!(
            tracker.consume_batch(&[], InputCompleteness::Complete, no_resolution),
            Ok(0)
        );
    }

    #[test]
    fn incomplete_input_freezes_before_any_prefix_and_never_invents_an_exit() {
        let mut tracker = Tracker::<4, 4>::new();
        tracker
            .consume(start(1), |_, _| StartResolution::Track(ONE))
            .unwrap();
        assert_eq!(
            tracker.consume_batch(&[stop(1)], InputCompleteness::Incomplete, no_resolution),
            Err(BatchError {
                consumed: 0,
                error: TriggerError::IncompleteInput
            })
        );
        assert!(!tracker.is_complete());
        assert_eq!(tracker.active_pairs(ONE), 1);
        assert_eq!(tracker.pair_count(), 1);
        assert_eq!(
            tracker.events().collect::<Vec<_>>(),
            [OccupancyEvent::Entered(ONE)]
        );
        assert_eq!(
            tracker.consume(stop(1), no_resolution),
            Err(TriggerError::IncompleteInput)
        );
        assert_eq!(tracker.remove_actor(10), Err(TriggerError::IncompleteInput));
        assert_eq!(tracker.remove_sensor(1), Err(TriggerError::IncompleteInput));
        assert_eq!(
            tracker.consume_batch(&[], InputCompleteness::Complete, no_resolution),
            Err(BatchError {
                consumed: 0,
                error: TriggerError::IncompleteInput
            })
        );
        tracker.reconcile(&[], InputCompleteness::Complete).unwrap();
        assert!(tracker.is_complete());
        assert_eq!(tracker.pair_count(), 0);
        assert_eq!(tracker.pop_event(), None); // No synthetic exit, old output acknowledged.
    }

    #[test]
    fn bad_reconciliation_snapshots_are_atomic_and_success_installs_a_quiet_baseline() {
        let mut tracker = Tracker::<2, 4>::new();
        tracker
            .consume(start(1), |_, _| StartResolution::Track(ONE))
            .unwrap();
        tracker.mark_incomplete();
        let p = resolved(2, OTHER);
        let reverse = ResolvedPair {
            collider1: p.collider2,
            collider2: p.collider1,
            ..p
        };
        let invalid = ResolvedPair {
            collider2: p.collider1,
            ..p
        };
        for (pairs, completeness, error) in [
            (
                vec![p],
                InputCompleteness::Incomplete,
                TriggerError::IncompleteInput,
            ),
            (
                vec![p, resolved(3, ONE), resolved(4, TWO)],
                InputCompleteness::Complete,
                TriggerError::PairCapacity,
            ),
            (
                vec![p, reverse],
                InputCompleteness::Complete,
                TriggerError::DuplicatePair,
            ),
            (
                vec![invalid],
                InputCompleteness::Complete,
                TriggerError::InvalidPair,
            ),
        ] {
            assert_eq!(tracker.reconcile(&pairs, completeness), Err(error));
            assert!(!tracker.is_complete());
            assert_eq!(tracker.active_pairs(ONE), 1);
            assert_eq!(
                tracker.events().collect::<Vec<_>>(),
                [OccupancyEvent::Entered(ONE)]
            );
        }
        tracker
            .reconcile(&[p, resolved(3, OTHER)], InputCompleteness::Complete)
            .unwrap();
        assert!(tracker.is_complete());
        assert_eq!(tracker.active_pairs(ONE), 0);
        assert_eq!(tracker.active_pairs(OTHER), 2);
        assert_eq!(tracker.occupants().collect::<Vec<_>>(), [OTHER]);
        assert_eq!(tracker.pending_events(), 0);
        tracker.consume(stop(2), no_resolution).unwrap();
        assert_eq!(tracker.pending_events(), 0);
        tracker.consume(stop(3), no_resolution).unwrap();
        assert_eq!(tracker.pop_event(), Some(OccupancyEvent::Exited(OTHER)));
    }

    #[test]
    fn explicit_actor_removal_preflights_all_exits_and_retains_stop_identities() {
        let mut tracker = Tracker::<4, 3>::new();
        tracker
            .consume(start(1), |_, _| StartResolution::Track(ONE))
            .unwrap();
        tracker
            .consume(start(2), |_, _| StartResolution::Track(ONE))
            .unwrap();
        tracker
            .consume(start(3), |_, _| StartResolution::Track(TWO))
            .unwrap();
        assert_eq!(tracker.remove_actor(10), Err(TriggerError::EventCapacity));
        assert_eq!(tracker.active_pairs(ONE), 2);
        assert_eq!(tracker.active_pairs(TWO), 1);
        assert_eq!(tracker.pending_events(), 2);
        tracker.clear_events();
        assert_eq!(tracker.remove_actor(10), Ok(2));
        assert_eq!(
            tracker.events().collect::<Vec<_>>(),
            [OccupancyEvent::Exited(ONE), OccupancyEvent::Exited(TWO)]
        );
        assert_eq!(tracker.occupants().count(), 0);
        assert_eq!(tracker.pair_count(), 3);
        assert_eq!(tracker.remove_actor(10), Ok(0));
        assert_eq!(
            tracker.consume(start(1), no_resolution),
            Ok(PairUpdate::DuplicateStart)
        );
        // Retained removals must not prevent new physical pairs entering later.
        tracker
            .consume(start(4), |_, _| StartResolution::Track(ONE))
            .unwrap();
        for n in 1..=3 {
            tracker.consume(stop(n), no_resolution).unwrap();
        }
        assert_eq!(tracker.pair_count(), 1);
        assert_eq!(tracker.active_pairs(ONE), 1);
        assert_eq!(tracker.pending_events(), 3); // Late stops emitted nothing.
        tracker.clear_events();
        tracker.consume(stop(4), no_resolution).unwrap();
        assert_eq!(tracker.pop_event(), Some(OccupancyEvent::Exited(ONE)));
    }

    #[test]
    fn removed_pairs_keep_capacity_until_stop_and_sensor_removal_keeps_other_sensors() {
        let mut tracker = Tracker::<3, 4>::new();
        tracker
            .consume(start(1), |_, _| StartResolution::Track(ONE))
            .unwrap();
        tracker
            .consume(start(2), |_, _| StartResolution::Track(OTHER))
            .unwrap();
        tracker
            .consume(start(3), |_, _| StartResolution::Track(TWO))
            .unwrap();
        tracker.clear_events();
        assert_eq!(tracker.remove_sensor(1), Ok(2));
        assert_eq!(tracker.active_pairs(TWO), 1);
        assert_eq!(tracker.active_pairs(ONE), 0);
        assert_eq!(tracker.active_pairs(OTHER), 0);
        assert_eq!(
            tracker.consume(start(4), |_, _| StartResolution::Track(OTHER)),
            Err(TriggerError::PairCapacity)
        );
        tracker.consume(stop(1), no_resolution).unwrap();
        tracker
            .consume(start(4), |_, _| StartResolution::Track(OTHER))
            .unwrap();
        tracker.consume(stop(2), no_resolution).unwrap();
        assert_eq!(tracker.active_pairs(OTHER), 1);
        assert_eq!(
            tracker.events().collect::<Vec<_>>(),
            [
                OccupancyEvent::Exited(ONE),
                OccupancyEvent::Exited(OTHER),
                OccupancyEvent::Entered(OTHER)
            ]
        );
    }

    #[test]
    fn output_fifo_wraps_and_reset_clears_pairs_pending_output_and_loss_state() {
        let mut tracker = Tracker::<4, 2>::default();
        tracker
            .consume(start(1), |_, _| StartResolution::Track(ONE))
            .unwrap();
        tracker
            .consume(start(2), |_, _| StartResolution::Track(OTHER))
            .unwrap();
        assert_eq!(tracker.pop_event(), Some(OccupancyEvent::Entered(ONE)));
        tracker.consume(stop(1), no_resolution).unwrap();
        assert_eq!(tracker.events().len(), 2);
        assert_eq!(
            tracker.events().collect::<Vec<_>>(),
            [OccupancyEvent::Entered(OTHER), OccupancyEvent::Exited(ONE)]
        );
        tracker.mark_incomplete();
        tracker.reset();
        assert!(tracker.is_complete());
        assert_eq!(tracker.pending_events(), 0);
        assert_eq!(tracker.pair_count(), 0);
        assert_eq!(tracker.occupants().count(), 0);
        tracker
            .consume(start(1), |_, _| StartResolution::Track(ONE))
            .unwrap();
        assert_eq!(tracker.pop_event(), Some(OccupancyEvent::Entered(ONE)));
    }

    #[test]
    fn unknown_stop_is_not_an_exit_and_reordered_or_lost_input_requires_caller_report() {
        let mut tracker = Tracker::<4, 4>::new();
        assert_eq!(
            tracker.consume(stop(1), no_resolution),
            Ok(PairUpdate::UntrackedStop)
        );
        assert_eq!(tracker.pending_events(), 0);
        tracker
            .consume(start(1), |_, _| StartResolution::Track(ONE))
            .unwrap();
        tracker.mark_incomplete(); // Caller discovers that the stop preceded its start.
        assert_eq!(
            tracker.consume(stop(1), no_resolution),
            Err(TriggerError::IncompleteInput)
        );
        assert_eq!(tracker.active_pairs(ONE), 1);
        assert_eq!(
            tracker.events().collect::<Vec<_>>(),
            [OccupancyEvent::Entered(ONE)]
        );
        // Complete caller snapshot repairs an omitted child start without an enter.
        tracker
            .reconcile(
                &[resolved(1, ONE), resolved(2, ONE)],
                InputCompleteness::Complete,
            )
            .unwrap();
        tracker.consume(stop(1), no_resolution).unwrap();
        assert_eq!(tracker.pending_events(), 0);
        tracker.consume(stop(2), no_resolution).unwrap();
        assert_eq!(tracker.pop_event(), Some(OccupancyEvent::Exited(ONE)));
        // An actual reentry may reuse exactly the same collider handles.
        tracker
            .consume(start(1), |_, _| StartResolution::Track(ONE))
            .unwrap();
        assert_eq!(tracker.pop_event(), Some(OccupancyEvent::Entered(ONE)));
    }

    macro_rules! world_tests {
        ($name:ident, $dimension:ident) => {
            #[test]
            fn $name() {
                use crate::$dimension::{ColliderBuilder, PhysicsWorld, RigidBodyBuilder, Vector};
                let mut world = PhysicsWorld::default();
                let actor = world
                    .insert_compound(
                        RigidBodyBuilder::fixed(),
                        [
                            ColliderBuilder::ball(1.).translation(-Vector::X),
                            ColliderBuilder::ball(1.).translation(Vector::X),
                        ],
                    )
                    .unwrap();
                let child = world.bodies[actor].colliders()[0];
                let (_, sensor) = world.insert(
                    RigidBodyBuilder::fixed(),
                    ColliderBuilder::ball(3.).sensor(true),
                );
                let mut tracker = SensorOccupancyTracker::<8, 4>::new();
                let occupancy = Occupancy { sensor, actor };
                world.step(1. / 120.);
                assert_eq!(world.contact_events().len(), 2);
                tracker
                    .consume_batch(
                        world.contact_events(),
                        InputCompleteness::Complete,
                        |a, b| {
                            assert!(world.colliders[sensor].is_sensor());
                            let other = if a == sensor {
                                b
                            } else if b == sensor {
                                a
                            } else {
                                return StartResolution::Ignore;
                            };
                            StartResolution::Track(Occupancy {
                                sensor,
                                actor: world.colliders[other].parent().unwrap(),
                            })
                        },
                    )
                    .unwrap();
                assert_eq!(
                    tracker.pop_event(),
                    Some(OccupancyEvent::Entered(occupancy))
                );
                assert_eq!(tracker.active_pairs(occupancy), 2);
                world.remove_collider(child).unwrap();
                world.step(1. / 120.);
                tracker
                    .consume_batch(
                        world.contact_events(),
                        InputCompleteness::Complete,
                        |_, _| panic!("stop resolution"),
                    )
                    .unwrap();
                assert_eq!(tracker.active_pairs(occupancy), 1);
                assert_eq!(tracker.pending_events(), 0);
                assert!(world.remove_body(actor));
                // Reuse a removed body/collider slot before the old generation's stop.
                let (fresh, _) = world.insert(RigidBodyBuilder::fixed(), ColliderBuilder::ball(1.));
                let next = Occupancy {
                    sensor,
                    actor: fresh,
                };
                assert_ne!(actor, fresh);
                assert!(world.bodies.get(actor).is_none());
                world.step(1. / 120.);
                assert_eq!(world.contact_events().len(), 2);
                tracker
                    .consume_batch(
                        world.contact_events(),
                        InputCompleteness::Complete,
                        |a, b| {
                            let other = if a == sensor { b } else { a };
                            StartResolution::Track(Occupancy {
                                sensor,
                                actor: world.colliders[other].parent().unwrap(),
                            })
                        },
                    )
                    .unwrap();
                assert_eq!(
                    tracker.events().collect::<Vec<_>>(),
                    [
                        OccupancyEvent::Entered(next),
                        OccupancyEvent::Exited(occupancy)
                    ]
                );
                assert_eq!(tracker.active_pairs(occupancy), 0);
                assert_eq!(tracker.active_pairs(next), 1);
                tracker.clear_events();
                assert!(world.remove_body(fresh));
                world.step(1. / 120.);
                tracker
                    .consume_batch(
                        world.contact_events(),
                        InputCompleteness::Complete,
                        |_, _| panic!("removed-generation lookup"),
                    )
                    .unwrap();
                assert_eq!(tracker.pop_event(), Some(OccupancyEvent::Exited(next)));
                assert_eq!(tracker.pair_count(), 0);
            }
        };
    }

    #[cfg(feature = "dim2")]
    world_tests!(real_sensor_compound_and_removed_generations_dim2, d2);
    #[cfg(feature = "dim3")]
    world_tests!(real_sensor_compound_and_removed_generations_dim3, d3);
}
