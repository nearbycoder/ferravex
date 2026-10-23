//! Hold interactions bound to a specific actor/target pair with explicit interruption.
//!
//! Query [`Interaction::eligibility`] for prompts without changing the hold. On
//! each actual simulation tick, supply current conditions to
//! [`Interaction::step_checked`]; only its `completed` result grants a reward or
//! accepted-event feedback. Hosts remain responsible for authoritative range,
//! visibility and permission checks. These rules read no platform state or clock.
use super::{LoopError, seconds};

/// A host-supplied snapshot of the conditions for one actor/target pair.
///
/// `pair` is `(actor, target)`. All gates must be true to advance a hold. Range
/// and visibility are supplied as booleans so this rule does not duplicate the
/// host's geometry queries. `focused` means gameplay input is enabled for this
/// actor; suppressing local input must not stop authoritative simulation ticks.
/// The default has no target and all gates closed.
///
/// ```
/// use ferravex_play::gameplay::interaction::{Interaction, InteractionInput};
/// let input = InteractionInput {
///     pair: Some((1, 2)), held: true, in_range: true, focused: true,
///     permitted: true, visible: true,
/// };
/// let interaction = Interaction::new(1.0).unwrap();
/// assert_eq!(interaction.eligibility(input), Ok(()));
/// assert_eq!(interaction.progress(), 0.0);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InteractionInput {
    /// The actor and target, or `None` when no interaction target is selected.
    pub pair: Option<(u64, u64)>,
    /// Whether the interaction button is currently held.
    pub held: bool,
    /// Whether the actor is within the target's interaction range.
    pub in_range: bool,
    /// Whether gameplay input for this actor has focus.
    pub focused: bool,
    /// Whether the actor is allowed to perform the interaction.
    pub permitted: bool,
    /// Whether required line of sight is clear.
    pub visible: bool,
}

/// The first failing gate reported by [`Interaction::eligibility`].
///
/// Gate precedence is no target, focus, hold, range, permission, visibility,
/// then the completion latch. A failed gate can therefore rearm a completed
/// hold when applied by [`Interaction::step_checked`], just like legacy `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionRejection {
    /// No actor/target pair was supplied.
    NoTarget,
    /// Gameplay input for this actor no longer has focus.
    FocusLost,
    /// The interaction button is not held.
    NotHeld,
    /// The target is outside interaction range.
    OutOfRange,
    /// The host denied permission to interact.
    PermissionDenied,
    /// Required line of sight is blocked.
    Obstructed,
    /// This pair already completed; continued holding cannot grant again.
    AlreadyCompleted,
}

/// Why an unfinished hold was discarded on this tick.
///
/// Rejections interrupt only when there was an unfinished hold. A valid pair
/// switch both interrupts the old hold and starts the new one using this tick's
/// time. Resetting a completed latch does not report an unfinished interruption.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionInterruption {
    /// A required condition failed. `AlreadyCompleted` never interrupts a hold.
    Rejected(InteractionRejection),
    /// An eligible actor/target pair replaced the unfinished one.
    PairChanged {
        /// The pair whose progress was discarded.
        previous: (u64, u64),
        /// The pair receiving this tick's time.
        current: (u64, u64),
    },
}

/// Result of one checked simulation step; no events are stored internally.
///
/// `completed` is the sole accepted completion signal. A rejection never
/// completes or advances progress. A pair switch can interrupt the old hold and
/// complete the new one on the same tick if enough simulation time is supplied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InteractionOutcome {
    /// True exactly once when this hold completes; apply rewards/feedback here.
    pub completed: bool,
    /// The gate preventing progress, or `None` for an eligible step.
    pub rejection: Option<InteractionRejection>,
    /// The unfinished hold discarded by this step, if any.
    pub interruption: Option<InteractionInterruption>,
}

