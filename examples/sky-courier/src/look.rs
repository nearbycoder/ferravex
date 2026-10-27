//! Golden-hour art direction: palette, per-state grade, sky and emissive glows.
//!
//! Everything here is presentation. The sky, sun and glows are immediate 2D
//! draws in screen space that land in the post-processed world layer (before
//! the explicit HUD boundary), so additive glows feed the bloom while the HUD
//! stays crisp. Animated accents use the presentation clocks, which freeze in
//! pause and under reduced motion.
use super::overlay::Projector;
use super::*;
use ferravex_render::post::{self, PostFx, Tonemap};
use ferravex_render::shapes::{self, Fill};

/// Upper sky, warm haze at the horizon (also the fog color) and the sun.
pub(super) const ZENITH: Color = Color::new(0.23, 0.42, 0.58, 1.0);
pub(super) const UPPER: Color = Color::new(0.55, 0.65, 0.74, 1.0);
pub(super) const HORIZON: Color = Color::new(0.97, 0.80, 0.62, 1.0);
const SUN: Color = Color::new(1.0, 0.86, 0.62, 1.0);
const SUN_CORE: Color = Color::new(1.0, 0.97, 0.88, 1.0);
/// Direction toward the low evening sun, ahead and to the left of the route.
pub(super) const SUN_DIR: Vec3 = vec3(-0.45, 0.19, 0.87);
const RING_GLOW: Color = Color::new(1.0, 0.72, 0.36, 1.0);
const HOT_GLOW: Color = Color::new(1.0, 0.88, 0.50, 1.0);
const SECRET_GLOW: Color = Color::new(0.80, 1.0, 0.78, 1.0);
const BEACON: Color = Color::new(1.0, 0.62, 0.42, 1.0);
const PORT: Color = Color::new(1.0, 0.30, 0.25, 1.0);
const STARBOARD: Color = Color::new(0.40, 1.0, 0.55, 1.0);

/// The grade for each scene mood. Thresholds stay high because the sky is
/// bright: only the sun, glows and the brightest cloud tops bloom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mood {
    Flight,
    Menu,
    Paused,
    Delivered,
    Grounded,
}

impl Mood {
    pub(super) fn of(state: SessionState) -> Self {
        match state {
            SessionState::Playing => Self::Flight,
            SessionState::Ready => Self::Menu,
            SessionState::Paused => Self::Paused,
            SessionState::Won => Self::Delivered,
            SessionState::Lost => Self::Grounded,
        }
    }

    pub(super) fn post(self) -> PostFx {
        let base = PostFx::off()
            .bloom(1.15, 0.86, 1.25)
            .bloom_knee(0.3)
            .tonemap(Tonemap::Filmic)
            .exposure(1.04)
            .contrast(1.05)
            .saturation(1.08)
            // Cool lifted shadows, warm highlights: golden hour.
            .lift_gamma_gain([0.0, 0.012, 0.035], [1.0, 1.0, 1.0], [1.06, 0.99, 0.90])
            .vignette(0.30, 0.62)
            .auto_hud(false);
        match self {
            Self::Flight => base,
            Self::Menu => base.exposure(0.94).vignette(0.42, 0.6),
            Self::Paused => base.exposure(0.84).saturation(0.72).vignette(0.5, 0.55),
            Self::Delivered => base.bloom(1.6, 0.82, 1.4).saturation(1.14),
            Self::Grounded => base
                .saturation(0.78)
                .lift_gamma_gain([0.0, 0.02, 0.05], [1.0, 1.0, 1.0], [0.96, 0.98, 1.02])
                .vignette(0.48, 0.58),
        }
    }
}

/// Sticky post settings: only touch the renderer when the mood changes.
#[derive(Default)]
pub(super) struct Grade {
    current: Option<Mood>,
}

impl Grade {
    pub(super) fn apply(&mut self, state: SessionState) {
        let mood = Mood::of(state);
        if self.current != Some(mood) {
            post::set_post_fx(mood.post());
            self.current = Some(mood);
        }
    }
}

/// Screen-space horizon height for a level chase camera.
fn horizon_y(projector: &Projector, camera: Vec3, target: Vec3, h: f32) -> f32 {
    let flat = vec3(target.x - camera.x, 0., target.z - camera.z)
        .try_normalize()
        .unwrap_or(Vec3::Z);
    projector
        .project(camera + flat * 600.)
        .filter(|(_, front)| *front)
        .map_or(h * 0.5, |(p, _)| p.y)
        .clamp(h * 0.12, h * 0.95)
}

/// Gradient sky and low sun, drawn before the 3D world.
pub(super) fn sky(projector: &Projector, camera: Vec3, target: Vec3, w: f32, h: f32) {
    let horizon = horizon_y(projector, camera, target, h);
    let upper = horizon * 0.55;
    shapes::fill_rounded_rect(
        Rect::new(0., 0., w, upper + 1.),
        0.,
        Fill::vertical(Rect::new(0., 0., w, upper), ZENITH, UPPER),
    );
    let band = Rect::new(0., upper, w, horizon - upper + 1.);
    shapes::fill_rounded_rect(band, 0., Fill::vertical(band, UPPER, HORIZON));
    shapes::fill_rounded_rect(Rect::new(0., horizon, w, h - horizon), 0., HORIZON);
    if let Some((sun, true)) = projector.project(camera + SUN_DIR * 2000.) {
        let size = h.min(w) * 0.05;
        post::glow_circle(sun.x, sun.y, size * 7., SUN, 0.55);
        post::glow_circle(sun.x, sun.y, size * 2.4, SUN, 0.9);
        shapes::fill_circle(sun, size, SUN_CORE);
    }
}

