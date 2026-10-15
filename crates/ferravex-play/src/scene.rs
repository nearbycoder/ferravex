//! Generational scene hierarchy, exact matrix composition, prefabs, a bounded
//! binary scene format, and skeleton skin palettes. No renderer or asset loader.
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
        if let Some(old) = old {
            self.node_mut(old)?.children.retain(|child| *child != id);
        }
        if let Some(parent) = parent {
            self.node_mut(parent)?.children.push(id);
        }
        self.node_mut(id)?.parent = parent;
        Ok(())
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
    pub fn world_pose(&self, locals: &[Transform]) -> Result<Vec<Matrix>, SceneError> {
        if locals.len() != self.joints.len() {
            return Err(SceneError::InvalidSkeleton);
        }
        let mut world = Vec::with_capacity(locals.len());
        for (joint, local) in self.joints.iter().zip(locals) {
            let matrix = local.matrix()?;
            let matrix = joint
                .parent
                .map_or(matrix, |parent| multiply(world[parent], matrix));
            if !matrix.into_iter().all(f32::is_finite) {
                return Err(SceneError::InvalidTransform);
            }
            world.push(matrix);
        }
        Ok(world)
    }
    /// Model-space joint world pose multiplied by inverse bind, ready for skinning.
    pub fn skin_palette(&self, locals: &[Transform]) -> Result<Vec<Matrix>, SceneError> {
        let world = self.world_pose(locals)?;
        let palette: Vec<_> = world
            .into_iter()
            .zip(&self.joints)
            .map(|(pose, joint)| multiply(pose, joint.inverse_bind))
            .collect();
        if palette.iter().flatten().all(|v| v.is_finite()) {
            Ok(palette)
        } else {
            Err(SceneError::InvalidTransform)
        }
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
