//! Authored triangular navigation surfaces. Walk portals join exact shared edges;
//! directional traversal links remain explicit host actions. Bake clearance for
//! the agent radius before construction. This is not an automatic mesh baker.
//! Use the checked constructors for indexed authoring diagnostics; the original
//! constructors retain their permissive adjacency rules.
use super::NavigationError;
use crate::ai::WorkBudget;
use ferravex_math::Vec3;

const MAX_TRIANGLES: usize = 2048;
const MAX_LINKS: usize = 16384;
const MAX_COORDINATE: f32 = 1_000_000.;

#[derive(Clone, Copy, Debug)]
pub struct NavTriangle(pub [Vec3; 3]);
impl NavTriangle {
    fn center(self) -> Vec3 {
        (self.0[0] + self.0[1] + self.0[2]) / 3.
    }
    /// Test either winding using a 0.001 world-unit plane tolerance and 1e-5
    /// barycentric edge tolerance. Non-finite or degenerate input returns false.
    /// Geometry arithmetic uses f64 intermediates, including thin faces.
    pub fn contains(self, p: Vec3) -> bool {
        if !p.is_finite() || self.0.iter().any(|v| !v.is_finite()) {
            return false;
        }
        let [a, b, c] = self.0;
        let u = difference(b, a);
        let v = difference(c, a);
        let d = difference(p, a);
        let n = cross(u, v);
        let den = dot(n, n);
        if den < 1e-10 || dot(d, n).abs() > 0.001 * den.sqrt() {
            return false;
        }
        // Cross products avoid subtracting nearly equal squared edge lengths.
        // Widen before subtraction so skinny, large-coordinate faces work too.
        let x = dot(cross(d, v), n) / den;
        let y = dot(cross(u, d), n) / den;
        x >= -1e-5 && y >= -1e-5 && x + y <= 1.00001
    }
}

fn difference(a: Vec3, b: Vec3) -> [f64; 3] {
    [
        f64::from(a.x) - f64::from(b.x),
        f64::from(a.y) - f64::from(b.y),
        f64::from(a.z) - f64::from(b.z),
    ]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// First authoring failure from a checked mesh operation, with zero-based input
/// indices. Edge `e` runs from vertex `e` to vertex `(e + 1) % 3`.
///
/// Geometry is checked before adjacency, then links in input order. Correct the
/// reported issue and validate again to find subsequent issues. Both global
/// winding directions are accepted; neighbors must traverse a shared edge in
/// opposite directions. Disconnected components need not share an orientation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavMeshDiagnostic {
    EmptyMesh,
    TriangleCapacity {
        count: usize,
        maximum: usize,
    },
    LinkCapacity {
        count: usize,
        maximum: usize,
    },
    NonFiniteVertex {
        triangle: usize,
        vertex: usize,
    },
    /// A vertex component exceeds the supported absolute limit of 1,000,000.
    VertexOutOfRange {
        triangle: usize,
        vertex: usize,
    },
    /// Repeated/collinear vertices or squared cross-product magnitude < 1e-6.
    DegenerateTriangle {
        triangle: usize,
    },
    /// The same three exact vertices appear in two faces, in any order.
    DuplicateTriangle {
        triangle: usize,
        other: usize,
    },
    /// More than two faces use the same exact edge.
    NonManifoldEdge {
        triangle: usize,
        edge: usize,
        first_neighbor: usize,
        second_neighbor: usize,
    },
    InconsistentWinding {
        triangle: usize,
        edge: usize,
        neighbor: usize,
        neighbor_edge: usize,
    },
    SourceOutOfBounds {
        link: usize,
        triangle: usize,
    },
    DestinationOutOfBounds {
        link: usize,
        triangle: usize,
    },
    SelfLink {
        link: usize,
        triangle: usize,
    },
    InvalidLinkCost {
        link: usize,
    },
    /// Entry is non-finite or outside the indicated face's containment tolerance.
    InvalidEntry {
        link: usize,
        triangle: usize,
    },
    /// Exit is non-finite or outside the destination face's containment tolerance.
    InvalidExit {
        link: usize,
        triangle: usize,
    },
    MissingSharedEdge {
        link: usize,
        from: usize,
        to: usize,
    },
    /// Walk entry/exit differ by more than 0.001 world units.
    PortalMismatch {
        link: usize,
    },
    /// Walk entry or exit is more than 0.001 units from the shared edge segment.
    PortalOffEdge {
        link: usize,
        triangle: usize,
        edge: usize,
    },
    /// The shared mesh/grid identity counter is exhausted; no mesh was installed.
    RevisionExhausted,
}

impl NavMeshDiagnostic {
    fn navigation_error(self) -> NavigationError {
        match self {
            Self::EmptyMesh | Self::TriangleCapacity { .. } | Self::LinkCapacity { .. } => {
                NavigationError::Capacity
            }
            Self::RevisionExhausted => NavigationError::RevisionExhausted,
            _ => NavigationError::InvalidInput,
        }
    }
}

