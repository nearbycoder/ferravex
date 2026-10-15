//! Reusable geometry with face normals for lit 3D scenes.
use crate::prelude::*;

/// Construct once and draw many times. Mesh dimensions are baked into vertices;
/// draw transforms use only rotation/translation to keep normals correct.
pub struct Mesh3d {
    pub mesh: Mesh,
    cached: std::cell::OnceCell<crate::retained::ResidentMesh>,
}

impl Mesh3d {
    pub fn from_mesh(mesh: Mesh) -> Self {
        Self {
            mesh,
            cached: Default::default(),
        }
    }
    /// Invalidate the GPU snapshot after editing the public source mesh.
    pub fn invalidate_gpu(&mut self) {
        self.cached.take();
    }
    pub fn resident(&self) -> &crate::retained::ResidentMesh {
        self.cached.get_or_init(|| {
            crate::retained::ResidentMesh::new(&self.mesh).expect("valid retained Mesh3d")
        })
    }
    pub fn draw_transformed(&self, model: Mat4) {
        crate::draw_resident(self.resident(), model);
    }
    /// CPU path for geometry whose exact submission order must be preserved.
    pub fn draw_immediate(&self, model: Mat4) {
        crate::draw_mesh_transformed(&self.mesh, model);
    }

    /// Tessellated XZ ground plane centered at the origin, facing positive Y.
    /// Subdivision provides nearby vertices for vertex lighting/distance fog on large surfaces.
    /// Bounded to 1..=28 cells per side to keep low-poly scenes inexpensive.
    pub fn plane(size: Vec2, color: Color, subdivisions: u16) -> Self {
        assert!(
            size.is_finite() && size.min_element() > 0.0,
            "plane size must be finite and positive"
        );
        let n = subdivisions.clamp(1, 28);
        let stride = n + 1;
        let mut vertices = Vec::with_capacity(stride as usize * stride as usize);
        let mut indices = Vec::with_capacity(n as usize * n as usize * 6);
        for z in 0..=n {
            for x in 0..=n {
                let uv = vec2(x as f32, z as f32) / n as f32;
                let position = (uv - Vec2::splat(0.5)) * size;
                let mut vertex = Vertex::new2(vec3(position.x, 0.0, position.y), uv, color);
                vertex.normal = Vec3::Y.extend(0.0);
                vertices.push(vertex);
            }
        }
        for z in 0..n {
            for x in 0..n {
                let a = z * stride + x;
                let b = a + stride;
                indices.extend([a, b, a + 1, a + 1, b, b + 1]);
            }
        }
        Self {
            cached: Default::default(),
            mesh: Mesh {
                vertices,
                indices,
                texture: None,
            },
        }
    }

    /// Flat-shaded, capped cylinder along Y. Construct once and reuse for props.
    pub fn cylinder(radius: f32, height: f32, color: Color, segments: u16) -> Self {
        Self::frustum(radius, radius, height, color, segments)
    }

    /// Flat-shaded cone along Y, with its tip at positive Y.
    pub fn cone(radius: f32, height: f32, color: Color, segments: u16) -> Self {
        Self::frustum(radius, 0.0, height, color, segments)
    }

