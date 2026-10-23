//! Generational scene hierarchy, exact matrix composition, prefabs, a bounded
//! binary scene format, and skeleton skin palettes. No renderer or asset loader.
//!
//! [`Scene::resolve_path`] and [`Scene::path_from`] use literal name components
//! and reject ambiguous sibling names. Paths describe the current hierarchy;
//! retain a [`NodeId`] when identity must survive reparenting.
use std::collections::HashMap;

pub type Matrix = [f32; 16];
pub const IDENTITY: Matrix = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];
const MAX_NODES: usize = 100_000;
const MAX_NAME: usize = 4096;
const MAX_BYTES: usize = 16 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}
impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        }
    }
}
impl Transform {
    pub fn valid(self) -> bool {
        self.translation
            .into_iter()
            .chain(self.rotation)
            .chain(self.scale)
            .all(f32::is_finite)
            && self
                .rotation
                .into_iter()
                .map(|n| f64::from(n).powi(2))
                .sum::<f64>()
                > 1e-20
    }
    /// Column-major T*R*S. Quaternion input is normalized; zero scale is allowed.
    pub fn matrix(self) -> Result<Matrix, SceneError> {
        if !self.valid() {
            return Err(SceneError::InvalidTransform);
        }
        let length = self
            .rotation
            .into_iter()
            .map(|v| f64::from(v).powi(2))
            .sum::<f64>()
            .sqrt();
        let [x, y, z, w] = self.rotation.map(|v| (f64::from(v) / length) as f32);
        let [sx, sy, sz] = self.scale;
        let [tx, ty, tz] = self.translation;
        Ok([
            (1.0 - 2.0 * (y * y + z * z)) * sx,
            2.0 * (x * y + z * w) * sx,
            2.0 * (x * z - y * w) * sx,
            0.0,
            2.0 * (x * y - z * w) * sy,
            (1.0 - 2.0 * (x * x + z * z)) * sy,
            2.0 * (y * z + x * w) * sy,
            0.0,
            2.0 * (x * z + y * w) * sz,
            2.0 * (y * z - x * w) * sz,
            (1.0 - 2.0 * (x * x + y * y)) * sz,
            0.0,
            tx,
            ty,
            tz,
            1.0,
        ])
    }
}
/// Column-major multiplication. Using matrices preserves shear from nonuniform
/// parent scale followed by child rotation instead of approximating it as TRS.
pub fn multiply(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|index| {
        let row = index % 4;
        let col = index / 4;
        (0..4)
            .map(|k| f64::from(a[k * 4 + row]) * f64::from(b[col * 4 + k]))
            .sum::<f64>() as f32
    })
}
pub fn transform_point(m: Matrix, p: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|row| m[row] * p[0] + m[4 + row] * p[1] + m[8 + row] * p[2] + m[12 + row])
}
/// Generational handle scoped to the scene that created it. Handles from other
/// scenes must not be mixed: their slot indices and generations can coincide.
/// Reparenting preserves identity. Removing a subtree invalidates its handles,
/// even if later nodes reuse the same slots or names. Decoding a scene remaps
/// handles; a path is a fresh name lookup, not a persistent identity token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId {
    index: u32,
    generation: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneError {
    InvalidNode,
    InvalidTransform,
    Cycle,
    Capacity,
    NameTooLong,
    InvalidFormat,
    InvalidSkeleton,
}
impl std::fmt::Display for SceneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidNode => f.write_str(
                "node handle is stale or missing; reacquire it after removing a subtree",
            ),
            Self::InvalidTransform => f.write_str(
                "invalid transform; use finite TRS values and a nonzero quaternion, and avoid world-matrix overflow",
            ),
            Self::Cycle => f.write_str(
                "reparenting would create a cycle; a node cannot be its own ancestor",
            ),
            Self::Capacity => write!(
                f,
                "scene capacity exceeded or prefab is empty; limits are {MAX_NODES} node slots and {MAX_BYTES} encoded bytes (retired slots cannot be reused)",
            ),
            Self::NameTooLong => write!(f, "node name exceeds the {MAX_NAME}-byte UTF-8 limit"),
            Self::InvalidFormat => f.write_str(
                "invalid scene or prefab; check GSC1 version 1, lengths, UTF-8 names and acyclic parent links; prefab parents must precede children under one root",
            ),
            Self::InvalidSkeleton => f.write_str(
                "invalid skeleton or pose buffers; use 1..=4096 joints with parents before children, finite inverse-bind matrices and one pose entry per joint",
            ),
        }
    }
}
impl std::error::Error for SceneError {}

/// Checked hierarchy lookup/inspection errors, separate from [`SceneError`] so
/// existing scene mutation error matches remain compatible. Component indices
/// are zero-based, relative to the supplied parent (or the scene's roots).
///
/// Duplicate names remain legal in a scene. Path operations reject an ambiguous
/// step immediately, even if only one matching sibling has the requested suffix.
/// Use an already known [`NodeId`] as the parent to address its children instead.
///
/// ```
/// use ferravex_play::scene::{Scene, ScenePathError, Transform};
/// let mut scene = Scene::new();
/// let first = scene.create("actor", Transform::default(), None)?;
/// scene.create("actor", Transform::default(), None)?;
/// assert_eq!(scene.resolve_path(None, &["actor"]),
///     Err(ScenePathError::Ambiguous { component: 0, parent: None }));
/// assert_eq!(scene.resolve_path(Some(first), &[])?, first);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScenePathError {
    /// A supplied handle is stale or missing. Foreign-scene handles whose slot
    /// and generation coincide cannot be detected; see [`NodeId`].
    InvalidNode(NodeId),
    /// An empty absolute path cannot select a node; scenes have no implicit root.
    EmptyPath,
    /// A path exceeds the scene's maximum of 100,000 name components.
    TooDeep,
    /// A component exceeds the existing 4,096-byte UTF-8 node-name limit.
    NameTooLong { component: usize },
    /// No direct child (or root when `parent` is `None`) has this name.
    NotFound {
        component: usize,
        parent: Option<NodeId>,
    },
    /// More than one direct child (or root) has this name. Never picks the first.
    Ambiguous {
        component: usize,
        parent: Option<NodeId>,
    },
    /// Both handles are live, but `node` is neither `ancestor` nor its descendant.
    NotDescendant { node: NodeId, ancestor: NodeId },
}
impl std::fmt::Display for ScenePathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidNode(id) => write!(f, "path node handle {id:?} is stale or missing"),
            Self::EmptyPath => {
                f.write_str("an absolute scene path needs at least one name component")
            }
            Self::TooDeep => write!(f, "scene path exceeds the {MAX_NODES}-component limit"),
            Self::NameTooLong { component } => write!(
                f,
                "scene path component {component} exceeds the {MAX_NAME}-byte UTF-8 name limit"
            ),
            Self::NotFound { component, parent } => write!(
                f,
                "scene path component {component} has no matching child under {parent:?} (None means scene roots)"
            ),
            Self::Ambiguous { component, parent } => write!(
                f,
                "scene path component {component} is ambiguous under {parent:?}; duplicate sibling names require a known parent handle"
            ),
            Self::NotDescendant { node, ancestor } => write!(
                f,
                "path node {node:?} is not within the subtree of {ancestor:?}"
            ),
        }
    }
}
impl std::error::Error for ScenePathError {}