impl std::fmt::Display for NavMeshDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::EmptyMesh => write!(f, "add at least one navigation triangle"),
            Self::TriangleCapacity { count, maximum } => {
                write!(f, "{count} triangles exceed the limit of {maximum}")
            }
            Self::LinkCapacity { count, maximum } => {
                write!(f, "{count} links exceed the limit of {maximum}")
            }
            Self::NonFiniteVertex { triangle, vertex } => {
                write!(f, "triangle {triangle} vertex {vertex} must be finite")
            }
            Self::VertexOutOfRange { triangle, vertex } => write!(
                f,
                "triangle {triangle} vertex {vertex} must have components within +/-1,000,000"
            ),
            Self::DegenerateTriangle { triangle } => write!(
                f,
                "triangle {triangle} needs three distinct non-collinear vertices with squared cross magnitude >= 1e-6"
            ),
            Self::DuplicateTriangle { triangle, other } => {
                write!(
                    f,
                    "triangle {triangle} duplicates triangle {other}; remove one face"
                )
            }
            Self::NonManifoldEdge {
                triangle,
                edge,
                first_neighbor,
                second_neighbor,
            } => write!(
                f,
                "triangle {triangle} edge {edge} is shared with triangles {first_neighbor} and {second_neighbor}; at most two faces may share an edge"
            ),
            Self::InconsistentWinding {
                triangle,
                edge,
                neighbor,
                neighbor_edge,
            } => write!(
                f,
                "triangle {triangle} edge {edge} and triangle {neighbor} edge {neighbor_edge} run in the same direction; reverse one face's winding"
            ),
            Self::SourceOutOfBounds { link, triangle } => {
                write!(f, "link {link} source triangle {triangle} does not exist")
            }
            Self::DestinationOutOfBounds { link, triangle } => {
                write!(
                    f,
                    "link {link} destination triangle {triangle} does not exist"
                )
            }
            Self::SelfLink { link, triangle } => {
                write!(
                    f,
                    "link {link} must connect distinct triangles, not triangle {triangle} to itself"
                )
            }
            Self::InvalidLinkCost { link } => {
                write!(f, "link {link} cost must be finite and nonnegative")
            }
            Self::InvalidEntry { link, triangle } => {
                write!(
                    f,
                    "link {link} entry must be finite and lie on triangle {triangle}"
                )
            }
            Self::InvalidExit { link, triangle } => {
                write!(
                    f,
                    "link {link} exit must be finite and lie on triangle {triangle}"
                )
            }
            Self::MissingSharedEdge { link, from, to } => write!(
                f,
                "walk link {link} triangles {from} and {to} must share an exact vertex edge; use an explicit traversal for a gap"
            ),
            Self::PortalMismatch { link } => {
                write!(
                    f,
                    "walk link {link} entry and exit must be within 0.001 units"
                )
            }
            Self::PortalOffEdge {
                link,
                triangle,
                edge,
            } => write!(
                f,
                "walk link {link} entry and exit must lie on triangle {triangle} edge {edge} within 0.001 units"
            ),
            Self::RevisionExhausted => write!(f, "navigation identity counter exhausted"),
        }
    }
}

