//! Fixed-clock spawn admission. Hosts choose safe spawn locations and apply the
//! accepted spawn; the director never reads hidden player positions.
//!
//! [`EncounterDirector::preview`] explains admission without spending credits
//! or advancing time. Credits are a fixed encounter budget: advancing the
//! director drains timers but does not replenish credits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectorError {
    Invalid,
}
#[derive(Clone, Copy, Debug)]
pub struct EncounterConfig {
    pub credits: u32,
    pub max_alive: u16,
    pub interval: f32,
    pub first_delay: f32,
    pub recovery_seconds: f32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pacing {
    Build,
    Recover,
    Exhausted,
}
/// A snapshot of whether a spawn can be admitted, or its first blocking reason.
///
/// Reasons follow the admission checks in order: zero cost, insufficient
/// credits, alive limit, then pacing. Pacing reports both concurrent timers.
/// A preview does not reserve credits or an alive slot; call
/// [`EncounterDirector::admit`] after validating the spawn location.
/// See [`EncounterDirector::preview`] for an example.
#[derive(Clone, Copy, Debug, PartialEq)]
#[must_use]
pub enum AdmissionPreview {
    /// The request can be admitted immediately.
    Ready,
    /// Spawn costs must be nonzero, including when the budget is exhausted.
    ZeroCost,
    /// The fixed encounter budget cannot pay this request.
    InsufficientCredits {
        /// Requested spawn cost.
        cost: u32,
        /// Unspent encounter credits.
        credits: u32,
    },
    /// The host-reported alive count has reached or exceeded the cap.
    AliveLimit {
        /// Alive count supplied to the preview, without narrowing or clamping.
        alive: usize,
        /// Configured maximum alive count.
        limit: u16,
    },
    /// Cost and alive checks pass, but at least one pacing timer is positive.
    Pacing {
        /// Simulation seconds left in the first delay or post-admission interval.
        cooldown: f64,
        /// Simulation seconds left in pressure-triggered recovery.
        recovery: f64,
    },
}
impl AdmissionPreview {
    /// Whether admission would succeed at the time of the preview.
    ///
    /// ```
    /// use ferravex_play::encounter::AdmissionPreview;
    /// assert!(AdmissionPreview::Ready.is_ready());
    /// assert!(!AdmissionPreview::ZeroCost.is_ready());
    /// ```
    pub fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Simulation seconds until the current pacing gates open, if time alone
    /// can make this request admissible. Ready requests return `Some(0.0)`.
    ///
    /// Zero cost, insufficient credits and the alive limit return `None`:
    /// advancing time cannot change the cost, replenish the budget or predict
    /// an actor's departure. Otherwise the wait is the maximum of the two
    /// timers, which drain concurrently. The `f64` result preserves the
    /// director's timer precision.
    ///
    /// This is a snapshot, not a reservation or a wall-clock deadline. Future
    /// admissions, alive-count changes or newly triggered recovery can change
    /// availability. Advance on simulation ticks with the existing `0.25`
    /// second per-call limit, then preview again before admitting.
    ///
    /// ```
    /// use ferravex_play::encounter::AdmissionPreview;
    /// let wait = AdmissionPreview::Pacing { cooldown: 0.5, recovery: 1.0 };
    /// assert_eq!(wait.next_available_in(), Some(1.0));
    /// assert_eq!(AdmissionPreview::Ready.next_available_in(), Some(0.0));
    /// assert_eq!(AdmissionPreview::AliveLimit { alive: 2, limit: 2 }
    ///     .next_available_in(), None);
    /// ```
    pub fn next_available_in(self) -> Option<f64> {
        match self {
            Self::Ready => Some(0.),
            Self::Pacing { cooldown, recovery } => Some(cooldown.max(recovery)),
            Self::ZeroCost | Self::InsufficientCredits { .. } | Self::AliveLimit { .. } => None,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct EncounterDirector {
    config: EncounterConfig,
    credits: u32,
    cooldown: f64,
    recovery: f64,
    armed: bool,
}
impl EncounterDirector {
    pub fn new(config: EncounterConfig) -> Result<Self, DirectorError> {
        if config.credits > 100_000
            || config.max_alive == 0
            || !config.interval.is_finite()
            || !(0.01..=60.).contains(&config.interval)
            || !config.first_delay.is_finite()
            || !(0. ..=60.).contains(&config.first_delay)
            || !config.recovery_seconds.is_finite()
            || !(0. ..=60.).contains(&config.recovery_seconds)
        {
            return Err(DirectorError::Invalid);
        }
        Ok(Self {
            config,
            credits: config.credits,
            cooldown: f64::from(config.first_delay),
            recovery: 0.,
            armed: true,
        })
    }
    pub fn credits(&self) -> u32 {
        self.credits
    }
    pub fn pacing(&self) -> Pacing {
        if self.credits == 0 {
            Pacing::Exhausted
        } else if self.recovery > 0. {
            Pacing::Recover
        } else {
            Pacing::Build
        }
    }
    /// Pressure is a host-authored 0..1 measure of danger. Hysteresis prevents a
    /// constantly injured player from retriggering recovery every frame.
    pub fn advance(&mut self, dt: f32, pressure: f32) -> Result<(), DirectorError> {
        if !dt.is_finite()
            || !(0. ..=0.25).contains(&dt)
            || !pressure.is_finite()
            || !(0. ..=1.).contains(&pressure)
        {
            return Err(DirectorError::Invalid);
        }
        self.cooldown = (self.cooldown - f64::from(dt)).max(0.);
        self.recovery = (self.recovery - f64::from(dt)).max(0.);
        if pressure < 0.5 {
            self.armed = true;
        }
        if pressure >= 0.8 && self.armed {
            self.recovery = f64::from(self.config.recovery_seconds);
            self.armed = false;
        }
        Ok(())
    }
    /// Inspect admission without spending credits, advancing timers or changing
    /// pressure hysteresis. Uses the same checks as [`Self::admit`].
    ///
    /// A blocked request reports the first reason in this order: zero cost,
    /// insufficient credits, alive limit, pacing. This ensures a pacing
    /// countdown is returned only when the cost and alive count already pass.
    /// The result describes this instant; it does not reserve a later spawn.
    ///
    /// ```
    /// use ferravex_play::encounter::{AdmissionPreview, EncounterConfig, EncounterDirector};
    /// let mut director = EncounterDirector::new(EncounterConfig {
    ///     credits: 3, max_alive: 2, interval: 0.5,
    ///     first_delay: 0.25, recovery_seconds: 1.0,
    /// }).unwrap();
    /// assert_eq!(director.preview(1, 0), AdmissionPreview::Pacing {
    ///     cooldown: 0.25, recovery: 0.0,
    /// });
    /// assert_eq!(director.preview(1, 0).next_available_in(), Some(0.25));
    /// director.advance(0.25, 0.0).unwrap();
    /// assert!(director.preview(1, 0).is_ready());
    /// assert_eq!(director.credits(), 3); // Previewing spends nothing.
    /// assert!(director.admit(1, 0)); // Validate the spawn location first.
    /// assert_eq!(director.credits(), 2);
    /// assert_eq!(director.preview(1, 2), AdmissionPreview::AliveLimit {
    ///     alive: 2, limit: 2,
    /// });
    /// ```
    pub fn preview(&self, cost: u32, alive: usize) -> AdmissionPreview {
        if cost == 0 {
            AdmissionPreview::ZeroCost
        } else if cost > self.credits {
            AdmissionPreview::InsufficientCredits {
                cost,
                credits: self.credits,
            }
        } else if alive >= usize::from(self.config.max_alive) {
            AdmissionPreview::AliveLimit {
                alive,
                limit: self.config.max_alive,
            }
        } else if self.cooldown > 0. || self.recovery > 0. {
            AdmissionPreview::Pacing {
                cooldown: self.cooldown,
                recovery: self.recovery,
            }
        } else {
            AdmissionPreview::Ready
        }
    }
    /// At most one admission per interval, no catch-up spawn bursts. Check the
    /// location first: successful admission spends credits exactly once.
    /// [`Self::preview`] provides the same decision and a reason without mutation.
    pub fn admit(&mut self, cost: u32, alive: usize) -> bool {
        if !self.preview(cost, alive).is_ready() {
            return false;
        }
        self.credits -= cost;
        self.cooldown = f64::from(self.config.interval);
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn director() -> EncounterDirector {
        EncounterDirector::new(EncounterConfig {
            credits: 3,
            max_alive: 2,
            interval: 0.5,
            first_delay: 0.,
            recovery_seconds: 1.,
        })
        .unwrap()
    }
    #[test]
    fn budgets_caps_and_no_catchup_bursts() {
        let mut d = director();
        assert!(!d.admit(1, 2));
        assert!(d.admit(2, 0));
        assert!(!d.admit(1, 0));
        for _ in 0..8 {
            d.advance(0.25, 0.).unwrap();
        }
        assert!(d.admit(1, 0));
        assert_eq!(d.pacing(), Pacing::Exhausted);
        assert!(!d.admit(0, 0));
    }
    #[test]
    fn pressure_hysteresis_and_invalid_input_are_transactional() {
        let mut d = director();
        d.advance(0., 1.).unwrap();
        assert_eq!(d.pacing(), Pacing::Recover);
        assert!(!d.admit(1, 0));
        for _ in 0..4 {
            d.advance(0.25, 1.).unwrap();
        }
        assert!(d.admit(1, 0));
        assert!(d.advance(f32::NAN, 0.).is_err());
        assert_eq!(d.credits(), 2);
        d.advance(0., 0.).unwrap();
        d.advance(0., 1.).unwrap();
        assert_eq!(d.pacing(), Pacing::Recover);
    }

    #[test]
    fn preview_does_not_reserve_or_spend_credits() {
        let mut d = director();
        for _ in 0..4 {
            assert_eq!(d.preview(1, 0), AdmissionPreview::Ready);
            assert_eq!(d.preview(3, 1), AdmissionPreview::Ready);
            assert_eq!(d.preview(3, 1).next_available_in(), Some(0.));
        }
        assert_eq!(d.credits(), 3);
        assert!(d.admit(2, 1));
        assert_eq!(d.credits(), 1);
        let waiting = AdmissionPreview::Pacing {
            cooldown: 0.5,
            recovery: 0.,
        };
        assert_eq!(d.preview(1, 0), waiting);
        assert!(!d.admit(1, 0));
        assert_eq!(d.preview(1, 0), waiting);
        assert_eq!(d.credits(), 1);
    }

    #[test]
    fn cost_and_alive_reasons_take_precedence_over_pacing() {
        let mut d = director();
        assert!(d.admit(1, 0));
        d.advance(0., 1.).unwrap();
        let cases = [
            (0, usize::MAX, AdmissionPreview::ZeroCost),
            (
                u32::MAX,
                usize::MAX,
                AdmissionPreview::InsufficientCredits {
                    cost: u32::MAX,
                    credits: 2,
                },
            ),
            (1, 2, AdmissionPreview::AliveLimit { alive: 2, limit: 2 }),
            (
                1,
                usize::MAX,
                AdmissionPreview::AliveLimit {
                    alive: usize::MAX,
                    limit: 2,
                },
            ),
        ];
        for (cost, alive, expected) in cases {
            let preview = d.preview(cost, alive);
            assert_eq!(preview, expected);
            assert!(!preview.is_ready());
            assert_eq!(preview.next_available_in(), None);
            assert!(!d.admit(cost, alive));
        }
        assert_eq!(d.credits(), 2);
        assert_eq!(
            d.preview(2, 1),
            AdmissionPreview::Pacing {
                cooldown: 0.5,
                recovery: 1.,
            }
        );
    }

    #[test]
    fn next_availability_tracks_first_delay_and_admission_interval() {
        let mut d = EncounterDirector::new(EncounterConfig {
            first_delay: 0.5,
            interval: 0.125,
            ..director().config
        })
        .unwrap();
        assert_eq!(d.preview(1, 0).next_available_in(), Some(0.5));
        d.advance(0., 0.).unwrap();
        assert_eq!(d.preview(1, 0).next_available_in(), Some(0.5));
        d.advance(0.25, 0.).unwrap();
        assert_eq!(d.preview(1, 0).next_available_in(), Some(0.25));
        assert!(!d.admit(1, 0));
        d.advance(0.25, 0.).unwrap();
        assert_eq!(d.preview(1, 0), AdmissionPreview::Ready);
        assert!(d.admit(1, 0));
        assert_eq!(d.preview(1, 0).next_available_in(), Some(0.125));
        d.advance(0.0625, 0.).unwrap();
        assert_eq!(d.preview(1, 0).next_available_in(), Some(0.0625));
        d.advance(0.0625, 0.).unwrap();
        assert!(d.preview(1, 0).is_ready());
        assert!(d.admit(1, 0));
        assert_eq!(d.credits(), 1);
    }

    #[test]
    fn overlapping_pacing_waits_for_the_longer_timer_and_preserves_hysteresis() {
        for (cooldown, recovery) in [(0.5, 0.25), (0.25, 0.5)] {
            let mut d = EncounterDirector::new(EncounterConfig {
                first_delay: cooldown,
                recovery_seconds: recovery,
                ..director().config
            })
            .unwrap();
            d.advance(0., 1.).unwrap();
            assert_eq!(
                d.preview(1, 0),
                AdmissionPreview::Pacing {
                    cooldown: f64::from(cooldown),
                    recovery: f64::from(recovery),
                }
            );
            assert_eq!(d.preview(1, 0).next_available_in(), Some(0.5));
            for expected_wait in [0.375, 0.25, 0.125, 0.] {
                d.advance(0.125, 1.).unwrap();
                assert_eq!(d.preview(1, 0).next_available_in(), Some(expected_wait));
            }
            assert!(d.preview(1, 0).is_ready());
            // Continued pressure does not retrigger a completed recovery.
            d.advance(0., 1.).unwrap();
            assert!(d.preview(1, 0).is_ready());
            d.advance(0., 0.).unwrap();
            assert!(d.preview(1, 0).is_ready());
            // A low-to-high pressure transition can invalidate a ready preview.
            d.advance(0., 1.).unwrap();
            assert_eq!(
                d.preview(1, 0),
                AdmissionPreview::Pacing {
                    cooldown: 0.,
                    recovery: f64::from(recovery),
                }
            );
            assert!(!d.admit(1, 0));
            assert_eq!(d.credits(), 3);
        }
    }

    #[test]
    fn exhaustion_never_promises_time_based_credit_replenishment() {
        for credits in [0, 3] {
            let mut d = EncounterDirector::new(EncounterConfig {
                credits,
                ..director().config
            })
            .unwrap();
            if credits > 0 {
                assert!(d.admit(credits, 0));
            }
            let exhausted = AdmissionPreview::InsufficientCredits {
                cost: 1,
                credits: 0,
            };
            assert_eq!(d.preview(0, 0), AdmissionPreview::ZeroCost);
            assert_eq!(d.preview(1, 0), exhausted);
            assert_eq!(exhausted.next_available_in(), None);
            d.advance(0., 1.).unwrap();
            for _ in 0..8 {
                d.advance(0.25, 1.).unwrap();
            }
            assert_eq!(d.pacing(), Pacing::Exhausted);
            assert_eq!(d.preview(1, 0), exhausted);
            assert!(!d.admit(1, 0));
        }
    }

    #[test]
    fn preview_retains_small_positive_timer_remainders() {
        let mut d = EncounterDirector::new(EncounterConfig {
            first_delay: 0.3,
            ..director().config
        })
        .unwrap();
        d.advance(0.1, 0.).unwrap();
        d.advance(0.2, 0.).unwrap();
        let remaining = f64::from(0.3_f32) - f64::from(0.1_f32) - f64::from(0.2_f32);
        assert!(remaining > 0. && remaining < f64::from(f32::EPSILON));
        assert_eq!(d.preview(1, 0).next_available_in(), Some(remaining));
        assert!(!d.preview(1, 0).is_ready());
        assert!(!d.admit(1, 0));
        d.advance(0., 0.).unwrap();
        assert_eq!(d.preview(1, 0).next_available_in(), Some(remaining));
        d.advance(remaining as f32, 0.).unwrap();
        assert!(d.preview(1, 0).is_ready());
        assert!(d.admit(1, 0));
    }

    #[test]
    fn invalid_advances_preserve_preview_and_recovery_arming() {
        let mut d = director();
        assert!(d.admit(1, 0));
        d.advance(0., 1.).unwrap();
        let before = d.preview(1, 0);
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
            assert_eq!(d.advance(dt, pressure), Err(DirectorError::Invalid));
            assert_eq!(d.preview(1, 0), before);
            assert_eq!(d.credits(), 2);
        }
        for _ in 0..4 {
            d.advance(0.25, 1.).unwrap();
        }
        assert!(d.preview(1, 0).is_ready());
        assert!(d.admit(1, 0));
    }

    #[test]
    fn repeated_previews_preserve_fixed_tick_admissions_and_credit_history() {
        let mut observed = director();
        let mut baseline = observed;
        let mut admitted = 0;
        for tick in 0..128 {
            let pressure = if tick % 16 < 8 { 1. } else { 0. };
            let alive = usize::from(tick % 3 == 0) * 2;
            observed.advance(0.125, pressure).unwrap();
            baseline.advance(0.125, pressure).unwrap();
            for (cost, count) in [(0, 0), (u32::MAX, 0), (1, usize::MAX), (2, 1)] {
                let _ = observed.preview(cost, count).next_available_in();
            }
            let preview = observed.preview(1, alive);
            let accepted = observed.admit(1, alive);
            assert_eq!(preview.is_ready(), accepted);
            assert_eq!(accepted, baseline.admit(1, alive));
            admitted += u32::from(accepted);
            assert_eq!(observed.credits(), 3 - admitted);
            assert_eq!(observed.credits(), baseline.credits());
            assert_eq!(observed.pacing(), baseline.pacing());
            assert_eq!(observed.preview(1, 0), baseline.preview(1, 0));
        }
        assert_eq!(admitted, 3);
    }
}
