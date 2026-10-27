//! Foundry atmosphere: the dusk sky, furnace light, the molten channel,
//! chimney smoke, drifting embers, red-hot shot, sparks, dust and flashes.
//!
//! Everything here is drawn in screen space between the 3D world and the
//! post-processing HUD boundary, so it is graded and bright pieces bloom.
//! It only reads simulation state. Ambient motion (embers, smoke, flicker)
//! follows a presentation clock that stops under reduced motion; flicker is
//! removed under reduced flash. Positions are pure functions of that clock and
//! an index hash, so nothing is stored or allocated.
use super::juice::PuffKind;
use super::*;
use ferravex_render::post;
use ferravex_render::shapes::{self, Fill, Stroke};

/// Sky, from smoky zenith to an ember-lit horizon.
pub(super) const SKY_TOP: Color = Color::from_rgba(20, 17, 22, 255);
const SKY_MID: Color = Color::from_rgba(52, 34, 34, 255);
const SKY_LOW: Color = Color::from_rgba(122, 70, 46, 255);
/// Fog matches the horizon haze so distance reads as smoky air.
pub(super) const HAZE: Color = Color::from_rgba(70, 46, 40, 255);
pub(super) const MOLTEN: Color = Color::from_rgba(255, 138, 40, 255);
const HOT: Color = Color::new(1.0, 0.86, 0.5, 1.0);

/// The furnace mouth on the foundry building's front wall.
pub(super) const FURNACE: Vec3 = vec3(-13.4, 1.25, -11.8);
/// The molten channel runs down the left of the range toward the cannon.
pub(super) const CHANNEL_X: f32 = -12.6;
pub(super) const CHANNEL_Z: (f32, f32) = (-11.0, 17.0);
const CHIMNEYS: [Vec3; 2] = [vec3(-12.0, 12.1, -17.0), vec3(-16.0, 11.1, -18.0)];
const EMBERS: usize = 34;
const SMOKE: usize = 9;

/// Presentation clock and channel scales for the atmosphere.
#[derive(Clone, Copy, Debug)]
pub(super) struct Air {
    /// Seconds of ambient motion; frozen while reduced motion is on.
    pub time: f32,
    /// 0..=1 motion scale (0 under reduced motion).
    pub motion: f32,
    /// 0..=1 flicker scale (0 under reduced flash).
    pub flicker: f32,
}

fn hash(i: u32, salt: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9e37_79b9) ^ salt.wrapping_mul(0x85eb_ca6b);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    (x & 0x00ff_ffff) as f32 / 0x0100_0000 as f32
}

fn fade(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * alpha.clamp(0.0, 1.0))
}

fn heat(t: f32) -> Color {
    // 1 = white-yellow, 0 = deep orange-red.
    shapes::mix(Color::from_rgba(255, 84, 24, 255), HOT, t.clamp(0.0, 1.0))
}

/// Post-processing per screen. Only additive emissives (molten metal, red-hot
/// shot, sparks, flashes) cross the high bloom threshold; the grade warms
/// highlights, lifts shadows toward ember brown and frames the range with a
/// soft vignette. The HUD is drawn after an explicit boundary, so it stays
/// crisp. Reduced flash caps bloom inside the engine.
pub(super) fn look(state: SessionState) -> post::PostFx {
    let base = post::PostFx::warm_film()
        .bloom(1.25, 0.92, 1.25)
        .bloom_knee(0.2)
        .contrast(1.1)
        .saturation(1.08)
        .lift_gamma_gain([0.03, 0.012, 0.0], [1.0, 0.98, 0.95], [1.08, 0.98, 0.86])
        .grain(0.02)
        .auto_hud(false);
    match state {
        SessionState::Playing => base.vignette(0.34, 0.62),
        // Title: deeper frame and a touch more glow for the attract scene.
        SessionState::Ready => base.vignette(0.48, 0.55).bloom(1.5, 0.9, 1.35),
        // Pause: the foundry cools and dims behind the card.
        SessionState::Paused => base.vignette(0.55, 0.5).saturation(0.55).exposure(0.72),
        SessionState::Won => base.vignette(0.38, 0.6).bloom(1.5, 0.9, 1.35),
        SessionState::Lost => base.vignette(0.48, 0.55).saturation(0.8).exposure(0.88),
    }
}

/// Full-screen dusk gradient behind the 3D world.
pub(super) fn sky(w: f32, h: f32) {
    let upper = Rect::new(0.0, 0.0, w, h * 0.32);
    shapes::fill_rounded_rect(upper, 0.0, Fill::vertical(upper, SKY_TOP, SKY_MID));
    let lower = Rect::new(0.0, h * 0.32 - 1.0, w, h * 0.68 + 1.0);
    shapes::fill_rounded_rect(lower, 0.0, Fill::vertical(lower, SKY_MID, SKY_LOW));
}