    /// Capped tapered prism; radii are at negative/positive Y respectively.
    /// Zero-radius caps are omitted. Tessellation is bounded to 3..=128 sides.
    pub fn frustum(bottom: f32, top: f32, height: f32, color: Color, segments: u16) -> Self {
        assert!(
            bottom.is_finite()
                && top.is_finite()
                && bottom >= 0.0
                && top >= 0.0
                && bottom.max(top) > 0.0
                && height.is_finite()
                && height > 0.0,
            "frustum dimensions must be finite with positive height and at least one positive radius"
        );
        let segments = segments.clamp(3, 128);
        let mut vertices = Vec::with_capacity(segments as usize * 12);
        let mut triangle = |a: Vec3, b: Vec3, c: Vec3| {
            let cross = (b - a).cross(c - a);
            if cross.length_squared() < 1e-16 {
                return;
            }
            let normal = cross.normalize().extend(0.0);
            for position in [a, b, c] {
                let mut vertex = Vertex::new2(position, Vec2::ZERO, color);
                vertex.normal = normal;
                vertices.push(vertex);
            }
        };
        let y = height * 0.5;
        for i in 0..segments {
            let angle = |n: u16| n as f32 / segments as f32 * std::f32::consts::TAU;
            let ring = |theta: f32, radius: f32, y: f32| {
                vec3(theta.cos() * radius, y, theta.sin() * radius)
            };
            let a = ring(angle(i), bottom, -y);
            let b = ring(angle(i + 1), bottom, -y);
            let c = ring(angle(i), top, y);
            let d = ring(angle(i + 1), top, y);
            triangle(a, c, b);
            triangle(b, c, d);
            triangle(vec3(0.0, -y, 0.0), a, b);
            triangle(vec3(0.0, y, 0.0), d, c);
        }
        let indices = (0..vertices.len() as u16).collect();
        Self {
            cached: Default::default(),
            mesh: Mesh {
                vertices,
                indices,
                texture: None,
            },
        }
    }

    pub fn cuboid(size: Vec3, color: Color) -> Self {
        assert!(
            size.is_finite() && size.min_element() > 0.0,
            "cuboid size must be finite and positive"
        );
        let mut vertices = Vec::with_capacity(24);
        let mut indices = Vec::with_capacity(36);
        let faces = [
            (Vec3::X, Vec3::Y, Vec3::Z),
            (-Vec3::X, Vec3::Y, -Vec3::Z),
            (Vec3::Y, Vec3::Z, Vec3::X),
            (-Vec3::Y, Vec3::Z, -Vec3::X),
            (Vec3::Z, Vec3::X, Vec3::Y),
            (-Vec3::Z, Vec3::X, -Vec3::Y),
        ];
        for (normal, u, v) in faces {
            let base = vertices.len() as u16;
            for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let mut vertex = Vertex::new2(
                    (normal + u * a + v * b) * size * 0.5,
                    vec2((a + 1.0) * 0.5, (b + 1.0) * 0.5),
                    color,
                );
                vertex.normal = normal.extend(0.0);
                vertices.push(vertex);
            }
            indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        Self {
            cached: Default::default(),
            mesh: Mesh {
                vertices,
                indices,
                texture: None,
            },
        }
    }

