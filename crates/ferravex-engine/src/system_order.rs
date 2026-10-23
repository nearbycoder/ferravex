//! Stable topological ordering for sequential engine stages. No systems execute here.
//! Up to 256 named systems. Among systems whose prerequisites are satisfied,
//! the earliest registered system always runs first, including newly ready systems.
//!
//! Use [`SystemOrder::before_checked`](crate::system_order::SystemOrder::before_checked) for transactional dependency edits and
//! [`SystemOrder::inspect`](crate::system_order::SystemOrder::inspect) to explain the resulting order. Legacy [`SystemOrder::before`](crate::system_order::SystemOrder::before)
//! still permits cycles until resolution.
//! [`SystemOrder::resolve_layers`] groups systems by dependency depth, while
//! [`SystemOrder::inspect_layers`] also explains each system's direct prerequisites.
//! Layers describe ordering constraints only; they neither execute systems nor
//! establish whether systems may safely run concurrently.
//!
//! ```
//! use ferravex_engine::system_order::{OrderDiagnostic, SystemOrder};
//!
//! let mut order = SystemOrder::default();
//! for name in ["render", "input", "physics", "audio"] {
//!     order.add_checked(name)?;
//! }
//! order.before_checked("input", "physics")?;
//! order.before_checked("physics", "render")?;
//! assert_eq!(order.resolve_checked()?, ["input", "physics", "render", "audio"]);
//! let stages = order.inspect()?;
//! assert_eq!(stages[1].name, "physics");
//! assert_eq!(stages[1].prerequisites, ["input"]);
//! # Ok::<(), OrderDiagnostic>(())
//! ```
use std::{collections::BTreeSet, fmt};

const MAX_SYSTEMS: usize = 256;
const MAX_NAME_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderError {
    InvalidName,
    Duplicate,
    Capacity,
    UnknownSystem,
    Cycle,
}

/// An owned, named ordering failure with an actionable [`fmt::Display`] message.
/// Obtained from the checked methods on [`SystemOrder`]; [`Self::kind`] maps back
/// to the original error codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderDiagnostic {
    /// Names must contain 1 through 128 UTF-8 bytes.
    InvalidName { name: String },
    /// The exact, case-sensitive name is already registered.
    Duplicate { name: String },
    /// Registering this name would exceed the limit of 256 systems.
    Capacity { name: String },
    /// The requested `first -> second` dependency has unregistered endpoints.
    UnknownSystems {
        first: String,
        second: String,
        /// Missing names in argument order, with duplicates removed.
        missing: Vec<String>,
    },
    /// A closed directed cycle: every adjacent pair is a declared dependency,
    /// and the first name is repeated at the end. Only actual cycle members
    /// are included, not systems merely blocked by the cycle.
    Cycle { path: Vec<String> },
}

impl OrderDiagnostic {
    /// Return the corresponding legacy error code without losing this diagnostic.
    ///
    /// ```
    /// use ferravex_engine::system_order::{OrderError, SystemOrder};
    /// let error = SystemOrder::default().before_checked("input", "render").unwrap_err();
    /// assert_eq!(error.kind(), OrderError::UnknownSystem);
    /// ```
    pub fn kind(&self) -> OrderError {
        match self {
            Self::InvalidName { .. } => OrderError::InvalidName,
            Self::Duplicate { .. } => OrderError::Duplicate,
            Self::Capacity { .. } => OrderError::Capacity,
            Self::UnknownSystems { .. } => OrderError::UnknownSystem,
            Self::Cycle { .. } => OrderError::Cycle,
        }
    }
}

impl fmt::Display for OrderDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName { name } => write!(
                f,
                "invalid system name {name:?}: use 1..={MAX_NAME_BYTES} UTF-8 bytes (got {})",
                name.len()
            ),
            Self::Duplicate { name } => write!(
                f,
                "system {name:?} is already registered; reuse it or choose a unique name"
            ),
            Self::Capacity { name } => write!(
                f,
                "cannot register system {name:?}: the {MAX_SYSTEMS}-system limit is reached"
            ),
            Self::UnknownSystems {
                first,
                second,
                missing,
            } => {
                write!(f, "cannot declare {first:?} before {second:?}: register ")?;
                for (index, name) in missing.iter().enumerate() {
                    if index > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{name:?}")?;
                }
                write!(
                    f,
                    " with add or add_checked before declaring the dependency"
                )
            }
            Self::Cycle { path } => {
                write!(f, "system dependency cycle: ")?;
                for (index, name) in path.iter().enumerate() {
                    if index > 0 {
                        write!(f, " -> ")?;
                    }
                    write!(f, "{name:?}")?;
                }
                write!(f, "; remove or reverse one of these before constraints")
            }
        }
    }
}

impl std::error::Error for OrderDiagnostic {}

/// A system in execution order, as returned by [`SystemOrder::inspect`] or
/// [`SystemOrder::inspect_layers`].
/// Names borrow from the ordering graph; inspection does not execute any system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderedSystem<'a> {
    /// Registered system name.
    pub name: &'a str,
    /// Direct prerequisites in registration order, not execution order.
    /// Transitive prerequisites are not repeated here.
    pub prerequisites: Vec<&'a str>,
}

