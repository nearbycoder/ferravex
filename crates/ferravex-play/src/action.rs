//! Fixed-clock action timelines. Gameplay drives animation, never the reverse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionError {
    Invalid,
    Capacity,
    Exhausted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionPhase {
    Idle,
    Windup,
    Active,
    Recovery,
}
/// Why an interruption was rejected by [`ActionTimeline::check_cancel`] or
/// [`ActionTimeline::try_cancel`]. A rejection leaves the timeline unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionCancelError {
    /// No action is running, even if cancellation was forced.
    Idle,
    /// Windup has advanced strictly past the authored cancellation cutoff.
    CutoffPassed,
    /// The action is active; cancellation requires `force = true`.
    Active,
    /// The action is recovering; cancellation requires `force = true`.
    Recovery,
}
#[derive(Clone, Copy, Debug)]
pub struct ActionSpec {
    pub windup: f32,
    pub active: f32,
    pub recovery: f32,
    pub cancel_before: f32,
    pub buffer_seconds: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionEdge {
    Started,
    Activated,
    Recovered,
    Finished,
    Cancelled,
}
#[derive(Clone, Copy, Debug)]
pub struct ActionEvent {
    pub edge: ActionEdge,
    pub serial: u64,
    pub offset: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct ActionFrame {
    events: [Option<ActionEvent>; 8],
}
impl ActionFrame {
    pub fn events(&self) -> impl Iterator<Item = ActionEvent> + '_ {
        self.events.iter().flatten().copied()
    }
    pub fn activated(&self) -> bool {
        self.events().any(|e| e.edge == ActionEdge::Activated)
    }
    fn push(&mut self, edge: ActionEdge, serial: u64, offset: f32) {
        let slot = self
            .events
            .iter_mut()
            .find(|v| v.is_none())
            .expect("at most two three-phase actions per step");
        *slot = Some(ActionEvent {
            edge,
            serial,
            offset,
        });
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ActionTimeline {
    spec: ActionSpec,
    phase: ActionPhase,
    elapsed: f64,
    serial: u64,
    buffered: bool,
}
impl ActionTimeline {
    pub fn new(spec: ActionSpec) -> Result<Self, ActionError> {
        if ![
            spec.windup,
            spec.active,
            spec.recovery,
            spec.cancel_before,
            spec.buffer_seconds,
        ]
        .iter()
        .all(|v| v.is_finite() && (0. ..=10.).contains(v))
            || spec.active < 0.0001
            || spec.cancel_before > spec.windup
            || spec.buffer_seconds > spec.recovery.min(0.5)
        {
            return Err(ActionError::Invalid);
        }
        Ok(Self {
            spec,
            phase: ActionPhase::Idle,
            elapsed: 0.,
            serial: 0,
            buffered: false,
        })
    }
    pub fn phase(&self) -> ActionPhase {
        self.phase
    }
    pub fn serial(&self) -> u64 {
        self.serial
    }
    fn duration(&self) -> f64 {
        f64::from(match self.phase {
            ActionPhase::Idle => 0.,
            ActionPhase::Windup => self.spec.windup,
            ActionPhase::Active => self.spec.active,
            ActionPhase::Recovery => self.spec.recovery,
        })
    }
    pub fn progress(&self) -> f32 {
        if self.duration() == 0. {
            0.
        } else {
            (self.elapsed / self.duration()).clamp(0., 1.) as f32
        }
    }
    /// Simulation seconds left in the current phase, or zero while idle.
    ///
    /// This inspection neither advances time nor consumes input. Values are
    /// rounded to `f32`; [`Self::step`] retains its existing boundary tolerance.
    /// See [`Self::remaining`] for the time through the end of this action.
    ///
    /// ```
    /// use ferravex_play::action::{ActionPhase, ActionSpec, ActionTimeline};
    /// let mut action = ActionTimeline::new(ActionSpec {
    ///     windup: 0.125, active: 0.0625, recovery: 0.25,
    ///     cancel_before: 0.0625, buffer_seconds: 0.125,
    /// }).unwrap();
    /// assert_eq!(action.phase_remaining(), 0.0);
    /// action.step(0.0625, true).unwrap();
    /// assert_eq!(action.phase(), ActionPhase::Windup);
    /// assert_eq!(action.phase_remaining(), 0.0625);
    /// assert_eq!(action.remaining(), 0.375);
    /// ```
    pub fn phase_remaining(&self) -> f32 {
        (self.duration() - self.elapsed).max(0.) as f32
    }
    /// Simulation seconds until the current action finishes, or zero while idle.
    ///
    /// Includes the remainder of this phase and all subsequent phases of this
    /// action. Excludes any buffered follow-up: finishing may immediately start
    /// that next action instead of leaving the timeline idle. Values are rounded
    /// to `f32`. For a usage example, see [`Self::phase_remaining`].
    pub fn remaining(&self) -> f32 {
        let subsequent = match self.phase {
            ActionPhase::Windup => f64::from(self.spec.active) + f64::from(self.spec.recovery),
            ActionPhase::Active => f64::from(self.spec.recovery),
            ActionPhase::Recovery | ActionPhase::Idle => 0.,
        };
        ((self.duration() - self.elapsed).max(0.) + subsequent) as f32
    }
    /// Inspect whether cancellation would succeed at the current simulation time.
    ///
    /// Ordinary cancellation is allowed only during windup, at or before
    /// `cancel_before`. Forced cancellation accepts any non-idle phase. This
    /// does not reserve a cancellation; [`Self::try_cancel`] checks again when
    /// called, so advancing the timeline can change the answer.
    ///
    /// ```
    /// use ferravex_play::action::{ActionCancelError, ActionSpec, ActionTimeline};
    /// let mut action = ActionTimeline::new(ActionSpec {
    ///     windup: 0.125, active: 0.0625, recovery: 0.25,
    ///     cancel_before: 0.0625, buffer_seconds: 0.125,
    /// }).unwrap();
    /// assert_eq!(action.check_cancel(true), Err(ActionCancelError::Idle));
    /// action.step(0.0625, true).unwrap();
    /// assert_eq!(action.check_cancel(false), Ok(())); // Cutoff is inclusive.
    /// action.step(0.03125, false).unwrap();
    /// assert_eq!(action.check_cancel(false), Err(ActionCancelError::CutoffPassed));
    /// assert_eq!(action.check_cancel(true), Ok(()));
    /// ```
    pub fn check_cancel(&self, force: bool) -> Result<(), ActionCancelError> {
        match self.phase {
            ActionPhase::Idle => Err(ActionCancelError::Idle),
            _ if force => Ok(()),
            ActionPhase::Windup if self.elapsed <= f64::from(self.spec.cancel_before) => Ok(()),
            ActionPhase::Windup => Err(ActionCancelError::CutoffPassed),
            ActionPhase::Active => Err(ActionCancelError::Active),
            ActionPhase::Recovery => Err(ActionCancelError::Recovery),
        }
    }
    /// Cancel an action, returning its accepted [`ActionEdge::Cancelled`] event.
    ///
    /// Uses the same policy as [`Self::check_cancel`]. Rejection changes nothing;
    /// success enters idle and clears the buffered follow-up. The event carries
    /// the cancelled action's serial and offset zero (the time of this call).
    /// Cancellation does not allocate a new serial, undo previously returned
    /// events, or enqueue an event for the next [`Self::step`]. The host should
    /// consume this result once and close any associated [`HitWindow`] itself.
    ///
    /// ```
    /// use ferravex_play::action::{ActionEdge, ActionSpec, ActionTimeline, HitWindow};
    /// let mut action = ActionTimeline::new(ActionSpec {
    ///     windup: 0.125, active: 0.0625, recovery: 0.25,
    ///     cancel_before: 0.0625, buffer_seconds: 0.125,
    /// }).unwrap();
    /// let mut hits = HitWindow::<4>::new().unwrap();
    /// for event in action.step(0.125, true).unwrap().events() {
    ///     if event.edge == ActionEdge::Activated {
    ///         assert!(hits.begin(event.serial));
    ///     }
    /// }
    /// assert!(hits.accept(action.serial(), 7).unwrap());
    /// let cancelled = action.try_cancel(true).unwrap();
    /// assert_eq!(cancelled.edge, ActionEdge::Cancelled);
    /// assert_eq!(cancelled.serial, action.serial());
    /// assert_eq!(cancelled.offset, 0.0);
    /// hits.close();
    /// assert!(!hits.accept(cancelled.serial, 8).unwrap());
    /// assert!(!hits.begin(cancelled.serial)); // Cannot reopen the old action.
    /// assert_eq!(action.step(0.25, false).unwrap().events().count(), 0);
    /// ```
    pub fn try_cancel(&mut self, force: bool) -> Result<ActionEvent, ActionCancelError> {
        self.check_cancel(force)?;
        self.phase = ActionPhase::Idle;
        self.elapsed = 0.;
        self.buffered = false;
        Ok(ActionEvent {
            edge: ActionEdge::Cancelled,
            serial: self.serial,
            offset: 0.,
        })
    }
    /// External interruption can obey the authored windup cutoff or force cancellation
    /// on death/stun. Successful cancellation clears queued follow-up actions.
    /// Use [`Self::try_cancel`] for a rejection reason or accepted cancellation event.
    pub fn cancel(&mut self, force: bool) -> bool {
        self.try_cancel(force).is_ok()
    }
    fn start(&mut self) -> Result<(), ActionError> {
        self.serial = self.serial.checked_add(1).ok_or(ActionError::Exhausted)?;
        self.phase = ActionPhase::Windup;
        self.elapsed = 0.;
        self.buffered = false;
        Ok(())
    }
    /// At most eight transition reports: an existing action and one buffered action.
    /// Offsets let an authoritative host schedule queries at the proper substep.
    pub fn step(&mut self, dt: f32, trigger: bool) -> Result<ActionFrame, ActionError> {
        if !dt.is_finite() || !(0. ..=0.25).contains(&dt) {
            return Err(ActionError::Invalid);
        }
        let mut frame = ActionFrame { events: [None; 8] };
        if trigger && self.phase == ActionPhase::Idle {
            self.start()?;
            frame.push(ActionEdge::Started, self.serial, 0.);
        } else if trigger
            && self.phase == ActionPhase::Recovery
            && self.duration() - self.elapsed <= f64::from(self.spec.buffer_seconds)
        {
            self.buffered = true;
        }
        let mut left = f64::from(dt);
        let mut offset = 0.;
        for _ in 0..8 {
            if self.phase == ActionPhase::Idle {
                break;
            }
            let remaining = (self.duration() - self.elapsed).max(0.);
            if left + 1e-9 < remaining {
                self.elapsed += left;
                break;
            }
            left = (left - remaining).max(0.);
            offset += remaining;
            self.elapsed = 0.;
            match self.phase {
                ActionPhase::Windup => {
                    self.phase = ActionPhase::Active;
                    frame.push(ActionEdge::Activated, self.serial, offset as f32);
                }
                ActionPhase::Active => {
                    self.phase = ActionPhase::Recovery;
                    frame.push(ActionEdge::Recovered, self.serial, offset as f32);
                }
                ActionPhase::Recovery => {
                    self.phase = ActionPhase::Idle;
                    frame.push(ActionEdge::Finished, self.serial, offset as f32);
                    if self.buffered {
                        self.start()?;
                        frame.push(ActionEdge::Started, self.serial, offset as f32);
                    } else {
                        break;
                    }
                }
                ActionPhase::Idle => break,
            }
        }
        Ok(frame)
    }
}
/// Prevent repeated damage to one generational target during a single action.
#[derive(Clone, Debug)]
pub struct HitWindow<const N: usize> {
    serial: u64,
    open: bool,
    targets: [u64; N],
    len: usize,
}
impl<const N: usize> HitWindow<N> {
    pub fn new() -> Result<Self, ActionError> {
        if N == 0 || N > 128 {
            return Err(ActionError::Capacity);
        }
        Ok(Self {
            serial: 0,
            open: false,
            targets: [0; N],
            len: 0,
        })
    }
    pub fn begin(&mut self, serial: u64) -> bool {
        if serial <= self.serial {
            return false;
        }
        self.serial = serial;
        self.open = true;
        self.len = 0;
        true
    }
    pub fn close(&mut self) {
        self.open = false;
    }
    pub fn accept(&mut self, serial: u64, target: u64) -> Result<bool, ActionError> {
        if target == 0 {
            return Err(ActionError::Invalid);
        }
        if !self.open || serial != self.serial || self.targets[..self.len].contains(&target) {
            return Ok(false);
        }
        if self.len == N {
            return Err(ActionError::Capacity);
        }
        self.targets[self.len] = target;
        self.len += 1;
        Ok(true)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn timeline() -> ActionTimeline {
        ActionTimeline::new(ActionSpec {
            windup: 0.125,
            active: 0.0625,
            recovery: 0.25,
            cancel_before: 0.0625,
            buffer_seconds: 0.125,
        })
        .unwrap()
    }
    #[test]
    fn ordered_edges_offsets_and_partition_independence() {
        let mut t = timeline();
        assert!(!t.step(0.0625, true).unwrap().activated());
        let frame = t.step(0.125, false).unwrap();
        let edges: Vec<_> = frame.events().map(|e| e.edge).collect();
        assert_eq!(edges, [ActionEdge::Activated, ActionEdge::Recovered]);
        assert_eq!(t.phase(), ActionPhase::Recovery);
        let mut a = timeline();
        let mut b = timeline();
        let mut count = 0;
        for i in 0..7 {
            count += b.step(0.0625, i == 0).unwrap().events().count();
        }
        let count_a = a.step(0.25, true).unwrap().events().count()
            + a.step(0.1875, false).unwrap().events().count();
        assert_eq!(count, count_a);
        assert_eq!(a.phase(), b.phase());
        assert_eq!(a.phase(), ActionPhase::Idle);
    }
    #[test]
    fn cancellation_cutoff_and_buffer_do_not_skip_windup() {
        let mut t = timeline();
        t.step(0.03125, true).unwrap();
        assert!(t.cancel(false));
        t.step(0.1, true).unwrap();
        assert!(!t.cancel(false));
        assert!(t.cancel(true));
        t = timeline();
        t.step(0.25, true).unwrap();
        t.step(0.0625, true).unwrap();
        let frame = t.step(0.125, true).unwrap();
        assert!(frame.events().any(|e| e.edge == ActionEdge::Started));
        assert_eq!(t.phase(), ActionPhase::Windup);
        assert_eq!(t.serial(), 2);
    }
    fn state(t: &ActionTimeline) -> (ActionPhase, f64, u64, bool) {
        (t.phase, t.elapsed, t.serial, t.buffered)
    }
    #[test]
    fn remaining_tracks_each_phase_without_advancing() {
        let mut t = timeline();
        assert_eq!(t.phase_remaining(), 0.);
        assert_eq!(t.remaining(), 0.);
        for (dt, trigger, phase, phase_remaining, remaining) in [
            (0., true, ActionPhase::Windup, 0.125, 0.4375),
            (0.0625, false, ActionPhase::Windup, 0.0625, 0.375),
            (0., false, ActionPhase::Windup, 0.0625, 0.375),
            (0.0625, false, ActionPhase::Active, 0.0625, 0.3125),
            (0.03125, false, ActionPhase::Active, 0.03125, 0.28125),
            (0.03125, false, ActionPhase::Recovery, 0.25, 0.25),
            (0.125, false, ActionPhase::Recovery, 0.125, 0.125),
            (0.125, false, ActionPhase::Idle, 0., 0.),
        ] {
            t.step(dt, trigger).unwrap();
            let before = state(&t);
            assert_eq!(t.phase(), phase);
            assert_eq!(t.phase_remaining(), phase_remaining);
            assert_eq!(t.remaining(), remaining);
            assert_eq!(state(&t), before);
        }
        assert_eq!(t.serial(), 1);
    }
    #[test]
    fn checked_and_legacy_cancellation_agree_for_every_phase() {
        for (dt, rejection) in [
            (None, Some(ActionCancelError::Idle)),
            (Some(0.), None),
            (Some(0.0625), None),
            (Some(0.09375), Some(ActionCancelError::CutoffPassed)),
            (Some(0.125), Some(ActionCancelError::Active)),
            (Some(0.1875), Some(ActionCancelError::Recovery)),
        ] {
            let mut t = timeline();
            if let Some(dt) = dt {
                t.step(dt, true).unwrap();
            }
            for force in [false, true] {
                let mut checked = t;
                let mut legacy = t;
                let expected = if force && t.phase() != ActionPhase::Idle {
                    Ok(())
                } else {
                    rejection.map_or(Ok(()), Err)
                };
                let before = state(&checked);
                assert_eq!(checked.check_cancel(force), expected);
                assert_eq!(state(&checked), before);
                let result = checked.try_cancel(force);
                assert_eq!(result.map(|_| ()), expected);
                assert_eq!(legacy.cancel(force), result.is_ok());
                assert_eq!(state(&legacy), state(&checked));
                if let Ok(event) = result {
                    assert_eq!(event.edge, ActionEdge::Cancelled);
                    assert_eq!(event.serial, t.serial());
                    assert_eq!(event.offset, 0.);
                    assert_eq!(state(&checked), (ActionPhase::Idle, 0., t.serial(), false));
                    assert_eq!(checked.phase_remaining(), 0.);
                    assert_eq!(checked.remaining(), 0.);
                    assert_eq!(
                        checked.try_cancel(force).err(),
                        Some(ActionCancelError::Idle)
                    );
                } else {
                    assert_eq!(state(&checked), before);
                }
            }
        }
    }
    #[test]
    fn cancellation_cutoff_is_exact_and_zero_windup_is_already_active() {
        let mut t = timeline();
        t.step(t.spec.cancel_before, true).unwrap();
        assert_eq!(t.check_cancel(false), Ok(()));
        let just_after = f32::from_bits(t.spec.cancel_before.to_bits() + 1);
        t.step(just_after - t.spec.cancel_before, false).unwrap();
        assert_eq!(
            t.try_cancel(false).err(),
            Some(ActionCancelError::CutoffPassed)
        );

        let mut zero_cutoff = ActionTimeline::new(ActionSpec {
            cancel_before: 0.,
            ..t.spec
        })
        .unwrap();
        zero_cutoff.step(0., true).unwrap();
        assert_eq!(zero_cutoff.check_cancel(false), Ok(()));
        zero_cutoff.step(0.0001, false).unwrap();
        assert_eq!(
            zero_cutoff.check_cancel(false),
            Err(ActionCancelError::CutoffPassed)
        );

        let mut instant_windup = ActionTimeline::new(ActionSpec {
            windup: 0.,
            cancel_before: 0.,
            recovery: 0.,
            buffer_seconds: 0.,
            ..t.spec
        })
        .unwrap();
        assert!(instant_windup.step(0., true).unwrap().activated());
        assert_eq!(instant_windup.phase_remaining(), t.spec.active);
        assert_eq!(instant_windup.remaining(), t.spec.active);
        assert_eq!(
            instant_windup.check_cancel(false),
            Err(ActionCancelError::Active)
        );
        instant_windup.step(t.spec.active, false).unwrap();
        assert_eq!(instant_windup.phase(), ActionPhase::Idle);
        assert_eq!(instant_windup.remaining(), 0.);
    }
    #[test]
    fn failed_cancel_preserves_buffer_and_forced_cancel_clears_it() {
        let mut t = timeline();
        t.step(0.25, true).unwrap();
        t.step(0.0625, false).unwrap();
        t.step(0., true).unwrap();
        assert!(t.buffered);
        assert_eq!(t.phase_remaining(), 0.125);
        assert_eq!(t.remaining(), 0.125); // Excludes the queued action.
        let before = state(&t);
        assert_eq!(t.try_cancel(false).err(), Some(ActionCancelError::Recovery));
        assert_eq!(state(&t), before);

        let mut continuing = t;
        let edges: Vec<_> = continuing
            .step(0.125, false)
            .unwrap()
            .events()
            .map(|event| (event.edge, event.serial, event.offset))
            .collect();
        assert_eq!(
            edges,
            [
                (ActionEdge::Finished, 1, 0.125),
                (ActionEdge::Started, 2, 0.125)
            ]
        );
        assert_eq!(continuing.phase(), ActionPhase::Windup);
        assert_eq!(continuing.remaining(), 0.4375);

        let event = t.try_cancel(true).unwrap();
        assert_eq!(
            (event.edge, event.serial, event.offset),
            (ActionEdge::Cancelled, 1, 0.)
        );
        assert!(!t.buffered);
        assert_eq!(t.step(0.25, false).unwrap().events().count(), 0);
        assert_eq!(t.serial(), 1);
        let restarted = t.step(0., true).unwrap().events().next().unwrap();
        assert_eq!((restarted.edge, restarted.serial), (ActionEdge::Started, 2));
    }
    #[test]
    fn cancelled_serial_cannot_reopen_hit_window_and_next_action_is_fresh() {
        let mut t = timeline();
        let mut hits = HitWindow::<1>::new().unwrap();
        let frame = t.step(0.125, true).unwrap();
        let activated = frame
            .events()
            .find(|event| event.edge == ActionEdge::Activated)
            .unwrap();
        assert!(hits.begin(activated.serial));
        assert!(hits.accept(activated.serial, 9).unwrap());
        assert_eq!(
            hits.accept(activated.serial, 10),
            Err(ActionError::Capacity)
        );
        assert!(!hits.accept(activated.serial, 9).unwrap());

        let event = t.try_cancel(true).unwrap();
        assert_eq!(event.serial, activated.serial);
        hits.close();
        assert!(!hits.begin(event.serial));
        assert!(!hits.accept(event.serial, 10).unwrap());
        let next = t.step(0.125, true).unwrap();
        let next = next
            .events()
            .find(|event| event.edge == ActionEdge::Activated)
            .unwrap();
        assert_eq!(next.serial, event.serial + 1);
        assert!(hits.begin(next.serial));
        assert!(!hits.accept(event.serial, 9).unwrap());
        assert!(hits.accept(next.serial, 9).unwrap());
        assert_eq!(hits.accept(next.serial, 10), Err(ActionError::Capacity));
    }
    #[test]
    fn rejected_input_and_exhausted_serials_are_not_accepted_cancellations() {
        let mut t = timeline();
        t.step(0.03125, true).unwrap();
        let before = state(&t);
        for dt in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.01, 0.251] {
            assert_eq!(t.step(dt, true).err(), Some(ActionError::Invalid));
            assert_eq!(state(&t), before);
        }
        // Even the final representable action can be cancelled without wrapping.
        t.serial = u64::MAX;
        let event = t.try_cancel(false).unwrap();
        assert_eq!(event.serial, u64::MAX);
        let before = state(&t);
        assert_eq!(t.try_cancel(true).err(), Some(ActionCancelError::Idle));
        assert_eq!(t.step(0., true).err(), Some(ActionError::Exhausted));
        assert_eq!(state(&t), before);
    }
    #[test]
    fn buffered_catch_up_remains_bounded_and_keeps_event_serials() {
        let mut t = ActionTimeline::new(ActionSpec {
            windup: 0.,
            active: 0.03125,
            recovery: 0.03125,
            cancel_before: 0.,
            buffer_seconds: 0.03125,
        })
        .unwrap();
        t.step(0.03125, true).unwrap();
        assert_eq!(t.check_cancel(false), Err(ActionCancelError::Recovery));
        let frame = t.step(0.25, true).unwrap();
        let events: Vec<_> = frame
            .events()
            .map(|e| (e.edge, e.serial, e.offset))
            .collect();
        assert_eq!(
            events,
            [
                (ActionEdge::Finished, 1, 0.03125),
                (ActionEdge::Started, 2, 0.03125),
                (ActionEdge::Activated, 2, 0.03125),
                (ActionEdge::Recovered, 2, 0.0625),
                (ActionEdge::Finished, 2, 0.09375),
            ]
        );
        assert!(events.len() <= 8);
        assert_eq!(t.phase(), ActionPhase::Idle);
        assert_eq!(t.phase_remaining(), 0.);
        assert_eq!(t.remaining(), 0.);
    }
    #[test]
    fn target_windows_dedup_serial_and_capacity() {
        let mut w = HitWindow::<1>::new().unwrap();
        assert!(w.begin(1));
        assert!(w.accept(1, 9).unwrap());
        assert!(!w.begin(1));
        assert!(!w.accept(1, 9).unwrap());
        assert_eq!(w.accept(1, 10), Err(ActionError::Capacity));
        w.close();
        assert!(!w.begin(1));
        assert!(w.begin(2));
        assert!(w.accept(2, 9).unwrap());
    }
    #[test]
    fn ability_reservation_commits_only_when_active_and_refunds_cancel() {
        use crate::ability::*;
        let mut bank = AbilityBank::<1>::new(
            1,
            &[AbilitySpec {
                charges: 1,
                recharge_seconds: 1.,
                cost: 5.,
            }],
        )
        .unwrap();
        let mut pool = ResourcePool::new(10., 10., 0.).unwrap();
        let token = bank.begin(0, &mut pool, true).unwrap();
        let mut t = timeline();
        t.step(0.03125, true).unwrap();
        assert!(t.cancel(false));
        bank.cancel(token, &mut pool).unwrap();
        assert_eq!(pool.value(), 10.);
        let token = bank.begin(0, &mut pool, true).unwrap();
        let frame = t.step(0.125, true).unwrap();
        assert!(frame.activated());
        bank.commit(token).unwrap();
        assert_eq!(pool.value(), 5.);
        assert!(bank.commit(token).is_err());
    }
}
