use super::{AnimationBank, AnimationError, Animator, EventBuffer};
use std::sync::Arc;

pub const MAX_ANIMATION_STATES: usize = 64;
pub const MAX_ANIMATION_TRANSITIONS: usize = 256;
pub const MAX_ANIMATION_PARAMETERS: usize = 32;
#[derive(Clone, Copy, Debug)]
pub struct AnimationState {
    pub clip: u16,
    pub speed: f32,
}
#[derive(Clone, Copy, Debug)]
pub enum AnimationCondition {
    Greater {
        parameter: u8,
        value: f32,
    },
    Less {
        parameter: u8,
        value: f32,
    },
    /// Latched separately from scalar parameters; consumed only when its
    /// transition wins. Other pending triggers survive until cleared/consumed.
    Trigger(u8),
    Finished,
}
#[derive(Clone, Debug)]
pub struct AnimationTransition {
    pub from: u16,
    pub to: u16,
    pub seconds: f32,
    /// Minimum normalized phase (0..=1). Repeating clips wrap at one, so use
    /// <1 for a looping exit window, or Finished for one-shot completion.
    pub exit_time: f32,
    /// AND conditions; an empty list is unconditional. Authored order breaks ties.
    pub conditions: Vec<AnimationCondition>,
}
/// Read-only result of checking one authored edge against a graph player.
///
/// Reports the first blocking check in runtime order: source state, exit time,
/// then conditions in authored order. Eligibility is independent of pause, the
/// next update's delta, and competing edges. See
/// [`GraphPlayer::transition_eligibility`] for an example and
/// [`GraphPlayer::selected_transition`] for the edge that would actually win.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationTransitionEligibility {
    /// The edge's source is not the current graph state.
    DifferentState,
    /// The animator's current normalized time is below the edge's exit time.
    ExitTimeNotReached,
    /// The first false condition; `condition` indexes the edge's conditions.
    ConditionNotMet { condition: usize },
    /// All checks pass. An earlier eligible edge still takes priority.
    Eligible,
}
#[derive(Debug)]
struct GraphData {
    bank: AnimationBank,
    states: Vec<AnimationState>,
    transitions: Vec<AnimationTransition>,
    parameters: usize,
}
#[derive(Clone, Debug)]
pub struct AnimationGraph(Arc<GraphData>);
impl AnimationGraph {
    pub fn new(
        bank: &AnimationBank,
        states: Vec<AnimationState>,
        transitions: Vec<AnimationTransition>,
        parameters: usize,
    ) -> Result<Self, AnimationError> {
        if states.is_empty()
            || states.len() > MAX_ANIMATION_STATES
            || transitions.len() > MAX_ANIMATION_TRANSITIONS
            || parameters > MAX_ANIMATION_PARAMETERS
        {
            return Err(AnimationError::Capacity);
        }
        if states.iter().any(|s| {
            s.clip as usize >= bank.clips().len()
                || !s.speed.is_finite()
                || !(0.0..=8.).contains(&s.speed)
        }) {
            return Err(AnimationError::InvalidClip);
        }
        for t in &transitions {
            if t.from as usize >= states.len()
                || t.to as usize >= states.len()
                || t.from == t.to
                || !t.seconds.is_finite()
                || !(0.0..=60.).contains(&t.seconds)
                || !t.exit_time.is_finite()
                || !(0.0..=1.).contains(&t.exit_time)
                || t.conditions.len() > 8
            {
                return Err(AnimationError::InvalidClip);
            }
            for c in &t.conditions {
                let parameter = match *c {
                    AnimationCondition::Greater { parameter, value }
                    | AnimationCondition::Less { parameter, value } => {
                        if !value.is_finite() {
                            return Err(AnimationError::InvalidKey);
                        }
                        Some(parameter)
                    }
                    AnimationCondition::Trigger(parameter) => Some(parameter),
                    AnimationCondition::Finished => None,
                };
                if parameter.is_some_and(|p| p as usize >= parameters) {
                    return Err(AnimationError::InvalidKey);
                }
            }
        }
        Ok(Self(Arc::new(GraphData {
            bank: bank.clone(),
            states,
            transitions,
            parameters,
        })))
    }
    pub fn states(&self) -> &[AnimationState] {
        &self.0.states
    }
    pub fn transitions(&self) -> &[AnimationTransition] {
        &self.0.transitions
    }
    pub fn parameter_count(&self) -> usize {
        self.0.parameters
    }
}
/// Bounded state graph with deterministic authored transition priority. Examines
/// transitions once before each nonzero, unpaused update; completion conditions
/// become eligible on the next update after reaching the end. At most one edge
/// wins per call, so unconditional graph cycles cannot spin inside one frame.
#[derive(Clone, Debug)]
pub struct GraphPlayer {
    graph: AnimationGraph,
    animator: Animator,
    state: usize,
    parameters: [f32; MAX_ANIMATION_PARAMETERS],
    triggers: [bool; MAX_ANIMATION_PARAMETERS],
}
impl GraphPlayer {
    pub fn new(graph: &AnimationGraph, state: usize) -> Result<Self, AnimationError> {
        let definition = graph
            .0
            .states
            .get(state)
            .ok_or(AnimationError::InvalidClip)?;
        let mut animator = Animator::new(&graph.0.bank, definition.clip as usize)?;
        animator.set_speed(definition.speed)?;
        Ok(Self {
            graph: graph.clone(),
            animator,
            state,
            parameters: [0.; MAX_ANIMATION_PARAMETERS],
            triggers: [false; MAX_ANIMATION_PARAMETERS],
        })
    }
    /// Restart an authored state for respawn/replay without rebuilding storage.
    /// Clears scalar parameters and pending triggers; preserves pause state.
    pub fn reset(&mut self, state: usize) -> Result<(), AnimationError> {
        let definition = self
            .graph
            .0
            .states
            .get(state)
            .ok_or(AnimationError::InvalidClip)?;
        self.animator.play(definition.clip as usize)?;
        self.animator.set_speed(definition.speed)?;
        self.state = state;
        self.parameters.fill(0.);
        self.triggers.fill(false);
        Ok(())
    }
    pub fn state(&self) -> usize {
        self.state
    }
    pub fn animator(&self) -> &Animator {
        &self.animator
    }
    /// Pose layers, seek and pause/speed controls are available on the player.
    /// Calling play/transition directly overrides presentation until the next
    /// graph transition; it does not change the graph's state index.
    pub fn animator_mut(&mut self) -> &mut Animator {
        &mut self.animator
    }
    pub fn set_parameter(&mut self, parameter: usize, value: f32) -> Result<(), AnimationError> {
        if parameter >= self.graph.0.parameters || !value.is_finite() {
            return Err(AnimationError::InvalidKey);
        }
        self.parameters[parameter] = value;
        Ok(())
    }
    pub fn parameter(&self, parameter: usize) -> Option<f32> {
        (parameter < self.graph.0.parameters).then(|| self.parameters[parameter])
    }
    /// Scalar values in stable parameter-index order, with exactly
    /// [`AnimationGraph::parameter_count`] entries. Borrows existing storage;
    /// trigger latches are independent and available via [`Self::pending_triggers`].
    ///
    /// ```
    /// use ferravex_play::{animation::*, scene::Transform};
    /// let clip = AnimationClip::new(1, 1., LoopMode::Repeat, vec![], vec![])?;
    /// let bank = AnimationBank::new(vec![Transform::default()], vec![clip])?;
    /// let graph = AnimationGraph::new(
    ///     &bank, vec![AnimationState { clip: 0, speed: 1. }], vec![], 2,
    /// )?;
    /// let mut player = GraphPlayer::new(&graph, 0)?;
    /// player.set_parameter(1, 2.)?;
    /// player.trigger(0)?;
    /// assert_eq!(player.parameters(), &[0., 2.]);
    /// assert_eq!(player.pending_triggers(), &[true, false]);
    /// // Inspect both channels using the same stable parameter index.
    /// for (index, (&value, &pending)) in player.parameters().iter()
    ///     .zip(player.pending_triggers()).enumerate()
    /// {
    ///     assert_eq!(player.parameter(index), Some(value));
    ///     assert_eq!(pending, index == 0);
    /// }
    /// # Ok::<(), AnimationError>(())
    /// ```
    pub fn parameters(&self) -> &[f32] {
        &self.parameters[..self.graph.0.parameters]
    }
    /// Pending trigger latches in the same stable index order and length as
    /// [`Self::parameters`]. Inspection never consumes a trigger or allocates.
    /// See that method's example for inspecting both parameter channels.
    pub fn pending_triggers(&self) -> &[bool] {
        &self.triggers[..self.graph.0.parameters]
    }
    pub fn trigger(&mut self, parameter: usize) -> Result<(), AnimationError> {
        if parameter >= self.graph.0.parameters {
            return Err(AnimationError::InvalidKey);
        }
        self.triggers[parameter] = true;
        Ok(())
    }
    pub fn clear_trigger(&mut self, parameter: usize) -> Result<(), AnimationError> {
        if parameter >= self.graph.0.parameters {
            return Err(AnimationError::InvalidKey);
        }
        self.triggers[parameter] = false;
        Ok(())
    }
    /// Explain an edge using the current state, animator timeline and parameters,
    /// without advancing playback or consuming triggers. `transition` is a stable
    /// index into [`AnimationGraph::transitions`]; an invalid index returns `None`.
    /// Condition failure indices refer to that edge's authored conditions.
    ///
    /// This checks eligibility even when paused. It does not predict the timeline
    /// after an update: exit/completion checks use the current animator timeline,
    /// including any direct overrides through [`Self::animator_mut`]. Use
    /// [`Self::selected_transition`] to account for pause, delta and edge priority.
    ///
    /// ```
    /// use ferravex_play::{animation::*, scene::Transform};
    /// let clip = AnimationClip::new(1, 1., LoopMode::Clamp, vec![], vec![])?;
    /// let bank = AnimationBank::new(vec![Transform::default()], vec![clip])?;
    /// let graph = AnimationGraph::new(
    ///     &bank,
    ///     vec![AnimationState { clip: 0, speed: 1. }; 2],
    ///     vec![AnimationTransition {
    ///         from: 0, to: 1, seconds: 0., exit_time: 0.,
    ///         conditions: vec![AnimationCondition::Trigger(0)],
    ///     }],
    ///     1,
    /// )?;
    /// let mut player = GraphPlayer::new(&graph, 0)?;
    /// assert_eq!(player.transition_eligibility(0),
    ///     Some(AnimationTransitionEligibility::ConditionNotMet { condition: 0 }));
    /// player.trigger(0)?;
    /// assert_eq!(player.transition_eligibility(0),
    ///     Some(AnimationTransitionEligibility::Eligible));
    /// assert_eq!(player.selected_transition(0.)?, None);
    /// assert_eq!(player.selected_transition(0.125)?, Some(0));
    /// assert_eq!(player.pending_triggers(), &[true]); // still latched
    /// player.advance(0.125, &mut EventBuffer::<0>::default())?;
    /// assert_eq!(player.state(), 1);
    /// assert_eq!(player.pending_triggers(), &[false]);
    /// assert_eq!(player.transition_eligibility(usize::MAX), None);
    /// # Ok::<(), AnimationError>(())
    /// ```
    pub fn transition_eligibility(
        &self,
        transition: usize,
    ) -> Option<AnimationTransitionEligibility> {
        self.graph
            .0
            .transitions
            .get(transition)
            .map(|transition| self.eligibility(transition))
    }
    /// Return the authored index of the edge that the next [`Self::advance`]
    /// with this `dt` would select, without changing any state or emitting events.
    /// See [`Self::transition_eligibility`] for a usage example.
    ///
    /// Uses the same time validation as `advance`: non-finite, negative or
    /// greater-than-[`super::MAX_ANIMATION_DT`] input returns
    /// [`AnimationError::InvalidTime`], even while paused. Zero delta or pause
    /// returns `Ok(None)`; otherwise the first eligible edge wins. Zero playback
    /// speed does not prevent selection, matching `advance`. No future time is
    /// simulated, so reaching an exit/completion threshold during this update
    /// only makes the edge eligible for the following update.
    ///
    /// Work is bounded by the authored transitions and conditions and does not
    /// allocate. This is a current-state query, not a reservation: subsequent
    /// player changes may alter the selected edge.
    pub fn selected_transition(&self, dt: f32) -> Result<Option<usize>, AnimationError> {
        if !dt.is_finite() || !(0.0..=super::MAX_ANIMATION_DT).contains(&dt) {
            return Err(AnimationError::InvalidTime);
        }
        if dt == 0. || self.animator.timeline().paused() {
            return Ok(None);
        }
        Ok(self.graph.0.transitions.iter().position(|transition| {
            self.eligibility(transition) == AnimationTransitionEligibility::Eligible
        }))
    }
    fn eligibility(&self, transition: &AnimationTransition) -> AnimationTransitionEligibility {
        if transition.from as usize != self.state {
            return AnimationTransitionEligibility::DifferentState;
        }
        if self.animator.timeline().normalized_time() < transition.exit_time {
            return AnimationTransitionEligibility::ExitTimeNotReached;
        }
        for (condition, check) in transition.conditions.iter().enumerate() {
            let met = match *check {
                AnimationCondition::Greater { parameter, value } => {
                    self.parameters[parameter as usize] > value
                }
                AnimationCondition::Less { parameter, value } => {
                    self.parameters[parameter as usize] < value
                }
                AnimationCondition::Trigger(parameter) => self.triggers[parameter as usize],
                AnimationCondition::Finished => self.animator.timeline().finished(),
            };
            if !met {
                return AnimationTransitionEligibility::ConditionNotMet { condition };
            }
        }
        AnimationTransitionEligibility::Eligible
    }
    pub fn advance<const N: usize>(
        &mut self,
        dt: f32,
        events: &mut EventBuffer<N>,
    ) -> Result<(), AnimationError> {
        if let Some(index) = self.selected_transition(dt)? {
            let t = &self.graph.0.transitions[index];
            let state = self.graph.0.states[t.to as usize];
            self.animator.transition(state.clip as usize, t.seconds)?;
            self.animator.set_speed(state.speed)?;
            self.state = t.to as usize;
            for c in &t.conditions {
                if let AnimationCondition::Trigger(p) = c {
                    self.triggers[*p as usize] = false;
                }
            }
        }
        self.animator.advance(dt, events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        animation::{AnimationClip, AnimationEvent, LoopMode},
        scene::Transform,
    };

    fn edge(from: u16, to: u16, conditions: Vec<AnimationCondition>) -> AnimationTransition {
        AnimationTransition {
            from,
            to,
            seconds: 0.25,
            exit_time: 0.,
            conditions,
        }
    }

    fn graph(transitions: Vec<AnimationTransition>, parameters: usize) -> AnimationGraph {
        let clips = [LoopMode::Repeat, LoopMode::Clamp, LoopMode::Repeat]
            .into_iter()
            .enumerate()
            .map(|(index, mode)| {
                AnimationClip::new(
                    1,
                    1.,
                    mode,
                    vec![],
                    vec![AnimationEvent {
                        time: 0.,
                        id: index as u32 + 10,
                    }],
                )
                .unwrap()
            })
            .collect();
        let bank = AnimationBank::new(vec![Transform::default()], clips).unwrap();
        AnimationGraph::new(
            &bank,
            vec![
                AnimationState { clip: 0, speed: 1. },
                AnimationState { clip: 1, speed: 2. },
                AnimationState { clip: 2, speed: 1. },
            ],
            transitions,
            parameters,
        )
        .unwrap()
    }

    #[test]
    fn parameter_inspection_has_stable_indices_and_only_authored_entries() {
        for count in [0, 3, MAX_ANIMATION_PARAMETERS] {
            let graph = graph(vec![], count);
            let mut player = GraphPlayer::new(&graph, 0).unwrap();
            for index in (0..count).rev() {
                player.set_parameter(index, index as f32 - 4.).unwrap();
                if index % 2 == 0 {
                    player.trigger(index).unwrap();
                }
            }
            assert_eq!(player.parameters().len(), count);
            assert_eq!(player.pending_triggers().len(), count);
            for (index, (&value, &pending)) in player
                .parameters()
                .iter()
                .zip(player.pending_triggers())
                .enumerate()
            {
                assert_eq!(value, index as f32 - 4.);
                assert_eq!(player.parameter(index), Some(value));
                assert_eq!(pending, index % 2 == 0);
            }
            let before = player.clone();
            for index in [count, usize::MAX] {
                assert_eq!(player.parameter(index), None);
                assert_eq!(
                    player.set_parameter(index, 1.),
                    Err(AnimationError::InvalidKey)
                );
                assert_eq!(player.trigger(index), Err(AnimationError::InvalidKey));
                assert_eq!(player.clear_trigger(index), Err(AnimationError::InvalidKey));
            }
            for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                assert_eq!(
                    player.set_parameter(0, value),
                    Err(AnimationError::InvalidKey)
                );
            }
            assert_eq!(player.parameters(), before.parameters());
            assert_eq!(player.pending_triggers(), before.pending_triggers());
            if count > 0 {
                player.clear_trigger(0).unwrap();
                assert!(!player.pending_triggers()[0]);
                assert_eq!(player.parameters(), before.parameters());
                player.trigger(0).unwrap();
            }
            player.animator_mut().set_paused(true);
            assert_eq!(player.reset(usize::MAX), Err(AnimationError::InvalidClip));
            assert_eq!(player.parameters(), before.parameters());
            assert_eq!(player.pending_triggers(), before.pending_triggers());
            player.reset(1).unwrap();
            assert_eq!(player.parameters(), &vec![0.; count]);
            assert_eq!(player.pending_triggers(), &vec![false; count]);
            assert!(player.animator().timeline().paused());
        }
    }