fn ring_radius(
    projector: &Projector,
    center: Vec3,
    right: Vec3,
    radius: f32,
) -> Option<(Vec2, f32)> {
    let (c, front) = projector.project(center)?;
    let (edge, _) = projector.project(center + right * radius)?;
    let r = (edge - c).length();
    (front && r.is_finite() && r > 0.5).then_some((c, r))
}

/// Emissive accents feeding the bloom: the live ring, unclaimed hidden rings,
/// the delivery beacon and the aircraft's navigation lights.
pub(super) fn glows(
    projector: &Projector,
    flight: &Flight,
    camera: Vec3,
    target: Vec3,
    clock: f32,
    flash: bool,
) {
    let right = (target - camera)
        .cross(Vec3::Y)
        .try_normalize()
        .unwrap_or(Vec3::X);
    if let Some(gate) = ROUTE.get(flight.next).filter(|_| flight.next < GATES) {
        let hot = flight.ledger.multiplier() >= 3;
        let pulse = 1. + 0.05 * (clock * 4.).sin();
        if let Some((c, r)) = ring_radius(projector, *gate, right, 9. * pulse) {
            let color = if hot { HOT_GLOW } else { RING_GLOW };
            shapes::glow_ring(c, r, (r * 0.35).clamp(3., 60.), with_alpha(color, 0.55));
            post::glow_circle(c.x, c.y, r * 1.6, color, 0.12);
        }
    }
    for (i, ring) in courier::BONUS_RINGS.iter().enumerate() {
        if flight.ledger.bonus & (1 << i) != 0 || ring.distance(flight.pos) > 320. {
            continue;
        }
        if let Some((c, r)) = ring_radius(projector, *ring, right, 5.5) {
            let twinkle = 0.6 + 0.4 * (clock * 3. + i as f32 * 2.).sin().abs();
            shapes::glow_ring(
                c,
                r,
                (r * 0.4).clamp(2., 30.),
                with_alpha(SECRET_GLOW, 0.45 * twinkle),
            );
            post::glow_circle(c.x, c.y, (r * 0.5).max(3.), SECRET_GLOW, 0.5 * twinkle);
        }
    }
    if flight.next == GATES {
        let pad = ROUTE[GATES];
        if let (Some((base, true)), Some((top, _))) = (
            projector.project(pad - Vec3::Y * 6.),
            projector.project(pad + Vec3::Y * 34.),
        ) {
            let width = ((top - base).length() * 0.08).clamp(3., 40.);
            post::with_glow(|| {
                shapes::fill_capsule(
                    base,
                    top,
                    width,
                    Fill::linear(base, top, with_alpha(BEACON, 0.45), with_alpha(BEACON, 0.)),
                );
            });
            post::glow_circle(base.x, base.y, width * 4., BEACON, 0.5);
        }
    }
    // Navigation lights: steady under reduced flash, a slow blink otherwise.
    let rotation = Quat::from_rotation_y(flight.motor.yaw)
        * Quat::from_rotation_x(-flight.motor.pitch)
        * Quat::from_rotation_z(flight.motor.bank);
    let on = !flash || (clock * 1.2).fract() < 0.75;
    if on {
        for (side, color) in [(-1., STARBOARD), (1., PORT)] {
            if let Some((p, true)) =
                projector.project(flight.pos + rotation * vec3(side * 5.75, 0.2, 0.6))
            {
                post::glow_circle(p.x, p.y, 7., color, 0.9);
            }
        }
    }
}

pub(super) fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * alpha)
}

/// Thin anti-aliased stroke helper for overlay brackets and arrows.
/// Gull flight paths: lazy circles around islands, flapping on ambient time.
pub(super) fn gull(index: usize, time: f32) -> (Vec3, f32, f32) {
    let (center, _) = ISLANDS[[0, 2, 4, 5, 6, 7][index % 6]];
    let dir = if index.is_multiple_of(2) { 1. } else { -1. };
    let radius = 26. + index as f32 * 5.;
    let angle = time * 0.32 * dir + index as f32 * 1.7;
    let pos = center
        + vec3(
            angle.cos() * radius,
            20. + index as f32 * 3. + (time * 0.8 + index as f32).sin() * 2.,
            angle.sin() * radius,
        );
    let tangent = vec2(-angle.sin(), angle.cos()) * dir;
    let heading = tangent.x.atan2(tangent.y);
    let flap = (time * 6.5 + index as f32 * 1.3).sin() * 0.55;
    (pos, heading, flap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_mood_has_a_distinct_bounded_grade_with_an_explicit_hud_boundary() {
        let moods = [
            Mood::Flight,
            Mood::Menu,
            Mood::Paused,
            Mood::Delivered,
            Mood::Grounded,
        ];
        for (i, a) in moods.iter().enumerate() {
            let fx = a.post();
            assert_eq!(fx, fx.sanitized(), "{a:?} stays within documented ranges");
            assert!(!fx.auto_hud, "the HUD boundary is explicit");
            assert!(fx.bloom_threshold >= 0.8, "a bright sky must not haze");
            for b in &moods[i + 1..] {
                assert_ne!(fx, b.post());
            }
        }
        assert_eq!(Mood::of(SessionState::Paused), Mood::Paused);
    }

    #[test]
    fn gulls_stay_aloft_and_clear_of_terrain() {
        for index in 0..6 {
            for step in 0..200 {
                let (pos, heading, flap) = gull(index, step as f32 * 0.37);
                assert!(pos.is_finite() && heading.is_finite() && flap.abs() <= 0.55);
                assert!(!island_hit(pos), "gull {index} clips an island");
                assert!(pos.y > 0. && pos.y < 80.);
            }
        }
    }
}
