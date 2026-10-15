use crate::{Bounds, Mat4, Mesh, MeshError};

/// Stable CPU mesh handle; generation prevents reuse of a released slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MeshId {
    pub index: u32,
    pub generation: u64,
}
struct Entry {
    mesh: Mesh,
    bounds: Bounds,
}
struct Slot {
    generation: u64,
    entry: Option<Entry>,
}
#[derive(Default)]
pub struct MeshCache {
    slots: Vec<Slot>,
    free: Vec<u32>,
    live: usize,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub meshes: usize,
    pub allocated_slots: usize,
    pub vertices: usize,
    pub indices: usize,
}
impl MeshCache {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&mut self, mesh: Mesh) -> Result<MeshId, MeshError> {
        let bounds = mesh.validate()?;
        let entry = Some(Entry { mesh, bounds });
        let id = if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.entry = entry;
            MeshId {
                index,
                generation: slot.generation,
            }
        } else {
            let index = u32::try_from(self.slots.len()).map_err(|_| MeshError::TooLarge)?;
            self.slots.push(Slot {
                generation: 1,
                entry,
            });
            MeshId {
                index,
                generation: 1,
            }
        };
        self.live += 1;
        Ok(id)
    }
    pub fn get(&self, id: MeshId) -> Option<&Mesh> {
        self.entry(id).map(|entry| &entry.mesh)
    }
    pub fn bounds(&self, id: MeshId) -> Option<Bounds> {
        self.entry(id).map(|entry| entry.bounds)
    }
    fn entry(&self, id: MeshId) -> Option<&Entry> {
        let slot = self.slots.get(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        slot.entry.as_ref()
    }
    /// Replaces geometry in-place after complete validation. Old data survives a bad update.
    pub fn update(&mut self, id: MeshId, mesh: Mesh) -> Result<bool, MeshError> {
        let bounds = mesh.validate()?;
        let Some(slot) = self.slots.get_mut(id.index as usize) else {
            return Ok(false);
        };
        if slot.generation != id.generation || slot.entry.is_none() {
            return Ok(false);
        }
        slot.entry = Some(Entry { mesh, bounds });
        Ok(true)
    }
    pub fn remove(&mut self, id: MeshId) -> Option<Mesh> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        let entry = slot.entry.take()?;
        self.live -= 1;
        // Retire the slot rather than wrapping a generation and reviving stale handles.
        if let Some(next) = slot.generation.checked_add(1) {
            slot.generation = next;
            self.free.push(id.index);
        }
        Some(entry.mesh)
    }
    pub fn stats(&self) -> CacheStats {
        let mut result = CacheStats {
            meshes: self.live,
            allocated_slots: self.slots.len(),
            ..Default::default()
        };
        for entry in self.slots.iter().filter_map(|slot| slot.entry.as_ref()) {
            result.vertices += entry.mesh.vertices.len();
            result.indices += entry.mesh.indices.len();
        }
        result
    }
}
#[derive(Clone, Copy, Debug)]
pub struct RenderItem {
    pub mesh: MeshId,
    pub transform: Mat4,
}
impl RenderItem {
    pub const fn new(mesh: MeshId, transform: Mat4) -> Self {
        Self { mesh, transform }
    }
}
#[derive(Debug)]
pub struct InstanceBatch {
    pub mesh: MeshId,
    pub transforms: Vec<Mat4>,
}
/// Groups identical mesh handles. This is CPU submission grouping, not a GPU draw call.
pub fn instance_batches(items: &[RenderItem]) -> Vec<InstanceBatch> {
    let mut sorted = items.to_vec();
    sorted.sort_by_key(|item| item.mesh);
    let mut batches: Vec<InstanceBatch> = Vec::new();
    for item in sorted {
        if let Some(last) = batches.last_mut().filter(|batch| batch.mesh == item.mesh) {
            last.transforms.push(item.transform);
        } else {
            batches.push(InstanceBatch {
                mesh: item.mesh,
                transforms: vec![item.transform],
            });
        }
    }
    batches
}
