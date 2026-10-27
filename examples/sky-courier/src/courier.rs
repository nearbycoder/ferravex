//! Deterministic courier scoring: ring lines, tip streaks, hidden rings and the
//! drop grade. Everything here is updated from accepted fixed ticks only; the
//! presentation reads it, never writes it.
use super::{GATES, Vec3, vec3};

/// Three optional golden rings well off the authored route. Each pays a flat
/// tip and tops up boost; finding all three pays a scenic-route bonus.
pub(super) const BONUS_RINGS: [Vec3; 3] = [
    // Skim the swell between the start beacon and gate two.
    vec3(30., -6., 150.),
    // Above the cloud bank after gate three.
    vec3(-24., 84., 380.),
    // Between the eastern islands on the final approach.
    vec3(66., 26., 612.),
];
pub(super) const BONUS_RADIUS: f32 = 6.;
pub(super) const BONUS_TIP: u32 = 50;
pub(super) const SCENIC_TIP: u32 = 100;
pub(super) const MAX_MULTIPLIER: u32 = 5;
const GATE_TIP: u32 = 10;
const HOT_TIP: u32 = 5;

/// How close to the ring centre the aircraft's line passed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Line {
    Bullseye,
    Clean,
    Scrape,
}
impl Line {
    fn from_miss(miss: f32) -> Self {
        if miss <= 2.5 {
            Self::Bullseye
        } else if miss <= 5.5 {
            Self::Clean
        } else {
            Self::Scrape
        }
    }
    fn tip(self) -> u32 {
        match self {
            Self::Bullseye => 10,
            Self::Clean => 5,
            Self::Scrape => 0,
        }
    }
}

/// One accepted gate pass, kept for callouts and the shift summary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct GatePass {
    pub line: Line,
    /// Boosting through the ring pays a small style tip.
    pub hot: bool,
    pub multiplier: u32,
    pub tip: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum DropGrade {
    Bumpy,
    Smooth,
    Bullseye,
}
impl DropGrade {
    pub fn name(self) -> &'static str {
        match self {
            Self::Bullseye => "BULLSEYE DROP",
            Self::Smooth => "SMOOTH DROP",
            Self::Bumpy => "BUMPY DROP",
        }
    }
    fn tip(self) -> u32 {
        match self {
            Self::Bullseye => 60,
            Self::Smooth => 40,
            Self::Bumpy => 20,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Drop {
    pub grade: DropGrade,
    pub multiplier: u32,
    /// Grade tip after the streak multiplier.
    pub tip: u32,
    /// One dollar per whole second left on the delivery window.
    pub early: u32,
}

/// Why the most recent impact happened; selects a quip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Impact {
    #[default]
    Water,
    Island,
    Boundary,
}

/// The run's accepted courier ledger.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Ledger {
    pub tips: u32,
    /// Consecutive clean gates since the last impact.
    pub streak: u32,
    pub best_streak: u32,
    pub gates: [Option<GatePass>; GATES],
    pub bullseyes: u32,
    pub drop: Option<Drop>,
    /// Bit `i` is set once hidden ring `i` was flown through.
    pub bonus: u8,
    pub impacts: u32,
    pub last_impact: Impact,
}

impl Ledger {
    /// The multiplier the next accepted gate or drop will receive.
    pub fn multiplier(&self) -> u32 {
        (self.streak + 1).min(MAX_MULTIPLIER)
    }

    pub fn bonus_count(&self) -> u32 {
        self.bonus.count_ones()
    }

    pub fn scenic(&self) -> bool {
        self.bonus_count() as usize == BONUS_RINGS.len()
    }

    /// `miss` is the distance from the ring centre to the flight line.
    pub fn gate(&mut self, index: usize, miss: f32, hot: bool) -> GatePass {
        let line = Line::from_miss(miss);
        let multiplier = self.multiplier();
        let tip = (GATE_TIP + line.tip() + if hot { HOT_TIP } else { 0 }) * multiplier;
        let pass = GatePass {
            line,
            hot,
            multiplier,
            tip,
        };
        self.tips += tip;
        self.streak += 1;
        self.best_streak = self.best_streak.max(self.streak);
        self.bullseyes += u32::from(line == Line::Bullseye);
        self.gates[index] = Some(pass);
        pass
    }

