//! Ordered, authored respawn anchors with rules-checked compact snapshots.
use super::LoopError;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckpointSnapshot {
    pub rules: u64,
    pub index: usize,
}
#[derive(Clone, Copy, Debug)]
pub struct Checkpoints<T: Copy, const N: usize> {
    anchors: [T; N],
    index: usize,
    rules: u64,
}
impl<T: Copy, const N: usize> Checkpoints<T, N> {
    /// Hosts validate anchor geometry and supply a stable nonzero rules ID.
    pub fn new(rules: u64, anchors: [T; N]) -> Result<Self, LoopError> {
        if N == 0 || N > 256 || rules == 0 {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            anchors,
            index: 0,
            rules,
        })
    }
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn respawn(&self) -> T {
        self.anchors[self.index]
    }
    /// Old/duplicate checkpoints cannot move the respawn backward. Skipping an
    /// authored checkpoint fails, preventing exit triggers from bypassing a route.
    pub fn activate(&mut self, index: usize) -> Result<bool, LoopError> {
        if index >= N {
            return Err(LoopError::Invalid);
        }
        if index <= self.index {
            return Ok(false);
        }
        if index != self.index + 1 {
            return Err(LoopError::Unavailable);
        }
        self.index = index;
        Ok(true)
    }
    pub fn snapshot(&self) -> CheckpointSnapshot {
        CheckpointSnapshot {
            rules: self.rules,
            index: self.index,
        }
    }
    /// Trusted save restoration only. This changes respawn state, not world state;
    /// the host restores inventory/objectives with its existing save transaction.
    pub fn restore(&mut self, saved: CheckpointSnapshot) -> Result<(), LoopError> {
        if saved.rules != self.rules || saved.index >= N {
            return Err(LoopError::Invalid);
        }
        self.index = saved.index;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkpoints_are_ordered_and_snapshots_validate_before_mutation() {
        let mut c = Checkpoints::new(4, [10, 20, 30]).unwrap();
        assert_eq!(c.respawn(), 10);
        assert_eq!(c.activate(2), Err(LoopError::Unavailable));
        assert!(c.activate(1).unwrap());
        assert!(!c.activate(0).unwrap());
        let saved = c.snapshot();
        assert!(c.activate(2).unwrap());
        c.restore(saved).unwrap();
        assert_eq!(c.respawn(), 20);
        assert!(
            c.restore(CheckpointSnapshot { rules: 5, index: 0 })
                .is_err()
        );
        assert_eq!(c.index(), 1);
        assert!(
            c.restore(CheckpointSnapshot { rules: 4, index: 3 })
                .is_err()
        );
        assert_eq!(c.index(), 1);
    }
}
