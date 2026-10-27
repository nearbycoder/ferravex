//! Procedural low-poly art: faceted boxes, hand-picked colors, no assets.
//!
//! Every model is built once at startup. Characters are four-influence GPU
//! skins (rigid per part, which keeps the chunky faceted look); props and
//! hats are retained static meshes; the park itself is merged into a few
//! ambient-occlusion-baked chunks by [`crate::scene`].
use ferravex_render::mesh::Mesh3d;
use ferravex_render::prelude::*;
use ferravex_render::skinning::{SkinWeights, SkinnedMesh};

pub fn rgb(hex: u32) -> Color {
    Color::from_rgba((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 255)
}

pub fn shade(c: Color, k: f32) -> Color {
    Color::new((c.r * k).min(1.), (c.g * k).min(1.), (c.b * k).min(1.), c.a)
}

/// Indexed flat-shaded geometry with optional per-vertex bone weights.
pub struct Builder {
    pub mesh: Mesh,
    weights: Option<Vec<SkinWeights>>,
    bone: u16,
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        Self {
            mesh: Mesh {
                vertices: Vec::new(),
                indices: Vec::new(),
                texture: None,
            },
            weights: None,
            bone: 0,
        }
    }
    pub fn skinned() -> Self {
        let mut b = Self::new();
        b.weights = Some(Vec::new());
        b
    }
    pub fn bone(&mut self, bone: u16) -> &mut Self {
        self.bone = bone;
        self
    }
    fn vertex(&mut self, p: Vec3, n: Vec3, c: Color) -> u16 {
        let i = self.mesh.vertices.len();
        assert!(i < u16::MAX as usize - 1, "model too large");
        let mut v = Vertex::new2(p, Vec2::ZERO, c);
        v.normal = n.extend(0.);
        self.mesh.vertices.push(v);
        if let Some(w) = &mut self.weights {
            w.push(SkinWeights::rigid(self.bone));
        }
        i as u16
    }
    pub fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, color: Color) {
        let n = (b - a).cross(c - a).normalize_or_zero();
        let i = self.vertex(a, n, color);
        self.vertex(b, n, color);
        self.vertex(c, n, color);
        self.mesh.indices.extend_from_slice(&[i, i + 1, i + 2]);
    }
    pub fn quad(&mut self, p: [Vec3; 4], color: Color) {
        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        let i = self.vertex(p[0], n, color);
        for q in &p[1..] {
            self.vertex(*q, n, color);
        }
        self.mesh
            .indices
            .extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
    }
    pub fn cube(&mut self, center: Vec3, size: Vec3, color: Color) -> &mut Self {
        self.box_rot(center, size, Quat::IDENTITY, color)
    }
    /// A box with a slightly lighter top face (cheap top-lit look).
    pub fn box_rot(&mut self, center: Vec3, size: Vec3, rot: Quat, color: Color) -> &mut Self {
        let h = size * 0.5;
        let p = |x: f32, y: f32, z: f32| center + rot * vec3(x * h.x, y * h.y, z * h.z);
        let top = shade(color, 1.06);
        let side = color;
        let under = shade(color, 0.85);
        self.quad(
            [
                p(-1., -1., 1.),
                p(1., -1., 1.),
                p(1., 1., 1.),
                p(-1., 1., 1.),
            ],
            side,
        );
        self.quad(
            [
                p(1., -1., -1.),
                p(-1., -1., -1.),
                p(-1., 1., -1.),
                p(1., 1., -1.),
            ],
            side,
        );
        self.quad(
            [
                p(-1., 1., 1.),
                p(1., 1., 1.),
                p(1., 1., -1.),
                p(-1., 1., -1.),
            ],
            top,
        );
        self.quad(
            [
                p(-1., -1., -1.),
                p(1., -1., -1.),
                p(1., -1., 1.),
                p(-1., -1., 1.),
            ],
            under,
        );
        self.quad(
            [
                p(1., -1., 1.),
                p(1., -1., -1.),
                p(1., 1., -1.),
                p(1., 1., 1.),
            ],
            side,
        );
        self.quad(
            [
                p(-1., -1., -1.),
                p(-1., -1., 1.),
                p(-1., 1., 1.),
                p(-1., 1., -1.),
            ],
            side,
        );
        self
    }
    /// Faceted cylinder/cone frustum along +Y from `base`.
    pub fn frustum(
        &mut self,
        base: Vec3,
        bottom: f32,
        top: f32,
        height: f32,
        color: Color,
        sides: u16,
    ) -> &mut Self {
        let n = sides.max(3) as usize;
        let ring = |r: f32, y: f32, k: usize| {
            let a = k as f32 / n as f32 * std::f32::consts::TAU;
            base + vec3(a.cos() * r, y, a.sin() * r)
        };
        for k in 0..n {
            let (a0, a1) = (ring(bottom, 0., k), ring(bottom, 0., k + 1));
            let (b0, b1) = (ring(top, height, k), ring(top, height, k + 1));
            let c = if k % 2 == 0 {
                color
            } else {
                shade(color, 0.94)
            };
            if top > 1e-4 {
                self.quad([a0, a1, b1, b0], c);
                self.tri(base + Vec3::Y * height, b1, b0, shade(color, 1.06));
            } else {
                self.tri(a0, a1, b0, c);
            }
            self.tri(base, a0, a1, shade(color, 0.8));
        }
        self
    }
    /// Low-poly blob (squashed icosahedron-ish sphere).
    pub fn blob(&mut self, center: Vec3, radius: Vec3, color: Color, detail: u16) -> &mut Self {
        let sphere = Mesh3d::sphere(1., color, detail.max(3) * 2, detail.max(2));
        let m = &sphere.mesh;
        for t in m.indices.chunks(3) {
            let v = |i: u16| {
                let p = m.vertices[i as usize].position;
                center + p * radius
            };
            let (a, b, c) = (v(t[0]), v(t[1]), v(t[2]));
            let lift = ((a.y + b.y + c.y) / 3. - center.y) / radius.y.max(1e-3);
            let k = 0.88 + 0.16 * lift.clamp(-1., 1.);
            self.tri(a, b, c, shade(color, k));
        }
        self
    }
    pub fn into_skin(self, joints: usize) -> SkinnedMesh {
        SkinnedMesh::new(
            &self.mesh,
            self.weights.as_ref().expect("skinned builder"),
            joints,
        )
        .expect("valid authored skin")
    }
    pub fn into_mesh(self) -> Mesh3d {
        Mesh3d::from_mesh(self.mesh)
    }
}

