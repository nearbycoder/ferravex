//! Art direction: per-scene post grade, a gradient desert sky with heat
//! shimmer, emissive glows projected from the world, and glowing boost trails.
//!
//! Everything here is presentation. Glows are drawn in screen space inside the
//! world pass (before the explicit HUD boundary), so only they and the sun cross
//! the bloom threshold; sand, stone and pale curbs never haze.
use super::juice::Juice;
use super::race::{GATES, HALF_WIDTH, HOVER_HEIGHT, Race, right, world};
use ferravex_play::particles::{RibbonFacing, Trail, TrailConfig, TrailSample};
use ferravex_render::effects::{EffectBlend, EffectStyle, draw_ribbon};
use ferravex_render::post::{self, PostFx, Tonemap};
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Fill};

/// Sky gradient: deep warm teal overhead to a peach heat haze on the horizon.
pub(super) const ZENITH: Color = Color::new(0.30, 0.55, 0.64, 1.0);
pub(super) const MID_SKY: Color = Color::new(0.62, 0.77, 0.78, 1.0);
pub(super) const HAZE: Color = Color::new(0.95, 0.82, 0.64, 1.0);
const SUN_POSITION: Vec3 = vec3(-95.0, 75.0, -130.0);
const EMBER: Color = Color::new(1.0, 0.52, 0.18, 1.0);
const WHITE_HOT: Color = Color::new(1.0, 0.86, 0.55, 1.0);
const GATE_GLOW: Color = Color::new(0.30, 0.95, 0.85, 1.0);

/// Which grade the frame should use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Scene {
    Title,
    Race,
    Paused,
    Results,
}

/// Warm film for the desert: filmic curve, warm gain, slight lift, soft
/// vignette and a whisper of grain. The threshold sits above the brightest
/// lit sand, sky and curb, so only the sun, exhaust, trails and sparks bloom.
pub(super) fn grade(scene: Scene) -> PostFx {
    let race = PostFx::warm_film()
        .bloom(1.25, 1.0, 1.2)
        .bloom_knee(0.1)
        .tonemap(Tonemap::Filmic)
        .exposure(1.04)
        .contrast(1.07)
        .saturation(1.06)
        .lift_gamma_gain([0.02, 0.012, 0.0], [1.0, 1.0, 1.0], [1.05, 1.0, 0.91])
        .vignette(0.3, 0.62)
        .grain(0.018)
        // Glows are drawn in screen space inside the world pass, then the HUD
        // boundary is explicit (`post_fx_hud_begin`).
        .auto_hud(false);
    match scene {
        Scene::Race => race,
        Scene::Title => race.bloom(1.5, 1.0, 1.35).vignette(0.42, 0.6),
        Scene::Paused => race.saturation(0.55).exposure(0.86).vignette(0.5, 0.55),
        Scene::Results => race.saturation(0.9).exposure(0.92).vignette(0.46, 0.6),
    }
}

/// Applies a scene grade only when it changes (post settings are sticky).
#[derive(Default)]
pub(super) struct Grade {
    current: Option<Scene>,
}

impl Grade {
    pub fn set(&mut self, scene: Scene) {
        if self.current != Some(scene) {
            post::set_post_fx(grade(scene));
            self.current = Some(scene);
        }
    }
}

/// World → screen projection matching `Camera3D` (right-handed, GL depth).
#[derive(Clone, Copy)]
pub(super) struct Projector {
    matrix: Mat4,
    size: Vec2,
}

