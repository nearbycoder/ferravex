//! Thistledown Green, drawn: a golden-afternoon low-poly park.
//!
//! Static geometry (ground, hedges, tables, bandstand, the hill, the distant
//! village) is merged into chunked retained meshes and baked once with vertex
//! ambient occlusion. Trees use two-level LOD chains; grass, flowers, bushes
//! and canopies sway through a custom vertex hook; lanterns, the sun and
//! water glints use emissive fragment hooks so the bloom threshold only
//! catches them. Vision cones are alpha-blended ground decals with their own
//! fragment hook. Characters are GPU-skinned with sun shadows.
use crate::models::{self, Builder, Hat, rgb, shade};
use crate::rig::{BipedMotion, BipedRig, QuadInput, QuadMotion, QuadRig, model};
use crate::sim::folk::{ALARMED, CURIOUS, Kind, Mode, Pose};
use crate::sim::park::{DecoKind, Look, PropKind, Role, TreeKind};
use crate::sim::props::Holder;
use crate::sim::{Game, flat, heading};
use ferravex_render::ao::{AoScene, AoSettings};
use ferravex_render::lighting::{PointLight, SunShadow};
use ferravex_render::material::{BlendMode, MaterialOptions, ShaderMaterial, ShaderSource};
use ferravex_render::mesh::Mesh3d;
use ferravex_render::prelude::*;
use ferravex_render::shaders::{LitMaterial, LitSettings};
use ferravex_render::skinning::SkinnedMesh;
use ferravex_render::visibility::{LodChain, LodState};

pub const SUN: Vec3 = Vec3::new(-0.62, 0.56, 0.55);
pub const HAZE: u32 = 0xf6dcb4;
pub const SKY_TOP: u32 = 0x8cc3e8;

const SWAY_VERTEX: &str = r#"
vec3 ferravex_vertex(vec3 position) {
    float h = max(position.y - u_params[0].w, 0.0);
    float phase = a_model3.x * 0.37 + a_model3.z * 0.23;
    float t = u_params[0].x;
    float k = u_params[0].y * h * h;
    position.x += (sin(t * 1.7 + phase) + 0.45 * sin(t * 3.3 + phase * 2.1)) * k;
    position.z += cos(t * 1.25 + phase * 1.3) * k * 0.55;
    return position;
}
vec3 ferravex_normal(vec3 normal) { return normal; }
"#;

const GLOW_FRAGMENT: &str = r#"
vec4 ferravex_fragment(vec4 color, vec3 world, vec3 normal) {
    float pulse = 1.0 + u_params[0].y * sin(u_params[0].z * 3.0 + world.x * 1.7 + world.z);
    return vec4(color.rgb * u_params[0].x * pulse + u_params[1].rgb, color.a);
}
"#;

const CONE_FRAGMENT: &str = r#"
vec4 ferravex_fragment(vec4 color, vec3 world, vec3 normal) {
    vec2 d = world.xz - u_params[1].xz;
    float r = length(d) / max(u_params[1].w, 0.01);
    float inside = 1.0 - smoothstep(0.95, 1.0, r);
    float near = smoothstep(0.03, 0.14, r);
    // Bright rims on the arc and both straight edges keep the cone readable.
    float angle = acos(clamp(dot(normalize(d + vec2(1e-5)), u_params[2].yz), -1.0, 1.0));
    float side = smoothstep(u_params[2].w - 0.1, u_params[2].w - 0.02, angle);
    float arc = smoothstep(0.84, 0.94, r);
    float rim = max(side, arc);
    float bands = 0.72 + 0.28 * step(0.5, fract(r * 6.0 - u_params[2].x * 0.7));
    float a = u_params[0].a * near * inside * mix(bands, 2.2, rim);
    return vec4(mix(u_params[0].rgb, vec3(1.0), rim * 0.25), clamp(a, 0.0, 0.9));
}
"#;

const WATER_FRAGMENT: &str = r#"
vec4 ferravex_fragment(vec4 color, vec3 world, vec3 normal) {
    float t = u_params[0].x;
    float w = sin(world.x * 2.3 + t * 1.4) * sin(world.z * 1.9 - t * 1.1)
            + 0.5 * sin((world.x + world.z) * 3.7 + t * 2.3);
    float glint = smoothstep(1.05, 1.45, w) * u_params[0].y;
    return vec4(color.rgb * (0.9 + 0.08 * w) + vec3(1.6, 1.35, 0.9) * glint, 1.0);
}
"#;

const JELLY_VERTEX: &str = r#"
vec3 ferravex_vertex(vec3 p) {
    float a = u_params[0].y;
    float t = u_params[0].x * 17.0;
    float h = p.y + 0.15;
    p.x *= 1.0 + sin(t) * a * h * 2.2;
    p.z *= 1.0 + cos(t) * a * h * 2.2;
    p.y *= 1.0 - sin(t) * a * 0.7;
    return p;
}
vec3 ferravex_normal(vec3 normal) { return normal; }
"#;

const GHOST_FRAGMENT: &str = r#"
vec4 ferravex_fragment(vec4 color, vec3 world, vec3 normal) {
    float l = dot(color.rgb, vec3(0.3, 0.55, 0.15));
    float rim = 0.5 + 0.5 * sin(world.y * 34.0 + u_params[0].x * 5.0);
    return vec4(vec3(0.6, 0.86, 1.0) * (0.55 + l * 0.9) + rim * 0.1, 0.42);
}
"#;

fn material(vertex: Option<&str>, fragment: Option<&str>, opts: MaterialOptions) -> ShaderMaterial {
    let mut source = ShaderSource::default();
    if let Some(v) = vertex {
        source.vertex = v.into();
    }
    if let Some(f) = fragment {
        source.fragment = f.into();
    }
    ShaderMaterial::new(source, opts).expect("authored material")
}

pub struct Materials {
    pub sway: ShaderMaterial,
    pub grass: ShaderMaterial,
    pub glow: ShaderMaterial,
    pub cone: ShaderMaterial,
    pub water: ShaderMaterial,
    pub jelly: ShaderMaterial,
    pub ghost: ShaderMaterial,
}

impl Materials {
    fn new() -> Self {
        let opaque = MaterialOptions {
            blend: BlendMode::Opaque,
            bounds_padding: 0.6,
            ..MaterialOptions::default()
        };
        Self {
            sway: material(Some(SWAY_VERTEX), None, opaque),
            grass: material(
                Some(SWAY_VERTEX),
                None,
                MaterialOptions {
                    casts_shadows: false,
                    bounds_padding: 0.3,
                    ..opaque
                },
            ),
            glow: material(
                None,
                Some(GLOW_FRAGMENT),
                MaterialOptions {
                    blend: BlendMode::Opaque,
                    casts_shadows: false,
                    receives_shadows: false,
                    bounds_padding: 0.,
                },
            ),
            cone: material(
                None,
                Some(CONE_FRAGMENT),
                MaterialOptions {
                    blend: BlendMode::Alpha,
                    casts_shadows: false,
                    receives_shadows: false,
                    bounds_padding: 0.,
                },
            ),
            water: material(
                None,
                Some(WATER_FRAGMENT),
                MaterialOptions {
                    casts_shadows: false,
                    ..opaque
                },
            ),
            jelly: material(Some(JELLY_VERTEX), None, opaque),
            ghost: material(
                None,
                Some(GHOST_FRAGMENT),
                MaterialOptions {
                    blend: BlendMode::Alpha,
                    casts_shadows: false,
                    receives_shadows: false,
                    bounds_padding: 0.,
                },
            ),
        }
    }
}

struct Static {
    pieces: Vec<Builder>,
}

impl Static {
    fn piece(&mut self) -> &mut Builder {
        self.pieces.push(Builder::new());
        self.pieces.last_mut().unwrap()
    }
}

fn noise(x: f32, z: f32) -> f32 {
    (x * 0.31).sin() * (z * 0.27).cos() * 0.5
        + (x * 0.83 + z * 0.61).sin() * 0.3
        + (z * 1.7 - x * 0.4).sin() * 0.2
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0., 1.);
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        1.,
    )
}

