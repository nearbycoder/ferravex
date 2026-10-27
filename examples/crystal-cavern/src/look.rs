//! Art direction: per-state post-processing grades and additive light pools.
//!
//! The details contract keeps three real point lights at Ready (one per
//! crystal), so the rest of the cave's light is carried by emissive meshes,
//! additive "light pool" decals on the floor, camera-facing halos and bloom.
//! Everything here is presentation: it reads accepted game state and never
//! changes it. Pools and halos reuse one disc mesh whose vertex colors are
//! rewritten in place, so drawing them allocates nothing.
use super::{Game, Interactable, MINT, at, lantern_positions, note_position, shy_position, world};
use ferravex_core::session::SessionState;
use ferravex_render::post::{self, PostFx, Tonemap};
use ferravex_render::prelude::*;

/// Cave grade: teal-lifted shadows, warm highlights and a threshold high
/// enough that only crystals, lanterns, daylight and glows bloom.
pub(super) const CAVE: PostFx = PostFx::off()
    .bloom(1.25, 0.74, 1.3)
    .bloom_knee(0.3)
    .tonemap(Tonemap::Filmic)
    .exposure(1.14)
    .contrast(1.08)
    .saturation(1.14)
    .lift_gamma_gain([0., 0.018, 0.04], [1.0, 1.03, 1.08], [1.08, 1.0, 0.9])
    .vignette(0.36, 0.62);
/// Menus over the cave: the same grade, dimmed and framed so cards lead.
pub(super) const CAVE_MENU: PostFx = PostFx::off()
    .bloom(1.5, 0.72, 1.3)
    .bloom_knee(0.3)
    .tonemap(Tonemap::Filmic)
    .exposure(1.05)
    .contrast(1.08)
    .saturation(1.05)
    .lift_gamma_gain([0., 0.018, 0.04], [1.0, 1.03, 1.08], [1.08, 1.0, 0.9])
    .vignette(0.5, 0.55);
/// Results: the expedition steps into warm daylight.
pub(super) const DAYLIGHT: PostFx = PostFx::off()
    .bloom(2.0, 0.7, 1.45)
    .bloom_knee(0.35)
    .tonemap(Tonemap::Filmic)
    .exposure(1.4)
    .contrast(1.04)
    .saturation(1.08)
    .lift_gamma_gain([0.02, 0.015, 0.0], [1.05, 1.03, 1.0], [1.12, 1.04, 0.88])
    .vignette(0.28, 0.65);

pub(super) fn post_for(state: SessionState) -> PostFx {
    match state {
        SessionState::Playing => CAVE,
        SessionState::Won => DAYLIGHT,
        _ => CAVE_MENU,
    }
}

pub(super) const AMBER: Color = Color::new(1.0, 0.68, 0.32, 1.0);
pub(super) const SUN: Color = Color::new(1.0, 0.9, 0.62, 1.0);
pub(super) const GOLD: Color = Color::new(1.0, 0.8, 0.4, 1.0);
pub(super) const VIOLET: Color = Color::new(0.72, 0.5, 1.0, 1.0);

const SEGMENTS: usize = 16;
/// (radius fraction, alpha): a soft Gaussian-like falloff, zero at the rim.
const RINGS: [(f32, f32); 5] = [(0., 1.), (0.25, 0.76), (0.5, 0.32), (0.75, 0.08), (1., 0.)];

/// One reusable unit disc in the XY plane (112 triangles: an even count keeps
/// the shared stream's quad alignment intact for overlay inspectors).
pub(super) struct Glow {
    disc: Mesh,
}

impl Glow {
    pub fn new() -> Self {
        let mut vertices = Vec::with_capacity(1 + SEGMENTS * 2);
        vertices.push(Vertex::new2(Vec3::ZERO, Vec2::ZERO, WHITE));
        for &(r, _) in &RINGS[1..] {
            for s in 0..SEGMENTS {
                let a = s as f32 * std::f32::consts::TAU / SEGMENTS as f32;
                vertices.push(Vertex::new2(
                    vec3(a.cos() * r, a.sin() * r, 0.),
                    Vec2::ZERO,
                    WHITE,
                ));
            }
        }
        let mut indices = Vec::with_capacity(SEGMENTS * 3 * (2 * RINGS.len() - 3));
        let ring = |k: usize, s: usize| (1 + k * SEGMENTS + s % SEGMENTS) as u16;
        for s in 0..SEGMENTS {
            indices.extend_from_slice(&[0, ring(0, s), ring(0, s + 1)]);
            for k in 0..RINGS.len() - 2 {
                indices.extend_from_slice(&[ring(k, s), ring(k + 1, s), ring(k + 1, s + 1)]);
                indices.extend_from_slice(&[ring(k, s), ring(k + 1, s + 1), ring(k, s + 1)]);
            }
        }
        Self {
            disc: Mesh {
                vertices,
                indices,
                texture: None,
            },
        }
    }

    fn tint(&mut self, color: Color, intensity: f32) {
        let k = intensity.clamp(0., 1.);
        for (i, v) in self.disc.vertices.iter_mut().enumerate() {
            let ring = if i == 0 { 0 } else { 1 + (i - 1) / SEGMENTS };
            let alpha = RINGS[ring].1;
            v.color = Color::new(color.r * k, color.g * k, color.b * k, alpha).to_rgba();
        }
    }

