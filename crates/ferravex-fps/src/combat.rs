//! Shared weapon timing and an authority policy for practice or tactical rules.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeaponSettings {
    pub magazine: u8,
    pub fire_ticks: u16,
    pub reload_ticks: u16,
}
impl Default for WeaponSettings {
    fn default() -> Self {
        Self {
            magazine: MAGAZINE_SIZE,
            fire_ticks: FIRE_INTERVAL_TICKS,
            reload_ticks: RELOAD_TICKS,
        }
    }
}
impl WeaponSettings {
    pub fn valid(self) -> bool {
        (1..=MAGAZINE_SIZE).contains(&self.magazine)
            && (1..=4096).contains(&self.fire_ticks)
            && (1..=4096).contains(&self.reload_ticks)
    }
}
/// The authority owns policy. Clients submit controls, never health or hits.
/// Raw inputs pass through the policy before the simulation sanitizes them.
pub trait CombatRules {
    fn input(&mut self, _player: &Player, input: InputFrame) -> InputFrame {
        input
    }
    fn frozen(&self, _id: u8) -> bool {
        false
    }
    fn auto_respawn(&self) -> bool {
        true
    }
    fn weapon(&self, _id: u8) -> WeaponSettings {
        WeaponSettings::default()
    }
    fn damage(&mut self, _attacker: u8, _victim: &Player, headshot: bool) -> Option<u8> {
        Some(if headshot { 100 } else { 34 })
    }
}
pub struct PracticeRules;
impl CombatRules for PracticeRules {}
