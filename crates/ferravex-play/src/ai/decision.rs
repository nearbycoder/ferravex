use super::AiError;
pub const MAX_DECISION_STATES: u16 = 64;
pub const MAX_DECISION_EDGES: usize = 256;
/// First matching edge wins. all/none refer to host-authored sensor bits. None
/// as source matches any state. Matching the current destination holds that
/// state and blocks lower-priority edges without resetting its timer.
#[derive(Clone, Copy, Debug)]
pub struct DecisionEdge {
    pub from: Option<u16>,
    pub to: u16,
    pub all: u64,
    pub none: u64,
    pub after: f32,
}
pub struct DecisionGraph {
    states: u16,
    initial: u16,
    edges: Vec<DecisionEdge>,
}
impl DecisionGraph {
    pub fn new(states: u16, initial: u16, edges: &[DecisionEdge]) -> Result<Self, AiError> {
        if states == 0
            || states > MAX_DECISION_STATES
            || initial >= states
            || edges.len() > MAX_DECISION_EDGES
            || edges.iter().any(|e| {
                e.to >= states
                    || e.from.is_some_and(|s| s >= states || s == e.to)
                    || e.all & e.none != 0
                    || !e.after.is_finite()
                    || !(0. ..=3600.).contains(&e.after)
            })
        {
            return Err(AiError::InvalidConfiguration);
        }
        Ok(Self {
            states,
            initial,
            edges: edges.to_vec(),
        })
    }
    pub fn initial(&self) -> DecisionState {
        DecisionState {
            state: self.initial,
            elapsed: 0.,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecisionState {
    state: u16,
    elapsed: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecisionChange {
    pub from: u16,
    pub to: u16,
}
impl DecisionState {
    pub fn state(&self) -> u16 {
        self.state
    }
    pub fn elapsed(&self) -> f64 {
        self.elapsed
    }
    pub fn reset(&mut self, graph: &DecisionGraph) {
        *self = graph.initial();
    }
    /// At most one transition per call, including dt=0. Timers stop at 3600s;
    /// transition overshoot is not applied to the destination state.
    pub fn update(
        &mut self,
        graph: &DecisionGraph,
        flags: u64,
        dt: f32,
    ) -> Result<Option<DecisionChange>, AiError> {
        if self.state >= graph.states {
            return Err(AiError::InvalidState);
        }
        if !dt.is_finite() || !(0. ..=60.).contains(&dt) {
            return Err(AiError::InvalidInput);
        }
        self.elapsed = (self.elapsed + f64::from(dt)).min(3600.);
        for edge in &graph.edges {
            if edge.from.is_none_or(|s| s == self.state)
                && flags & edge.all == edge.all
                && flags & edge.none == 0
                && self.elapsed >= f64::from(edge.after)
            {
                if edge.to == self.state {
                    return Ok(None);
                }
                let change = DecisionChange {
                    from: self.state,
                    to: edge.to,
                };
                self.state = edge.to;
                self.elapsed = 0.;
                return Ok(Some(change));
            }
        }
        Ok(None)
    }
}
