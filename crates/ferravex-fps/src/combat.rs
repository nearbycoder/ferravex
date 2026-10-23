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
/// Accepted transitions during one successful local motor/weapon computation.
/// Flags describe executed work, not requested buttons. A buffered/coyote jump
/// can be accepted after the press; a held jump is not a new acceptance.
/// Processing order is jump, reload completion, reload start, then fire. In
/// particular, completion and fire can both be true on the same tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerTransitions {
    pub reload_started: bool,
    pub reload_completed: bool,
    pub jumped: bool,
    pub fired: bool,
}
impl PlayerTransitions {
    /// Whether this computation accepted none of the four transitions.
    pub fn is_empty(self) -> bool {
        !self.reload_started && !self.reload_completed && !self.jumped && !self.fired
    }
}

/// Descriptive diagnostics for the additive local step; the bool compatibility
/// helpers still return only `fired`. No new validation is imposed on those
/// helpers. Movement errors retain their existing sanitized aim update, while
/// motion, jump state, weapon timers and ammunition remain untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerStepError {
    InvalidWeaponSettings,
    Movement(CharacterError),
}
impl std::fmt::Display for PlayerStepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "local player step: {self:?}")
    }
}
impl std::error::Error for PlayerStepError {}

/// A local computation's outcomes, without authority, tick, session or life
/// claims. Prediction and reconciliation legitimately compute the same outcome
/// again. This type does not suppress simulation or globally deduplicate work.
/// Errors have empty transitions; dead countdown/respawn ticks are error-free
/// with empty transitions. Authority policy gates (frozen/no-auto-respawn and
/// invalid shot time) belong to Match, not this pure local helper.
///
/// This runnable first-party recipe predicts a reload completion twice from the
/// same snapshot, then executes and re-executes the authority tick. Only the
/// confirmed channel owns feedback. Its bounded caller ledger retains keys
/// across reconciliation/seek, so repeated observations play completion once.
/// The existing checkpoint contains no transition sidecar.
/// ```
/// use ferravex_fps::{Match, StepOutput, InputFrame, Player,
///     step_player_detailed, BUTTON_RELOAD, BUTTON_FIRE};
/// let mut game = Match::default();
/// assert!(game.spawn_player(0));
/// game.player_mut(0).unwrap().ammo = 0;
/// let mut output = StepOutput::new();
/// let reload = InputFrame { buttons: BUTTON_RELOAD, ..Default::default() };
/// game.step_into(&[(0, reload)], &mut output).unwrap();
/// assert!(output.transitions()[0].transitions.reload_started);
/// while game.player(0).unwrap().reload_ticks > 1 {
///     game.step_into(&[], &mut output).unwrap();
/// }
/// let mut checkpoint = Vec::new();
/// game.checkpoint_into(&mut checkpoint).unwrap();
/// let saved = game.player(0).unwrap().snapshot();
/// let fire = InputFrame { buttons: BUTTON_FIRE, ..Default::default() };
/// let mut first_prediction = None;
/// for _ in 0..2 { // Ordinary prediction and recomputation after reconciliation.
///     let mut predicted = Player::from_snapshot(&saved);
///     let local = step_player_detailed(&mut predicted, fire, &game.arena);
///     assert!(local.error.is_none());
///     assert!(local.transitions.reload_completed && local.transitions.fired);
///     if let Some(first) = first_prediction { assert_eq!(local, first); }
///     first_prediction = Some(local); // No confirmed sound/animation here.
/// }
/// let session = 19_u64; // Host-owned session identity, never a client claim.
/// let mut seen = [None; 64]; // Bounded recent confirmation window.
/// let mut cursor = 0;
/// let mut completion_feedback = 0;
/// let mut first_authority = None;
/// for _ in 0..2 {
///     game = Match::from_checkpoint(&checkpoint).unwrap();
///     game.step_into(&[(0, fire)], &mut output).unwrap();
///     let accepted = output.transitions()[0];
///     assert_eq!(accepted.tick, game.tick); // Executed tick, not input sequence.
///     if let Some(first) = first_authority { assert_eq!(accepted, first); }
///     first_authority = Some(accepted);
///     let key = (session, accepted.player, accepted.life, accepted.tick);
///     if !seen.contains(&Some(key)) {
///         seen[cursor] = Some(key);
///         cursor = (cursor + 1) % seen.len();
///         completion_feedback += usize::from(accepted.transitions.reload_completed);
///         // Dispatch sounds/animation from all accepted flags here.
///     }
/// }
/// assert_eq!(completion_feedback, 1);
/// output.clear(); // Clear presentation output on reset; retain allocations.
/// assert!(output.transitions().is_empty());
/// ```
///
/// Production consumers must size the recent ledger for their replay/prediction
/// window. Silent historical playback/seek should not dispatch live feedback;
/// entries evicted from a bounded ledger can be observed again. Start a fresh
/// session identity on a new game/discontinuous timeline and before tick identity
/// can alias after a full u32 cycle; do not clear the live ledger on ordinary
/// reconciliation. Use the full authority life generation below, not an input
/// sequence or just a player slot. If dispatching flags separately, add the flag
/// kind to the key. Optional speculative feedback needs its own ownership and
/// confirmation handoff; it is not automatically confirmed by this result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerStepResult {
    pub transitions: PlayerTransitions,
    pub error: Option<PlayerStepError>,
}

/// At most one aggregate nonempty transition record per checked roster actor.
/// Fixed inline storage has this capacity even in a zero-event StepOutput.
pub const MAX_STEP_TRANSITIONS: usize = MAX_PLAYERS;

/// Accepted Match motor/weapon outcomes before damage resolution. An actor can
/// fire/jump and subsequently be killed in that tick. No record is emitted for
/// an empty outcome or for dead countdown/respawn, deferred spawn, frozen or
/// disabled-auto-respawn actors. GameEvent remains the damage/respawn channel.
/// This observation is local output only, absent from all existing wire,
/// checkpoint and replay encodings. Re-execution can return the same record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StepTransition {
    pub player: u8,
    /// Resulting Match::tick after this accepted tick (wrapping u32), matching
    /// the tick hosts attach to GameEvents; not a ShotTime or InputFrame sequence.
    pub tick: u32,
    /// Full authority life generation at execution, before later shot damage.
    pub life: u64,
    pub transitions: PlayerTransitions,
}

// Checked callers preflight the bounded unique roster before clearing output.
// Inline storage avoids a third fallible capacity contract and preserves public
// StepCounts literals and exhaustive StepError matches.
#[derive(Debug)]
pub(crate) struct TransitionBuffer {
    records: [StepTransition; MAX_STEP_TRANSITIONS],
    len: usize,
}
impl TransitionBuffer {
    pub(crate) fn new() -> Self {
        Self {
            records: [StepTransition::default(); MAX_STEP_TRANSITIONS],
            len: 0,
        }
    }
    pub(crate) fn clear(&mut self) {
        self.len = 0;
    }
    pub(crate) fn push(&mut self, record: StepTransition) {
        self.records[self.len] = record;
        self.len += 1;
    }
    pub(crate) fn as_slice(&self) -> &[StepTransition] {
        &self.records[..self.len]
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
