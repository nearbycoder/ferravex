//! Fixed-clock spawn admission. Hosts choose safe spawn locations and apply the
//! accepted spawn; the director never reads hidden player positions.
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
    /// At most one admission per interval, no catch-up spawn bursts. Check the
    /// location first: successful admission spends credits exactly once.
    pub fn admit(&mut self, cost: u32, alive: usize) -> bool {
        if cost == 0
            || cost > self.credits
            || alive >= usize::from(self.config.max_alive)
            || self.cooldown > 0.
            || self.recovery > 0.
        {
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
}