    pub fn impact(&mut self, cause: Impact) {
        self.streak = 0;
        self.impacts += 1;
        self.last_impact = cause;
    }

    /// Returns the tip paid, or `None` if this ring was already collected.
    pub fn bonus(&mut self, index: usize) -> Option<u32> {
        let bit = 1 << index;
        if self.bonus & bit != 0 {
            return None;
        }
        self.bonus |= bit;
        let tip = BONUS_TIP + if self.scenic() { SCENIC_TIP } else { 0 };
        self.tips += tip;
        Some(tip)
    }

    /// Grade the final low pass from its line and airspeed.
    pub fn deliver(&mut self, miss: f32, speed: f32, remaining: f32) -> Drop {
        let grade = if miss <= 2.5 && speed <= 15. {
            DropGrade::Bullseye
        } else if miss <= 5. {
            DropGrade::Smooth
        } else {
            DropGrade::Bumpy
        };
        let multiplier = self.multiplier();
        let drop = Drop {
            grade,
            multiplier,
            tip: grade.tip() * multiplier,
            early: remaining.max(0.).floor() as u32,
        };
        self.tips += drop.tip + drop.early;
        self.drop = Some(drop);
        drop
    }
}

/// Distance from `center` to the infinite line through `pos` along `forward`.
pub(super) fn line_miss(pos: Vec3, forward: Vec3, center: Vec3) -> f32 {
    let to = center - pos;
    (to - forward * to.dot(forward)).length()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaks_multiply_tips_cap_and_reset_on_impact() {
        let mut ledger = Ledger::default();
        let tips: Vec<_> = (0..5).map(|i| ledger.gate(i, 1., false).tip).collect();
        assert_eq!(tips, [20, 40, 60, 80, 100]);
        assert_eq!(ledger.multiplier(), MAX_MULTIPLIER);
        assert_eq!(
            (ledger.streak, ledger.best_streak, ledger.bullseyes),
            (5, 5, 5)
        );
        ledger.impact(Impact::Island);
        assert_eq!((ledger.streak, ledger.best_streak), (0, 5));
        assert_eq!(ledger.multiplier(), 1);
        assert_eq!(ledger.last_impact, Impact::Island);
        let scrape = ledger.gate(0, 7., true);
        assert_eq!((scrape.line, scrape.tip), (Line::Scrape, 15));
        assert_eq!(ledger.gate(1, 4., false).line, Line::Clean);
        assert_eq!(ledger.tips, 300 + 15 + 30);
    }

    #[test]
    fn hidden_rings_pay_once_and_the_full_set_pays_the_scenic_bonus() {
        let mut ledger = Ledger::default();
        assert_eq!(ledger.bonus(1), Some(BONUS_TIP));
        assert_eq!(ledger.bonus(1), None);
        assert_eq!(ledger.bonus(0), Some(BONUS_TIP));
        assert!(!ledger.scenic());
        assert_eq!(ledger.bonus(2), Some(BONUS_TIP + SCENIC_TIP));
        assert!(ledger.scenic());
        assert_eq!(ledger.tips, 3 * BONUS_TIP + SCENIC_TIP);
        assert_eq!(ledger.streak, 0, "hidden rings do not build streaks");
    }

    #[test]
    fn drop_grade_rewards_slow_centred_passes_and_early_arrival() {
        let mut ledger = Ledger {
            streak: 2,
            ..Ledger::default()
        };
        let drop = ledger.deliver(1., 13., 41.7);
        assert_eq!(drop.grade, DropGrade::Bullseye);
        assert_eq!((drop.multiplier, drop.tip, drop.early), (3, 180, 41));
        assert_eq!(ledger.tips, 221);
        assert_eq!(
            Ledger::default().deliver(1., 17., 0.).grade,
            DropGrade::Smooth
        );
        assert_eq!(
            Ledger::default().deliver(6., 13., 0.).grade,
            DropGrade::Bumpy
        );
        let miss = line_miss(Vec3::ZERO, Vec3::Z, vec3(3., 4., 20.));
        assert!((miss - 5.).abs() < 1e-5);
    }
}
