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

impl DecisionEdge {
    fn inspect(
        &self,
        index: usize,
        state: u16,
        flags: u64,
        elapsed: f64,
    ) -> DecisionEdgeExplanation {
        DecisionEdgeExplanation {
            index,
            source_matches: self.from.is_none_or(|source| source == state),
            missing_flags: self.all & !flags,
            forbidden_flags: self.none & flags,
            remaining_seconds: (f64::from(self.after) - elapsed).max(0.),
            outcome: DecisionEdgeOutcome::Rejected,
        }
    }
}

/// The first invalid graph setting or edge, returned without allocating.
/// Edge indices refer to the original slice in priority order. See
/// [`DecisionGraph::validate`] for check ordering and an example.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecisionGraphError {
    /// The count must be in `1..=MAX_DECISION_STATES`.
    InvalidStateCount { states: u16 },
    /// The initial state must be below the state count.
    InvalidInitialState { initial: u16, states: u16 },
    /// The edge count exceeds [`MAX_DECISION_EDGES`].
    TooManyEdges { len: usize },
    /// This destination must be below the state count.
    InvalidDestination { edge: usize, to: u16, states: u16 },
    /// This explicit source must be below the state count.
    InvalidSource { edge: usize, from: u16, states: u16 },
    /// An explicit source equals its destination; wildcard hold edges are valid.
    SelfTransition { edge: usize, state: u16 },
    /// These sensor bits occur in both `all` and `none`.
    ConflictingGuards { edge: usize, flags: u64 },
    /// The delay must be finite and in `0..=3600` seconds.
    InvalidDelay { edge: usize },
}

impl std::fmt::Display for DecisionGraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidStateCount { states } => write!(
                f,
                "decision state count {states} must be in 1..={MAX_DECISION_STATES}"
            ),
            Self::InvalidInitialState { initial, states } => {
                write!(f, "initial state {initial} must be below {states}")
            }
            Self::TooManyEdges { len } => {
                write!(f, "decision edge count {len} exceeds {MAX_DECISION_EDGES}")
            }
            Self::InvalidDestination { edge, to, states } => {
                write!(f, "edge {edge} destination {to} must be below {states}")
            }
            Self::InvalidSource { edge, from, states } => {
                write!(f, "edge {edge} source {from} must be below {states}")
            }
            Self::SelfTransition { edge, state } => {
                write!(
                    f,
                    "edge {edge} explicitly transitions state {state} to itself"
                )
            }
            Self::ConflictingGuards { edge, flags } => write!(
                f,
                "edge {edge} requires and forbids sensor bits {flags:#018x}"
            ),
            Self::InvalidDelay { edge } => {
                write!(
                    f,
                    "edge {edge} delay must be finite and in 0..=3600 seconds"
                )
            }
        }
    }
}

impl std::error::Error for DecisionGraphError {}

impl From<DecisionGraphError> for AiError {
    fn from(_: DecisionGraphError) -> Self {
        Self::InvalidConfiguration
    }
}

/// An immutable, bounded graph in authored priority order.
/// Multi-state cycles are allowed: each update follows at most one edge.
#[derive(Debug)]
pub struct DecisionGraph {
    states: u16,
    initial: u16,
    edges: Vec<DecisionEdge>,
}
impl DecisionGraph {
    /// Construct a graph using the existing coarse error category.
    /// Use [`Self::new_checked`] for an actionable configuration diagnostic.
    pub fn new(states: u16, initial: u16, edges: &[DecisionEdge]) -> Result<Self, AiError> {
        Self::new_checked(states, initial, edges).map_err(AiError::from)
    }