// --- Palette: golden-afternoon picnic.
pub const FUR: u32 = 0x8f9098;
pub const FUR_DARK: u32 = 0x45454e;
pub const FUR_LIGHT: u32 = 0xd9d6cd;
pub const MASK: u32 = 0x26262d;
pub const SKIN: u32 = 0xf0c4a0;

/// Raccoon: ten joints. 0 root, 1 body, 2 chest, 3 head, 4/5 tail, 6/7 front
/// legs (left/right), 8/9 back legs. Faces -Z.
pub fn raccoon() -> SkinnedMesh {
    let mut m = Builder::skinned();
    let fur = rgb(FUR);
    let dark = rgb(FUR_DARK);
    let light = rgb(FUR_LIGHT);
    let mask = rgb(MASK);
    let eye = rgb(0xfdfdfb);
    let pupil = rgb(0x111116);
    // A round little body and a big, expressive head (chibi proportions).
    m.bone(1)
        .cube(vec3(0., 0.33, 0.07), vec3(0.32, 0.26, 0.38), fur)
        .cube(
            vec3(0., 0.42, 0.07),
            vec3(0.24, 0.06, 0.32),
            shade(fur, 1.06),
        )
        .cube(vec3(0., 0.23, 0.05), vec3(0.24, 0.06, 0.32), light);
    m.bone(2)
        .cube(vec3(0., 0.36, -0.17), vec3(0.3, 0.26, 0.18), fur)
        .cube(vec3(0., 0.29, -0.24), vec3(0.2, 0.1, 0.08), light);
    m.bone(3)
        .cube(vec3(0., 0.5, -0.44), vec3(0.4, 0.31, 0.3), fur)
        .cube(
            vec3(0., 0.645, -0.44),
            vec3(0.3, 0.04, 0.24),
            shade(fur, 1.05),
        )
        // Fluffy cheeks.
        .cube(vec3(-0.2, 0.45, -0.49), vec3(0.06, 0.15, 0.2), light)
        .cube(vec3(0.2, 0.45, -0.49), vec3(0.06, 0.15, 0.2), light)
        // Muzzle, nose and a dark forehead stripe.
        .cube(vec3(0., 0.435, -0.64), vec3(0.17, 0.12, 0.12), light)
        .cube(vec3(0., 0.47, -0.705), vec3(0.075, 0.06, 0.03), pupil)
        .cube(vec3(0., 0.6, -0.594), vec3(0.05, 0.11, 0.012), mask)
        // The bandit mask wraps around the sides of the head.
        .cube(vec3(0., 0.525, -0.555), vec3(0.42, 0.1, 0.12), mask)
        // Big shiny eyes and expressive white brows.
        .cube(vec3(-0.085, 0.53, -0.617), vec3(0.075, 0.07, 0.012), eye)
        .cube(vec3(0.085, 0.53, -0.617), vec3(0.075, 0.07, 0.012), eye)
        .cube(vec3(-0.078, 0.525, -0.625), vec3(0.04, 0.05, 0.01), pupil)
        .cube(vec3(0.092, 0.525, -0.625), vec3(0.04, 0.05, 0.01), pupil)
        .cube(vec3(-0.07, 0.54, -0.631), vec3(0.014, 0.014, 0.004), eye)
        .cube(vec3(0.1, 0.54, -0.631), vec3(0.014, 0.014, 0.004), eye)
        .cube(vec3(-0.1, 0.6, -0.6), vec3(0.1, 0.03, 0.02), light)
        .cube(vec3(0.1, 0.6, -0.6), vec3(0.1, 0.03, 0.02), light)
        // Ears with dark insides and pale rims.
        .cube(vec3(-0.135, 0.69, -0.41), vec3(0.11, 0.12, 0.06), fur)
        .cube(vec3(0.135, 0.69, -0.41), vec3(0.11, 0.12, 0.06), fur)
        .cube(vec3(-0.135, 0.69, -0.442), vec3(0.065, 0.075, 0.01), dark)
        .cube(vec3(0.135, 0.69, -0.442), vec3(0.065, 0.075, 0.01), dark)
        .cube(vec3(-0.135, 0.755, -0.41), vec3(0.09, 0.025, 0.065), light)
        .cube(vec3(0.135, 0.755, -0.41), vec3(0.09, 0.025, 0.065), light);
    // The magnificent ringed tail, fluffier toward the middle.
    // It sweeps upward so the rings read from behind instead of end-on.
    m.bone(4)
        .cube(vec3(0., 0.41, 0.31), vec3(0.16, 0.16, 0.1), light)
        .cube(vec3(0., 0.44, 0.4), vec3(0.2, 0.2, 0.09), mask);
    m.bone(5)
        .cube(vec3(0., 0.49, 0.48), vec3(0.22, 0.22, 0.1), light)
        .cube(vec3(0., 0.55, 0.555), vec3(0.21, 0.21, 0.09), mask)
        .cube(vec3(0., 0.62, 0.62), vec3(0.18, 0.18, 0.09), light)
        .cube(vec3(0., 0.69, 0.67), vec3(0.12, 0.12, 0.07), mask);
    for (bone, x, z) in [
        (6, -0.1, -0.24),
        (7, 0.1, -0.24),
        (8, -0.1, 0.17),
        (9, 0.1, 0.17),
    ] {
        m.bone(bone)
            .cube(vec3(x, 0.15, z), vec3(0.09, 0.24, 0.09), dark)
            .cube(vec3(x, 0.025, z - 0.025), vec3(0.1, 0.05, 0.13), mask);
    }
    m.into_skin(10)
}

