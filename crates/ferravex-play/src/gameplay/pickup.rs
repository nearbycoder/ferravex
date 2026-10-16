//! Pickup attraction toward an authoritative target. Hosts resolve walls before
//! accepting the returned position/collection; attraction grants no collision bypass.
use super::{LoopError, seconds};
use ferravex_math::Vec2;
#[derive(Clone, Copy, Debug)]
pub struct Magnet {
    pub radius: f32,
    pub collect_radius: f32,
    pub speed: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickupStep {
    pub position: Vec2,
    pub collected: bool,
}
impl Magnet {
    pub fn step(self, position: Vec2, target: Vec2, dt: f32) -> Result<PickupStep, LoopError> {
        let dt = seconds(dt)?;
        if !position.is_finite()
            || !target.is_finite()
            || position.abs().max_element() > 1e6
            || target.abs().max_element() > 1e6
            || !self.radius.is_finite()
            || !(0. ..=1e6).contains(&self.radius)
            || !self.collect_radius.is_finite()
            || !(0. ..=self.radius).contains(&self.collect_radius)
            || !self.speed.is_finite()
            || !(0. ..=1e5).contains(&self.speed)
        {
            return Err(LoopError::Invalid);
        }
        let delta = target - position;
        let distance = delta.length();
        let travel = if distance <= self.radius {
            (f64::from(self.speed) * dt).min(f64::from(distance)) as f32
        } else {
            0.
        };
        let next = position + delta.normalize_or_zero() * travel;
        Ok(PickupStep {
            position: next,
            collected: distance - travel <= self.collect_radius,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn attraction_is_bounded_collects_on_sweep_and_ignores_distant_items() {
        let m = Magnet {
            radius: 100.,
            collect_radius: 5.,
            speed: 50.,
        };
        assert_eq!(
            m.step(Vec2::X * 150., Vec2::ZERO, 1.).unwrap().position,
            Vec2::X * 150.
        );
        let p = m.step(Vec2::X * 40., Vec2::ZERO, 1.).unwrap();
        assert_eq!(
            p,
            PickupStep {
                position: Vec2::ZERO,
                collected: true
            }
        );
        let a = m.step(Vec2::X * 80., Vec2::ZERO, 0.5).unwrap();
        let b = m.step(a.position, Vec2::ZERO, 0.5).unwrap();
        assert_eq!(b, m.step(Vec2::X * 80., Vec2::ZERO, 1.).unwrap());
        assert!(m.step(Vec2::NAN, Vec2::ZERO, 1.).is_err());
    }
}