impl std::error::Error for NavMeshDiagnostic {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Traversal {
    Walk,
    Jump,
    Drop,
    Climb,
}
#[derive(Clone, Copy, Debug)]
pub struct TraversalLink {
    pub from: usize,
    pub to: usize,
    pub entry: Vec3,
    pub exit: Vec3,
    pub kind: Traversal,
    pub cost: f32,
    pub enabled: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct NavWaypoint {
    pub position: Vec3,
    pub traversal: Traversal,
}

fn validate_capacity(triangles: usize, links: usize) -> Result<(), NavMeshDiagnostic> {
    if triangles == 0 {
        return Err(NavMeshDiagnostic::EmptyMesh);
    }
    if triangles > MAX_TRIANGLES {
        return Err(NavMeshDiagnostic::TriangleCapacity {
            count: triangles,
            maximum: MAX_TRIANGLES,
        });
    }
    if links > MAX_LINKS {
        return Err(NavMeshDiagnostic::LinkCapacity {
            count: links,
            maximum: MAX_LINKS,
        });
    }
    Ok(())
}

fn validate_triangles(triangles: &[NavTriangle]) -> Result<(), NavMeshDiagnostic> {
    for (triangle, t) in triangles.iter().enumerate() {
        for (vertex, p) in t.0.iter().enumerate() {
            if !p.is_finite() {
                return Err(NavMeshDiagnostic::NonFiniteVertex { triangle, vertex });
            }
            if p.abs().max_element() > MAX_COORDINATE {
                return Err(NavMeshDiagnostic::VertexOutOfRange { triangle, vertex });
            }
        }
        let n = cross(difference(t.0[1], t.0[0]), difference(t.0[2], t.0[0]));
        if dot(n, n) < f64::from(1e-6_f32) {
            return Err(NavMeshDiagnostic::DegenerateTriangle { triangle });
        }
    }
    Ok(())
}

fn shared_edge(a: NavTriangle, b: NavTriangle) -> Option<(usize, usize)> {
    for edge in 0..3 {
        for other in 0..3 {
            let [a0, a1] = [a.0[edge], a.0[(edge + 1) % 3]];
            let [b0, b1] = [b.0[other], b.0[(other + 1) % 3]];
            if (a0 == b0 && a1 == b1) || (a0 == b1 && a1 == b0) {
                return Some((edge, other));
            }
        }
    }
    None
}

fn validate_topology(triangles: &[NavTriangle]) -> Result<(), NavMeshDiagnostic> {
    let mut neighbors = vec![[None; 3]; triangles.len()];
    let mut winding = None;
    for (i, a) in triangles.iter().enumerate() {
        for (j, b) in triangles.iter().enumerate().skip(i + 1) {
            if a.0.iter().all(|p| b.0.contains(p)) {
                return Err(NavMeshDiagnostic::DuplicateTriangle {
                    triangle: j,
                    other: i,
                });
            }
            if let Some((edge, other_edge)) = shared_edge(*a, *b) {
                for (triangle, edge, neighbor) in [(i, edge, j), (j, other_edge, i)] {
                    if let Some(first_neighbor) = neighbors[triangle][edge] {
                        return Err(NavMeshDiagnostic::NonManifoldEdge {
                            triangle,
                            edge,
                            first_neighbor,
                            second_neighbor: neighbor,
                        });
                    }
                    neighbors[triangle][edge] = Some(neighbor);
                }
                if a.0[edge] == b.0[other_edge] && winding.is_none() {
                    winding = Some(NavMeshDiagnostic::InconsistentWinding {
                        triangle: i,
                        edge,
                        neighbor: j,
                        neighbor_edge: other_edge,
                    });
                }
            }
        }
    }
    // A third face on an edge is more useful to report than the winding conflict
    // that inevitably accompanies it.
    winding.map_or(Ok(()), Err)
}

fn on_edge(p: Vec3, a: Vec3, b: Vec3) -> bool {
    let edge = difference(b, a);
    let offset = difference(p, a);
    let t = (dot(offset, edge) / dot(edge, edge)).clamp(0., 1.);
    let distance = [
        offset[0] - t * edge[0],
        offset[1] - t * edge[1],
        offset[2] - t * edge[2],
    ];
    dot(distance, distance) <= 0.001_f64.powi(2)
}

fn validate_links(
    triangles: &[NavTriangle],
    links: &[TraversalLink],
    checked: bool,
) -> Result<(), NavMeshDiagnostic> {
    for (link, l) in links.iter().enumerate() {
        let from = triangles
            .get(l.from)
            .ok_or(NavMeshDiagnostic::SourceOutOfBounds {
                link,
                triangle: l.from,
            })?;
        let to = triangles
            .get(l.to)
            .ok_or(NavMeshDiagnostic::DestinationOutOfBounds {
                link,
                triangle: l.to,
            })?;
        if checked && l.from == l.to {
            return Err(NavMeshDiagnostic::SelfLink {
                link,
                triangle: l.from,
            });
        }
        if !l.cost.is_finite() || l.cost < 0. {
            return Err(NavMeshDiagnostic::InvalidLinkCost { link });
        }
        if !from.contains(l.entry) {
            return Err(NavMeshDiagnostic::InvalidEntry {
                link,
                triangle: l.from,
            });
        }
        if !to.contains(l.exit) {
            return Err(NavMeshDiagnostic::InvalidExit {
                link,
                triangle: l.to,
            });
        }
        if l.kind == Traversal::Walk {
            if l.entry.distance(l.exit) > 0.001 {
                return Err(NavMeshDiagnostic::PortalMismatch { link });
            }
            if checked {
                let (edge, _) =
                    shared_edge(*from, *to).ok_or(NavMeshDiagnostic::MissingSharedEdge {
                        link,
                        from: l.from,
                        to: l.to,
                    })?;
                let a = from.0[edge];
                let b = from.0[(edge + 1) % 3];
                if !on_edge(l.entry, a, b) || !on_edge(l.exit, a, b) {
                    return Err(NavMeshDiagnostic::PortalOffEdge {
                        link,
                        triangle: l.from,
                        edge,
                    });
                }
            }
            if !to.contains(l.entry) {
                return Err(NavMeshDiagnostic::InvalidEntry {
                    link,
                    triangle: l.to,
                });
            }
        }
    }
    Ok(())
}

fn welded_links(triangles: &[NavTriangle]) -> Result<Vec<TraversalLink>, NavMeshDiagnostic> {
    let mut links = Vec::new();
    for (i, a) in triangles.iter().enumerate() {
        for (j, b) in triangles.iter().enumerate().skip(i + 1) {
            let mut shared = [Vec3::ZERO; 3];
            let mut n = 0;
            for v in a.0 {
                if b.0.contains(&v) {
                    shared[n] = v;
                    n += 1;
                }
            }
            if n == 2 {
                let p = (shared[0] + shared[1]) * 0.5;
                let cost = a.center().distance(b.center());
                for (from, to) in [(i, j), (j, i)] {
                    if links.len() == MAX_LINKS {
                        return Err(NavMeshDiagnostic::LinkCapacity {
                            count: MAX_LINKS + 1,
                            maximum: MAX_LINKS,
                        });
                    }
                    links.push(TraversalLink {
                        from,
                        to,
                        entry: p,
                        exit: p,
                        kind: Traversal::Walk,
                        cost,
                        enabled: true,
                    });
                }
            }
        }
    }
    Ok(links)
}

pub struct NavMesh {
    triangles: Vec<NavTriangle>,
    links: Vec<TraversalLink>,
    edges: Vec<Vec<usize>>,
    blocked: Vec<bool>,
    revision: u64,
    identity: u64,
}
impl NavMesh {
    /// Construction allocates; searches reuse their own scratch space. Retains
    /// permissive legacy adjacency; use [`Self::new_checked`] for authoring.
    pub fn new(
        triangles: Vec<NavTriangle>,
        links: Vec<TraversalLink>,
    ) -> Result<Self, NavigationError> {
        validate_capacity(triangles.len(), links.len())
            .map_err(NavMeshDiagnostic::navigation_error)?;
        validate_triangles(&triangles).map_err(NavMeshDiagnostic::navigation_error)?;
        validate_links(&triangles, &links, false).map_err(NavMeshDiagnostic::navigation_error)?;
        Self::build(triangles, links).map_err(NavMeshDiagnostic::navigation_error)
    }

    /// Inspect borrowed authoring data without allocating a mesh identity or
    /// changing a live mesh. Scratch storage is at most three neighbor slots per
    /// face; topology checking is O(triangles²), bounded to 2048 faces/16384 links.
    ///
    /// Checks finite, bounded, nondegenerate faces, duplicates, manifold shared
    /// edges and consistent relative winding. Walk links require an exact shared
    /// edge and endpoints within 0.001 units of its segment. Disabled links are
    /// checked too; non-walk links may bridge gaps. Self links are rejected.
    /// Missing reverse links are allowed: authored links remain directional.
    /// This does not detect geometric overlaps, T-junctions, clearance or slope.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::navigation::{NavMesh, NavMeshDiagnostic, NavTriangle};
    /// let faces = [NavTriangle([Vec3::ZERO, Vec3::X, Vec3::X])];
    /// assert_eq!(NavMesh::validate(&faces, &[]),
    ///     Err(NavMeshDiagnostic::DegenerateTriangle { triangle: 0 }));
    /// ```
    pub fn validate(
        triangles: &[NavTriangle],
        links: &[TraversalLink],
    ) -> Result<(), NavMeshDiagnostic> {
        validate_capacity(triangles.len(), links.len())?;
        validate_triangles(triangles)?;
        validate_topology(triangles)?;
        validate_links(triangles, links, true)
    }