    /// Validate without allocating or building the graph.
    ///
    /// Checks state count, initial state, then edge count. Edges are checked in
    /// slice order: destination, source, explicit self-transition, conflicting
    /// sensor masks, then delay. Only the first error is returned. Cycles,
    /// duplicate edges, unreachable states and priority-shadowed edges are
    /// allowed; this checks configuration validity, not behavioral reachability.
    ///
    /// ```
    /// use ferravex_play::ai::{DecisionEdge, DecisionGraph, DecisionGraphError};
    /// let edge = DecisionEdge {
    ///     from: Some(0), to: 1, all: 4, none: 4, after: 0.,
    /// };
    /// let error = DecisionGraph::validate(2, 0, &[edge]).unwrap_err();
    /// assert_eq!(error, DecisionGraphError::ConflictingGuards { edge: 0, flags: 4 });
    /// assert!(error.to_string().contains("edge 0"));
    /// ```
    pub fn validate(
        states: u16,
        initial: u16,
        edges: &[DecisionEdge],
    ) -> Result<(), DecisionGraphError> {
        if states == 0 || states > MAX_DECISION_STATES {
            return Err(DecisionGraphError::InvalidStateCount { states });
        }
        if initial >= states {
            return Err(DecisionGraphError::InvalidInitialState { initial, states });
        }
        if edges.len() > MAX_DECISION_EDGES {
            return Err(DecisionGraphError::TooManyEdges { len: edges.len() });
        }
        for (edge, e) in edges.iter().enumerate() {
            if e.to >= states {
                return Err(DecisionGraphError::InvalidDestination {
                    edge,
                    to: e.to,
                    states,
                });
            }
            if let Some(from) = e.from {
                if from >= states {
                    return Err(DecisionGraphError::InvalidSource { edge, from, states });
                }
                if from == e.to {
                    return Err(DecisionGraphError::SelfTransition { edge, state: from });
                }
            }
            if e.all & e.none != 0 {
                return Err(DecisionGraphError::ConflictingGuards {
                    edge,
                    flags: e.all & e.none,
                });
            }
            if !e.after.is_finite() || !(0. ..=3600.).contains(&e.after) {
                return Err(DecisionGraphError::InvalidDelay { edge });
            }
        }
        Ok(())
    }

    /// Construct a graph with the diagnostics from [`Self::validate`].
    /// Copies at most [`MAX_DECISION_EDGES`] edges, only after validation passes.
    ///
    /// ```
    /// use ferravex_play::ai::{DecisionEdge, DecisionGraph, DecisionGraphError};
    /// let graph = DecisionGraph::new_checked(2, 0, &[
    ///     DecisionEdge { from: Some(0), to: 1, all: 1, none: 0, after: 0.5 },
    ///     DecisionEdge { from: Some(1), to: 0, all: 0, none: 1, after: 0. },
    /// ])?; // Cycles are valid.
    /// assert_eq!(graph.initial().state(), 0);
    /// assert_eq!(graph.edges()[1].to, 0);
    /// # Ok::<(), DecisionGraphError>(())
    /// ```
    pub fn new_checked(
        states: u16,
        initial: u16,
        edges: &[DecisionEdge],
    ) -> Result<Self, DecisionGraphError> {
        Self::validate(states, initial, edges)?;
        Ok(Self {
            states,
            initial,
            edges: edges.to_vec(),
        })
    }

    /// Borrow the validated edges in priority order without allocating.
    /// Indices in diagnostics and explanations refer to this slice; see
    /// [`Self::new_checked`] for an example.
    pub fn edges(&self) -> &[DecisionEdge] {
        &self.edges
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

/// An edge's disposition for one proposed update. See [`DecisionState::explain`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecisionEdgeOutcome {
    /// At least one source, sensor or timer check failed.
    Rejected,
    /// The first matching edge changes state and resets elapsed time.
    Transition,
    /// The first matching edge targets the current state, retaining elapsed time.
    Hold,
    /// All checks pass, but the earlier edge at `by` wins, even if it holds.
    Shadowed { by: usize },
}

/// All guard checks for an edge, even when an earlier edge wins or its source
/// does not match. No sensor callbacks run: guards inspect supplied bits only.
/// See [`DecisionExplanation::edges`] and [`DecisionState::explain`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecisionEdgeExplanation {
    /// Index in [`DecisionGraph::edges`].
    pub index: usize,
    /// Whether the explicit source matches, or the source is a wildcard.
    pub source_matches: bool,
    /// Required sensor bits absent from the supplied flags.
    pub missing_flags: u64,
    /// Forbidden sensor bits present in the supplied flags.
    pub forbidden_flags: u64,
    /// Time still needed after the proposed delta, zero when the timer passes.
    pub remaining_seconds: f64,
    /// Whether the edge fails, wins, or loses to an earlier matching edge.
    pub outcome: DecisionEdgeOutcome,
}

