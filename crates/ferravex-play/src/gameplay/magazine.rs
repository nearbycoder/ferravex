//! Ammunition and reload timing. A shot spends one round only on acceptance.
use super::{LoopError, seconds};
#[derive(Clone, Copy, Debug)]
pub struct Magazine {
    capacity: u16,
    loaded: u16,
    reserve: u32,
    duration: f64,
    remaining: f64,
}
impl Magazine {
    pub fn new(capacity: u16, reserve: u32, reload_seconds: f32) -> Result<Self, LoopError> {
        let duration = seconds(reload_seconds)?;
        if capacity == 0 || capacity > 4096 || reserve > 1_000_000 || duration == 0. {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            capacity,
            loaded: capacity,
            reserve,
            duration,
            remaining: 0.,
        })
    }
    pub fn loaded(&self) -> u16 {
        self.loaded
    }
    pub fn reserve(&self) -> u32 {
        self.reserve
    }
    pub fn reloading(&self) -> bool {
        self.remaining > 0.
    }
    pub fn remaining(&self) -> f32 {
        self.remaining as f32
    }
    pub fn fire(&mut self) -> bool {
        if self.reloading() || self.loaded == 0 {
            return false;
        }
        self.loaded -= 1;
        true
    }
    pub fn reload(&mut self) -> bool {
        if self.reloading() || self.loaded == self.capacity || self.reserve == 0 {
            return false;
        }
        self.remaining = self.duration;
        true
    }
    pub fn cancel_reload(&mut self) {
        self.remaining = 0.;
    }
    /// Returns true exactly once when ammunition transfers at reload completion.
    pub fn advance(&mut self, dt: f32) -> Result<bool, LoopError> {
        let dt = seconds(dt)?;
        if !self.reloading() {
            return Ok(false);
        }
        self.remaining = (self.remaining - dt).max(0.);
        if self.remaining > 1e-7 {
            return Ok(false);
        }
        self.remaining = 0.;
        let amount = u32::from(self.capacity - self.loaded).min(self.reserve) as u16;
        self.loaded += amount;
        self.reserve -= u32::from(amount);
        Ok(true)
    }
    pub fn add_reserve(&mut self, amount: u32) -> Result<(), LoopError> {
        self.reserve = self
            .reserve
            .checked_add(amount)
            .filter(|n| *n <= 1_000_000)
            .ok_or(LoopError::Capacity)?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reloading_transfers_once_and_cancellation_conserves_ammo() {
        let mut m = Magazine::new(3, 5, 1.).unwrap();
        for _ in 0..3 {
            assert!(m.fire());
        }
        assert!(!m.fire());
        assert!(m.reload());
        assert!(!m.fire());
        m.advance(0.5).unwrap();
        m.cancel_reload();
        assert_eq!((m.loaded(), m.reserve()), (0, 5));
        m.reload();
        for _ in 0..9 {
            assert!(!m.advance(0.1).unwrap());
        }
        assert!(m.advance(0.1).unwrap());
        assert_eq!((m.loaded(), m.reserve()), (3, 2));
        assert!(!m.advance(2.).unwrap());
        assert!(m.add_reserve(u32::MAX).is_err());
        assert_eq!(m.reserve(), 2);
    }
}