#[derive(Clone, Copy)]
pub struct Outfit {
    pub top: u32,
    pub bottom: u32,
    pub shoes: u32,
    pub skin: u32,
    pub hair: u32,
    pub skirt: bool,
}

/// Biped: 0 root, 1 hips, 2 chest, 3 head, 4/5 arms (left/right), 6/7 legs.
pub fn person(o: Outfit, extras: impl FnOnce(&mut Builder)) -> SkinnedMesh {
    let mut m = Builder::skinned();
    let top = rgb(o.top);
    let bottom = rgb(o.bottom);
    let skin = rgb(o.skin);
    for (bone, x) in [(6, -0.1), (7, 0.1)] {
        m.bone(bone)
            .cube(vec3(x, 0.5, 0.), vec3(0.15, 0.82, 0.16), bottom)
            .cube(vec3(x, 0.05, -0.04), vec3(0.17, 0.1, 0.26), rgb(o.shoes));
    }
    m.bone(1)
        .cube(vec3(0., 0.93, 0.), vec3(0.4, 0.22, 0.26), bottom);
    if o.skirt {
        m.cube(vec3(0., 0.74, 0.), vec3(0.46, 0.34, 0.32), bottom);
    }
    m.bone(2)
        .cube(vec3(0., 1.24, 0.), vec3(0.44, 0.5, 0.27), top)
        .cube(vec3(0., 1.47, 0.), vec3(0.18, 0.06, 0.18), skin);
    for (bone, x) in [(4, -0.275), (5, 0.275)] {
        m.bone(bone)
            .cube(vec3(x, 1.2, 0.), vec3(0.11, 0.46, 0.13), top)
            .cube(vec3(x, 0.93, 0.), vec3(0.1, 0.1, 0.1), skin);
    }
    m.bone(3)
        .cube(vec3(0., 1.64, 0.), vec3(0.29, 0.31, 0.28), skin)
        .cube(
            vec3(0., 1.61, -0.15),
            vec3(0.06, 0.07, 0.05),
            shade(skin, 0.92),
        )
        .cube(
            vec3(-0.065, 1.67, -0.143),
            vec3(0.04, 0.045, 0.012),
            rgb(0x2b2320),
        )
        .cube(
            vec3(0.065, 1.67, -0.143),
            vec3(0.04, 0.045, 0.012),
            rgb(0x2b2320),
        )
        .cube(
            vec3(-0.11, 1.62, -0.141),
            vec3(0.04, 0.025, 0.01),
            rgb(0xf09a8a),
        )
        .cube(
            vec3(0.11, 1.62, -0.141),
            vec3(0.04, 0.025, 0.01),
            rgb(0xf09a8a),
        )
        .cube(vec3(0., 1.8, 0.02), vec3(0.31, 0.06, 0.31), rgb(o.hair));
    extras(&mut m);
    m.into_skin(8)
}