fn ground(game: &Game, s: &mut Static) {
    let grass = rgb(0x86b85a);
    let lush = rgb(0x6fa64d);
    let dry = rgb(0xa9c36a);
    let path = rgb(0xe3c995);
    let paths: Vec<_> = game
        .park
        .decos
        .iter()
        .filter(|d| d.kind == DecoKind::Path)
        .collect();
    let color = |x: f32, z: f32| {
        let n = noise(x, z);
        let mut c = if n > 0. {
            mix(grass, lush, n * 1.6)
        } else {
            mix(grass, dry, -n * 1.4)
        };
        for p in &paths {
            let dx = ((x - p.at.x).abs() - p.size.x * 0.5).max(0.);
            let dz = ((z - p.at.z).abs() - p.size.z * 0.5).max(0.);
            let d = (dx * dx + dz * dz).sqrt();
            let wobble = 0.25 * (x * 1.3 + z * 0.7).sin();
            if d < 0.6 + wobble {
                c = mix(path, c, (d - 0.1 + wobble).max(0.) * 1.6);
            }
        }
        let pond = game.park.zone(Role::Pond);
        let dp = flat(vec3(x, 0., z) - pond.at).length() - pond.radius;
        if dp < 1.2 {
            c = mix(rgb(0x8f8f52), c, (dp + 0.2).max(0.) / 1.4);
        }
        c
    };
    // 4 × 4 chunks of 15 m, one quad per metre; outside the hedge a wider apron.
    for cz in 0..4 {
        for cx in 0..4 {
            let b = s.piece();
            for z in 0..15 {
                for x in 0..15 {
                    let x0 = -30. + (cx * 15 + x) as f32;
                    let z0 = -30. + (cz * 15 + z) as f32;
                    let p = [
                        vec3(x0, 0., z0 + 1.),
                        vec3(x0 + 1., 0., z0 + 1.),
                        vec3(x0 + 1., 0., z0),
                        vec3(x0, 0., z0),
                    ];
                    let n = Vec3::Y;
                    let base = b.mesh.vertices.len() as u16;
                    for q in p {
                        let mut v = Vertex::new2(q, Vec2::ZERO, color(q.x, q.z));
                        v.normal = n.extend(0.);
                        b.mesh.vertices.push(v);
                    }
                    b.mesh.indices.extend_from_slice(&[
                        base,
                        base + 1,
                        base + 2,
                        base,
                        base + 2,
                        base + 3,
                    ]);
                }
            }
        }
    }
    // Rolling countryside beyond the hedge.
    let b = s.piece();
    b.cube(vec3(0., -0.06, 0.), vec3(220., 0.1, 220.), rgb(0x7caa52));
    for (x, z, r, h, c) in [
        (-70., -60., 38., 9., 0x7fae58),
        (10., -85., 52., 14., 0x86b45c),
        (75., -50., 40., 11., 0x78a752),
        (85., 30., 45., 8., 0x83b05a),
        (-80., 35., 42., 10., 0x7aa955),
        (0., 90., 60., 9., 0x80ad57),
        (-35., -95., 30., 16., 0x6f9f4e),
    ] {
        b.blob(vec3(x, -h * 0.25, z), vec3(r, h, r * 0.7), rgb(c), 4);
    }
}

fn hedge(b: &mut Builder, center: Vec3, half: Vec3) {
    let green = rgb(0x62a050);
    b.cube(center, half * 2., green);
    let along_x = half.x > half.z;
    let len = if along_x { half.x } else { half.z } * 2.;
    let n = (len / 1.3) as i32;
    for k in 0..=n {
        let t = -len * 0.5 + k as f32 * len / n.max(1) as f32;
        let p = if along_x {
            vec3(center.x + t, center.y + half.y, center.z)
        } else {
            vec3(center.x, center.y + half.y, center.z + t)
        };
        let r = 0.55 + 0.15 * (k as f32 * 1.7).sin();
        b.blob(
            p,
            vec3(r, r * 0.6, r),
            shade(green, 0.95 + 0.1 * (k as f32 * 0.9).sin()),
            3,
        );
    }
}

fn table(b: &mut Builder, t: &crate::sim::park::Table, cloth_on: bool) {
    let wood = rgb(0xa8774a);
    let top = t.center + Vec3::Y * (t.height - t.top.y * 0.5);
    b.cube(top, t.top, wood);
    let leg = t.height - t.top.y;
    for (sx, sz) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
        b.cube(
            t.center
                + vec3(
                    sx * (t.top.x * 0.5 - 0.14),
                    leg * 0.5,
                    sz * (t.top.z * 0.5 - 0.1),
                ),
            vec3(0.08, leg, 0.08),
            shade(wood, 0.85),
        );
    }
    if !cloth_on {
        return;
    }
    // Gingham cloth: checks on top, a draped skirt with trim on every side.
    let cloth = Color::from_rgba(t.cloth[0], t.cloth[1], t.cloth[2], 255);
    let trim = Color::from_rgba(t.trim[0], t.trim[1], t.trim[2], 255);
    let y = t.center.y + t.height + 0.006;
    let (w, d) = (t.top.x + 0.12, t.top.z + 0.12);
    let nx = (w / 0.2).round() as i32;
    let nz = (d / 0.2).round() as i32;
    for i in 0..nx {
        for j in 0..nz {
            let x0 = t.center.x - w * 0.5 + i as f32 * w / nx as f32;
            let z0 = t.center.z - d * 0.5 + j as f32 * d / nz as f32;
            let (x1, z1) = (x0 + w / nx as f32, z0 + d / nz as f32);
            let c = match (i + j) % 2 {
                0 => cloth,
                _ => mix(cloth, trim, 0.62),
            };
            b.quad(
                [
                    vec3(x0, y, z1),
                    vec3(x1, y, z1),
                    vec3(x1, y, z0),
                    vec3(x0, y, z0),
                ],
                c,
            );
        }
    }
    let drop = 0.36_f32.min(t.height - 0.42);
    for (dx, dz, len, along_x) in [
        (0., d * 0.5, w, true),
        (0., -d * 0.5, w, true),
        (w * 0.5, 0., d, false),
        (-w * 0.5, 0., d, false),
    ] {
        let c = vec3(t.center.x + dx, y - drop * 0.5, t.center.z + dz);
        let size = if along_x {
            vec3(len, drop, 0.012)
        } else {
            vec3(0.012, drop, len)
        };
        b.cube(c, size, shade(cloth, 0.92));
        let tc = vec3(c.x, y - drop + 0.03, c.z);
        let tsize = if along_x {
            vec3(len + 0.01, 0.05, 0.02)
        } else {
            vec3(0.02, 0.05, len + 0.01)
        };
        b.cube(tc, tsize, trim);
    }
}

fn cottage(b: &mut Builder, at: Vec3, w: f32, d: f32, wall: u32, roof: u32, yaw: f32) {
    let r = Quat::from_rotation_y(yaw);
    let h = 2.6;
    b.box_rot(at + Vec3::Y * h * 0.5, vec3(w, h, d), r, rgb(wall));
    // Pitched roof from two slabs.
    for s in [-1., 1.] {
        let tilt = Quat::from_rotation_z(s * 0.62);
        b.box_rot(
            at + r * vec3(s * w * 0.25, h + 0.75, 0.),
            vec3(w * 0.62, 0.14, d + 0.3),
            r * tilt,
            rgb(roof),
        );
    }
    b.box_rot(
        at + r * vec3(w * 0.25, h + 1.3, d * 0.2),
        vec3(0.35, 0.9, 0.35),
        r,
        rgb(0x8c5a46),
    );
    for k in [-1., 1.] {
        b.box_rot(
            at + r * vec3(k * w * 0.25, h * 0.55, -d * 0.5 - 0.01),
            vec3(0.5, 0.55, 0.04),
            r,
            rgb(0xf6e7a8),
        );
    }
    b.box_rot(
        at + r * vec3(0., 0.6, -d * 0.5 - 0.01),
        vec3(0.55, 1.2, 0.04),
        r,
        rgb(0x7a4b3a),
    );
}

