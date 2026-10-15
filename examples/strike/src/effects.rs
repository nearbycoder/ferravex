//! Accepted shot traces and bounded procedural impacts. No effect manufactures
//! a hit confirmation, damage, a collision or an authoritative game event.
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
        }
    }
    pub fn update(&mut self, dt: f32) {
        self.beams.update(dt).unwrap();
        self.particles.update(dt).unwrap();
    }
    pub fn shot(&mut self, remote: bool, origin: Vec3, end: Vec3) {
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
