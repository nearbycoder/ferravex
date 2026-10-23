//! Two retained simulation values and caller-selected presentation interpolation.
//!
//! Commit once **after each actual simulation update**, including every update in
//! a catch-up frame. A zero-tick frame only samples. Call [`InterpolationPair::snap`](crate::interpolation::InterpolationPair::snap)
//! for an accepted teleport, respawn or restart, replacing both endpoints and
//! advancing a local discontinuity generation. No clock, input, tick scheduling,
//! replay format or network history is owned here; longer histories still belong
//! to [`crate::snapshots`] and [`crate::replay::ghost`].
//!
//! A presentation hold retains the last sampled alpha and the existing endpoints,
//! so an inactive session's zero alpha cannot rewind the picture. Explicit resume,
//! a committed update or a snap releases the hold. Thus a local presentation hold
//! cannot prevent authoritative simulation updates. To freeze an independent image
//! while a remote match keeps updating, retain that image separately in the host.
//!
//! # Local session: zero ticks, catch-up, pause, resume and teleport
//!
//! This runnable recipe injects elapsed time into the existing session. The host
//! still owns input consumption, update success and lifecycle transitions.
//!
//! ```
//! use ferravex_engine::{
//!     interpolation::{InterpolationError, InterpolationPair},
//!     math::Vec2,
//!     session::GameSession,
//! };
//!
//! let mut session = GameSession::new(64.0, 4, true);
//! let mut position = Vec2::ZERO;
//! let mut history = InterpolationPair::new(position);
//! let blend = |a: &Vec2, b: &Vec2, t| a.lerp(*b, t);
//!
//! let frame = session.advance(0.5 / 64.0);
//! assert_eq!(frame.steps, 0); // Do not commit or consume gameplay edges here.
//! assert_eq!(history.sample_with(frame.alpha, blend)?, Vec2::ZERO);
//!
//! let frame = session.advance(2.0 / 64.0);
//! for _dt in frame.ticks() {
//!     if !session.is_playing() { break; } // A prior update could finish the game.
//!     position.x += 1.0; // Execute rules and consume tick input here.
//!     history.commit(position); // Only the accepted, actually executed update.
//! }
//! assert_eq!(*history.previous(), Vec2::new(1.0, 0.0));
//! assert_eq!(*history.current(), Vec2::new(2.0, 0.0));
//! let visible = history.sample_with(frame.alpha, blend)?;
//! assert_eq!(visible, Vec2::new(1.5, 0.0));
//!
//! session.set_paused(true);
//! history.hold(); // Capture the last successful sample's fraction (0.5).
//! let inactive = session.advance(30.0);
//! assert_eq!((inactive.steps, inactive.alpha), (0, 0.0));
//! assert_eq!(history.sample_with(inactive.alpha, blend)?, visible);
//!
//! session.set_paused(false);
//! history.resume();
//! let resumed = session.advance(0.0); // Retained session fraction, no new tick.
//! assert_eq!(resumed.steps, 0);
//! assert_eq!(history.sample_with(resumed.alpha, blend)?, visible);
//!
//! position = Vec2::new(100.0, 20.0); // The simulation accepted a teleport.
//! assert_eq!(history.snap(position), 1);
//! assert_eq!(history.sample_with(0.0, blend)?, position); // No blend across it.
//! session.restart();
//! position = Vec2::ZERO; // Reset the simulation as well as presentation.
//! assert_eq!(history.snap(position), 2);
//! assert_eq!(history.sample_with(0.75, blend)?, position);
//! # Ok::<(), InterpolationError>(())
//! ```
//!
//! # A composed Vec3 / Quat pose
//!
//! Use an existing pose type or a tuple; no second transform type is required.
//! Position uses linear interpolation and rotation uses the existing shortest-arc
//! quaternion interpolation. Value validity and interpolation policy belong to the
//! caller, including normalizing/validating rotations before committing them.
//!
//! ```
//! use ferravex_engine::{interpolation::InterpolationPair, math::{Quat, Vec3}};
//! use std::f32::consts::FRAC_PI_2;
//!
//! let mut pose = InterpolationPair::new((Vec3::ZERO, Quat::IDENTITY));
//! let turn = Quat::from_rotation_y(FRAC_PI_2);
//! pose.commit((Vec3::new(8.0, 4.0, 2.0), -turn)); // q and -q are equivalent.
//! let (position, rotation) = pose.sample_with(0.5, |a, b, t| {
//!     (a.0.lerp(b.0, t), a.1.slerp(b.1, t))
//! })?;
//! assert_eq!(position, Vec3::new(4.0, 2.0, 1.0));
//! let expected = Quat::from_rotation_y(FRAC_PI_2 * 0.5);
//! assert!(rotation.try_angle_between(expected).unwrap() < 1e-6);
//! # Ok::<(), ferravex_engine::interpolation::InterpolationError>(())
//! ```

