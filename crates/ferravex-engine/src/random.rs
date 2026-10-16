//! Reproducible SplitMix64 streams for engine variation, never cryptography.
//! Forks depend on the original seed and stream ID, independent of parent draws.
#[derive(Clone, Debug)]
pub struct Random {
    seed: u64,
    state: u64,
}
fn mix(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
impl Random {
    pub fn new(seed: u64) -> Self {
        Self { seed, state: seed }
    }
    pub fn fork(&self, stream: u64) -> Self {
        Self::new(mix(self.seed ^ mix(stream.wrapping_add(0x9e3779b97f4a7c15))))
    }
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        mix(self.state)
    }
    /// Uniform integer in [0, upper). Rejection avoids modulo bias. Zero returns None.
    pub fn below(&mut self, upper: u64) -> Option<u64> {
        if upper == 0 {
            return None;
        }
        let threshold = upper.wrapping_neg() % upper;
        loop {
            let n = self.next_u64();
            if n >= threshold {
                return Some(n % upper);
            }
        }
    }
    /// Exactly 24 random mantissa bits in [0,1); independent of platform libm.
    pub fn unit_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / 16777216.0
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_vector_and_independent_streams() {
        let mut r = Random::new(0);
        assert_eq!(r.next_u64(), 0xe220a8397b1dcdaf);
        assert_eq!(r.next_u64(), 0x6e789e6aa1b965f4);
        let mut child = r.fork(7);
        for _ in 0..100 {
            r.next_u64();
        }
        assert_eq!(child.next_u64(), r.fork(7).next_u64());
        assert_ne!(r.fork(7).next_u64(), r.fork(8).next_u64());
        assert_eq!(r.below(0), None);
        assert_eq!(r.below(1), Some(0));
        for _ in 0..10000 {
            assert!(r.below(7).unwrap() < 7);
            let f = r.unit_f32();
            assert!((0.0..1.0).contains(&f));
        }
    }
}
