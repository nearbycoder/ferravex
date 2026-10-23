//! Authored wave sequencing composed with the shared encounter director.
//!
//! [`WaveSequence::preview`] reads the current wave, its successor, remaining
//! spawn budget and simulation countdowns without advancing the schedule.
//! [`WaveSequence::reset`] restarts the same authored sequence.
use super::{LoopError, seconds};
use crate::encounter::{AdmissionPreview, EncounterConfig, EncounterDirector};
#[derive(Clone, Copy, Debug, PartialEq)]
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

/// Current scheduling phase, as reported by [`WaveSequence::preview`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WaveStatus {
    /// Credits remain. The director explains admission for the requested cost
    /// and alive count, including any cooldown or pressure-recovery countdown.
    Active(AdmissionPreview),
    /// All credits were spent. The next [`WaveSequence::advance`] with no live
    /// actors emits the clear event; observing zero actors does not emit it.
    AwaitingClear {
        /// Host-reported alive count supplied to the preview.
        alive: usize,
    },
    /// The current wave has cleared; its successor starts on an advance after
    /// this rest elapses. Even a zero rest requires a subsequent advance.
    Resting {
        /// Remaining simulation seconds, retaining the scheduler's precision.
        /// The existing `1e-7` completion tolerance applies on advance only.
        remaining: f64,
    },
    /// The last wave has cleared; no further spawns or rewards are scheduled.
    Finished,
}

/// A read-only schedule snapshot for one nonzero spawn cost and alive count.
///
/// This does not reserve a spawn or predict actor deaths or future pressure.
/// See [`WaveSequence::preview`] for a complete example.
#[derive(Clone, Copy, Debug, PartialEq)]
#[must_use]
pub struct WavePreview {
    /// Current zero-based wave index, retained during rest and after finishing.
    pub index: usize,
    /// Authored specification at `index`, including its reward and rest.
    pub spec: WaveSpec,
    /// Authored successor at `index + 1`, or `None` on the final wave. Its start
    /// time is unknown while the current wave is active or awaiting clear.
    pub upcoming: Option<WaveSpec>,
    /// Unspent credits in the current wave; zero during rest and after finish.
    pub remaining_credits: u32,
    /// Current credits divided by the requested nonzero spawn cost, rounded
    /// down. This is budget capacity, not a count of already scheduled actors;
    /// timing and alive limits may block admission. Other spawn costs can
    /// consume a remainder, and future-wave credits are not included.
    pub remaining_spawns: u32,
    /// Scheduling phase and applicable countdown or admission blocker.
    pub status: WaveStatus,
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

