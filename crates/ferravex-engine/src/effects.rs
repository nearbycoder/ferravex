//! Original bounded simulation and shared low-poly GPU effect rendering.
pub use ferravex_play::particles::*;
pub use ferravex_render::effects::{
    BillboardView, DrawReport, EffectBlend, EffectError, EffectStats, EffectStyle,
    ParticleRenderer, ParticleShape, ParticleStyle, draw_beams, draw_ribbon, draw_triangles,
    particle_triangles, stats as effect_stats,
};
