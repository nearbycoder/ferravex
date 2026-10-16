//! Analytic limb IK and presentation-only foot contacts. Hosts supply actual
//! support queries and apply rotations after their authored animation graph.
use super::AnimationError;
use ferravex_math::Vec3;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LimbPose {
    pub root: Vec3,
    pub joint: Vec3,
    pub end: Vec3,
    pub clamped: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct TwoBone {
    pub upper: f32,
    pub lower: f32,
}
impl TwoBone {
    pub fn solve(self, root: Vec3, target: Vec3, pole: Vec3) -> Result<LimbPose, AnimationError> {
        if ![self.upper, self.lower]
            .iter()
            .all(|v| v.is_finite() && (0.001..=1000.).contains(v))
            || ![root, target, pole]
                .iter()
                .all(|v| v.is_finite() && v.abs().max_element() <= 1e6)
        {
            return Err(AnimationError::InvalidPose);
        }
        let delta = target - root;
        let length = delta.length();
        let axis = delta.try_normalize().unwrap_or(-Vec3::Y);
        let epsilon = self.upper.min(self.lower) * 0.0001;
        let reach = length.clamp(
            (self.upper - self.lower).abs() + epsilon,
            self.upper + self.lower - epsilon,
        );
        let along =
            (self.upper * self.upper - self.lower * self.lower + reach * reach) / (2. * reach);
        let side = (pole - root - axis * (pole - root).dot(axis))
            .try_normalize()
            .unwrap_or_else(|| axis.any_orthonormal_vector());
        Ok(LimbPose {
            root,
            joint: root
                + axis * along
                + side * (self.upper * self.upper - along * along).max(0.).sqrt(),
            end: root + axis * reach,
            clamped: (reach - length).abs() > epsilon,
        })
    }
}
#[derive(Clone, Copy, Debug)]
pub struct FootContact {
    pub position: Vec3,
    pub normal: Vec3,
    pub support: u64,
    pub support_delta: Vec3,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FootPose {
    pub position: Vec3,
    pub normal: Vec3,
    pub weight: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct FootPlant {
    support: Option<u64>,
    pose: FootPose,
    max_correction: f32,
    half_life: f32,
}
impl FootPlant {
    pub fn new(max_correction: f32, half_life: f32) -> Result<Self, AnimationError> {
        if !max_correction.is_finite()
            || !(0.001..=10.).contains(&max_correction)
            || !half_life.is_finite()
            || !(0.001..=1.).contains(&half_life)
        {
            return Err(AnimationError::InvalidPose);
        }
        Ok(Self {
            support: None,
            pose: FootPose {
                position: Vec3::ZERO,
                normal: Vec3::Y,
                weight: 0.,
            },
            max_correction,
            half_life,
        })
    }
    pub fn clear(&mut self) {
        self.support = None;
        self.pose.weight = 0.;
    }
    pub fn update(
        &mut self,
        animated: Vec3,
        contact: Option<FootContact>,
        plant: bool,
        dt: f32,
    ) -> Result<FootPose, AnimationError> {
        if !animated.is_finite()
            || !dt.is_finite()
            || !(0. ..=0.25).contains(&dt)
            || contact.is_some_and(|c| {
                !c.position.is_finite()
                    || !c.normal.is_finite()
                    || (c.normal.length_squared() - 1.).abs() > 0.001
                    || !c.support_delta.is_finite()
            })
        {
            return Err(AnimationError::InvalidPose);
        }
        let alpha = 1. - (-std::f32::consts::LN_2 * dt / self.half_life).exp();
        let usable = contact.filter(|c| {
            plant
                && c.normal.dot(Vec3::Y) > 0.5
                && c.position.distance(animated) <= self.max_correction
        });
        if let Some(c) = usable {
            let desired = if self.support == Some(c.support) && self.pose.weight > 0.01 {
                let p = self.pose.position + c.support_delta;
                if p.distance(animated) <= self.max_correction {
                    p
                } else {
                    c.position
                }
            } else {
                c.position
            };
            if self.support != Some(c.support) {
                self.pose.position = desired;
            }
            self.pose.position = self.pose.position.lerp(desired, alpha);
            self.pose.normal = self.pose.normal.lerp(c.normal, alpha).normalize_or_zero();
            self.pose.weight += (1. - self.pose.weight) * alpha;
            self.support = Some(c.support);
        } else {
            self.support = None;
            self.pose.weight *= 1. - alpha;
            self.pose.position = animated;
            self.pose.normal = Vec3::Y;
        }
        Ok(self.pose)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ik_preserves_lengths_and_handles_unreachable_and_collinear_poles() {
        for target in [Vec3::ZERO, Vec3::X, Vec3::X * 5.] {
            let p = TwoBone {
                upper: 0.6,
                lower: 0.4,
            }
            .solve(Vec3::ZERO, target, Vec3::X)
            .unwrap();
            assert!((p.joint.distance(p.root) - 0.6).abs() < 0.0001);
            assert!((p.end.distance(p.joint) - 0.4).abs() < 0.0001);
            assert!(p.joint.is_finite());
        }
    }
    #[test]
    fn contact_moves_with_support_and_releases_without_changing_body() {
        let mut p = FootPlant::new(0.5, 0.01).unwrap();
        let c = FootContact {
            position: Vec3::Y * 0.1,
            normal: Vec3::Y,
            support: 2,
            support_delta: Vec3::ZERO,
        };
        let a = p.update(Vec3::ZERO, Some(c), true, 0.1).unwrap();
        assert!(a.weight > 0.9);
        let b = p
            .update(
                Vec3::ZERO,
                Some(FootContact {
                    support_delta: Vec3::X * 0.1,
                    ..c
                }),
                true,
                0.1,
            )
            .unwrap();
        assert!(b.position.x > 0.09);
        let gone = p.update(Vec3::ZERO, None, false, 0.1).unwrap();
        assert!(gone.weight < 0.01);
    }
}