#[derive(Clone, Debug)]
pub struct Node {
    name: String,
    local: Transform,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
}
impl Node {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn local(&self) -> Transform {
        self.local
    }
    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }
    pub fn children(&self) -> &[NodeId] {
        &self.children
    }
}
struct Slot {
    generation: u32,
    node: Option<Node>,
}
#[derive(Default)]
pub struct Scene {
    slots: Vec<Slot>,
    free: Vec<u32>,
    len: usize,
}
#[derive(Clone, Debug)]
pub struct Prefab {
    nodes: Vec<PrefabNode>,
}
#[derive(Clone, Debug)]
struct PrefabNode {
    name: String,
    local: Transform,
    parent: Option<usize>,
}
impl Prefab {
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}
impl Scene {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn node(&self, id: NodeId) -> Result<&Node, SceneError> {
        self.slots
            .get(id.index as usize)
            .filter(|s| s.generation == id.generation)
            .and_then(|s| s.node.as_ref())
            .ok_or(SceneError::InvalidNode)
    }
    fn node_mut(&mut self, id: NodeId) -> Result<&mut Node, SceneError> {
        self.slots
            .get_mut(id.index as usize)
            .filter(|s| s.generation == id.generation)
            .and_then(|s| s.node.as_mut())
            .ok_or(SceneError::InvalidNode)
    }
    pub fn nodes(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            slot.node.as_ref().map(|node| {
                (
                    NodeId {
                        index: index as u32,
                        generation: slot.generation,
                    },
                    node,
                )
            })
        })
    }
    /// Resolves literal name components one hierarchy level at a time.
    /// `None` starts among scene roots; `Some(parent)` starts among that node's
    /// direct children. An empty relative path selects its live parent; an empty
    /// absolute path returns [`ScenePathError::EmptyPath`].
    ///
    /// Names are case-sensitive UTF-8, with no normalization, splitting, escaping,
    /// or special `.`/`..` semantics. For example, `["a/b", ""]` selects the
    /// empty-named child of root `a/b`. This is the scene hierarchy, not the
    /// authoring system's expanded identifier namespace.
    ///
    /// Duplicate roots/siblings are allowed by mutations but always produce
    /// [`ScenePathError::Ambiguous`] at the first ambiguous component. A suffix
    /// never disambiguates a prefix. Missing names return `NotFound`, including
    /// the component index and its resolved parent. The supplied parent is
    /// validated first, followed by the entire path's bounds (100,000 components,
    /// 4,096 bytes per name), before any names are resolved.
    ///
    /// No allocation or recursion. Uses existing child links and [`Self::nodes`]
    /// for roots; work is bounded by scene slots/visited sibling groups and input
    /// names. The result is a current [`NodeId`], not a persistent path binding:
    /// removing it and recreating its names can make the same path select a new
    /// identity. Stale parent handles are rejected even for empty paths.
    ///
    /// ```
    /// use ferravex_play::scene::{Scene, ScenePathError, Transform};
    /// let mut scene = Scene::new();
    /// let actor = scene.create("actor", Transform::default(), None)?;
    /// let socket = scene.create("hand/socket", Transform::default(), Some(actor))?;
    /// assert_eq!(scene.resolve_path(None, &["actor", "hand/socket"])?, socket);
    /// assert_eq!(scene.resolve_path(Some(actor), &["hand/socket"])?, socket);
    /// scene.remove_subtree(actor)?;
    /// assert_eq!(scene.resolve_path(Some(actor), &[]),
    ///     Err(ScenePathError::InvalidNode(actor)));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn resolve_path(
        &self,
        parent: Option<NodeId>,
        components: &[&str],
    ) -> Result<NodeId, ScenePathError> {
        if let Some(parent) = parent {
            self.node(parent)
                .map_err(|_| ScenePathError::InvalidNode(parent))?;
        }
        if components.len() > MAX_NODES {
            return Err(ScenePathError::TooDeep);
        }
        for (component, name) in components.iter().enumerate() {
            if name.len() > MAX_NAME {
                return Err(ScenePathError::NameTooLong { component });
            }
        }
        let mut cursor = parent;
        for (component, &name) in components.iter().enumerate() {
            let id = if let Some(parent) = cursor {
                let node = self
                    .node(parent)
                    .map_err(|_| ScenePathError::InvalidNode(parent))?;
                unique_path_match(
                    node.children().iter().map(|&id| {
                        (
                            id,
                            self.node(id)
                                .expect("scene child links reference live nodes"),
                        )
                    }),
                    name,
                    component,
                    cursor,
                )?
            } else {
                unique_path_match(
                    self.nodes().filter(|(_, node)| node.parent().is_none()),
                    name,
                    component,
                    None,
                )?
            };
            cursor = Some(id);
        }
        cursor.ok_or(ScenePathError::EmptyPath)
    }
    /// Inspects `id` as an unambiguous path from `parent`, excluding the parent's
    /// own name. `None` includes the root's name; `Some(id)` returns an empty path.
    /// The parent is checked first, then `id`. Both must be live, and `id` must
    /// lie in the parent's subtree, or [`ScenePathError::NotDescendant`] is returned.
    ///
    /// Walks existing parent links iteratively, then uses [`Self::resolve_path`]
    /// to check the same duplicate-name policy. On success, resolving the returned
    /// components from the same parent returns exactly `id`. Duplicate ancestors
    /// above or equal to an explicit parent do not affect relative paths.
    ///
    /// Allocates only a vector of borrowed names, bounded by scene depth (at most
    /// 100,000 entries); names are not copied. The borrow prevents scene edits
    /// while the path is in use. Owned copies of paths can change meaning after
    /// edits; keep the [`NodeId`] to follow the same live node across reparenting.
    /// Handles are scene-local and must not be reused after decoding a new scene.
    ///
    /// ```
    /// use ferravex_play::scene::{Scene, Transform};
    /// let mut scene = Scene::new();
    /// let actor = scene.create("actor", Transform::default(), None)?;
    /// let socket = scene.create("socket", Transform::default(), Some(actor))?;
    /// let path = scene.path_from(None, socket)?;
    /// assert_eq!(path, ["actor", "socket"]);
    /// assert_eq!(scene.resolve_path(None, &path)?, socket);
    /// assert_eq!(scene.path_from(Some(actor), socket)?, ["socket"]);
    /// scene.set_parents(&[(actor, Some(socket)), (socket, None)])?;
    /// assert_eq!(scene.path_from(None, actor)?, ["socket", "actor"]);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn path_from(
        &self,
        parent: Option<NodeId>,
        id: NodeId,
    ) -> Result<Vec<&str>, ScenePathError> {
        if let Some(parent) = parent {
            self.node(parent)
                .map_err(|_| ScenePathError::InvalidNode(parent))?;
        }
        self.node(id).map_err(|_| ScenePathError::InvalidNode(id))?;
        let mut components = Vec::new();
        let mut cursor = Some(id);
        while let Some(current) = cursor {
            if cursor == parent {
                break;
            }
            let node = self
                .node(current)
                .map_err(|_| ScenePathError::InvalidNode(current))?;
            components.push(node.name());
            cursor = node.parent();
        }
        if cursor != parent {
            return Err(ScenePathError::NotDescendant {
                node: id,
                ancestor: parent.expect("only a specified parent can be outside the ancestry"),
            });
        }
        components.reverse();
        self.resolve_path(parent, &components)?;
        Ok(components)
    }
    /// Visits a root and its descendants in depth-first, parent-before-child
    /// order, respecting each node's child order. A stale root is rejected before
    /// traversal starts. The iterator borrows the scene, preventing edits while
    /// it is in use, and uses a heap stack rather than recursion.
    ///
    /// Collect IDs first when applying edits to the visited nodes:
    ///
    /// ```
    /// use ferravex_play::scene::{Scene, Transform};
    /// let mut scene = Scene::new();
    /// let actor = scene.create("actor", Transform::default(), None)?;
    /// let socket = scene.create("socket", Transform::default(), Some(actor))?;
    /// let ids: Vec<_> = scene.subtree(actor)?.map(|(id, _)| id).collect();
    /// assert_eq!(ids, [actor, socket]);
    /// for id in ids {
    ///     scene.set_local(id, Transform::default())?;
    /// }
    /// # Ok::<(), ferravex_play::scene::SceneError>(())
    /// ```
    pub fn subtree(
        &self,
        root: NodeId,
    ) -> Result<impl Iterator<Item = (NodeId, &Node)> + '_, SceneError> {
        self.node(root)?;
        let mut pending = vec![root];
        Ok(std::iter::from_fn(move || {
            let id = pending.pop()?;
            let node = self
                .node(id)
                .expect("scene child links reference live nodes");
            pending.extend(node.children.iter().rev().copied());
            Some((id, node))
        }))
    }
    pub fn create(
        &mut self,
        name: impl Into<String>,
        local: Transform,
        parent: Option<NodeId>,
    ) -> Result<NodeId, SceneError> {
        let name = name.into();
        if name.len() > MAX_NAME {
            return Err(SceneError::NameTooLong);
        }
        if !local.valid() {
            return Err(SceneError::InvalidTransform);
        }
        if self.len == MAX_NODES || (self.free.is_empty() && self.slots.len() == MAX_NODES) {
            return Err(SceneError::Capacity);
        }
        if let Some(parent) = parent {
            self.node(parent)?;
        }
        let index = if let Some(index) = self.free.pop() {
            index
        } else {
            self.slots.push(Slot {
                generation: 0,
                node: None,
            });
            (self.slots.len() - 1) as u32
        };
        let slot = &mut self.slots[index as usize];
        let id = NodeId {
            index,
            generation: slot.generation,
        };
        slot.node = Some(Node {
            name,
            local,
            parent,
            children: Vec::new(),
        });
        self.len += 1;
        if let Some(parent) = parent {
            self.node_mut(parent)?.children.push(id);
        }
        Ok(id)
    }
    pub fn set_local(&mut self, id: NodeId, local: Transform) -> Result<(), SceneError> {
        if !local.valid() {
            return Err(SceneError::InvalidTransform);
        }
        self.node_mut(id)?.local = local;
        Ok(())
    }
    /// Reparent while retaining local TRS. Cycles and invalid handles are rejected
    /// before any links change. World-space position may change.
    pub fn set_parent(&mut self, id: NodeId, parent: Option<NodeId>) -> Result<(), SceneError> {
        let old = self.node(id)?.parent;
        let mut ancestor = parent;
        while let Some(candidate) = ancestor {
            if candidate == id {
                return Err(SceneError::Cycle);
            }
            ancestor = self.node(candidate)?.parent;
        }
        if old == parent {
            return Ok(());
        }
        self.relink(id, parent);
        Ok(())
    }
    /// Applies reparenting edits atomically, retaining every local transform.
    /// Validates the *final* hierarchy, so edits can succeed even when applying
    /// them individually in the given order would temporarily create a cycle.
    /// Any stale handle or final cycle leaves the entire scene unchanged.
    ///
    /// Repeated node IDs use the last requested parent; all supplied handles are
    /// checked, including overwritten requests. Moved children are appended in
    /// order of their first appearance. Unchanged links retain their order.
    /// Work and scratch space are linear in scene slots plus requested edits;
    /// traversal does not recurse, even for deeply nested hierarchies.
    ///
    /// ```
    /// use ferravex_play::scene::{Scene, Transform};
    /// let mut scene = Scene::new();
    /// let carrier = scene.create("carrier", Transform::default(), None)?;
    /// let payload = scene.create("payload", Transform::default(), Some(carrier))?;
    /// // Make payload the root and attach carrier beneath it in one edit.
    /// scene.set_parents(&[(carrier, Some(payload)), (payload, None)])?;
    /// assert_eq!(scene.node(carrier)?.parent(), Some(payload));
    /// assert_eq!(scene.node(payload)?.parent(), None);
    /// # Ok::<(), ferravex_play::scene::SceneError>(())
    /// ```
    pub fn set_parents(&mut self, changes: &[(NodeId, Option<NodeId>)]) -> Result<(), SceneError> {
        if changes.is_empty() {
            return Ok(());
        }
        for &(id, parent) in changes {
            self.node(id)?;
            if let Some(parent) = parent {
                self.node(parent)?;
            }
        }
        let mut parents: Vec<_> = self
            .slots
            .iter()
            .map(|slot| slot.node.as_ref().and_then(|node| node.parent))
            .collect();
        for &(id, parent) in changes {
            parents[id.index as usize] = parent;
        }
        // Every new cycle must contain an edited node. Each ancestor is visited
        // at most once across all edits, using the final parent map.
        let mut marks = vec![0_u8; self.slots.len()];
        let mut chain = Vec::new();
        for &(id, _) in changes {
            let mut cursor = Some(id);
            while let Some(id) = cursor {
                let index = id.index as usize;
                if marks[index] != 0 {
                    break;
                }
                marks[index] = 1;
                chain.push(index);
                cursor = parents[index];
            }
            if cursor.is_some_and(|id| marks[id.index as usize] == 1) {
                return Err(SceneError::Cycle);
            }
            for index in chain.drain(..) {
                marks[index] = 2;
            }
        }
        // Detach in one pass; retaining a wide parent's child list separately
        // for each moved child would make a large batch quadratic.
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if let Some(node) = &mut slot.node {
                let id = NodeId {
                    index: index as u32,
                    generation: slot.generation,
                };
                node.children
                    .retain(|child| parents[child.index as usize] == Some(id));
            }
        }
        for &(id, _) in changes {
            let parent = parents[id.index as usize];
            let node = self.node_mut(id).expect("validated node");
            if node.parent == parent {
                continue;
            }
            node.parent = parent;
            if let Some(parent) = parent {
                self.node_mut(parent)
                    .expect("validated parent")
                    .children
                    .push(id);
            }
        }
        Ok(())
    }
    // Call only after validating all handles and the resulting parent graph.
    // No fallible operations remain once link mutation begins.
    fn relink(&mut self, id: NodeId, parent: Option<NodeId>) {
        let old = self.node(id).expect("validated node").parent;
        if old == parent {
            return;
        }
        if let Some(old) = old {
            self.node_mut(old)
                .expect("scene parent links reference live nodes")
                .children
                .retain(|child| *child != id);
        }
        if let Some(parent) = parent {
            self.node_mut(parent)
                .expect("validated parent")
                .children
                .push(id);
        }
        self.node_mut(id).expect("validated node").parent = parent;
    }
    pub fn world_matrix(&self, id: NodeId) -> Result<Matrix, SceneError> {
        let node = self.node(id)?;
        let mut matrix = node.local.matrix()?;
        let mut parent = node.parent;
        while let Some(id) = parent {
            let node = self.node(id)?;
            matrix = multiply(node.local.matrix()?, matrix);
            parent = node.parent;
        }
        if matrix.into_iter().all(f32::is_finite) {
            Ok(matrix)
        } else {
            Err(SceneError::InvalidTransform)
        }
    }
    pub fn remove_subtree(&mut self, root: NodeId) -> Result<usize, SceneError> {
        let parent = self.node(root)?.parent;
        let mut ids = vec![root];
        let mut index = 0;
        while index < ids.len() {
            ids.extend_from_slice(self.node(ids[index])?.children());
            index += 1;
        }
        if let Some(parent) = parent {
            self.node_mut(parent)?
                .children
                .retain(|child| *child != root);
        }
        for id in ids.iter().rev() {
            let slot = &mut self.slots[id.index as usize];
            slot.node = None;
            if let Some(next) = slot.generation.checked_add(1) {
                slot.generation = next;
                self.free.push(id.index);
            }
        }
        self.len -= ids.len();
        Ok(ids.len())
    }
    /// Captures local transforms and hierarchy; external parent links are omitted.
    pub fn prefab(&self, root: NodeId) -> Result<Prefab, SceneError> {
        self.node(root)?;
        let mut pending = vec![(root, None)];
        let mut nodes = Vec::new();
        let mut cursor = 0;
        while cursor < pending.len() {
            let (id, parent) = pending[cursor];
            let node = self.node(id)?;
            nodes.push(PrefabNode {
                name: node.name.clone(),
                local: node.local,
                parent,
            });
            pending.extend(node.children.iter().map(|child| (*child, Some(cursor))));
            cursor += 1;
        }
        Ok(Prefab { nodes })
    }
    /// Inserts an independent copy under `parent`, retaining prefab-local TRS.
    /// All entries, links and capacity are checked before insertion; any returned
    /// error leaves existing nodes, child order and handle allocation unchanged.
    ///
    /// ```
    /// use ferravex_play::scene::{Scene, Transform};
    /// let mut scene = Scene::new();
    /// let source = scene.create("enemy", Transform::default(), None)?;
    /// let prefab = scene.prefab(source)?;
    /// let copy = scene.instantiate(&prefab, None)?;
    /// assert_ne!(source, copy);
    /// assert_eq!(scene.node(copy)?.name(), "enemy");
    /// # Ok::<(), ferravex_play::scene::SceneError>(())
    /// ```
    pub fn instantiate(
        &mut self,
        prefab: &Prefab,
        parent: Option<NodeId>,
    ) -> Result<NodeId, SceneError> {
        if let Some(parent) = parent {
            self.node(parent)?;
        }
        let available = MAX_NODES - self.slots.len() + self.free.len();
        if prefab.nodes.is_empty() || prefab.nodes.len() > available {
            return Err(SceneError::Capacity);
        }
        // Preflight the whole prefab: a later invalid transform/name must not
        // leave earlier nodes inserted, and invalid indices must never panic.
        for (index, node) in prefab.nodes.iter().enumerate() {
            if node.name.len() > MAX_NAME {
                return Err(SceneError::NameTooLong);
            }
            if !node.local.valid() {
                return Err(SceneError::InvalidTransform);
            }
            match (index, node.parent) {
                (0, None) => {}
                (_, Some(parent)) if parent < index => {}
                _ => return Err(SceneError::InvalidFormat),
            }
        }
        let mut ids = Vec::with_capacity(prefab.nodes.len());
        for node in &prefab.nodes {
            let node_parent = node.parent.map(|index| ids[index]).or(parent);
            ids.push(self.create(node.name.clone(), node.local, node_parent)?);
        }
        Ok(ids[0])
    }
    pub fn clone_subtree(
        &mut self,
        root: NodeId,
        parent: Option<NodeId>,
    ) -> Result<NodeId, SceneError> {
        self.instantiate(&self.prefab(root)?, parent)
    }
    /// GSC1 format: magic, u16 version, u32 count; each node has u32 parent
    /// (MAX=root), u16 UTF-8 name length, name bytes, and 10 little-endian f32 TRS
    /// values. Handles are remapped; serialized data never trusts slot generations.
    pub fn encode(&self) -> Result<Vec<u8>, SceneError> {
        let nodes: Vec<_> = self.nodes().collect();
        let indices: HashMap<_, _> = nodes
            .iter()
            .enumerate()
            .map(|(i, (id, _))| (*id, i as u32))
            .collect();
        let length = 10
            + nodes
                .iter()
                .map(|(_, node)| 46 + node.name.len())
                .sum::<usize>();
        if length > MAX_BYTES {
            return Err(SceneError::Capacity);
        }
        let mut bytes = Vec::with_capacity(length);
        bytes.extend_from_slice(b"GSC1");
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&(nodes.len() as u32).to_le_bytes());
        for (_, node) in nodes {
            bytes.extend_from_slice(
                &node
                    .parent
                    .map_or(u32::MAX, |parent| indices[&parent])
                    .to_le_bytes(),
            );
            bytes.extend_from_slice(&(node.name.len() as u16).to_le_bytes());
            bytes.extend_from_slice(node.name.as_bytes());
            for value in node
                .local
                .translation
                .into_iter()
                .chain(node.local.rotation)
                .chain(node.local.scale)
            {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, SceneError> {
        if bytes.len() > MAX_BYTES {
            return Err(SceneError::InvalidFormat);
        }
        let mut reader = Reader { bytes, cursor: 0 };
        if reader.take(4)? != b"GSC1" || reader.u16()? != 1 {
            return Err(SceneError::InvalidFormat);
        }
        let count = reader.u32()? as usize;
        if count > MAX_NODES || count > bytes.len().saturating_sub(10) / 46 {
            return Err(SceneError::InvalidFormat);
        }
        let mut scene = Self::new();
        let mut ids = Vec::with_capacity(count);
        let mut parents = Vec::with_capacity(count);
        for _ in 0..count {
            let parent = reader.u32()?;
            if parent != u32::MAX && parent as usize >= count {
                return Err(SceneError::InvalidFormat);
            }
            let len = reader.u16()? as usize;
            if len > MAX_NAME {
                return Err(SceneError::InvalidFormat);
            }
            let name =
                std::str::from_utf8(reader.take(len)?).map_err(|_| SceneError::InvalidFormat)?;
            let mut values = [0.0; 10];
            for value in &mut values {
                *value = reader.f32()?;
            }
            let transform = Transform {
                translation: [values[0], values[1], values[2]],
                rotation: [values[3], values[4], values[5], values[6]],
                scale: [values[7], values[8], values[9]],
            };
            ids.push(
                scene
                    .create(name, transform, None)
                    .map_err(|_| SceneError::InvalidFormat)?,
            );
            parents.push(parent);
        }
        if reader.cursor != bytes.len() {
            return Err(SceneError::InvalidFormat);
        }
        // Validate the parent graph in linear time before linking. Repeatedly
        // calling set_parent for a long serialized chain would be quadratic.
        let mut marks = vec![0_u8; count];
        let mut chain = Vec::new();
        for start in 0..count {
            if marks[start] != 0 {
                continue;
            }
            let mut cursor = Some(start);
            while let Some(index) = cursor {
                if marks[index] != 0 {
                    break;
                }
                marks[index] = 1;
                chain.push(index);
                cursor = (parents[index] != u32::MAX).then_some(parents[index] as usize);
            }
            if cursor.is_some_and(|index| marks[index] == 1) {
                return Err(SceneError::InvalidFormat);
            }
            for index in chain.drain(..) {
                marks[index] = 2;
            }
        }
        for (index, parent) in parents.into_iter().enumerate() {
            if parent != u32::MAX {
                let child_id = ids[index];
                let parent_id = ids[parent as usize];
                scene.node_mut(child_id)?.parent = Some(parent_id);
                scene.node_mut(parent_id)?.children.push(child_id);
            }
        }
        Ok(scene)
    }
}
fn unique_path_match<'a>(
    nodes: impl Iterator<Item = (NodeId, &'a Node)>,
    name: &str,
    component: usize,
    parent: Option<NodeId>,
) -> Result<NodeId, ScenePathError> {
    let mut matches = nodes
        .filter(|(_, node)| node.name() == name)
        .map(|(id, _)| id);
    let id = matches
        .next()
        .ok_or(ScenePathError::NotFound { component, parent })?;
    if matches.next().is_some() {
        return Err(ScenePathError::Ambiguous { component, parent });
    }
    Ok(id)
}
struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], SceneError> {
        let end = self
            .cursor
            .checked_add(count)
            .ok_or(SceneError::InvalidFormat)?;
        let bytes = self
            .bytes
            .get(self.cursor..end)
            .ok_or(SceneError::InvalidFormat)?;
        self.cursor = end;
        Ok(bytes)
    }
    fn u16(&mut self) -> Result<u16, SceneError> {
        Ok(u16::from_le_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| SceneError::InvalidFormat)?,
        ))
    }
    fn u32(&mut self) -> Result<u32, SceneError> {
        Ok(u32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| SceneError::InvalidFormat)?,
        ))
    }
    fn f32(&mut self) -> Result<f32, SceneError> {
        Ok(f32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| SceneError::InvalidFormat)?,
        ))
    }
}