/// Rejected presentation input; endpoints, alpha, hold and generation are unchanged.
///
/// See [`InterpolationPair::sample_with`] for a runnable invalid-input example.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterpolationError {
    /// Alpha is nonfinite or outside the inclusive range `[0, 1]`.
    InvalidAlpha,
}

impl std::fmt::Display for InterpolationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAlpha => f.write_str("interpolation alpha must be finite and in [0, 1]"),
        }
    }
}

impl std::error::Error for InterpolationError {}

/// Exactly two values, a presentation fraction, a hold flag and a local generation.
///
/// The pair never allocates its own storage or retains a sampler/output. Construction
/// and snapping clone one value; commits move values without cloning. Allocations,
/// side effects or panics in `T::clone`, `T::drop` or the supplied interpolator are
/// caller-owned. Use small value poses for allocation-free presentation.
///
/// [`sample_with`](Self::sample_with) records its effective alpha for
/// [`hold`](Self::hold); it does not advance either endpoint. A pure interpolator
/// and stable values are needed to reproduce the same image while held.
/// See the [module recipes](self) for Vec2 and Vec3/Quat consumers.
#[derive(Clone, Debug, PartialEq)]
pub struct InterpolationPair<T> {
    previous: T,
    current: T,
    alpha: f32,
    held: bool,
    generation: u64,
}

impl<T> InterpolationPair<T> {
    /// Initialize both endpoints from one value, unheld, at alpha 1 and generation 0.
    ///
    /// Clones `value` once; see the [module recipes](self) for construction and use.
    pub fn new(value: T) -> Self
    where
        T: Clone,
    {
        Self {
            previous: value.clone(),
            current: value,
            alpha: 1.0,
            held: false,
            generation: 0,
        }
    }

    /// The preceding committed value. Initially, and after a snap, both agree.
    /// See [`Self::commit`] for a runnable history example.
    pub fn previous(&self) -> &T {
        &self.previous
    }

    /// The most recently committed value; see [`Self::commit`].
    pub fn current(&self) -> &T {
        &self.current
    }

    /// Last successful sample's effective fraction, or 1 after new/commit/snap.
    ///
    /// This fraction applies to the current pair only. Related presentation such
    /// as hazard time can use it after sampling. See [`Self::hold`] for an example.
    pub fn alpha(&self) -> f32 {
        self.alpha
    }

    /// Whether samples use the retained alpha; see [`Self::hold`].
    pub fn is_held(&self) -> bool {
        self.held
    }

    /// Count explicit snaps since construction, wrapping from `u64::MAX` to 0.
    ///
    /// Compare for change to invalidate presentation caches; this is neither a
    /// tick ID nor a globally unique identity. Clones copy the generation, new
    /// pairs start at 0, and a full wrap can alias an old value. Ordinary commits,
    /// samples, holds and resumes do not change it. See [`Self::snap`].
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Shift current to previous and retain an actual update's new value.
    ///
    /// Drops the oldest value without cloning. Call after each accepted update,
    /// never just because a tick was scheduled or a render frame occurred. A
    /// catch-up batch keeps only its last two executed states. Holds are released
    /// and alpha becomes 1, so holding before the next sample selects this newest
    /// committed state. A commit does not advance the discontinuity generation.
    ///
    /// ```
    /// use ferravex_engine::interpolation::InterpolationPair;
    /// let mut history = InterpolationPair::new(0);
    /// for executed_state in [1, 2, 3] { history.commit(executed_state); }
    /// assert_eq!((history.previous(), history.current()), (&2, &3));
    /// assert_eq!(history.generation(), 0);
    /// ```
    pub fn commit(&mut self, value: T) {
        self.previous = std::mem::replace(&mut self.current, value);
        self.alpha = 1.0;
        self.held = false;
    }