fn statics(game: &Game, s: &mut Static) {
    ground(game, s);
    let park = &game.park;
    for solid in &park.solids {
        let c = Color::from_rgba(solid.color[0], solid.color[1], solid.color[2], 255);
        let b = s.piece();
        match solid.look {
            Look::Hedge => hedge(b, solid.center, solid.half),
            Look::Hidden => {}
            Look::Bandstand => {
                let at = solid.center;
                b.frustum(at - Vec3::Y * 0.11, 3.45, 3.3, 0.22, c, 8);
                // A red skirt band whose cap hides under the floor.
                b.frustum(at - Vec3::Y * 0.02, 3.47, 3.36, 0.1, rgb(0xc94f4f), 8);
                for k in 0..8 {
                    let a = (k as f32 + 0.5) / 8. * std::f32::consts::TAU;
                    let p = at + vec3(a.cos() * 3.05, 1.7, a.sin() * 3.05);
                    b.cube(p, vec3(0.14, 3.2, 0.14), rgb(0xf4ecdc));
                    // Little railing.
                    let a2 = (k as f32 + 1.5) / 8. * std::f32::consts::TAU;
                    let q = at + vec3(a2.cos() * 3.05, 0.65, a2.sin() * 3.05);
                    let mid = (vec3(p.x, 0.65, p.z) + q) * 0.5;
                    if k != 6 {
                        b.box_rot(
                            mid,
                            vec3(flat(p).distance(flat(q)), 0.06, 0.06),
                            Quat::from_rotation_y(-(q.z - p.z).atan2(q.x - p.x)),
                            rgb(0xf4ecdc),
                        );
                    }
                }
                b.frustum(at + Vec3::Y * 3.25, 3.9, 0.2, 1.6, rgb(0xd65a4e), 8);
                b.frustum(at + Vec3::Y * 3.2, 3.95, 3.9, 0.1, rgb(0xf7f1e6), 8);
                b.blob(at + Vec3::Y * 4.95, Vec3::splat(0.16), rgb(0xf2c34b), 2);
            }
            Look::MusicStand => {
                let at = solid.center;
                b.cube(
                    vec3(at.x, (at.y + 0.22) * 0.5, at.z),
                    vec3(0.04, at.y - 0.22, 0.04),
                    c,
                );
                b.box_rot(
                    at + vec3(0., 0.02, 0.),
                    solid.half * 2.,
                    Quat::from_rotation_x(-0.25),
                    c,
                );
            }
            Look::Bench => {
                b.cube(solid.center, solid.half * 2., c);
                for dz in [-0.7, 0.7] {
                    b.cube(
                        vec3(solid.center.x, 0.2, solid.center.z + dz),
                        vec3(0.45, 0.4, 0.08),
                        shade(c, 0.8),
                    );
                }
            }
            Look::Toybox => {
                b.cube(solid.center, solid.half * 2., c);
                b.cube(
                    solid.center + Vec3::Y * (solid.half.y + 0.02),
                    vec3(solid.half.x * 2. + 0.06, 0.05, solid.half.z * 2. + 0.06),
                    rgb(0xf2c94c),
                );
                b.cube(
                    solid.center + vec3(0., 0., -solid.half.z - 0.01),
                    vec3(0.3, 0.12, 0.02),
                    rgb(0xe8607a),
                );
            }
            Look::Sandbox => {
                b.cube(solid.center, solid.half * 2., rgb(0xead39b));
                for (dx, dz, w, d) in [
                    (0., solid.half.z, solid.half.x * 2. + 0.2, 0.12),
                    (0., -solid.half.z, solid.half.x * 2. + 0.2, 0.12),
                    (solid.half.x, 0., 0.12, solid.half.z * 2.),
                    (-solid.half.x, 0., 0.12, solid.half.z * 2.),
                ] {
                    b.cube(
                        solid.center + vec3(dx, 0.06, dz),
                        vec3(w, 0.28, d),
                        rgb(0xb5834f),
                    );
                }
                b.frustum(
                    solid.center + vec3(0.6, 0.08, 0.4),
                    0.45,
                    0.05,
                    0.3,
                    rgb(0xdfc287),
                    6,
                );
                b.cube(
                    solid.center + vec3(-0.8, 0.2, -0.5),
                    vec3(0.18, 0.2, 0.18),
                    rgb(0xd9534f),
                );
            }
            Look::Hay => {
                b.cube(solid.center, solid.half * 2., c);
                for dz in [-0.3, 0.3] {
                    b.cube(
                        solid.center + vec3(0., 0., dz),
                        vec3(solid.half.x * 2. + 0.02, solid.half.y * 2. + 0.02, 0.05),
                        rgb(0xb98a3c),
                    );
                }
            }
            Look::Grass | Look::Ramp | Look::Plain | Look::Stool => {
                b.box_rot(solid.center, solid.half * 2., solid.rotation, c);
            }
        }
    }
    for ramp in &park.ramps {
        let b = s.piece();
        b.box_rot(ramp.center, ramp.half * 2., ramp.rotation, rgb(0x7fb257));
        // Mown stripes down the slope.
        for k in 0..7 {
            let x = -ramp.half.x + (k as f32 + 0.5) * ramp.half.x * 2. / 7.;
            let top = ramp.center + ramp.rotation * vec3(x, ramp.half.y + 0.004, 0.);
            if k % 2 == 0 {
                b.box_rot(
                    top,
                    vec3(ramp.half.x * 2. / 7., 0.004, ramp.half.z * 2.),
                    ramp.rotation,
                    rgb(0x8cbe60),
                );
            }
        }
    }
    for (i, t) in park.tables.iter().enumerate() {
        // The banquet's tablecloth is a live prop for the finale.
        let banquet = i == banquet_index(game);
        table(s.piece(), t, !banquet);
    }
    for tree in &park.trees {
        let b = s.piece();
        let k = tree.scale;
        match tree.kind {
            TreeKind::Oak => {
                b.frustum(tree.at, 0.3 * k, 0.2 * k, 2.6 * k, rgb(0x7a5638), 7);
                b.box_rot(
                    tree.at + vec3(0.35, 2.0, 0.) * k,
                    vec3(0.9, 0.16, 0.16) * k,
                    Quat::from_rotation_z(0.7),
                    rgb(0x7a5638),
                );
            }
            TreeKind::Pine => {
                b.frustum(tree.at, 0.22 * k, 0.15 * k, 1.4 * k, rgb(0x6e4a32), 6);
            }
            TreeKind::Birch => {
                b.frustum(tree.at, 0.16 * k, 0.12 * k, 3.0 * k, rgb(0xf0ede6), 6);
                for j in 0..5 {
                    b.cube(
                        tree.at + vec3(0., 0.4 + j as f32 * 0.55, 0.) * k,
                        vec3(0.33, 0.05, 0.33) * k,
                        rgb(0x3a3838),
                    );
                }
            }
        }
    }
    for (p, h) in &park.poles {
        let b = s.piece();
        b.frustum(*p, 0.07, 0.05, *h, rgb(0xf4ecdc), 6);
        b.blob(
            *p + Vec3::Y * (h + 0.06),
            Vec3::splat(0.1),
            rgb(0xf2c34b),
            2,
        );
    }
    for d in &park.decos {
        let b = s.piece();
        match d.kind {
            DecoKind::BellArch => {
                let wood = rgb(0x8a5c3a);
                for x in [-1.4, 1.4] {
                    b.cube(d.at + vec3(x, 1.65, 0.), vec3(0.24, 3.3, 0.24), wood);
                }
                b.cube(d.at + vec3(0., 3.35, 0.), vec3(3.4, 0.24, 0.32), wood);
                for s2 in [-1., 1.] {
                    b.box_rot(
                        d.at + vec3(s2 * 0.95, 3.85, 0.),
                        vec3(2.1, 0.12, 0.7),
                        Quat::from_rotation_z(s2 * -0.45),
                        rgb(0xb9473f),
                    );
                }
                // Rope.
                b.cube(
                    d.at + vec3(1.05, 1.9, -0.6),
                    vec3(0.04, 2.3, 0.04),
                    rgb(0xd9c08a),
                );
                b.cube(
                    d.at + vec3(1.05, 0.7, -0.6),
                    vec3(0.09, 0.18, 0.09),
                    rgb(0xd9534f),
                );
                b.cube(
                    d.at + vec3(0.55, 3.05, -0.3),
                    vec3(1.0, 0.03, 0.03),
                    rgb(0xd9c08a),
                );
            }
            DecoKind::Sign => {
                b.cube(
                    d.at + vec3(0., 1.35, 0.),
                    vec3(0.14, 2.7, 0.14),
                    rgb(0x8a5c3a),
                );
                b.cube(
                    d.at + vec3(0.42, 2.6, 0.),
                    vec3(0.95, 0.1, 0.1),
                    rgb(0x8a5c3a),
                );
            }
            DecoKind::Seesaw => {
                for z in [-0.32, 0.32] {
                    b.box_rot(
                        d.at + vec3(-0.18, 0.3, z),
                        vec3(0.1, 0.72, 0.1),
                        Quat::from_rotation_z(0.45),
                        rgb(0xd9534f),
                    );
                    b.box_rot(
                        d.at + vec3(0.18, 0.3, z),
                        vec3(0.1, 0.72, 0.1),
                        Quat::from_rotation_z(-0.45),
                        rgb(0xd9534f),
                    );
                }
                b.cube(
                    d.at + vec3(0., 0.6, 0.),
                    vec3(0.12, 0.12, 0.8),
                    rgb(0x3f4552),
                );
            }
            DecoKind::Flowers => {
                b.cube(
                    d.at + Vec3::Y * 0.06,
                    vec3(d.size.x, 0.12, d.size.z),
                    rgb(0x7a5638),
                );
                for (dx, dz) in [(1., 1.), (1., -1.), (-1., 1.), (-1., -1.)] {
                    let _ = (dx, dz);
                }
                b.cube(
                    d.at + vec3(0., 0.08, 0.),
                    vec3(d.size.x + 0.16, 0.1, d.size.z + 0.16),
                    rgb(0xc9b9a0),
                );
            }
            DecoKind::Tent => {
                let r = d.size;
                for (sx, sz) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
                    b.cube(
                        d.at + vec3(sx * r.x * 0.5, r.y * 0.5, sz * r.z * 0.5),
                        vec3(0.1, r.y, 0.1),
                        rgb(0xf4ecdc),
                    );
                }
                let stripes = 8;
                for k in 0..stripes {
                    let x0 = -r.x * 0.5 + k as f32 * r.x / stripes as f32;
                    let c = if k % 2 == 0 {
                        rgb(0x8e5aa8)
                    } else {
                        rgb(0xf7f1e6)
                    };
                    for s2 in [-1., 1.] {
                        b.box_rot(
                            d.at + vec3(
                                x0 + r.x / stripes as f32 * 0.5,
                                r.y + 0.42,
                                s2 * r.z * 0.25,
                            ),
                            vec3(r.x / stripes as f32, 0.06, r.z * 0.56),
                            Quat::from_rotation_x(s2 * 0.55),
                            c,
                        );
                    }
                    // Scalloped valance.
                    b.cube(
                        d.at + vec3(
                            x0 + r.x / stripes as f32 * 0.5,
                            r.y - 0.05,
                            -r.z * 0.5 - 0.02,
                        ),
                        vec3(r.x / stripes as f32, 0.22, 0.02),
                        c,
                    );
                }
                // A banner: PRIZE VEGETABLES (as colourful blocks).
                b.cube(
                    d.at + vec3(0., r.y + 0.95, 0.),
                    vec3(1.6, 0.3, 0.04),
                    rgb(0xf3c64a),
                );
                // Prize veg decorations and rosettes on the table.
                for (x, c, s2) in [
                    (-1.1, 0xe8892d, 0.24),
                    (-0.6, 0xe8892d, 0.18),
                    (0.7, 0xd94a4a, 0.14),
                    (1.15, 0x9cc46a, 0.16),
                ] {
                    b.blob(
                        d.at + vec3(x, 0.84 + s2 * 0.8, -0.1),
                        Vec3::splat(s2),
                        rgb(c),
                        3,
                    );
                }
                b.frustum(
                    d.at + vec3(0., 0.84, 0.),
                    0.33,
                    0.33,
                    0.06,
                    rgb(0x6d2b74),
                    10,
                );
                b.blob(
                    d.at + vec3(0.42, 1.14, 0.48),
                    vec3(0.09, 0.09, 0.02),
                    rgb(0x3f7fc1),
                    2,
                );
                b.cube(
                    d.at + vec3(0.42, 1.0, 0.485),
                    vec3(0.06, 0.14, 0.01),
                    rgb(0x3f7fc1),
                );
            }
            DecoKind::Awning => {
                let r = d.size;
                for (sx, sz) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
                    b.cube(
                        d.at + vec3(sx * r.x * 0.5, r.y * 0.5, sz * r.z * 0.5),
                        vec3(0.08, r.y, 0.08),
                        rgb(0xf4ecdc),
                    );
                }
                for k in 0..6 {
                    let x0 = -r.x * 0.5 + (k as f32 + 0.5) * r.x / 6.;
                    let c = if k % 2 == 0 {
                        rgb(0x5aa86a)
                    } else {
                        rgb(0xf7f1e6)
                    };
                    b.box_rot(
                        d.at + vec3(x0, r.y + 0.12, 0.),
                        vec3(r.x / 6., 0.05, r.z + 0.5),
                        Quat::from_rotation_x(0.22),
                        c,
                    );
                }
                for (x, z) in [(-0.6, 0.1), (-0.3, -0.2), (0.5, 0.2)] {
                    b.blob(d.at + vec3(x, 0.92, z), Vec3::splat(0.08), rgb(0xd9352f), 2);
                }
                b.cube(
                    d.at + vec3(0.8, 0.95, 0.),
                    vec3(0.4, 0.2, 0.3),
                    rgb(0xf2c94c),
                );
            }
            DecoKind::Path => {}
        }
    }
    // Burrows: a dark hole in the hedge and a very honest sign.
    for (i, den) in park.dens.iter().enumerate() {
        let b = s.piece();
        let toward = flat(*den).normalize_or_zero();
        let hole = *den + toward * 2.1;
        b.blob(
            hole + Vec3::Y * 0.2,
            vec3(0.75, 0.6, 0.75),
            rgb(0x2a2a24),
            3,
        );
        b.blob(
            hole + Vec3::Y * 0.05 - toward * 0.4,
            vec3(0.9, 0.12, 0.9),
            rgb(0x6b5232),
            3,
        );
        if i == 0 {
            let side = vec3(-toward.z, 0., toward.x);
            let post = *den + toward * 0.9 + side * 1.3;
            b.cube(post + Vec3::Y * 0.4, vec3(0.06, 0.8, 0.06), rgb(0x8a5c3a));
            b.box_rot(
                post + Vec3::Y * 0.78,
                vec3(0.6, 0.3, 0.04),
                Quat::from_rotation_y(-crate::sim::yaw_of(toward)),
                rgb(0xf2e2bf),
            );
        }
    }
    // Lily pads, reeds and stones round the duck pond.
    let pond = park.zone(Role::Pond);
    let b = s.piece();
    for k in 0..14 {
        let a = k as f32 / 14. * std::f32::consts::TAU;
        let r = pond.radius + 0.25;
        b.blob(
            pond.at + vec3(a.cos() * r, 0.05, a.sin() * r),
            vec3(0.35, 0.16, 0.28),
            rgb(if k % 3 == 0 { 0xb4ad9c } else { 0x9f9a8c }),
            2,
        );
        if k % 2 == 0 {
            let p = pond.at + vec3(a.cos() * (r - 0.6), 0., a.sin() * (r - 0.6));
            for j in 0..3 {
                b.cube(
                    p + vec3(j as f32 * 0.08, 0.35, 0.),
                    vec3(0.03, 0.7 + j as f32 * 0.1, 0.03),
                    rgb(0x5c8a3c),
                );
            }
        }
    }
    // The picnic basket (a wicker box with a lid) beside the blue table.
    let basket = park.zone(Role::Basket);
    let b = s.piece();
    b.cube(
        basket.at + Vec3::Y * 0.22,
        vec3(0.62, 0.44, 0.46),
        rgb(0xc9955a),
    );
    for k in 0..4 {
        b.cube(
            basket.at + vec3(0., 0.08 + k as f32 * 0.1, 0.),
            vec3(0.64, 0.025, 0.48),
            rgb(0xa8763f),
        );
    }
    b.cube(
        basket.at + vec3(0., 0.55, 0.),
        vec3(0.04, 0.25, 0.04),
        rgb(0xa8763f),
    );
    // The distant village: cottages, a church and some far trees.
    let b = s.piece();
    for (x, z, w, d, wall, roof, yaw) in [
        (-40., -38., 5., 4., 0xf2e6cf, 0xb9473f, 0.3),
        (-46., -24., 4., 4., 0xe9d6b0, 0x6d6a73, 1.2),
        (-38., 42., 5., 4., 0xf7efe2, 0x9c5a3c, -0.4),
        (40., -40., 6., 4., 0xefe2c8, 0x5f7b8f, -0.8),
        (46., 12., 4., 5., 0xf4ead7, 0xb9473f, 1.6),
        (42., 40., 5., 4., 0xe6d3ad, 0x7a4b3a, 0.9),
        (-12., 44., 4., 4., 0xf2e6cf, 0x6d6a73, 0.1),
        (14., 46., 5., 4., 0xefe2c8, 0xb9473f, -0.2),
    ] {
        cottage(b, vec3(x, 0., z), w, d, wall, roof, yaw);
    }
    let church = vec3(-6., 0., -48.);
    b.cube(church + vec3(0., 2.5, 0.), vec3(5., 5., 9.), rgb(0xd8d2c4));
    b.box_rot(
        church + vec3(-1.4, 6.0, 0.),
        vec3(3.4, 0.2, 9.4),
        Quat::from_rotation_z(0.75),
        rgb(0x6d6a73),
    );
    b.box_rot(
        church + vec3(1.4, 6.0, 0.),
        vec3(3.4, 0.2, 9.4),
        Quat::from_rotation_z(-0.75),
        rgb(0x6d6a73),
    );
    b.cube(
        church + vec3(0., 5.5, 5.),
        vec3(2.6, 11., 2.6),
        rgb(0xd8d2c4),
    );
    b.frustum(church + vec3(0., 11., 5.), 1.9, 0.05, 5., rgb(0x6d6a73), 4);
    b.cube(
        church + vec3(0., 8.5, 3.69),
        vec3(1.1, 1.1, 0.05),
        rgb(0xf3e3a0),
    );
    for k in 0..22 {
        let a = k as f32 * 2.399;
        let r = 36. + (k % 5) as f32 * 4.5;
        let p = vec3(a.cos() * r, 0., a.sin() * r);
        if p.x.abs() < 33. && p.z.abs() < 33. {
            continue;
        }
        if k % 3 == 0 {
            b.frustum(p, 1.6, 0.05, 5.5, rgb(0x3f6e3c), 6);
        } else {
            b.frustum(p, 0.25, 0.2, 1.4, rgb(0x6e4a32), 5);
            b.blob(p + Vec3::Y * 2.6, vec3(1.8, 1.6, 1.8), rgb(0x5f9446), 3);
        }
    }
}

