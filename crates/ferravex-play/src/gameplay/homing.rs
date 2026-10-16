//! Bounded 2D turning for rockets, enemies and boomerangs. Does not query targets.
use super::{LoopError, seconds};
use ferravex_math::Vec2;
pub fn steer(
    velocity: Vec2,
    to_target: Vec2,
    radians_per_second: f32,
    dt: f32,
) -> Result<Vec2, LoopError> {
    let dt = seconds(dt)?;
    if !velocity.is_finite()
        || !to_target.is_finite()
        || velocity.abs().max_element() > 1e6
        || to_target.abs().max_element() > 1e6
        || !radians_per_second.is_finite()
        || !(0. ..=100.).contains(&radians_per_second)
    {
        return Err(LoopError::Invalid);
    }
    let speed = velocity.length();
    if speed < 1e-6 || to_target.length_squared() < 1e-12 {
        return Ok(velocity);
    }
    let current = velocity.y.atan2(velocity.x);
    let desired = to_target.y.atan2(to_target.x);
    let delta = (desired - current + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    let turn = (f64::from(radians_per_second) * dt).min(f64::from(std::f32::consts::PI)) as f32;
    let angle = current + delta.clamp(-turn, turn);
    Ok(Vec2::new(angle.cos(), angle.sin()) * speed)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rockets_turn_at_authored_rate_without_changing_speed() {
        let v = steer(Vec2::X * 100., Vec2::Y, 1., 0.1).unwrap();
        assert!((v.length() - 100.).abs() < 1e-4);
        assert!((v.y.atan2(v.x) - 0.1).abs() < 1e-5);
        assert_eq!(steer(Vec2::X, Vec2::ZERO, 1., 1.).unwrap(), Vec2::X);
        assert_eq!(steer(Vec2::ZERO, Vec2::Y, 1., 1.).unwrap(), Vec2::ZERO);
        let v = steer(Vec2::new(-1., 0.01), Vec2::new(-1., -0.01), 1., 0.1).unwrap();
        assert!(v.x < 0. && v.y < 0., "turn across the wrap boundary");
        assert!(steer(Vec2::X, Vec2::Y, -1., 1.).is_err());
    }
}
