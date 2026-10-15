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
    pub fn advance<const N: usize>(
        &mut self,
        dt: f32,
        events: &mut EventBuffer<N>,
    ) -> Result<(), AnimationError> {
        if !dt.is_finite() || !(0.0..=super::MAX_ANIMATION_DT).contains(&dt) {
            return Err(AnimationError::InvalidTime);
        }
        if dt > 0. && !self.animator.timeline().paused() {
            let next = self.graph.0.transitions.iter().find(|t| {
                t.from as usize == self.state
                    && self.animator.timeline().normalized_time() >= t.exit_time
                    && t.conditions.iter().all(|c| match *c {
                        AnimationCondition::Greater { parameter, value } => {
                            self.parameters[parameter as usize] > value
                        }
                        AnimationCondition::Less { parameter, value } => {
                            self.parameters[parameter as usize] < value
                        }
                        AnimationCondition::Trigger(p) => self.triggers[p as usize],
                        AnimationCondition::Finished => self.animator.timeline().finished(),
                    })
            });
            if let Some(t) = next {
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
        }
        self.animator.advance(dt, events)
    }
}