    #[test]
    fn eligibility_reports_first_blocker_and_strict_condition_boundaries() {
        use AnimationTransitionEligibility::*;
        let conditions = vec![
            AnimationCondition::Greater {
                parameter: 0,
                value: 1.,
            },
            AnimationCondition::Less {
                parameter: 1,
                value: 2.,
            },
            AnimationCondition::Trigger(2),
            AnimationCondition::Finished,
        ];
        let mut from_other_state = edge(0, 1, conditions.clone());
        from_other_state.exit_time = 0.75;
        let mut from_current_state = edge(1, 2, conditions);
        from_current_state.exit_time = 0.5;
        let graph = graph(vec![from_other_state, from_current_state], 3);
        let mut player = GraphPlayer::new(&graph, 1).unwrap();
        assert_eq!(player.transition_eligibility(0), Some(DifferentState));
        assert_eq!(player.transition_eligibility(1), Some(ExitTimeNotReached));
        player.animator_mut().seek(0.5).unwrap();
        player.set_parameter(0, 1.).unwrap();
        assert_eq!(
            player.transition_eligibility(1),
            Some(ConditionNotMet { condition: 0 })
        );
        player.set_parameter(0, 1.5).unwrap();
        player.set_parameter(1, 2.).unwrap();
        assert_eq!(
            player.transition_eligibility(1),
            Some(ConditionNotMet { condition: 1 })
        );
        player.set_parameter(1, -1.).unwrap();
        assert_eq!(
            player.transition_eligibility(1),
            Some(ConditionNotMet { condition: 2 })
        );
        player.trigger(2).unwrap();
        assert_eq!(
            player.transition_eligibility(1),
            Some(ConditionNotMet { condition: 3 })
        );
        player.animator_mut().seek(1.).unwrap();
        assert_eq!(player.transition_eligibility(1), Some(Eligible));
        assert_eq!(player.transition_eligibility(0), Some(DifferentState));
        assert_eq!(player.transition_eligibility(2), None);
        assert_eq!(player.transition_eligibility(usize::MAX), None);
        assert!(player.pending_triggers()[2]);
    }