#[derive(Default)]
pub struct SystemOrder {
    names: Vec<String>,
    edges: BTreeSet<(usize, usize)>,
}
impl SystemOrder {
    pub fn add(&mut self, name: &str) -> Result<(), OrderError> {
        if name.is_empty() || name.len() > MAX_NAME_BYTES {
            return Err(OrderError::InvalidName);
        }
        if self.names.iter().any(|n| n == name) {
            return Err(OrderError::Duplicate);
        }
        if self.names.len() == MAX_SYSTEMS {
            return Err(OrderError::Capacity);
        }
        self.names.push(name.into());
        Ok(())
    }

    /// Register a system, returning its name and a remedy on failure.
    /// Validation and failure atomicity are identical to [`Self::add`].
    ///
    /// ```
    /// use ferravex_engine::system_order::{OrderDiagnostic, SystemOrder};
    /// let mut order = SystemOrder::default();
    /// order.add_checked("input")?;
    /// assert_eq!(order.add_checked("input"), Err(OrderDiagnostic::Duplicate {
    ///     name: "input".into(),
    /// }));
    /// # Ok::<(), OrderDiagnostic>(())
    /// ```
    pub fn add_checked(&mut self, name: &str) -> Result<(), OrderDiagnostic> {
        self.add(name).map_err(|error| match error {
            OrderError::InvalidName => OrderDiagnostic::InvalidName { name: name.into() },
            OrderError::Duplicate => OrderDiagnostic::Duplicate { name: name.into() },
            OrderError::Capacity => OrderDiagnostic::Capacity { name: name.into() },
            _ => unreachable!("registration only fails for invalid, duplicate or excess names"),
        })
    }

    /// Declare `first` before `second`. Both names must already be registered.
    /// Cycles are accepted here and reported by [`Self::resolve`]. To reject them
    /// without changing the graph, use [`Self::before_checked`].
    pub fn before(&mut self, first: &str, second: &str) -> Result<(), OrderError> {
        let find = |name: &str| {
            self.names
                .iter()
                .position(|n| n == name)
                .ok_or(OrderError::UnknownSystem)
        };
        let a = find(first)?;
        let b = find(second)?;
        self.edges.insert((a, b));
        Ok(())
    }

    /// Declare `first` before `second` only if the resulting graph is acyclic.
    /// Reports every missing endpoint, or a named cycle path. On any error the
    /// graph is unchanged, including existing copies of the requested edge.
    /// Repeating an edge succeeds if the graph is acyclic. A pre-existing cycle
    /// created with [`Self::before`] is also an error, even for a repeated edge.
    ///
    /// ```
    /// use ferravex_engine::system_order::{OrderDiagnostic, SystemOrder};
    /// let mut order = SystemOrder::default();
    /// order.add_checked("input")?;
    /// order.add_checked("render")?;
    /// order.before_checked("input", "render")?;
    /// let error = order.before_checked("render", "input").unwrap_err();
    /// assert_eq!(error, OrderDiagnostic::Cycle {
    ///     path: vec!["input".into(), "render".into(), "input".into()],
    /// });
    /// assert_eq!(order.resolve_checked()?, ["input", "render"]);
    /// # Ok::<(), OrderDiagnostic>(())
    /// ```
    pub fn before_checked(&mut self, first: &str, second: &str) -> Result<(), OrderDiagnostic> {
        let a = self.names.iter().position(|name| name == first);
        let b = self.names.iter().position(|name| name == second);
        let (Some(a), Some(b)) = (a, b) else {
            let mut missing = Vec::new();
            if a.is_none() {
                missing.push(first.into());
            }
            if b.is_none() && (a.is_some() || first != second) {
                missing.push(second.into());
            }
            return Err(OrderDiagnostic::UnknownSystems {
                first: first.into(),
                second: second.into(),
                missing,
            });
        };
        let inserted = self.edges.insert((a, b));
        if let Err(error) = self.resolve_checked() {
            if inserted {
                self.edges.remove(&(a, b));
            }
            return Err(error);
        }
        Ok(())
    }

    /// Resolve execution order, breaking ties by earliest registration among all
    /// currently ready systems. Disconnected systems participate in the same rule.
    pub fn resolve(&self) -> Result<Vec<&str>, OrderError> {
        let mut degree = vec![0; self.names.len()];
        for &(_, b) in &self.edges {
            degree[b] += 1;
        }
        let mut ready: BTreeSet<_> = degree
            .iter()
            .enumerate()
            .filter_map(|(i, &n)| (n == 0).then_some(i))
            .collect();
        let mut out = Vec::new();
        while let Some(a) = ready.pop_first() {
            out.push(self.names[a].as_str());
            for &(_, b) in self.edges.range((a, 0)..=(a, usize::MAX)) {
                degree[b] -= 1;
                if degree[b] == 0 {
                    ready.insert(b);
                }
            }
        }
        if out.len() == self.names.len() {
            Ok(out)
        } else {
            Err(OrderError::Cycle)
        }
    }