#[derive(Clone, Copy, Debug)]
pub struct Interaction {
    duration: f64,
    elapsed: f64,
    pair: Option<(u64, u64)>,
    completed: bool,
}
impl Interaction {
    pub fn new(duration: f32) -> Result<Self, LoopError> {
        let duration = seconds(duration)?;
        if duration == 0. {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            duration,
            elapsed: 0.,
            pair: None,
            completed: false,
        })
    }
    pub fn progress(&self) -> f32 {
        (self.elapsed / self.duration) as f32
    }
    pub fn cancel(&mut self) {
        self.elapsed = 0.;
        self.pair = None;
        self.completed = false;
    }

    /// Checks current conditions without consuming time, changing the pair,
    /// clearing progress or rearming the completion latch.
    ///
    /// `Ok(())` means the hold may advance, not that it is complete. A different
    /// eligible pair may start a new hold; the query does not perform that switch.
    /// If several gates fail, precedence follows [`InteractionRejection`]. Always
    /// supply current conditions again when stepping; a query is not authorization
    /// to use a stale snapshot later.
    ///
    /// ```
    /// use ferravex_play::gameplay::interaction::{
    ///     Interaction, InteractionInput, InteractionRejection,
    /// };
    /// let mut hold = Interaction::new(1.0).unwrap();
    /// hold.step(0.5, Some((1, 2))).unwrap();
    /// let blocked = InteractionInput {
    ///     pair: Some((1, 2)), held: true, in_range: false, focused: true,
    ///     permitted: true, visible: true,
    /// };
    /// assert_eq!(hold.eligibility(blocked), Err(InteractionRejection::OutOfRange));
    /// assert_eq!(hold.progress(), 0.5);
    /// ```
    pub fn eligibility(&self, input: InteractionInput) -> Result<(), InteractionRejection> {
        let reason = if input.pair.is_none() {
            InteractionRejection::NoTarget
        } else if !input.focused {
            InteractionRejection::FocusLost
        } else if !input.held {
            InteractionRejection::NotHeld
        } else if !input.in_range {
            InteractionRejection::OutOfRange
        } else if !input.permitted {
            InteractionRejection::PermissionDenied
        } else if !input.visible {
            InteractionRejection::Obstructed
        } else if input.pair == self.pair && self.completed {
            InteractionRejection::AlreadyCompleted
        } else {
            return Ok(());
        };
        Err(reason)
    }

    /// Advances an eligible hold and reports rejection/interruption reasons.
    ///
    /// A failed condition discards the unfinished hold (continuous holding is
    /// required), but spends none of `dt` and cannot emit a completion. Repeating
    /// a rejected input reports the same rejection without another interruption.
    /// `AlreadyCompleted` preserves the completion latch and full progress.
    /// Pair switches restart exactly as in [`Self::step`].
    ///
    /// Call only for actual simulation ticks, using simulation seconds in
    /// `0..=3600`. Invalid time returns [`LoopError::Invalid`] before checking
    /// gates or changing any state, including when input would interrupt.
    ///
    /// ```
    /// use ferravex_play::gameplay::interaction::{
    ///     Interaction, InteractionInput, InteractionInterruption, InteractionRejection,
    /// };
    /// let input = InteractionInput {
    ///     pair: Some((1, 2)), held: true, in_range: true, focused: true,
    ///     permitted: true, visible: true,
    /// };
    /// let mut hold = Interaction::new(1.0).unwrap();
    /// assert!(!hold.step_checked(0.5, input).unwrap().completed);
    /// let lost_focus = InteractionInput { focused: false, ..input };
    /// let stopped = hold.step_checked(1.0, lost_focus).unwrap();
    /// assert!(!stopped.completed);
    /// assert_eq!(stopped.interruption, Some(InteractionInterruption::Rejected(
    ///     InteractionRejection::FocusLost,
    /// )));
    /// assert_eq!(hold.progress(), 0.0);
    /// assert!(hold.step_checked(1.0, input).unwrap().completed);
    /// assert_eq!(hold.step_checked(1.0, input).unwrap().rejection,
    ///     Some(InteractionRejection::AlreadyCompleted));
    /// ```
    pub fn step_checked(
        &mut self,
        dt: f32,
        input: InteractionInput,
    ) -> Result<InteractionOutcome, LoopError> {
        let dt = seconds(dt)?;
        if let Err(reason) = self.eligibility(input) {
            let interruption = (self.pair.is_some() && !self.completed)
                .then_some(InteractionInterruption::Rejected(reason));
            if reason != InteractionRejection::AlreadyCompleted {
                self.cancel();
            }
            return Ok(InteractionOutcome {
                completed: false,
                rejection: Some(reason),
                interruption,
            });
        }
        let interruption = match (self.pair, input.pair) {
            (Some(previous), Some(current)) if previous != current && !self.completed => {
                Some(InteractionInterruption::PairChanged { previous, current })
            }
            _ => None,
        };
        Ok(InteractionOutcome {
            completed: self.step_validated(dt, input.pair),
            rejection: None,
            interruption,
        })
    }

    /// None interrupts. Supply Some only while the button, range, permissions and
    /// line of sight are valid. Target switches restart; completion latches until
    /// interruption/switch, so holding cannot repeatedly grant a reward.
    /// Use [`Self::step_checked`] to retain typed reasons for failed conditions.
    pub fn step(&mut self, dt: f32, pair: Option<(u64, u64)>) -> Result<bool, LoopError> {
        Ok(self.step_validated(seconds(dt)?, pair))
    }

    fn step_validated(&mut self, dt: f64, pair: Option<(u64, u64)>) -> bool {
        if pair != self.pair {
            self.cancel();
            self.pair = pair;
        }
        if pair.is_none() || self.completed {
            return false;
        }
        self.elapsed = (self.elapsed + dt).min(self.duration);
        if self.elapsed + 1e-7 >= self.duration {
            self.elapsed = self.duration;
            self.completed = true;
            true
        } else {
            false
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn eligible(pair: (u64, u64)) -> InteractionInput {
        InteractionInput {
            pair: Some(pair),
            held: true,
            in_range: true,
            focused: true,
            permitted: true,
            visible: true,
        }
    }

    fn blocked_inputs() -> [(InteractionInput, InteractionRejection); 6] {
        let input = eligible((1, 2));
        [
            (
                InteractionInput {
                    pair: None,
                    ..input
                },
                InteractionRejection::NoTarget,
            ),
            (
                InteractionInput {
                    focused: false,
                    ..input
                },
                InteractionRejection::FocusLost,
            ),
            (
                InteractionInput {
                    held: false,
                    ..input
                },
                InteractionRejection::NotHeld,
            ),
            (
                InteractionInput {
                    in_range: false,
                    ..input
                },
                InteractionRejection::OutOfRange,
            ),
            (
                InteractionInput {
                    permitted: false,
                    ..input
                },
                InteractionRejection::PermissionDenied,
            ),
            (
                InteractionInput {
                    visible: false,
                    ..input
                },
                InteractionRejection::Obstructed,
            ),
        ]
    }

    fn assert_same_state(actual: Interaction, expected: Interaction) {
        assert_eq!(actual.duration, expected.duration);
        assert_eq!(actual.elapsed, expected.elapsed);
        assert_eq!(actual.pair, expected.pair);
        assert_eq!(actual.completed, expected.completed);
    }

    #[test]
    fn interruptions_and_target_switches_require_a_full_new_hold() {
        let mut i = Interaction::new(1.).unwrap();
        assert!(!i.step(0.5, Some((1, 2))).unwrap());
        assert!(!i.step(0.5, Some((1, 3))).unwrap());
        assert!(!i.step(0., None).unwrap());
        assert_eq!(i.progress(), 0.);
        assert!(i.step(1., Some((1, 3))).unwrap());
        assert!(!i.step(2., Some((1, 3))).unwrap());
        assert!(i.step(f32::NAN, None).is_err());
        assert_eq!(i.progress(), 1.);
    }

    #[test]
    fn eligibility_queries_preserve_idle_partial_and_completed_holds() {
        for elapsed in [0., 0.5, 1.] {
            let mut hold = Interaction::new(1.).unwrap();
            if elapsed > 0. {
                hold.step(elapsed, Some((1, 2))).unwrap();
            }
            let before = hold;
            for (input, reason) in blocked_inputs() {
                assert_eq!(hold.eligibility(input), Err(reason));
                assert_same_state(hold, before);
            }
            let expected = if elapsed == 1. {
                Err(InteractionRejection::AlreadyCompleted)
            } else {
                Ok(())
            };
            for _ in 0..3 {
                assert_eq!(hold.eligibility(eligible((1, 2))), expected);
                // Merely displaying another target must not switch or rearm.
                assert_eq!(hold.eligibility(eligible((1, 3))), Ok(()));
                assert_eq!(hold.eligibility(eligible((9, 2))), Ok(()));
                assert_same_state(hold, before);
            }
        }
    }

    #[test]
    fn failed_gates_interrupt_once_without_using_time_or_completing() {
        let valid = eligible((1, 2));
        for (input, reason) in blocked_inputs() {
            let mut hold = Interaction::new(1.).unwrap();
            assert!(!hold.step_checked(0.75, valid).unwrap().completed);
            assert_eq!(
                hold.step_checked(3600., input).unwrap(),
                InteractionOutcome {
                    completed: false,
                    rejection: Some(reason),
                    interruption: Some(InteractionInterruption::Rejected(reason)),
                }
            );
            assert_eq!(hold.progress(), 0.);
            assert_eq!(
                hold.step_checked(3600., input).unwrap(),
                InteractionOutcome {
                    completed: false,
                    rejection: Some(reason),
                    interruption: None,
                }
            );
            // Neither the previous hold nor the blocked time is carried over.
            assert!(!hold.step_checked(0.25, valid).unwrap().completed);
            assert_eq!(hold.progress(), 0.25);
            assert!(hold.step_checked(0.75, valid).unwrap().completed);
        }
    }

    #[test]
    fn gate_precedence_is_stable_even_after_completion() {
        let mut hold = Interaction::new(1.).unwrap();
        hold.step(1., Some((1, 2))).unwrap();
        let mut input = InteractionInput::default();
        assert_eq!(hold.eligibility(input), Err(InteractionRejection::NoTarget));
        input.pair = Some((1, 2));
        assert_eq!(
            hold.eligibility(input),
            Err(InteractionRejection::FocusLost)
        );
        input.focused = true;
        assert_eq!(hold.eligibility(input), Err(InteractionRejection::NotHeld));
        input.held = true;
        assert_eq!(
            hold.eligibility(input),
            Err(InteractionRejection::OutOfRange)
        );
        input.in_range = true;
        assert_eq!(
            hold.eligibility(input),
            Err(InteractionRejection::PermissionDenied)
        );
        input.permitted = true;
        assert_eq!(
            hold.eligibility(input),
            Err(InteractionRejection::Obstructed)
        );
        input.visible = true;
        assert_eq!(
            hold.eligibility(input),
            Err(InteractionRejection::AlreadyCompleted)
        );
        assert_eq!(hold.progress(), 1.);
    }

    #[test]
    fn completion_latches_until_a_condition_fails_or_the_pair_changes() {
        let valid = eligible((1, 2));
        for (input, reason) in blocked_inputs() {
            let mut hold = Interaction::new(1.).unwrap();
            assert!(hold.step_checked(1., valid).unwrap().completed);
            let completed = hold;
            for _ in 0..3 {
                assert_eq!(
                    hold.step_checked(3600., valid).unwrap(),
                    InteractionOutcome {
                        completed: false,
                        rejection: Some(InteractionRejection::AlreadyCompleted),
                        interruption: None,
                    }
                );
                assert_same_state(hold, completed);
            }
            assert_eq!(
                hold.step_checked(0., input).unwrap(),
                InteractionOutcome {
                    completed: false,
                    rejection: Some(reason),
                    interruption: None,
                }
            );
            assert_eq!(hold.progress(), 0.);
            assert!(!hold.step_checked(0.5, valid).unwrap().completed);
            assert!(hold.step_checked(0.5, valid).unwrap().completed);
        }
        let mut hold = Interaction::new(1.).unwrap();
        hold.step_checked(1., valid).unwrap();
        assert_eq!(
            hold.step_checked(1., eligible((1, 3))).unwrap(),
            InteractionOutcome {
                completed: true,
                rejection: None,
                interruption: None,
            }
        );
    }

    #[test]
    fn actor_and_target_switches_report_the_discarded_hold() {
        for next_pair in [(1, 3), (9, 2)] {
            for dt in [0.25, 1.] {
                let mut hold = Interaction::new(1.).unwrap();
                hold.step_checked(0.75, eligible((1, 2))).unwrap();
                assert_eq!(
                    hold.step_checked(dt, eligible(next_pair)).unwrap(),
                    InteractionOutcome {
                        completed: dt == 1.,
                        rejection: None,
                        interruption: Some(InteractionInterruption::PairChanged {
                            previous: (1, 2),
                            current: next_pair,
                        }),
                    }
                );
                assert_eq!(hold.progress(), dt);
            }
        }
    }

    #[test]
    fn ineligible_replacement_cannot_receive_or_complete_the_old_hold() {
        let mut hold = Interaction::new(1.).unwrap();
        hold.step_checked(0.75, eligible((1, 2))).unwrap();
        let unavailable = InteractionInput {
            in_range: false,
            ..eligible((1, 3))
        };
        let result = hold.step_checked(1., unavailable).unwrap();
        assert!(!result.completed);
        assert_eq!(result.rejection, Some(InteractionRejection::OutOfRange));
        assert_eq!(
            result.interruption,
            Some(InteractionInterruption::Rejected(
                InteractionRejection::OutOfRange
            ))
        );
        assert_eq!(hold.pair, None);
        assert!(!hold.step_checked(0.25, eligible((1, 3))).unwrap().completed);
        assert_eq!(hold.progress(), 0.25);
    }

    #[test]
    fn invalid_time_is_transactional_before_gates_switches_and_latches() {
        let valid = eligible((1, 2));
        for elapsed in [0., 0.5, 1.] {
            let mut hold = Interaction::new(1.).unwrap();
            if elapsed > 0. {
                hold.step_checked(elapsed, valid).unwrap();
            }
            let before = hold;
            for dt in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1, 3601.] {
                for input in [valid, eligible((9, 3)), InteractionInput::default()]
                    .into_iter()
                    .chain(blocked_inputs().map(|(input, _)| input))
                {
                    assert_eq!(hold.step_checked(dt, input), Err(LoopError::Invalid));
                    assert_same_state(hold, before);
                    assert_eq!(hold.step(dt, input.pair), Err(LoopError::Invalid));
                    assert_same_state(hold, before);
                }
            }
        }
    }

    #[test]
    fn legacy_and_checked_steps_share_progress_cancellation_and_completion() {
        let mut legacy = Interaction::new(1.).unwrap();
        let mut checked = legacy;
        let valid = eligible((1, 2));
        let switched = eligible((9, 3));
        for (dt, input) in [
            (0., valid),
            (0.5, valid),
            (0.5, switched),
            (0., InteractionInput::default()),
            (0.25, switched),
            (0.75, switched),
            (1., switched),
            (0.5, valid),
            (0.5, valid),
        ] {
            assert_eq!(
                legacy.step(dt, input.pair).unwrap(),
                checked.step_checked(dt, input).unwrap().completed
            );
            assert_same_state(checked, legacy);
        }
        checked.cancel();
        legacy.cancel();
        assert_same_state(checked, legacy);
        assert_eq!(checked.eligibility(valid), Ok(()));
        assert_eq!(checked.progress(), 0.);
    }

    #[test]
    fn fixed_ticks_complete_once_and_queries_between_ticks_do_not_advance() {
        let mut hold = Interaction::new(1.).unwrap();
        let input = eligible((1, 2));
        let mut completions = 0;
        for tick in 0..120 {
            let before = hold;
            for _ in 0..3 {
                let _ = hold.eligibility(input);
            }
            assert_same_state(hold, before);
            let result = hold.step_checked(1. / 60., input).unwrap();
            completions += usize::from(result.completed);
            assert_eq!(result.completed, tick == 59);
        }
        assert_eq!(completions, 1);
        assert_eq!(hold.progress(), 1.);
    }
}