pub fn nana() -> SkinnedMesh {
    person(
        Outfit {
            top: 0xb7a0dc,
            bottom: 0x7e7891,
            shoes: 0x5a3f36,
            skin: 0xf2c9ac,
            hair: 0xf3f0ea,
            skirt: true,
        },
        |m| {
            m.bone(3)
                .cube(vec3(0., 1.84, 0.1), vec3(0.15, 0.13, 0.15), rgb(0xf3f0ea))
                .cube(vec3(0., 1.7, 0.1), vec3(0.3, 0.14, 0.12), rgb(0xf3f0ea))
                // Spectacles.
                .cube(
                    vec3(-0.065, 1.67, -0.152),
                    vec3(0.08, 0.07, 0.01),
                    rgb(0xc9a14b),
                )
                .cube(
                    vec3(0.065, 1.67, -0.152),
                    vec3(0.08, 0.07, 0.01),
                    rgb(0xc9a14b),
                );
            m.bone(2)
                .cube(vec3(0., 1.3, -0.14), vec3(0.2, 0.28, 0.02), rgb(0xf6e7d0))
                .cube(
                    vec3(0., 1.42, -0.145),
                    vec3(0.06, 0.06, 0.02),
                    rgb(0xe36f8a),
                );
        },
    )
}

pub fn pumble() -> SkinnedMesh {
    person(
        Outfit {
            top: 0xe0a35a,
            bottom: 0x5b8a4b,
            shoes: 0x4b3a2a,
            skin: 0xe9b48f,
            hair: 0x9a9690,
            skirt: false,
        },
        |m| {
            m.bone(2)
                .cube(vec3(0., 1.18, -0.14), vec3(0.3, 0.36, 0.02), rgb(0x5b8a4b))
                .cube(
                    vec3(-0.12, 1.4, -0.14),
                    vec3(0.05, 0.12, 0.02),
                    rgb(0x5b8a4b),
                )
                .cube(
                    vec3(0.12, 1.4, -0.14),
                    vec3(0.05, 0.12, 0.02),
                    rgb(0x5b8a4b),
                );
            m.bone(3)
                .cube(vec3(0., 1.53, -0.12), vec3(0.27, 0.15, 0.08), rgb(0xd9d4cc))
                .cube(vec3(0., 1.84, 0.), vec3(0.6, 0.035, 0.6), rgb(0xe8c97c))
                .cube(vec3(0., 1.9, 0.), vec3(0.32, 0.12, 0.32), rgb(0xdcbc6c))
                .cube(vec3(0., 1.86, 0.), vec3(0.33, 0.04, 0.33), rgb(0xc0392b));
        },
    )
}

