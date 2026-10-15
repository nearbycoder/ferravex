//! Procedural art: flat normals, hand-picked material colors, no runtime asset downloads.
use ferravex_fps::Arena;
use ferravex_render::prelude::*;

pub fn rgb(hex: u32) -> Color {
    Color::from_rgba((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 255)
}

pub struct Model {
    pub mesh: Mesh,
    cached: std::cell::OnceCell<ferravex_render::retained::ResidentMesh>,
    weights: Option<Vec<ferravex_render::skinning::SkinWeights>>,
    bone: u16,
}
impl Model {
    pub fn new() -> Self {
        Self {
            cached: Default::default(),
            weights: None,
            bone: 0,
            mesh: Mesh {
                vertices: Vec::new(),
                indices: Vec::new(),
                texture: None,
            },
        }
    }
    fn skinned() -> Self {
        let mut model = Self::new();
        model.weights = Some(Vec::new());
        model
    }
    fn bone(&mut self, bone: u16) {
        self.bone = bone;
    }
    fn into_skin(self, joints: usize) -> ferravex_render::skinning::SkinnedMesh {
        ferravex_render::skinning::SkinnedMesh::new(
            &self.mesh,
            self.weights.as_ref().expect("skinned model"),
            joints,
        )
        .expect("valid authored skin")
    }
    pub fn resident(&self) -> &ferravex_render::retained::ResidentMesh {
        self.cached.get_or_init(|| {
            ferravex_render::retained::ResidentMesh::new(&self.mesh).expect("valid retained model")
        })
    }
    pub fn quad(&mut self, points: [Vec3; 4], color: Color) {
        self.cached.take();
        assert!(self.mesh.vertices.len() + 4 < u16::MAX as usize);
        let base = self.mesh.vertices.len() as u16;
        let normal = (points[1] - points[0])
            .cross(points[2] - points[0])
            .normalize_or_zero();
        for (p, uv) in
            points
                .into_iter()
                .zip([vec2(0., 0.), vec2(1., 0.), vec2(1., 1.), vec2(0., 1.)])
        {
            let mut v = Vertex::new2(p, uv, color);
            v.normal = normal.extend(0.0);
            self.mesh.vertices.push(v);
        }
        if let Some(weights) = &mut self.weights {
            weights.extend([ferravex_render::skinning::SkinWeights::rigid(self.bone); 4]);
        }
        self.mesh
            .indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    pub fn cube(&mut self, center: Vec3, size: Vec3, color: Color) {
        self.box_rotated(center, size, Quat::IDENTITY, color);
    }
    pub fn box_rotated(&mut self, center: Vec3, size: Vec3, rot: Quat, color: Color) {
        let h = size * 0.5;
        let p = |x: f32, y: f32, z: f32| center + rot * vec3(x * h.x, y * h.y, z * h.z);
        self.quad(
            [
                p(-1., -1., 1.),
                p(1., -1., 1.),
                p(1., 1., 1.),
                p(-1., 1., 1.),
            ],
            color,
        );
        self.quad(
            [
                p(1., -1., -1.),
                p(-1., -1., -1.),
                p(-1., 1., -1.),
                p(1., 1., -1.),
            ],
            color,
        );
        self.quad(
            [
                p(-1., 1., 1.),
                p(1., 1., 1.),
                p(1., 1., -1.),
                p(-1., 1., -1.),
            ],
            color,
        );
        self.quad(
            [
                p(-1., -1., -1.),
                p(1., -1., -1.),
                p(1., -1., 1.),
                p(-1., -1., 1.),
            ],
            color,
        );
        self.quad(
            [
                p(1., -1., 1.),
                p(1., -1., -1.),
                p(1., 1., -1.),
                p(1., 1., 1.),
            ],
            color,
        );
        self.quad(
            [
                p(-1., -1., -1.),
                p(-1., -1., 1.),
                p(-1., 1., 1.),
                p(-1., 1., -1.),
            ],
            color,
        );
    }
    pub fn draw(&self) {
        ferravex_render::draw_resident(self.resident(), Mat4::IDENTITY);
    }
    pub fn triangles(&self) -> usize {
        self.mesh.indices.len() / 3
    }
}

/// Authored industrial container yard. Structural collision comes from the shared arena.
pub fn build_yard(arena: &Arena) -> Model {
    let mut m = Model::new();
    m.cube(vec3(0., -0.22, 0.), vec3(100., 0.4, 100.), rgb(0x77796e));
    // Slab seams and faded paint break up the ground without texture bandwidth.
    for i in -7..=7 {
        m.cube(
            vec3(i as f32 * 4., -0.012, 0.),
            vec3(0.025, 0.015, 54.),
            rgb(0x62665f),
        );
        m.cube(
            vec3(0., -0.011, i as f32 * 4.),
            vec3(54., 0.015, 0.025),
            rgb(0x62665f),
        );
    }
    for x in [-14., 14.] {
        m.cube(vec3(x, 0.005, 0.), vec3(0.10, 0.01, 29.), rgb(0xbab48a));
    }
    for (i, obstacle) in arena.solids.iter().enumerate() {
        let c = Vec3::from_array(obstacle.center.to_array());
        let s = Vec3::from_array(obstacle.half_extents.to_array()) * 2.;
        let boundary = s.x > 20. || s.z > 20.;
        let paint = if boundary {
            rgb(0xa49f8d)
        } else {
            [rgb(0x63766e), rgb(0x9b654b), rgb(0x566971), rgb(0xb4a386)][i % 4]
        };
        if !boundary {
            // Baked directional shadow for the fixed sun. Collision geometry remains authoritative.
            let x0 = c.x - s.x * 0.5;
            let x1 = c.x + s.x * 0.5;
            let z0 = c.z - s.z * 0.5;
            let z1 = c.z + s.z * 0.5;
            let dx = s.y * 0.59;
            let dz = -s.y * 0.35;
            let y = 0.022;
            let shadow = rgb(0x596357);
            m.quad(
                [
                    vec3(x0, y, z0),
                    vec3(x0 + dx, y, z0 + dz),
                    vec3(x1 + dx, y, z0 + dz),
                    vec3(x1, y, z0),
                ],
                shadow,
            );
            m.quad(
                [
                    vec3(x1, y, z0),
                    vec3(x1 + dx, y, z0 + dz),
                    vec3(x1 + dx, y, z1 + dz),
                    vec3(x1, y, z1),
                ],
                shadow,
            );
        }
        m.cube(c, s, paint);
        // Narrow coping caps and dark foundation make the industrial walls legible.
        m.cube(
            c + vec3(0., s.y * 0.5 + 0.065, 0.),
            vec3(s.x + 0.08, 0.13, s.z + 0.08),
            if boundary {
                rgb(0x777c70)
            } else {
                rgb(0x46504b)
            },
        );
        if !boundary {
            m.cube(
                c + vec3(0., -s.y * 0.5 + 0.08, 0.),
                vec3(s.x + 0.04, 0.16, s.z + 0.04),
                rgb(0x46504b),
            );
            // Corrugation: shallow strips; silhouette stays low-poly, never a noisy texture.
            let n = (s.x / 0.6).floor() as i32;
            for j in 1..n {
                let x = c.x - s.x * 0.5 + j as f32 * 0.6;
                for z in [c.z - s.z * 0.5 - 0.012, c.z + s.z * 0.5 + 0.012] {
                    m.cube(
                        vec3(x, c.y, z),
                        vec3(0.045, (s.y - 0.22).max(0.1), 0.035),
                        Color::new(paint.r * 0.83, paint.g * 0.83, paint.b * 0.83, 1.),
                    );
                }
            }
            // Stenciled shipping label, rendered as a few geometric bars.
            let z = c.z + s.z * 0.5 + 0.035;
            m.cube(
                vec3(c.x + s.x * 0.22, c.y + 0.1, z),
                vec3((s.x * 0.22).min(0.9), 0.3, 0.018),
                rgb(0xd0c7a4),
            );
            for j in 0..3 {
                m.cube(
                    vec3(c.x + s.x * 0.22, c.y + 0.18 - j as f32 * 0.08, z + 0.014),
                    vec3((s.x * 0.16).min(0.65), 0.025, 0.01),
                    rgb(0x5b6358),
                );
            }
        }
    }
    // Distant warehouse silhouettes sit outside the playable collision perimeter.
    for (x, z, w, h, d) in [
        (-34., -24., 16., 10., 10.),
        (1., -36., 25., 7., 9.),
        (34., -15., 12., 13., 17.),
        (-34., 13., 12., 8., 20.),
        (34., 23., 16., 7., 10.),
    ] {
        m.cube(vec3(x, h * 0.5, z), vec3(w, h, d), rgb(0x9b9d8d));
        m.cube(vec3(x, h, z), vec3(w + 0.4, 0.3, d + 0.4), rgb(0x5f7069));
        for row in 0..2 {
            for col in 0..4 {
                m.cube(
                    vec3(
                        x - w * 0.35 + col as f32 * w * 0.23,
                        h * 0.45 + row as f32 * 2.5,
                        z + d * 0.5 + 0.02,
                    ),
                    vec3(1.3, 1.0, 0.05),
                    rgb(0x5b6e6b),
                );
            }
        }
    }
    // Gantry crane: an unmistakable silhouette, minimal geometry.
    for x in [-25., 25.] {
        m.cube(vec3(x, 7., -29.), vec3(0.55, 14., 0.55), rgb(0x957849));
        m.box_rotated(
            vec3(x, 5., -29.),
            vec3(0.22, 10., 0.22),
            Quat::from_rotation_z(if x < 0. { -0.16 } else { 0.16 }),
            rgb(0x806c48),
        );
    }
    m.cube(vec3(0., 13.7, -29.), vec3(52., 0.8, 0.8), rgb(0x957849));
    m.cube(vec3(-5., 12.9, -29.), vec3(2.1, 0.7, 1.6), rgb(0x55655d));
    m.cube(vec3(-5., 9.7, -29.), vec3(0.045, 6., 0.045), rgb(0x394740));
    // Site designation: a floor target zone, readable from a first-person camera.
    let zone = vec3(9., 0.015, -8.);
    for x in [-3., 3.] {
        m.cube(zone + vec3(x, 0., 0.), vec3(0.12, 0.015, 6.), rgb(0xd7b66b));
    }
    for z in [-3., 3.] {
        m.cube(zone + vec3(0., 0., z), vec3(6., 0.015, 0.12), rgb(0xd7b66b));
    }
    m
}

pub fn build_operator() -> ferravex_render::skinning::SkinnedMesh {
    let mut m = Model::skinned();
    m.bone(1);
    let cloth = rgb(0x6a705e);
    let armor = rgb(0x353e38);
    let boots = rgb(0x292f2c);
    m.cube(vec3(0., 1.10, 0.), vec3(0.58, 0.65, 0.33), cloth);
    m.cube(vec3(0., 1.15, -0.20), vec3(0.48, 0.43, 0.13), armor);
    m.bone(2);
    m.cube(vec3(0., 1.63, 0.), vec3(0.31, 0.33, 0.30), rgb(0xb79875));
    m.cube(vec3(0., 1.76, 0.01), vec3(0.39, 0.16, 0.37), cloth);
    m.cube(vec3(0., 1.66, -0.16), vec3(0.30, 0.09, 0.03), rgb(0x243331));
    for x in [-0.16, 0.16] {
        m.bone(if x < 0. { 3 } else { 4 });
        m.cube(vec3(x, 0.44, 0.), vec3(0.23, 0.74, 0.27), cloth);
        m.cube(vec3(x, 0.12, -0.07), vec3(0.25, 0.23, 0.40), boots);
        m.bone(if x < 0. { 5 } else { 6 });
        m.cube(vec3(x * 2., 1.05, -0.15), vec3(0.17, 0.46, 0.21), cloth);
    }
    m.bone(7);
    m.cube(vec3(0.22, 1.10, -0.49), vec3(0.12, 0.16, 0.78), armor);
    m.into_skin(8)
}

pub fn build_rifle() -> ferravex_render::skinning::SkinnedMesh {
    let mut m = Model::skinned();
    let steel = rgb(0x3d4948);
    let dark = rgb(0x283330);
    let wood = rgb(0x827452);
    m.cube(vec3(0., 0., -0.07), vec3(0.11, 0.16, 0.43), steel);
    m.cube(vec3(0., -0.02, 0.23), vec3(0.09, 0.14, 0.25), wood);
    m.cube(vec3(0., 0.015, -0.38), vec3(0.085, 0.10, 0.23), wood);
    m.cube(vec3(0., 0.045, -0.62), vec3(0.035, 0.035, 0.30), dark);
    m.bone(1);
    m.box_rotated(
        vec3(0., -0.18, -0.06),
        vec3(0.075, 0.24, 0.10),
        Quat::from_rotation_x(-0.25),
        dark,
    );
    m.bone(0);
    m.box_rotated(
        vec3(0., -0.14, 0.10),
        vec3(0.065, 0.18, 0.08),
        Quat::from_rotation_x(-0.25),
        wood,
    );
    m.cube(vec3(0., 0.11, -0.12), vec3(0.035, 0.04, 0.32), dark);
    m.cube(vec3(0., 0.09, -0.71), vec3(0.016, 0.065, 0.018), steel);
    // Two gloved hands and rolled sleeves, in the same low-poly vocabulary.
    m.bone(3);
    m.box_rotated(
        vec3(0.045, -0.19, 0.10),
        vec3(0.11, 0.11, 0.17),
        Quat::from_rotation_x(-0.4),
        rgb(0x656b53),
    );
    m.bone(2);
    m.box_rotated(
        vec3(-0.07, -0.10, -0.32),
        vec3(0.13, 0.13, 0.19),
        Quat::from_rotation_z(-0.3),
        rgb(0x656b53),
    );
    m.box_rotated(
        vec3(-0.15, -0.23, -0.20),
        vec3(0.14, 0.32, 0.16),
        Quat::from_rotation_z(-0.45),
        rgb(0x92947a),
    );
    m.into_skin(4)
}
