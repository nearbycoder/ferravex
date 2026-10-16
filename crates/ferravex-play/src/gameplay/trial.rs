//! Ordered time-trial splits and authored medal thresholds on simulation time.
use super::{LoopError, seconds};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Medal {
    Gold,
    Silver,
    Bronze,
    Unranked,
}
#[derive(Clone, Copy, Debug)]
pub struct Trial<const N: usize> {
    elapsed: f64,
    splits: [Option<f64>; N],
    next: usize,
    running: bool,
    thresholds: [f64; 3],
}
impl<const N: usize> Trial<N> {
    pub fn new(thresholds: [f32; 3]) -> Result<Self, LoopError> {
        if N == 0 || N > 256 {
            return Err(LoopError::Invalid);
        }
        let thresholds = [
            seconds(thresholds[0])?,
            seconds(thresholds[1])?,
            seconds(thresholds[2])?,
        ];
        if thresholds[0] == 0. || thresholds[0] > thresholds[1] || thresholds[1] > thresholds[2] {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            elapsed: 0.,
            splits: [None; N],
            next: 0,
            running: false,
            thresholds,
        })
    }
    pub fn start(&mut self) {
        self.elapsed = 0.;
        self.splits = [None; N];
        self.next = 0;
        self.running = true;
    }
    pub fn elapsed(&self) -> f64 {
        self.elapsed
    }
    pub fn splits(&self) -> &[Option<f64>; N] {
        &self.splits
    }
    pub fn finished(&self) -> bool {
        self.next == N
    }
    pub fn advance(&mut self, dt: f32) -> Result<(), LoopError> {
        let dt = seconds(dt)?;
        if self.running {
            self.elapsed += dt;
        }
        Ok(())
    }
    pub fn split(&mut self, index: usize) -> Result<bool, LoopError> {
        if index >= N {
            return Err(LoopError::Invalid);
        }
        if index < self.next {
            return Ok(false);
        }
        if !self.running || index != self.next {
            return Err(LoopError::Unavailable);
        }
        self.splits[index] = Some(self.elapsed);
        self.next += 1;
        if self.next == N {
            self.running = false;
        }
        Ok(true)
    }
    pub fn medal(&self) -> Option<Medal> {
        if !self.finished() {
            return None;
        }
        Some(if self.elapsed <= self.thresholds[0] + 1e-6 {
            Medal::Gold
        } else if self.elapsed <= self.thresholds[1] + 1e-6 {
            Medal::Silver
        } else if self.elapsed <= self.thresholds[2] + 1e-6 {
            Medal::Bronze
        } else {
            Medal::Unranked
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordered_splits_freeze_at_finish_and_medals_use_elapsed_time() {
        let mut t = Trial::<2>::new([10., 20., 30.]).unwrap();
        assert!(t.split(0).is_err());
        t.start();
        t.advance(4.).unwrap();
        assert!(t.split(1).is_err());
        t.split(0).unwrap();
        assert!(!t.split(0).unwrap());
        t.advance(6.).unwrap();
        t.split(1).unwrap();
        t.advance(100.).unwrap();
        assert_eq!(t.elapsed(), 10.);
        assert_eq!(t.medal(), Some(Medal::Gold));
        assert_eq!(*t.splits(), [Some(4.), Some(10.)]);
        t.start();
        assert_eq!(t.medal(), None);
        assert_eq!(t.elapsed(), 0.);
        assert!(t.advance(-1.).is_err());
    }
}
