//! Offline, deterministic triangle-scene vertex occlusion. No per-frame ray work.
use crate::{Mat4, Mesh, Vec3};
use ferravex_graphics::Bounds;
#[derive(Clone, Copy)]
struct Triangle {
    a: Vec3,
    b: Vec3,
    c: Vec3,
}
impl Triangle {
    fn bounds(self) -> Bounds {
        Bounds {
            min: self.a.min(self.b).min(self.c),
            max: self.a.max(self.b).max(self.c),
        }
    }
}
struct Node {
    bounds: Bounds,
    left: usize,
    right: usize,
    start: usize,
    count: usize,
}
pub struct AoScene {
    triangles: Vec<Triangle>,
    nodes: Vec<Node>,
}
#[derive(Clone, Copy, Debug)]
pub struct AoSettings {
    pub samples: usize,
    pub radius: f32,
    pub bias: f32,
    pub strength: f32,
    pub max_tests: usize,
}
impl Default for AoSettings {
    fn default() -> Self {
        Self {
            samples: 8,
            radius: 2.,
            bias: 0.01,
            strength: 0.35,
            max_tests: 2_000_000,
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct AoStats {
    pub rays: usize,
    pub node_tests: usize,
    pub triangle_tests: usize,
    pub occluded: usize,
}
impl AoScene {
    pub fn new<'a>(
        meshes: impl IntoIterator<Item = (&'a Mesh, Mat4)>,
    ) -> Result<Self, &'static str> {
        let mut triangles = Vec::new();
        for (mesh, model) in meshes {
            if !affine(model) || !mesh.indices.len().is_multiple_of(3) {
                return Err("invalid AO scene transform/triangles");
            }
            for face in mesh.indices.as_chunks::<3>().0 {
                if triangles.len() >= 100_000 {
                    return Err("AO scene exceeds 100000 triangles");
                }
                let mut p = [Vec3::ZERO; 3];
                for i in 0..3 {
                    let v = mesh
                        .vertices
                        .get(face[i] as usize)
                        .ok_or("invalid AO vertex index")?;
                    p[i] = model.transform_point3(v.position);
                    if !p[i].is_finite() || p[i].abs().max_element() > 1e6 {
                        return Err("invalid AO position");
                    }
                }
                if (p[1] - p[0]).cross(p[2] - p[0]).length_squared() > 1e-14 {
                    triangles.push(Triangle {
                        a: p[0],
                        b: p[1],
                        c: p[2],
                    });
                }
            }
        }
        let mut scene = Self {
            nodes: Vec::with_capacity(triangles.len() * 2),
            triangles,
        };
        if !scene.triangles.is_empty() {
            scene.build(0, scene.triangles.len());
        }
        Ok(scene)
    }
    fn build(&mut self, start: usize, count: usize) -> usize {
        let bounds = self.triangles[start..start + count].iter().fold(
            Bounds {
                min: Vec3::splat(f32::INFINITY),
                max: Vec3::splat(f32::NEG_INFINITY),
            },
            |b, t| {
                let t = t.bounds();
                Bounds {
                    min: b.min.min(t.min),
                    max: b.max.max(t.max),
                }
            },
        );
        let index = self.nodes.len();
        self.nodes.push(Node {
            bounds,
            left: 0,
            right: 0,
            start,
            count,
        });
        if count > 8 {
            let size = bounds.max - bounds.min;
            let axis = if size.x >= size.y && size.x >= size.z {
                0
            } else if size.y >= size.z {
                1
            } else {
                2
            };
            let mid = count / 2;
            self.triangles[start..start + count].select_nth_unstable_by(mid, |a, b| {
                (a.a + a.b + a.c).to_array()[axis].total_cmp(&(b.a + b.b + b.c).to_array()[axis])
            });
            let left = self.build(start, mid);
            let right = self.build(start + mid, count - mid);
            self.nodes[index].left = left;
            self.nodes[index].right = right;
            self.nodes[index].count = 0;
        }
        index
    }
    fn occluded(
        &self,
        origin: Vec3,
        direction: Vec3,
        distance: f32,
        stats: &mut AoStats,
        limit: usize,
    ) -> Result<bool, &'static str> {
        if self.nodes.is_empty() {
            return Ok(false);
        }
        let mut stack = [0usize; 64];
        let mut len = 1;
        while len > 0 {
            len -= 1;
            let node = &self.nodes[stack[len]];
            stats.node_tests += 1;
            if stats.node_tests + stats.triangle_tests > limit {
                return Err("AO work budget exceeded");
            }
            if !ray_bounds(origin, direction, distance, node.bounds) {
                continue;
            }
            if node.count == 0 {
                if len + 2 > stack.len() {
                    return Err("AO traversal depth exceeded");
                }
                stack[len] = node.left;
                stack[len + 1] = node.right;
                len += 2;
            } else {
                for t in &self.triangles[node.start..node.start + node.count] {
                    stats.triangle_tests += 1;
                    if stats.node_tests + stats.triangle_tests > limit {
                        return Err("AO work budget exceeded");
                    }
                    if ray_triangle(origin, direction, distance, *t) {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
    /// All colors are committed together; a budget/validation failure preserves the mesh.
    pub fn bake(
        &self,
        mesh: &mut Mesh,
        model: Mat4,
        settings: AoSettings,
    ) -> Result<AoStats, &'static str> {
        if !affine(model)
            || mesh.vertices.len() > 100_000
            || !(1..=32).contains(&settings.samples)
            || !settings.radius.is_finite()
            || !(0.001..=1000.).contains(&settings.radius)
            || !settings.bias.is_finite()
            || settings.bias <= 0.
            || settings.bias >= settings.radius
            || !settings.strength.is_finite()
            || !(0. ..=1.).contains(&settings.strength)
            || settings.max_tests == 0
        {
            return Err("invalid AO bake settings");
        }
        let normal_matrix = model
            .try_inverse()
            .ok_or("singular AO transform")?
            .transpose();
        let mut colors = Vec::with_capacity(mesh.vertices.len());
        let mut stats = AoStats::default();
        for vertex in &mesh.vertices {
            let p = model.transform_point3(vertex.position);
            let normal = normal_matrix
                .transform_vector3(vertex.normal.truncate())
                .normalize_or_zero();
            if !p.is_finite() || p.abs().max_element() > 1e6 || !vertex.normal.is_finite() {
                return Err("invalid AO vertex");
            }
            let tangent = normal
                .cross(if normal.y.abs() < 0.95 {
                    Vec3::Y
                } else {
                    Vec3::X
                })
                .normalize_or_zero();
            let bitangent = normal.cross(tangent);
            let mut hits = 0;
            if normal.length_squared() > 0.5 {
                for sample in 0..settings.samples {
                    let u = (sample as f32 + 0.5) / settings.samples as f32;
                    let angle = sample as f32 * 2.3999631;
                    let r = u.sqrt();
                    let direction = tangent * (r * angle.cos())
                        + bitangent * (r * angle.sin())
                        + normal * (1. - u).sqrt();
                    stats.rays += 1;
                    if self.occluded(
                        p + normal * settings.bias,
                        direction,
                        settings.radius,
                        &mut stats,
                        settings.max_tests,
                    )? {
                        hits += 1;
                        stats.occluded += 1;
                    }
                }
            }
            let factor = 1. - settings.strength * hits as f32 / settings.samples as f32;
            let mut color = vertex.color;
            for c in &mut color[..3] {
                *c = (*c as f32 * factor).round() as u8;
            }
            colors.push(color);
        }
        for (vertex, color) in mesh.vertices.iter_mut().zip(colors) {
            vertex.color = color;
        }
        Ok(stats)
    }
}
fn affine(m: Mat4) -> bool {
    let v = m.to_cols_array();
    m.is_finite() && v[3] == 0. && v[7] == 0. && v[11] == 0. && v[15] == 1.
}
fn ray_bounds(o: Vec3, d: Vec3, distance: f32, b: Bounds) -> bool {
    let mut near = 0f32;
    let mut far = distance;
    for i in 0..3 {
        let o = o.to_array()[i];
        let d = d.to_array()[i];
        let min = b.min.to_array()[i];
        let max = b.max.to_array()[i];
        if d.abs() < 1e-9 {
            if o < min || o > max {
                return false;
            }
        } else {
            let a = (min - o) / d;
            let b = (max - o) / d;
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return false;
            }
        }
    }
    true
}
fn ray_triangle(o: Vec3, d: Vec3, distance: f32, t: Triangle) -> bool {
    let e1 = t.b - t.a;
    let e2 = t.c - t.a;
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-8 {
        return false;
    }
    let inverse = 1. / det;
    let s = o - t.a;
    let u = s.dot(p) * inverse;
    if !(0. ..=1.).contains(&u) {
        return false;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inverse;
    if v < 0. || u + v > 1. {
        return false;
    }
    let time = e2.dot(q) * inverse;
    time > 1e-5 && time < distance
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WHITE, mesh::Mesh3d};
    #[test]
    fn scene_occlusion_is_baked_and_budget_failure_is_transactional() {
        let ceiling = Mesh3d::cuboid(Vec3::new(6., 0.1, 6.), WHITE);
        let scene = AoScene::new([(&ceiling.mesh, Mat4::from_translation(Vec3::Y * 0.5))]).unwrap();
        let mut floor = Mesh3d::plane(crate::Vec2::ONE, WHITE, 1).mesh;
        let original = floor.vertices[0].color;
        assert!(
            scene
                .bake(
                    &mut floor,
                    Mat4::IDENTITY,
                    AoSettings {
                        max_tests: 1,
                        ..Default::default()
                    }
                )
                .is_err()
        );
        assert_eq!(floor.vertices[0].color, original);
        let stats = scene
            .bake(&mut floor, Mat4::IDENTITY, Default::default())
            .unwrap();
        assert!(stats.occluded > 0);
        assert!(floor.vertices.iter().any(|v| v.color[0] < original[0]));
        assert!(floor.vertices.iter().all(|v| v.color[3] == 255));
    }
}
