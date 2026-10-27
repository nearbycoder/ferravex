//! Time-of-day art direction. The orchard starts in soft morning light and
//! warms through the afternoon to a lantern-lit dusk by the last wave; a win
//! ends in a glowing harvest festival. Light is presentation only: it follows
//! accepted wave progress and never touches the simulation.
//!
//! Post-processing is chosen per state (play, pause, win, loss) and set only
//! when that state or the wave changes, as `set_post_fx` is sticky.
use crate::game::{Game, Outcome};
use crate::juice::{self, Juice};
use ferravex_play::tween::{Motion, Spring};
use ferravex_render::post::{self, PostFx, Tonemap};
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Fill};

/// Lamp posts beside the path; they light up as dusk falls.
pub(super) const LANTERNS: [Vec2; 6] = [
    vec2(222., 228.),
    vec2(298., 432.),
    vec2(540., 168.),
    vec2(812., 250.),
    vec2(812., 542.),
    vec2(1030., 542.),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Scene {
    Play(u8),
    Paused,
    Won,
    Lost,
}

/// Day phase target: 0 morning, ~0.5 golden afternoon, 1 lantern dusk.
pub(super) fn daylight_target(game: &Game) -> f32 {
    let waves = game.level.waves.len().max(2) as f32;
    match game.outcome {
        Outcome::Won => 1.,
        Outcome::Lost => 0.9,
        Outcome::Playing => {
            (f32::from(game.wave.saturating_sub(1)) / (waves - 1.)).clamp(0., 1.) * 0.85
        }
    }
}

pub(super) fn post_for(scene: Scene) -> PostFx {
    // Thresholds sit above the brightest authored paint (cream path ≈ 0.84),
    // so only additive glows bloom; the pale field never hazes.
    let cozy = PostFx::subtle()
        .bloom(1.1, 0.9, 1.2)
        .bloom_knee(0.2)
        .tonemap(Tonemap::Soft)
        .contrast(1.04)
        .saturation(1.08)
        .lift_gamma_gain([0.012, 0.008, 0.], [1., 1., 1.], [1.03, 1., 0.95])
        .vignette(0.24, 0.7);
    match scene {
        Scene::Play(wave) => cozy.vignette(0.22 + 0.04 * f32::from(wave.min(4)), 0.7),
        Scene::Paused => cozy.saturation(0.8).exposure(0.94).vignette(0.32, 0.65),
        Scene::Won => cozy
            .bloom(1.6, 0.85, 1.3)
            .saturation(1.14)
            .vignette(0.3, 0.7),
        Scene::Lost => cozy
            .saturation(0.72)
            .lift_gamma_gain([0., 0.006, 0.02], [1., 1., 1.], [0.95, 0.97, 1.04])
            .vignette(0.38, 0.6),
    }
}

pub(super) struct DayLight {
    phase: Spring,
    scene: Option<Scene>,
}

impl DayLight {
    pub fn new() -> Self {
        Self {
            phase: Spring::critical(0., 0.6),
            scene: None,
        }
    }

    pub fn phase(&self) -> f32 {
        self.phase.value.clamp(0., 1.)
    }

    /// Ease toward the game's time of day and apply the scene's post look.
    pub fn frame(&mut self, game: &Game, paused: bool, dt: f32, motion: Motion) {
        self.phase.set_target(daylight_target(game));
        self.phase.advance(dt, motion);
        let scene = if paused {
            Scene::Paused
        } else {
            match game.outcome {
                Outcome::Won => Scene::Won,
                Outcome::Lost => Scene::Lost,
                Outcome::Playing => Scene::Play(game.wave),
            }
        };
        if self.scene != Some(scene) {
            self.scene = Some(scene);
            post::set_post_fx(post_for(scene));
        }
    }

    /// Restart: snap back to morning.
    pub fn reset(&mut self) {
        self.phase.snap(0.);
    }

    /// Darkness 0..=1 used for lanterns and fireflies.
    pub fn dusk(&self) -> f32 {
        ((self.phase() - 0.55) / 0.45).clamp(0., 1.)
    }
}

/// Sun glow and sky wash drawn under the trees and towers.
pub(super) fn sky(light: &DayLight) {
    let p = light.phase();
    // Low morning sun from the left, warm afternoon from above-right.
    let sun = vec2(120. + 900. * p, 40. + 60. * (p - 0.5).abs());
    let warm = Color::new(1., 0.86 - 0.2 * p, 0.55 - 0.25 * p, 0.16 + 0.1 * p);
    shapes::fill_circle(
        sun,
        420.,
        Fill::radial(sun, 420., warm, Color::new(1., 0.8, 0.5, 0.)),
    );
}

/// Time-of-day wash over the whole board, then emissive lights on top.
pub(super) fn atmosphere(game: &Game, juice: &Juice, light: &DayLight, flash: f32) {
    let p = light.phase();
    let board = Rect::new(0., 70., 1200., 590.);
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    // Morning gold → clear noon → amber afternoon → violet dusk.
    let (top, bottom) = if p < 0.5 {
        let t = p / 0.5;
        (
            Color::new(1., 0.92, 0.70, lerp(0.10, 0.04, t)),
            Color::new(1., 0.95, 0.80, 0.),
        )
    } else {
        let t = (p - 0.5) / 0.5;
        (
            Color::new(
                lerp(1., 0.36, t),
                lerp(0.70, 0.28, t),
                lerp(0.40, 0.52, t),
                lerp(0.10, 0.34, t),
            ),
            Color::new(
                lerp(1., 0.95, t),
                lerp(0.75, 0.50, t),
                lerp(0.45, 0.35, t),
                lerp(0.06, 0.20, t),
            ),
        )
    };
    shapes::fill_rounded_rect(board, 0., Fill::vertical(board, top, bottom));
    let dusk = light.dusk();
    if dusk <= 0. {
        return;
    }
    // Lantern light pools, farmhouse windows and fireflies glow at dusk.
    for (i, at) in LANTERNS.into_iter().enumerate() {
        let flicker = 1. + (juice.clock * 7. + i as f32 * 1.7).sin() * 0.05 * juice.motion();
        post::glow_circle(
            at.x,
            at.y + 4.,
            70. * flicker,
            Color::new(1., 0.72, 0.36, 1.),
            0.35 * dusk,
        );
        post::glow_circle(
            at.x,
            at.y - 14.,
            12.,
            Color::new(1., 0.85, 0.5, 1.),
            1.6 * dusk,
        );
    }
    post::glow_circle(1134., 400., 26., Color::new(1., 0.78, 0.4, 1.), 1.2 * dusk);
    if juice.motion() > 0. {
        for i in 0..14 {
            let t = juice.clock * (0.25 + (i % 5) as f32 * 0.04) + i as f32 * 2.3;
            let x = ((i * 157 + 90) % 1150) as f32 + 25. + t.sin() * 30.;
            let y = 110. + ((i * 263 + 40) % 520) as f32 + (t * 1.7).cos() * 18.;
            let blink = ((t * 2.1).sin() * 0.5 + 0.5).powi(2);
            post::glow_circle(
                x,
                y,
                9.,
                Color::new(0.85, 1., 0.55, 1.),
                1.4 * dusk * blink * flash.max(0.4),
            );
        }
    }
    let _ = game;
}

/// Lamp posts: drawn with the world, lit by `dusk`.
pub(super) fn lamp_posts(light: &DayLight) {
    let dusk = light.dusk();
    for at in LANTERNS {
        draw_ellipse(
            at.x + 3.,
            at.y + 8.,
            8.,
            3.,
            0.,
            Color::new(0.2, 0.25, 0.15, 0.25),
        );
        draw_rectangle(at.x - 1.5, at.y - 14., 3., 22., Color::from_hex(0x5b4a35));
        let glass = Color::new(0.95, 0.80 + 0.12 * dusk, 0.45 + 0.2 * dusk, 1.);
        shapes::fill_rounded_rect(Rect::new(at.x - 5., at.y - 22., 10., 11.), 3., glass);
        draw_rectangle(at.x - 6., at.y - 24., 12., 3., Color::from_hex(0x3f3426));
    }
    let _ = juice::GOLD;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daylight_warms_with_waves_and_settles_on_the_outcome() {
        let mut game = Game::new();
        assert_eq!(daylight_target(&game), 0.);
        game.wave = 2;
        let mid = daylight_target(&game);
        game.wave = 4;
        let late = daylight_target(&game);
        assert!(0. < mid && mid < late && late <= 0.85);
        game.outcome = Outcome::Won;
        assert_eq!(daylight_target(&game), 1.);
        let mut light = DayLight::new();
        light.phase.set_target(1.);
        light.phase.advance(0.016, Motion::Reduced);
        assert_eq!(light.phase(), 1., "reduced motion snaps the sky");
        assert_eq!(light.dusk(), 1.);
        light.reset();
        assert_eq!(light.dusk(), 0.);
    }

    #[test]
    fn every_scene_keeps_bloom_above_the_painted_field() {
        for scene in [
            Scene::Play(0),
            Scene::Play(4),
            Scene::Paused,
            Scene::Won,
            Scene::Lost,
        ] {
            let fx = post_for(scene).sanitized();
            assert!(fx.bloom_threshold >= 0.85, "{scene:?}");
            assert!(fx.vignette <= 0.4 && fx.grain == 0.);
        }
    }
}