impl Projector {
    pub fn new(camera: &Camera3D, w: f32, h: f32) -> Self {
        let matrix =
            Mat4::perspective_rh_gl(camera.fovy, w / h.max(1.), camera.z_near, camera.z_far)
                * Mat4::look_at_rh(camera.position, camera.target, camera.up);
        Self {
            matrix,
            size: vec2(w, h),
        }
    }
    /// Screen position and clip depth `w` (distance-like), or `None` behind the eye.
    pub fn project(&self, p: Vec3) -> Option<(Vec2, f32)> {
        let clip = self.matrix * p.extend(1.);
        if clip.w <= 0.1 || !clip.is_finite() {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some((
            vec2(
                (ndc.x + 1.) * 0.5 * self.size.x,
                (1. - ndc.y) * 0.5 * self.size.y,
            ),
            clip.w,
        ))
    }
    /// Pixels per world unit at a clip depth.
    pub fn scale(&self, depth: f32) -> f32 {
        self.size.y / depth.max(0.1)
    }
}

/// Full-screen gradient sky; drawn first, under the world.
pub(super) fn sky(horizon_y: f32) {
    let w = screen_width();
    let h = screen_height();
    let horizon = horizon_y.clamp(h * 0.15, h * 0.85);
    let upper = Rect::new(0., 0., w, horizon * 0.62);
    shapes::fill_rounded_rect(upper, 0., Fill::vertical(upper, ZENITH, MID_SKY));
    let lower = Rect::new(0., upper.h, w, h - upper.h);
    let band = Rect::new(0., upper.h, w, horizon - upper.h + 1.);
    shapes::fill_rounded_rect(band, 0., Fill::vertical(band, MID_SKY, HAZE));
    // Below the horizon the ground covers this; it only shows through gaps.
    let rest = Rect::new(0., horizon, w, (lower.y + lower.h - horizon).max(1.));
    shapes::fill_rounded_rect(rest, 0., HAZE);
}

/// Heat shimmer: a few faint, slowly wobbling haze lines hugging the horizon.
/// Static under reduced motion.
pub(super) fn shimmer(horizon_y: f32, time: f32, motion: f32) {
    let w = screen_width();
    let mut points = [Vec2::ZERO; 33];
    for line in 0..3 {
        let y = horizon_y - 3. - line as f32 * 5.;
        let alpha = 0.11 - line as f32 * 0.03;
        for (i, p) in points.iter_mut().enumerate() {
            let x = i as f32 / 32. * w;
            let wave = (x * 0.021 + time * (1.7 + line as f32 * 0.6)).sin()
                + (x * 0.047 - time * 2.3).sin() * 0.5;
            *p = vec2(x, y + wave * 1.6 * motion);
        }
        shapes::polyline(
            &points,
            shapes::Stroke::new(2.5, Color::new(1., 0.95, 0.85, alpha)),
        );
    }
}

/// Screen y of the horizon for the current view: a far point straight ahead.
pub(super) fn horizon(projector: &Projector, eye: Vec3, forward: Vec2) -> f32 {
    projector
        .project(world(vec2(eye.x, eye.z) + forward * 330., 0.))
        .map_or(screen_height() * 0.45, |(p, _)| p.y)
}

/// Emissive accents projected from the world: the sun halo, the next gate's
/// beacons and the twin engine nozzles (hotter and larger while boosting).
pub(super) fn glows(
    projector: &Projector,
    race: &Race,
    course: &super::race::Course,
    position: Vec2,
    yaw: f32,
    juice: &Juice,
    title: bool,
) {
    if let Some((p, _)) = projector.project(SUN_POSITION) {
        post::glow_circle(p.x, p.y, 120., Color::new(1., 0.86, 0.6, 1.), 0.55);
    }
    if !race.finished && !title {
        let gate = course.gates[race.next_gate];
        let pulse = 0.8 + 0.2 * (juice.time * 4.).sin() * juice.motion().min(1.);
        for side in [-1., 1.] {
            let marker = gate.position + right(gate.direction) * (HALF_WIDTH + 0.4) * side;
            if let Some((p, depth)) = projector.project(world(marker, 4.7)) {
                let r = (projector.scale(depth) * 1.6).clamp(6., 90.);
                post::glow_circle(p.x, p.y, r, GATE_GLOW, 0.9 * pulse);
            }
        }
    }
    let forward = vec2(yaw.sin(), yaw.cos());
    let side = right(forward);
    let heat = if race.boosting {
        1.
    } else {
        (race.speed() / 30.).clamp(0., 1.) * 0.55
    };
    if heat > 0.02 || title {
        let heat = if title { 0.6 } else { heat };
        let color = if race.boosting { WHITE_HOT } else { EMBER };
        for x in [-1.12, 1.12] {
            let nozzle =
                position - forward * (2.1 + if race.boosting { 0.4 } else { 0. }) + side * -x;
            if let Some((p, depth)) = projector.project(world(nozzle, HOVER_HEIGHT - 0.05)) {
                let r = (projector.scale(depth) * (0.55 + 0.5 * heat)).clamp(3., 120.);
                post::glow_circle(p.x, p.y, r, color, 0.6 + 1.1 * heat);
            }
        }
    }
    let _ = GATES;
}

/// Twin glowing ribbons from the nozzles: thick and white-hot on boost, a
/// faint ember line at cruise, gone at rest. Bounded rings; no allocation.
pub(super) struct Trails {
    trails: [Trail; 2],
}

impl Trails {
    pub fn new() -> Self {
        let config = TrailConfig {
            capacity: 48,
            lifetime: 0.42,
            spacing: 0.5,
            teleport_distance: 8.,
            max_per_update: 16,
            fade_width: true,
        };
        Self {
            trails: [
                Trail::new(config).expect("bounded trail"),
                Trail::new(config).expect("bounded trail"),
            ],
        }
    }