pub fn banquet_index(game: &Game) -> usize {
    game.park
        .tables
        .iter()
        .position(|t| t.top.x > 5.)
        .unwrap_or(0)
}

fn tree_canopy(kind: TreeKind, detail: u16) -> Mesh3d {
    let mut b = Builder::new();
    match kind {
        TreeKind::Oak => {
            b.blob(
                vec3(0., 3.4, 0.),
                vec3(2.0, 1.55, 2.0),
                rgb(0x5d9a43),
                detail,
            );
            if detail > 2 {
                b.blob(
                    vec3(0.9, 3.1, 0.6),
                    vec3(1.2, 1.0, 1.2),
                    rgb(0x6aa74c),
                    detail,
                );
                b.blob(
                    vec3(-0.8, 4.2, -0.5),
                    vec3(1.1, 0.9, 1.1),
                    rgb(0x76b155),
                    detail,
                );
            }
        }
        TreeKind::Pine => {
            for k in 0..4 {
                let y = 1.2 + k as f32 * 0.95;
                let r = 1.55 - k as f32 * 0.32;
                b.frustum(
                    vec3(0., y, 0.),
                    r,
                    0.04,
                    1.55,
                    shade(rgb(0x3d7a48), 1. + k as f32 * 0.05),
                    if detail > 2 { 8 } else { 5 },
                );
            }
        }
        TreeKind::Birch => {
            b.blob(
                vec3(0., 3.3, 0.),
                vec3(1.25, 1.35, 1.25),
                rgb(0x9cc95a),
                detail,
            );
            if detail > 2 {
                b.blob(
                    vec3(0.5, 2.7, 0.3),
                    vec3(0.8, 0.7, 0.8),
                    rgb(0xadd36a),
                    detail,
                );
            }
        }
    }
    b.into_mesh()
}

fn flag(color: Color) -> Mesh3d {
    let mut b = Builder::new();
    let (a, c, d) = (vec3(-0.17, 0., 0.), vec3(0.17, 0., 0.), vec3(0., -0.36, 0.));
    b.tri(a, c, d, color);
    b.tri(c, a, d, shade(color, 0.85));
    b.into_mesh()
}