    /// Inspect upcoming authoring, spawn budget and countdowns without changing
    /// timers, pressure hysteresis, credits or clear-event delivery.
    ///
    /// `cost == 0` returns [`LoopError::Invalid`] in every phase. For other
    /// costs, an active wave delegates admission checks to
    /// [`EncounterDirector::preview`]. Exhausted credits await a clear event,
    /// even if `alive == 0`; rest and finished phases cannot admit spawns.
    ///
    /// Countdown values describe simulation time at this instant, not wall
    /// time or a guarantee of later admission. Continue to call [`Self::advance`]
    /// on authoritative fixed ticks (at most `0.25` seconds per call), including
    /// while displaying a local preview menu. A preview never consumes a tick.
    ///
    /// ```
    /// use ferravex_play::encounter::AdmissionPreview;
    /// use ferravex_play::gameplay::waves::{WaveSequence, WaveSpec, WaveStatus};
    /// let spec = WaveSpec { credits: 2, interval: 0.125, rest: 0.25, reward: 5 };
    /// let mut waves = WaveSequence::new([spec; 2], 2).unwrap();
    /// let preview = waves.preview(1, 0).unwrap();
    /// assert_eq!(preview.index, 0);
    /// assert_eq!(preview.spec, spec);
    /// assert_eq!(preview.upcoming, Some(spec));
    /// assert_eq!(preview.remaining_credits, 2);
    /// assert_eq!(preview.remaining_spawns, 2);
    /// assert_eq!(preview.status, WaveStatus::Active(AdmissionPreview::Ready));
    /// assert!(waves.admit(2, 0)); // Only accepted admissions spend credits.
    /// assert_eq!(waves.preview(1, 1).unwrap().status,
    ///     WaveStatus::AwaitingClear { alive: 1 });
    /// assert_eq!(waves.advance(0.125, 0.0, 0).unwrap().unwrap().reward, 5);
    /// assert_eq!(waves.preview(1, 0).unwrap().status,
    ///     WaveStatus::Resting { remaining: 0.25 });
    /// waves.advance(0.25, 0.0, 0).unwrap();
    /// assert_eq!(waves.preview(1, 0).unwrap().upcoming, None);
    /// ```
    pub fn preview(&self, cost: u32, alive: usize) -> Result<WavePreview, LoopError> {
        if cost == 0 {
            return Err(LoopError::Invalid);
        }
        let remaining_credits = self.director.credits();
        let status = if self.finished {
            WaveStatus::Finished
        } else if let Some(remaining) = self.rest {
            WaveStatus::Resting { remaining }
        } else if remaining_credits == 0 {
            WaveStatus::AwaitingClear { alive }
        } else {
            WaveStatus::Active(self.director.preview(cost, alive))
        };
        Ok(WavePreview {
            index: self.index,
            spec: self.specs[self.index],
            upcoming: self.specs.get(self.index + 1).copied(),
            remaining_credits,
            remaining_spawns: remaining_credits / cost,
            status,
        })
    }

