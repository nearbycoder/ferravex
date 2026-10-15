//! Retained low-poly geometry and ordered instanced draw recording.
use crate::{Mat4, Mesh, shaders::LitSettings};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

const MAX_MESHES: usize = 1024;
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_INSTANCES: usize = 65536;
#[derive(Clone)]
pub struct ResidentMesh(Rc<Geometry>);
pub(crate) struct Geometry {
    pub id: u32,
    pub vertices: Vec<[f32; 10]>,
    opaque: bool,
}
#[derive(Default)]
struct Registry {
    next: u32,
    meshes: Vec<Weak<Geometry>>,
}
thread_local! {static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default());}
impl ResidentMesh {
    /// Snapshot validated geometry. Changes to the source Mesh require a new snapshot.
    pub fn new(mesh: &Mesh) -> Result<Self, &'static str> {
        if mesh.indices.is_empty() || !mesh.indices.len().is_multiple_of(3) {
            return Err("mesh requires complete triangles");
        }
        if mesh.indices.len() > MAX_BYTES / 40 {
            return Err("mesh exceeds retained storage budget");
        }
        if mesh.indices.iter().any(|i| {
            mesh.vertices
                .get(*i as usize)
                .is_none_or(|v| !v.position.is_finite() || !v.normal.is_finite())
        }) {
            return Err("invalid retained mesh geometry");
        }
        REGISTRY.with(|registry| {
            let mut r = registry.borrow_mut();
            r.meshes.retain(|m| m.strong_count() > 0);
            let bytes: usize = r
                .meshes
                .iter()
                .filter_map(Weak::upgrade)
                .map(|m| m.vertices.len() * 40)
                .sum();
            if r.meshes.len() >= MAX_MESHES || bytes + mesh.indices.len() * 40 > MAX_BYTES {
                return Err("retained storage capacity exceeded");
            }
            r.next = r.next.checked_add(1).ok_or("mesh identity exhausted")?;
            let data = Rc::new(Geometry {
                id: r.next,
                opaque: mesh
                    .indices
                    .iter()
                    .all(|i| mesh.vertices[*i as usize].color[3] == 255),
                vertices: mesh
                    .indices
                    .iter()
                    .map(|i| {
                        let v = mesh.vertices[*i as usize];
                        [
                            v.position.x,
                            v.position.y,
                            v.position.z,
                            v.normal.x,
                            v.normal.y,
                            v.normal.z,
                            v.color[0] as f32 / 255.,
                            v.color[1] as f32 / 255.,
                            v.color[2] as f32 / 255.,
                            v.color[3] as f32 / 255.,
                        ]
                    })
                    .collect(),
            });
            r.meshes.push(Rc::downgrade(&data));
            Ok(Self(data))
        })
    }
    pub fn id(&self) -> u32 {
        self.0.id
    }
    pub fn vertex_count(&self) -> usize {
        self.0.vertices.len()
    }
}
pub(crate) fn geometry(id: u32) -> Option<Rc<Geometry>> {
    REGISTRY.with(|r| {
        r.borrow()
            .meshes
            .iter()
            .filter_map(Weak::upgrade)
            .find(|m| m.id == id)
    })
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn is_live(id: u32) -> bool {
    geometry(id).is_some()
}
/// ABI: kind 0 = immediate batch, kind 1 = retained batch. Preserves transparency/UI order.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Command {
    pub kind: u32,
    pub index: u32,
}
/// ABI: 144 bytes. Lighting: direction.xyz/enabled, eye.xyz/ambient, fog.rgba, start/end/padding.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetainedBatch {
    pub mesh: u32,
    pub first: u32,
    pub count: u32,
    pub depth: u32,
    pub camera: [f32; 16],
    pub light: [f32; 16],
}
#[derive(Default)]
pub(crate) struct Frame {
    pub commands: Vec<Command>,
    pub batches: Vec<RetainedBatch>,
    pub instances: Vec<[f32; 28]>,
    scratch_instances: Vec<[f32; 28]>,
    scratch_batches: Vec<RetainedBatch>,
    keep_alive: Vec<ResidentMesh>,
}
impl Frame {
    pub fn clear(&mut self) {
        self.commands.clear();
        self.batches.clear();
        self.instances.clear();
        self.keep_alive.clear();
    }
    pub fn record(
        &mut self,
        mesh: &ResidentMesh,
        model: Mat4,
        camera: Mat4,
        depth: bool,
        light: Option<LitSettings>,
    ) -> bool {
        if self.instances.len() >= MAX_INSTANCES || !model.is_finite() || !camera.is_finite() {
            return false;
        }
        let Some(inverse) = model.try_inverse() else {
            return false;
        };
        if model.to_cols_array()[3] != 0.
            || model.to_cols_array()[7] != 0.
            || model.to_cols_array()[11] != 0.
            || model.to_cols_array()[15] != 1.
        {
            return false;
        }
        let normal = inverse.transpose().to_cols_array();
        let mut instance = [0.; 28];
        instance[..16].copy_from_slice(&model.to_cols_array());
        for i in 0..3 {
            instance[16 + i * 4..19 + i * 4].copy_from_slice(&normal[i * 4..i * 4 + 3]);
        }
        let l = light.unwrap_or_default().sanitized();
        let lighting = [
            l.light_direction.x,
            l.light_direction.y,
            l.light_direction.z,
            if light.is_some() { 1. } else { 0. },
            l.camera_position.x,
            l.camera_position.y,
            l.camera_position.z,
            l.ambient,
            l.fog_color.r,
            l.fog_color.g,
            l.fog_color.b,
            l.fog_color.a,
            l.fog_start,
            l.fog_end,
            0.,
            if mesh.0.opaque { 1. } else { 0. },
        ];
        let batch = RetainedBatch {
            mesh: mesh.id(),
            first: self.instances.len() as u32,
            count: 1,
            depth: depth as u32,
            camera: camera.to_cols_array(),
            light: lighting,
        };
        if let Some(last) = self.batches.last_mut().filter(|last| {
            self.commands.last().is_some_and(|c| c.kind == 1)
                && last.mesh == batch.mesh
                && last.camera == batch.camera
                && last.light == batch.light
                && last.depth == batch.depth
        }) {
            last.count += 1;
        } else {
            self.commands.push(Command {
                kind: 1,
                index: self.batches.len() as u32,
            });
            self.batches.push(batch);
            self.keep_alive.push(mesh.clone());
        }
        self.instances.push(instance);
        true
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn vertices(&self) -> u64 {
        self.batches
            .iter()
            .map(|b| geometry(b.mesh).map_or(0, |m| m.vertices.len() as u64 * b.count as u64))
            .sum()
    }
    /// Group opaque depth-tested draws within an uninterrupted camera/material pass.
    /// Transparent draws and every immediate command are strict ordering barriers.
    pub fn finalize(&mut self) {
        let opaque = |c: &Command| {
            c.kind == 1
                && self.batches[c.index as usize].depth == 1
                && self.batches[c.index as usize].light[15] > 0.5
        };
        let mut begin = 0;
        while begin < self.commands.len() {
            if !opaque(&self.commands[begin]) {
                begin += 1;
                continue;
            }
            let first = self.batches[self.commands[begin].index as usize];
            let mut end = begin + 1;
            while end < self.commands.len() && opaque(&self.commands[end]) {
                let b = self.batches[self.commands[end].index as usize];
                if b.camera != first.camera || b.light != first.light {
                    break;
                }
                end += 1;
            }
            self.commands[begin..end].sort_unstable_by_key(|c| self.batches[c.index as usize].mesh);
            begin = end;
        }
        self.scratch_instances.clear();
        self.scratch_batches.clear();
        let mut written = 0;
        let mut previous_retained = false;
        for i in 0..self.commands.len() {
            let c = self.commands[i];
            if c.kind == 0 {
                self.commands[written] = c;
                written += 1;
                previous_retained = false;
                continue;
            }
            let mut batch = self.batches[c.index as usize];
            let source =
                &self.instances[batch.first as usize..(batch.first + batch.count) as usize];
            batch.first = self.scratch_instances.len() as u32;
            self.scratch_instances.extend_from_slice(source);
            if let Some(last) = self.scratch_batches.last_mut().filter(|last| {
                previous_retained
                    && last.mesh == batch.mesh
                    && last.camera == batch.camera
                    && last.light == batch.light
                    && last.depth == batch.depth
            }) {
                last.count += batch.count;
            } else {
                self.commands[written] = Command {
                    kind: 1,
                    index: self.scratch_batches.len() as u32,
                };
                written += 1;
                self.scratch_batches.push(batch);
            }
            previous_retained = true;
        }
        self.commands.truncate(written);
        std::mem::swap(&mut self.instances, &mut self.scratch_instances);
        std::mem::swap(&mut self.batches, &mut self.scratch_batches);
    }
}

/// Same vertex-lighting/fog equations as the CPU immediate path, evaluated by the GPU.
pub const VERTEX_BODY: &str = r#"
attribute vec3 a_position;
attribute vec3 a_normal;
attribute vec4 a_color;
attribute vec4 a_model0; attribute vec4 a_model1; attribute vec4 a_model2; attribute vec4 a_model3;
attribute vec4 a_normal0; attribute vec4 a_normal1; attribute vec4 a_normal2;
uniform mat4 u_camera;
uniform vec4 u_light[4];
varying vec4 v_color;
void main(){
 mat4 model=mat4(a_model0,a_model1,a_model2,a_model3);
 vec4 world=model*vec4(a_position,1.0);
 vec3 normal=mat3(a_normal0.xyz,a_normal1.xyz,a_normal2.xyz)*a_normal;
 float n=length(normal);normal=n>0.0?normal/n:vec3(0.0);
 vec4 color=a_color;
 if(u_light[0].w>0.5){
  float shade=u_light[1].w+(1.0-u_light[1].w)*clamp(dot(normal,u_light[0].xyz),0.0,1.0);
  float fog=clamp((distance(world.xyz,u_light[1].xyz)-u_light[3].x)/max(u_light[3].y-u_light[3].x,0.001),0.0,1.0);
  color.rgb=mix(color.rgb*shade,u_light[2].rgb,fog);
 }
 gl_Position=u_camera*world;v_color=color;
}
"#;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Vec3, mesh::Mesh3d};
    fn mesh() -> ResidentMesh {
        ResidentMesh::new(&Mesh3d::cuboid(Vec3::ONE, Color::new(1., 1., 1., 1.)).mesh).unwrap()
    }
    #[test]
    fn compatible_instances_merge_but_camera_and_order_are_preserved() {
        let mesh = mesh();
        let mut f = Frame::default();
        assert!(f.record(&mesh, Mat4::IDENTITY, Mat4::IDENTITY, true, None));
        f.record(
            &mesh,
            Mat4::from_translation(Vec3::X),
            Mat4::IDENTITY,
            true,
            None,
        );
        assert_eq!(f.batches.len(), 1);
        assert_eq!(f.batches[0].count, 2);
        f.commands.push(Command { kind: 0, index: 0 });
        f.record(&mesh, Mat4::IDENTITY, Mat4::IDENTITY, true, None);
        f.record(
            &mesh,
            Mat4::IDENTITY,
            Mat4::from_translation(Vec3::X),
            true,
            None,
        );
        assert_eq!(f.batches.len(), 3);
        assert_eq!(f.commands.len(), 4);
        assert!(!f.record(
            &mesh,
            Mat4::from_scale(Vec3::ZERO),
            Mat4::IDENTITY,
            true,
            None
        ));
    }
    #[test]
    fn lifetime_keeps_draws_valid_and_ids_never_recycle() {
        let mut f = Frame::default();
        let id;
        {
            let mesh = mesh();
            id = mesh.id();
            f.record(&mesh, Mat4::IDENTITY, Mat4::IDENTITY, true, None);
        }
        assert!(is_live(id));
        f.clear();
        assert!(!is_live(id));
        assert_ne!(id, mesh().id());
    }
    #[test]
    fn wire_layout_and_normal_matrix_are_defined() {
        assert_eq!(std::mem::size_of::<Command>(), 8);
        assert_eq!(std::mem::size_of::<RetainedBatch>(), 144);
        assert_eq!(std::mem::offset_of!(RetainedBatch, camera), 16);
        assert_eq!(std::mem::offset_of!(RetainedBatch, light), 80);
        let mut f = Frame::default();
        f.record(
            &mesh(),
            Mat4::from_scale(Vec3::new(2., 4., 8.)),
            Mat4::IDENTITY,
            true,
            None,
        );
        assert_eq!(f.instances[0][16], 0.5);
        assert_eq!(f.instances[0][21], 0.25);
        assert_eq!(f.instances[0][26], 0.125);
    }
    #[test]
    fn opaque_reordering_copies_instances_and_stops_at_ui_barriers() {
        let a = mesh();
        let b = mesh();
        let mut f = Frame::default();
        for (i, m) in [&a, &b, &a].into_iter().enumerate() {
            f.record(
                m,
                Mat4::from_translation(Vec3::X * i as f32),
                Mat4::IDENTITY,
                true,
                None,
            );
        }
        f.commands.push(Command { kind: 0, index: 0 });
        f.record(&a, Mat4::IDENTITY, Mat4::IDENTITY, true, None);
        f.finalize();
        assert_eq!(f.commands.len(), 4);
        assert_eq!(f.batches[0].count, 2);
        assert_eq!(
            f.instances.iter().map(|i| i[12]).collect::<Vec<_>>(),
            [0., 2., 1., 0.]
        );
    }
}