pub struct Actors {
    pub quad: QuadRig,
    pub biped: BipedRig,
    pub coon: QuadMotion,
    pub ghost: QuadMotion,
    pub folk: [Option<BipedMotion>; 5],
    pub dog: QuadMotion,
}

impl Default for Actors {
    fn default() -> Self {
        Self::new()
    }
}

impl Actors {
    pub fn new() -> Self {
        let quad = QuadRig::new();
        let biped = BipedRig::new();
        Self {
            coon: QuadMotion::new(&quad),
            ghost: QuadMotion::new(&quad),
            dog: QuadMotion::new(&quad),
            folk: std::array::from_fn(|i| (i < 4).then(|| BipedMotion::new(&biped))),
            quad,
            biped,
        }
    }

    pub fn reset(&mut self) {
        self.coon.reset();
        self.dog.reset();
        self.ghost.reset();
    }

    /// Advance presentation animation from accepted state. Returns
    /// (raccoon footsteps, picnicker footsteps) this frame.
    pub fn update(&mut self, game: &Game, dt: f32, ghost_speed: Option<f32>) -> (usize, usize) {
        let c = &game.coon;
        self.coon.update(
            &self.quad,
            QuadInput {
                speed: c.speed,
                grounded: c.motor.grounded || c.hide == crate::sim::raccoon::Hide::Basket,
                sliding: c.slide > 0. || c.flop,
                sneaking: c.sneaking,
                scruffed: c.scruffed.is_some(),
                celebrate: c.celebrate > 0. || game.is_over(),
                carrying: c.held.is_some(),
                chitter: c.chitter_age,
                wag: false,
                sit: c.hide == crate::sim::raccoon::Hide::Basket,
            },
            dt,
        );
        if let Some(speed) = ghost_speed {
            self.ghost.update(
                &self.quad,
                QuadInput {
                    speed,
                    grounded: true,
                    ..QuadInput::default()
                },
                dt,
            );
        }
        let mut steps = 0;
        for (i, f) in game.folk.iter().enumerate() {
            if let Some(m) = &mut self.folk[i] {
                m.update(&self.biped, f.kind, f.pose, f.speed, dt);
                steps += m.steps();
            }
        }
        let dog = &game.folk[4];
        let happy = matches!(dog.mode, Mode::React(_)) || dog.content > 0.;
        self.dog.update(
            &self.quad,
            QuadInput {
                speed: dog.speed,
                grounded: true,
                sneaking: false,
                celebrate: matches!(dog.pose, Pose::Cheer),
                chitter: if dog.bubble == crate::sim::folk::Bubble::Alarm {
                    dog.bubble_age % 0.6
                } else {
                    10.
                },
                wag: happy || dog.speed > 0.2 || dog.mode == Mode::Chase,
                sit: matches!(dog.pose, Pose::Act(crate::sim::park::Act::Beg)),
                ..QuadInput::default()
            },
            dt,
        );
        (self.coon.steps(), steps + self.dog.steps())
    }
}

pub struct Scene {
    pub mat: Materials,
    chunks: Vec<Mesh3d>,
    canopy: [LodChain; 3],
    lod: Vec<LodState>,
    grass: Mesh3d,
    grass_at: Vec<Mat4>,
    flowers: [Mesh3d; 3],
    flower_at: Vec<(usize, Mat4)>,
    bush: Mesh3d,
    lantern: Mesh3d,
    flags: [Mesh3d; 5],
    string: Mesh3d,
    seesaw: Mesh3d,
    bell: Mesh3d,
    sign: Mesh3d,
    props: Vec<Mesh3d>,
    shiny: Mesh3d,
    water: Mesh3d,
    cones: [Mesh3d; 3],
    sun: Mesh3d,
    cloud: Mesh3d,
    cloth: Mesh3d,
    cape: Mesh3d,
    basket_lid: Mesh3d,
    raccoon: SkinnedMesh,
    folk: [SkinnedMesh; 4],
    dog: SkinnedMesh,
    hats: Vec<Option<Mesh3d>>,
    crown: Mesh3d,
    duck: Mesh3d,
    pub triangles: usize,
}

fn fan(fov: f32) -> Mesh3d {
    let mut b = Builder::new();
    let n = 18;
    let c = Color::new(1., 1., 1., 1.);
    for k in 0..n {
        let a0 = -fov * 0.5 + fov * k as f32 / n as f32;
        let a1 = -fov * 0.5 + fov * (k + 1) as f32 / n as f32;
        let p = |a: f32| vec3(a.sin(), 0., -a.cos());
        b.tri(Vec3::ZERO, p(a1), p(a0), c);
    }
    let mut m = b.into_mesh();
    for v in &mut m.mesh.vertices {
        v.color[3] = 255;
    }
    m
}

fn kind_index(kind: PropKind) -> usize {
    kind as usize
}

