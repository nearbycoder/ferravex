//! Authored wave sequencing composed with the shared encounter director.
use super::{LoopError, seconds};
use crate::encounter::{EncounterConfig, EncounterDirector};
#[derive(Clone, Copy, Debug)]
pub struct WaveSpec {
    pub credits: u32,
    pub interval: f32,
    pub rest: f32,
    pub reward: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaveCleared {
    pub index: usize,
    pub reward: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct WaveSequence<const N: usize> {
    specs: [WaveSpec; N],
    index: usize,
    director: EncounterDirector,
    cap: u16,
    rest: Option<f64>,
    finished: bool,
}
fn director(spec: WaveSpec, cap: u16) -> Result<EncounterDirector, LoopError> {
    EncounterDirector::new(EncounterConfig {
        credits: spec.credits,
        max_alive: cap,
        interval: spec.interval,
        first_delay: 0.,
        recovery_seconds: 1.5,
    })
    .map_err(|_| LoopError::Invalid)
}
impl<const N: usize> WaveSequence<N> {
    pub fn new(specs: [WaveSpec; N], max_alive: u16) -> Result<Self, LoopError> {
        if N == 0 || N > 128 {
            return Err(LoopError::Invalid);
        }
        for s in specs {
            if s.credits == 0 {
                return Err(LoopError::Invalid);
            }
            seconds(s.rest)?;
            director(s, max_alive)?;
        }
        Ok(Self {
            specs,
            index: 0,
            director: director(specs[0], max_alive)?,
            cap: max_alive,
            rest: None,
            finished: false,
        })
    }
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn finished(&self) -> bool {
        self.finished
    }
    pub fn resting(&self) -> bool {
        self.rest.is_some()
    }
    pub fn admit(&mut self, cost: u32, alive: usize) -> bool {
        !self.finished && self.rest.is_none() && self.director.admit(cost, alive)
    }
    /// Reports each cleared wave exactly once, after admitted actors are gone.
    /// Rest begins on the following update; a frame never skips an authored wave.
    pub fn advance(
        &mut self,
        dt: f32,
        pressure: f32,
        alive: usize,
    ) -> Result<Option<WaveCleared>, LoopError> {
        let elapsed = seconds(dt)?;
        if dt > 0.25 || !pressure.is_finite() || !(0. ..=1.).contains(&pressure) {
            return Err(LoopError::Invalid);
        }
        if self.finished {
            return Ok(None);
        }
        if let Some(rest) = &mut self.rest {
            *rest = (*rest - elapsed).max(0.);
            if *rest <= 1e-7 {
                self.index += 1;
                self.director = director(self.specs[self.index], self.cap)?;
                self.rest = None;
            }
            return Ok(None);
        }
        self.director
            .advance(dt, pressure)
            .map_err(|_| LoopError::Invalid)?;
        if self.director.credits() == 0 && alive == 0 {
            let event = WaveCleared {
                index: self.index,
                reward: self.specs[self.index].reward,
            };
            if self.index + 1 == N {
                self.finished = true;
            } else {
                self.rest = Some(f64::from(self.specs[self.index].rest));
            }
            Ok(Some(event))
        } else {
            Ok(None)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_actors_must_clear_before_a_single_reward_and_next_wave() {
        let mut w = WaveSequence::new(
            [WaveSpec {
                credits: 1,
                interval: 0.1,
                rest: 0.25,
                reward: 5,
            }; 2],
            2,
        )
        .unwrap();
        assert!(w.admit(1, 0));
        assert_eq!(w.advance(0.1, 0., 1).unwrap(), None);
        assert_eq!(
            w.advance(0.1, 0., 0).unwrap(),
            Some(WaveCleared {
                index: 0,
                reward: 5
            })
        );
        assert!(!w.admit(1, 0));
        assert_eq!(w.advance(0.25, 0., 0).unwrap(), None);
        assert_eq!(w.index(), 1);
        assert!(w.admit(1, 0));
        assert!(w.advance(0., 0., 0).unwrap().is_some());
        assert!(w.finished());
        assert_eq!(w.advance(0.1, 0., 0).unwrap(), None);
        assert!(!w.admit(1, 0));
    }
}