pub fn trumble() -> SkinnedMesh {
    person(
        Outfit {
            top: 0xc23b30,
            bottom: 0x283a66,
            shoes: 0x16161c,
            skin: 0xf1c6a2,
            hair: 0x3b2a20,
            skirt: false,
        },
        |m| {
            m.bone(2);
            for y in [1.1, 1.22, 1.34] {
                m.cube(vec3(0., y, -0.14), vec3(0.05, 0.05, 0.02), rgb(0xf3c64a));
            }
            m.cube(vec3(-0.18, 1.45, 0.), vec3(0.12, 0.05, 0.3), rgb(0xf3c64a))
                .cube(vec3(0.18, 1.45, 0.), vec3(0.12, 0.05, 0.3), rgb(0xf3c64a));
            m.bone(3)
                .cube(
                    vec3(0., 1.585, -0.15),
                    vec3(0.17, 0.035, 0.03),
                    rgb(0x3b2a20),
                )
                .cube(vec3(0., 1.95, 0.), vec3(0.3, 0.34, 0.3), rgb(0x1d2a52))
                .cube(vec3(0., 1.82, -0.04), vec3(0.32, 0.05, 0.38), rgb(0x16161c))
                .cube(
                    vec3(0., 1.99, -0.152),
                    vec3(0.08, 0.08, 0.02),
                    rgb(0xf3c64a),
                )
                .cube(vec3(0., 2.2, 0.), vec3(0.08, 0.2, 0.08), rgb(0xf7f3ec));
            // Baton.
            m.bone(5).cube(
                vec3(0.275, 0.86, -0.12),
                vec3(0.025, 0.025, 0.3),
                rgb(0xf7f3ec),
            );
        },
    )
}

pub fn pip() -> SkinnedMesh {
    person(
        Outfit {
            top: 0xf4cb4b,
            bottom: 0x3f7fc1,
            shoes: 0xd9534f,
            skin: 0xf3cba8,
            hair: 0x7a4b2a,
            skirt: false,
        },
        |m| {
            m.bone(3)
                .cube(vec3(0., 1.84, -0.06), vec3(0.32, 0.08, 0.36), rgb(0xd9534f))
                .cube(
                    vec3(0., 1.81, -0.24),
                    vec3(0.24, 0.025, 0.14),
                    rgb(0xd9534f),
                );
            m.bone(2)
                .cube(vec3(0., 1.16, -0.14), vec3(0.04, 0.4, 0.02), rgb(0xe0b33a));
        },
    )
}

/// The dog shares the raccoon's quadruped rig: floppy ears, waggy tail.
pub fn biscuit() -> SkinnedMesh {
    let mut m = Builder::skinned();
    let coat = rgb(0xc78a4c);
    let cream = rgb(0xf4ece0);
    let dark = rgb(0x6b4428);
    m.bone(1)
        .cube(vec3(0., 0.34, 0.06), vec3(0.3, 0.26, 0.5), coat)
        .cube(vec3(0., 0.25, 0.02), vec3(0.24, 0.06, 0.4), cream);
    m.bone(2)
        .cube(vec3(0., 0.38, -0.2), vec3(0.3, 0.28, 0.22), coat)
        .cube(vec3(0., 0.32, -0.3), vec3(0.2, 0.16, 0.04), cream)
        .cube(vec3(0., 0.42, -0.31), vec3(0.32, 0.04, 0.04), rgb(0xd9534f));
    m.bone(3)
        .cube(vec3(0., 0.53, -0.42), vec3(0.27, 0.26, 0.26), coat)
        .cube(vec3(0., 0.47, -0.6), vec3(0.15, 0.12, 0.16), cream)
        .cube(
            vec3(0., 0.51, -0.685),
            vec3(0.07, 0.06, 0.03),
            rgb(0x1a1414),
        )
        .cube(
            vec3(-0.07, 0.575, -0.553),
            vec3(0.04, 0.045, 0.012),
            rgb(0x1a1414),
        )
        .cube(
            vec3(0.07, 0.575, -0.553),
            vec3(0.04, 0.045, 0.012),
            rgb(0x1a1414),
        )
        .cube(vec3(0., 0.44, -0.56), vec3(0.06, 0.05, 0.1), rgb(0xe36f8a))
        .cube(vec3(-0.16, 0.5, -0.4), vec3(0.06, 0.2, 0.12), dark)
        .cube(vec3(0.16, 0.5, -0.4), vec3(0.06, 0.2, 0.12), dark);
    m.bone(4)
        .cube(vec3(0., 0.42, 0.34), vec3(0.07, 0.07, 0.14), coat);
    m.bone(5)
        .cube(vec3(0., 0.47, 0.47), vec3(0.06, 0.06, 0.14), coat)
        .cube(vec3(0., 0.5, 0.56), vec3(0.06, 0.06, 0.06), cream);
    for (bone, x, z) in [
        (6, -0.1, -0.27),
        (7, 0.1, -0.27),
        (8, -0.1, 0.17),
        (9, 0.1, 0.17),
    ] {
        m.bone(bone)
            .cube(vec3(x, 0.16, z), vec3(0.085, 0.26, 0.085), coat)
            .cube(vec3(x, 0.03, z - 0.02), vec3(0.095, 0.06, 0.12), cream);
    }
    m.into_skin(10)
}