impl Scene {
    pub fn new(game: &Game) -> Self {
        let mut s = Static { pieces: Vec::new() };
        statics(game, &mut s);
        // Bake ambient occlusion from every static piece, then chunk.
        let triangles: usize = s.pieces.iter().map(|b| b.mesh.indices.len() / 3).sum();
        if let Ok(ao) = AoScene::new(s.pieces.iter().map(|b| (&b.mesh, Mat4::IDENTITY))) {
            for b in &mut s.pieces {
                let _ = ao.bake(
                    &mut b.mesh,
                    Mat4::IDENTITY,
                    AoSettings {
                        samples: 6,
                        radius: 1.6,
                        strength: 0.42,
                        max_tests: 40_000_000,
                        ..AoSettings::default()
                    },
                );
            }
        }
        let chunk_of = |b: &Builder| {
            let n = b.mesh.vertices.len().max(1) as f32;
            let c = b
                .mesh
                .vertices
                .iter()
                .fold(Vec3::ZERO, |a, v| a + v.position)
                / n;
            if c.x.abs() > 34. || c.z.abs() > 34. {
                16
            } else {
                let x = ((c.x + 30.) / 15.).floor().clamp(0., 3.) as usize;
                let z = ((c.z + 30.) / 15.).floor().clamp(0., 3.) as usize;
                z * 4 + x
            }
        };
        let mut buckets: Vec<Vec<&Builder>> = (0..17).map(|_| Vec::new()).collect();
        for b in &s.pieces {
            if !b.mesh.indices.is_empty() {
                buckets[chunk_of(b)].push(b);
            }
        }
        let mut chunks = Vec::new();
        for bucket in buckets {
            let mut group: Vec<&Builder> = Vec::new();
            let mut verts = 0;
            for b in bucket {
                if verts + b.mesh.vertices.len() > 60_000 && !group.is_empty() {
                    chunks.push(
                        Mesh3d::combine(group.iter().map(|g| (&g.mesh, Mat4::IDENTITY)))
                            .expect("static chunk"),
                    );
                    group.clear();
                    verts = 0;
                }
                verts += b.mesh.vertices.len();
                group.push(b);
            }
            if !group.is_empty() {
                chunks.push(
                    Mesh3d::combine(group.iter().map(|g| (&g.mesh, Mat4::IDENTITY)))
                        .expect("static chunk"),
                );
            }
        }
        let canopy_meshes: Vec<Mesh3d> = [TreeKind::Oak, TreeKind::Pine, TreeKind::Birch]
            .into_iter()
            .flat_map(|k| [tree_canopy(k, 4), tree_canopy(k, 2)])
            .collect();
        let chain = |i: usize| {
            LodChain::new(
                vec![
                    canopy_meshes[i * 2].resident().clone(),
                    canopy_meshes[i * 2 + 1].resident().clone(),
                ],
                vec![26.],
                0.12,
            )
            .expect("tree LOD")
        };
        let canopy = [chain(0), chain(1), chain(2)];
        // Grass tufts and flowers, scattered deterministically.
        let mut grass = Builder::new();
        for (k, (dx, dz, h)) in [(0., 0., 0.32), (0.06, 0.04, 0.26), (-0.05, 0.03, 0.22)]
            .into_iter()
            .enumerate()
        {
            let c = if k == 1 { rgb(0x9fd06a) } else { rgb(0x79b04f) };
            let base = vec3(dx, 0., dz);
            let lean = vec3(dx * 1.4, h, dz * 1.4 - 0.02);
            b_blade(&mut grass, base, lean, c);
        }
        let mut rng = crate::sim::Rng::new(77);
        let mut grass_at = Vec::new();
        let mut flower_at = Vec::new();
        while grass_at.len() < 700 {
            let mut p = vec3(rng.range(-29., 29.), 0., rng.range(-29., 29.));
            if let Some(y) = hill_height(p) {
                p.y = y;
            } else if game.park.on_hill(p) {
                continue;
            }
            if game.park.in_pond(p)
                || game.park.under_table(p + Vec3::Y * 0.1).is_some()
                || noise(p.x, p.z) < -0.2
            {
                continue;
            }
            let s = rng.range(0.8, 1.5);
            grass_at.push(Mat4::from_scale_rotation_translation(
                Vec3::splat(s),
                Quat::from_rotation_y(rng.range(0., std::f32::consts::TAU)),
                p,
            ));
        }
        for d in &game.park.decos {
            if d.kind != DecoKind::Flowers {
                continue;
            }
            let n = (d.size.x * d.size.z * 5.) as usize;
            for k in 0..n {
                let p = d.at
                    + vec3(
                        rng.range(-0.45, 0.45) * d.size.x,
                        0.12,
                        rng.range(-0.45, 0.45) * d.size.z,
                    );
                flower_at.push((k % 3, Mat4::from_translation(p)));
            }
        }
        for _ in 0..90 {
            let p = vec3(rng.range(-28., 28.), 0., rng.range(-28., 28.));
            if game.park.on_hill(p) || game.park.in_pond(p) || noise(p.x, p.z) < 0.15 {
                continue;
            }
            flower_at.push(((p.x.abs() as usize) % 3, Mat4::from_translation(p)));
        }
        let flower = |petal: u32| {
            let mut b = Builder::new();
            b.cube(vec3(0., 0.12, 0.), vec3(0.02, 0.24, 0.02), rgb(0x5aa34a))
                .cube(vec3(0., 0.25, 0.), vec3(0.12, 0.04, 0.12), rgb(petal))
                .cube(vec3(0., 0.27, 0.), vec3(0.05, 0.03, 0.05), rgb(0xf6c444));
            b.into_mesh()
        };
        let mut bush = Builder::new();
        bush.blob(vec3(0., 0.55, 0.), vec3(1.1, 0.75, 1.1), rgb(0x6aae4c), 3)
            .blob(vec3(0.5, 0.75, 0.3), vec3(0.6, 0.5, 0.6), rgb(0x7abb55), 3)
            .blob(
                vec3(-0.45, 0.8, -0.35),
                vec3(0.55, 0.45, 0.55),
                rgb(0x82c25a),
                3,
            );
        for (x, z) in [(0.6, -0.5), (-0.3, 0.7), (0.1, 0.1)] {
            bush.cube(vec3(x, 1.12, z), vec3(0.1, 0.06, 0.1), rgb(0xf3a3b8));
        }
        let mut lantern = Builder::new();
        lantern
            .blob(
                vec3(0., -0.14, 0.),
                vec3(0.13, 0.15, 0.13),
                rgb(0xffa04a),
                3,
            )
            .cube(vec3(0., 0.02, 0.), vec3(0.1, 0.03, 0.1), rgb(0x8a3a2a));
        let mut string = Builder::new();
        string.cube(vec3(0.5, 0., 0.), vec3(1., 0.012, 0.012), rgb(0xf4ecdc));
        let mut seesaw = Builder::new();
        let h = crate::sim::props::SEESAW_HALF;
        seesaw.cube(Vec3::ZERO, h * 2., rgb(0x3f7fc1)).cube(
            vec3(0., h.y, 0.),
            vec3(h.x * 2. - 0.1, 0.01, h.z * 1.6),
            rgb(0x5a94d1),
        );
        for s2 in [-1., 1.] {
            seesaw.cube(
                vec3(s2 * (h.x - 0.05), h.y + 0.07, 0.),
                vec3(0.1, 0.14, h.z * 2.),
                rgb(0xf2c94c),
            );
            seesaw.cube(
                vec3(s2 * (h.x - 0.45), h.y + 0.2, 0.),
                vec3(0.06, 0.3, 0.06),
                rgb(0xf2c94c),
            );
            seesaw.cube(
                vec3(s2 * (h.x - 0.45), h.y + 0.35, 0.),
                vec3(0.06, 0.06, 0.4),
                rgb(0xd9534f),
            );
        }
        let mut bell = Builder::new();
        bell.frustum(vec3(0., -0.62, 0.), 0.27, 0.13, 0.42, rgb(0xd9a94a), 10)
            .frustum(vec3(0., -0.2, 0.), 0.13, 0.05, 0.1, rgb(0xc99a3e), 10)
            .blob(vec3(0., -0.6, 0.), Vec3::splat(0.07), rgb(0x7a5a2a), 2)
            .cube(vec3(0., -0.06, 0.), vec3(0.06, 0.14, 0.06), rgb(0x5a4632));
        let mut sign = Builder::new();
        sign.cube(vec3(0., -0.42, 0.), vec3(1.44, 0.6, 0.07), rgb(0xf2e2bf))
            .cube(vec3(0., -0.42, -0.04), vec3(1.3, 0.48, 0.01), rgb(0xf7efe2))
            .cube(vec3(-0.6, -0.06, 0.), vec3(0.03, 0.12, 0.03), rgb(0x8a6a4a))
            .cube(vec3(0.6, -0.06, 0.), vec3(0.03, 0.12, 0.03), rgb(0x8a6a4a));
        // "VILLAGE PICNIC" painted as chunky letter blocks.
        for (k, w) in [0.12, 0.1, 0.12, 0.1, 0.12, 0.1, 0.12].iter().enumerate() {
            sign.cube(
                vec3(-0.48 + k as f32 * 0.16, -0.32, -0.05),
                vec3(*w, 0.1, 0.01),
                rgb(0xc0392b),
            );
        }
        for k in 0..6 {
            sign.cube(
                vec3(-0.4 + k as f32 * 0.16, -0.52, -0.05),
                vec3(0.1, 0.1, 0.01),
                rgb(0x3f7fc1),
            );
        }
        let props = [
            PropKind::Pie,
            PropKind::Marrow,
            PropKind::Pinecone,
            PropKind::Sheet,
            PropKind::Tray,
            PropKind::Blanket,
            PropKind::Melon,
            PropKind::Cushion,
            PropKind::Crate,
            PropKind::Cake,
            PropKind::Dish,
            PropKind::Teapot,
            PropKind::Jelly,
            PropKind::Apple,
        ]
        .into_iter()
        .map(models::prop)
        .collect();
        let pond = game.park.zone(Role::Pond);
        let mut water = Builder::new();
        water.frustum(pond.at, pond.radius, pond.radius, 0.03, rgb(0x6fb1c9), 16);
        for k in 0..6 {
            let a = k as f32 * 1.1;
            let p = pond.at + vec3(a.cos() * 2.3, 0.04, a.sin() * 2.0);
            water.frustum(p, 0.35, 0.35, 0.02, rgb(0x5aa34a), 7);
            water.cube(
                p + vec3(0.12, 0.05, 0.),
                vec3(0.08, 0.04, 0.08),
                rgb(0xf3a3b8),
            );
        }
        let mut sun = Builder::new();
        sun.blob(Vec3::ZERO, Vec3::splat(9.), rgb(0xfff2c6), 3);
        let mut cloud = Builder::new();
        cloud
            .blob(Vec3::ZERO, vec3(9., 2.4, 5.), rgb(0xfdf7ee), 3)
            .blob(vec3(5., 1.2, 1.), vec3(5., 2.6, 4.), rgb(0xfffaf2), 3)
            .blob(vec3(-5.5, 0.6, -0.5), vec3(4.5, 2., 3.5), rgb(0xf8f0e4), 3);
        let banquet = &game.park.tables[banquet_index(game)];
        let mut cloth = Builder::new();
        table(&mut cloth, banquet, true);
        // Strip the wooden table parts: keep only the cloth (indices 0..6 boxes).
        let cloth_only = {
            let mut b = Builder::new();
            let mut t = *banquet;
            t.height += 0.0;
            cloth_skin(&mut b, &t);
            b
        };
        let _ = cloth;
        let mut cape = Builder::new();
        cape.box_rot(
            vec3(0., 0.42, 0.12),
            vec3(0.42, 0.02, 0.6),
            Quat::from_rotation_x(0.35),
            rgb(0xf6f2e8),
        )
        .box_rot(
            vec3(0., 0.31, 0.43),
            vec3(0.46, 0.02, 0.08),
            Quat::from_rotation_x(0.35),
            rgb(0xe2a0b0),
        );
        let mut lid = Builder::new();
        lid.cube(vec3(0., 0., 0.), vec3(0.66, 0.05, 0.5), rgb(0xb98648));
        Self {
            mat: Materials::new(),
            chunks,
            canopy,
            lod: vec![LodState::default(); game.park.trees.len()],
            grass: grass.into_mesh(),
            grass_at,
            flowers: [flower(0xfdfdf7), flower(0xf3a3b8), flower(0xb7a0dc)],
            flower_at,
            bush: bush.into_mesh(),
            lantern: lantern.into_mesh(),
            flags: [
                flag(rgb(0xd9534f)),
                flag(rgb(0xf2c94c)),
                flag(rgb(0x3f7fc1)),
                flag(rgb(0xf7f1e6)),
                flag(rgb(0x5aa86a)),
            ],
            string: string.into_mesh(),
            seesaw: seesaw.into_mesh(),
            bell: bell.into_mesh(),
            sign: sign.into_mesh(),
            props,
            shiny: models::shiny(),
            water: water.into_mesh(),
            cones: [
                fan(100_f32.to_radians()),
                fan(125_f32.to_radians()),
                fan(150_f32.to_radians()),
            ],
            sun: sun.into_mesh(),
            cloud: cloud.into_mesh(),
            cloth: cloth_only.into_mesh(),
            cape: cape.into_mesh(),
            basket_lid: lid.into_mesh(),
            raccoon: models::raccoon(),
            folk: [
                models::nana(),
                models::pumble(),
                models::trumble(),
                models::pip(),
            ],
            dog: models::biscuit(),
            hats: Hat::ALL.iter().map(|h| models::hat(*h)).collect(),
            crown: models::ghost_crown(),
            duck: models::duck(),
            triangles,
        }
    }
}