    /// Sample borrowed endpoints with a caller-provided interpolation function.
    ///
    /// Validates the supplied alpha even while held. Calls `interpolate` exactly
    /// once with `(previous, current, effective_alpha)`, including at 0/1 or after
    /// a snap. While held, effective alpha is the retained fraction; otherwise it
    /// is the supplied fraction. Records it only after the callback returns.
    /// The callback may return any presentation type; the pair does not inspect
    /// that result or validate endpoint contents. A callback returning its own
    /// `Err` is still a completed sample, and produces `Ok(Err(..))`.
    ///
    /// # Errors
    ///
    /// Nonfinite or out-of-range alpha returns [`InterpolationError::InvalidAlpha`]
    /// without invoking the callback or mutating any state. Both signed zeros
    /// and exactly 1 are valid; no clamping or extrapolation is performed.
    ///
    /// ```
    /// use ferravex_engine::interpolation::{InterpolationError, InterpolationPair};
    /// let mut history = InterpolationPair::new(0.0_f32);
    /// history.commit(8.0);
    /// assert_eq!(history.sample_with(0.25, |a, b, t| a + (b - a) * t)?, 2.0);
    /// assert_eq!(history.sample_with(f32::NAN, |_, _, _| panic!("not called")),
    ///            Err::<(), _>(InterpolationError::InvalidAlpha));
    /// assert_eq!(history.alpha(), 0.25);
    /// # Ok::<(), InterpolationError>(())
    /// ```
    pub fn sample_with<R>(
        &mut self,
        alpha: f32,
        interpolate: impl FnOnce(&T, &T, f32) -> R,
    ) -> Result<R, InterpolationError> {
        if !(0.0..=1.0).contains(&alpha) {
            return Err(InterpolationError::InvalidAlpha);
        }
        let effective_alpha = if self.held { self.alpha } else { alpha };
        let sampled = interpolate(&self.previous, &self.current, effective_alpha);
        self.alpha = effective_alpha;
        Ok(sampled)
    }

    /// Hold the last successful sample's alpha; return whether a hold was entered.
    ///
    /// Repeated calls are idempotent. Before any sample following new/commit/snap,
    /// alpha is 1, so holding selects the current endpoint. This operation retains
    /// no third value and performs no interpolation. A commit or snap releases it.
    ///
    /// ```
    /// use ferravex_engine::interpolation::InterpolationPair;
    /// let mut history = InterpolationPair::new(0.0_f32);
    /// history.commit(8.0);
    /// let blend = |a: &f32, b: &f32, t| a + (b - a) * t;
    /// assert_eq!(history.sample_with(0.75, blend)?, 6.0);
    /// assert!(history.hold());
    /// assert!(!history.hold());
    /// assert!(history.is_held());
    /// assert_eq!(history.sample_with(0.0, blend)?, 6.0);
    /// assert_eq!(history.alpha(), 0.75);
    /// # Ok::<(), ferravex_engine::interpolation::InterpolationError>(())
    /// ```
    pub fn hold(&mut self) -> bool {
        !std::mem::replace(&mut self.held, true)
    }

    /// Release a hold; return whether one was active. Other state is unchanged.
    ///
    /// The next sample uses its supplied fraction. Pass the active session's
    /// retained fraction on a zero-tick resume, not an inactive frame's zero.
    /// No smoothing or monotonic-alpha constraint is added: after an actual tick
    /// a smaller alpha refers to a newer interval and is expected.
    /// If an inspector displayed alpha 1 while paused, keep the hold until the
    /// next commit to avoid returning to a smaller retained session fraction.
    ///
    /// ```
    /// use ferravex_engine::interpolation::InterpolationPair;
    /// let mut history = InterpolationPair::new(0.0_f32);
    /// history.commit(8.0);
    /// let blend = |a: &f32, b: &f32, t| a + (b - a) * t;
    /// history.sample_with(0.5, blend)?;
    /// history.hold();
    /// assert!(history.resume());
    /// assert!(!history.resume());
    /// assert_eq!(history.sample_with(0.75, blend)?, 6.0);
    /// # Ok::<(), ferravex_engine::interpolation::InterpolationError>(())
    /// ```
    pub fn resume(&mut self) -> bool {
        std::mem::replace(&mut self.held, false)
    }