    /// Resolve in the same order as [`Self::resolve`], with a named cycle on error.
    /// Cycle discovery visits systems and outgoing dependencies in registration
    /// order, so the chosen path is independent of edge insertion order. When
    /// several cycles exist, the first found cycle is reported, not necessarily
    /// the shortest one. The graph is never changed.
    ///
    /// ```
    /// use ferravex_engine::system_order::{OrderDiagnostic, SystemOrder};
    /// let mut order = SystemOrder::default();
    /// order.add_checked("audio")?;
    /// order.before("audio", "audio").unwrap();
    /// assert_eq!(order.resolve_checked(), Err(OrderDiagnostic::Cycle {
    ///     path: vec!["audio".into(), "audio".into()],
    /// }));
    /// # Ok::<(), OrderDiagnostic>(())
    /// ```
    pub fn resolve_checked(&self) -> Result<Vec<&str>, OrderDiagnostic> {
        self.resolve().map_err(|_| OrderDiagnostic::Cycle {
            path: self.cycle_path(),
        })
    }

    /// Resolve dependency layers, ordered from prerequisites to dependents.
    /// Layer zero contains every system without prerequisites. Each other system
    /// appears exactly once, one layer after its deepest direct prerequisite.
    /// Systems within a layer use registration order, independent of the order
    /// in which dependencies were declared. An empty graph returns no layers.
    ///
    /// Flattening these layers produces a valid topological order, but may differ
    /// from [`Self::resolve`], which immediately considers newly ready systems.
    /// No systems execute here and shared resource access is not analyzed.
    ///
    /// A cycle anywhere in the graph returns the same named diagnostic as
    /// [`Self::resolve_checked`], without a partial result or graph mutation.
    /// Missing endpoints are rejected when declaring dependencies with
    /// [`Self::before_checked`], so the stored graph cannot contain missing nodes.
    /// Allocates layer and working storage bounded by the 256 registered systems;
    /// returned names borrow from the graph.
    ///
    /// ```
    /// use ferravex_engine::system_order::{OrderDiagnostic, SystemOrder};
    /// let mut order = SystemOrder::default();
    /// for name in ["render", "input", "physics", "audio"] {
    ///     order.add_checked(name)?;
    /// }
    /// order.before_checked("input", "physics")?;
    /// order.before_checked("physics", "render")?;
    /// assert_eq!(order.resolve_layers()?, vec![
    ///     vec!["input", "audio"], vec!["physics"], vec!["render"],
    /// ]);
    /// assert_eq!(order.resolve_checked()?, ["input", "physics", "render", "audio"]);
    /// # Ok::<(), OrderDiagnostic>(())
    /// ```
    pub fn resolve_layers(&self) -> Result<Vec<Vec<&str>>, OrderDiagnostic> {
        Ok(self
            .layer_indices()?
            .into_iter()
            .map(|layer| {
                layer
                    .into_iter()
                    .map(|index| self.names[index].as_str())
                    .collect()
            })
            .collect())
    }

