//! Original built-in faceted lighting and distance fog.
//!
//! Immediate geometry evaluates lighting on the CPU. Retained meshes evaluate
//! directional lighting/fog plus scene lights/shadows in the GPU fragment shader. Both preserve flat
//! face normals and require no external material resources.
use crate::prelude::{Color, Vec3, vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LitSettings {
    /// World-space direction from a surface toward the directional light.
    pub light_direction: Vec3,
    pub camera_position: Vec3,
    pub fog_color: Color,
    pub fog_start: f32,
    pub fog_end: f32,
    pub ambient: f32,
}

impl Default for LitSettings {
    fn default() -> Self {
        Self {
            light_direction: vec3(-0.4, 0.85, 0.3),
            camera_position: vec3(12.0, 10.0, 16.0),
            fog_color: Color::from_rgba(14, 22, 35, 255),
            fog_start: 20.0,
            fog_end: 55.0,
            ambient: 0.38,
        }
    }
}

impl LitSettings {
    /// Normalize the light and bound user-supplied settings once per material begin.
    /// Nonfinite settings use their defaults; a zero light direction is ambient-only.
    pub fn sanitized(self) -> Self {
        let defaults = Self::default();
        let finite = |value: f32, fallback: f32| {
            if value.is_finite() { value } else { fallback }
        };
        Self {
            light_direction: if self.light_direction.is_finite() {
                self.light_direction.normalize_or_zero()
            } else {
                defaults.light_direction.normalize_or_zero()
            },
            camera_position: if self.camera_position.is_finite() {
                self.camera_position
            } else {
                defaults.camera_position
            },
            fog_color: Color::new(
                finite(self.fog_color.r, defaults.fog_color.r).clamp(0.0, 1.0),
                finite(self.fog_color.g, defaults.fog_color.g).clamp(0.0, 1.0),
                finite(self.fog_color.b, defaults.fog_color.b).clamp(0.0, 1.0),
                finite(self.fog_color.a, defaults.fog_color.a).clamp(0.0, 1.0),
            ),
            fog_start: finite(self.fog_start, defaults.fog_start).max(0.0),
            fog_end: finite(self.fog_end, defaults.fog_end).max(0.0),
            ambient: finite(self.ambient, defaults.ambient).clamp(0.0, 1.0),
        }
    }

    /// Shade an already world-transformed position and normal, retaining alpha.
    ///
    /// Pass settings through [`Self::sanitized`] before a batch so the light is
    /// normalized once. Normals may be unnormalized or zero. Reversed/coincident
    /// fog bounds become a 0.001-unit transition starting at `fog_start`.
    pub fn shade_vertex(self, position: Vec3, normal: Vec3, color: Color) -> Color {
        let diffuse = normal
            .normalize_or_zero()
            .dot(self.light_direction)
            .clamp(0.0, 1.0);
        let shade = self.ambient + (1.0 - self.ambient) * diffuse;
        let fog = ((position.distance(self.camera_position) - self.fog_start)
            / (self.fog_end - self.fog_start).max(0.001))
        .clamp(0.0, 1.0);
        Color::new(
            color.r * shade * (1.0 - fog) + self.fog_color.r * fog,
            color.g * shade * (1.0 - fog) + self.fog_color.g * fog,
            color.b * shade * (1.0 - fog) + self.fog_color.b * fog,
            color.a,
        )
    }
}

/// Built-in faceted lighting material; no external GPU resources are required.
///
/// `begin` applies to subsequently recorded geometry until `end` or another
/// material begins. Recorded draw colors retain their material state.
#[derive(Clone, Copy, Debug, Default)]
pub struct LitMaterial;

impl LitMaterial {
    /// Kept fallible for source compatibility with games that load materials.
    pub fn new() -> Result<Self, String> {
        Ok(Self)
    }

    pub fn begin(&self, settings: LitSettings) {
        crate::set_lighting(Some(settings.sanitized()));
    }

    pub fn end(&self) {
        crate::set_lighting(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> LitSettings {
        LitSettings {
            light_direction: Vec3::Y,
            camera_position: Vec3::ZERO,
            fog_color: Color::new(0.2, 0.4, 0.6, 1.0),
            fog_start: 10.0,
            fog_end: 20.0,
            ambient: 0.25,
        }
    }

    fn close(actual: Color, expected: Color) {
        assert!(actual.to_vec().abs_diff_eq(expected.to_vec(), 0.00001));
    }

    #[test]
    fn diffuse_faces_receive_light_back_faces_receive_ambient() {
        let material = settings().sanitized();
        let color = Color::new(0.8, 0.6, 0.4, 0.3);
        close(
            material.shade_vertex(Vec3::ZERO, Vec3::Y * 7.0, color),
            color,
        );
        for normal in [-Vec3::Y, Vec3::X, Vec3::ZERO] {
            close(
                material.shade_vertex(Vec3::ZERO, normal, color),
                Color::new(0.2, 0.15, 0.1, 0.3),
            );
        }
    }

    #[test]
    fn fog_interpolates_by_world_distance_and_preserves_alpha() {
        let material = settings().sanitized();
        let color = Color::new(0.8, 0.6, 0.4, 0.3);
        close(material.shade_vertex(Vec3::X * 10.0, Vec3::Y, color), color);
        close(
            material.shade_vertex(Vec3::X * 15.0, Vec3::Y, color),
            Color::new(0.5, 0.5, 0.5, 0.3),
        );
        close(
            material.shade_vertex(Vec3::X * 25.0, Vec3::Y, color),
            material.fog_color.with_alpha(0.3),
        );
        let moved_eye = LitSettings {
            camera_position: Vec3::X * 15.0,
            ..material
        };
        close(
            moved_eye.shade_vertex(Vec3::X * 15.0, Vec3::Y, color),
            color,
        );
    }

    #[test]
    fn light_normalization_zero_light_and_collapsed_fog_are_defined() {
        let color = Color::new(1.0, 1.0, 1.0, 0.7);
        let material = LitSettings {
            light_direction: Vec3::Y * 8.0,
            fog_end: 5.0,
            ..settings()
        }
        .sanitized();
        close(material.shade_vertex(Vec3::ZERO, Vec3::Y, color), color);
        close(
            material.shade_vertex(Vec3::X * 11.0, Vec3::Y, color),
            material.fog_color.with_alpha(0.7),
        );
        let ambient_only = LitSettings {
            light_direction: Vec3::ZERO,
            ..material
        };
        close(
            ambient_only.shade_vertex(Vec3::ZERO, Vec3::Y, color),
            Color::new(0.25, 0.25, 0.25, 0.7),
        );
    }

    #[test]
    fn invalid_settings_do_not_poison_recorded_vertex_colors() {
        let material = LitSettings {
            light_direction: Vec3::NAN,
            camera_position: Vec3::NAN,
            fog_color: Color::new(f32::NAN, f32::INFINITY, -1.0, 8.0),
            fog_start: f32::NAN,
            fog_end: f32::INFINITY,
            ambient: f32::NAN,
        }
        .sanitized();
        let color = material.shade_vertex(Vec3::ZERO, Vec3::Y, Color::new(1.0, 1.0, 1.0, 1.0));
        assert!(color.to_vec().is_finite());
        assert!(
            color
                .to_array()
                .into_iter()
                .all(|channel| (0.0..=1.0).contains(&channel))
        );
        assert_eq!(material.fog_color.b, 0.0);
        assert_eq!(material.fog_color.a, 1.0);
    }
}
