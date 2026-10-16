//! Capped upgrade ranks and distinct seeded offers. The host defines rank effects.
use super::{LoopError, loot::RewardRng};
#[derive(Clone, Copy, Debug)]
pub struct Upgrades<const N: usize> {
    ranks: [u8; N],
    caps: [u8; N],
}
impl<const N: usize> Upgrades<N> {
    pub fn new(caps: [u8; N]) -> Result<Self, LoopError> {
        if N == 0 || N > 256 || caps.contains(&0) {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            ranks: [0; N],
            caps,
        })
    }
    pub fn rank(&self, id: usize) -> Option<u8> {
        self.ranks.get(id).copied()
    }
    /// Only call after spending the host's earned upgrade point.
    pub fn apply(&mut self, id: usize) -> Result<u8, LoopError> {
        let cap = *self.caps.get(id).ok_or(LoopError::Invalid)?;
        if self.ranks[id] >= cap {
            return Err(LoopError::Unavailable);
        }
        self.ranks[id] += 1;
        Ok(self.ranks[id])
    }
    /// Fixed scratch, no duplicate or maxed offers. Fewer eligible choices leave
    /// trailing None entries. Persist chosen IDs, not RNG state, in run records.
    pub fn offers<const K: usize>(
        &self,
        rng: &mut RewardRng,
    ) -> Result<[Option<usize>; K], LoopError> {
        if K > N {
            return Err(LoopError::Invalid);
        }
        let mut eligible = [0; N];
        let mut count = 0;
        for id in 0..N {
            if self.ranks[id] < self.caps[id] {
                eligible[count] = id;
                count += 1;
            }
        }
        let mut offers = [None; K];
        for slot in &mut offers {
            if count == 0 {
                break;
            }
            let index = (rng.next_u64() % count as u64) as usize;
            *slot = Some(eligible[index]);
            count -= 1;
            eligible[index] = eligible[count];
        }
        Ok(offers)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offers_exclude_maxed_ranks_and_never_duplicate() {
        let mut u = Upgrades::new([1, 2, 1]).unwrap();
        u.apply(0).unwrap();
        let offers = u.offers::<3>(&mut RewardRng::new(7)).unwrap();
        assert_eq!(offers[2], None);
        assert_ne!(offers[0], offers[1]);
        assert!(!offers.contains(&Some(0)));
        assert_eq!(u.apply(0), Err(LoopError::Unavailable));
        u.apply(1).unwrap();
        u.apply(1).unwrap();
        u.apply(2).unwrap();
        assert_eq!(u.offers::<2>(&mut RewardRng::new(1)).unwrap(), [None, None]);
    }
}