/// Surface height of the hill (ramp and plateau), if `p` is on it.
fn hill_height(p: Vec3) -> Option<f32> {
    if p.x < -5.8 || p.x > 7.8 || p.z > -11.2 || p.z < -27.8 {
        return None;
    }
    Some(if p.z < -22. {
        3.
    } else {
        (-11. - p.z) / 11. * 3.
    })
}

fn b_blade(b: &mut Builder, base: Vec3, tip: Vec3, c: Color) {
    let side = vec3(0.035, 0., 0.);
    b.tri(base - side, base + side, tip, c);
    b.tri(base + side, base - side, tip, shade(c, 0.9));
}

fn cloth_skin(b: &mut Builder, t: &crate::sim::park::Table) {
    let cloth = Color::from_rgba(t.cloth[0], t.cloth[1], t.cloth[2], 255);
    let trim = Color::from_rgba(t.trim[0], t.trim[1], t.trim[2], 255);
    let y = t.center.y + t.height + 0.006;
    let (w, d) = (t.top.x + 0.12, t.top.z + 0.12);
    b.cube(vec3(t.center.x, y, t.center.z), vec3(w, 0.012, d), cloth);
    let drop = 0.36;
    for (dx, dz, along_x) in [
        (0., d * 0.5, true),
        (0., -d * 0.5, true),
        (w * 0.5, 0., false),
        (-w * 0.5, 0., false),
    ] {
        let c = vec3(t.center.x + dx, y - drop * 0.5, t.center.z + dz);
        let (size, tsize) = if along_x {
            (vec3(w, drop, 0.012), vec3(w + 0.01, 0.05, 0.02))
        } else {
            (vec3(0.012, drop, d), vec3(0.02, 0.05, d + 0.01))
        };
        b.cube(c, size, shade(cloth, 0.94));
        b.cube(vec3(c.x, y - drop + 0.03, c.z), tsize, trim);
        // Lace scallops.
        let n = if along_x {
            (w / 0.3) as i32
        } else {
            (d / 0.3) as i32
        };
        for k in 0..n {
            let f = -0.5 + (k as f32 + 0.5) / n as f32;
            let p = if along_x {
                vec3(t.center.x + f * w, y - drop - 0.02, c.z)
            } else {
                vec3(c.x, y - drop - 0.02, t.center.z + f * d)
            };
            b.cube(p, vec3(0.1, 0.05, 0.1), trim);
        }
    }
}

/// Everything the world pass needs to know about this frame.
pub struct Frame<'a> {
    pub game: &'a Game,
    pub alpha: f32,
    pub time: f32,
    pub camera: Camera3D,
    pub motion: f32,
    pub flash: bool,
    pub hat: Hat,
    pub ghost: Option<(Vec3, f32)>,
    pub cones: bool,
    pub jelly: f32,
    pub showcase: bool,
}

impl Scene {
    fn lit(&self, eye: Vec3) -> LitSettings {
        LitSettings {
            light_direction: SUN,
            camera_position: eye,
            fog_color: rgb(HAZE),
            fog_start: 34.,
            fog_end: 120.,
            ambient: 0.52,
        }
    }

    pub fn draw(&mut self, actors: &Actors, f: &Frame) {
        let game = f.game;
        let eye = f.camera.position;
        let lit = LitMaterial::new().expect("built-in lighting");
        let t = f.time;
        // --- Sky (unlit, far).
        lit.begin(LitSettings {
            ambient: 1.,
            fog_start: 1000.,
            fog_end: 2000.,
            ..self.lit(eye)
        });
        let _ = self.mat.glow.begin([
            [1.6, 0.03 * f.motion, t, 0.],
            [0.1, 0.06, 0.02, 0.],
            [0.; 4],
            [0.; 4],
        ]);
        self.sun
            .draw_transformed(Mat4::from_translation(eye + SUN.normalize() * 140.));
        self.mat.glow.end();
        for (k, (x, y, z, s)) in [
            (-60., 46., -110., 1.4),
            (40., 52., -120., 1.1),
            (110., 40., -30., 1.2),
            (-120., 38., 20., 1.3),
            (20., 56., 120., 1.0),
        ]
        .into_iter()
        .enumerate()
        {
            let drift = (t * 0.6 + k as f32 * 30.) % 80. - 40.;
            self.cloud
                .draw_transformed(Mat4::from_scale_rotation_translation(
                    Vec3::splat(s),
                    Quat::from_rotation_y(k as f32),
                    vec3(x + drift * f.motion, y, z),
                ));
        }
        // --- Park lighting: low golden sun with long shadows and lanterns.
        lit.begin(self.lit(eye));
        let focus = f.camera.target;
        let _ = ferravex_render::set_sun_shadow(Some(SunShadow {
            center: vec3(focus.x, 0., focus.z),
            direction: SUN,
            extent: 30.,
            depth: 90.,
            strength: 0.62,
            ..SunShadow::default()
        }));
        let mut lights = [PointLight {
            position: Vec3::ZERO,
            color: rgb(0xffb35c),
            radius: 3.5,
            intensity: 0.9,
        }; 6];
        let mut n = 0;
        for (i, l) in game.mech.links.iter().enumerate() {
            if i % 3 == 1 && n < 4 {
                let p = l.prev.lerp(l.pos, f.alpha) - Vec3::Y * 0.3;
                lights[n].position = p;
                n += 1;
            }
        }
        lights[n] = PointLight {
            position: vec3(-18., 2.6, -3.),
            color: rgb(0xffd08a),
            radius: 5.,
            intensity: 0.8,
        };
        n += 1;
        let _ = ferravex_render::set_point_lights(&lights[..n]);
        for c in &self.chunks {
            c.draw_transformed(Mat4::IDENTITY);
        }
        // --- Swaying foliage.
        let gust = 0.5 + 0.5 * game.wind.length();
        let _ = self.mat.sway.begin([
            [t, 0.012 * gust * f.motion, 0., 1.6],
            [0.; 4],
            [0.; 4],
            [0.; 4],
        ]);
        for (i, tree) in game.park.trees.iter().enumerate() {
            let k = match tree.kind {
                TreeKind::Oak => 0,
                TreeKind::Pine => 1,
                TreeKind::Birch => 2,
            };
            let m = Mat4::from_scale_rotation_translation(
                Vec3::splat(tree.scale),
                Quat::from_rotation_y(i as f32 * 1.3),
                tree.at,
            );
            ferravex_render::draw_lod(&self.canopy[k], m, &mut self.lod[i]);
        }
        self.mat.sway.end();
        let _ = self.mat.sway.begin([
            [t, 0.05 * gust * f.motion, 0., 0.15],
            [0.; 4],
            [0.; 4],
            [0.; 4],
        ]);
        for (i, b) in game.park.bushes.iter().enumerate() {
            let rustle = if matches!(game.coon.hide, crate::sim::raccoon::Hide::Bush(h) if h == i) {
                1.0 + 0.04 * (t * 9.).sin() * f.motion
            } else {
                1.
            };
            self.bush
                .draw_transformed(Mat4::from_scale_rotation_translation(
                    vec3(b.radius, b.radius * rustle, b.radius),
                    Quat::from_rotation_y(i as f32 * 2.1),
                    b.at,
                ));
        }
        self.mat.sway.end();
        let _ = self.mat.grass.begin([
            [t, 0.9 * gust * f.motion, 0., 0.],
            [0.; 4],
            [0.; 4],
            [0.; 4],
        ]);
        let view = flat(f.camera.target - eye).normalize_or_zero();
        for m in &self.grass_at {
            let p = m.w_axis.truncate();
            let d = p - eye;
            if d.length_squared() < 34. * 34. && flat(d).dot(view) > -4. {
                self.grass.draw_transformed(*m);
            }
        }
        for (k, m) in &self.flower_at {
            self.flowers[*k].draw_transformed(*m);
        }
        self.mat.grass.end();
        // --- Bunting, lanterns, jointed set pieces.
        self.draw_bunting(game, f);
        let m = &game.mech;
        let seesaw = m.seesaw_prev.slerp(m.seesaw_rot, f.alpha);
        self.seesaw
            .draw_transformed(Mat4::from_rotation_translation(seesaw, m.seesaw_pivot));
        let bell = m.bell_prev.slerp(m.bell_rot, f.alpha);
        self.bell
            .draw_transformed(Mat4::from_rotation_translation(bell, m.bell_pivot));
        let sign = m.sign_prev.slerp(m.sign_rot, f.alpha);
        self.sign
            .draw_transformed(Mat4::from_rotation_translation(sign, m.sign_pivot));
        // --- Props.
        for (i, p) in game.props.iter().enumerate() {
            if p.holder == Holder::Raccoon && game.coon.hide == crate::sim::raccoon::Hide::Basket {
                continue;
            }
            let (pos, rot) = game.prop_pose(i, f.alpha);
            let wobble = matches!(p.kind, PropKind::Jelly | PropKind::Cake) && f.jelly > 0.01;
            if wobble {
                let _ = self.mat.jelly.begin([
                    [t, f.jelly * f.motion.max(0.3), 0., 0.],
                    [0.; 4],
                    [0.; 4],
                    [0.; 4],
                ]);
            }
            self.props[kind_index(p.kind)]
                .draw_transformed(Mat4::from_rotation_translation(rot, pos));
            if wobble {
                self.mat.jelly.end();
            }
        }
        if !game.coon.cape {
            self.cloth.draw_transformed(Mat4::IDENTITY);
        }
        let basket = game.park.zone(Role::Basket).at;
        let peek = if game.coon.hide == crate::sim::raccoon::Hide::Basket {
            0.12 + 0.05 * (t * 3.).sin().abs() * f.motion
        } else {
            0.
        };
        self.basket_lid
            .draw_transformed(Mat4::from_rotation_translation(
                Quat::from_rotation_x(-peek * 2.),
                basket + vec3(0., 0.47 + peek, 0.),
            ));
        // --- Shiny bottle caps (spinning, glinting).
        let _ = self.mat.glow.begin([
            [1.25, 0.25 * f.motion, t * 2., 0.],
            [0.15, 0.12, 0.08, 0.],
            [0.; 4],
            [0.; 4],
        ]);
        for (k, (p, taken)) in game.shinies.iter().enumerate() {
            if *taken {
                continue;
            }
            let bob = (t * 2.2 + k as f32).sin() * 0.06 * f.motion;
            self.shiny.draw_transformed(Mat4::from_rotation_translation(
                Quat::from_rotation_y(t * 2. + k as f32) * Quat::from_rotation_x(1.2),
                *p + Vec3::Y * (0.12 + bob),
            ));
        }
        self.mat.glow.end();
        self.draw_characters(actors, f);
        // --- Water.
        let _ = self.mat.water.begin([
            [
                t * f.motion.max(0.2),
                if f.flash { 0.4 } else { 1. },
                0.,
                0.,
            ],
            [0.; 4],
            [0.; 4],
            [0.; 4],
        ]);
        self.water.draw_transformed(Mat4::IDENTITY);
        self.mat.water.end();
        // Three smug ducks paddle in slow circles.
        let pond = game.park.zone(Role::Pond);
        for k in 0..3 {
            let a = t * (0.16 + k as f32 * 0.05) * f.motion.max(0.15) + k as f32 * 2.1;
            let r = 1.2 + k as f32 * 0.9;
            let p = pond.at
                + vec3(
                    a.cos() * r,
                    0.02 + (t * 2.1 + k as f32).sin() * 0.015 * f.motion,
                    a.sin() * r,
                );
            let yaw = a + std::f32::consts::FRAC_PI_2;
            self.duck.draw_transformed(Mat4::from_rotation_translation(
                Quat::from_rotation_y(-yaw - std::f32::consts::FRAC_PI_2),
                p,
            ));
        }
        // --- Ghost (alpha) and vision cones (alpha), after opaque geometry.
        if let Some((p, yaw)) = f.ghost {
            let _ = self
                .mat
                .ghost
                .begin([[t, 0., 0., 0.], [0.; 4], [0.; 4], [0.; 4]]);
            let m = model(p, yaw, 0.92);
            ferravex_render::draw_skinned(self.raccoon.resident_ref(), m, &actors.ghost.palette);
            self.crown.draw_transformed(m * actors.ghost.head());
            self.mat.ghost.end();
        }
        if f.cones && !game.is_over() {
            self.draw_cones(f);
        }
        let _ = ferravex_render::set_point_lights(&[]);
        lit.end();
    }

