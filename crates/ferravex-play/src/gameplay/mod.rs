//! Composable game-loop rules. Hosts provide accepted events and simulation time.
//! No renderer, ECS, platform, or external dependencies. See `docs/GAMEPLAY_40.md`.
pub mod buffer;

/// Invalid authoring/input, capacity exhaustion, or an unavailable action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopError {
    Invalid,
    Capacity,
    Unavailable,
}
fn seconds(value: f32) -> Result<f64, LoopError> {
    if value.is_finite() && (0.0..=3600.0).contains(&value) {
        Ok(f64::from(value))
    } else {
        Err(LoopError::Invalid)
    }
}

pub mod combo;

pub mod progression;

pub mod loot;

pub mod pity;

pub mod pickup;

pub mod inventory;

pub mod crafting;

pub mod upgrades;

pub mod magazine;

pub mod charge;