    /// Faceted sphere. Latitude rings are bounded to retain a low-poly budget.
    pub fn sphere(radius: f32, color: Color, segments: u16, rings: u16) -> Self {
        assert!(
            radius.is_finite() && radius > 0.0,
            "sphere radius must be finite and positive"
        );
        let segments = segments.clamp(3, 64);
        // A flat sphere has 6 * segments * (rings - 1) nondegenerate vertices/indices.
        // Retain the original examples' 5,000-index budget even at maximum detail.
        let rings = rings.clamp(2, 64).min(1 + 4998 / (6 * segments));
        let mut vertices = Vec::with_capacity((segments as usize + 1) * (rings as usize + 1));
        let mut indices = Vec::with_capacity(segments as usize * rings as usize * 6);
        for y in 0..=rings {
            let v = y as f32 / rings as f32;
            let phi = v * std::f32::consts::PI;
            for x in 0..=segments {
                let u = x as f32 / segments as f32;
                let theta = u * std::f32::consts::TAU;
                let normal = if y == 0 {
                    Vec3::Y
                } else if y == rings {
                    -Vec3::Y
                } else {
                    vec3(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin())
                };
                let mut vertex = Vertex::new2(normal * radius, vec2(u, v), color);
                vertex.normal = normal.extend(0.0);
                vertices.push(vertex);
            }
        }
        for y in 0..rings {
            for x in 0..segments {
                let a = y * (segments + 1) + x;
                let b = a + segments + 1;
                indices.extend([a, b, a + 1, a + 1, b, b + 1]);
            }
        }
        // Duplicate triangle vertices so each facet receives its own normal.
        // This intentionally preserves a low-poly silhouette and flat lighting.
        let mut flat_vertices = Vec::with_capacity(indices.len());
        for triangle in indices.as_chunks::<3>().0 {
            // Calculate on the unit sphere so degeneracy tests and face normals
            // remain stable for both small projectiles and large scenery.
            let a = vertices[triangle[0] as usize].normal.truncate();
            let b = vertices[triangle[1] as usize].normal.truncate();
            let c = vertices[triangle[2] as usize].normal.truncate();
            let cross = (b - a).cross(c - a);
            if cross.length_squared() < 1e-12 {
                continue;
            }
            let outward = cross.dot(a + b + c) >= 0.0;
            let normal = if outward { cross } else { -cross }.normalize();
            // Winding agrees with the face normal, including on back-face-culling hosts.
            let order = if outward {
                *triangle
            } else {
                [triangle[0], triangle[2], triangle[1]]
            };
            for i in order {
                let mut vertex = vertices[i as usize];
                vertex.normal = normal.extend(0.0);
                flat_vertices.push(vertex);
            }
        }
        // Also catches unexpected extra triangles at the poles from numeric drift.
        assert!(
            flat_vertices.len() < 5000,
            "sphere tessellation exceeds the low-poly index budget"
        );
        let flat_indices = (0..flat_vertices.len() as u16).collect();
        Self {
            cached: Default::default(),
            mesh: Mesh {
                vertices: flat_vertices,
                indices: flat_indices,
                texture: None,
            },
        }
    }

