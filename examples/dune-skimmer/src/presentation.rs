//! Presentation-only policy, clocks and hints. Never advances the race session.
use ferravex_play::{
    camera::{Input3, Mode3, Rig3, ShakeImpulse, Target3},
    feedback::FeedbackPolicy,
    particles::{Emitter, ParticlePool},
};
use ferravex_render::{input::PlayerInput, material::ShaderSource, preferences, prelude::*};

use crate::{
    DUST_SEED,
    race::{HOVER_HEIGHT, Race, world},
};

#[derive(Default)]
pub(super) struct Presentation {
    pub policy: FeedbackPolicy,
    pub motion_time: f32,
    flash_time: f32,
}

impl Presentation {
    /// Called before accepted feedback on every frame, even paused/zero-tick frames.
    pub fn sync(&mut self, camera: &mut Rig3, dust: &mut ParticlePool, emitter: &mut Emitter) {
        let policy = preferences::feedback_policy();
        if policy.reduced_motion() {
            // Clearing matters while paused: hiding a live effect would revive it
            // when the preference is disabled without another simulation tick.
            camera.shake.clear();
            dust.clear();
            emitter.reset(DUST_SEED);
            self.motion_time = 0.;
            if !self.policy.reduced_motion() {
                camera.cut(); // Discard the old speed/FOV smoothing target too.
            }
        }
        if policy.reduced_flash() {
            self.flash_time = 0.;
        }
        self.policy = policy;
    }

    pub fn advance(&mut self, dt: f32) {
        if !self.policy.reduced_motion() {
            self.motion_time = (self.motion_time + dt).rem_euclid(std::f32::consts::TAU);
        }
        if !self.policy.reduced_flash() {
            self.flash_time = (self.flash_time + dt).rem_euclid(std::f32::consts::TAU);
        }
    }

    pub fn reset(&mut self) {
        self.motion_time = 0.;
        self.flash_time = 0.;
    }

    pub fn impact(&self, camera: &mut Rig3) {
        if !self.policy.reduced_motion() && self.policy.intensity() > 0. {
            camera
                .shake
                .push(ShakeImpulse {
                    translation: vec3(0.025, 0.015, 0.) * self.policy.intensity(),
                    rotation: vec3(0.006, 0.004, 0.018) * self.policy.intensity(),
                    duration: 0.22,
                    frequency: 24.,
                })
                .expect("impact cooldown bounds shake layers");
        }
    }

    pub fn exhaust_parameters(&self) -> [[f32; 4]; 4] {
        [
            [self.flash_time, self.motion_time, 0., 0.],
            [0.; 4],
            [0.; 4],
            [0.; 4],
        ]
    }
}

pub(super) fn exhaust_source() -> ShaderSource {
    ShaderSource {
        // Retain both authored shader formulas and files. Give vertex motion its
        // own clock; the fragment retains the original u_params[0].x contract.
        vertex: include_str!("../shaders/exhaust.vert").replace("u_params[0].x", "u_params[0].y"),
        fragment: include_str!("../shaders/exhaust.frag").into(),
    }
}

pub(super) fn chase_input(race: &Race, position: Vec2, yaw: f32, policy: FeedbackPolicy) -> Input3 {
    let motion = !policy.reduced_motion();
    Input3 {
        target: Target3 {
            position: world(position, HOVER_HEIGHT),
            rotation: Quat::from_rotation_y(yaw),
            velocity: world(race.velocity, 0.),
        },
        mode: Mode3::Chase {
            distance: 10.7,
            height: 4.8 - HOVER_HEIGHT,
            look_ahead: 6.,
            look_height: 1.6 - HOVER_HEIGHT,
            speed_distance: if motion { 0.035 } else { 0. },
            speed_height: if motion { 0.015 } else { 0. },
            prediction: 0.,
            level: true,
        },
        fov_y: if motion && race.boosting {
            54_f32
        } else {
            51_f32
        }
        .to_radians(),
    }
}

pub(super) fn binding_hint<'a>(input: &PlayerInput, action: &str, buffer: &'a mut [u8]) -> &'a str {
    // Show the first current-device binding to keep narrow HUDs legible. Controls
    // lists every alternative. Never substitute a stale/default key for a remap.
    input
        .format_hint(action, buffer)
        .unwrap_or("See Controls")
        .split(" / ")
        .next()
        .unwrap_or("Unbound")
}