/// Cosmetic hats in head-bone bind space (the raccoon's head top is ~0.62).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hat {
    None,
    PieTin,
    TeaCosy,
    PaperCrown,
    FlowerCrown,
    CapCrown,
}

impl Hat {
    pub const ALL: [Hat; 6] = [
        Hat::None,
        Hat::PieTin,
        Hat::TeaCosy,
        Hat::PaperCrown,
        Hat::FlowerCrown,
        Hat::CapCrown,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Hat::None => "Bare head",
            Hat::PieTin => "Pie tin",
            Hat::TeaCosy => "Tea cosy",
            Hat::PaperCrown => "Paper crown",
            Hat::FlowerCrown => "Daisy chain",
            Hat::CapCrown => "Bottle-cap crown",
        }
    }
    pub fn unlock(self) -> &'static str {
        match self {
            Hat::None => "Always in fashion.",
            Hat::PieTin => "Cross anything off the list.",
            Hat::TeaCosy => "Finish the whole list.",
            Hat::PaperCrown => "Finish in under 15 minutes.",
            Hat::FlowerCrown => "Find three hidden bonus tasks.",
            Hat::CapCrown => "Collect every bottle cap in one day.",
        }
    }
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|h| *h == self).unwrap_or(0)
    }
}

pub fn hat(h: Hat) -> Option<Mesh3d> {
    let mut m = Builder::new();
    let y = 0.665;
    let c = vec3(0., y, -0.43);
    match h {
        Hat::None => return None,
        Hat::PieTin => {
            m.frustum(c, 0.17, 0.13, 0.05, rgb(0xc9ccd2), 10).frustum(
                c + Vec3::Y * 0.05,
                0.11,
                0.1,
                0.012,
                rgb(0xd9a35a),
                10,
            );
        }
        Hat::TeaCosy => {
            m.blob(c + Vec3::Y * 0.07, vec3(0.17, 0.13, 0.15), rgb(0xe27d8d), 3)
                .blob(
                    c + Vec3::Y * 0.21,
                    vec3(0.045, 0.045, 0.045),
                    rgb(0xfff2c4),
                    2,
                )
                .cube(
                    c + vec3(0., 0.04, 0.),
                    vec3(0.35, 0.035, 0.3),
                    rgb(0xf6c2cc),
                );
        }
        Hat::PaperCrown => {
            for k in 0..6 {
                let a = k as f32 / 6. * std::f32::consts::TAU;
                let p = c + vec3(a.cos() * 0.12, 0.06, a.sin() * 0.11);
                m.box_rot(
                    p,
                    vec3(0.11, 0.12, 0.012),
                    Quat::from_rotation_y(-a + std::f32::consts::FRAC_PI_2),
                    if k % 2 == 0 {
                        rgb(0xf6d04d)
                    } else {
                        rgb(0xe85d75)
                    },
                );
            }
        }
        Hat::FlowerCrown => {
            for k in 0..7 {
                let a = k as f32 / 7. * std::f32::consts::TAU;
                let p = c + vec3(a.cos() * 0.15, 0.015, a.sin() * 0.14);
                m.cube(p, vec3(0.06, 0.04, 0.06), rgb(0xfdfdf7)).cube(
                    p + Vec3::Y * 0.022,
                    vec3(0.03, 0.01, 0.03),
                    rgb(0xf6c444),
                );
            }
        }
        Hat::CapCrown => {
            for k in 0..8 {
                let a = k as f32 / 8. * std::f32::consts::TAU;
                let p = c + vec3(a.cos() * 0.13, 0.03, a.sin() * 0.12);
                m.frustum(p, 0.04, 0.04, 0.02, rgb(0xd6dbe2), 6).frustum(
                    p + Vec3::Y * 0.02,
                    0.028,
                    0.028,
                    0.006,
                    rgb(0xd94a4a),
                    6,
                );
            }
        }
    }
    Some(m.into_mesh())
}

