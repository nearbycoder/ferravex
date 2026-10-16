//! Parametric solid blueprints shared by rendering, collision and authoring.
//! Construct once, then instantiate the exact same pieces in both hosts.
use ferravex_math::{Quat, Vec3};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildError {
    Invalid,
    Capacity,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxPiece {
    pub center: Vec3,
    pub half_extents: Vec3,
    pub rotation: Quat,
    pub material: u16,
}
impl BoxPiece {
    pub fn bounds(self) -> (Vec3, Vec3) {
        let h = (self.rotation * Vec3::X).abs() * self.half_extents.x
            + (self.rotation * Vec3::Y).abs() * self.half_extents.y
            + (self.rotation * Vec3::Z).abs() * self.half_extents.z;
        (self.center - h, self.center + h)
    }
}
#[derive(Clone, Debug)]
pub struct EnvironmentPlan {
    pieces: Vec<BoxPiece>,
    capacity: usize,
}
fn dimension(v: f32) -> bool {
    v.is_finite() && (0.001..=1000.).contains(&v)
}
impl EnvironmentPlan {
    pub fn new(capacity: usize) -> Result<Self, BuildError> {
        if capacity == 0 || capacity > 1024 {
            return Err(BuildError::Capacity);
        }
        Ok(Self {
            pieces: Vec::with_capacity(capacity),
            capacity,
        })
    }
    pub fn pieces(&self) -> &[BoxPiece] {
        &self.pieces
    }
    fn validate(&self, origin: Vec3, dimensions: &[f32], count: usize) -> Result<(), BuildError> {
        if !origin.is_finite()
            || origin.abs().max_element() > 100000.
            || !dimensions.iter().all(|v| dimension(*v))
        {
            return Err(BuildError::Invalid);
        }
        if self.pieces.len() + count > self.capacity {
            return Err(BuildError::Capacity);
        }
        Ok(())
    }
    /// Origin is the low edge at floor height. Each solid riser extends to floor.
    pub fn stairs(
        &mut self,
        origin: Vec3,
        width: f32,
        rise: f32,
        run: f32,
        count: u16,
        material: u16,
    ) -> Result<(), BuildError> {
        if count == 0 || count > 128 {
            return Err(BuildError::Invalid);
        }
        self.validate(origin, &[width, rise, run], usize::from(count))?;
        for i in 0..count {
            let h = rise * f32::from(i + 1);
            self.pieces.push(BoxPiece {
                center: origin + Vec3::new(0., h * 0.5, (f32::from(i) + 0.5) * run),
                half_extents: Vec3::new(width * 0.5, h * 0.5, run * 0.5),
                rotation: Quat::IDENTITY,
                material,
            });
        }
        Ok(())
    }
    /// A single oriented slab; rise/run specify its centerline endpoints.
    pub fn ramp(
        &mut self,
        origin: Vec3,
        width: f32,
        rise: f32,
        run: f32,
        thickness: f32,
        material: u16,
    ) -> Result<(), BuildError> {
        self.validate(origin, &[width, rise, run, thickness], 1)?;
        self.pieces.push(BoxPiece {
            center: origin + Vec3::new(0., rise * 0.5, run * 0.5),
            half_extents: Vec3::new(width * 0.5, thickness * 0.5, rise.hypot(run) * 0.5),
            rotation: Quat::from_rotation_x(-rise.atan2(run)),
            material,
        });
        Ok(())
    }
    /// Straight deck, rail beams and evenly spaced support posts. All parts are
    /// actual solids, including rails; no invisible collision approximation.
    pub fn bridge(
        &mut self,
        origin: Vec3,
        width: f32,
        length: f32,
        height: f32,
        segments: u16,
        material: u16,
    ) -> Result<(), BuildError> {
        if segments == 0 || segments > 128 {
            return Err(BuildError::Invalid);
        }
        let count = 3 + 2 * (usize::from(segments) + 1);
        self.validate(origin, &[width, length, height], count)?;
        self.pieces.push(BoxPiece {
            center: origin + Vec3::new(0., height, 0.),
            half_extents: Vec3::new(width * 0.5, 0.12, length * 0.5),
            rotation: Quat::IDENTITY,
            material,
        });
        for side in [-1., 1.] {
            let x = side * width * 0.5;
            self.pieces.push(BoxPiece {
                center: origin + Vec3::new(x, height + 0.9, 0.),
                half_extents: Vec3::new(0.06, 0.06, length * 0.5),
                rotation: Quat::IDENTITY,
                material,
            });
            for i in 0..=segments {
                self.pieces.push(BoxPiece {
                    center: origin
                        + Vec3::new(
                            x,
                            (height + 1.) * 0.5,
                            (f32::from(i) / f32::from(segments) - 0.5) * length,
                        ),
                    half_extents: Vec3::new(0.07, (height + 1.) * 0.5, 0.07),
                    rotation: Quat::IDENTITY,
                    material,
                });
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stairs_and_ramp_have_matching_authored_endpoints() {
        let mut p = EnvironmentPlan::new(16).unwrap();
        p.stairs(Vec3::ZERO, 2., 0.2, 0.5, 6, 0).unwrap();
        assert!((p.pieces()[5].bounds().1.y - 1.2).abs() < 1e-6);
        p.ramp(Vec3::ZERO, 2., 1.2, 3., 0.2, 1).unwrap();
        let r = p.pieces()[6];
        let axis = r.rotation * Vec3::Z * r.half_extents.z;
        assert!((r.center - axis).length() < 1e-5);
        assert!((r.center + axis - Vec3::new(0., 1.2, 3.)).length() < 1e-5);
    }
    #[test]
    fn rejected_builds_are_transactional_and_plans_repeat() {
        let mut p = EnvironmentPlan::new(5).unwrap();
        p.stairs(Vec3::ZERO, 2., 0.2, 0.5, 3, 0).unwrap();
        let old = p.pieces().to_vec();
        assert_eq!(
            p.bridge(Vec3::ZERO, 2., 3., 1., 2, 0),
            Err(BuildError::Capacity)
        );
        assert_eq!(p.pieces(), old);
        assert_eq!(
            p.ramp(Vec3::ZERO, 2., f32::NAN, 3., 0.2, 0),
            Err(BuildError::Invalid)
        );
    }
}
