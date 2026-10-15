//! Conservative transformed-bound visibility and per-object hysteretic LOD.
use crate::{Mat4, Vec3, retained::ResidentMesh};
use ferravex_graphics::{Bounds, Frustum};

#[derive(Clone, Copy, Debug)]
pub struct VisibilitySettings {
    pub frustum_culling: bool,
    /// Distance to the nearest point on the conservative world AABB, not its center.
    pub max_distance: Option<f32>,
}
impl Default for VisibilitySettings {
    fn default() -> Self {
        Self {
            frustum_culling: true,
            max_distance: None,
        }
    }
}
impl VisibilitySettings {
    pub fn valid(self) -> bool {
        self.max_distance.is_none_or(|n| n.is_finite() && n >= 0.)
    }
}
#[derive(Clone, Copy, Default, Debug)]
pub struct VisibilityStats {
    pub tested: u32,
    pub visible: u32,
    pub frustum_culled: u32,
    pub distance_culled: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Visible,
    OutsideFrustum,
    TooDistant,
    InvalidTransform,
}
pub fn distance_to_bounds(bounds: Bounds, model: Mat4, eye: Vec3) -> f32 {
    let center = model.transform_point3(bounds.center());
    let h = bounds.half_extents();
    let half = model.x_axis.truncate().abs() * h.x
        + model.y_axis.truncate().abs() * h.y
        + model.z_axis.truncate().abs() * h.z;
    ((eye - center).abs() - half).max(Vec3::ZERO).length()
}
pub fn classify(
    bounds: Bounds,
    model: Mat4,
    frustum: Option<Frustum>,
    eye: Option<Vec3>,
    settings: VisibilitySettings,
) -> Visibility {
    let a = model.to_cols_array();
    if !model.is_finite() || a[3] != 0. || a[7] != 0. || a[11] != 0. || a[15] != 1. {
        return Visibility::InvalidTransform;
    }
    if settings.frustum_culling && frustum.is_some_and(|f| !f.intersects_bounds(bounds, model)) {
        return Visibility::OutsideFrustum;
    }
    if let (Some(eye), Some(max)) = (eye, settings.max_distance)
        && distance_to_bounds(bounds, model, eye) > max
    {
        return Visibility::TooDistant;
    }
    Visibility::Visible
}
#[derive(Clone, Copy, Default, Debug)]
pub struct LodState {
    level: usize,
    initialized: bool,
}
impl LodState {
    pub fn level(self) -> usize {
        self.level
    }
}
pub struct LodChain {
    levels: Vec<ResidentMesh>,
    thresholds: Vec<f32>,
    hysteresis: f32,
    bounds: Bounds,
}
impl LodChain {
    /// 1..=16 authored levels and strictly increasing positive transition distances.
    /// Threshold distance is measured to the union of all level bounds.
    pub fn new(
        levels: Vec<ResidentMesh>,
        thresholds: Vec<f32>,
        hysteresis: f32,
    ) -> Result<Self, &'static str> {
        if levels.is_empty()
            || levels.len() > 16
            || thresholds.len() + 1 != levels.len()
            || !hysteresis.is_finite()
            || !(0. ..=0.5).contains(&hysteresis)
            || thresholds.iter().any(|t| !t.is_finite() || *t <= 0.)
            || thresholds.windows(2).any(|w| w[0] >= w[1])
        {
            return Err("invalid LOD levels/thresholds/hysteresis");
        }
        let mut bounds = levels[0].bounds();
        for level in &levels[1..] {
            let b = level.bounds();
            bounds.min = bounds.min.min(b.min);
            bounds.max = bounds.max.max(b.max);
        }
        Ok(Self {
            levels,
            thresholds,
            hysteresis,
            bounds,
        })
    }
    pub fn bounds(&self) -> Bounds {
        self.bounds
    }
    pub fn select(&self, distance: f32, state: &mut LodState) -> &ResidentMesh {
        let distance = if distance.is_finite() {
            distance.max(0.)
        } else {
            0.
        };
        if !state.initialized || state.level >= self.levels.len() {
            state.level = self.thresholds.partition_point(|t| distance > *t);
            state.initialized = true;
        } else {
            while state.level < self.thresholds.len()
                && distance > self.thresholds[state.level] * (1. + self.hysteresis)
            {
                state.level += 1;
            }
            while state.level > 0
                && distance < self.thresholds[state.level - 1] * (1. - self.hysteresis)
            {
                state.level -= 1;
            }
        }
        &self.levels[state.level]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{WHITE, mesh::Mesh3d};
    use ferravex_graphics::ClipDepth;
    #[test]
    fn conservative_visibility_handles_near_plane_scale_and_large_objects() {
        let bounds = Bounds {
            min: Vec3::splat(-1.),
            max: Vec3::ONE,
        };
        let f = Frustum::from_matrix(Mat4::IDENTITY, ClipDepth::NegativeOneToOne);
        assert_eq!(
            classify(bounds, Mat4::IDENTITY, f, None, Default::default()),
            Visibility::Visible
        );
        assert_eq!(
            classify(
                bounds,
                Mat4::from_translation(Vec3::X * 4.),
                f,
                None,
                Default::default()
            ),
            Visibility::OutsideFrustum
        );
        assert_eq!(
            classify(
                bounds,
                Mat4::from_translation(Vec3::Z * 1.5),
                f,
                None,
                Default::default()
            ),
            Visibility::Visible
        );
        assert_eq!(
            classify(
                bounds,
                Mat4::from_scale(Vec3::new(-5., 2., 1.)),
                f,
                None,
                Default::default()
            ),
            Visibility::Visible
        );
        let settings = VisibilitySettings {
            max_distance: Some(1.),
            ..Default::default()
        };
        assert_eq!(
            classify(
                bounds,
                Mat4::from_scale(Vec3::splat(100.)),
                None,
                Some(Vec3::X * 50.),
                settings
            ),
            Visibility::Visible
        );
        assert_eq!(
            classify(bounds, Mat4::IDENTITY, None, Some(Vec3::X * 50.), settings),
            Visibility::TooDistant
        );
    }
    #[test]
    fn lod_hysteresis_retains_level_near_boundary_and_handles_camera_jumps() {
        let mesh = Mesh3d::cuboid(Vec3::ONE, WHITE).resident().clone();
        let chain =
            LodChain::new(vec![mesh.clone(), mesh.clone(), mesh], vec![10., 30.], 0.1).unwrap();
        let mut state = LodState::default();
        for (distance, level) in [
            (0., 0),
            (10.5, 0),
            (12., 1),
            (9.5, 1),
            (8., 0),
            (100., 2),
            (0., 0),
        ] {
            chain.select(distance, &mut state);
            assert_eq!(state.level(), level);
        }
        let mut fresh = LodState::default();
        chain.select(100., &mut fresh);
        assert_eq!(fresh.level(), 2);
    }
    #[test]
    fn invalid_lod_configs_are_rejected() {
        assert!(LodChain::new(vec![], vec![], 0.1).is_err());
        let mesh = Mesh3d::cuboid(Vec3::ONE, WHITE).resident().clone();
        assert!(LodChain::new(vec![mesh.clone(), mesh.clone()], vec![f32::NAN], 0.1).is_err());
        assert!(LodChain::new(vec![mesh.clone()], vec![], 1.).is_err());
        assert!(
            LodChain::new(vec![mesh.clone(), mesh.clone(), mesh], vec![20., 10.], 0.1).is_err()
        );
    }
}
