//! Frame-independent scalar presentation smoothing. Uses platform exp, so this is
//! intended for cameras/UI/audio controls, not cross-platform lockstep state.
#[derive(Clone, Copy, Debug)]
pub struct ExponentialSmoother {
    value: f64,
    half_life: f64,
}
impl ExponentialSmoother {
    pub fn new(value: f64, half_life_seconds: f64) -> Option<Self> {
        if !value.is_finite() || !half_life_seconds.is_finite() || half_life_seconds <= 0. {
            return None;
        }
        Some(Self {
            value,
            half_life: half_life_seconds,
        })
    }
    /// Invalid inputs leave the current value unchanged. Large deltas converge to target.
    pub fn update(&mut self, target: f64, seconds: f64) -> f64 {
        if !target.is_finite() || !seconds.is_finite() || seconds <= 0. {
            return self.value;
        }
        let alpha = -(-std::f64::consts::LN_2 * seconds / self.half_life).exp_m1();
        self.value = self.value * (1. - alpha) + target * alpha;
        self.value
    }
    pub fn value(&self) -> f64 {
        self.value
    }
    pub fn snap(&mut self, value: f64) -> bool {
        if !value.is_finite() {
            return false;
        }
        self.value = value;
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn half_life_is_independent_of_frame_partition() {
        for fps in [30, 60, 144] {
            let mut s = ExponentialSmoother::new(0., 1.).unwrap();
            for _ in 0..fps {
                s.update(10., 1. / f64::from(fps));
            }
            assert!((s.value() - 5.).abs() < 1e-12);
        }
        let mut s = ExponentialSmoother::new(-f64::MAX, 1.).unwrap();
        assert!(s.update(f64::MAX, 1.).is_finite());
        let before = s.value();
        assert_eq!(s.update(f64::NAN, 1.), before);
        assert_eq!(s.update(0., f64::INFINITY), before);
        assert!(!s.snap(f64::NAN));
        assert!(s.snap(3.));
        assert_eq!(s.update(10., 1e10), 10.);
        assert!(ExponentialSmoother::new(0., 0.).is_none());
    }
}
