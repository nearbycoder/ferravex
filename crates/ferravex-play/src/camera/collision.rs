use super::{CameraError, bounded};
use ferravex_math::{Quat, Vec3};
pub const MAX_BLOCKERS: usize = 4096;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sweep {
    pub start: Vec3,
    pub end: Vec3,
    pub radius: f32,
}
impl Sweep {
    pub fn valid(self) -> bool {
        bounded(self.start)
            && bounded(self.end)
            && self.radius.is_finite()
            && (0.001..=1000.).contains(&self.radius)
    }
}
/// Return the first sphere-center impact fraction in `[0,1]`, or None if clear.
/// A blocked starting point must return Some(0), including zero-length sweeps.
pub trait CollisionQuery {
    fn sweep(&self, sweep: Sweep) -> Result<Option<f32>, CameraError>;
}
impl<F: Fn(Sweep) -> Result<Option<f32>, CameraError>> CollisionQuery for F {
    fn sweep(&self, sweep: Sweep) -> Result<Option<f32>, CameraError> {
        self(sweep)
    }
}
pub struct NoObstructions;
impl CollisionQuery for NoObstructions {
    fn sweep(&self, sweep: Sweep) -> Result<Option<f32>, CameraError> {
        if sweep.valid() {
            Ok(None)
        } else {
            Err(CameraError::InvalidQuery)
        }
    }
}
/// A small built-in adapter for authored environments. Box sweeps inflate the
/// local box by radius and are conservative at corners; physics adapters may
/// provide exact rounded-box/primitive sphere sweeps through CollisionQuery.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Blocker {
    Box {
        center: Vec3,
        half: Vec3,
        rotation: Quat,
    },
    Sphere {
        center: Vec3,
        radius: f32,
    },
    /// Solid half-space normal.dot(point) <= offset, with unit outward normal.
    Plane {
        normal: Vec3,
        offset: f32,
    },
}
impl Blocker {
    pub fn valid(self) -> bool {
        match self {
            Self::Box {
                center,
                half,
                rotation,
            } => {
                bounded(center)
                    && half.is_finite()
                    && half.min_element() > 0.
                    && half.max_element() <= 1e5
                    && rotation.is_finite()
                    && (rotation.length_squared() - 1.).abs() < 0.001
            }
            Self::Sphere { center, radius } => {
                bounded(center) && radius.is_finite() && (0.001..=1e5).contains(&radius)
            }
            Self::Plane { normal, offset } => {
                normal.is_finite()
                    && (normal.length_squared() - 1.).abs() < 0.001
                    && offset.is_finite()
                    && offset.abs() <= 1e6
            }
        }
    }
    fn fraction(self, sweep: Sweep) -> Option<f32> {
        match self {
            Self::Box {
                center,
                half,
                rotation,
            } => {
                let start = rotation.conjugate() * (sweep.start - center);
                let end = rotation.conjugate() * (sweep.end - center);
                let delta = end - start;
                let half = half + Vec3::splat(sweep.radius);
                let mut enter = 0_f32;
                let mut exit = 1_f32;
                for i in 0..3 {
                    if delta[i].abs() < 1e-10 {
                        if start[i].abs() > half[i] {
                            return None;
                        }
                    } else {
                        let a = (-half[i] - start[i]) / delta[i];
                        let b = (half[i] - start[i]) / delta[i];
                        enter = enter.max(a.min(b));
                        exit = exit.min(a.max(b));
                        if enter > exit {
                            return None;
                        }
                    }
                }
                Some(enter)
            }
            Self::Sphere { center, radius } => {
                let relative = sweep.start - center;
                let delta = sweep.end - sweep.start;
                let r = radius + sweep.radius;
                let c = relative.length_squared() - r * r;
                if c <= 0. {
                    return Some(0.);
                }
                let a = delta.length_squared();
                let b = relative.dot(delta);
                if a < 1e-20 || b >= 0. {
                    return None;
                }
                let discriminant = b * b - a * c;
                if discriminant < 0. {
                    return None;
                }
                let t = (-b - discriminant.sqrt()) / a;
                (0. ..=1.).contains(&t).then_some(t)
            }
            Self::Plane { normal, offset } => {
                let start = normal.dot(sweep.start) - offset - sweep.radius;
                if start <= 0. {
                    return Some(0.);
                }
                let end = normal.dot(sweep.end) - offset - sweep.radius;
                if end > 0. {
                    None
                } else {
                    Some(start / (start - end))
                }
            }
        }
    }
}
impl CollisionQuery for [Blocker] {
    fn sweep(&self, sweep: Sweep) -> Result<Option<f32>, CameraError> {
        if !sweep.valid() {
            return Err(CameraError::InvalidQuery);
        }
        if self.len() > MAX_BLOCKERS {
            return Err(CameraError::QueryBudget);
        }
        let mut first = None;
        for &blocker in self {
            if !blocker.valid() {
                return Err(CameraError::InvalidObstacle);
            }
            if let Some(t) = blocker.fraction(sweep) {
                first = Some(first.map_or(t, |old: f32| old.min(t)));
            }
        }
        Ok(first)
    }
}