    /// A light pool lying on the floor.
    pub fn pool(&mut self, p: Vec2, radius: f32, color: Color, intensity: f32) {
        self.tint(color, intensity);
        let model = Mat4::from_scale_rotation_translation(
            Vec3::splat(radius),
            Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            world(p, 0.025),
        );
        draw_mesh_transformed(&self.disc, model);
    }

    /// A camera-facing halo.
    pub fn halo(
        &mut self,
        at: Vec3,
        view: (Vec3, Vec3),
        radius: f32,
        color: Color,
        intensity: f32,
    ) {
        self.tint(color, intensity);
        let (right, up) = view;
        let normal = right.cross(up);
        let model = Mat4::from_cols(
            (right * radius).extend(0.),
            (up * radius).extend(0.),
            normal.extend(0.),
            at.extend(1.),
        );
        draw_mesh_transformed(&self.disc, model);
    }
}

impl Default for Glow {
    fn default() -> Self {
        Self::new()
    }
}

/// Every light pool and halo in the cave for one frame, additive so bloom
/// picks up the brightest. `pulse` is the shader glow phase (zero when
/// reduced flash bypasses it), `motion` the bob phase (frozen when reduced).
pub(super) fn draw_lights(
    glow: &mut Glow,
    game: &Game,
    view: (Vec3, Vec3),
    pulse: f32,
    motion: f32,
) {
    let focus = game.target();
    post::with_glow(|| {
        for (i, &p) in game.crystals.iter().enumerate() {
            if game.collected[i] {
                continue;
            }
            // Warmer/colder: pools and halos swell as the explorer approaches,
            // and a focused crystal brightens in anticipation of its pickup.
            let near = (1. - p.distance(game.position) / super::RESONANCE_RANGE).clamp(0., 1.);
            let focused = focus == Some(Interactable::Crystal(i));
            let shimmer = 1. + 0.12 * (pulse * 2.4 + i as f32 * 1.7).sin();
            let lift = if focused { 0.25 } else { 0. };
            glow.pool(
                p,
                2.2 + 0.6 * near,
                MINT,
                (0.12 + 0.12 * near + lift * 0.5) * shimmer,
            );
            let height = 0.95 + (motion * 1.6 + i as f32).sin() * 0.09;
            glow.halo(
                world(p, height),
                view,
                0.75 + 0.2 * near + lift * 0.5,
                MINT,
                (0.1 + 0.12 * near + lift * 0.4) * shimmer,
            );
        }
        for p in lantern_positions() {
            glow.pool(p, 2.0, AMBER, 0.22);
            glow.halo(world(p, 0.16), view, 0.45, AMBER, 0.45);
        }
        // Wendell's lantern also lights his note, so the secret reads as a place.
        glow.pool(note_position(), 1.1, AMBER, 0.1);
        let sun = at(b'X');
        glow.pool(
            sun + vec2(0., 0.6),
            3.2,
            SUN,
            if game.open() { 0.3 } else { 0.12 },
        );
        glow.halo(world(sun + vec2(0., -0.5), 1.75), view, 2.4, SUN, 0.22);
        for (index, marker) in b"AB".iter().enumerate() {
            if usize::from(game.switches) > index {
                glow.pool(at(*marker), 2.0, GOLD, 0.25);
                glow.halo(world(at(*marker), 1.25), view, 0.6, GOLD, 0.4);
            }
        }
        if !game.shy_found {
            // Steve glows faintly. He would rather you didn't notice.
            glow.pool(shy_position(), 0.9, VIOLET, 0.12);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grades_pick_a_look_per_state_and_keep_bloom_to_emissives() {
        assert_eq!(post_for(SessionState::Playing), CAVE);
        assert_eq!(post_for(SessionState::Ready), CAVE_MENU);
        assert_eq!(post_for(SessionState::Paused), CAVE_MENU);
        assert_eq!(post_for(SessionState::Won), DAYLIGHT);
        for fx in [CAVE, CAVE_MENU, DAYLIGHT] {
            assert!(!fx.is_off());
            // Lit rock stays under the threshold; mint crystals (0.94) exceed it.
            assert!(fx.bloom_threshold >= 0.7 && fx.bloom_threshold < 0.9);
            assert_eq!(fx.sanitized(), fx, "authored values are in range");
        }
    }

    #[test]
    fn the_glow_disc_has_an_even_triangle_count_and_fades_to_the_rim() {
        let mut glow = Glow::new();
        assert_eq!(glow.disc.indices.len() % 6, 0);
        glow.tint(MINT, 2.);
        assert_eq!(glow.disc.vertices[0].color[3], 255);
        assert!(glow.disc.vertices.last().unwrap().color[3] == 0);
        let capacity = glow.disc.vertices.capacity();
        glow.tint(AMBER, 0.5);
        assert_eq!(
            glow.disc.vertices.capacity(),
            capacity,
            "tinting reuses the mesh"
        );
    }
}
