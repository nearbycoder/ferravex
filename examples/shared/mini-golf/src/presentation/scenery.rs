//! Per-hole decoration built once when a hole loads: grass tufts, sand
//! grains, hedge flowers and (3D) one baked static course mesh. Everything is
//! derived from the hole index with a tiny deterministic generator, so a hole
//! always looks the same and nothing here allocates per frame.
use super::*;

/// xorshift64*: deterministic decoration only, never gameplay.
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        ((self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 40) as f32) / (1u64 << 24) as f32
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Tuft {
    pub at: Vec2,
    /// -1 (shadowed) ..= 1 (sunlit).
    pub shade: f32,
    pub lean: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Flower {
    pub at: Vec2,
    pub color: Color,
}

/// Ambient pollen drifting over the green; positions are derived from time.
#[derive(Clone, Copy, Debug)]
pub(super) struct Mote {
    pub home: Vec2,
    pub phase: f32,
    pub speed: f32,
}

pub(super) const MOTES: usize = 22;

pub(super) struct Scenery {
    pub hole: usize,
    pub tufts: Vec<Tuft>,
    pub grains: Vec<(Vec2, f32)>,
    pub flowers: Vec<Flower>,
    pub motes: [Mote; MOTES],
    /// 3D only: base, lawn, hazards, rails, tee mat and hedges in one mesh.
    pub course: Option<Mesh3d>,
}

const FLOWER_COLORS: [Color; 4] = [
    Color::new(0.96, 0.78, 0.38, 1.),
    Color::new(0.95, 0.55, 0.62, 1.),
    Color::new(0.98, 0.95, 0.88, 1.),
    Color::new(0.72, 0.62, 0.95, 1.),
];

fn in_patch(course: &course::Hole, p: Vec2, surface: Surface) -> bool {
    course
        .patches
        .iter()
        .any(|s| s.surface == surface && s.contains(p))
}

impl Scenery {
    pub fn empty() -> Self {
        Self {
            hole: usize::MAX,
            tufts: Vec::new(),
            grains: Vec::new(),
            flowers: Vec::new(),
            motes: [Mote {
                home: Vec2::ZERO,
                phase: 0.,
                speed: 0.,
            }; MOTES],
            course: None,
        }
    }

    pub fn build(course: &course::Hole, hole: usize, view: View) -> Self {
        let mut rng = Rng::new(hole as u64 + 7);
        let mut tufts = Vec::with_capacity(240);
        for _ in 0..240 {
            let at = vec2(
                rng.range(0.25, SIZE.x - 0.25),
                rng.range(0.25, SIZE.y - 0.25),
            );
            let (shade, lean) = (rng.range(-1., 1.), rng.range(-0.6, 0.6));
            if !in_patch(course, at, Surface::Water) && !in_patch(course, at, Surface::Sand) {
                tufts.push(Tuft { at, shade, lean });
            }
        }
        let mut grains = Vec::with_capacity(160);
        for patch in course.patches.iter().filter(|p| p.surface == Surface::Sand) {
            let count = (patch.size.x * patch.size.y * 8.) as usize;
            for _ in 0..count.min(160 - grains.len()) {
                grains.push((
                    patch.min + patch.size * vec2(rng.range(0.04, 0.96), rng.range(0.04, 0.96)),
                    rng.range(-1., 1.),
                ));
            }
        }
        let mut flowers = Vec::with_capacity(48);
        for _ in 0..48 {
            let side = rng.next();
            let x = rng.range(-0.6, SIZE.x + 0.6);
            let y = if side < 0.5 {
                rng.range(-1.05, -0.45)
            } else {
                rng.range(SIZE.y + 0.45, SIZE.y + 1.05)
            };
            flowers.push(Flower {
                at: vec2(x, y),
                color: FLOWER_COLORS[(rng.next() * 4.) as usize % 4],
            });
        }
        let motes = std::array::from_fn(|_| Mote {
            home: vec2(rng.range(0., SIZE.x), rng.range(0., SIZE.y)),
            phase: rng.range(0., std::f32::consts::TAU),
            speed: rng.range(0.15, 0.4),
        });
        let course_mesh = (view == View::ThreeD).then(|| course_mesh(course, hole));
        Self {
            hole,
            tufts,
            grains,
            flowers,
            motes,
            course: course_mesh,
        }
    }

    /// Mote position (course metres, height) at presentation time `t`.
    pub fn mote(&self, i: usize, t: f32) -> (Vec3, f32) {
        let m = self.motes[i];
        let a = m.phase + t * m.speed;
        let p = m.home + vec2((a * 0.7).sin() * 1.2, (a * 0.9).cos() * 0.8);
        let twinkle = 0.5 + 0.5 * (a * 3.1).sin();
        (vec3(p.x, 0.6 + 0.35 * (a * 1.3).sin(), p.y), twinkle)
    }
}

fn shade(c: Color, k: f32) -> Color {
    let k = 1. + k;
    Color::new(
        (c.r * k).clamp(0., 1.),
        (c.g * k).clamp(0., 1.),
        (c.b * k).clamp(0., 1.),
        1.,
    )
}

/// Grid of flat quads (unshared vertices) so each cell keeps its own color.
fn cells(
    min: Vec2,
    size: Vec2,
    step: f32,
    y: f32,
    mut color: impl FnMut(Vec2, usize, usize) -> Color,
) -> Mesh {
    let nx = (size.x / step).ceil().max(1.) as usize;
    let nz = (size.y / step).ceil().max(1.) as usize;
    let mut vertices = Vec::with_capacity(nx * nz * 4);
    let mut indices = Vec::with_capacity(nx * nz * 6);
    for z in 0..nz {
        for x in 0..nx {
            let a = min + vec2(x as f32, z as f32) * step;
            let b = (a + Vec2::splat(step)).min(min + size);
            let c = color(a, x, z);
            let base = vertices.len() as u16;
            for p in [a, vec2(b.x, a.y), vec2(a.x, b.y), b] {
                let mut v = Vertex::new2(vec3(p.x, y, p.y), Vec2::ZERO, c);
                v.normal = Vec3::Y.extend(0.);
                vertices.push(v);
            }
            indices.extend([base, base + 2, base + 1, base + 1, base + 2, base + 3]);
        }
    }
    Mesh {
        vertices,
        indices,
        texture: None,
    }
}

const LAWN_A: Color = Color::new(0.40, 0.62, 0.42, 1.);
const LAWN_B: Color = Color::new(0.36, 0.57, 0.38, 1.);

/// Bake the static 3D course once per hole.
fn course_mesh(course: &course::Hole, hole: usize) -> Mesh3d {
    let mut rng = Rng::new(hole as u64 * 31 + 3);
    let lawn = cells(Vec2::ZERO, SIZE, 0.5, 0., |p, x, _| {
        let stripe = if (x / 2) % 2 == 0 { LAWN_A } else { LAWN_B };
        let edge = (p.x.min(SIZE.x - p.x).min(p.y).min(SIZE.y - p.y) / 1.2).min(1.);
        shade(stripe, rng.range(-0.05, 0.05) - 0.08 * (1. - edge))
    });
    let mut parts: Vec<(Mesh, Mat4)> = Vec::with_capacity(48);
    parts.push((
        Mesh3d::cuboid(vec3(19.4, 0.7, 13.4), Color::from_rgba(24, 44, 36, 255)).mesh,
        Mat4::from_translation(vec3(9., -0.36, 6.)),
    ));
    parts.push((lawn, Mat4::IDENTITY));
    for p in &course.patches {
        match p.surface {
            Surface::Water => parts.push((
                cells(p.min, p.size, 0.5, 0.012, |q, _, _| {
                    let d = ((q - p.min) / p.size - Vec2::splat(0.5)).length();
                    shade(
                        Color::new(0.20, 0.47, 0.58, 1.),
                        -0.25 * d + rng.range(-0.03, 0.03),
                    )
                }),
                Mat4::IDENTITY,
            )),
            Surface::Sand => parts.push((
                cells(p.min, p.size, 0.25, 0.014, |_, _, _| {
                    shade(Color::new(0.88, 0.74, 0.45, 1.), rng.range(-0.07, 0.05))
                }),
                Mat4::IDENTITY,
            )),
            Surface::Slope => {}
        }
    }
    let tee = course.tee;
    parts.push((
        Mesh3d::cuboid(vec3(0.9, 0.02, 0.9), Color::from_rgba(70, 116, 74, 255)).mesh,
        Mat4::from_translation(vec3(tee.x, 0.01, tee.y)),
    ));
    let stone = Color::from_rgba(222, 210, 176, 255);
    let cap = Color::from_rgba(238, 229, 202, 255);
    for (x, z, sx, sz) in [
        (9., -0.16, 18.64, 0.32),
        (9., 12.16, 18.64, 0.32),
        (-0.16, 6., 0.32, 12.),
        (18.16, 6., 0.32, 12.),
    ] {
        parts.push((
            Mesh3d::cuboid(vec3(sx, 0.32, sz), stone).mesh,
            Mat4::from_translation(vec3(x, 0.16, z)),
        ));
        parts.push((
            Mesh3d::cuboid(vec3(sx + 0.04, 0.05, sz + 0.04), cap).mesh,
            Mat4::from_translation(vec3(x, 0.345, z)),
        ));
    }
    // Hedge row with flowers along both long sides.
    for i in 0..10 {
        let x = 0.6 + i as f32 * 1.85;
        for z in [-0.75, 12.75] {
            let tone = rng.range(-0.08, 0.08);
            parts.push((
                Mesh3d::cuboid(
                    vec3(0.62, 0.34, 0.5),
                    shade(Color::new(0.24, 0.40, 0.25, 1.), tone),
                )
                .mesh,
                Mat4::from_translation(vec3(x, 0.17, z)),
            ));
            for _ in 0..2 {
                let c = FLOWER_COLORS[(rng.next() * 4.) as usize % 4];
                parts.push((
                    Mesh3d::cuboid(Vec3::splat(0.09), c).mesh,
                    Mat4::from_translation(vec3(
                        x + rng.range(-0.25, 0.25),
                        0.36,
                        z + rng.range(-0.18, 0.18),
                    )),
                ));
            }
        }
    }
    Mesh3d::combine(parts.iter().map(|(m, t)| (m, *t))).expect("authored static course")
}