    #[test]
    fn repeated_inspection_preserves_priority_triggers_events_and_one_edge_per_update() {
        use AnimationCondition::*;
        let graph = graph(
            vec![
                edge(2, 0, vec![]),
                edge(
                    0,
                    2,
                    vec![
                        Trigger(2),
                        Greater {
                            parameter: 0,
                            value: 10.,
                        },
                    ],
                ),
                edge(
                    0,
                    1,
                    vec![
                        Trigger(0),
                        Trigger(0),
                        Less {
                            parameter: 1,
                            value: 1.,
                        },
                    ],
                ),
                edge(0, 2, vec![Trigger(2)]),
                edge(1, 2, vec![Trigger(2)]),
            ],
            3,
        );
        let mut player = GraphPlayer::new(&graph, 0).unwrap();
        player.set_parameter(0, -4.).unwrap();
        player.set_parameter(1, 0.5).unwrap();
        player.trigger(0).unwrap();
        player.trigger(2).unwrap();
        let mut control = player.clone();
        for _ in 0..8 {
            assert_eq!(player.selected_transition(0.125), Ok(Some(2)));
            assert_eq!(
                player.transition_eligibility(1),
                Some(AnimationTransitionEligibility::ConditionNotMet { condition: 1 })
            );
            assert_eq!(
                player.transition_eligibility(2),
                Some(AnimationTransitionEligibility::Eligible)
            );
            assert_eq!(
                player.transition_eligibility(3),
                Some(AnimationTransitionEligibility::Eligible)
            );
            assert_eq!(player.parameters(), control.parameters());
            assert_eq!(player.pending_triggers(), control.pending_triggers());
            assert_eq!(player.state(), control.state());
            assert_eq!(player.animator().timeline().time(), 0.);
            assert_eq!(player.animator().pose(), control.animator().pose());
            assert!(!player.animator().transitioning());
        }
        let mut actual = EventBuffer::<8>::default();
        let mut expected = EventBuffer::<8>::default();
        player.advance(0.125, &mut actual).unwrap();
        control.advance(0.125, &mut expected).unwrap();
        assert_eq!(actual.hits(), expected.hits());
        assert_eq!(actual.hits().len(), 1);
        assert_eq!(actual.hits()[0].id, 11);
        assert_eq!(actual.hits()[0].clip, 1);
        assert_eq!(player.state(), 1);
        assert_eq!(player.parameters(), &[-4., 0.5, 0.]);
        assert_eq!(player.pending_triggers(), &[false, false, true]);
        assert_eq!(player.animator().timeline().time(), 0.25);
        assert_eq!(player.animator().timeline().speed(), 2.);
        assert!(player.animator().transitioning());
        assert_eq!(player.selected_transition(0.125), Ok(Some(4)));
        player.advance(0.125, &mut actual).unwrap();
        assert_eq!(player.state(), 2);
        assert_eq!(player.pending_triggers(), &[false; 3]);
        assert_eq!(player.selected_transition(0.125), Ok(Some(0)));
    }