    /// Reset both endpoints to an accepted discontinuity and return its generation.
    ///
    /// Clones the value once, releases any hold and sets alpha to 1. The local
    /// generation increments with wrapping even if the new value equals the old
    /// value. Use for teleport/respawn/restart instead of replacing the whole pair,
    /// so existing observers can detect reset. There is no old pose to blend from.
    ///
    /// ```
    /// use ferravex_engine::interpolation::InterpolationPair;
    /// let mut history = InterpolationPair::new(0.0_f32);
    /// history.commit(8.0);
    /// history.hold();
    /// assert_eq!(history.snap(100.0), 1);
    /// assert!(!history.is_held());
    /// assert_eq!(history.generation(), 1);
    /// assert_eq!((history.previous(), history.current()), (&100.0, &100.0));
    /// assert_eq!(history.sample_with(0.25, |a, b, t| a + (b - a) * t)?, 100.0);
    /// # Ok::<(), ferravex_engine::interpolation::InterpolationError>(())
    /// ```
    pub fn snap(&mut self, value: T) -> u64
    where
        T: Clone,
    {
        let previous = value.clone();
        self.previous = previous;
        self.current = value;
        self.alpha = 1.0;
        self.held = false;
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        math::{Quat, Vec2, Vec3},
        session::GameSession,
    };
    use std::{
        cell::Cell,
        panic::{AssertUnwindSafe, catch_unwind},
    };

    fn blend(a: &f32, b: &f32, alpha: f32) -> f32 {
        a + (b - a) * alpha
    }