    /// Inspect dependency layers and each system's direct prerequisites.
    /// Layers and their systems have exactly [`Self::resolve_layers`]'s ordering
    /// and cycle diagnostics. Each prerequisite list uses registration order,
    /// just as in [`Self::inspect`]; transitive dependencies are not repeated.
    /// No graph state is changed or cached, including on failure.
    /// Allocates storage bounded by the registered systems and declared edges,
    /// and borrows all names from the graph.
    ///
    /// ```
    /// use ferravex_engine::system_order::{OrderDiagnostic, OrderedSystem, SystemOrder};
    /// let mut order = SystemOrder::default();
    /// for name in ["draw", "update", "input"] {
    ///     order.add_checked(name)?;
    /// }
    /// order.before_checked("input", "update")?;
    /// order.before_checked("update", "draw")?;
    /// let layers = order.inspect_layers()?;
    /// assert_eq!(layers[2], vec![OrderedSystem {
    ///     name: "draw", prerequisites: vec!["update"],
    /// }]);
    /// assert_eq!(layers[1][0].prerequisites, ["input"]);
    /// # Ok::<(), OrderDiagnostic>(())
    /// ```
    pub fn inspect_layers(&self) -> Result<Vec<Vec<OrderedSystem<'_>>>, OrderDiagnostic> {
        let layers = self.layer_indices()?;
        let mut prerequisites = self.direct_prerequisites();
        Ok(layers
            .into_iter()
            .map(|layer| {
                layer
                    .into_iter()
                    .map(|index| OrderedSystem {
                        name: self.names[index].as_str(),
                        prerequisites: std::mem::take(&mut prerequisites[index]),
                    })
                    .collect()
            })
            .collect())
    }

    /// Inspect the resolved execution order and each system's direct prerequisites.
    /// The returned list uses [`Self::resolve`]'s tie-break rule. Each prerequisite
    /// list uses registration order, irrespective of edge insertion order.
    /// Allocates the inspection result and borrows the registered names.
    ///
    /// ```
    /// use ferravex_engine::system_order::{OrderDiagnostic, OrderedSystem, SystemOrder};
    /// let mut order = SystemOrder::default();
    /// order.add_checked("draw")?;
    /// order.add_checked("update")?;
    /// order.before_checked("update", "draw")?;
    /// assert_eq!(order.inspect()?, vec![
    ///     OrderedSystem { name: "update", prerequisites: vec![] },
    ///     OrderedSystem { name: "draw", prerequisites: vec!["update"] },
    /// ]);
    /// # Ok::<(), OrderDiagnostic>(())
    /// ```
    pub fn inspect(&self) -> Result<Vec<OrderedSystem<'_>>, OrderDiagnostic> {
        let order = self.resolve_checked()?;
        let mut prerequisites = self.direct_prerequisites();
        Ok(order
            .into_iter()
            .map(|name| {
                let index = self.names.iter().position(|n| n == name).unwrap();
                OrderedSystem {
                    name,
                    prerequisites: std::mem::take(&mut prerequisites[index]),
                }
            })
            .collect())
    }

    fn direct_prerequisites(&self) -> Vec<Vec<&str>> {
        let mut prerequisites = vec![Vec::new(); self.names.len()];
        for &(a, b) in &self.edges {
            prerequisites[b].push(self.names[a].as_str());
        }
        prerequisites
    }

    fn layer_indices(&self) -> Result<Vec<Vec<usize>>, OrderDiagnostic> {
        let mut degree = vec![0; self.names.len()];
        for &(_, b) in &self.edges {
            degree[b] += 1;
        }
        let mut ready: Vec<_> = degree
            .iter()
            .enumerate()
            .filter_map(|(index, &count)| (count == 0).then_some(index))
            .collect();
        let mut layers = Vec::new();
        let mut resolved = 0;
        while !ready.is_empty() {
            let mut next = Vec::new();
            for &a in &ready {
                for &(_, b) in self.edges.range((a, 0)..=(a, usize::MAX)) {
                    degree[b] -= 1;
                    if degree[b] == 0 {
                        next.push(b);
                    }
                }
            }
            // Unlock order can differ from registration order. Newly ready
            // systems always wait until the entire current layer is complete.
            next.sort_unstable();
            resolved += ready.len();
            layers.push(ready);
            ready = next;
        }
        if resolved == self.names.len() {
            Ok(layers)
        } else {
            Err(OrderDiagnostic::Cycle {
                path: self.cycle_path(),
            })
        }
    }

    fn cycle_path(&self) -> Vec<String> {
        fn visit(
            order: &SystemOrder,
            node: usize,
            state: &mut [u8],
            stack: &mut Vec<usize>,
        ) -> Option<Vec<String>> {
            state[node] = 1;
            stack.push(node);
            for &(_, next) in order.edges.range((node, 0)..=(node, usize::MAX)) {
                if state[next] == 1 {
                    let start = stack.iter().position(|&index| index == next).unwrap();
                    return Some(
                        stack[start..]
                            .iter()
                            .copied()
                            .chain(std::iter::once(next))
                            .map(|index| order.names[index].clone())
                            .collect(),
                    );
                }
                if state[next] == 0
                    && let Some(path) = visit(order, next, state, stack)
                {
                    return Some(path);
                }
            }
            stack.pop();
            state[node] = 2;
            None
        }

        // Recursion is bounded by the 256-system registration limit.
        let mut state = vec![0; self.names.len()];
        let mut stack = Vec::new();
        for node in 0..self.names.len() {
            if state[node] == 0
                && let Some(path) = visit(self, node, &mut state, &mut stack)
            {
                return path;
            }
        }
        unreachable!("failed topological resolution guarantees a directed cycle")
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn registered(names: &[&str]) -> SystemOrder {
        let mut order = SystemOrder::default();
        for name in names {
            order.add_checked(name).unwrap();
        }
        order
    }

    #[test]
    fn stable_dependencies_duplicates_and_cycles() {
        let mut s = SystemOrder::default();
        for n in ["render", "input", "physics", "audio"] {
            s.add(n).unwrap();
        }
        s.before("input", "physics").unwrap();
        s.before("physics", "render").unwrap();
        s.before("physics", "render").unwrap();
        assert_eq!(
            s.resolve().unwrap(),
            ["input", "physics", "render", "audio"]
        );
        assert_eq!(s.add("input"), Err(OrderError::Duplicate));
        assert_eq!(s.before("missing", "input"), Err(OrderError::UnknownSystem));
        s.before("render", "input").unwrap();
        assert_eq!(s.resolve(), Err(OrderError::Cycle));
        let mut self_cycle = SystemOrder::default();
        self_cycle.add("x").unwrap();
        self_cycle.before("x", "x").unwrap();
        assert!(self_cycle.resolve().is_err());
    }

    #[test]
    fn cycle_path_excludes_ancestors_and_blocked_dependents() {
        let mut order = registered(&["startup", "tail", "input", "physics", "render", "audio"]);
        for (a, b) in [
            ("startup", "input"),
            ("input", "physics"),
            ("physics", "render"),
            ("render", "input"),
            ("render", "tail"),
        ] {
            order.before(a, b).unwrap();
        }
        let error = order.resolve_checked().unwrap_err();
        assert_eq!(
            error,
            OrderDiagnostic::Cycle {
                path: ["input", "physics", "render", "input"]
                    .map(String::from)
                    .into(),
            }
        );
        assert_eq!(error.kind(), OrderError::Cycle);
        assert_eq!(order.inspect(), Err(error.clone()));
        assert_eq!(
            error.to_string(),
            "system dependency cycle: \"input\" -> \"physics\" -> \"render\" -> \"input\"; remove or reverse one of these before constraints"
        );
        let OrderDiagnostic::Cycle { path } = error else {
            unreachable!()
        };
        for pair in path.windows(2) {
            let a = order
                .names
                .iter()
                .position(|name| name == &pair[0])
                .unwrap();
            let b = order
                .names
                .iter()
                .position(|name| name == &pair[1])
                .unwrap();
            assert!(order.edges.contains(&(a, b)));
        }
    }

    #[test]
    fn cycle_selection_uses_registration_not_edge_insertion_or_alphabetic_order() {
        let edges = [
            ("zeta", "alpha"),
            ("alpha", "zeta"),
            ("zeta", "beta"),
            ("beta", "zeta"),
        ];
        let mut forward = registered(&["zeta", "beta", "alpha"]);
        let mut reverse = registered(&["zeta", "beta", "alpha"]);
        for &(a, b) in &edges {
            forward.before(a, b).unwrap();
        }
        for &(a, b) in edges.iter().rev() {
            reverse.before(a, b).unwrap();
        }
        let expected = Err(OrderDiagnostic::Cycle {
            path: ["zeta", "beta", "zeta"].map(String::from).into(),
        });
        assert_eq!(forward.resolve_checked(), expected);
        assert_eq!(reverse.resolve_checked(), expected);
    }

    #[test]
    fn checked_edges_roll_back_cycles_and_allow_subsequent_valid_edits() {
        let mut order = registered(&["render", "input", "physics", "audio"]);
        order.before_checked("input", "physics").unwrap();
        order.before_checked("physics", "render").unwrap();
        let original_edges = order.edges.clone();
        let original_order = order.resolve().unwrap().join(",");
        assert_eq!(
            order.before_checked("render", "input"),
            Err(OrderDiagnostic::Cycle {
                path: ["render", "input", "physics", "render"]
                    .map(String::from)
                    .into(),
            })
        );
        assert_eq!(order.edges, original_edges);
        assert_eq!(order.resolve().unwrap().join(","), original_order);
        // Repeating a dependency does not add an inspection entry or affect degree.
        order.before_checked("physics", "render").unwrap();
        assert_eq!(order.edges, original_edges);
        assert_eq!(order.inspect().unwrap()[2].prerequisites, ["physics"]);
        order.before_checked("audio", "input").unwrap();
        assert_eq!(
            order.resolve_checked().unwrap(),
            ["audio", "input", "physics", "render"]
        );
    }

    #[test]
    fn self_cycles_are_named_and_checked_insertions_leave_no_edge() {
        let mut order = registered(&["physics"]);
        let expected = OrderDiagnostic::Cycle {
            path: ["physics", "physics"].map(String::from).into(),
        };
        assert_eq!(
            order.before_checked("physics", "physics"),
            Err(expected.clone())
        );
        assert!(order.edges.is_empty());
        assert_eq!(order.resolve_checked().unwrap(), ["physics"]);
        order.before("physics", "physics").unwrap();
        assert_eq!(order.resolve_checked(), Err(expected));
    }

    #[test]
    fn failed_checked_edits_preserve_preexisting_cycles_and_duplicate_edges() {
        let mut order = registered(&["input", "render", "audio"]);
        order.before("input", "render").unwrap();
        order.before("render", "input").unwrap();
        let original = order.edges.clone();
        let diagnostic = order.resolve_checked().unwrap_err();
        // Rollback must not remove an edge that was present before this call.
        assert_eq!(
            order.before_checked("input", "render"),
            Err(diagnostic.clone())
        );
        assert_eq!(order.edges, original);
        // An otherwise valid new edge is also rolled back on a cyclic graph.
        assert_eq!(order.before_checked("input", "audio"), Err(diagnostic));
        assert_eq!(order.edges, original);
        assert_eq!(order.resolve(), Err(OrderError::Cycle));
    }

    #[test]
    fn missing_endpoints_report_both_names_without_changing_the_graph() {
        let mut order = registered(&["input", "render"]);
        order.before_checked("input", "render").unwrap();
        let original = order.edges.clone();
        for (first, second, missing) in [
            ("missing", "input", vec!["missing"]),
            ("input", "missing", vec!["missing"]),
            ("missing", "other", vec!["missing", "other"]),
            ("missing", "missing", vec!["missing"]),
        ] {
            let error = order.before_checked(first, second).unwrap_err();
            assert_eq!(error.kind(), OrderError::UnknownSystem);
            assert_eq!(
                error,
                OrderDiagnostic::UnknownSystems {
                    first: first.into(),
                    second: second.into(),
                    missing: missing.into_iter().map(String::from).collect(),
                }
            );
            assert!(error.to_string().contains("register"));
            assert!(
                error
                    .to_string()
                    .contains(&format!("{first:?} before {second:?}"))
            );
            assert_eq!(order.edges, original);
            assert_eq!(order.resolve_checked().unwrap(), ["input", "render"]);
        }
    }

    #[test]
    fn inspection_stably_orders_ready_nodes_and_direct_prerequisites() {
        let names = ["render", "physics", "input", "audio", "network", "log"];
        let edges = [
            ("input", "physics"),
            ("physics", "render"),
            ("input", "render"),
            ("network", "log"),
            ("audio", "log"),
        ];
        let mut forward = registered(&names);
        let mut reverse = registered(&names);
        for &(a, b) in &edges {
            forward.before_checked(a, b).unwrap();
        }
        for &(a, b) in edges.iter().rev() {
            reverse.before_checked(a, b).unwrap();
        }
        let inspection = forward.inspect().unwrap();
        assert_eq!(inspection, reverse.inspect().unwrap());
        assert_eq!(
            inspection
                .iter()
                .map(|entry| entry.name)
                .collect::<Vec<_>>(),
            ["input", "physics", "render", "audio", "network", "log"]
        );
        assert_eq!(
            forward.resolve_checked().unwrap(),
            forward.resolve().unwrap()
        );
        assert!(inspection[0].prerequisites.is_empty());
        assert_eq!(inspection[1].prerequisites, ["input"]);
        // Prerequisites are in registration order, which differs from execution order.
        assert_eq!(inspection[2].prerequisites, ["physics", "input"]);
        assert!(inspection[3].prerequisites.is_empty());
        assert!(inspection[4].prerequisites.is_empty());
        assert_eq!(inspection[5].prerequisites, ["audio", "network"]);
        assert_eq!(forward.inspect().unwrap(), inspection);
    }

    #[test]
    fn empty_and_disconnected_systems_inspect_in_registration_order() {
        let empty = SystemOrder::default();
        assert!(empty.resolve_checked().unwrap().is_empty());
        assert!(empty.inspect().unwrap().is_empty());
        let order = registered(&["zeta", "alpha", "audio"]);
        assert_eq!(order.resolve_checked().unwrap(), ["zeta", "alpha", "audio"]);
        assert!(
            order
                .inspect()
                .unwrap()
                .iter()
                .all(|entry| entry.prerequisites.is_empty())
        );
    }

    #[test]
    fn named_registration_errors_keep_validation_and_capacity_atomic() {
        let mut order = SystemOrder::default();
        for name in [String::new(), "é".repeat(65)] {
            let error = order.add_checked(&name).unwrap_err();
            assert_eq!(error.kind(), OrderError::InvalidName);
            assert_eq!(error, OrderDiagnostic::InvalidName { name });
            assert!(error.to_string().contains("UTF-8 bytes"));
            assert!(order.names.is_empty());
        }
        let unicode = "é".repeat(64);
        order.add_checked(&unicode).unwrap();
        let error = order.add_checked(&unicode).unwrap_err();
        assert_eq!(error.kind(), OrderError::Duplicate);
        assert_eq!(
            error,
            OrderDiagnostic::Duplicate {
                name: unicode.clone()
            }
        );
        assert!(error.to_string().contains("unique name"));
        for index in 1..MAX_SYSTEMS {
            order.add_checked(&format!("system-{index}")).unwrap();
        }
        let error = order.add_checked("overflow").unwrap_err();
        assert_eq!(error.kind(), OrderError::Capacity);
        assert_eq!(
            error,
            OrderDiagnostic::Capacity {
                name: "overflow".into()
            }
        );
        assert!(error.to_string().contains("256-system limit"));
        assert_eq!(order.names.len(), MAX_SYSTEMS);
        assert_eq!(order.names[0], unicode);
        assert_eq!(order.resolve_checked().unwrap().len(), MAX_SYSTEMS);
        assert_eq!(
            order.add_checked("").unwrap_err().kind(),
            OrderError::InvalidName
        );
        assert_eq!(
            order.add_checked(&unicode).unwrap_err().kind(),
            OrderError::Duplicate
        );
    }

    #[test]
    fn maximum_length_cycle_is_closed_and_transactional() {
        let mut order = SystemOrder::default();
        let names: Vec<_> = (0..MAX_SYSTEMS)
            .map(|index| format!("system-{index}"))
            .collect();
        for name in &names {
            order.add_checked(name).unwrap();
        }
        for pair in names.windows(2) {
            order.before_checked(&pair[0], &pair[1]).unwrap();
        }
        let original = order.edges.clone();
        let error = order
            .before_checked(names.last().unwrap(), &names[0])
            .unwrap_err();
        let mut expected = names.clone();
        expected.push(names[0].clone());
        assert_eq!(error, OrderDiagnostic::Cycle { path: expected });
        assert_eq!(order.edges, original);
        assert_eq!(order.resolve_checked().unwrap(), names);
    }

    #[test]
    fn layers_group_whole_frontiers_and_preserve_sequential_resolution() {
        let mut order = registered(&["render", "physics", "ai", "input", "audio", "idle", "log"]);
        for (a, b) in [
            ("input", "physics"),
            ("input", "ai"),
            ("physics", "render"),
            ("ai", "render"),
            ("input", "render"), // A shortcut must not lower render's layer.
            ("audio", "log"),
            ("audio", "log"), // Duplicate edges must not change the degree.
        ] {
            order.before(a, b).unwrap();
        }
        let expected = vec![
            vec!["input", "audio", "idle"],
            vec!["physics", "ai", "log"],
            vec!["render"],
        ];
        assert_eq!(order.resolve_layers().unwrap(), expected);
        let inspection = order.inspect_layers().unwrap();
        assert_eq!(
            inspection
                .iter()
                .map(|layer| layer.iter().map(|system| system.name).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            expected
        );
        assert!(
            inspection[0]
                .iter()
                .all(|system| system.prerequisites.is_empty())
        );
        assert_eq!(inspection[1][0].prerequisites, ["input"]);
        assert_eq!(inspection[1][1].prerequisites, ["input"]);
        assert_eq!(inspection[1][2].prerequisites, ["audio"]);
        assert_eq!(inspection[2][0].prerequisites, ["physics", "ai", "input"]);
        let sequential = order.inspect().unwrap();
        for system in inspection.iter().flatten() {
            assert_eq!(
                sequential.iter().find(|entry| entry.name == system.name),
                Some(system)
            );
        }
        assert_eq!(
            order.resolve_checked().unwrap(),
            ["input", "physics", "ai", "render", "audio", "idle", "log"]
        );
    }

    #[test]
    fn layers_sort_by_registration_not_unlock_or_edge_insertion_order() {
        let names = ["join", "zeta", "alpha", "root-b", "root-a"];
        let edges = [
            ("root-b", "alpha"),
            ("root-a", "zeta"),
            ("zeta", "join"),
            ("alpha", "join"),
        ];
        let mut forward = registered(&names);
        let mut reverse = registered(&names);
        for &(a, b) in &edges {
            forward.before(a, b).unwrap();
        }
        for &(a, b) in edges.iter().rev() {
            reverse.before(a, b).unwrap();
        }
        let expected = vec![
            vec!["root-b", "root-a"],
            vec!["zeta", "alpha"],
            vec!["join"],
        ];
        assert_eq!(forward.resolve_layers().unwrap(), expected);
        assert_eq!(reverse.resolve_layers().unwrap(), expected);
        assert_eq!(
            forward.inspect_layers().unwrap(),
            reverse.inspect_layers().unwrap()
        );
    }

    #[test]
    fn layers_handle_empty_disconnected_and_changed_graphs() {
        let empty = SystemOrder::default();
        assert!(empty.resolve_layers().unwrap().is_empty());
        assert!(empty.inspect_layers().unwrap().is_empty());
        let mut order = registered(&["zeta", "alpha", "音声"]);
        assert_eq!(
            order.resolve_layers().unwrap(),
            vec![vec!["zeta", "alpha", "音声"]]
        );
        assert_eq!(
            order.inspect_layers().unwrap(),
            vec![order.inspect().unwrap()]
        );
        order.before_checked("alpha", "zeta").unwrap();
        assert_eq!(
            order.resolve_layers().unwrap(),
            vec![vec!["alpha", "音声"], vec!["zeta"]]
        );
        order.add_checked("late-root").unwrap();
        order.before_checked("late-root", "alpha").unwrap();
        assert_eq!(
            order.resolve_layers().unwrap(),
            vec![vec!["音声", "late-root"], vec!["alpha"], vec!["zeta"]]
        );
        assert_eq!(
            order.inspect_layers().unwrap()[1][0].prerequisites,
            ["late-root"]
        );
    }

    #[test]
    fn layer_cycles_report_named_paths_without_partial_results_or_mutation() {
        let mut forward = registered(&["ready", "tail", "zeta", "beta", "alpha"]);
        let mut reverse = registered(&["ready", "tail", "zeta", "beta", "alpha"]);
        let edges = [
            ("ready", "zeta"),
            ("zeta", "alpha"),
            ("alpha", "zeta"),
            ("zeta", "beta"),
            ("beta", "zeta"),
            ("zeta", "tail"),
        ];
        for &(a, b) in &edges {
            forward.before(a, b).unwrap();
        }
        for &(a, b) in edges.iter().rev() {
            reverse.before(a, b).unwrap();
        }
        let diagnostic = OrderDiagnostic::Cycle {
            path: ["zeta", "beta", "zeta"].map(String::from).into(),
        };
        for order in [&forward, &reverse] {
            let original = order.edges.clone();
            assert_eq!(order.resolve_layers(), Err(diagnostic.clone()));
            assert_eq!(order.inspect_layers(), Err(diagnostic.clone()));
            assert_eq!(order.resolve_checked(), Err(diagnostic.clone()));
            assert_eq!(order.edges, original);
        }
        let mut self_cycle = registered(&["ready", "loop"]);
        self_cycle.before("loop", "loop").unwrap();
        let diagnostic = OrderDiagnostic::Cycle {
            path: vec!["loop".into(), "loop".into()],
        };
        assert_eq!(self_cycle.resolve_layers(), Err(diagnostic.clone()));
        assert_eq!(self_cycle.inspect_layers(), Err(diagnostic));
    }

    #[test]
    fn rejected_missing_nodes_and_cycles_preserve_layers_and_inspection() {
        let mut order = registered(&["render", "input"]);
        order.before_checked("input", "render").unwrap();
        for (first, second, missing) in [
            ("missing", "input", vec!["missing"]),
            ("input", "missing", vec!["missing"]),
            ("missing", "other", vec!["missing", "other"]),
            ("missing", "missing", vec!["missing"]),
        ] {
            assert_eq!(
                order.before_checked(first, second),
                Err(OrderDiagnostic::UnknownSystems {
                    first: first.into(),
                    second: second.into(),
                    missing: missing.into_iter().map(String::from).collect(),
                })
            );
            assert_eq!(
                order.resolve_layers().unwrap(),
                vec![vec!["input"], vec!["render"]]
            );
            assert_eq!(
                order.inspect_layers().unwrap()[1][0].prerequisites,
                ["input"]
            );
        }
        assert_eq!(
            order.before("absent", "render"),
            Err(OrderError::UnknownSystem)
        );
        assert_eq!(
            order.before_checked("render", "input").unwrap_err().kind(),
            OrderError::Cycle
        );
        assert_eq!(
            order.resolve_layers().unwrap(),
            vec![vec!["input"], vec!["render"]]
        );
        assert_eq!(
            order.inspect_layers().unwrap()[1][0].prerequisites,
            ["input"]
        );
    }

    #[test]
    fn layers_satisfy_dependency_depth_and_unique_membership_across_small_dags() {
        let names = ["third", "last", "first", "second"];
        let topological = [2, 3, 0, 1];
        let mut possible_edges = Vec::new();
        for (offset, &a) in topological.iter().enumerate() {
            for &b in &topological[offset + 1..] {
                possible_edges.push((a, b));
            }
        }
        for mask in 0..(1 << possible_edges.len()) {
            let mut order = registered(&names);
            for (bit, &(a, b)) in possible_edges.iter().enumerate() {
                if mask & (1 << bit) != 0 {
                    order.before(names[a], names[b]).unwrap();
                }
            }
            let layers = order.resolve_layers().unwrap();
            let mut positions = [None; 4];
            for (depth, layer) in layers.iter().enumerate() {
                assert!(!layer.is_empty());
                let indices: Vec<_> = layer
                    .iter()
                    .map(|name| names.iter().position(|n| n == name).unwrap())
                    .collect();
                assert!(indices.windows(2).all(|pair| pair[0] < pair[1]));
                for index in indices {
                    assert_eq!(positions[index].replace(depth), None);
                }
            }
            assert!(positions.iter().all(Option::is_some));
            for (index, depth) in positions.iter().enumerate() {
                let expected = order
                    .edges
                    .iter()
                    .filter(|&&(_, b)| b == index)
                    .map(|&(a, _)| positions[a].unwrap() + 1)
                    .max()
                    .unwrap_or(0);
                assert_eq!(*depth, Some(expected));
            }
        }
    }

    #[test]
    fn layers_cover_maximum_width_depth_and_dense_dependencies() {
        let names: Vec<_> = (0..MAX_SYSTEMS)
            .map(|index| format!("system-{index}"))
            .collect();
        let mut order = registered(&names.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(
            order.resolve_layers().unwrap(),
            vec![names.iter().map(String::as_str).collect::<Vec<_>>()]
        );
        assert_eq!(order.inspect_layers().unwrap()[0].len(), MAX_SYSTEMS);
        // A fully connected DAG reaches both the longest depth and the maximum
        // acyclic edge count; insert without repeated transactional validation.
        for (b, second) in names.iter().enumerate() {
            for first in &names[..b] {
                order.before(first, second).unwrap();
            }
        }
        let layers = order.resolve_layers().unwrap();
        assert_eq!(layers.len(), MAX_SYSTEMS);
        for (layer, name) in layers.iter().zip(&names) {
            assert_eq!(layer, &[name.as_str()]);
        }
        let inspected = order.inspect_layers().unwrap();
        for (index, layer) in inspected.iter().enumerate() {
            assert_eq!(layer.len(), 1);
            assert_eq!(layer[0].name, names[index]);
            assert_eq!(layer[0].prerequisites, names[..index]);
        }
        order.before(names.last().unwrap(), &names[0]).unwrap();
        assert_eq!(
            order.resolve_layers().unwrap_err().kind(),
            OrderError::Cycle
        );
        assert_eq!(
            order.inspect_layers(),
            Err(order.resolve_checked().unwrap_err())
        );
    }
}
