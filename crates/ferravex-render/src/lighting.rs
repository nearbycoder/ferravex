//! Bounded frame lighting, explicit quality tiers and a directional shadow volume.
use crate::{Color, Mat4, Vec3};
pub const MAX_POINT_LIGHTS: usize = 8;
#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Quality {
    #[default]
    Low = 0,
    Medium = 1,
    High = 2,
}
impl Quality {
    pub fn shadow_size(self) -> u32 {
        match self {
            Self::Low => 0,
            Self::Medium => 512,
            Self::High => 1024,
        }
    }
    pub fn point_limit(self) -> usize {
        match self {
            Self::Low => 2,
            Self::Medium => 4,
            Self::High => 8,
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct PointLight {
    pub position: Vec3,
    pub color: Color,
    pub radius: f32,
    pub intensity: f32,
}
impl PointLight {
    pub fn valid(self) -> bool {
        self.position.is_finite()
            && self.position.abs().max_element() <= 1e6
            && self.color.to_vec().is_finite()
            && self.color.r >= 0.
            && self.color.g >= 0.
            && self.color.b >= 0.
            && self.color.r <= 1.
            && self.color.g <= 1.
            && self.color.b <= 1.
            && self.radius.is_finite()
            && (0.01..=1000.).contains(&self.radius)
            && self.intensity.is_finite()
            && (0. ..=16.).contains(&self.intensity)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct SunShadow {
    pub center: Vec3,
    pub direction: Vec3,
    pub extent: f32,
    pub depth: f32,
    pub bias: f32,
    pub normal_bias: f32,
    pub strength: f32,
}
impl Default for SunShadow {
    fn default() -> Self {
        Self {
            center: Vec3::ZERO,
            direction: Vec3::new(-0.4, 0.85, 0.3),
            extent: 30.,
            depth: 80.,
            bias: 0.001,
            normal_bias: 0.025,
            strength: 0.85,
        }
    }
}
impl SunShadow {
    pub fn valid(self) -> bool {
        self.center.is_finite()
            && self.center.abs().max_element() <= 1e6
            && self.direction.is_finite()
            && self.direction.length_squared().is_finite()
            && self.direction.length_squared() > 1e-8
            && self.extent.is_finite()
            && (0.1..=500.).contains(&self.extent)
            && self.depth.is_finite()
            && (1. ..=1000.).contains(&self.depth)
            && self.bias.is_finite()
            && (0. ..=0.1).contains(&self.bias)
            && self.normal_bias.is_finite()
            && (0. ..=1.).contains(&self.normal_bias)
            && self.strength.is_finite()
            && (0. ..=1.).contains(&self.strength)
    }
    pub fn matrix(self, size: u32) -> Mat4 {
        let direction = self.direction.normalize();
        let up = if direction.dot(Vec3::Y).abs() > 0.98 {
            Vec3::Z
        } else {
            Vec3::Y
        };
        let view = Mat4::look_at_rh(self.center + direction * self.depth, self.center, up);
        let mut projection = Mat4::orthographic_rh_gl(
            -self.extent,
            self.extent,
            -self.extent,
            self.extent,
            0.1,
            self.depth * 2.,
        );
        // Snap the light projection in texel units to reduce camera-follow shimmer.
        if size > 0 {
            let origin = projection * view * Vec3::ZERO.extend(1.);
            let texel = 2. / size as f32;
            let mut values = projection.to_cols_array();
            values[12] += (origin.x / texel).round() * texel - origin.x;
            values[13] += (origin.y / texel).round() * texel - origin.y;
            projection = Mat4::from_cols_array(&values);
        }
        projection * view
    }
}
/// Shared ABI: 384 bytes, with 16 point-light vec4s, shadow matrix and options, then metadata.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Environment {
    pub points: [f32; 64],
    pub shadow_matrix: [f32; 16],
    pub shadow_options: [f32; 4],
    pub shadow_extra: [f32; 4],
    pub shadow_direction: [f32; 4],
    pub light_count: u32,
    pub shadow_size: u32,
    pub pcf_radius: u32,
    pub quality: u32,
}
impl Default for Environment {
    fn default() -> Self {
        Self {
            points: [0.; 64],
            shadow_matrix: Mat4::IDENTITY.to_cols_array(),
            shadow_options: [0.; 4],
            shadow_extra: [0.; 4],
            shadow_direction: [0.; 4],
            light_count: 0,
            shadow_size: 0,
            pcf_radius: 0,
            quality: 0,
        }
    }
}
impl Environment {
    pub(crate) fn update(
        &mut self,
        lights: &[PointLight],
        sun: Option<SunShadow>,
        quality: Quality,
        eye: Option<Vec3>,
    ) {
        self.quality = quality as u32;
        self.points = [0.; 64];
        self.light_count = lights.len().min(quality.point_limit()) as u32;
        let mut order = [0usize; MAX_POINT_LIGHTS];
        for (i, index) in order.iter_mut().take(lights.len()).enumerate() {
            *index = i;
        }
        if let Some(eye) = eye {
            order[..lights.len()].sort_unstable_by(|a, b| {
                lights[*a]
                    .position
                    .distance_squared(eye)
                    .total_cmp(&lights[*b].position.distance_squared(eye))
            });
        }
        for (slot, &index) in order[..self.light_count as usize].iter().enumerate() {
            let light = lights[index];
            self.points[slot * 8..slot * 8 + 8].copy_from_slice(&[
                light.position.x,
                light.position.y,
                light.position.z,
                light.radius,
                light.color.r,
                light.color.g,
                light.color.b,
                light.intensity,
            ]);
        }
        self.shadow_size = sun.map_or(0, |_| quality.shadow_size());
        self.pcf_radius = if quality == Quality::High { 1 } else { 0 };
        if let Some(sun) = sun {
            self.shadow_matrix = sun.matrix(self.shadow_size).to_cols_array();
            let d = sun.direction.normalize();
            self.shadow_direction = [d.x, d.y, d.z, 0.];
            self.shadow_options = [sun.bias, sun.strength, self.pcf_radius as f32, 0.];
            self.shadow_extra = [
                sun.normal_bias,
                if self.shadow_size > 0 {
                    1. / self.shadow_size as f32
                } else {
                    0.
                },
                0.,
                0.,
            ];
        } else {
            self.shadow_options = [0.; 4];
            self.shadow_extra = [0.; 4];
            self.shadow_direction = [0.; 4];
        }
    }
    pub(crate) fn frustum(self) -> Option<ferravex_graphics::Frustum> {
        if self.shadow_size == 0 {
            None
        } else {
            ferravex_graphics::Frustum::from_matrix(
                Mat4::from_cols_array(&self.shadow_matrix),
                ferravex_graphics::ClipDepth::NegativeOneToOne,
            )
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quality_bounds_and_environment_layout() {
        assert_eq!(std::mem::size_of::<Environment>(), 384);
        assert_eq!(std::mem::offset_of!(Environment, light_count), 368);
        assert_eq!(Quality::Low.shadow_size(), 0);
        assert_eq!(Quality::High.point_limit(), 8);
        assert!(
            !SunShadow {
                direction: Vec3::ZERO,
                ..Default::default()
            }
            .valid()
        );
    }
    #[test]
    fn nearest_lights_and_vertical_shadow_direction_are_defined() {
        assert!(
            !SunShadow {
                direction: Vec3::splat(1e30),
                ..Default::default()
            }
            .valid()
        );
        let lights = [
            PointLight {
                position: Vec3::X * 20.,
                color: crate::WHITE,
                radius: 30.,
                intensity: 1.,
            },
            PointLight {
                position: Vec3::ZERO,
                color: crate::RED,
                radius: 3.,
                intensity: 1.,
            },
            PointLight {
                position: Vec3::Y,
                color: crate::BLUE,
                radius: 3.,
                intensity: 1.,
            },
        ];
        let mut env = Environment::default();
        env.update(
            &lights,
            Some(SunShadow {
                direction: Vec3::Y,
                ..Default::default()
            }),
            Quality::Low,
            Some(Vec3::ZERO),
        );
        assert_eq!(env.light_count, 2);
        assert_eq!(env.points[0], 0.);
        assert_eq!(env.shadow_size, 0);
        assert!(Mat4::from_cols_array(&env.shadow_matrix).is_finite());
        env.update(&lights, Some(Default::default()), Quality::High, None);
        assert!(env.frustum().unwrap().contains_point(Vec3::ZERO));
    }
}