/// Prop meshes, centered like their colliders.
pub fn prop(kind: crate::sim::park::PropKind) -> Mesh3d {
    use crate::sim::park::PropKind as K;
    let mut m = Builder::new();
    match kind {
        K::Pie => {
            m.frustum(vec3(0., -0.05, 0.), 0.2, 0.23, 0.07, rgb(0xd9a35a), 12)
                .frustum(vec3(0., 0.02, 0.), 0.21, 0.19, 0.03, rgb(0xe9bc6e), 12);
            for k in 0..4 {
                let a = k as f32 * 0.785;
                m.box_rot(
                    vec3(0., 0.052, 0.),
                    vec3(0.34, 0.012, 0.03),
                    Quat::from_rotation_y(a),
                    rgb(0xc98a45),
                );
            }
            m.cube(
                vec3(0.06, 0.06, -0.05),
                vec3(0.04, 0.012, 0.03),
                rgb(0x8e2c3a),
            );
        }
        K::Marrow => {
            m.blob(Vec3::ZERO, vec3(0.32, 0.11, 0.11), rgb(0x4f8a3c), 4)
                .cube(vec3(-0.18, 0.06, 0.), vec3(0.1, 0.02, 0.12), rgb(0x9cc46a))
                .cube(vec3(0.05, 0.07, 0.), vec3(0.08, 0.02, 0.12), rgb(0x9cc46a))
                .cube(vec3(0.34, 0.0, 0.), vec3(0.05, 0.03, 0.03), rgb(0x6b5a2e));
        }
        K::Pinecone => {
            for k in 0..4 {
                let y = -0.09 + k as f32 * 0.05;
                let r = 0.1 - (k as f32 - 1.).abs() * 0.025;
                m.frustum(
                    vec3(0., y, 0.),
                    r,
                    r * 0.7,
                    0.05,
                    shade(rgb(0x8a5a32), 1. - k as f32 * 0.06),
                    7,
                );
            }
        }
        K::Sheet => {
            m.cube(Vec3::ZERO, vec3(0.4, 0.016, 0.3), rgb(0xfbf6e6));
            for k in 0..5 {
                let z = -0.1 + k as f32 * 0.05;
                m.cube(vec3(0., 0.009, z), vec3(0.34, 0.003, 0.006), rgb(0x2b2a33));
            }
            for (x, z) in [(-0.12, -0.08), (-0.03, -0.03), (0.06, 0.02), (0.13, -0.05)] {
                m.cube(vec3(x, 0.011, z), vec3(0.03, 0.004, 0.022), rgb(0x2b2a33));
            }
        }
        K::Tray => {
            m.cube(vec3(0., -0.012, 0.), vec3(0.68, 0.025, 0.48), rgb(0xd7dbe0))
                .cube(vec3(0., 0.012, 0.23), vec3(0.68, 0.03, 0.02), rgb(0xb8bec7))
                .cube(
                    vec3(0., 0.012, -0.23),
                    vec3(0.68, 0.03, 0.02),
                    rgb(0xb8bec7),
                )
                .cube(vec3(0.33, 0.012, 0.), vec3(0.02, 0.03, 0.48), rgb(0xb8bec7))
                .cube(
                    vec3(-0.33, 0.012, 0.),
                    vec3(0.02, 0.03, 0.48),
                    rgb(0xb8bec7),
                );
        }
        K::Blanket => {
            for k in 0..4 {
                let y = -0.06 + k as f32 * 0.04;
                let c = if k % 2 == 0 {
                    rgb(0xd9534f)
                } else {
                    rgb(0xf7efe2)
                };
                m.cube(vec3(0., y, 0.), vec3(0.5, 0.04, 0.4), c);
            }
        }
        K::Melon => {
            for k in 0..8 {
                let a = k as f32 / 8. * std::f32::consts::TAU;
                let c = if k % 2 == 0 {
                    rgb(0x2f7d3a)
                } else {
                    rgb(0x4fa155)
                };
                m.box_rot(
                    vec3(a.cos() * 0.15, 0., a.sin() * 0.15),
                    vec3(0.24, 0.5, 0.2),
                    Quat::from_rotation_y(-a),
                    c,
                );
            }
            m.blob(Vec3::ZERO, Vec3::splat(0.262), rgb(0x3a8c44), 4);
        }
        K::Cushion => {
            m.blob(Vec3::ZERO, vec3(0.17, 0.045, 0.17), rgb(0xe8607a), 4)
                .cube(vec3(0., 0., 0.19), vec3(0.06, 0.03, 0.06), rgb(0xc74862));
        }
        K::Crate => {
            let wood = rgb(0xb5834f);
            m.cube(Vec3::ZERO, Vec3::splat(0.68), wood);
            for (y, s) in [(0.3, 0.72), (-0.3, 0.72)] {
                m.cube(vec3(0., y, 0.), vec3(s, 0.07, s), rgb(0x8d6239));
            }
            m.cube(vec3(0., 0., -0.345), vec3(0.6, 0.1, 0.02), rgb(0x8d6239))
                .cube(vec3(0., 0.05, -0.36), vec3(0.24, 0.12, 0.01), rgb(0xe9d9b9));
        }
        K::Cake => {
            m.frustum(vec3(0., -0.12, 0.), 0.35, 0.35, 0.24, rgb(0xf6e3c8), 12)
                .frustum(vec3(0., 0.12, 0.), 0.25, 0.25, 0.18, rgb(0xf7c6d4), 12)
                .frustum(vec3(0., 0.3, 0.), 0.15, 0.15, 0.15, rgb(0xfdf8f0), 12)
                .frustum(vec3(0., -0.02, 0.), 0.36, 0.36, 0.03, rgb(0xe36f8a), 12)
                .frustum(vec3(0., 0.27, 0.), 0.26, 0.26, 0.03, rgb(0xe36f8a), 12)
                .blob(vec3(0., 0.5, 0.), Vec3::splat(0.05), rgb(0xd62f3f), 2);
        }
        K::Dish => {
            m.frustum(vec3(0., -0.025, 0.), 0.12, 0.16, 0.035, rgb(0xf5f3ee), 10);
            m.blob(vec3(0., 0.03, 0.), vec3(0.09, 0.03, 0.09), rgb(0xe7b75a), 3);
        }
        K::Teapot => {
            m.blob(Vec3::ZERO, vec3(0.12, 0.1, 0.1), rgb(0x6fa8d6), 3)
                .cube(vec3(0.15, 0.02, 0.), vec3(0.1, 0.03, 0.03), rgb(0x5a93c1))
                .cube(vec3(-0.13, 0.02, 0.), vec3(0.04, 0.08, 0.02), rgb(0x5a93c1))
                .blob(vec3(0., 0.1, 0.), Vec3::splat(0.03), rgb(0xf5f3ee), 2);
        }
        K::Jelly => {
            m.frustum(vec3(0., -0.1, 0.), 0.13, 0.09, 0.2, rgb(0xe8445a), 8)
                .frustum(vec3(0., -0.1, 0.), 0.14, 0.14, 0.02, rgb(0xf5f3ee), 8);
        }
        K::Apple => {
            m.blob(Vec3::ZERO, Vec3::splat(0.07), rgb(0xd9352f), 3)
                .cube(vec3(0., 0.075, 0.), vec3(0.012, 0.03, 0.012), rgb(0x5a3a1a))
                .cube(
                    vec3(0.02, 0.08, 0.),
                    vec3(0.03, 0.008, 0.018),
                    rgb(0x5aa34a),
                );
        }
    }
    m.into_mesh()
}