    #[test]
    fn selection_matches_time_validation_pause_zero_delta_and_zero_speed() {
        let graph = graph(vec![edge(0, 1, vec![AnimationCondition::Trigger(0)])], 1);
        let mut player = GraphPlayer::new(&graph, 0).unwrap();
        player.trigger(0).unwrap();
        let mut events = EventBuffer::<8>::default();
        player.animator_mut().advance(0.125, &mut events).unwrap();
        let before_events = events.hits().to_vec();
        for paused in [false, true] {
            player.animator_mut().set_paused(paused);
            for dt in [
                -1.,
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                super::super::MAX_ANIMATION_DT + 1.,
            ] {
                assert_eq!(
                    player.selected_transition(dt),
                    Err(AnimationError::InvalidTime)
                );
                assert_eq!(
                    player.advance(dt, &mut events),
                    Err(AnimationError::InvalidTime)
                );
                assert_eq!(player.state(), 0);
                assert_eq!(player.animator().timeline().time(), 0.125);
                assert_eq!(player.pending_triggers(), &[true]);
                assert_eq!(events.hits(), before_events);
                assert_eq!(events.dropped(), 0);
            }
        }
        assert_eq!(
            player.transition_eligibility(0),
            Some(AnimationTransitionEligibility::Eligible)
        );
        assert_eq!(player.selected_transition(0.5), Ok(None));
        player.advance(0.5, &mut events).unwrap();
        assert_eq!(player.state(), 0);
        assert_eq!(player.animator().timeline().time(), 0.125);
        assert_eq!(player.pending_triggers(), &[true]);
        assert!(events.hits().is_empty());
        player.animator_mut().set_paused(false);
        for dt in [0., -0.] {
            assert_eq!(player.selected_transition(dt), Ok(None));
            player.advance(dt, &mut events).unwrap();
            assert_eq!(player.state(), 0);
            assert_eq!(player.pending_triggers(), &[true]);
        }
        player.animator_mut().set_speed(0.).unwrap();
        assert_eq!(
            player.selected_transition(super::super::MAX_ANIMATION_DT),
            Ok(Some(0))
        );
        player.advance(0.125, &mut events).unwrap();
        assert_eq!(player.state(), 1);
        assert_eq!(player.pending_triggers(), &[false]);
        assert_eq!(player.animator().timeline().speed(), 2.);
    }