    #[test]
    fn initial_pair_has_two_equal_endpoints_and_no_discontinuity() {
        let mut history = InterpolationPair::new(12.0_f32);
        assert_eq!((history.previous(), history.current()), (&12.0, &12.0));
        assert_eq!(history.alpha(), 1.0);
        assert_eq!(history.generation(), 0);
        assert!(!history.is_held());
        for alpha in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert_eq!(history.sample_with(alpha, blend), Ok(12.0));
            assert_eq!(history.alpha(), alpha);
        }
        assert!(history.hold());
        assert!(!history.hold());
        assert!(history.resume());
        assert!(!history.resume());
        assert_eq!(history.generation(), 0);
    }

    #[test]
    fn zero_tick_frames_keep_endpoints_and_catchup_keeps_last_executed_pair() {
        let mut session = GameSession::new(64.0, 4, true);
        let mut history = InterpolationPair::new(0.0_f32);
        let frame = session.advance(0.5 / 64.0);
        assert_eq!(frame.steps, 0);
        assert_eq!(history.sample_with(frame.alpha, blend), Ok(0.0));
        assert_eq!((history.previous(), history.current()), (&0.0, &0.0));

        let frame = session.advance(3.0 / 64.0);
        assert_eq!(frame.steps, 3);
        for _dt in frame.ticks() {
            history.commit(history.current() + 1.0);
        }
        assert_eq!((history.previous(), history.current()), (&2.0, &3.0));
        assert_eq!(history.sample_with(frame.alpha, blend), Ok(2.5));

        let frame = session.advance(0.25 / 64.0);
        assert_eq!(frame.steps, 0);
        assert_eq!(history.sample_with(frame.alpha, blend), Ok(2.75));
        assert_eq!((history.previous(), history.current()), (&2.0, &3.0));

        let frame = session.advance(0.25 / 64.0);
        assert_eq!(frame.steps, 1);
        for _dt in frame.ticks() {
            history.commit(history.current() + 1.0);
        }
        // A smaller alpha after an actual tick belongs to the next interval.
        assert_eq!(history.sample_with(frame.alpha, blend), Ok(3.0));
        assert_eq!(history.generation(), 0);
    }

    #[test]
    fn scheduled_but_unexecuted_updates_do_not_enter_history() {
        let mut session = GameSession::new(64.0, 4, true);
        let mut history = InterpolationPair::new(0.0_f32);
        let batch = session.advance_batch(3.5 / 64.0);
        let mut executed = 0;
        for _context in batch.ticks() {
            if !session.is_playing() {
                break;
            }
            executed += 1;
            history.commit(1.0);
            session.finish(true);
        }
        assert_eq!(executed, 1);
        assert_eq!(session.tick(), 3); // Scheduled IDs count the skipped tail.
        assert_eq!((history.previous(), history.current()), (&0.0, &1.0));
        // Terminal presentation deliberately shows the final accepted state.
        assert_eq!(history.sample_with(1.0, blend), Ok(1.0));
        history.hold();
        assert_eq!(
            history.sample_with(session.advance(30.0).alpha, blend),
            Ok(1.0)
        );
    }

    #[test]
    fn pause_and_zero_tick_resume_preserve_the_last_visible_pose() {
        let mut session = GameSession::new(64.0, 4, true);
        let mut history = InterpolationPair::new(0.0_f32);
        let frame = session.advance(1.75 / 64.0);
        for _dt in frame.ticks() {
            history.commit(8.0);
        }
        assert_eq!(history.sample_with(frame.alpha, blend), Ok(6.0));
        assert!(session.set_paused(true));
        assert!(history.hold());
        let held = history.clone();
        for elapsed in [0.0, 0.5 / 64.0, 30.0] {
            let frame = session.advance(elapsed);
            assert_eq!((frame.steps, frame.alpha), (0, 0.0));
            assert_eq!(history.sample_with(frame.alpha, blend), Ok(6.0));
            assert!(!history.hold());
            assert_eq!(history, held);
        }
        assert_eq!(session.tick(), 1);
        session.set_paused(false);
        assert!(history.resume());
        assert_eq!(history.alpha(), 0.75);
        let frame = session.advance(0.0);
        assert_eq!(frame.steps, 0);
        assert_eq!(history.sample_with(frame.alpha, blend), Ok(6.0));
        let frame = session.advance(0.125 / 64.0);
        assert_eq!(frame.steps, 0);
        assert_eq!(history.sample_with(frame.alpha, blend), Ok(7.0));
    }

    #[test]
    fn commits_release_holds_without_blocking_authoritative_progress() {
        let mut session = GameSession::new(64.0, 4, true);
        let mut history = InterpolationPair::new(0.0_f32);
        for step in 1..=4 {
            history.hold(); // A local overlay does not pause this remote session.
            let frame = session.advance(1.0 / 64.0);
            for _dt in frame.ticks() {
                history.commit(step as f32);
            }
            assert_eq!(session.tick(), step);
            assert!(!history.is_held());
            assert_eq!(history.alpha(), 1.0);
            assert_eq!(
                history.sample_with(frame.alpha, blend),
                Ok((step - 1) as f32)
            );
        }
        history.commit(5.0);
        history.hold(); // No sample since commit: holds the newest state.
        assert_eq!(history.sample_with(0.0, blend), Ok(5.0));
    }

    #[test]
    fn paused_inspector_commit_can_be_shown_and_held_until_next_ordinary_tick() {
        let mut session = GameSession::new(64.0, 4, true);
        let mut history = InterpolationPair::new(0.0_f32);
        assert_eq!(session.advance(0.5 / 64.0).steps, 0);
        session.set_paused(true);
        history.hold();
        let manual = session.step_paused().unwrap();
        for _dt in manual.ticks() {
            history.commit(8.0);
        }
        assert!(!history.is_held());
        assert_eq!(history.sample_with(manual.alpha, blend), Ok(8.0));
        history.hold();
        assert_eq!(
            history.sample_with(session.advance(30.0).alpha, blend),
            Ok(8.0)
        );
        session.set_paused(false);
        // Keep this explicit hold until another commit if showing alpha 1 while
        // paused: immediately resuming at the session's 0.5 would blend backward.
        let frame = session.advance(0.0);
        assert_eq!(frame.steps, 0);
        assert_eq!(history.sample_with(frame.alpha, blend), Ok(8.0));
        let frame = session.advance(0.5 / 64.0);
        assert_eq!(frame.steps, 1);
        for _dt in frame.ticks() {
            history.commit(16.0);
        }
        assert!(!history.is_held());
        assert_eq!(history.sample_with(frame.alpha, blend), Ok(8.0));
    }

    #[test]
    fn invalid_alphas_preserve_every_field_and_never_invoke_the_sampler() {
        let mut history = InterpolationPair::new(0.0_f32);
        history.snap(4.0);
        history.commit(8.0);
        history.sample_with(0.75, blend).unwrap();
        for held in [false, true] {
            if held {
                history.hold();
            }
            let before = history.clone();
            for alpha in [
                f32::NAN,
                -f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                -f32::MAX,
                -f32::from_bits(1),
                -0.01,
                f32::from_bits(1.0_f32.to_bits() + 1),
                f32::MAX,
            ] {
                let result =
                    history.sample_with(alpha, |_, _, _| panic!("invalid alpha called sampler"));
                assert_eq!(result, Err::<(), _>(InterpolationError::InvalidAlpha));
                assert_eq!(history, before);
            }
        }
    }

    #[test]
    fn valid_boundary_alphas_are_forwarded_exactly_and_held_input_is_ignored() {
        let mut history = InterpolationPair::new(2.0_f32);
        history.commit(4.0);
        for alpha in [-0.0_f32, 0.0, f32::from_bits(1), 0.5, 1.0] {
            let sampled = history
                .sample_with(alpha, |a, b, t| {
                    assert_eq!((*a, *b), (2.0, 4.0));
                    assert_eq!(t.to_bits(), alpha.to_bits());
                    t
                })
                .unwrap();
            assert_eq!(sampled.to_bits(), alpha.to_bits());
            assert_eq!(history.alpha().to_bits(), alpha.to_bits());
        }
        history.sample_with(-0.0, blend).unwrap();
        history.hold();
        for alpha in [0.0, 0.25, 0.75, 1.0] {
            assert_eq!(
                history.sample_with(alpha, |_, _, t| t.to_bits()),
                Ok((-0.0_f32).to_bits())
            );
        }
        assert_eq!(history.sample_with(1.0, blend), Ok(2.0));
        history.resume();
        assert_eq!(history.sample_with(1.0, blend), Ok(4.0));
    }

    #[test]
    fn sampler_is_fn_once_can_project_output_and_keeps_value_policy_with_caller() {
        let calls = Cell::new(0);
        let token = String::from("returned once");
        let mut history = InterpolationPair::new((1_u32, f32::NAN));
        let result = history
            .sample_with(0.25, |a, b, t| {
                calls.set(calls.get() + 1);
                assert!(a.1.is_nan() && b.1.is_nan()); // Only alpha is validated.
                (token, a.0 + b.0, t)
            })
            .unwrap();
        assert_eq!(result, (String::from("returned once"), 2, 0.25));
        assert_eq!(calls.get(), 1);
        assert_eq!(
            history.sample_with(0.75, |_, _, _| Err::<(), _>("consumer error")),
            Ok(Err("consumer error"))
        );
        assert_eq!(history.alpha(), 0.75);
    }

    #[test]
    fn panicking_sampler_does_not_record_an_unfinished_sample() {
        let mut history = InterpolationPair::new(0.0_f32);
        history.commit(8.0);
        history.sample_with(0.25, blend).unwrap();
        for held in [false, true] {
            if held {
                history.hold();
            }
            let before = history.clone();
            let panic = catch_unwind(AssertUnwindSafe(|| {
                let _ = history.sample_with(0.75, |_, _, _| panic!("sampler panic"));
            }));
            assert!(panic.is_err());
            assert_eq!(history, before);
        }
    }

    #[test]
    fn snap_replaces_both_endpoints_clears_hold_and_starts_a_continuous_interval() {
        let mut history = InterpolationPair::new(0.0_f32);
        history.commit(8.0);
        history.sample_with(0.75, blend).unwrap();
        history.hold();
        assert_eq!(history.snap(100.0), 1);
        assert_eq!(history.generation(), 1);
        assert!(!history.is_held());
        assert_eq!(history.alpha(), 1.0);
        for alpha in [0.0, 0.5, 1.0] {
            assert_eq!(history.sample_with(alpha, blend), Ok(100.0));
        }
        history.commit(108.0);
        assert_eq!(history.sample_with(0.25, blend), Ok(102.0));
        assert_eq!(history.generation(), 1);
        assert_eq!(history.snap(0.0), 2); // Restart preserves discontinuity tracking.
        assert_eq!(history.snap(0.0), 3); // Equal values still explicitly reset.
        assert_eq!((history.previous(), history.current()), (&0.0, &0.0));
    }

    #[test]
    fn discontinuity_generation_wraps_without_overflow_or_changing_tick_policy() {
        let mut history = InterpolationPair::new(0);
        history.generation = u64::MAX - 1;
        assert_eq!(history.snap(1), u64::MAX);
        history.commit(2);
        history.hold();
        history.resume();
        history.sample_with(0.5, |_, b, _| *b).unwrap();
        assert_eq!(history.generation(), u64::MAX);
        assert_eq!(history.snap(3), 0);
        assert_eq!(history.snap(4), 1);
        assert_eq!((history.previous(), history.current()), (&4, &4));
    }

    #[test]
    fn exactly_two_values_are_retained_and_commits_samples_and_holds_never_clone() {
        struct Tracked<'a> {
            value: u32,
            live: &'a Cell<u32>,
            clones: &'a Cell<u32>,
        }
        impl<'a> Tracked<'a> {
            fn new(value: u32, live: &'a Cell<u32>, clones: &'a Cell<u32>) -> Self {
                live.set(live.get() + 1);
                Self {
                    value,
                    live,
                    clones,
                }
            }
        }
        impl Clone for Tracked<'_> {
            fn clone(&self) -> Self {
                self.clones.set(self.clones.get() + 1);
                Self::new(self.value, self.live, self.clones)
            }
        }
        impl Drop for Tracked<'_> {
            fn drop(&mut self) {
                self.live.set(self.live.get() - 1);
            }
        }

        let live = Cell::new(0);
        let clones = Cell::new(0);
        let mut history = InterpolationPair::new(Tracked::new(0, &live, &clones));
        assert_eq!(live.get(), 2);
        assert_eq!(clones.get(), 1);
        for value in 1..=10_000 {
            history.commit(Tracked::new(value, &live, &clones));
            assert_eq!(live.get(), 2);
            assert_eq!(
                history.sample_with(0.5, |a, b, _| (a.value, b.value)),
                Ok((value - 1, value))
            );
            history.hold();
            history.sample_with(0.0, |_, _, _| ()).unwrap();
            history.resume();
        }
        assert_eq!(clones.get(), 1);
        history.snap(Tracked::new(20, &live, &clones));
        assert_eq!(live.get(), 2);
        assert_eq!(clones.get(), 2);
        assert_eq!(
            (history.previous().value, history.current().value),
            (20, 20)
        );
        drop(history);
        assert_eq!(live.get(), 0);
    }

    #[test]
    fn vec2_vec3_and_shortest_arc_quaternion_use_existing_math() {
        let mut flat = InterpolationPair::new(Vec2::new(2.0, -2.0));
        flat.commit(Vec2::new(10.0, 6.0));
        assert_eq!(
            flat.sample_with(0.25, |a, b, t| a.lerp(*b, t)),
            Ok(Vec2::new(4.0, 0.0))
        );
        let turn = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        let mut pose = InterpolationPair::new((Vec3::ZERO, Quat::IDENTITY));
        pose.commit((Vec3::new(8.0, 4.0, 2.0), -turn));
        let blend = |a: &(Vec3, Quat), b: &(Vec3, Quat), t| (a.0.lerp(b.0, t), a.1.slerp(b.1, t));
        let (position, rotation) = pose.sample_with(0.5, blend).unwrap();
        assert_eq!(position, Vec3::new(4.0, 2.0, 1.0));
        let expected = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
        assert!(rotation.try_angle_between(expected).unwrap() < 1e-6);
        assert!(rotation.is_normalized());
        pose.hold();
        assert_eq!(pose.sample_with(0.0, blend).unwrap(), (position, rotation));
        pose.snap((Vec3::new(100.0, 0.0, -20.0), turn));
        let (position, rotation) = pose.sample_with(0.0, blend).unwrap();
        assert_eq!(position, pose.current().0);
        assert!(rotation.try_angle_between(turn).unwrap() < 1e-6);
    }

    #[test]
    fn alpha_error_implements_standard_error() {
        let error: &dyn std::error::Error = &InterpolationError::InvalidAlpha;
        assert_eq!(
            error.to_string(),
            "interpolation alpha must be finite and in [0, 1]"
        );
        assert!(error.source().is_none());
    }
}
