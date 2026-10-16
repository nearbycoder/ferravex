//! Checked integer-weight sampling for engine content and procedural variants.
//! At most 4096 entries, zero weights are skipped; total weight must fit u64.
use crate::random::Random;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeightError {
    Empty,
    Capacity,
    ZeroTotal,
    Overflow,
}
#[derive(Debug)]
pub struct WeightedTable {
    cumulative: Vec<u64>,
    total: u64,
}
impl WeightedTable {
    pub fn new(weights: &[u64]) -> Result<Self, WeightError> {
        if weights.is_empty() {
            return Err(WeightError::Empty);
        }
        if weights.len() > 4096 {
            return Err(WeightError::Capacity);
        }
        let mut total = 0_u64;
        let mut cumulative = Vec::with_capacity(weights.len());
        for &weight in weights {
            total = total.checked_add(weight).ok_or(WeightError::Overflow)?;
            cumulative.push(total);
        }
        if total == 0 {
            return Err(WeightError::ZeroTotal);
        }
        Ok(Self { cumulative, total })
    }
    /// Resolve a ticket in [0,total). Useful when draws are supplied by a replay host.
    pub fn ticket(&self, ticket: u64) -> Option<usize> {
        (ticket < self.total).then(|| self.cumulative.partition_point(|&end| end <= ticket))
    }
    pub fn sample(&self, random: &mut Random) -> usize {
        self.ticket(random.below(self.total).unwrap()).unwrap()
    }
    pub fn total(&self) -> u64 {
        self.total
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_weight_boundaries_and_rejected_overflow() {
        let table = WeightedTable::new(&[0, 2, 0, 3, 0]).unwrap();
        assert_eq!(table.total(), 5);
        assert_eq!(
            (0..5).map(|t| table.ticket(t).unwrap()).collect::<Vec<_>>(),
            [1, 1, 3, 3, 3]
        );
        assert_eq!(table.ticket(5), None);
        assert!(matches!(WeightedTable::new(&[]), Err(WeightError::Empty)));
        assert!(matches!(
            WeightedTable::new(&[0]),
            Err(WeightError::ZeroTotal)
        ));
        assert!(matches!(
            WeightedTable::new(&[u64::MAX, 1]),
            Err(WeightError::Overflow)
        ));
        let table = WeightedTable::new(&[u64::MAX]).unwrap();
        assert_eq!(table.sample(&mut Random::new(1)), 0);
    }
}