    /// Construct only after all [`Self::validate`] checks pass. Errors carry
    /// input indices and implement [`std::fmt::Display`] for editor messages.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::navigation::{NavMesh, NavTriangle};
    /// let mesh = NavMesh::new_checked(
    ///     vec![NavTriangle([Vec3::ZERO, Vec3::X, Vec3::Z])], vec![])?;
    /// assert_eq!(mesh.locate(Vec3::ZERO), Some(0));
    /// # Ok::<(), ferravex_play::navigation::NavMeshDiagnostic>(())
    /// ```
    pub fn new_checked(
        triangles: Vec<NavTriangle>,
        links: Vec<TraversalLink>,
    ) -> Result<Self, NavMeshDiagnostic> {
        Self::validate(&triangles, &links)?;
        Self::build(triangles, links)
    }

    /// Replace an authored mesh transactionally. On error, geometry, block/link
    /// state, identity and existing searches are untouched. On success, faces
    /// start unblocked, link enabled flags come from the input, and every old
    /// search/result is invalidated by a fresh identity (even for identical data).
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::navigation::{NavMesh, NavTriangle};
    /// let mut mesh = NavMesh::new_checked(
    ///     vec![NavTriangle([Vec3::ZERO, Vec3::X, Vec3::Z])], vec![])?;
    /// assert!(mesh.replace_checked(vec![], vec![]).is_err());
    /// assert_eq!(mesh.locate(Vec3::ZERO), Some(0));
    /// # Ok::<(), ferravex_play::navigation::NavMeshDiagnostic>(())
    /// ```
    pub fn replace_checked(
        &mut self,
        triangles: Vec<NavTriangle>,
        links: Vec<TraversalLink>,
    ) -> Result<(), NavMeshDiagnostic> {
        let replacement = Self::new_checked(triangles, links)?;
        *self = replacement;
        Ok(())
    }

    fn build(
        triangles: Vec<NavTriangle>,
        links: Vec<TraversalLink>,
    ) -> Result<Self, NavMeshDiagnostic> {
        #[allow(deprecated)] // Preserve the workspace Rust 1.88 minimum.
        let identity = super::NEXT_GRID_ID
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |v| v.checked_add(1),
            )
            .map_err(|_| NavMeshDiagnostic::RevisionExhausted)?;
        let mut edges = vec![Vec::new(); triangles.len()];
        for (i, l) in links.iter().enumerate() {
            edges[l.from].push(i);
        }
        Ok(Self {
            identity,
            blocked: vec![false; triangles.len()],
            triangles,
            links,
            edges,
            revision: 0,
        })
    }
    /// Weld exact shared vertex edges; O(triangles²) setup, bounded to 2048 faces.
    /// Use [`Self::welded_checked`] to diagnose malformed adjacency before welding.
    pub fn welded(triangles: Vec<NavTriangle>) -> Result<Self, NavigationError> {
        validate_capacity(triangles.len(), 0).map_err(NavMeshDiagnostic::navigation_error)?;
        let links = welded_links(&triangles).map_err(NavMeshDiagnostic::navigation_error)?;
        Self::new(triangles, links)
    }

