//! Per-projectile target deduplication, piercing and ricochet budgets.
use super::LoopError;
use ferravex_math::Vec2;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitResult {
    Ignored,
    Continue,
    Stop,
}
#[derive(Clone, Copy, Debug)]
pub struct ProjectileBudget<const N: usize> {
    targets: [Option<u64>; N],
    pierces: u8,
    bounces: u8,
    alive: bool,
}
impl<const N: usize> ProjectileBudget<N> {
    pub fn new(pierces: u8, bounces: u8) -> Result<Self, LoopError> {
        if N == 0 || N > 256 || usize::from(pierces) >= N {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            targets: [None; N],
            pierces,
            bounces,
            alive: true,
        })
    }
    pub fn alive(&self) -> bool {
        self.alive
    }
    /// Supply a unique generational target ID after confirmed collision.
    pub fn target(&mut self, id: u64) -> HitResult {
        if !self.alive || self.targets.contains(&Some(id)) {
            return HitResult::Ignored;
        }
        let Some(slot) = self.targets.iter_mut().find(|s| s.is_none()) else {
            self.alive = false;
            return HitResult::Stop;
        };
        *slot = Some(id);
        if self.pierces == 0 {
            self.alive = false;
            HitResult::Stop
        } else {
            self.pierces -= 1;
            HitResult::Continue
        }
    }
    /// Separating contacts spend no bounce; invalid vectors leave state intact.
    pub fn wall(&mut self, velocity: Vec2, normal: Vec2) -> Result<Option<Vec2>, LoopError> {
        if !velocity.is_finite() || velocity.abs().max_element() > 1e6 || !normal.is_finite() {
            return Err(LoopError::Invalid);
        }
        let normal = normal.try_normalize().ok_or(LoopError::Invalid)?;
        if !self.alive {
            return Ok(None);
        }
        let dot = velocity.dot(normal);
        if dot >= 0. {
            return Ok(Some(velocity));
        }
        if self.bounces == 0 {
            self.alive = false;
            return Ok(None);
        }
        self.bounces -= 1;
        Ok(Some(velocity - normal * (2. * dot)))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_targets_and_separating_contacts_cannot_spend_twice() {
        let mut p = ProjectileBudget::<3>::new(1, 1).unwrap();
        assert_eq!(p.target(8), HitResult::Continue);
        assert_eq!(p.target(8), HitResult::Ignored);
        assert_eq!(p.wall(Vec2::X, Vec2::X).unwrap(), Some(Vec2::X));
        assert!(p.wall(-Vec2::X, Vec2::ZERO).is_err());
        assert_eq!(p.wall(-Vec2::X, Vec2::X).unwrap(), Some(Vec2::X));
        assert_eq!(p.target(9), HitResult::Stop);
        assert!(!p.alive());
        assert_eq!(p.target(10), HitResult::Ignored);
        let mut p = ProjectileBudget::<1>::new(0, 0).unwrap();
        assert_eq!(p.wall(-Vec2::X, Vec2::X).unwrap(), None);
    }
}
