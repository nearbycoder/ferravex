//! Deterministic safe-spawn selection. Hosts supply authoritative occupancy and
//! line-of-sight eligibility before spending encounter credits.
use super::LoopError;
use ferravex_math::Vec2;
#[derive(Clone, Copy, Debug)]
pub struct SpawnSelector {
    minimum: f32,
    cursor: usize,
}
impl SpawnSelector {
    pub fn new(minimum: f32) -> Result<Self, LoopError> {
        if !minimum.is_finite() || !(0. ..=1e6).contains(&minimum) {
            return Err(LoopError::Invalid);
        }
        Ok(Self { minimum, cursor: 0 })
    }
    /// Farthest eligible candidate wins, with rotating stable tie breaks.
    /// At most 256 eligibility queries. None means defer spawning, never fallback
    /// into an unsafe location. Callbacks should be read-only world queries.
    pub fn choose(
        &mut self,
        positions: &[Vec2],
        player: Vec2,
        mut eligible: impl FnMut(usize) -> bool,
    ) -> Result<Option<usize>, LoopError> {
        if positions.is_empty()
            || positions.len() > 256
            || !valid(player)
            || positions.iter().any(|p| !valid(*p))
        {
            return Err(LoopError::Invalid);
        }
        let mut best = None;
        let mut best_distance = -1.;
        for offset in 0..positions.len() {
            let i = (self.cursor + offset) % positions.len();
            let d = positions[i].distance_squared(player);
            if d >= self.minimum * self.minimum && d > best_distance && eligible(i) {
                best = Some(i);
                best_distance = d;
            }
        }
        if let Some(i) = best {
            self.cursor = (i + 1) % positions.len();
        }
        Ok(best)
    }
}
fn valid(p: Vec2) -> bool {
    p.is_finite() && p.abs().max_element() <= 1e6
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsafe_candidates_defer_and_equal_distances_rotate() {
        let mut s = SpawnSelector::new(10.).unwrap();
        let p = [Vec2::X * 20., -Vec2::X * 20., Vec2::X];
        assert_eq!(s.choose(&p, Vec2::ZERO, |_| true).unwrap(), Some(0));
        assert_eq!(s.choose(&p, Vec2::ZERO, |_| true).unwrap(), Some(1));
        assert_eq!(s.choose(&p, Vec2::ZERO, |_| false).unwrap(), None);
        assert_eq!(s.choose(&p, Vec2::ZERO, |i| i == 2).unwrap(), None);
        assert!(s.choose(&[], Vec2::ZERO, |_| panic!()).is_err());
    }
}