#[derive(Clone, Debug)]
pub struct Joint {
    pub parent: Option<usize>,
    pub inverse_bind: Matrix,
}
#[derive(Clone, Debug)]
pub struct Skeleton {
    joints: Vec<Joint>,
}
impl Skeleton {
    /// Joints must be parent-before-child; this also proves acyclic hierarchy.
    pub fn new(joints: Vec<Joint>) -> Result<Self, SceneError> {
        if joints.is_empty()
            || joints.len() > 4096
            || joints.iter().enumerate().any(|(i, joint)| {
                joint.parent.is_some_and(|parent| parent >= i)
                    || !joint.inverse_bind.into_iter().all(f32::is_finite)
            })
        {
            return Err(SceneError::InvalidSkeleton);
        }
        Ok(Self { joints })
    }
    pub fn joint_count(&self) -> usize {
        self.joints.len()
    }
    /// Borrow the validated parent order and inverse-bind data without cloning.
    /// Joint names and local rest poses remain caller-owned authoring metadata.
    pub fn joints(&self) -> &[Joint] {
        &self.joints
    }
    pub fn world_pose(&self, locals: &[Transform]) -> Result<Vec<Matrix>, SceneError> {
        let mut world = vec![IDENTITY; self.joints.len()];
        self.world_pose_into(locals, &mut world)?;
        Ok(world)
    }
    /// Allocation-free pose evaluation into caller-owned scratch. On numeric
    /// overflow output may be partially written; use it only after Ok(()).
    pub fn world_pose_into(
        &self,
        locals: &[Transform],
        world: &mut [Matrix],
    ) -> Result<(), SceneError> {
        if locals.len() != self.joints.len() || world.len() != locals.len() {
            return Err(SceneError::InvalidSkeleton);
        }
        if !locals.iter().all(|local| local.valid()) {
            return Err(SceneError::InvalidTransform);
        }
        for (i, (joint, local)) in self.joints.iter().zip(locals).enumerate() {
            let matrix = local.matrix()?;
            let matrix = joint
                .parent
                .map_or(matrix, |parent| multiply(world[parent], matrix));
            if !matrix.into_iter().all(f32::is_finite) {
                return Err(SceneError::InvalidTransform);
            }
            world[i] = matrix;
        }
        Ok(())
    }
    /// Model-space joint world pose multiplied by inverse bind, ready for skinning.
    pub fn skin_palette(&self, locals: &[Transform]) -> Result<Vec<Matrix>, SceneError> {
        let mut world = vec![IDENTITY; self.joints.len()];
        let mut palette = vec![IDENTITY; self.joints.len()];
        self.skin_palette_into(locals, &mut world, &mut palette)?;
        Ok(palette)
    }
    /// Model-space world * inverse-bind using preallocated buffers. Both outputs
    /// are scratch on error. No heap allocation or hierarchy traversal recursion.
    pub fn skin_palette_into(
        &self,
        locals: &[Transform],
        world: &mut [Matrix],
        palette: &mut [Matrix],
    ) -> Result<(), SceneError> {
        if palette.len() != self.joints.len() {
            return Err(SceneError::InvalidSkeleton);
        }
        self.world_pose_into(locals, world)?;
        for ((out, pose), joint) in palette.iter_mut().zip(world).zip(&self.joints) {
            let matrix = multiply(*pose, joint.inverse_bind);
            if !matrix.into_iter().all(f32::is_finite) {
                return Err(SceneError::InvalidTransform);
            }
            *out = matrix;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn translated(x: f32, y: f32, z: f32) -> Transform {
        Transform {
            translation: [x, y, z],
            ..Transform::default()
        }
    }
    #[test]
    fn hierarchy_composes_parent_rotation_and_child_translation() {
        let mut scene = Scene::new();
        let root = scene
            .create(
                "root",
                Transform {
                    rotation: [
                        0.0,
                        0.0,
                        std::f32::consts::FRAC_1_SQRT_2,
                        std::f32::consts::FRAC_1_SQRT_2,
                    ],
                    ..translated(2.0, 0.0, 0.0)
                },
                None,
            )
            .unwrap();
        let child = scene
            .create("child", translated(3.0, 0.0, 0.0), Some(root))
            .unwrap();
        let p = transform_point(scene.world_matrix(child).unwrap(), [0.0; 3]);
        assert!((p[0] - 2.0).abs() < 1e-5);
        assert!((p[1] - 3.0).abs() < 1e-5);
        assert_eq!(scene.set_parent(root, Some(child)), Err(SceneError::Cycle));
        assert_eq!(scene.node(root).unwrap().parent(), None);
    }
    #[test]
    fn prefab_copy_is_independent_and_stale_ids_stay_invalid() {
        let mut scene = Scene::new();
        let root = scene.create("root", Transform::default(), None).unwrap();
        let child = scene
            .create("child", translated(2.0, 0.0, 0.0), Some(root))
            .unwrap();
        let copy = scene.clone_subtree(root, None).unwrap();
        assert_eq!(scene.len(), 4);
        let copied_child = scene.node(copy).unwrap().children()[0];
        scene.set_local(child, translated(5.0, 0.0, 0.0)).unwrap();
        assert_eq!(
            scene.node(copied_child).unwrap().local().translation[0],
            2.0
        );
        assert_eq!(scene.remove_subtree(root).unwrap(), 2);
        scene.create("reuse", Transform::default(), None).unwrap();
        assert!(matches!(scene.node(root), Err(SceneError::InvalidNode)));
        assert!(matches!(scene.node(child), Err(SceneError::InvalidNode)));
    }
    #[test]
    fn subtree_preserves_sibling_order_and_rejects_reused_handles() {
        let mut scene = Scene::new();
        let root = scene.create("root", Transform::default(), None).unwrap();
        let left = scene
            .create("left", Transform::default(), Some(root))
            .unwrap();
        let right = scene
            .create("right", Transform::default(), Some(root))
            .unwrap();
        let leaf = scene
            .create("leaf", Transform::default(), Some(left))
            .unwrap();
        let ids: Vec<_> = scene.subtree(root).unwrap().map(|(id, _)| id).collect();
        assert_eq!(ids, [root, left, leaf, right]);
        scene.remove_subtree(left).unwrap();
        let replacement = scene
            .create("replacement", Transform::default(), Some(root))
            .unwrap();
        assert_eq!(replacement.index, left.index);
        assert_ne!(replacement, left);
        assert_eq!(scene.subtree(left).err(), Some(SceneError::InvalidNode));
        let names: Vec<_> = scene
            .subtree(root)
            .unwrap()
            .map(|(_, node)| node.name())
            .collect();
        assert_eq!(names, ["root", "right", "replacement"]);
    }
    #[test]
    fn paths_use_literal_names_and_roundtrip_every_level() {
        let mut scene = Scene::new();
        let root = scene.create("a/b", Transform::default(), None).unwrap();
        let names = ["", ".", "..", "父", "\\", "\0", "é", "e\u{301}"];
        let mut leaf = root;
        for name in names {
            leaf = scene
                .create(name, Transform::default(), Some(leaf))
                .unwrap();
        }
        let before = scene.encode().unwrap();
        for (id, _) in scene.subtree(root).unwrap() {
            let absolute = scene.path_from(None, id).unwrap();
            assert_eq!(scene.resolve_path(None, &absolute), Ok(id));
            let relative = scene.path_from(Some(root), id).unwrap();
            assert_eq!(scene.resolve_path(Some(root), &relative), Ok(id));
        }
        assert_eq!(scene.path_from(Some(root), leaf).unwrap(), names);
        assert_eq!(scene.resolve_path(Some(root), &names), Ok(leaf));
        assert_eq!(scene.resolve_path(Some(root), &[]), Ok(root));
        assert!(scene.path_from(Some(leaf), leaf).unwrap().is_empty());
        assert_eq!(scene.encode().unwrap(), before);

        let a = scene.create("a", Transform::default(), None).unwrap();
        let b = scene.create("b", Transform::default(), Some(a)).unwrap();
        assert_eq!(scene.resolve_path(None, &["a/b"]), Ok(root));
        assert_eq!(scene.resolve_path(None, &["a", "b"]), Ok(b));
        assert_eq!(
            scene.resolve_path(None, &["A"]),
            Err(ScenePathError::NotFound {
                component: 0,
                parent: None
            })
        );
        let composed = scene.create("é", Transform::default(), None).unwrap();
        let decomposed = scene
            .create("e\u{301}", Transform::default(), None)
            .unwrap();
        assert_eq!(scene.resolve_path(None, &["é"]), Ok(composed));
        assert_eq!(scene.resolve_path(None, &["e\u{301}"]), Ok(decomposed));
        let unnamed = scene.create("", Transform::default(), None).unwrap();
        assert_eq!(scene.resolve_path(None, &[""]), Ok(unnamed));
        assert_eq!(scene.path_from(None, unnamed).unwrap(), [""]);
    }
    #[test]
    fn paths_reject_duplicate_prefixes_without_searching_the_suffix() {
        let mut scene = Scene::new();
        let first = scene.create("actor", Transform::default(), None).unwrap();
        let leaf = scene
            .create("socket", Transform::default(), Some(first))
            .unwrap();
        let second = scene.create("actor", Transform::default(), None).unwrap();
        let root_error = ScenePathError::Ambiguous {
            component: 0,
            parent: None,
        };
        for path in [&["actor"][..], &["actor", "socket"], &["actor", "missing"]] {
            assert_eq!(scene.resolve_path(None, path), Err(root_error));
        }
        assert_eq!(scene.path_from(None, first), Err(root_error));
        assert_eq!(scene.path_from(None, leaf), Err(root_error));
        assert_eq!(scene.path_from(None, second), Err(root_error));
        assert_eq!(scene.resolve_path(Some(first), &["socket"]), Ok(leaf));
        assert_eq!(scene.path_from(Some(first), leaf).unwrap(), ["socket"]);

        scene.remove_subtree(second).unwrap();
        scene
            .create("unrelated", Transform::default(), Some(first))
            .unwrap();
        let duplicate = scene
            .create("socket", Transform::default(), Some(first))
            .unwrap();
        let child_error = ScenePathError::Ambiguous {
            component: 1,
            parent: Some(first),
        };
        assert_eq!(
            scene.resolve_path(None, &["actor", "socket"]),
            Err(child_error)
        );
        assert_eq!(scene.path_from(None, leaf), Err(child_error));
        assert_eq!(scene.path_from(None, duplicate), Err(child_error));
        assert_eq!(
            scene.resolve_path(Some(first), &["socket"]),
            Err(ScenePathError::Ambiguous {
                component: 0,
                parent: Some(first)
            })
        );
        // An explicit handle still selects a duplicate-named node unambiguously.
        assert_eq!(scene.resolve_path(Some(duplicate), &[]), Ok(duplicate));
    }
    #[test]
    fn missing_paths_report_the_component_and_resolved_parent() {
        let mut scene = Scene::new();
        let first = scene.create("first", Transform::default(), None).unwrap();
        let second = scene.create("second", Transform::default(), None).unwrap();
        let child = scene
            .create("child", Transform::default(), Some(second))
            .unwrap();
        for (path, component, parent) in [
            (vec!["child"], 0, None),
            (vec!["first", "child"], 1, Some(first)),
            (vec!["second", "child", "missing"], 2, Some(child)),
        ] {
            assert_eq!(
                scene.resolve_path(None, &path),
                Err(ScenePathError::NotFound { component, parent })
            );
        }
        // A matching name under an unrelated parent does not create ambiguity.
        let other = scene
            .create("child", Transform::default(), Some(first))
            .unwrap();
        assert_eq!(scene.resolve_path(Some(first), &["child"]), Ok(other));
        assert_eq!(scene.resolve_path(Some(second), &["child"]), Ok(child));
        assert_eq!(scene.path_from(Some(first), other).unwrap(), ["child"]);
    }
    #[test]
    fn path_bounds_are_checked_before_resolving_names() {
        let mut scene = Scene::new();
        assert_eq!(
            scene.resolve_path(None, &[]),
            Err(ScenePathError::EmptyPath)
        );
        let root = scene.create("root", Transform::default(), None).unwrap();
        let max_name = "é".repeat(MAX_NAME / 2);
        let child = scene
            .create(&max_name, Transform::default(), Some(root))
            .unwrap();
        assert_eq!(scene.resolve_path(Some(root), &[&max_name]), Ok(child));
        let long_name = format!("{max_name}x");
        assert_eq!(
            scene.resolve_path(None, &["missing", &long_name]),
            Err(ScenePathError::NameTooLong { component: 1 })
        );
        scene.create("root", Transform::default(), None).unwrap();
        assert_eq!(
            scene.resolve_path(None, &["root", &long_name]),
            Err(ScenePathError::NameTooLong { component: 1 })
        );
        let too_deep = vec![""; MAX_NODES + 1];
        assert_eq!(
            scene.resolve_path(None, &too_deep),
            Err(ScenePathError::TooDeep)
        );
        assert_eq!(
            scene.resolve_path(Some(root), &too_deep),
            Err(ScenePathError::TooDeep)
        );
        assert!(matches!(
            scene.resolve_path(None, &too_deep[..MAX_NODES]),
            Err(ScenePathError::NotFound { component: 0, .. })
        ));
        scene.remove_subtree(root).unwrap();
        for components in [&[][..], &[long_name.as_str()], too_deep.as_slice()] {
            assert_eq!(
                scene.resolve_path(Some(root), components),
                Err(ScenePathError::InvalidNode(root))
            );
        }
    }
    #[test]
    fn paths_reject_stale_handles_even_when_slots_and_names_are_reused() {
        let mut scene = Scene::new();
        let root = scene.create("root", Transform::default(), None).unwrap();
        let child = scene
            .create("child", Transform::default(), Some(root))
            .unwrap();
        scene.remove_subtree(root).unwrap();
        let replacement = scene.create("root", Transform::default(), None).unwrap();
        let replacement_child = scene
            .create("child", Transform::default(), Some(replacement))
            .unwrap();
        assert_eq!(root.index, replacement.index);
        assert_eq!(child.index, replacement_child.index);
        assert_ne!(root, replacement);
        assert_ne!(child, replacement_child);
        for stale in [root, child] {
            assert_eq!(
                scene.path_from(None, stale),
                Err(ScenePathError::InvalidNode(stale))
            );
            assert_eq!(
                scene.path_from(Some(stale), stale),
                Err(ScenePathError::InvalidNode(stale))
            );
            assert_eq!(
                scene.path_from(Some(stale), replacement_child),
                Err(ScenePathError::InvalidNode(stale))
            );
            assert_eq!(
                scene.path_from(Some(replacement), stale),
                Err(ScenePathError::InvalidNode(stale))
            );
            assert_eq!(
                scene.resolve_path(Some(stale), &["child"]),
                Err(ScenePathError::InvalidNode(stale))
            );
        }
        assert_eq!(
            scene.resolve_path(None, &["root", "child"]),
            Ok(replacement_child)
        );
    }
    #[test]
    fn path_inspection_rejects_live_nodes_outside_the_requested_subtree() {
        let mut scene = Scene::new();
        let root = scene.create("root", Transform::default(), None).unwrap();
        let child = scene
            .create("child", Transform::default(), Some(root))
            .unwrap();
        let other = scene.create("other", Transform::default(), None).unwrap();
        for (ancestor, node) in [(root, other), (other, child), (child, root)] {
            assert_eq!(
                scene.path_from(Some(ancestor), node),
                Err(ScenePathError::NotDescendant { node, ancestor })
            );
        }
        assert_eq!(scene.path_from(None, child).unwrap(), ["root", "child"]);
    }
    #[test]
    fn paths_follow_successful_batches_and_failed_batches_preserve_identity() {
        let mut scene = Scene::new();
        let root = scene
            .create("root", translated(1.0, 0.0, 0.0), None)
            .unwrap();
        let child = scene
            .create("child", translated(2.0, 0.0, 0.0), Some(root))
            .unwrap();
        let stale = scene.create("stale", Transform::default(), None).unwrap();
        scene.remove_subtree(stale).unwrap();
        scene
            .set_parents(&[(root, Some(child)), (child, None)])
            .unwrap();
        assert_eq!(scene.resolve_path(None, &["child", "root"]), Ok(root));
        assert_eq!(scene.path_from(None, root).unwrap(), ["child", "root"]);
        assert_eq!(scene.node(root).unwrap().local(), translated(1.0, 0.0, 0.0));
        let before = scene.encode().unwrap();
        let free = scene.free.clone();
        for (changes, error) in [
            (vec![(root, None), (child, Some(child))], SceneError::Cycle),
            (
                vec![(root, None), (child, Some(stale)), (child, None)],
                SceneError::InvalidNode,
            ),
        ] {
            assert_eq!(scene.set_parents(&changes), Err(error));
            assert_eq!(scene.encode().unwrap(), before);
            assert_eq!(scene.free, free);
            assert_eq!(scene.node(child).unwrap().children(), [root]);
            assert_eq!(scene.resolve_path(None, &["child", "root"]), Ok(root));
            assert_eq!(scene.path_from(None, root).unwrap(), ["child", "root"]);
        }
    }
    #[test]
    fn duplicate_names_remain_legal_in_batches_and_prefabs() {
        let mut scene = Scene::new();
        let root = scene.create("root", Transform::default(), None).unwrap();
        let child = scene
            .create("node", Transform::default(), Some(root))
            .unwrap();
        let other = scene.create("node", Transform::default(), None).unwrap();
        scene.set_parents(&[(other, Some(root))]).unwrap();
        assert_eq!(scene.node(root).unwrap().children(), [child, other]);
        assert!(matches!(
            scene.resolve_path(None, &["root", "node"]),
            Err(ScenePathError::Ambiguous { component: 1, .. })
        ));
        scene.set_parents(&[(other, None)]).unwrap();
        assert_eq!(scene.resolve_path(None, &["root", "node"]), Ok(child));
        let prefab = scene.prefab(root).unwrap();
        let copy = scene.instantiate(&prefab, None).unwrap();
        assert!(matches!(
            scene.path_from(None, child),
            Err(ScenePathError::Ambiguous { component: 0, .. })
        ));
        let copied_child = scene.resolve_path(Some(copy), &["node"]).unwrap();
        assert_ne!(child, copied_child);
        assert_eq!(scene.path_from(Some(copy), copied_child).unwrap(), ["node"]);
        scene.remove_subtree(root).unwrap();
        assert_eq!(
            scene.resolve_path(None, &["root", "node"]),
            Ok(copied_child)
        );
    }
    #[test]
    fn paths_remain_compatible_with_the_existing_scene_wire_format() {
        let mut scene = Scene::new();
        let root = scene.create("父/a", Transform::default(), None).unwrap();
        scene.create("", Transform::default(), Some(root)).unwrap();
        scene
            .create("duplicate", Transform::default(), None)
            .unwrap();
        scene
            .create("duplicate", Transform::default(), None)
            .unwrap();
        let bytes = scene.encode().unwrap();
        assert_eq!(&bytes[..6], b"GSC1\x01\x00");
        let decoded = Scene::decode(&bytes).unwrap();
        assert_eq!(decoded.encode().unwrap(), bytes);
        let id = decoded.resolve_path(None, &["父/a", ""]).unwrap();
        assert_eq!(decoded.path_from(None, id).unwrap(), ["父/a", ""]);
        assert!(matches!(
            decoded.resolve_path(None, &["duplicate"]),
            Err(ScenePathError::Ambiguous { component: 0, .. })
        ));
    }
    #[test]
    fn deep_paths_are_iterative_and_wide_siblings_are_checked() {
        const DEPTH: usize = 20_000;
        let mut scene = Scene::new();
        let root = scene.create("n", Transform::default(), None).unwrap();
        let mut leaf = root;
        for _ in 1..DEPTH {
            leaf = scene.create("n", Transform::default(), Some(leaf)).unwrap();
        }
        let path = scene.path_from(None, leaf).unwrap();
        assert_eq!(path.len(), DEPTH);
        assert_eq!(scene.resolve_path(None, &path), Ok(leaf));
        assert_eq!(scene.path_from(Some(root), leaf).unwrap().len(), DEPTH - 1);
        let target = scene
            .create("target", Transform::default(), Some(leaf))
            .unwrap();
        for _ in 0..1024 {
            scene
                .create("other", Transform::default(), Some(leaf))
                .unwrap();
        }
        assert_eq!(scene.resolve_path(Some(leaf), &["target"]), Ok(target));
        scene
            .create("target", Transform::default(), Some(leaf))
            .unwrap();
        assert_eq!(
            scene.resolve_path(Some(leaf), &["target"]),
            Err(ScenePathError::Ambiguous {
                component: 0,
                parent: Some(leaf)
            })
        );
    }
    #[test]
    fn path_errors_implement_standard_error_with_actionable_context() {
        let id = NodeId {
            index: 1,
            generation: 2,
        };
        for (error, expected) in [
            (ScenePathError::InvalidNode(id), "stale"),
            (ScenePathError::EmptyPath, "at least one"),
            (ScenePathError::TooDeep, "100000-component"),
            (ScenePathError::NameTooLong { component: 3 }, "4096-byte"),
            (
                ScenePathError::NotFound {
                    component: 3,
                    parent: Some(id),
                },
                "component 3",
            ),
            (
                ScenePathError::Ambiguous {
                    component: 3,
                    parent: None,
                },
                "duplicate sibling",
            ),
            (
                ScenePathError::NotDescendant {
                    node: id,
                    ancestor: id,
                },
                "not within",
            ),
        ] {
            let standard_error: &dyn std::error::Error = &error;
            assert!(standard_error.to_string().contains(expected));
        }
    }
    #[test]
    fn batch_reparent_validates_final_graph_and_retains_local_transforms() {
        let mut scene = Scene::new();
        let carrier = scene
            .create("carrier", translated(1.0, 0.0, 0.0), None)
            .unwrap();
        let payload = scene
            .create("payload", translated(2.0, 0.0, 0.0), Some(carrier))
            .unwrap();
        let leaf = scene
            .create("leaf", translated(3.0, 0.0, 0.0), Some(payload))
            .unwrap();
        assert_eq!(
            scene.set_parent(carrier, Some(payload)),
            Err(SceneError::Cycle)
        );
        scene
            .set_parents(&[(carrier, Some(payload)), (payload, None)])
            .unwrap();
        assert_eq!(scene.node(carrier).unwrap().parent(), Some(payload));
        assert!(scene.node(carrier).unwrap().children().is_empty());
        assert_eq!(scene.node(payload).unwrap().parent(), None);
        assert_eq!(scene.node(payload).unwrap().children(), [leaf, carrier]);
        assert_eq!(
            scene.node(carrier).unwrap().local(),
            translated(1.0, 0.0, 0.0)
        );
        assert_eq!(
            scene.node(payload).unwrap().local(),
            translated(2.0, 0.0, 0.0)
        );
        assert_eq!(scene.world_matrix(leaf).unwrap()[12], 5.0);
    }
    #[test]
    fn failed_reparent_edits_leave_links_order_and_allocation_unchanged() {
        let mut scene = Scene::new();
        let root = scene.create("root", Transform::default(), None).unwrap();
        let child = scene
            .create("child", Transform::default(), Some(root))
            .unwrap();
        let leaf = scene
            .create("leaf", Transform::default(), Some(child))
            .unwrap();
        let stale = scene.create("old", Transform::default(), None).unwrap();
        scene.remove_subtree(stale).unwrap();
        let replacement = scene.create("new", Transform::default(), None).unwrap();
        assert_eq!(stale.index, replacement.index);
        let before = scene.encode().unwrap();
        let free = scene.free.clone();
        for (changes, error) in [
            (
                vec![(child, None), (stale, Some(root))],
                SceneError::InvalidNode,
            ),
            (
                vec![(child, None), (leaf, Some(stale))],
                SceneError::InvalidNode,
            ),
            (
                vec![(child, Some(stale)), (child, None)],
                SceneError::InvalidNode,
            ),
            (
                vec![(replacement, Some(root)), (root, Some(leaf))],
                SceneError::Cycle,
            ),
            (vec![(child, None), (root, Some(root))], SceneError::Cycle),
        ] {
            assert_eq!(scene.set_parents(&changes), Err(error));
            assert_eq!(scene.encode().unwrap(), before);
            assert_eq!(scene.node(root).unwrap().children(), [child]);
            assert_eq!(scene.node(child).unwrap().children(), [leaf]);
            assert_eq!(scene.len(), 4);
            assert_eq!(scene.free, free);
        }
        for (id, parent, error) in [
            (child, Some(stale), SceneError::InvalidNode),
            (stale, Some(root), SceneError::InvalidNode),
            (root, Some(leaf), SceneError::Cycle),
        ] {
            assert_eq!(scene.set_parent(id, parent), Err(error));
            assert_eq!(scene.encode().unwrap(), before);
        }
    }
    #[test]
    fn batch_reparent_duplicate_requests_use_final_parent_and_stable_order() {
        let mut scene = Scene::new();
        let root = scene.create("root", Transform::default(), None).unwrap();
        let a = scene.create("a", Transform::default(), Some(root)).unwrap();
        let b = scene.create("b", Transform::default(), Some(root)).unwrap();
        let c = scene.create("c", Transform::default(), None).unwrap();
        let d = scene.create("d", Transform::default(), None).unwrap();
        scene
            .set_parents(&[
                (a, Some(a)), // Overridden before graph validation.
                (c, None),
                (d, Some(root)),
                (a, Some(root)),
                (c, Some(root)),
            ])
            .unwrap();
        assert_eq!(scene.node(root).unwrap().children(), [a, b, c, d]);
        let before = scene.encode().unwrap();
        scene.set_parents(&[]).unwrap();
        scene.set_parent(a, Some(root)).unwrap();
        assert_eq!(scene.encode().unwrap(), before);
        assert_eq!(scene.node(root).unwrap().children(), [a, b, c, d]);
    }
    #[test]
    fn batch_reparent_moves_many_siblings_without_losing_unchanged_links() {
        let mut scene = Scene::new();
        let from = scene.create("from", Transform::default(), None).unwrap();
        let to = scene.create("to", Transform::default(), None).unwrap();
        let existing = scene
            .create("existing", Transform::default(), Some(to))
            .unwrap();
        let children: Vec<_> = (0..128)
            .map(|_| {
                scene
                    .create("child", Transform::default(), Some(from))
                    .unwrap()
            })
            .collect();
        let changes: Vec<_> = children
            .iter()
            .enumerate()
            .filter_map(|(index, &id)| match index % 3 {
                0 => Some((id, Some(to))),
                1 => Some((id, None)),
                _ => None,
            })
            .collect();
        scene.set_parents(&changes).unwrap();
        let kept: Vec<_> = children.iter().copied().skip(2).step_by(3).collect();
        let moved: Vec<_> = std::iter::once(existing)
            .chain(children.iter().copied().step_by(3))
            .collect();
        assert_eq!(scene.node(from).unwrap().children(), kept);
        assert_eq!(scene.node(to).unwrap().children(), moved);
        for (id, parent) in changes {
            assert_eq!(scene.node(id).unwrap().parent(), parent);
        }
        // Successful edits remain consumable by the unchanged binary format.
        let bytes = scene.encode().unwrap();
        assert_eq!(Scene::decode(&bytes).unwrap().encode().unwrap(), bytes);
    }
    #[test]
    fn prefab_preflight_rejects_bad_entries_without_consuming_handles() {
        let mut source = Scene::new();
        let root = source.create("root", Transform::default(), None).unwrap();
        source
            .create("child", Transform::default(), Some(root))
            .unwrap();
        let prefab = source.prefab(root).unwrap();
        let mut invalid = Vec::new();
        let mut bad = prefab.clone();
        bad.nodes[1].local.translation[0] = f32::NAN;
        invalid.push((bad, SceneError::InvalidTransform));
        let mut bad = prefab.clone();
        bad.nodes[1].name = "x".repeat(MAX_NAME + 1);
        invalid.push((bad, SceneError::NameTooLong));
        for parent in [None, Some(1), Some(usize::MAX)] {
            let mut bad = prefab.clone();
            bad.nodes[1].parent = parent;
            invalid.push((bad, SceneError::InvalidFormat));
        }
        let mut bad = prefab.clone();
        bad.nodes[0].parent = Some(0);
        invalid.push((bad, SceneError::InvalidFormat));
        invalid.push((Prefab { nodes: Vec::new() }, SceneError::Capacity));

        let mut scene = Scene::new();
        let anchor = scene.create("anchor", Transform::default(), None).unwrap();
        let stale = scene
            .create("old", Transform::default(), Some(anchor))
            .unwrap();
        scene.remove_subtree(stale).unwrap();
        let before = scene.encode().unwrap();
        let free = scene.free.clone();
        let generations: Vec<_> = scene.slots.iter().map(|slot| slot.generation).collect();
        for (prefab, error) in invalid {
            assert_eq!(scene.instantiate(&prefab, Some(anchor)), Err(error));
            assert_eq!(scene.encode().unwrap(), before);
            assert_eq!(scene.free, free);
            assert_eq!(scene.len(), 1);
            assert_eq!(
                scene
                    .slots
                    .iter()
                    .map(|slot| slot.generation)
                    .collect::<Vec<_>>(),
                generations
            );
            assert!(scene.node(anchor).unwrap().children().is_empty());
        }
        assert_eq!(
            scene.instantiate(&prefab, Some(stale)),
            Err(SceneError::InvalidNode)
        );
        assert_eq!(scene.encode().unwrap(), before);
        let copy = scene.instantiate(&prefab, Some(anchor)).unwrap();
        assert_eq!(copy.index, stale.index);
        assert_ne!(copy, stale);
        assert_eq!(scene.node(anchor).unwrap().children(), [copy]);
        assert_eq!(scene.subtree(copy).unwrap().count(), 2);
    }
    #[test]
    fn prefab_capacity_accounts_for_retired_slots_before_insertion() {
        let mut scene = Scene::new();
        let root = scene.create("root", Transform::default(), None).unwrap();
        let single = scene.prefab(root).unwrap();
        let mut pair = single.clone();
        pair.nodes.push(PrefabNode {
            name: "child".into(),
            local: Transform::default(),
            parent: Some(0),
        });
        // Model generation-exhausted slots without billions of remove/create calls.
        scene.slots.extend((1..MAX_NODES).map(|_| Slot {
            generation: u32::MAX,
            node: None,
        }));
        scene.slots[1].generation = 7;
        scene.free.push(1);
        assert_eq!(
            scene.instantiate(&pair, Some(root)),
            Err(SceneError::Capacity)
        );
        assert_eq!(scene.len(), 1);
        assert_eq!(scene.free, [1]);
        assert!(scene.node(root).unwrap().children().is_empty());
        let copy = scene.instantiate(&single, Some(root)).unwrap();
        assert_eq!(
            copy,
            NodeId {
                index: 1,
                generation: 7
            }
        );
        assert_eq!(scene.len(), 2);
    }
    #[test]
    fn deep_hierarchy_traversal_prefabs_and_reparenting_do_not_recurse() {
        const DEPTH: usize = 20_000;
        let mut scene = Scene::new();
        let root = scene.create("", translated(1.0, 0.0, 0.0), None).unwrap();
        let mut leaf = root;
        for _ in 1..DEPTH {
            leaf = scene
                .create("", translated(1.0, 0.0, 0.0), Some(leaf))
                .unwrap();
        }
        assert_eq!(scene.subtree(root).unwrap().count(), DEPTH);
        assert_eq!(scene.subtree(root).unwrap().last().unwrap().0, leaf);
        assert_eq!(scene.world_matrix(leaf).unwrap()[12], DEPTH as f32);
        assert_eq!(
            scene.set_parents(&[(root, Some(leaf))]),
            Err(SceneError::Cycle)
        );
        let prefab = scene.prefab(root).unwrap();
        let copy = scene.instantiate(&prefab, None).unwrap();
        assert_eq!(scene.subtree(copy).unwrap().count(), DEPTH);
        assert_eq!(scene.remove_subtree(root).unwrap(), DEPTH);
        assert_eq!(scene.subtree(leaf).err(), Some(SceneError::InvalidNode));
        let bytes = scene.encode().unwrap();
        assert_eq!(Scene::decode(&bytes).unwrap().encode().unwrap(), bytes);
        assert_eq!(scene.remove_subtree(copy).unwrap(), DEPTH);
        assert!(scene.is_empty());
    }
    #[test]
    fn scene_errors_explain_limits_and_implement_standard_error() {
        let error: &dyn std::error::Error = &SceneError::Cycle;
        assert!(error.to_string().contains("own ancestor"));
        assert!(SceneError::InvalidNode.to_string().contains("stale"));
        assert!(SceneError::NameTooLong.to_string().contains("4096-byte"));
        assert!(SceneError::Capacity.to_string().contains("100000"));
        assert!(
            SceneError::InvalidFormat
                .to_string()
                .contains("GSC1 version 1")
        );
    }
    #[test]
    fn scene_binary_roundtrip_and_strict_malformed_rejection() {
        let mut scene = Scene::new();
        let root = scene.create("父", translated(1.0, 2.0, 3.0), None).unwrap();
        scene
            .create("child", translated(4.0, 0.0, 0.0), Some(root))
            .unwrap();
        let bytes = scene.encode().unwrap();
        let decoded = Scene::decode(&bytes).unwrap();
        assert_eq!(decoded.encode().unwrap(), bytes);
        for cut in 0..bytes.len() {
            assert!(Scene::decode(&bytes[..cut]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(Scene::decode(&trailing).is_err());
        let mut cycle = bytes.clone();
        cycle[10..14].copy_from_slice(&1_u32.to_le_bytes());
        assert!(Scene::decode(&cycle).is_err());
        let mut oversized = bytes.clone();
        oversized[6..10].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Scene::decode(&oversized).is_err());
    }
    #[test]
    fn skeleton_bind_pose_palette_is_identity_and_animation_moves_child() {
        let inverse = translated(-2.0, 0.0, 0.0).matrix().unwrap();
        let skeleton = Skeleton::new(vec![
            Joint {
                parent: None,
                inverse_bind: IDENTITY,
            },
            Joint {
                parent: Some(0),
                inverse_bind: inverse,
            },
        ])
        .unwrap();
        let palette = skeleton
            .skin_palette(&[Transform::default(), translated(2.0, 0.0, 0.0)])
            .unwrap();
        assert_eq!(palette, vec![IDENTITY, IDENTITY]);
        let pose = skeleton
            .world_pose(&[translated(1.0, 0.0, 0.0), translated(3.0, 0.0, 0.0)])
            .unwrap();
        assert_eq!(transform_point(pose[1], [0.0; 3]), [4.0, 0.0, 0.0]);
        assert!(
            Skeleton::new(vec![Joint {
                parent: Some(0),
                inverse_bind: IDENTITY
            }])
            .is_err()
        );
        assert!(skeleton.skin_palette(&[]).is_err());
    }
}
