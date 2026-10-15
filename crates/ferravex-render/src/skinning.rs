//! Four-influence linear blend skinning for retained low-poly meshes. Immutable
//! geometry/weights upload once; only bounded pose palettes change each frame.
use crate::{Mat4, Mesh, Vec3, retained::ResidentMesh};
use ferravex_graphics::Bounds;
/// Fits minimum WebGL2 vertex-uniform limits, including normal palettes and
/// existing material/lighting uniforms. Large rigs must be split at authoring.
pub const MAX_SKIN_JOINTS: usize = 32;
pub const MAX_FRAME_SKIN_JOINTS: usize = 32768;
pub const MAX_SKIN_VERTICES: usize = 65536;
#[derive(Clone, Copy, Debug)]
pub struct SkinWeights {
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}
impl SkinWeights {
    pub fn rigid(joint: u16) -> Self {
        Self {
            joints: [joint; 4],
            weights: [1., 0., 0., 0.],
        }
    }
    fn packed(self, count: usize) -> Result<[f32; 8], &'static str> {
        if self.joints.iter().any(|i| *i as usize >= count)
            || self.weights.iter().any(|v| !v.is_finite() || *v < 0.)
        {
            return Err("invalid skin weights/joints");
        }
        let sum = self.weights.iter().map(|v| f64::from(*v)).sum::<f64>();
        if sum <= 0. {
            return Err("skin weights sum to zero");
        }
        Ok(std::array::from_fn(|i| {
            if i < 4 {
                self.joints[i] as f32
            } else {
                (f64::from(self.weights[i - 4]) / sum) as f32
            }
        }))
    }
}
#[derive(Clone)]
pub struct SkinnedMesh {
    mesh: ResidentMesh,
    joints: usize,
    bounds: [Option<Bounds>; MAX_SKIN_JOINTS],
}
impl SkinnedMesh {
    /// One influence record per source vertex; indices and all influences are
    /// validated before registration. Weights normalize once, using f64 sums.
    pub fn new(mesh: &Mesh, weights: &[SkinWeights], joints: usize) -> Result<Self, &'static str> {
        if joints == 0
            || joints > MAX_SKIN_JOINTS
            || weights.len() != mesh.vertices.len()
            || mesh.vertices.len() > MAX_SKIN_VERTICES
        {
            return Err("invalid skin joint/vertex count");
        }
        let packed: Vec<_> = weights
            .iter()
            .map(|w| w.packed(joints))
            .collect::<Result<_, _>>()?;
        let mut bounds: [Option<Bounds>; MAX_SKIN_JOINTS] = [None; MAX_SKIN_JOINTS];
        for index in &mesh.indices {
            let i = *index as usize;
            let p = mesh
                .vertices
                .get(i)
                .ok_or("skin index out of bounds")?
                .position;
            for j in 0..4 {
                if packed[i][4 + j] > 0. {
                    let at = packed[i][j] as usize;
                    bounds[at] = Some(bounds[at].map_or(Bounds { min: p, max: p }, |b| Bounds {
                        min: b.min.min(p),
                        max: b.max.max(p),
                    }));
                }
            }
        }
        let resident = ResidentMesh::with_skin(mesh, Some((&packed, joints)))?;
        Ok(Self {
            mesh: resident,
            joints,
            bounds,
        })
    }
    pub fn resident(&self) -> &ResidentMesh {
        &self.mesh
    }
    pub fn joint_count(&self) -> usize {
        self.joints
    }
    /// Conservative local-space deformed bounds: a union of influenced bone
    /// boxes encloses every convexly weighted vertex. No per-vertex CPU skinning.
    pub fn bounds(&self, palette: &SkinPalette) -> Result<Bounds, &'static str> {
        if self.joints != palette.count {
            return Err("skin palette joint count mismatch");
        }
        let mut bound = Bounds {
            min: Vec3::splat(f32::INFINITY),
            max: Vec3::splat(f32::NEG_INFINITY),
        };
        for (i, b) in self.bounds.iter().enumerate().take(self.joints) {
            if let Some(b) = b {
                let m = Mat4::from_cols_array(&palette.matrices[i]);
                for x in [b.min.x, b.max.x] {
                    for y in [b.min.y, b.max.y] {
                        for z in [b.min.z, b.max.z] {
                            let p = m.transform_point3(Vec3::new(x, y, z));
                            if !p.is_finite() {
                                return Err("skin bounds overflow");
                            }
                            bound.min = bound.min.min(p);
                            bound.max = bound.max.max(p);
                        }
                    }
                }
            }
        }
        // Cover rounding in normalized f32 weights and GPU multiply/add order.
        let padding = bound.min.abs().max(bound.max.abs()).max_element() * 0.00001 + 0.00001;
        bound.min -= Vec3::splat(padding);
        bound.max += Vec3::splat(padding);
        if !bound.min.is_finite() || !bound.max.is_finite() {
            return Err("skin bounds overflow");
        }
        Ok(bound)
    }
}
/// Validated position + inverse-transpose normal palettes. Fixed storage avoids
/// per-frame allocation; set() is transactional. Singular/non-affine transforms
/// are rejected rather than sending undefined normals to the GPU.
#[derive(Clone)]
pub struct SkinPalette {
    pub(crate) count: usize,
    pub(crate) matrices: [[f32; 16]; MAX_SKIN_JOINTS],
    pub(crate) normals: [[f32; 9]; MAX_SKIN_JOINTS],
}
impl SkinPalette {
    pub fn new(matrices: &[[f32; 16]]) -> Result<Self, &'static str> {
        let mut result = Self {
            count: 0,
            matrices: [[0.; 16]; MAX_SKIN_JOINTS],
            normals: [[0.; 9]; MAX_SKIN_JOINTS],
        };
        result.set(matrices)?;
        Ok(result)
    }
    pub fn set(&mut self, matrices: &[[f32; 16]]) -> Result<(), &'static str> {
        if matrices.is_empty() || matrices.len() > MAX_SKIN_JOINTS {
            return Err("invalid skin palette length");
        }
        let mut normals = [[0.; 9]; MAX_SKIN_JOINTS];
        for (i, columns) in matrices.iter().enumerate() {
            let m = Mat4::from_cols_array(columns);
            if !m.is_finite()
                || columns[3] != 0.
                || columns[7] != 0.
                || columns[11] != 0.
                || columns[15] != 1.
            {
                return Err("invalid skin transform");
            }
            let n = m
                .try_inverse()
                .ok_or("singular skin transform")?
                .transpose()
                .to_cols_array();
            normals[i] = [n[0], n[1], n[2], n[4], n[5], n[6], n[8], n[9], n[10]];
        }
        self.count = matrices.len();
        self.matrices[..self.count].copy_from_slice(matrices);
        self.normals = normals;
        Ok(())
    }
    pub fn joint_count(&self) -> usize {
        self.count
    }
    pub fn matrices(&self) -> &[[f32; 16]] {
        &self.matrices[..self.count]
    }
    /// CPU reference for tooling/tests, with identical normalized influences.
    pub fn transform_point(&self, point: Vec3, weights: SkinWeights) -> Result<Vec3, &'static str> {
        if !point.is_finite() {
            return Err("invalid skin point");
        }
        let weights = weights.packed(self.count)?;
        let mut out = Vec3::ZERO;
        for i in 0..4 {
            out += Mat4::from_cols_array(&self.matrices[weights[i] as usize])
                .transform_point3(point)
                * weights[4 + i];
        }
        if out.is_finite() {
            Ok(out)
        } else {
            Err("skin point overflow")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        GREEN, Vec2,
        mesh::Mesh3d,
        retained::{DrawStyle, Frame},
    };
    #[test]
    fn visibility_uses_deformed_bounds_instead_of_bind_bounds() {
        let mut mesh = Mesh3d::cuboid(Vec3::ONE, GREEN).mesh;
        for v in &mut mesh.vertices {
            v.position.x += 100.;
        }
        let skin =
            SkinnedMesh::new(&mesh, &vec![SkinWeights::rigid(0); mesh.vertices.len()], 1).unwrap();
        let mut palette =
            SkinPalette::new(&[Mat4::from_translation(-Vec3::X * 100.).to_cols_array()]).unwrap();
        crate::set_camera(&crate::Camera3D {
            position: Vec3::new(0., 1., 8.),
            target: Vec3::ZERO,
            ..Default::default()
        });
        assert!(crate::draw_skinned(&skin, Mat4::IDENTITY, &palette));
        palette.set(&[Mat4::IDENTITY.to_cols_array()]).unwrap();
        assert!(!crate::draw_skinned(&skin, Mat4::IDENTITY, &palette));
        crate::set_default_camera();
    }
    #[test]
    fn weights_are_validated_and_normalized_once() {
        let mesh = Mesh3d::plane(Vec2::ONE, GREEN, 1).mesh;
        let weights = vec![
            SkinWeights {
                joints: [0, 1, 0, 1],
                weights: [2., 2., 0., 0.]
            };
            mesh.vertices.len()
        ];
        let skin = SkinnedMesh::new(&mesh, &weights, 2).unwrap();
        let palette = SkinPalette::new(&[
            Mat4::IDENTITY.to_cols_array(),
            Mat4::from_translation(Vec3::X * 2.).to_cols_array(),
        ])
        .unwrap();
        let point = palette.transform_point(Vec3::ZERO, weights[0]).unwrap();
        assert_eq!(point, Vec3::X);
        let packed = crate::retained::geometry(skin.resident().id()).unwrap();
        assert_eq!(
            packed.skin.as_ref().unwrap()[0],
            [0., 1., 0., 1., 0.5, 0.5, 0., 0.]
        );
        for invalid in [
            SkinWeights {
                joints: [0; 4],
                weights: [0.; 4],
            },
            SkinWeights {
                joints: [0; 4],
                weights: [f32::NAN, 0., 0., 0.],
            },
            SkinWeights {
                joints: [0; 4],
                weights: [-1., 2., 0., 0.],
            },
            SkinWeights {
                joints: [3; 4],
                weights: [1., 0., 0., 0.],
            },
        ] {
            assert!(SkinnedMesh::new(&mesh, &vec![invalid; mesh.vertices.len()], 2).is_err());
        }
        assert!(SkinnedMesh::new(&mesh, &weights, 33).is_err());
        assert!(SkinnedMesh::new(&mesh, &weights[..1], 2).is_err());
        let huge = SkinWeights {
            joints: [0; 4],
            weights: [f32::MAX; 4],
        };
        assert_eq!(huge.packed(1).unwrap()[4..], [0.25; 4]);
    }
    #[test]
    fn palette_validation_is_transactional_and_normals_use_inverse_transpose() {
        let scale = Mat4::from_scale(Vec3::new(2., 4., 8.)).to_cols_array();
        let mut palette = SkinPalette::new(&[scale]).unwrap();
        assert_eq!(
            palette.normals[0],
            [0.5, 0., 0., 0., 0.25, 0., 0., 0., 0.125]
        );
        for bad in [
            Mat4::from_scale(Vec3::ZERO).to_cols_array(),
            Mat4::perspective_rh_gl(1., 1., 0.1, 10.).to_cols_array(),
            [f32::NAN; 16],
        ] {
            assert!(palette.set(&[bad]).is_err());
            assert_eq!(palette.matrices(), &[scale]);
        }
        assert!(palette.set(&[]).is_err());
        assert_eq!(palette.joint_count(), 1);
    }
    #[test]
    fn animated_bounds_enclose_weighted_vertices_and_draws_snapshot_palettes() {
        let mesh = Mesh3d::cuboid(Vec3::ONE, GREEN).mesh;
        let weights = vec![
            SkinWeights {
                joints: [0, 1, 0, 1],
                weights: [0.25, 0.75, 0., 0.]
            };
            mesh.vertices.len()
        ];
        let skin = SkinnedMesh::new(&mesh, &weights, 2).unwrap();
        let mut palette = SkinPalette::new(&[
            Mat4::from_translation(Vec3::X * 3.).to_cols_array(),
            (Mat4::from_translation(Vec3::Y * 2.) * Mat4::from_scale(Vec3::new(2., 1., 3.)))
                .to_cols_array(),
        ])
        .unwrap();
        let bounds = skin.bounds(&palette).unwrap();
        for v in &mesh.vertices {
            let p = palette.transform_point(v.position, weights[0]).unwrap();
            assert!(
                p.x >= bounds.min.x
                    && p.x <= bounds.max.x
                    && p.y >= bounds.min.y
                    && p.y <= bounds.max.y
                    && p.z >= bounds.min.z
                    && p.z <= bounds.max.z
            );
        }
        let mut frame = Frame::default();
        for _ in 0..2 {
            assert!(frame.record_style(
                skin.resident(),
                Mat4::IDENTITY,
                Mat4::IDENTITY,
                true,
                DrawStyle {
                    skin: Some(&palette),
                    ..Default::default()
                }
            ));
        }
        assert_eq!(frame.batches.len(), 1);
        assert_eq!(frame.batches[0].count, 2);
        assert_eq!(frame.skin_matrices.len(), 2);
        let previous = frame.skin_matrices.clone();
        palette.set(&[Mat4::IDENTITY.to_cols_array(); 2]).unwrap();
        assert_eq!(frame.skin_matrices, previous);
        assert!(frame.record_style(
            skin.resident(),
            Mat4::IDENTITY,
            Mat4::IDENTITY,
            true,
            DrawStyle {
                skin: Some(&palette),
                ..Default::default()
            }
        ));
        frame.finalize();
        assert_eq!(frame.batches.len(), 2);
        assert_eq!(frame.batches[0].reserved, [0, 2]);
        assert_eq!(frame.batches[1].reserved, [2, 2]);
        assert!(!frame.record(skin.resident(), Mat4::IDENTITY, Mat4::IDENTITY, true, None));
        assert!(
            skin.bounds(&SkinPalette::new(&[Mat4::IDENTITY.to_cols_array()]).unwrap())
                .is_err()
        );
        frame.clear();
        assert!(frame.skin_matrices.is_empty());
        assert!(frame.skin_normals.is_empty());
    }
}
