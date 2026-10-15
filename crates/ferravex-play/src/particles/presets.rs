//! Original procedural templates. Units are meters/seconds; scale the authored
//! velocity, acceleration and size for pixel-coordinate games.
use super::*;
pub fn sparks(color: [f32; 4]) -> EmitterDesc {
    EmitterDesc {
        particle: ParticleDesc {
            lifetime: 0.42,
            acceleration: [0., -9., 0.],
            start_color: color,
            end_color: [color[0], color[1], color[2], 0.],
            start_size: 0.07,
            end_size: 0.01,
            ..Default::default()
        },
        motion: ParticleMotion {
            drag: 1.2,
            angular_velocity: 7.,
            ..Default::default()
        },
        shape: SpawnShape::Sphere { radius: 0.04 },
        velocity_cone: Some(DirectionCone {
            axis: [0., 1., 0.],
            angle: 1.3,
            speed: [1., 4.],
        }),
        lifetime_jitter: 0.3,
        size_jitter: 0.4,
        rotation_jitter: std::f32::consts::PI,
        rate: 0.,
        max_per_update: 64,
        ..Default::default()
    }
}
pub fn dust(color: [f32; 4]) -> EmitterDesc {
    EmitterDesc {
        particle: ParticleDesc {
            lifetime: 0.7,
            velocity: [0., 0.5, -0.5],
            acceleration: [0., 0.2, 0.],
            start_color: color,
            end_color: [color[0], color[1], color[2], 0.],
            start_size: 0.15,
            end_size: 0.6,
            ..Default::default()
        },
        motion: ParticleMotion {
            drag: 1.5,
            angular_velocity: 0.5,
            ..Default::default()
        },
        shape: SpawnShape::Box {
            half: [0.6, 0.03, 0.1],
        },
        velocity_jitter: [0.6, 0.2, 0.3],
        lifetime_jitter: 0.2,
        size_jitter: 0.3,
        rotation_jitter: std::f32::consts::PI,
        inherit_velocity: 0.05,
        rate: 32.,
        ..Default::default()
    }
}
pub fn exhaust(color: [f32; 4]) -> EmitterDesc {
    EmitterDesc {
        particle: ParticleDesc {
            lifetime: 0.5,
            velocity: [0., 0., -2.],
            acceleration: [0., 0.4, 0.],
            start_color: color,
            end_color: [color[0], color[1], color[2], 0.],
            start_size: 0.08,
            end_size: 0.32,
            ..Default::default()
        },
        motion: ParticleMotion {
            drag: 0.8,
            ..Default::default()
        },
        shape: SpawnShape::Disc { radius: 0.07 },
        velocity_jitter: [0.12, 0.12, 0.4],
        size_jitter: 0.2,
        rotation_jitter: std::f32::consts::PI,
        rate: 24.,
        ..Default::default()
    }
}