    /// Validate geometry and topology before generating bidirectional walk
    /// portals, then check generated endpoints before publishing the mesh.
    /// Uses the same midpoint/cost/order as [`Self::welded`] on valid input.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::navigation::{NavMesh, NavTriangle};
    /// let mesh = NavMesh::welded_checked(vec![
    ///     NavTriangle([Vec3::ZERO, Vec3::X, Vec3::Z]),
    ///     NavTriangle([Vec3::X, Vec3::X + Vec3::Z, Vec3::Z]),
    /// ])?;
    /// assert!(mesh.search(Vec3::ZERO, Vec3::X + Vec3::Z).is_ok());
    /// # Ok::<(), ferravex_play::navigation::NavMeshDiagnostic>(())
    /// ```
    pub fn welded_checked(triangles: Vec<NavTriangle>) -> Result<Self, NavMeshDiagnostic> {
        validate_capacity(triangles.len(), 0)?;
        validate_triangles(&triangles)?;
        validate_topology(&triangles)?;
        let links = welded_links(&triangles)?;
        validate_links(&triangles, &links, true)?;
        Self::build(triangles, links)
    }
    pub fn locate(&self, p: Vec3) -> Option<usize> {
        self.triangles
            .iter()
            .enumerate()
            .find(|(i, t)| !self.blocked[*i] && t.contains(p))
            .map(|(i, _)| i)
    }
    pub fn set_blocked(&mut self, index: usize, blocked: bool) -> Result<(), NavigationError> {
        let value = self
            .blocked
            .get_mut(index)
            .ok_or(NavigationError::OutOfBounds)?;
        if *value != blocked {
            let revision = self
                .revision
                .checked_add(1)
                .ok_or(NavigationError::RevisionExhausted)?;
            *value = blocked;
            self.revision = revision;
        }
        Ok(())
    }
    pub fn set_link_enabled(&mut self, index: usize, enabled: bool) -> Result<(), NavigationError> {
        let l = self
            .links
            .get_mut(index)
            .ok_or(NavigationError::OutOfBounds)?;
        if l.enabled != enabled {
            let revision = self
                .revision
                .checked_add(1)
                .ok_or(NavigationError::RevisionExhausted)?;
            l.enabled = enabled;
            self.revision = revision;
        }
        Ok(())
    }
    pub fn search(&self, start: Vec3, end: Vec3) -> Result<NavSearch, NavigationError> {
        let a = self.locate(start).ok_or(NavigationError::BlockedEndpoint)?;
        let b = self.locate(end).ok_or(NavigationError::BlockedEndpoint)?;
        let n = self.triangles.len();
        let mut cost = vec![f64::INFINITY; n];
        cost[a] = 0.;
        Ok(NavSearch {
            identity: self.identity,
            revision: self.revision,
            cost,
            previous: vec![None; n],
            closed: vec![false; n],
            start: a,
            end: b,
            end_point: end,
            route: Vec::with_capacity(n * 3 + 2),
            done: false,
            current: None,
            edge: 0,
        })
    }
}
/// Incremental Dijkstra search. Each node scan and outgoing edge consumes a path
/// work unit. A mesh edit invalidates the search, including an existing result.
pub struct NavSearch {
    revision: u64,
    identity: u64,
    cost: Vec<f64>,
    previous: Vec<Option<usize>>,
    closed: Vec<bool>,
    start: usize,
    end: usize,
    end_point: Vec3,
    route: Vec<NavWaypoint>,
    done: bool,
    current: Option<usize>,
    edge: usize,
}
impl NavSearch {
    /// Returns true only when the route is complete; NoPath or mesh edits fail.
    /// Up to 2048 distance comparisons select a node; one unit accounts that scan.
    pub fn step(
        &mut self,
        mesh: &NavMesh,
        budget: &mut WorkBudget,
    ) -> Result<bool, NavigationError> {
        if self.identity != mesh.identity
            || self.revision != mesh.revision
            || self.cost.len() != mesh.triangles.len()
        {
            return Err(NavigationError::InvalidInput);
        }
        if self.done {
            return Ok(true);
        }
        while budget.spend_path_steps(1) {
            if let Some(i) = self.current {
                if let Some(&link) = mesh.edges[i].get(self.edge) {
                    self.edge += 1;
                    let l = mesh.links[link];
                    if l.enabled && !mesh.blocked[l.to] && !self.closed[l.to] {
                        let cost = self.cost[i] + f64::from(l.cost);
                        if cost < self.cost[l.to] {
                            self.cost[l.to] = cost;
                            self.previous[l.to] = Some(link);
                        }
                    }
                } else {
                    self.current = None;
                }
            } else {
                let next = self
                    .cost
                    .iter()
                    .enumerate()
                    .filter(|(i, c)| !self.closed[*i] && c.is_finite())
                    .min_by(|(i, a), (j, b)| a.total_cmp(b).then(i.cmp(j)))
                    .map(|(i, _)| i)
                    .ok_or(NavigationError::NoPath)?;
                if next == self.end {
                    self.route.push(NavWaypoint {
                        position: self.end_point,
                        traversal: Traversal::Walk,
                    });
                    self.route.push(NavWaypoint {
                        position: mesh.triangles[self.end].center(),
                        traversal: Traversal::Walk,
                    });
                    let mut node = self.end;
                    while node != self.start {
                        let l = mesh.links[self.previous[node].ok_or(NavigationError::NoPath)?];
                        self.route.push(NavWaypoint {
                            position: l.exit,
                            traversal: l.kind,
                        });
                        if l.kind != Traversal::Walk {
                            self.route.push(NavWaypoint {
                                position: l.entry,
                                traversal: Traversal::Walk,
                            });
                        }
                        self.route.push(NavWaypoint {
                            position: mesh.triangles[l.from].center(),
                            traversal: Traversal::Walk,
                        });
                        node = l.from;
                    }
                    self.route.reverse();
                    self.done = true;
                    return Ok(true);
                }
                self.closed[next] = true;
                self.current = Some(next);
                self.edge = 0;
            }
        }
        Ok(false)
    }
    pub fn route<'a>(&'a self, mesh: &NavMesh) -> Result<&'a [NavWaypoint], NavigationError> {
        if self.identity != mesh.identity || self.revision != mesh.revision || !self.done {
            Err(NavigationError::InvalidInput)
        } else {
            Ok(&self.route)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn faces() -> Vec<NavTriangle> {
        vec![
            NavTriangle([Vec3::ZERO, Vec3::X, Vec3::Z]),
            NavTriangle([Vec3::X, Vec3::X + Vec3::Z, Vec3::Z]),
        ]
    }
    #[test]
    fn portals_budget_and_revision() {
        let mut m = NavMesh::welded(faces()).unwrap();
        let mut s = m.search(Vec3::ZERO, (Vec3::X + Vec3::Z) * 0.9).unwrap();
        assert!(!s.step(&m, &mut WorkBudget::new(0, 0, 0)).unwrap());
        for _ in 0..20 {
            if s.step(&m, &mut WorkBudget::new(0, 0, 1)).unwrap() {
                break;
            }
        }
        assert!(s.route(&m).unwrap().len() >= 2);
        m.set_blocked(1, true).unwrap();
        assert!(s.route(&m).is_err());
        assert!(s.step(&m, &mut WorkBudget::new(0, 0, 99)).is_err());
    }
    #[test]
    fn directional_links_need_host_traversal_and_disable() {
        let mut f = faces();
        for v in &mut f[1].0 {
            *v += Vec3::Y * 2.;
        }
        let l = TraversalLink {
            from: 0,
            to: 1,
            entry: Vec3::X,
            exit: Vec3::X + Vec3::Y * 2.,
            kind: Traversal::Climb,
            cost: 3.,
            enabled: true,
        };
        let mut m = NavMesh::new(f, vec![l]).unwrap();
        let mut s = m.search(Vec3::ZERO, Vec3::X + Vec3::Y * 2.).unwrap();
        assert!(s.step(&m, &mut WorkBudget::new(0, 0, 30)).unwrap());
        assert!(
            s.route(&m)
                .unwrap()
                .iter()
                .any(|w| w.traversal == Traversal::Climb)
        );
        let mut reverse = m.search(Vec3::X + Vec3::Y * 2., Vec3::ZERO).unwrap();
        assert_eq!(
            reverse.step(&m, &mut WorkBudget::new(0, 0, 30)),
            Err(NavigationError::NoPath)
        );
        m.set_link_enabled(0, false).unwrap();
        let mut s = m.search(Vec3::ZERO, Vec3::X + Vec3::Y * 2.).unwrap();
        assert_eq!(
            s.step(&m, &mut WorkBudget::new(0, 0, 30)),
            Err(NavigationError::NoPath)
        );
    }

    fn walk_link() -> TraversalLink {
        TraversalLink {
            from: 0,
            to: 1,
            entry: (Vec3::X + Vec3::Z) * 0.5,
            exit: (Vec3::X + Vec3::Z) * 0.5,
            kind: Traversal::Walk,
            cost: 1.,
            enabled: true,
        }
    }

    fn completed_route(mesh: &NavMesh) -> Vec<(Vec3, Traversal)> {
        let mut search = mesh.search(Vec3::ZERO, Vec3::X + Vec3::Z).unwrap();
        assert!(search.step(mesh, &mut WorkBudget::new(0, 0, 30)).unwrap());
        search
            .route(mesh)
            .unwrap()
            .iter()
            .map(|w| (w.position, w.traversal))
            .collect()
    }

    #[test]
    fn diagnostic_capacity_is_checked_before_geometry() {
        assert_eq!(
            NavMesh::validate(&[], &[]),
            Err(NavMeshDiagnostic::EmptyMesh)
        );
        let too_many = vec![NavTriangle([Vec3::ZERO; 3]); MAX_TRIANGLES + 1];
        assert_eq!(
            NavMesh::validate(&too_many, &[]),
            Err(NavMeshDiagnostic::TriangleCapacity {
                count: MAX_TRIANGLES + 1,
                maximum: MAX_TRIANGLES,
            })
        );
        assert_eq!(
            NavMesh::validate(&faces(), &vec![walk_link(); MAX_LINKS + 1]),
            Err(NavMeshDiagnostic::LinkCapacity {
                count: MAX_LINKS + 1,
                maximum: MAX_LINKS
            })
        );
        // Exact limits remain inclusive. Repeated directional links are legal.
        assert!(NavMesh::validate(&faces(), &vec![walk_link(); MAX_LINKS]).is_ok());
        assert_eq!(
            NavMesh::new(vec![], vec![]).err(),
            Some(NavigationError::Capacity)
        );
    }

    #[test]
    fn diagnostic_identifies_nonfinite_and_out_of_range_vertices() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut f = faces();
            f[1].0[2].y = value;
            assert_eq!(
                NavMesh::validate(&f, &[]),
                Err(NavMeshDiagnostic::NonFiniteVertex {
                    triangle: 1,
                    vertex: 2
                })
            );
        }
        let mut f = faces();
        f[1].0[1].x = MAX_COORDINATE + 1.;
        assert_eq!(
            NavMesh::validate(&f, &[]),
            Err(NavMeshDiagnostic::VertexOutOfRange {
                triangle: 1,
                vertex: 1
            })
        );
    }

    #[test]
    fn diagnostic_rejects_repeated_collinear_and_tiny_faces() {
        for triangle in [
            NavTriangle([Vec3::ZERO, Vec3::X, Vec3::X]),
            NavTriangle([Vec3::ZERO, Vec3::X, Vec3::X * 2.]),
            NavTriangle([Vec3::ZERO, Vec3::X, Vec3::Z * 0.0001]),
        ] {
            assert_eq!(
                NavMesh::welded_checked(vec![triangle]).err(),
                Some(NavMeshDiagnostic::DegenerateTriangle { triangle: 0 })
            );
            assert_eq!(
                NavMesh::new(vec![triangle], vec![]).err(),
                Some(NavigationError::InvalidInput)
            );
        }
    }

    #[test]
    fn relative_winding_accepts_both_orientations_and_preserves_routes() {
        for reverse in [false, true] {
            let mut f = faces();
            if reverse {
                for t in &mut f {
                    t.0.swap(1, 2);
                }
            }
            let legacy = NavMesh::welded(f.clone()).unwrap();
            let checked = NavMesh::welded_checked(f).unwrap();
            assert_eq!(completed_route(&legacy), completed_route(&checked));
            assert_eq!(checked.links.len(), 2);
            for (a, b) in legacy.links.iter().zip(&checked.links) {
                assert_eq!((a.from, a.to, a.cost), (b.from, b.to, b.cost));
            }
        }
        let mut f = faces();
        f[1].0.swap(1, 2);
        assert_eq!(
            NavMesh::validate(&f, &[]),
            Err(NavMeshDiagnostic::InconsistentWinding {
                triangle: 0,
                edge: 1,
                neighbor: 1,
                neighbor_edge: 0,
            })
        );
        assert!(
            NavMesh::welded(f).is_ok(),
            "legacy winding remains permissive"
        );
    }

    #[test]
    fn disconnected_and_folded_faces_do_not_require_an_up_axis() {
        let a = NavTriangle([Vec3::ZERO, Vec3::X, Vec3::Z]);
        let folded = NavTriangle([Vec3::X, Vec3::ZERO, Vec3::Y]);
        assert!(NavMesh::welded_checked(vec![a, folded]).is_ok());
        let mut disconnected = a;
        disconnected.0.swap(1, 2);
        for v in &mut disconnected.0 {
            *v += Vec3::Y * 2.;
        }
        assert!(NavMesh::welded_checked(vec![a, disconnected]).is_ok());
    }

    #[test]
    fn duplicate_faces_report_both_indices_in_any_order() {
        let a = faces()[0];
        for vertices in [a.0, [a.0[2], a.0[0], a.0[1]], [a.0[0], a.0[2], a.0[1]]] {
            assert_eq!(
                NavMesh::validate(&[a, NavTriangle(vertices)], &[]),
                Err(NavMeshDiagnostic::DuplicateTriangle {
                    triangle: 1,
                    other: 0
                })
            );
        }
    }

    #[test]
    fn nonmanifold_edge_reports_third_face_before_winding() {
        let f = vec![
            NavTriangle([Vec3::ZERO, Vec3::X, Vec3::Z]),
            NavTriangle([Vec3::X, Vec3::ZERO, Vec3::Y]),
            NavTriangle([Vec3::ZERO, Vec3::X, -Vec3::Z]),
        ];
        assert_eq!(
            NavMesh::validate(&f, &[]),
            Err(NavMeshDiagnostic::NonManifoldEdge {
                triangle: 0,
                edge: 0,
                first_neighbor: 1,
                second_neighbor: 2,
            })
        );
    }

    #[test]
    fn links_report_indices_costs_and_disabled_errors() {
        let f = faces();
        let base = walk_link();
        assert_eq!(
            NavMesh::validate(
                &f,
                &[
                    base,
                    TraversalLink {
                        from: 5,
                        enabled: false,
                        ..base
                    }
                ]
            ),
            Err(NavMeshDiagnostic::SourceOutOfBounds {
                link: 1,
                triangle: 5
            })
        );
        assert_eq!(
            NavMesh::validate(&f, &[TraversalLink { to: 6, ..base }]),
            Err(NavMeshDiagnostic::DestinationOutOfBounds {
                link: 0,
                triangle: 6
            })
        );
        assert_eq!(
            NavMesh::validate(&f, &[TraversalLink { to: 0, ..base }]),
            Err(NavMeshDiagnostic::SelfLink {
                link: 0,
                triangle: 0
            })
        );
        for cost in [-1., f32::NAN, f32::INFINITY] {
            assert_eq!(
                NavMesh::validate(&f, &[TraversalLink { cost, ..base }]),
                Err(NavMeshDiagnostic::InvalidLinkCost { link: 0 })
            );
        }
        assert!(NavMesh::new_checked(f, vec![TraversalLink { cost: 0., ..base }]).is_ok());
    }

    #[test]
    fn links_report_nonfinite_and_outside_endpoints() {
        let f = faces();
        let base = walk_link();
        for position in [
            Vec3::splat(f32::NAN),
            Vec3::splat(f32::INFINITY),
            Vec3::Y,
            Vec3::X * 2.,
        ] {
            assert_eq!(
                NavMesh::validate(
                    &f,
                    &[TraversalLink {
                        entry: position,
                        ..base
                    }]
                ),
                Err(NavMeshDiagnostic::InvalidEntry {
                    link: 0,
                    triangle: 0
                })
            );
            assert_eq!(
                NavMesh::validate(
                    &f,
                    &[TraversalLink {
                        exit: position,
                        ..base
                    }]
                ),
                Err(NavMeshDiagnostic::InvalidExit {
                    link: 0,
                    triangle: 1
                })
            );
        }
    }

    #[test]
    fn walk_portals_require_an_edge_not_a_vertex_or_overlap() {
        let a = faces()[0];
        for b in [
            NavTriangle([Vec3::ZERO, -Vec3::X, -Vec3::Z]),
            NavTriangle([Vec3::ZERO, Vec3::X * 2., Vec3::Z * 2.]),
        ] {
            let link = TraversalLink {
                entry: Vec3::ZERO,
                exit: Vec3::ZERO,
                ..walk_link()
            };
            assert!(NavMesh::new(vec![a, b], vec![link]).is_ok());
            assert_eq!(
                NavMesh::new_checked(vec![a, b], vec![link]).err(),
                Some(NavMeshDiagnostic::MissingSharedEdge {
                    link: 0,
                    from: 0,
                    to: 1
                })
            );
        }
    }

    #[test]
    fn walk_portals_must_match_and_lie_on_the_shared_segment() {
        let mut link = walk_link();
        link.exit += (Vec3::X - Vec3::Z) * 0.01;
        assert_eq!(
            NavMesh::validate(&faces(), &[link]),
            Err(NavMeshDiagnostic::PortalMismatch { link: 0 })
        );

        let mut f = faces();
        for t in &mut f {
            for v in &mut t.0 {
                *v *= 1000.;
            }
        }
        // Both faces' barycentric tolerance accepts these points, but the
        // portal's world-space tolerance excludes an interior point and an
        // extension past the endpoint of the shared segment.
        for p in [
            (Vec3::X + Vec3::Z) * 500. - Vec3::X * 0.005,
            Vec3::X * 1000. + (Vec3::X - Vec3::Z) * 0.005,
        ] {
            assert!(f[0].contains(p) && f[1].contains(p));
            link.entry = p;
            link.exit = p;
            assert_eq!(
                NavMesh::validate(&f, &[link]),
                Err(NavMeshDiagnostic::PortalOffEdge {
                    link: 0,
                    triangle: 0,
                    edge: 1
                })
            );
        }
        // Segment endpoints are valid portals, too.
        for p in [Vec3::X, Vec3::Z] {
            assert!(
                NavMesh::validate(
                    &faces(),
                    &[TraversalLink {
                        entry: p,
                        exit: p,
                        ..walk_link()
                    }]
                )
                .is_ok()
            );
        }
    }

    #[test]
    fn checked_traversal_links_still_cross_gaps_directionally() {
        let mut f = faces();
        for p in &mut f[1].0 {
            *p += Vec3::Y * 2.;
        }
        for kind in [Traversal::Jump, Traversal::Drop, Traversal::Climb] {
            let link = TraversalLink {
                entry: Vec3::X,
                exit: Vec3::X + Vec3::Y * 2.,
                kind,
                ..walk_link()
            };
            let mesh = NavMesh::new_checked(f.clone(), vec![link]).unwrap();
            let mut forward = mesh.search(Vec3::ZERO, link.exit).unwrap();
            assert!(forward.step(&mesh, &mut WorkBudget::new(0, 0, 30)).unwrap());
            assert!(
                forward
                    .route(&mesh)
                    .unwrap()
                    .iter()
                    .any(|p| p.traversal == kind)
            );
            let mut reverse = mesh.search(link.exit, Vec3::ZERO).unwrap();
            assert_eq!(
                reverse.step(&mesh, &mut WorkBudget::new(0, 0, 30)),
                Err(NavigationError::NoPath)
            );
        }
    }

    #[test]
    fn thin_large_coordinate_faces_remain_locatable() {
        let triangle = NavTriangle([
            Vec3::ZERO,
            Vec3::X * MAX_COORDINATE,
            Vec3::X * MAX_COORDINATE + Vec3::Z,
        ]);
        let interior = Vec3::X * 750_000. + Vec3::Z * 0.25;
        assert!(triangle.contains(interior));
        for p in triangle.0 {
            assert!(triangle.contains(p));
        }
        assert!(!triangle.contains(interior + Vec3::Y * 0.01));
        let mesh = NavMesh::welded_checked(vec![triangle]).unwrap();
        assert_eq!(mesh.locate(interior), Some(0));
        let mut search = mesh.search(interior, triangle.0[2]).unwrap();
        assert!(search.step(&mesh, &mut WorkBudget::new(0, 0, 1)).unwrap());
        assert_eq!(
            search.route(&mesh).unwrap().last().unwrap().position,
            triangle.0[2]
        );
    }

    #[test]
    fn contains_rejects_malformed_faces_and_points() {
        assert!(!NavTriangle([Vec3::ZERO; 3]).contains(Vec3::ZERO));
        assert!(!NavTriangle([Vec3::ZERO, Vec3::X, Vec3::splat(f32::NAN)]).contains(Vec3::ZERO));
        assert!(!faces()[0].contains(Vec3::splat(f32::NAN)));
    }

    #[test]
    fn failed_replace_preserves_state_and_completed_and_pending_searches() {
        let mut f = faces();
        let mut extra = f[0];
        for v in &mut extra.0 {
            *v += Vec3::Y * 3.;
        }
        f.push(extra);
        let mut mesh = NavMesh::welded_checked(f).unwrap();
        mesh.set_blocked(2, true).unwrap();
        mesh.set_link_enabled(1, false).unwrap();
        let identity = mesh.identity;
        let revision = mesh.revision;
        let mut done = mesh.search(Vec3::ZERO, Vec3::X + Vec3::Z).unwrap();
        assert!(done.step(&mesh, &mut WorkBudget::new(0, 0, 30)).unwrap());
        let mut pending = mesh.search(Vec3::ZERO, Vec3::X + Vec3::Z).unwrap();
        assert!(!pending.step(&mesh, &mut WorkBudget::new(0, 0, 1)).unwrap());
        assert_eq!(
            mesh.replace_checked(
                faces(),
                vec![
                    walk_link(),
                    TraversalLink {
                        to: 99,
                        ..walk_link()
                    }
                ]
            ),
            Err(NavMeshDiagnostic::DestinationOutOfBounds {
                link: 1,
                triangle: 99
            })
        );
        assert_eq!((mesh.identity, mesh.revision), (identity, revision));
        assert_eq!(mesh.triangles.len(), 3);
        assert_eq!(mesh.blocked, [false, false, true]);
        assert!(!mesh.links[1].enabled);
        assert!(done.route(&mesh).is_ok());
        assert!(pending.step(&mesh, &mut WorkBudget::new(0, 0, 30)).unwrap());
        let mut reverse = mesh.search(Vec3::X + Vec3::Z, Vec3::ZERO).unwrap();
        assert_eq!(
            reverse.step(&mesh, &mut WorkBudget::new(0, 0, 30)),
            Err(NavigationError::NoPath)
        );
    }

    #[test]
    fn successful_replace_resets_state_and_invalidates_all_old_searches() {
        let mut mesh = NavMesh::welded_checked(faces()).unwrap();
        let mut done = mesh.search(Vec3::ZERO, Vec3::X + Vec3::Z).unwrap();
        assert!(done.step(&mesh, &mut WorkBudget::new(0, 0, 30)).unwrap());
        let mut pending = mesh.search(Vec3::ZERO, Vec3::X + Vec3::Z).unwrap();
        let identity = mesh.identity;
        mesh.replace_checked(faces(), vec![walk_link()]).unwrap();
        assert_ne!(mesh.identity, identity);
        assert!(done.route(&mesh).is_err());
        assert!(pending.step(&mesh, &mut WorkBudget::new(0, 0, 30)).is_err());
        mesh.set_blocked(1, true).unwrap();
        mesh.set_link_enabled(0, false).unwrap();
        mesh.replace_checked(faces(), vec![walk_link()]).unwrap();
        assert_eq!(mesh.revision, 0);
        assert_eq!(mesh.blocked, [false, false]);
        assert!(mesh.links[0].enabled);
        assert!(!completed_route(&mesh).is_empty());
    }

    #[test]
    fn diagnostics_display_actionable_indices() {
        let diagnostic = NavMeshDiagnostic::InconsistentWinding {
            triangle: 2,
            edge: 1,
            neighbor: 5,
            neighbor_edge: 0,
        };
        let text = diagnostic.to_string();
        assert!(text.contains("triangle 2 edge 1"));
        assert!(text.contains("triangle 5 edge 0"));
        assert!(text.contains("reverse"));
        let error: &dyn std::error::Error = &diagnostic;
        assert!(error.source().is_none());
    }
}