pub(super) fn text_width(text: &str, size: u16) -> f32 {
    Font.try_metrics(size, 1.)
        .and_then(|metrics| metrics.measure_line(text))
        .expect("finite bounded HUD label")
        .width
}

#[cfg(test)]
pub(super) fn fitted_size(text: &str, width: f32, size: u16) -> f32 {
    let measured = text_width(text, size).max(1.);
    (size as f32 * (width.max(1.) / measured).min(1.)).max(1.)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DUST_SEED, race::Course, session::RaceSession};
    use ferravex_play::{
        camera::NoObstructions,
        particles::{EmitterTransform, presets},
    };
    use ferravex_render::input::{Binding, InputCode, Preset};

    // The shared policy is thread-local; restore it even on an assertion failure.
    struct RestorePolicy(FeedbackPolicy);
    impl Drop for RestorePolicy {
        fn drop(&mut self) {
            preferences::set_feedback_policy(self.0);
        }
    }

    #[test]
    fn live_policy_clears_old_effects_during_pause_without_spending_a_tick() {
        let _restore = RestorePolicy(preferences::feedback_policy());
        preferences::set_feedback_policy(FeedbackPolicy::default());
        let mut trial = RaceSession::new();
        trial.race.boosting = true;
        trial.race.velocity = trial.race.forward() * 40.;
        let mut camera = Rig3::default();
        camera
            .update(
                0.,
                chase_input(
                    &trial.race,
                    trial.race.position,
                    trial.race.yaw,
                    FeedbackPolicy::default(),
                ),
                &NoObstructions,
            )
            .unwrap();
        let mut effects = Presentation::default();
        effects.impact(&mut camera);
        let mut dust = ParticlePool::new(32);
        let mut emitter = Emitter::new(presets::dust([1.; 4]), DUST_SEED).unwrap();
        emitter.set_rate(60.).unwrap();
        emitter
            .update(0.1, EmitterTransform::at([0.; 3]), &mut dust)
            .unwrap();
        assert!(!dust.is_empty());
        assert!(camera.shake.active() > 0);
        effects.advance(0.1);
        let old_flash = effects.exhaust_parameters()[0][0];
        trial.session.set_paused(true);
        let time = trial.race.elapsed;
        let countdown = trial.race.countdown;
        let tick = trial.session.tick();
        preferences::set_feedback_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
        effects.sync(&mut camera, &mut dust, &mut emitter);
        trial.advance(&Course::new(), Default::default(), false, 60.);
        effects.advance(0.);
        effects.impact(&mut camera); // A newly accepted event must also stay quiet.
        let pose = camera
            .update(
                0.,
                chase_input(
                    &trial.race,
                    trial.race.position,
                    trial.race.yaw,
                    effects.policy,
                ),
                &NoObstructions,
            )
            .unwrap()
            .pose;
        assert_eq!(pose.fov_y, 51_f32.to_radians());
        assert!(dust.is_empty());
        assert_eq!(camera.shake.active(), 0);
        assert_eq!(emitter.sequence(), 0);
        assert_eq!(effects.motion_time, 0.);
        assert_eq!(effects.exhaust_parameters()[0][0], old_flash);
        assert_eq!(
            (
                trial.race.elapsed,
                trial.race.countdown,
                trial.session.tick()
            ),
            (time, countdown, tick)
        );

        preferences::set_feedback_policy(FeedbackPolicy::default());
        effects.sync(&mut camera, &mut dust, &mut emitter);
        effects.advance(0.);
        assert!(dust.is_empty());
        assert_eq!(camera.shake.active(), 0);
        assert_eq!(effects.motion_time, 0.); // No suspended animation phase revives.
        effects.impact(&mut camera);
        assert_eq!(camera.shake.active(), 1); // A fresh event works again.
    }

    #[test]
    fn shader_motion_and_flash_clocks_are_independent_and_resettable() {
        let _restore = RestorePolicy(preferences::feedback_policy());
        let mut camera = Rig3::default();
        let mut dust = ParticlePool::new(16);
        let mut emitter = Emitter::new(presets::dust([1.; 4]), DUST_SEED).unwrap();
        for (motion, flash) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut effects = Presentation::default();
            effects.advance(0.1);
            preferences::set_feedback_policy(FeedbackPolicy::new(motion, flash, 1.).unwrap());
            effects.sync(&mut camera, &mut dust, &mut emitter);
            effects.advance(0.1);
            let clocks = effects.exhaust_parameters()[0];
            assert_eq!(clocks[0] == 0., flash);
            assert_eq!(clocks[1] == 0., motion);
            assert_eq!(effects.motion_time == 0., motion);
            if !motion {
                effects.impact(&mut camera);
                assert!(camera.shake.active() > 0); // Flash-only leaves camera feedback.
            }
            effects.reset();
            assert_eq!(effects.exhaust_parameters(), [[0.; 4]; 4]);
        }
        let shader = exhaust_source();
        assert_eq!(shader.vertex.matches("u_params[0].y").count(), 1);
        assert!(!shader.vertex.contains("u_params[0].x"));
        assert_eq!(shader.fragment, include_str!("../shaders/exhaust.frag"));
        assert_eq!(
            shader.vertex.replace("u_params[0].y", "u_params[0].x"),
            include_str!("../shaders/exhaust.vert")
        );
    }

    #[test]
    fn reduced_flash_clears_a_running_pulse_and_cannot_revive_it() {
        let _restore = RestorePolicy(preferences::feedback_policy());
        let mut effects = Presentation::default();
        let mut camera = Rig3::default();
        let mut dust = ParticlePool::new(8);
        let mut emitter = Emitter::new(presets::dust([1.; 4]), DUST_SEED).unwrap();
        effects.advance(0.12);
        let motion = effects.motion_time;
        preferences::set_feedback_policy(FeedbackPolicy::new(false, true, 1.).unwrap());
        effects.sync(&mut camera, &mut dust, &mut emitter);
        assert_eq!(effects.exhaust_parameters()[0], [0., motion, 0., 0.]);
        preferences::set_feedback_policy(FeedbackPolicy::default());
        effects.sync(&mut camera, &mut dust, &mut emitter);
        effects.advance(0.);
        assert_eq!(effects.exhaust_parameters()[0], [0., motion, 0., 0.]);
    }

    #[test]
    fn hints_follow_rebinds_unbound_actions_and_capacity_without_default_fallback() {
        let mut input = PlayerInput::preset("dune-skimmer-137-hints", Preset::HoverRacer);
        let mut buffer = [0; 256];
        assert_eq!(binding_hint(&input, "boost", &mut buffer), "Space");
        input
            .rebind(
                "boost",
                vec![Binding::digital(InputCode::Key(KeyCode::B as u16))],
            )
            .unwrap();
        assert_eq!(binding_hint(&input, "boost", &mut buffer), "B");
        input.rebind("boost", vec![]).unwrap();
        assert_eq!(binding_hint(&input, "boost", &mut buffer), "Unbound");
        assert_eq!(binding_hint(&input, "unknown", &mut buffer), "See Controls");
        assert_eq!(binding_hint(&input, "restart", &mut []), "See Controls");
    }

    #[test]
    fn default_narrow_hints_fit_without_shrinking_below_readable_sizes() {
        let input = PlayerInput::preset("dune-skimmer-137-layout", Preset::HoverRacer);
        let mut buffers = [[0; 256]; 4];
        let names = ["move_forward", "boost", "pause", "restart"];
        let hints: Vec<_> = names
            .iter()
            .zip(&mut buffers)
            .map(|(name, buffer)| binding_hint(&input, name, buffer))
            .collect();
        for width in [320., 390.] {
            for label in [
                format!("DRIVE {}  /  BOOST {}", hints[0], hints[1]),
                format!("PAUSE {}  /  RETRY {}  /  F2 CONTROLS", hints[2], hints[3]),
            ] {
                let fitted = fitted_size(&label, width - 32., 13);
                assert!(fitted >= 10., "{width} {label} => {fitted}");
                assert!(text_width(&label, 13) * fitted / 13. <= width - 31.99);
            }
        }
    }
}