    fn draw_bunting(&self, game: &Game, f: &Frame) {
        let links = &game.mech.links;
        let anchors = &game.mech.anchors;
        let mut lantern_at = [Vec3::ZERO; 12];
        let mut lanterns = 0;
        for (i, l) in links.iter().enumerate() {
            let p = l.prev.lerp(l.pos, f.alpha);
            let prev = if i > 0 && links[i - 1].string == l.string {
                let q = &links[i - 1];
                q.prev.lerp(q.pos, f.alpha)
            } else {
                anchors[l.string as usize * 2].0
            };
            let next = if i + 1 < links.len() && links[i + 1].string == l.string {
                let q = &links[i + 1];
                q.prev.lerp(q.pos, f.alpha)
            } else {
                anchors[l.string as usize * 2 + 1].0
            };
            {
                let (a, b) = (prev, p);
                let d = b - a;
                let len = d.length();
                if len > 1e-3 {
                    let rot = Quat::from_rotation_arc(Vec3::X, d / len);
                    self.string
                        .draw_transformed(Mat4::from_scale_rotation_translation(
                            vec3(len, 1., 1.),
                            rot,
                            a,
                        ));
                }
            }
            if i + 1 == links.len() || links[i + 1].string != l.string {
                let d = next - p;
                let len = d.length();
                if len > 1e-3 {
                    let rot = Quat::from_rotation_arc(Vec3::X, d / len);
                    self.string
                        .draw_transformed(Mat4::from_scale_rotation_translation(
                            vec3(len, 1., 1.),
                            rot,
                            p,
                        ));
                }
            }
            let along = flat(next - prev).normalize_or_zero();
            let yaw = (-along.z).atan2(along.x);
            let flutter = (f.time * 4. + i as f32).sin() * 0.25 * f.motion + game.wind.z * 0.3;
            self.flags[i % 5].draw_transformed(Mat4::from_rotation_translation(
                Quat::from_rotation_y(yaw) * Quat::from_rotation_x(flutter),
                p,
            ));
            if i % 3 == 1 && lanterns < lantern_at.len() {
                lantern_at[lanterns] = p;
                lanterns += 1;
            }
        }
        let _ = self.mat.glow.begin([
            [2.3, 0.08 * f.motion, f.time, 0.],
            [0.25, 0.08, 0.02, 0.],
            [0.; 4],
            [0.; 4],
        ]);
        for p in &lantern_at[..lanterns] {
            self.lantern
                .draw_transformed(Mat4::from_translation(*p - Vec3::Y * 0.12));
        }
        for k in 0..4 {
            let a = (k as f32 + 0.5) / 4. * std::f32::consts::TAU;
            self.lantern.draw_transformed(Mat4::from_translation(vec3(
                -18. + a.cos() * 2.9,
                3.1,
                -3. + a.sin() * 2.9,
            )));
        }
        self.mat.glow.end();
    }

    fn draw_characters(&self, actors: &Actors, f: &Frame) {
        let game = f.game;
        let (p, yaw) = game.coon_pose(f.alpha);
        let mut p = p;
        if game.coon.hide == crate::sim::raccoon::Hide::Basket {
            p.y -= 0.32;
        }
        let m = model(p, yaw, 1.);
        ferravex_render::draw_skinned(self.raccoon.resident_ref(), m, &actors.coon.palette);
        if let Some(Some(hat)) = self.hats.get(f.hat.index()) {
            hat.draw_transformed(m * actors.coon.head());
        }
        if game.coon.cape {
            self.cape
                .draw_transformed(m * Mat4::from_cols_array(&actors.coon.matrices[1]));
        }
        for (i, folk) in game.folk.iter().enumerate() {
            let pos = folk.prev.lerp(folk.pos, f.alpha);
            let yaw = crate::sim::lerp_angle(folk.prev_yaw, folk.yaw, f.alpha);
            let lift = 0.;
            match (&actors.folk[i], i) {
                (Some(motion), _) => {
                    let scale = if folk.kind == Kind::Pip { 0.68 } else { 1. };
                    let m = model(pos + Vec3::Y * lift, yaw, scale);
                    ferravex_render::draw_skinned(self.folk[i].resident_ref(), m, &motion.palette);
                }
                (None, _) => {
                    let m = model(pos, yaw, 1.28);
                    ferravex_render::draw_skinned(self.dog.resident_ref(), m, &actors.dog.palette);
                }
            }
        }
    }

    fn draw_cones(&self, f: &Frame) {
        let game = f.game;
        let (coon, _) = game.coon_pose(f.alpha);
        for folk in &game.folk {
            if matches!(
                folk.mode,
                Mode::Carry | Mode::React(_) | Mode::Puffed(_) | Mode::Sit
            ) && folk.state() != CURIOUS
            {
                continue;
            }
            let pos = folk.prev.lerp(folk.pos, f.alpha);
            let near = flat(pos - coon).length();
            if near > 24. {
                continue;
            }
            let fade = (1. - (near - 16.) / 8.).clamp(0., 1.);
            let range = folk.range(game.coon.sneaking);
            let (c, a) = match folk.state() {
                ALARMED => (rgb(0xff6b4a), 0.34),
                CURIOUS => (rgb(0xffc94a), 0.24 + 0.12 * folk.alert),
                _ if folk.content > 0. => (rgb(0xf3a3b8), 0.14),
                _ => (rgb(0xfff8e8), 0.17),
            };
            let cone = match folk.kind {
                Kind::Pip => &self.cones[1],
                Kind::Biscuit => &self.cones[2],
                _ => &self.cones[0],
            };
            let yaw = crate::sim::lerp_angle(folk.prev_yaw, folk.yaw, f.alpha);
            let apex = vec3(pos.x, pos.y + 0.035, pos.z);
            let fwd = heading(yaw);
            let _ = self.mat.cone.begin([
                [c.r, c.g, c.b, a * fade],
                [apex.x, apex.y, apex.z, range],
                [f.time, fwd.x, fwd.z, folk.kind.fov() * 0.5],
                [0.; 4],
            ]);
            cone.draw_transformed(Mat4::from_scale_rotation_translation(
                vec3(range, 1., range),
                Quat::from_rotation_y(-yaw),
                apex,
            ));
            self.mat.cone.end();
        }
    }
}

/// Small helper trait: a skinned mesh reference for draw calls.
pub trait SkinRef {
    fn resident_ref(&self) -> &SkinnedMesh;
}
impl SkinRef for SkinnedMesh {
    fn resident_ref(&self) -> &SkinnedMesh {
        self
    }
}