    #[test]
    fn exit_time_and_completion_are_selected_only_on_the_following_update() {
        let mut exit = edge(0, 2, vec![]);
        exit.exit_time = 0.5;
        let graph = graph(
            vec![exit, edge(1, 2, vec![AnimationCondition::Finished])],
            0,
        );
        for initial_state in [0, 1] {
            let mut player = GraphPlayer::new(&graph, initial_state).unwrap();
            let mut events = EventBuffer::<0>::default();
            assert_eq!(player.selected_transition(0.5), Ok(None));
            player.advance(0.5, &mut events).unwrap();
            assert_eq!(player.state(), initial_state);
            assert_eq!(player.selected_transition(0.125), Ok(Some(initial_state)));
            player.advance(0.125, &mut events).unwrap();
            assert_eq!(player.state(), 2);
        }
    }

    #[test]
    fn loop_wrapping_and_direct_animator_overrides_use_the_current_timeline() {
        let mut exit_at_end = edge(0, 2, vec![]);
        exit_at_end.exit_time = 1.;
        let mut exit_window = edge(0, 1, vec![]);
        exit_window.exit_time = 0.75;
        let graph = graph(vec![exit_at_end, exit_window], 0);
        let mut player = GraphPlayer::new(&graph, 0).unwrap();
        player.animator_mut().seek(0.5).unwrap();
        assert_eq!(player.selected_transition(0.5), Ok(None));
        player
            .advance(0.5, &mut EventBuffer::<0>::default())
            .unwrap();
        assert_eq!(player.animator().timeline().cycles(), 1);
        assert_eq!(player.animator().timeline().time(), 0.);
        assert_eq!(player.selected_transition(0.125), Ok(None));
        player.animator_mut().seek(0.75).unwrap();
        assert_eq!(player.selected_transition(0.125), Ok(Some(1)));
        player.animator_mut().play(1).unwrap();
        player.animator_mut().seek(1.).unwrap();
        assert_eq!(player.state(), 0);
        assert_eq!(player.selected_transition(0.125), Ok(Some(0)));
        player
            .advance(0.125, &mut EventBuffer::<0>::default())
            .unwrap();
        assert_eq!(player.state(), 2);
    }