    pub fn clear(&mut self) {
        for trail in &mut self.trails {
            trail.clear();
        }
    }

    /// Age and extend by accepted time only; reduced motion clears them.
    pub fn update(&mut self, dt: f32, race: &Race, position: Vec2, yaw: f32, motion: f32) {
        if motion == 0. {
            self.clear();
            return;
        }
        let forward = vec2(yaw.sin(), yaw.cos());
        let side = right(forward);
        let speed = race.speed();
        let emit = !race.finished && speed > 8.;
        for (trail, x) in self.trails.iter_mut().zip([-1.12_f32, 1.12]) {
            let sample = emit.then(|| {
                let p = position - forward * 2.3 + side * -x;
                let (color, width) = if race.boosting {
                    ([1., 0.62, 0.25, 0.85], 0.42)
                } else {
                    let a = ((speed - 8.) / 30.).clamp(0., 1.) * 0.35;
                    ([1., 0.45, 0.15, a], 0.16)
                };
                TrailSample {
                    position: world(p, HOVER_HEIGHT - 0.05).to_array(),
                    color,
                    width,
                }
            });
            let _ = trail.update(dt, sample);
        }
    }

    pub fn draw(&self, eye: Vec3, up: Vec3) {
        for trail in &self.trails {
            if trail.is_empty() {
                continue;
            }
            let _ = draw_ribbon(
                trail,
                RibbonFacing::Camera {
                    eye: eye.to_array(),
                    up: up.to_array(),
                },
                2.,
                EffectStyle {
                    blend: EffectBlend::Additive,
                    ..Default::default()
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grades_bloom_only_above_lit_surfaces_and_keep_an_explicit_hud_boundary() {
        // Brightest authored world color is the pale curb/lintel (238/255).
        let pale = 238. / 255.;
        for scene in [Scene::Title, Scene::Race, Scene::Paused, Scene::Results] {
            let fx = grade(scene).sanitized();
            assert!(!fx.is_off());
            assert!(
                fx.bloom_threshold - fx.bloom_knee * 0.5 > pale - 0.02,
                "{scene:?}"
            );
            assert!(!fx.auto_hud, "glows rely on the explicit HUD boundary");
            assert!(fx.vignette <= 0.5 && fx.grain <= 0.03);
        }
        assert!(grade(Scene::Paused).saturation < grade(Scene::Race).saturation);
    }

    #[test]
    fn projector_matches_camera_conventions() {
        let camera = Camera3D {
            position: vec3(0., 2., -10.),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fovy: 1.,
            z_near: 0.1,
            z_far: 100.,
            ..Default::default()
        };
        let p = Projector::new(&camera, 800., 600.);
        let (center, depth) = p.project(Vec3::ZERO).unwrap();
        assert!((center - vec2(400., 300.)).length() < 0.01);
        assert!(depth > 9.);
        let (above, _) = p.project(vec3(0., 1., 0.)).unwrap();
        assert!(above.y < 300., "screen y grows downward");
        assert!(p.project(vec3(0., 2., -20.)).is_none(), "behind the eye");
    }

    #[test]
    fn trails_follow_boost_and_clear_under_reduced_motion() {
        let mut race = Race::new();
        race.countdown = 0.;
        race.velocity = race.forward() * 40.;
        race.boosting = true;
        let mut trails = Trails::new();
        let mut position = race.position;
        for _ in 0..30 {
            position += race.forward() * 0.4;
            trails.update(1. / 60., &race, position, race.yaw, 1.);
        }
        assert!(trails.trails.iter().all(|t| !t.is_empty()));
        trails.update(1. / 60., &race, position, race.yaw, 0.);
        assert!(trails.trails.iter().all(Trail::is_empty));
    }
}
