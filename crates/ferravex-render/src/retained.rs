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
    pub skin: Option<Vec<[f32; 8]>>,
    pub joint_count: usize,
    opaque: bool,
    bounds: ferravex_graphics::Bounds,
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
        Self::with_skin(mesh, None)
    }
    pub(crate) fn with_skin(
        mesh: &Mesh,
        skin: Option<(&[[f32; 8]], usize)>,
    ) -> Result<Self, &'static str> {
        if skin.is_some_and(|(weights, _)| weights.len() != mesh.vertices.len()) {
            return Err("skin source vertex count mismatch");
        }
        if mesh.indices.is_empty() || !mesh.indices.len().is_multiple_of(3) {
            return Err("mesh requires complete triangles");
        }
        let stride = if skin.is_some() { 72 } else { 40 };
        if mesh.indices.len() > MAX_BYTES / stride {
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
                .map(|m| m.vertices.len() * if m.skin.is_some() { 72 } else { 40 })
                .sum();
            if r.meshes.len() >= MAX_MESHES || bytes + mesh.indices.len() * stride > MAX_BYTES {
                return Err("retained storage capacity exceeded");
            }
            r.next = r.next.checked_add(1).ok_or("mesh identity exhausted")?;
            let data = Rc::new(Geometry {
                id: r.next,
                skin: skin.map(|(weights, _)| {
                    mesh.indices.iter().map(|i| weights[*i as usize]).collect()
                }),
                joint_count: skin.map_or(0, |(_, joints)| joints),
                bounds: mesh
                    .indices
                    .iter()
                    .map(|i| mesh.vertices[*i as usize].position)
                    .fold(
                        ferravex_graphics::Bounds {
                            min: crate::Vec3::splat(f32::INFINITY),
                            max: crate::Vec3::splat(f32::NEG_INFINITY),
                        },
                        |b, p| ferravex_graphics::Bounds {
                            min: b.min.min(p),
                            max: b.max.max(p),
                        },
                    ),
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
    pub(crate) fn is_opaque(&self) -> bool {
        self.0.opaque
    }
    pub fn bounds(&self) -> ferravex_graphics::Bounds {
        self.0.bounds
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
/// ABI: kind 0 = immediate, kind 1 = retained, kind 2 = flagged triangles, kind 3 = instanced sprites. Preserves transparency/UI order.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Command {
    pub kind: u32,
    pub index: u32,
}
/// ABI: 224 bytes. Lighting: direction.xyz/enabled, eye.xyz/ambient, fog.rgba, start/end/padding.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetainedBatch {
    pub mesh: u32,
    pub first: u32,
    pub count: u32,
    /// Bit 0: depth test; bit 1: suppress depth writes; bit 2: additive blend.
    pub depth: u32,
    pub camera: [f32; 16],
    pub light: [f32; 16],
    pub params: [f32; 16],
    pub material: u32,
    pub revision: u32,
    /// Skin palette first joint / joint count; count zero disables skinning.
    pub reserved: [u32; 2],
}
#[derive(Default)]
pub(crate) struct Frame {
    pub commands: Vec<Command>,
    pub effects: Vec<crate::effects::EffectBatch>,
    pub sprite_batches: Vec<crate::effects::SpriteBatch>,
    pub sprites: Vec<[f32; 16]>,
    pub batches: Vec<RetainedBatch>,
    pub instances: Vec<[f32; 28]>,
    pub skin_matrices: Vec<[f32; 16]>,
    pub skin_normals: Vec<[f32; 9]>,
    pub environment: crate::lighting::Environment,
    scratch_instances: Vec<[f32; 28]>,
    scratch_batches: Vec<RetainedBatch>,
    keep_alive: Vec<ResidentMesh>,
    keep_materials: Vec<crate::material::ShaderMaterial>,
}
#[derive(Default, Clone, Copy)]
pub(crate) struct DrawStyle<'a> {
    pub look: crate::Look,
    pub light: Option<LitSettings>,
    pub material: Option<&'a crate::material::ShaderMaterial>,
    pub params: [f32; 16],
    pub shadow_only: bool,
    pub skip_shadow: bool,
    pub skin: Option<&'a crate::skinning::SkinPalette>,
}
impl RetainedBatch {
    fn same_pass(&self, other: &Self) -> bool {
        self.camera == other.camera
            && self.light == other.light
            && self.params == other.params
            && self.material == other.material
            && self.revision == other.revision
            && self.depth & 7 == other.depth & 7
    }
    fn compatible(&self, other: &Self) -> bool {
        self.mesh == other.mesh
            && self.depth == other.depth
            && self.reserved == other.reserved
            && self.same_pass(other)
    }
}
impl Frame {
    pub fn clear(&mut self) {
        self.commands.clear();
        self.effects.clear();
        self.sprite_batches.clear();
        self.sprites.clear();
        self.batches.clear();
        self.instances.clear();
        self.skin_matrices.clear();
        self.skin_normals.clear();
        self.keep_alive.clear();
        self.keep_materials.clear();
    }
    #[cfg(test)]
    pub fn record(
        &mut self,
        mesh: &ResidentMesh,
        model: Mat4,
        camera: Mat4,
        depth: bool,
        light: Option<LitSettings>,
    ) -> bool {
        self.record_style(
            mesh,
            model,
            camera,
            depth,
            DrawStyle {
                light,
                ..Default::default()
            },
        )
    }
    pub fn record_style(
        &mut self,
        mesh: &ResidentMesh,
        model: Mat4,
        camera: Mat4,
        depth: bool,
        style: DrawStyle<'_>,
    ) -> bool {
        let light = style.light;
        let bones = style.skin.map_or(0, |skin| skin.count);
        let skin_at = if let Some(skin) = style.skin.filter(|skin| {
            self.skin_matrices.len() >= bones
                && self.skin_matrices[self.skin_matrices.len() - bones..] == skin.matrices[..bones]
                && self.skin_normals[self.skin_normals.len() - bones..] == skin.normals[..bones]
        }) {
            self.skin_matrices.len() - skin.count
        } else {
            self.skin_matrices.len()
        };
        if bones != mesh.0.joint_count || skin_at + bones > crate::skinning::MAX_FRAME_SKIN_JOINTS {
            return false;
        }
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
            style.look as u32 as f32,
            if mesh.0.opaque
                && style
                    .material
                    .is_none_or(|m| m.0.options.blend == crate::material::BlendMode::Opaque)
            {
                1.
            } else {
                0.
            },
        ];
        let mut depth_flags = depth as u32;
        if !depth {
            depth_flags |= 16;
        }
        if let Some(m) = style.material {
            if m.0.options.blend != crate::material::BlendMode::Opaque {
                depth_flags |= 2;
            }
            if m.0.options.blend == crate::material::BlendMode::Additive {
                depth_flags |= 4;
            }
            if !m.0.options.casts_shadows {
                depth_flags |= 8;
            }
            if !m.0.options.receives_shadows {
                depth_flags |= 16;
            }
        }
        if style.skip_shadow {
            depth_flags |= 8;
        }
        if style.shadow_only {
            depth_flags |= 32;
        }
        if let Some(skin) = style.skin.filter(|_| skin_at == self.skin_matrices.len()) {
            self.skin_matrices
                .extend_from_slice(&skin.matrices[..bones]);
            self.skin_normals.extend_from_slice(&skin.normals[..bones]);
        }
        let batch = RetainedBatch {
            mesh: mesh.id(),
            first: self.instances.len() as u32,
            count: 1,
            depth: depth_flags,
            camera: camera.to_cols_array(),
            light: lighting,
            params: style.params,
            material: style.material.map_or(0, |m| m.id()),
            revision: style.material.map_or(0, |m| m.0.revision.get()),
            reserved: if bones == 0 {
                [0; 2]
            } else {
                [skin_at as u32, bones as u32]
            },
        };
        if let Some(last) = self.batches.last_mut().filter(|last| {
            self.commands.last().is_some_and(|c| c.kind == 1) && last.compatible(&batch)
        }) {
            last.count += 1;
        } else {
            self.commands.push(Command {
                kind: 1,
                index: self.batches.len() as u32,
            });
            self.batches.push(batch);
            self.keep_alive.push(mesh.clone());
            if let Some(material) = style.material {
                self.keep_materials.push(material.clone());
            }
        }
        self.instances.push(instance);
        true
    }
    /// Group opaque depth-tested draws within an uninterrupted camera/material pass.
    /// Transparent draws and every immediate command are strict ordering barriers.
    pub fn finalize(&mut self) {
        let opaque = |c: &Command| {
            c.kind == 1
                && self.batches[c.index as usize].depth & 7 == 1
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
                if !b.same_pass(&first) {
                    break;
                }
                end += 1;
            }
            self.commands[begin..end].sort_unstable_by_key(|c| {
                let b = self.batches[c.index as usize];
                (b.mesh, b.depth)
            });
            begin = end;
        }
        self.scratch_instances.clear();
        self.scratch_batches.clear();
        let mut written = 0;
        let mut previous_retained = false;
        for i in 0..self.commands.len() {
            let c = self.commands[i];
            if c.kind != 1 {
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
            if let Some(last) = self
                .scratch_batches
                .last_mut()
                .filter(|last| previous_retained && last.compatible(&batch))
            {
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
attribute vec4 a_joints; attribute vec4 a_weights;
uniform int u_skin_count;
uniform mat4 u_bones[32];
uniform mat3 u_bone_normals[32];
attribute vec4 a_model0; attribute vec4 a_model1; attribute vec4 a_model2; attribute vec4 a_model3;
attribute vec4 a_normal0; attribute vec4 a_normal1; attribute vec4 a_normal2;
uniform mat4 u_camera;
uniform vec4 u_light[4];
uniform vec4 u_params[4];
varying vec4 v_color;
varying vec3 v_world;
varying vec3 v_normal;
vec3 ferravex_vertex(vec3 position);
vec3 ferravex_normal(vec3 normal);
void main(){
 mat4 model=mat4(a_model0,a_model1,a_model2,a_model3);
 vec3 position=a_position;vec3 posed_normal=a_normal;
 if(u_skin_count>0){
  position=vec3(0.0);posed_normal=vec3(0.0);
  for(int i=0;i<4;i++){int joint=int(a_joints[i]);position+=(u_bones[joint]*vec4(a_position,1.0)).xyz*a_weights[i];posed_normal+=u_bone_normals[joint]*a_normal*a_weights[i];}
 }
 vec4 world=model*vec4(ferravex_vertex(position),1.0);
 vec3 normal=mat3(a_normal0.xyz,a_normal1.xyz,a_normal2.xyz)*ferravex_normal(posed_normal);
 float n=length(normal);normal=n>0.0?normal/n:vec3(0.0);
 gl_Position=u_camera*world;v_color=a_color;v_world=world.xyz;v_normal=normal;
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
        assert_eq!(std::mem::size_of::<RetainedBatch>(), 224);
        assert_eq!(std::mem::offset_of!(RetainedBatch, camera), 16);
        assert_eq!(std::mem::offset_of!(RetainedBatch, light), 80);
        assert_eq!(std::mem::offset_of!(RetainedBatch, params), 144);
        assert_eq!(std::mem::offset_of!(RetainedBatch, material), 208);
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
    fn material_and_parameter_changes_cannot_be_merged() {
        use crate::material::{MaterialOptions, ShaderMaterial};
        let mesh = mesh();
        let a = ShaderMaterial::new(
            Default::default(),
            MaterialOptions {
                blend: crate::material::BlendMode::Opaque,
                ..Default::default()
            },
        )
        .unwrap();
        let b = ShaderMaterial::new(
            Default::default(),
            MaterialOptions {
                blend: crate::material::BlendMode::Opaque,
                ..Default::default()
            },
        )
        .unwrap();
        let mut f = Frame::default();
        for (m, v) in [(&a, 1.), (&b, 1.), (&b, 2.)] {
            f.record_style(
                &mesh,
                Mat4::IDENTITY,
                Mat4::IDENTITY,
                true,
                DrawStyle {
                    material: Some(m),
                    params: [v; 16],
                    ..Default::default()
                },
            );
        }
        f.finalize();
        assert_eq!(f.batches.len(), 3);
        assert_eq!(
            f.batches
                .iter()
                .map(|b| (b.material, b.params[0]))
                .collect::<Vec<_>>(),
            [(a.id(), 1.), (b.id(), 1.), (b.id(), 2.)]
        );
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