    /// Restart at wave zero with the same validated specifications and alive
    /// cap. Restores the credit budget and clears cooldown, pressure recovery
    /// and its hysteresis, rest and completion state. Does not emit a clear
    /// event or reward.
    ///
    /// The host must reset actors, earned rewards and other run state separately.
    /// Subsequent clears can award rewards again for the new run. Reconstructs
    /// the first director before mutation, propagating any validation error
    /// without changing the sequence. No allocation or catch-up is performed.
    ///
    /// ```
    /// use ferravex_play::encounter::AdmissionPreview;
    /// use ferravex_play::gameplay::waves::{WaveSequence, WaveSpec, WaveStatus};
    /// let spec = WaveSpec { credits: 1, interval: 0.125, rest: 0.0, reward: 5 };
    /// let mut waves = WaveSequence::new([spec], 1).unwrap();
    /// assert!(waves.admit(1, 0));
    /// assert!(waves.advance(0.0, 0.0, 0).unwrap().is_some());
    /// assert!(waves.finished());
    /// waves.reset().unwrap();
    /// assert!(!waves.finished());
    /// assert_eq!(waves.preview(1, 0).unwrap().status,
    ///     WaveStatus::Active(AdmissionPreview::Ready));
    /// assert_eq!(waves.advance(0.0, 0.0, 0).unwrap(), None);
    /// assert!(waves.admit(1, 0));
    /// ```
    pub fn reset(&mut self) -> Result<(), LoopError> {
        let initial = director(self.specs[0], self.cap)?;
        self.director = initial;
        self.index = 0;
        self.rest = None;
        self.finished = false;
        Ok(())
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

    fn sequence() -> WaveSequence<2> {
        WaveSequence::new(
            [
                WaveSpec {
                    credits: 5,
                    interval: 0.125,
                    rest: 0.25,
                    reward: 7,
                },
                WaveSpec {
                    credits: 2,
                    interval: 0.25,
                    rest: 0.5,
                    reward: 11,
                },
            ],
            2,
        )
        .unwrap()
    }

    #[test]
    fn preview_tracks_budget_and_director_gates_without_spending() {
        let mut waves = sequence();
        let initial = waves.preview(2, 0).unwrap();
        assert_eq!(initial.remaining_credits, 5);
        assert_eq!(initial.remaining_spawns, 2);
        assert_eq!(initial.spec, waves.specs[0]);
        assert_eq!(initial.upcoming, Some(waves.specs[1]));
        assert_eq!(initial.status, WaveStatus::Active(AdmissionPreview::Ready));
        assert_eq!(waves.preview(0, 0), Err(LoopError::Invalid));
        assert_eq!(
            waves.preview(1, usize::MAX).unwrap().status,
            WaveStatus::Active(AdmissionPreview::AliveLimit {
                alive: usize::MAX,
                limit: 2,
            })
        );
        let too_expensive = waves.preview(u32::MAX, 0).unwrap();
        assert_eq!(too_expensive.remaining_spawns, 0);
        assert_eq!(
            too_expensive.status,
            WaveStatus::Active(AdmissionPreview::InsufficientCredits {
                cost: u32::MAX,
                credits: 5,
            })
        );
        assert_eq!(waves.preview(2, 0).unwrap(), initial);
        assert!(waves.admit(2, 0));
        for cooldown in [0.125, 0.0625] {
            let preview = waves.preview(2, 0).unwrap();
            assert_eq!(preview.remaining_credits, 3);
            assert_eq!(preview.remaining_spawns, 1);
            assert_eq!(
                preview.status,
                WaveStatus::Active(AdmissionPreview::Pacing {
                    cooldown,
                    recovery: 0.,
                })
            );
            assert!(!waves.admit(2, 0));
            waves.advance(0.0625, 0., 0).unwrap();
        }
        waves.advance(0., 1., 0).unwrap();
        for tick in 0..6 {
            assert_eq!(
                waves.preview(2, 0).unwrap().status,
                WaveStatus::Active(AdmissionPreview::Pacing {
                    cooldown: 0.,
                    recovery: 1.5 - f64::from(tick) * 0.25,
                })
            );
            assert!(!waves.admit(2, 0));
            waves.advance(0.25, 1., 0).unwrap();
        }
        assert!(waves.admit(2, 0));
        let remainder = waves.preview(2, 0).unwrap();
        assert_eq!(remainder.remaining_credits, 1);
        assert_eq!(remainder.remaining_spawns, 0);
        assert_eq!(
            remainder.status,
            WaveStatus::Active(AdmissionPreview::InsufficientCredits {
                cost: 2,
                credits: 1,
            })
        );
        waves.advance(0.125, 1., 0).unwrap();
        assert!(waves.admit(1, 0));
        assert_eq!(
            waves.preview(1, usize::MAX).unwrap().status,
            WaveStatus::AwaitingClear { alive: usize::MAX }
        );
    }

    #[test]
    fn preview_tracks_clear_rest_and_finish_without_delivering_events() {
        let mut waves = sequence();
        assert!(waves.admit(5, 0));
        assert_eq!(waves.advance(0.125, 0., 1).unwrap(), None);
        assert_eq!(
            waves.preview(1, 1).unwrap().status,
            WaveStatus::AwaitingClear { alive: 1 }
        );
        assert_eq!(
            waves.preview(1, 0).unwrap().status,
            WaveStatus::AwaitingClear { alive: 0 }
        );
        assert!(!waves.resting());
        assert_eq!(
            waves.advance(0.25, 0., 0).unwrap(),
            Some(WaveCleared {
                index: 0,
                reward: 7
            })
        );
        let rest = waves.preview(1, 0).unwrap();
        assert_eq!(rest.index, 0);
        assert_eq!(rest.upcoming, Some(waves.specs[1]));
        assert_eq!(rest.remaining_credits, 0);
        assert_eq!(rest.remaining_spawns, 0);
        assert_eq!(rest.status, WaveStatus::Resting { remaining: 0.25 });
        assert_eq!(waves.advance(0.125, 0., 0).unwrap(), None);
        assert_eq!(
            waves.preview(1, 0).unwrap().status,
            WaveStatus::Resting { remaining: 0.125 }
        );
        assert!(!waves.admit(1, 0));
        // Overshoot starts only the successor; it does not apply pressure or
        // advance its director using the unused part of this tick.
        assert_eq!(waves.advance(0.25, 1., 0).unwrap(), None);
        let next = waves.preview(1, 0).unwrap();
        assert_eq!(next.index, 1);
        assert_eq!(next.spec, waves.specs[1]);
        assert_eq!(next.upcoming, None);
        assert_eq!(next.remaining_spawns, 2);
        assert_eq!(next.status, WaveStatus::Active(AdmissionPreview::Ready));
        assert!(waves.admit(2, 0));
        assert_eq!(
            waves.advance(0., 0., 0).unwrap(),
            Some(WaveCleared {
                index: 1,
                reward: 11,
            })
        );
        let finished = waves.preview(1, 0).unwrap();
        assert_eq!(finished.index, 1);
        assert_eq!(finished.upcoming, None);
        assert_eq!(finished.remaining_credits, 0);
        assert_eq!(finished.remaining_spawns, 0);
        assert_eq!(finished.status, WaveStatus::Finished);
        assert_eq!(waves.advance(0.25, 0., 0).unwrap(), None);
        assert!(!waves.admit(1, 0));
        assert_eq!(waves.preview(1, 0).unwrap(), finished);
    }

    #[test]
    fn zero_rest_requires_an_advance_and_never_skips_an_authored_wave() {
        let mut waves = WaveSequence::new(
            [WaveSpec {
                credits: 1,
                interval: 0.125,
                rest: 0.,
                reward: 1,
            }; 3],
            1,
        )
        .unwrap();
        for index in 0..3 {
            assert_eq!(waves.preview(1, 0).unwrap().index, index);
            assert!(waves.admit(1, 0));
            assert_eq!(
                waves.advance(0.25, 0., 0).unwrap(),
                Some(WaveCleared { index, reward: 1 })
            );
            let expected = if index == 2 {
                WaveStatus::Finished
            } else {
                WaveStatus::Resting { remaining: 0. }
            };
            for _ in 0..3 {
                assert_eq!(waves.preview(1, 0).unwrap().status, expected);
                assert!(!waves.admit(1, 0));
            }
            assert_eq!(waves.advance(0.25, 0., 0).unwrap(), None);
        }
        assert!(waves.finished());
    }

    #[test]
    fn rest_preview_retains_small_countdown_until_advance_applies_tolerance() {
        let mut specs = sequence().specs;
        specs[0].rest = 5e-8;
        let mut waves = WaveSequence::new(specs, 2).unwrap();
        assert!(waves.admit(5, 0));
        assert!(waves.advance(0., 0., 0).unwrap().is_some());
        assert_eq!(
            waves.preview(1, 0).unwrap().status,
            WaveStatus::Resting {
                remaining: f64::from(specs[0].rest),
            }
        );
        assert_eq!(waves.index(), 0);
        assert_eq!(waves.advance(0., 0., 0).unwrap(), None);
        assert_eq!(waves.index(), 1);
        assert!(waves.admit(1, 0));
    }

    fn phases() -> [WaveSequence<2>; 5] {
        let mut active = sequence();
        assert!(active.admit(2, 0));
        active.advance(0., 1., 1).unwrap();
        let mut clearing = sequence();
        assert!(clearing.admit(5, 0));
        let mut resting = clearing;
        assert!(resting.advance(0., 0., 0).unwrap().is_some());
        let mut finished = resting;
        finished.advance(0.25, 0., 0).unwrap();
        assert!(finished.admit(2, 0));
        assert!(finished.advance(0., 0., 0).unwrap().is_some());
        [sequence(), active, clearing, resting, finished]
    }

    #[test]
    fn reset_from_every_phase_reproduces_fresh_admissions_and_clear_events() {
        for mut waves in phases() {
            waves.reset().unwrap();
            let mut fresh = sequence();
            assert_eq!(waves.preview(1, 0), fresh.preview(1, 0));
            assert_eq!(waves.advance(0., 0., 0).unwrap(), None);
            assert!(!waves.finished());
            assert!(!waves.resting());
            assert_eq!(waves.index(), 0);
            assert!(!waves.admit(1, 2)); // The authored alive cap is retained.
            // Reset also re-arms pressure recovery, even if previously spent.
            waves.advance(0., 1., 0).unwrap();
            fresh.advance(0., 1., 0).unwrap();
            assert_eq!(
                waves.preview(1, 0).unwrap().status,
                WaveStatus::Active(AdmissionPreview::Pacing {
                    cooldown: 0.,
                    recovery: 1.5,
                })
            );
            let mut admissions = 0;
            let mut rewards = Vec::new();
            for _ in 0..64 {
                let accepted = waves.admit(1, 0);
                assert_eq!(accepted, fresh.admit(1, 0));
                admissions += u32::from(accepted);
                let event = waves.advance(0.125, 0., 0).unwrap();
                assert_eq!(event, fresh.advance(0.125, 0., 0).unwrap());
                if let Some(event) = event {
                    rewards.push(event);
                }
                assert_eq!(waves.preview(1, 0), fresh.preview(1, 0));
            }
            assert_eq!(admissions, 7);
            assert_eq!(
                rewards,
                [
                    WaveCleared {
                        index: 0,
                        reward: 7
                    },
                    WaveCleared {
                        index: 1,
                        reward: 11,
                    },
                ]
            );
            assert!(waves.finished());
            waves.reset().unwrap();
            waves.reset().unwrap();
            assert_eq!(waves.preview(1, 0), sequence().preview(1, 0));
        }
    }

    #[test]
    fn invalid_input_preserves_each_phase_and_its_countdown() {
        for mut waves in phases() {
            let before = waves.preview(1, 0).unwrap();
            assert_eq!(waves.preview(0, usize::MAX), Err(LoopError::Invalid));
            for (dt, pressure) in [
                (f32::NAN, 0.),
                (f32::INFINITY, 0.),
                (-0.1, 0.),
                (0.3, 0.),
                (0.25, f32::NAN),
                (0.25, f32::INFINITY),
                (0.25, -0.1),
                (0.25, 1.1),
            ] {
                assert_eq!(waves.advance(dt, pressure, 0), Err(LoopError::Invalid));
                assert_eq!(waves.preview(1, 0).unwrap(), before);
            }
        }
    }

    #[test]
    fn repeated_previews_preserve_fixed_tick_history_and_bounded_admission() {
        let mut observed = sequence();
        let mut baseline = observed;
        let mut admissions = 0;
        let mut clears = 0;
        for tick in 0..128 {
            let pressure = if tick % 32 < 16 { 1. } else { 0. };
            let alive = usize::from(tick % 3 == 0) * 2;
            let event = observed.advance(0.125, pressure, alive).unwrap();
            assert_eq!(event, baseline.advance(0.125, pressure, alive).unwrap());
            clears += usize::from(event.is_some());
            for (cost, count) in [(0, 0), (u32::MAX, 0), (1, usize::MAX), (2, 1)] {
                let _ = observed.preview(cost, count);
            }
            let ready = matches!(
                observed.preview(1, alive).unwrap().status,
                WaveStatus::Active(AdmissionPreview::Ready)
            );
            let accepted = observed.admit(1, alive);
            assert_eq!(ready, accepted);
            assert_eq!(accepted, baseline.admit(1, alive));
            admissions += u32::from(accepted);
            // Neither repeated previews nor accumulated elapsed intervals buy
            // extra admissions on the same tick.
            assert!(!observed.admit(1, alive));
            assert_eq!(observed.preview(1, 0), baseline.preview(1, 0));
        }
        assert_eq!(admissions, 7);
        assert_eq!(clears, 2);
        assert!(observed.finished());
    }

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