struct Projector<'a> {
    camera: &'a RangeCamera,
    w: f32,
    h: f32,
}

impl Projector<'_> {
    fn at(&self, p: Vec3) -> Option<Vec2> {
        self.camera.project(self.w, self.h, p)
    }

    /// Pixels per metre at a world point.
    fn scale(&self, p: Vec3) -> f32 {
        match (self.at(p), self.at(p + Vec3::Y)) {
            (Some(a), Some(b)) => a.distance(b),
            _ => 0.0,
        }
    }
}

/// Everything world-attached that should be graded and may bloom.
pub(super) fn world(foundry: &Foundry, camera: &RangeCamera, w: f32, h: f32, air: Air) {
    let view = Projector { camera, w, h };
    let juice = &foundry.juice;
    let game = &foundry.range;
    let t = air.time;
    let flicker = |i: u32| {
        1.0 + air.flicker
            * 0.12
            * ((t * (7.0 + i as f32)).sin() * 0.6 + (t * 13.3 + i as f32).sin() * 0.4)
    };

    // Furnace light pool on the floor, then the mouth itself.
    if let Some(at) = view.at(vec3(FURNACE.x + 1.0, 0.0, FURNACE.z + 3.0)) {
        let r = view.scale(FURNACE) * 5.5;
        post::glow_circle(
            at.x,
            at.y,
            r,
            Color::from_rgba(255, 110, 40, 255),
            0.3 * flicker(1),
        );
    }
    if let Some(at) = view.at(FURNACE) {
        let r = view.scale(FURNACE) * 1.7;
        post::glow_circle(at.x, at.y, r, MOLTEN, 1.2 * flicker(2));
    }
    // The molten channel: a wide warm halo, then a hot core line.
    let mut points = [Vec2::ZERO; 12];
    let mut count = 0;
    for i in 0..12 {
        let z = CHANNEL_Z.0 + (CHANNEL_Z.1 - CHANNEL_Z.0) * i as f32 / 11.0;
        if let Some(p) = view.at(vec3(CHANNEL_X, 0.08, z)) {
            points[count] = p;
            count += 1;
        }
    }
    if count >= 2 {
        let near = view.scale(vec3(CHANNEL_X, 0.0, CHANNEL_Z.1));
        post::with_glow(|| {
            shapes::polyline(
                &points[..count],
                Stroke::new(
                    near * 0.9,
                    fade(Color::from_rgba(255, 90, 30, 255), 0.12 * flicker(3)),
                ),
            );
            shapes::polyline(
                &points[..count],
                Stroke::new(near * 0.22, fade(MOLTEN, 0.75 * flicker(4))),
            );
        });
        // A few slow bright blobs flow toward the cannon.
        for i in 0..5_u32 {
            let phase = (hash(i, 7) + t * 0.05).fract();
            let z = CHANNEL_Z.0 + (CHANNEL_Z.1 - CHANNEL_Z.0) * phase;
            let p = vec3(CHANNEL_X, 0.1, z);
            if let Some(at) = view.at(p) {
                post::glow_circle(at.x, at.y, view.scale(p) * 0.5, HOT, 0.7);
            }
        }
    }
    // Chimney mouths glow; smoke rolls off downwind.
    for (c, top) in CHIMNEYS.iter().enumerate() {
        for i in 0..SMOKE as u32 {
            let seed = i + c as u32 * 31;
            let life = (hash(seed, 1) + t * 0.07).fract();
            let p = *top
                + vec3(
                    life * 6.0 + hash(seed, 2) * 0.8,
                    life * 5.0 + hash(seed, 3) * 0.5,
                    life * 2.0,
                );
            if let Some(at) = view.at(p) {
                let r = view.scale(p) * (0.9 + life * 2.6);
                let alpha = (life * 6.0).min(1.0) * (1.0 - life) * 0.55;
                shapes::fill_circle(
                    at,
                    r,
                    Fill::radial(
                        at,
                        r,
                        Color::new(0.16, 0.12, 0.12, alpha),
                        Color::new(0.16, 0.12, 0.12, 0.0),
                    ),
                );
            }
        }
        if let Some(at) = view.at(*top) {
            post::glow_circle(
                at.x,
                at.y,
                view.scale(*top) * 1.4,
                MOLTEN,
                0.5 * flicker(5 + c as u32),
            );
        }
    }
    // Embers drift up from the furnace and channel across the range.
    post::with_glow(|| {
        for i in 0..EMBERS as u32 {
            let life = (hash(i, 11) + t * (0.05 + 0.05 * hash(i, 12))).fract();
            let from_channel = i % 2 == 0;
            let base = if from_channel {
                vec3(
                    CHANNEL_X,
                    0.2,
                    CHANNEL_Z.0 + (CHANNEL_Z.1 - CHANNEL_Z.0) * hash(i, 13),
                )
            } else {
                FURNACE + vec3(hash(i, 13) * 3.0 - 1.5, 0.5, 0.5)
            };
            let sway = (t * 1.3 + i as f32).sin() * 0.6;
            let p = base + vec3(life * 4.0 + sway, life * 9.0, life * 1.5);
            if let Some(at) = view.at(p) {
                let alpha = (life * 8.0).min(1.0) * (1.0 - life);
                let size = (view.scale(p) * 0.08).clamp(1.2, 3.5);
                shapes::fill_circle(at, size, fade(heat(1.0 - life), alpha));
            }
        }
    });

    // Red-hot shot: every ball glows, hottest while flying fast.
    for shot in &game.shots {
        let body = &game.physics.bodies[shot.handle];
        let p = Vec3::from_array(body.translation().to_array());
        let speed = Vec3::from_array(body.linvel().to_array()).length();
        if let Some(at) = view.at(p) {
            let hot = (speed / SPEED).clamp(0.25, 1.0);
            post::glow_circle(
                at.x,
                at.y,
                view.scale(p) * (0.7 + 0.5 * hot),
                heat(hot),
                0.9 + 1.2 * hot,
            );
        }
    }
    // Ember trail behind the newest ball.
    let len = juice.trail_len;
    for (i, p) in juice.trail[..len].iter().enumerate().skip(1) {
        if let Some(at) = view.at(*p) {
            let k = i as f32 / len as f32;
            let r = view.scale(*p) * 0.24 * (1.0 - 0.6 * k);
            shapes::fill_circle(at, r * 1.6, Color::new(0.2, 0.17, 0.16, 0.35 * (1.0 - k)));
            post::with_glow(|| {
                shapes::fill_circle(at, r * 0.7, fade(heat(1.0 - k), 0.8 * (1.0 - k)))
            });
        }
    }
    // Impact dust and muzzle smoke.
    for puff in juice.puffs.iter().flatten() {
        let Some(at) = view.at(puff.position) else {
            continue;
        };
        let r = puff.current_radius() * view.scale(puff.position);
        let at = at + vec2(puff.spin.cos(), puff.spin.sin()) * r * 0.04;
        let alpha = puff.alpha();
        match puff.kind {
            PuffKind::Dust => {
                let c = Color::new(0.72, 0.6, 0.48, 0.7 * alpha);
                shapes::fill_circle(at, r, Fill::radial(at, r, c, fade(c, 0.15)));
            }
            PuffKind::Smoke => {
                let c = Color::new(0.34, 0.3, 0.29, 0.62 * alpha);
                shapes::fill_circle(at, r, Fill::radial(at, r, c, fade(c, 0.1)));
            }
            PuffKind::Flash => post::glow_circle(at.x, at.y, r * 1.6, HOT, 1.8 * alpha),
        }
    }
    for ring in juice.rings.iter().flatten() {
        if let Some(at) = view.at(ring.at) {
            let k = ring.age / ring.life;
            let r = ring.radius * (0.4 + k) * view.scale(ring.at);
            shapes::glow_ring(at, r, 6.0 + 10.0 * (1.0 - k), fade(MOLTEN, 0.9 * (1.0 - k)));
        }
    }
    if juice.flash > 0.0 {
        let muzzle = PIVOT + game.direction() * 1.9;
        if let Some(at) = view.at(muzzle) {
            let r = view.scale(muzzle) * (1.0 + 1.2 * juice.flash);
            post::glow_circle(at.x, at.y, r, HOT, 2.5 * juice.flash);
        }
    }
    // Sparks: additive streaks along their velocity, cooling as they age.
    post::with_glow(|| {
        for spark in juice.sparks.iter().flatten() {
            let (Some(a), Some(b)) = (
                view.at(spark.position),
                view.at(spark.position - spark.velocity * 0.035),
            ) else {
                continue;
            };
            let k = spark.heat();
            shapes::polyline(
                &[a, b],
                Stroke::new(1.0 + 1.6 * k, fade(heat(k), 0.4 + 0.6 * k)),
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambient_hash_is_deterministic_and_in_range() {
        for i in 0..200 {
            let a = hash(i, 3);
            assert_eq!(a, hash(i, 3));
            assert!((0.0..1.0).contains(&a));
        }
        assert_ne!(hash(1, 3), hash(2, 3));
    }
}
