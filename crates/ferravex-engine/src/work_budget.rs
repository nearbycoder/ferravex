//! Deterministic shared work quotas for AI, streaming and incremental rebuilds.
//! Units are host-defined work items, not inferred CPU time. Requests never partially
//! consume unless the caller explicitly uses grant_up_to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkBudget {
    limit: u64,
    remaining: u64,
}
impl WorkBudget {
    pub fn new(limit: u64) -> Self {
        Self {
            limit,
            remaining: limit,
        }
    }
    pub fn try_consume(&mut self, units: u64) -> bool {
        if units > self.remaining {
            return false;
        }
        self.remaining -= units;
        true
    }
    pub fn grant_up_to(&mut self, requested: u64) -> u64 {
        let grant = requested.min(self.remaining);
        self.remaining -= grant;
        grant
    }
    pub fn remaining(&self) -> u64 {
        self.remaining
    }
    pub fn spent(&self) -> u64 {
        self.limit - self.remaining
    }
    pub fn reset(&mut self) {
        self.remaining = self.limit;
    }
    pub fn set_limit(&mut self, limit: u64) {
        self.limit = limit;
        self.remaining = limit;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn systems_share_quota_and_failed_reservations_are_atomic() {
        let mut b = WorkBudget::new(100);
        assert!(b.try_consume(60));
        assert!(!b.try_consume(50));
        assert_eq!(b.remaining(), 40);
        assert_eq!(b.grant_up_to(90), 40);
        assert_eq!(b.spent(), 100);
        assert!(b.try_consume(0));
        assert!(!b.try_consume(1));
        b.reset();
        assert_eq!(b.remaining(), 100);
        b.set_limit(u64::MAX);
        assert!(b.try_consume(u64::MAX));
        assert_eq!(b.spent(), u64::MAX);
    }
}
