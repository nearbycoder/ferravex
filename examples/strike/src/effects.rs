//! Accepted shot traces and bounded procedural impacts. No effect manufactures
//! a hit confirmation, damage, a collision or an authoritative game event.
use ferravex_play::feedback::{FeedbackConfig, FeedbackController, FeedbackEvent};
use ferravex_play::particles::{
    self, BeamDesc, BeamPool, Emitter, EmitterDesc, EmitterTransform, OverflowPolicy, ParticleDesc,
    ParticlePool,
};
use ferravex_render::{
    Quat, Vec3,
    effects::{
        self, BillboardView, EffectBlend, EffectStyle, ParticleRenderer, ParticleStyle,
        RibbonFacing,
    },
};
pub(super) struct CombatEffects {
    pub beams: BeamPool,
    pub particles: ParticlePool,
    impacts: Emitter,
    muzzle: Emitter,
    renderer: ParticleRenderer,
    feedback: FeedbackController<64>,
    event_id: u64,
    pub fired: bool,
    pub hit_timer: f32,
    pub shot_timer: f32,
    pub muzzle_timer: f32,
    pub damage_flash: f32,
}
impl CombatEffects {
    pub fn new() -> Self {
        Self {
            beams: BeamPool::new(super::MAX_TRACERS, OverflowPolicy::ReplaceOldest),
            particles: ParticlePool::new(384),
            impacts: Emitter::new(
                particles::presets::sparks([1., 0.65, 0.24, 0.9]),
                0x535452494b45,
            )
            .unwrap(),
            muzzle: Emitter::new(
                EmitterDesc {
                    particle: ParticleDesc {
                        lifetime: 0.045,
                        start_color: [1., 0.76, 0.38, 0.9],
                        end_color: [1., 0.4, 0.12, 0.],
                        start_size: 0.16,
                        end_size: 0.035,
                        ..Default::default()
                    },
                    size_jitter: 0.25,
                    rotation_jitter: std::f32::consts::PI,
                    rate: 0.,
                    max_per_update: 8,
                    ..Default::default()
                },
                0x4d555a5a4c45,
            )
            .unwrap(),
            renderer: ParticleRenderer::new(384).unwrap(),
            feedback: FeedbackController::new(
                FeedbackConfig {
                    // Preserve Strike's weapon recoil; no camera shake or hit-stop.
                    motion: 0.,
                    flash: 0.11,
                    rumble: 0.,
                    networked: true,
                    ..Default::default()
                },
                0x535452494b45,
            )
            .unwrap(),
            event_id: 0,
            fired: false,
            hit_timer: 0.,
            shot_timer: 0.,
            muzzle_timer: 0.,
            damage_flash: 0.,
        }
    }
    /// Always run before accepting events, including menu/zero-tick frames.
    pub fn begin_frame(&mut self) {
        ferravex_render::preferences::sync_feedback(&mut self.feedback);
        self.fired = false;
        let policy = self.feedback.policy();
        if policy.reduced_motion() || policy.intensity() == 0. {
            self.shot_timer = 0.;
        }
        if policy.reduced_flash() || policy.intensity() == 0. {
            self.muzzle_timer = 0.;
        }
        if !self.particles_enabled() {
            self.beams.clear();
            self.particles.clear();
        }
        self.damage_flash = self.feedback.impact().flash;
    }
    fn particles_enabled(&self) -> bool {
        let policy = self.feedback.policy();
        !policy.reduced_motion() && !policy.reduced_flash() && policy.intensity() > 0.
    }
    fn emit(&mut self, event: FeedbackEvent) -> bool {
        // The source drains already accepted/acknowledged FPS events once.
        // These IDs are presentation-local, never protocol or simulation IDs.
        self.event_id = self.event_id.wrapping_add(1).max(1);
        matches!(self.feedback.emit(self.event_id, event), Ok(Some(_)))
    }
    pub fn local_shot(&mut self, origin: Vec3) -> bool {
        let Ok(event) = FeedbackEvent::fire(origin, 1.) else {
            return false;
        };
        if !self.emit(event) {
            return false;
        }
        self.fired = true;
        let policy = self.feedback.policy();
        if !policy.reduced_motion() {
            self.shot_timer = 0.09 * policy.intensity();
        }
        if !policy.reduced_flash() {
            self.muzzle_timer = 0.09 * policy.intensity();
        }
        true
    }
    pub fn hit(&mut self, attacker: bool, victim: bool) {
        if attacker {
            self.hit_timer = 0.25;
        }
        if victim {
            self.emit(FeedbackEvent::hurt(Vec3::ZERO, 1.).unwrap());
        }
    }
    pub fn update(&mut self, dt: f32) {
        self.shot_timer = (self.shot_timer - dt).max(0.);
        self.muzzle_timer = (self.muzzle_timer - dt).max(0.);
        self.hit_timer = (self.hit_timer - dt).max(0.);
        self.damage_flash = self
            .feedback
            .advance(dt)
            .expect("bounded presentation delta")
            .flash;
        self.beams.update(dt).unwrap();
        self.particles.update(dt).unwrap();
    }
    pub fn shot(&mut self, remote: bool, origin: Vec3, end: Vec3) {
        if !self.particles_enabled() {
            return;
        }
        if remote {
            let _ = self.beams.spawn(BeamDesc {
                start: origin.to_array(),
                end: end.to_array(),
                start_color: [0.91, 0.71, 0.42, 0.85],
                end_color: [1., 0.85, 0.62, 0.65],
                width: 0.018,
                lifetime: 0.09,
            });
        }
        // The authority's maximum-range endpoint is not an impact surface.
        let delta = origin - end;
        let distance = delta.length();
        if distance > 0.001 && distance < ferravex_fps::MAX_SHOT_DISTANCE - 0.01 {
            let normal = (delta / distance + Vec3::Y * 0.25).normalize_or_zero();
            let _ = self.impacts.burst(
                6,
                EmitterTransform {
                    position: end.to_array(),
                    rotation: Quat::from_rotation_arc(Vec3::Y, normal).to_array(),
                },
                &mut self.particles,
            );
        }
    }
    pub fn muzzle(&mut self, position: Vec3) {
        if !self.particles_enabled() {
            return;
        }
        let _ = self.muzzle.burst(
            2,
            EmitterTransform::at(position.to_array()),
            &mut self.particles,
        );
    }
    pub fn draw(&mut self, view: BillboardView) -> u32 {
        let style = EffectStyle {
            blend: EffectBlend::Additive,
            ..Default::default()
        };
        let beams = effects::draw_beams(
            &self.beams,
            RibbonFacing::Camera {
                eye: view.eye.to_array(),
                up: view.up.to_array(),
            },
            style,
        )
        .unwrap();
        let particles = self
            .renderer
            .draw_3d(
                &self.particles,
                view,
                ParticleStyle {
                    draw: style,
                    stretch: 0.035,
                    max_stretch: 4.,
                    ..Default::default()
                },
            )
            .unwrap();
        beams.triangles + particles.triangles
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_play::feedback::FeedbackPolicy;
    use ferravex_render::preferences;

    #[test]
    fn live_policy_clears_existing_channels_independently_without_reviving_them() {
        let previous = preferences::feedback_policy();
        preferences::set_feedback_policy(FeedbackPolicy::default());
        let mut effects = CombatEffects::new();
        effects.begin_frame();
        effects.local_shot(Vec3::ZERO);
        effects.hit(true, true);
        effects.shot(true, Vec3::Y, Vec3::Y - Vec3::Z * 5.);
        effects.muzzle(Vec3::ZERO);
        effects.update(0.);
        assert!(!effects.particles.is_empty());
        assert_eq!(effects.beams.len(), 1);
        assert!(effects.damage_flash > 0. && effects.muzzle_timer > 0.);
        assert!(effects.shot_timer > 0. && effects.hit_timer > 0.);

        // No simulation/aging is needed: a menu/zero-tick frame synchronizes it.
        preferences::set_feedback_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
        effects.begin_frame();
        assert_eq!(effects.particles.len(), 0);
        assert_eq!(effects.beams.len(), 0);
        assert_eq!(effects.shot_timer, 0.);
        assert!(effects.damage_flash > 0. && effects.muzzle_timer > 0.);
        assert_eq!(effects.hit_timer, 0.25, "hit confirmation is factual");
        effects.shot(true, Vec3::Y, Vec3::ZERO);
        effects.muzzle(Vec3::ZERO);
        assert_eq!(effects.particles.len(), 0);

        preferences::set_feedback_policy(FeedbackPolicy::default());
        effects.begin_frame();
        assert_eq!(effects.shot_timer, 0.);
        assert_eq!(effects.particles.len(), 0);
        effects.local_shot(Vec3::ZERO);
        preferences::set_feedback_policy(FeedbackPolicy::new(false, true, 1.).unwrap());
        effects.begin_frame();
        assert!(effects.shot_timer > 0.);
        assert_eq!(effects.damage_flash, 0.);
        assert_eq!(effects.muzzle_timer, 0.);
        effects.hit(false, true);
        effects.update(0.);
        assert_eq!(effects.damage_flash, 0.);
        preferences::set_feedback_policy(FeedbackPolicy::default());
        effects.begin_frame();
        assert_eq!(effects.damage_flash, 0.);
        assert_eq!(effects.muzzle_timer, 0.);
        assert_eq!(effects.particles.len(), 0);

        preferences::set_feedback_policy(FeedbackPolicy::new(false, false, 0.).unwrap());
        effects.begin_frame();
        assert!(
            effects.local_shot(Vec3::ZERO),
            "preferences do not reject authority"
        );
        effects.hit(true, true);
        effects.update(0.);
        assert_eq!(
            (
                effects.shot_timer,
                effects.muzzle_timer,
                effects.damage_flash
            ),
            (0., 0., 0.)
        );
        preferences::set_feedback_policy(previous);
    }
}
