//! Authored triangular navigation surfaces. Walk portals join exact shared edges;
//! directional traversal links remain explicit host actions. Bake clearance for
//! the agent radius before construction. This is not an automatic mesh baker.
use super::NavigationError;
use crate::ai::WorkBudget;
use ferravex_math::Vec3;
#[derive(Clone, Copy, Debug)]
pub struct NavTriangle(pub [Vec3; 3]);
impl NavTriangle {
    fn center(self) -> Vec3 {
        (self.0[0] + self.0[1] + self.0[2]) / 3.
    }
    pub fn contains(self, p: Vec3) -> bool {
        let [a, b, c] = self.0;
        let u = b - a;
        let v = c - a;
        let d = p - a;
        let n = u.cross(v);
        if !p.is_finite() || n.length_squared() < 1e-10 || d.dot(n).abs() > 0.001 * n.length() {
            return false;
        }
        let den = u.dot(u) * v.dot(v) - u.dot(v).powi(2);
        let x = (d.dot(u) * v.dot(v) - d.dot(v) * u.dot(v)) / den;
        let y = (d.dot(v) * u.dot(u) - d.dot(u) * u.dot(v)) / den;
        x >= -1e-5 && y >= -1e-5 && x + y <= 1.00001
    }
}
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
pub struct NavMesh {
    triangles: Vec<NavTriangle>,
    links: Vec<TraversalLink>,
    edges: Vec<Vec<usize>>,
    blocked: Vec<bool>,
    revision: u64,
    identity: u64,
}
impl NavMesh {
    /// Construction allocates; searches reuse their own scratch space.
    pub fn new(
        triangles: Vec<NavTriangle>,
        links: Vec<TraversalLink>,
    ) -> Result<Self, NavigationError> {
        if triangles.is_empty() || triangles.len() > 2048 || links.len() > 16384 {
            return Err(NavigationError::Capacity);
        }
        if triangles.iter().any(|t| {
            t.0.iter()
                .any(|p| !p.is_finite() || p.abs().max_element() > 1_000_000.)
                || (t.0[1] - t.0[0]).cross(t.0[2] - t.0[0]).length_squared() < 1e-6
        }) {
            return Err(NavigationError::InvalidInput);
        }
        let mut edges = vec![Vec::new(); triangles.len()];
        for (i, l) in links.iter().enumerate() {
            if l.from >= triangles.len()
                || l.to >= triangles.len()
                || !l.cost.is_finite()
                || l.cost < 0.
                || !triangles[l.from].contains(l.entry)
                || !triangles[l.to].contains(l.exit)
                || (l.kind == Traversal::Walk
                    && (l.entry.distance(l.exit) > 0.001 || !triangles[l.to].contains(l.entry)))
            {
                return Err(NavigationError::InvalidInput);
            }
            edges[l.from].push(i);
        }
        #[allow(deprecated)] // Preserve the workspace Rust 1.88 minimum.
        let identity = super::NEXT_GRID_ID
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |v| v.checked_add(1),
            )
            .map_err(|_| NavigationError::RevisionExhausted)?;
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
    pub fn welded(triangles: Vec<NavTriangle>) -> Result<Self, NavigationError> {
        if triangles.len() > 2048 {
            return Err(NavigationError::Capacity);
        }
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
                        if links.len() == 16384 {
                            return Err(NavigationError::Capacity);
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
        Self::new(triangles, links)
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
}