impl DecisionEdgeExplanation {
    fn matches(&self) -> bool {
        self.source_matches
            && self.missing_flags == 0
            && self.forbidden_flags == 0
            && self.remaining_seconds == 0.
    }
}

/// A read-only snapshot of one proposed update, borrowing only the graph.
/// Stores constant-size metadata and lazily explains at most
/// [`MAX_DECISION_EDGES`] edges, without allocating. Later updates of the source
/// state do not change this snapshot. See [`DecisionState::explain`] for usage.
#[derive(Clone, Copy, Debug)]
pub struct DecisionExplanation<'a> {
    edges: &'a [DecisionEdge],
    state: u16,
    flags: u64,
    elapsed: f64,
    selected_edge: Option<usize>,
}

impl DecisionExplanation<'_> {
    /// Elapsed time used to evaluate guards, including the proposed delta and
    /// the 3600-second cap, before any transition resets the timer.
    pub fn elapsed(&self) -> f64 {
        self.elapsed
    }

    /// Index of the first matching edge, including a wildcard hold.
    /// `None` means no edge matches; see [`DecisionState::explain`].
    pub fn selected_edge(&self) -> Option<usize> {
        self.selected_edge
    }

    /// The change [`DecisionState::update`] would return for these inputs.
    /// `None` covers both a hold and no match; inspect [`Self::selected_edge`]
    /// to distinguish them. No state or elapsed time is changed.
    pub fn change(&self) -> Option<DecisionChange> {
        let to = self.edges[self.selected_edge?].to;
        (to != self.state).then_some(DecisionChange {
            from: self.state,
            to,
        })
    }

    /// Explain every edge in authored order without allocating. Lower-priority
    /// guards are evaluated against the same pre-transition state and time;
    /// a matching loser is [`DecisionEdgeOutcome::Shadowed`], a failing edge is
    /// [`DecisionEdgeOutcome::Rejected`]. See [`DecisionState::explain`].
    pub fn edges(
        &self,
    ) -> impl ExactSizeIterator<Item = DecisionEdgeExplanation> + DoubleEndedIterator + '_ {
        self.edges.iter().enumerate().map(|(index, edge)| {
            let mut result = edge.inspect(index, self.state, self.flags, self.elapsed);
            if result.matches() {
                result.outcome = match self.selected_edge {
                    Some(by) if by != index => DecisionEdgeOutcome::Shadowed { by },
                    _ if edge.to == self.state => DecisionEdgeOutcome::Hold,
                    _ => DecisionEdgeOutcome::Transition,
                };
            }
            result
        })
    }
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

    /// Preview one update without advancing the state or timer. Shares the
    /// evaluator with [`Self::update`], including first-match priority, wildcard
    /// holds, the elapsed-time cap and discarded transition overshoot. Use
    /// `dt = 0` to inspect current guards. Work is bounded by the graph's edge
    /// count and does not allocate.
    ///
    /// Returns [`AiError::InvalidState`] if the current state is outside this
    /// graph, then [`AiError::InvalidInput`] for a nonfinite delta or one outside
    /// `0..=60` seconds. As with `update`, graphs are interchangeable when the
    /// state index fits; no graph identity is tracked.
    ///
    /// ```
    /// use ferravex_play::ai::{DecisionEdge, DecisionEdgeOutcome, DecisionGraph};
    /// let graph = DecisionGraph::new(2, 0, &[
    ///     DecisionEdge { from: Some(0), to: 1, all: 1, none: 2, after: 0.5 },
    ///     DecisionEdge { from: None, to: 0, all: 0, none: 0, after: 0. },
    /// ]).unwrap();
    /// let mut state = graph.initial();
    /// let waiting = state.explain(&graph, 1, 0.25).unwrap();
    /// assert_eq!(waiting.elapsed(), 0.25);
    /// assert_eq!(waiting.selected_edge(), Some(1)); // Wildcard hold.
    /// assert_eq!(waiting.change(), None);
    /// let edge = waiting.edges().next().unwrap();
    /// assert!(edge.source_matches);
    /// assert_eq!((edge.missing_flags, edge.forbidden_flags), (0, 0));
    /// assert_eq!(edge.remaining_seconds, 0.25);
    /// assert_eq!(edge.outcome, DecisionEdgeOutcome::Rejected);
    /// assert_eq!(state.elapsed(), 0.); // Preview never advances simulation.
    /// let ready = state.explain(&graph, 1, 0.5).unwrap();
    /// assert_eq!(ready.selected_edge(), Some(0));
    /// assert_eq!(ready.edges().next().unwrap().outcome, DecisionEdgeOutcome::Transition);
    /// assert_eq!(ready.edges().nth(1).unwrap().outcome, DecisionEdgeOutcome::Shadowed { by: 0 });
    /// assert_eq!(state.update(&graph, 1, 0.5).unwrap(), ready.change());
    /// assert_eq!(state.elapsed(), 0.); // Transition discards overshoot.
    /// assert_eq!(waiting.edges().nth(1).unwrap().outcome, DecisionEdgeOutcome::Hold);
    /// ```
    pub fn explain<'a>(
        &self,
        graph: &'a DecisionGraph,
        flags: u64,
        dt: f32,
    ) -> Result<DecisionExplanation<'a>, AiError> {
        if self.state >= graph.states {
            return Err(AiError::InvalidState);
        }
        if !dt.is_finite() || !(0. ..=60.).contains(&dt) {
            return Err(AiError::InvalidInput);
        }
        let elapsed = (self.elapsed + f64::from(dt)).min(3600.);
        let selected_edge = graph.edges.iter().enumerate().find_map(|(index, edge)| {
            edge.inspect(index, self.state, flags, elapsed)
                .matches()
                .then_some(index)
        });
        Ok(DecisionExplanation {
            edges: &graph.edges,
            state: self.state,
            flags,
            elapsed,
            selected_edge,
        })
    }

    /// At most one transition per call, including dt=0. Timers stop at 3600s;
    /// transition overshoot is not applied to the destination state.
    pub fn update(
        &mut self,
        graph: &DecisionGraph,
        flags: u64,
        dt: f32,
    ) -> Result<Option<DecisionChange>, AiError> {
        let explanation = self.explain(graph, flags, dt)?;
        let change = explanation.change();
        self.elapsed = explanation.elapsed();
        if let Some(change) = change {
            self.state = change.to;
            self.elapsed = 0.;
        }
        Ok(change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(from: Option<u16>, to: u16) -> DecisionEdge {
        DecisionEdge {
            from,
            to,
            all: 0,
            none: 0,
            after: 0.,
        }
    }

    fn assert_invalid(
        states: u16,
        initial: u16,
        edges: &[DecisionEdge],
        expected: DecisionGraphError,
    ) {
        assert_eq!(
            DecisionGraph::validate(states, initial, edges),
            Err(expected)
        );
        assert_eq!(
            DecisionGraph::new_checked(states, initial, edges).unwrap_err(),
            expected
        );
        assert_eq!(
            DecisionGraph::new(states, initial, edges).unwrap_err(),
            AiError::InvalidConfiguration
        );
        assert!(!expected.to_string().is_empty());
    }

    #[test]
    fn configuration_diagnostics_keep_legacy_bounds_and_error_category() {
        for states in [0, MAX_DECISION_STATES + 1, u16::MAX] {
            assert_invalid(
                states,
                0,
                &[],
                DecisionGraphError::InvalidStateCount { states },
            );
        }
        assert_invalid(
            2,
            2,
            &[],
            DecisionGraphError::InvalidInitialState {
                initial: 2,
                states: 2,
            },
        );
        let edges = [edge(None, 0); MAX_DECISION_EDGES + 1];
        assert_invalid(
            1,
            0,
            &edges,
            DecisionGraphError::TooManyEdges {
                len: MAX_DECISION_EDGES + 1,
            },
        );
        let graph = DecisionGraph::new_checked(
            MAX_DECISION_STATES,
            MAX_DECISION_STATES - 1,
            &edges[..MAX_DECISION_EDGES],
        )
        .unwrap();
        assert_eq!(graph.edges().len(), MAX_DECISION_EDGES);
        assert_eq!(graph.initial().state(), MAX_DECISION_STATES - 1);
        assert!(DecisionGraph::validate(1, 0, &[]).is_ok());
    }

    #[test]
    fn edge_diagnostics_identify_indices_and_all_invalid_edge_kinds() {
        let valid = edge(Some(0), 1);
        for (invalid, error) in [
            (
                edge(Some(2), 2),
                DecisionGraphError::InvalidDestination {
                    edge: 1,
                    to: 2,
                    states: 2,
                },
            ),
            (
                edge(Some(2), 0),
                DecisionGraphError::InvalidSource {
                    edge: 1,
                    from: 2,
                    states: 2,
                },
            ),
            (
                edge(Some(1), 1),
                DecisionGraphError::SelfTransition { edge: 1, state: 1 },
            ),
            (
                DecisionEdge {
                    all: 5 | (1 << 63),
                    none: 4 | (1 << 63),
                    ..valid
                },
                DecisionGraphError::ConflictingGuards {
                    edge: 1,
                    flags: 4 | (1 << 63),
                },
            ),
        ] {
            assert_invalid(2, 0, &[valid, invalid], error);
        }
        for after in [
            -f32::EPSILON,
            3600.25,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            assert_invalid(
                2,
                0,
                &[valid, DecisionEdge { after, ..valid }],
                DecisionGraphError::InvalidDelay { edge: 1 },
            );
        }
        for after in [-0., 0., 3600.] {
            assert!(DecisionGraph::validate(2, 0, &[DecisionEdge { after, ..valid }]).is_ok());
        }
    }

    #[test]
    fn diagnostics_choose_the_first_failure_in_documented_order() {
        let invalid = DecisionEdge {
            all: 1,
            none: 1,
            after: f32::NAN,
            ..edge(Some(0), 0)
        };
        let too_many = [invalid; MAX_DECISION_EDGES + 1];
        assert_eq!(
            DecisionGraph::validate(0, 2, &too_many),
            Err(DecisionGraphError::InvalidStateCount { states: 0 })
        );
        assert_eq!(
            DecisionGraph::validate(2, 2, &too_many),
            Err(DecisionGraphError::InvalidInitialState {
                initial: 2,
                states: 2
            })
        );
        assert_eq!(
            DecisionGraph::validate(2, 0, &too_many),
            Err(DecisionGraphError::TooManyEdges {
                len: MAX_DECISION_EDGES + 1
            })
        );
        assert_eq!(
            DecisionGraph::validate(2, 0, &[invalid, edge(None, 9)]),
            Err(DecisionGraphError::SelfTransition { edge: 0, state: 0 })
        );
        assert_eq!(
            DecisionGraph::validate(2, 0, &[DecisionEdge { to: 1, ..invalid }]),
            Err(DecisionGraphError::ConflictingGuards { edge: 0, flags: 1 })
        );
    }

    #[test]
    fn zero_delta_cycles_follow_one_edge_and_preview_is_read_only() {
        let graph = DecisionGraph::new_checked(
            3,
            0,
            &[edge(Some(0), 1), edge(Some(1), 2), edge(Some(2), 0)],
        )
        .unwrap();
        let mut state = graph.initial();
        for index in [0, 1, 2, 0, 1, 2] {
            let before = state;
            let explanation = state.explain(&graph, 0, 0.).unwrap();
            assert_eq!(state, before);
            assert_eq!(explanation.selected_edge(), Some(index));
            let expected = Some(DecisionChange {
                from: index as u16,
                to: (index as u16 + 1) % 3,
            });
            assert_eq!(explanation.change(), expected);
            assert_eq!(
                explanation.edges().nth(index).unwrap().outcome,
                DecisionEdgeOutcome::Transition
            );
            assert_eq!(state.update(&graph, 0, 0.).unwrap(), expected);
            assert_eq!(explanation.change(), expected); // Snapshot survives the update.
            assert_eq!(state.elapsed(), 0.);
        }
    }

    #[test]
    fn wildcard_hold_blocks_lower_priority_without_resetting_time() {
        let graph = DecisionGraph::new_checked(
            2,
            0,
            &[
                DecisionEdge {
                    all: 1,
                    ..edge(None, 0)
                },
                edge(Some(0), 1),
            ],
        )
        .unwrap();
        let mut state = graph.initial();
        state.update(&graph, 1, 0.25).unwrap();
        let explanation = state.explain(&graph, 1, 0.5).unwrap();
        assert_eq!(explanation.selected_edge(), Some(0));
        assert_eq!(explanation.change(), None);
        assert_eq!(explanation.elapsed(), 0.75);
        assert_eq!(
            explanation.edges().next().unwrap().outcome,
            DecisionEdgeOutcome::Hold
        );
        assert_eq!(
            explanation.edges().nth(1).unwrap().outcome,
            DecisionEdgeOutcome::Shadowed { by: 0 }
        );
        assert_eq!(state.update(&graph, 1, 0.5).unwrap(), None);
        assert_eq!(state.elapsed(), 0.75);
        assert_eq!(
            state.update(&graph, 0, 0.).unwrap(),
            Some(DecisionChange { from: 0, to: 1 })
        );
        assert_eq!(state.elapsed(), 0.);
    }

    #[test]
    fn explanations_report_all_guards_even_after_a_priority_winner() {
        let high = 1 << 63;
        let graph = DecisionGraph::new_checked(
            3,
            0,
            &[
                edge(None, 0),
                DecisionEdge {
                    all: high | 1,
                    none: 2 | 4,
                    after: 0.5,
                    ..edge(Some(1), 2)
                },
                edge(Some(0), 2),
            ],
        )
        .unwrap();
        let state = graph.initial();
        let explanation = state.explain(&graph, 2, 0.25).unwrap();
        assert_eq!(explanation.edges().len(), 3);
        assert_eq!(
            explanation.edges().nth(1).unwrap(),
            DecisionEdgeExplanation {
                index: 1,
                source_matches: false,
                missing_flags: high | 1,
                forbidden_flags: 2,
                remaining_seconds: 0.25,
                outcome: DecisionEdgeOutcome::Rejected,
            }
        );
        assert_eq!(
            explanation.edges().next_back().unwrap().outcome,
            DecisionEdgeOutcome::Shadowed { by: 0 }
        );
        assert_eq!(state.elapsed(), 0.);
    }

    #[test]
    fn unmatched_and_empty_graphs_advance_only_when_updated() {
        let graphs = [
            DecisionGraph::new_checked(1, 0, &[]).unwrap(),
            DecisionGraph::new_checked(
                2,
                0,
                &[DecisionEdge {
                    all: 1,
                    ..edge(Some(0), 1)
                }],
            )
            .unwrap(),
        ];
        for graph in graphs {
            let mut state = graph.initial();
            let explanation = state.explain(&graph, 0, 0.5).unwrap();
            assert_eq!(explanation.selected_edge(), None);
            assert_eq!(explanation.change(), None);
            assert_eq!(explanation.elapsed(), 0.5);
            assert!(
                explanation
                    .edges()
                    .all(|e| e.outcome == DecisionEdgeOutcome::Rejected)
            );
            assert_eq!(state.elapsed(), 0.);
            assert_eq!(state.update(&graph, 0, 0.5).unwrap(), None);
            assert_eq!(state.elapsed(), 0.5);
        }
    }

    #[test]
    fn invalid_preview_and_update_inputs_preserve_state_and_error_order() {
        let graph = DecisionGraph::new_checked(2, 1, &[edge(None, 0)]).unwrap();
        let smaller = DecisionGraph::new_checked(1, 0, &[]).unwrap();
        let mut state = graph.initial();
        let original = state;
        for dt in [
            -f32::EPSILON,
            60.25,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            assert_eq!(
                state.explain(&graph, 0, dt).unwrap_err(),
                AiError::InvalidInput
            );
            assert_eq!(state.update(&graph, 0, dt), Err(AiError::InvalidInput));
            assert_eq!(
                state.explain(&smaller, 0, dt).unwrap_err(),
                AiError::InvalidState
            );
            assert_eq!(state.update(&smaller, 0, dt), Err(AiError::InvalidState));
            assert_eq!(state, original);
        }
        assert_eq!(
            state.explain(&smaller, 0, 0.).unwrap_err(),
            AiError::InvalidState
        );
        state.reset(&smaller);
        assert_eq!(
            state.explain(&smaller, 0, 0.).unwrap().selected_edge(),
            None
        );
    }

    #[test]
    fn cap_and_transition_overshoot_match_the_proposed_elapsed_time() {
        let graph = DecisionGraph::new_checked(
            2,
            0,
            &[DecisionEdge {
                after: 3600.,
                all: 1,
                ..edge(Some(0), 1)
            }],
        )
        .unwrap();
        let mut state = graph.initial();
        for _ in 0..59 {
            state.update(&graph, 0, 60.).unwrap();
        }
        state.update(&graph, 0, 59.75).unwrap();
        assert_eq!(state.elapsed(), 3599.75);
        let waiting = state.explain(&graph, 1, 0.).unwrap();
        assert_eq!(waiting.edges().next().unwrap().remaining_seconds, 0.25);
        assert_eq!(waiting.selected_edge(), None);
        let ready = state.explain(&graph, 1, 0.25).unwrap();
        assert_eq!(ready.selected_edge(), Some(0));
        assert_eq!(ready.elapsed(), 3600.);
        state.update(&graph, 0, 60.).unwrap();
        state.update(&graph, 0, 60.).unwrap();
        assert_eq!(state.elapsed(), 3600.);
        assert_eq!(state.update(&graph, 1, 60.).unwrap(), ready.change());
        assert_eq!(state.elapsed(), 0.);

        let short = DecisionGraph::new_checked(
            2,
            0,
            &[DecisionEdge {
                after: 0.5,
                ..edge(Some(0), 1)
            }],
        )
        .unwrap();
        state.reset(&short);
        let preview = state.explain(&short, 0, 60.).unwrap();
        assert_eq!(preview.elapsed(), 60.);
        assert_eq!(state.update(&short, 0, 60.).unwrap(), preview.change());
        assert_eq!(state.elapsed(), 0.);
    }

    #[test]
    fn previews_match_legacy_selection_across_sensor_and_time_combinations() {
        let high = 1 << 63;
        let graph = DecisionGraph::new_checked(
            3,
            0,
            &[
                DecisionEdge {
                    all: high | 1,
                    none: 2,
                    after: 0.5,
                    ..edge(None, 2)
                },
                DecisionEdge {
                    all: 2,
                    none: 1,
                    after: 0.25,
                    ..edge(Some(0), 1)
                },
                DecisionEdge {
                    none: 2,
                    after: 0.5,
                    ..edge(Some(1), 0)
                },
                DecisionEdge {
                    none: high,
                    ..edge(Some(2), 0)
                },
                DecisionEdge {
                    after: 1.,
                    ..edge(None, 1)
                },
            ],
        )
        .unwrap();
        for source in 0..3 {
            for bits in 0..16 {
                let flags = (bits & 7) | if bits & 8 != 0 { high } else { 0 };
                for elapsed in [0., 0.25, 0.5, 1., 3599.75, 3600.] {
                    for dt in [0., 0.25, 60.] {
                        let mut state = DecisionState {
                            state: source,
                            elapsed,
                        };
                        let proposed = (elapsed + f64::from(dt)).min(3600.);
                        // Original decision predicate, independent of diagnostic helpers.
                        let selected = graph.edges().iter().position(|e| {
                            e.from.is_none_or(|s| s == source)
                                && flags & e.all == e.all
                                && flags & e.none == 0
                                && proposed >= f64::from(e.after)
                        });
                        let change = selected.and_then(|index| {
                            let to = graph.edges()[index].to;
                            (source != to).then_some(DecisionChange { from: source, to })
                        });
                        let explanation = state.explain(&graph, flags, dt).unwrap();
                        assert_eq!(explanation.selected_edge(), selected);
                        assert_eq!(explanation.change(), change);
                        assert_eq!(explanation.elapsed(), proposed);
                        let repeated = state.explain(&graph, flags, dt).unwrap();
                        assert!(explanation.edges().eq(repeated.edges()));
                        assert_eq!(
                            state,
                            DecisionState {
                                state: source,
                                elapsed
                            }
                        );
                        assert_eq!(state.update(&graph, flags, dt).unwrap(), change);
                        assert_eq!(state.state(), change.map_or(source, |c| c.to));
                        assert_eq!(
                            state.elapsed(),
                            if change.is_some() { 0. } else { proposed }
                        );
                    }
                }
            }
        }
    }
}