    #[test]
    fn diagnostics_handle_empty_and_maximum_transition_lists() {
        let empty = graph(vec![], 0);
        let player = GraphPlayer::new(&empty, 0).unwrap();
        assert_eq!(player.transition_eligibility(0), None);
        assert_eq!(player.selected_transition(1.), Ok(None));

        let mut edges = vec![edge(1, 2, vec![]); MAX_ANIMATION_TRANSITIONS];
        edges[MAX_ANIMATION_TRANSITIONS - 1] = edge(0, 1, vec![]);
        let full = graph(edges, 0);
        let mut player = GraphPlayer::new(&full, 0).unwrap();
        assert_eq!(
            player.transition_eligibility(MAX_ANIMATION_TRANSITIONS),
            None
        );
        assert_eq!(
            player.selected_transition(0.125),
            Ok(Some(MAX_ANIMATION_TRANSITIONS - 1))
        );
        player
            .advance(0.125, &mut EventBuffer::<0>::default())
            .unwrap();
        assert_eq!(player.state(), 1);
        assert_eq!(player.selected_transition(0.125), Ok(Some(0)));
        player
            .advance(0.125, &mut EventBuffer::<0>::default())
            .unwrap();
        assert_eq!(player.state(), 2);
        assert_eq!(player.selected_transition(0.125), Ok(None));
    }
}