pub fn shiny() -> Mesh3d {
    let mut m = Builder::new();
    m.frustum(Vec3::ZERO, 0.075, 0.075, 0.025, rgb(0xe6e8ee), 8)
        .frustum(Vec3::Y * 0.025, 0.055, 0.055, 0.006, rgb(0xd94a4a), 8);
    m.into_mesh()
}

/// A pond duck: white, smug, unbothered by the chaos.
pub fn duck() -> Mesh3d {
    let mut m = Builder::new();
    m.blob(vec3(0., 0.12, 0.), vec3(0.2, 0.12, 0.14), rgb(0xfbfaf4), 3)
        .cube(vec3(0.17, 0.27, 0.), vec3(0.13, 0.13, 0.12), rgb(0xfbfaf4))
        .cube(vec3(0.27, 0.25, 0.), vec3(0.1, 0.04, 0.07), rgb(0xf2a43a))
        .cube(
            vec3(0.2, 0.3, 0.061),
            vec3(0.025, 0.025, 0.01),
            rgb(0x111116),
        )
        .cube(
            vec3(0.2, 0.3, -0.061),
            vec3(0.025, 0.025, 0.01),
            rgb(0x111116),
        )
        .box_rot(
            vec3(-0.19, 0.2, 0.),
            vec3(0.1, 0.05, 0.1),
            Quat::from_rotation_z(0.6),
            rgb(0xf1efe6),
        );
    m.into_mesh()
}

/// Speedrun ghost: the raccoon, slightly smaller, with a little crown.
pub fn ghost_crown() -> Mesh3d {
    let mut m = Builder::new();
    for k in 0..5 {
        let a = k as f32 / 5. * std::f32::consts::TAU;
        m.cube(
            vec3(a.cos() * 0.1, 0.68, -0.41 + a.sin() * 0.1),
            vec3(0.05, 0.1, 0.05),
            rgb(0xffe68a),
        );
    }
    m.into_mesh()
}