    pub fn draw(&self, position: Vec3, rotation: Quat) {
        self.draw_transformed(Mat4::from_rotation_translation(rotation, position));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maximum_sphere_requests_remain_complete_within_renderer_limits() {
        for (segments, rings) in [(64, 64), (u16::MAX, u16::MAX), (3, 64), (16, 64)] {
            let sphere = Mesh3d::sphere(1.0, WHITE, segments, rings);
            assert!(sphere.mesh.indices.len() < 5000);
            assert!(sphere.mesh.vertices.len() < 10000);
            for axis in [Vec3::X, Vec3::Y, Vec3::Z, -Vec3::X, -Vec3::Y, -Vec3::Z] {
                assert!(
                    sphere
                        .mesh
                        .vertices
                        .iter()
                        .any(|v| v.position.dot(axis) > 0.4)
                );
            }
        }
        for radius in [0.001, 1.0, 1000.0] {
            let sphere = Mesh3d::sphere(radius, WHITE, 64, 64);
            assert_eq!(sphere.mesh.indices.len(), 4992);
            assert!(sphere.mesh.vertices.iter().all(|v| v.normal.is_finite()));
        }
    }

    #[test]
    fn mesh_indices_and_normals_are_valid() {
        for shape in [
            Mesh3d::cuboid(vec3(1.0, 2.0, 3.0), WHITE),
            Mesh3d::sphere(1.0, WHITE, 16, 12),
            Mesh3d::cylinder(1.0, 2.0, WHITE, 12),
            Mesh3d::cone(1.0, 2.0, WHITE, 6),
            Mesh3d::plane(vec2(100.0, 100.0), WHITE, 20),
        ] {
            assert!(
                shape
                    .mesh
                    .indices
                    .iter()
                    .all(|&i| (i as usize) < shape.mesh.vertices.len())
            );
            assert!(
                shape
                    .mesh
                    .vertices
                    .iter()
                    .all(|v| (v.normal.truncate().length() - 1.0).abs() < 0.0001)
            );
        }
    }

    #[test]
    fn triangle_winding_matches_outward_flat_normals() {
        for shape in [
            Mesh3d::cuboid(vec3(1.0, 2.0, 3.0), WHITE),
            Mesh3d::sphere(1.0, WHITE, 12, 8),
            Mesh3d::sphere(0.001, WHITE, 64, 64),
            Mesh3d::cylinder(1.0, 2.0, WHITE, 12),
            Mesh3d::cone(1.0, 2.0, WHITE, 6),
            Mesh3d::frustum(0.0, 1.0, 2.0, WHITE, 6),
            Mesh3d::plane(vec2(20.0, 10.0), WHITE, 8),
        ] {
            for triangle in shape.mesh.indices.as_chunks::<3>().0 {
                let [a, b, c] = triangle.map(|i| shape.mesh.vertices[i as usize]);
                let winding = (b.position - a.position).cross(c.position - a.position);
                assert!(winding.dot(a.normal.truncate()) > 0.0);
                assert!(a.normal.abs_diff_eq(b.normal, 0.0001));
                assert!(a.normal.abs_diff_eq(c.normal, 0.0001));
            }
        }
    }

    #[test]
    fn cylinder_and_cone_segments_are_bounded_without_broken_caps() {
        for segments in [0, 1, 2, 128, u16::MAX] {
            let expected = usize::from(segments.clamp(3, 128));
            for (shape, per_segment) in [
                (Mesh3d::cylinder(1.0, 2.0, WHITE, segments), 12),
                (Mesh3d::cone(1.0, 2.0, WHITE, segments), 6),
            ] {
                assert_eq!(shape.mesh.indices.len(), expected * per_segment);
                assert_eq!(shape.mesh.indices.len(), shape.mesh.vertices.len());
                assert!(shape.mesh.vertices.iter().all(|v| v.normal.is_finite()));
            }
        }
    }

    #[test]
    fn invalid_mesh_dimensions_are_rejected_before_recording() {
        for invalid in [0.0, -1.0, f32::INFINITY, f32::NAN] {
            assert!(std::panic::catch_unwind(|| Mesh3d::sphere(invalid, WHITE, 8, 6)).is_err());
            assert!(
                std::panic::catch_unwind(|| Mesh3d::cuboid(Vec3::splat(invalid), WHITE)).is_err()
            );
            assert!(
                std::panic::catch_unwind(|| Mesh3d::plane(Vec2::splat(invalid), WHITE, 4)).is_err()
            );
            assert!(std::panic::catch_unwind(|| Mesh3d::cylinder(1.0, invalid, WHITE, 8)).is_err());
        }
    }

    #[test]
    fn tapered_mesh_faces_point_outward_and_caps_close() {
        for shape in [
            Mesh3d::cylinder(1.0, 2.0, WHITE, 8),
            Mesh3d::cone(1.0, 2.0, WHITE, 8),
        ] {
            for face in shape.mesh.vertices.as_chunks::<3>().0 {
                let center = (face[0].position + face[1].position + face[2].position) / 3.0;
                let normal = face[0].normal.truncate();
                assert!(normal.dot(center) > 0.0);
            }
        }
        assert_eq!(
            Mesh3d::cylinder(1.0, 2.0, WHITE, 8).mesh.indices.len(),
            8 * 12
        );
        assert_eq!(Mesh3d::cone(1.0, 2.0, WHITE, 8).mesh.indices.len(), 8 * 6);
    }

    #[test]
    fn ground_plane_has_nearby_vertices_correct_winding_and_bounded_batches() {
        let plane = Mesh3d::plane(vec2(440.0, 440.0), WHITE, 22);
        assert!(
            plane
                .mesh
                .vertices
                .iter()
                .any(|v| v.position.length() < 0.001)
        );
        for triangle in plane.mesh.indices.as_chunks::<3>().0 {
            let [a, b, c] = [triangle[0], triangle[1], triangle[2]]
                .map(|i| plane.mesh.vertices[i as usize].position);
            assert!((b - a).cross(c - a).y > 0.0);
        }
        assert!(Mesh3d::plane(Vec2::ONE, WHITE, u16::MAX).mesh.indices.len() < 5000);
    }
}
