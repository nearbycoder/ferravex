//! One pending command, consumed only when the host can execute it.
//!
//! Push only fresh gameplay input edges inside executed simulation ticks. Advance
//! once per executed tick, not per render frame or per issued-but-skipped update.
//! Outcome methods never invoke game code: `Executed` means the command was handed
//! to the caller, which must apply its game rules before emitting accepted feedback.
//!
//! ```
//! use ferravex_play::gameplay::buffer::{
//!     AdvanceOutcome, CommandBuffer, ConsumeOutcome, PushOutcome,
//! };
//! let mut commands = CommandBuffer::new(0.125)?;
//! assert_eq!(commands.push_checked(1), PushOutcome::Queued);
//! assert_eq!(commands.push_checked(2), PushOutcome::Replaced(1));
//! assert_eq!(commands.remaining_life(), Some(0.125));
//! assert_eq!(commands.take_if_checked(false), ConsumeOutcome::Gated);
//! assert_eq!(commands.advance_checked(0.0625)?, AdvanceOutcome::Pending);
//! assert_eq!(commands.pending(), Some(2)); // Inspection does not consume.
//! assert_eq!(commands.remaining_life(), Some(0.0625));
//! assert_eq!(commands.take_if_checked(true), ConsumeOutcome::Executed(2));
//! assert_eq!(commands.take_if_checked(true), ConsumeOutcome::Empty);
//! assert_eq!(commands.push_checked(3), PushOutcome::Queued);
//! assert_eq!(commands.advance_checked(0.125)?, AdvanceOutcome::Expired(3));
//! assert_eq!(commands.advance_checked(0.0)?, AdvanceOutcome::Empty);
//! assert_eq!(commands.remaining_life(), None);
//! # Ok::<(), ferravex_play::gameplay::LoopError>(())
//! ```
use super::{LoopError, seconds};

/// Result of admitting a fresh edge into the single command slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushOutcome<T: Copy> {
    /// The empty slot now holds the supplied command for a full lifetime.
    Queued,
    /// The supplied command replaced this old value and reset the lifetime.
    Replaced(T),
}

/// Result of advancing the command lifetime by a validated simulation delta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdvanceOutcome<T: Copy> {
    /// There was no pending command.
    Empty,
    /// The command remains pending; inspect it without consuming via the buffer.
    Pending,
    /// This value expired and was removed by this operation.
    Expired(T),
}

/// Result of asking for a command with a caller-supplied execution gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsumeOutcome<T: Copy> {
    /// No command was pending, regardless of the gate.
    Empty,
    /// The closed gate retained the command and its exact remaining lifetime.
    Gated,
    /// This value was removed for execution by the caller, exactly once.
    Executed(T),
}

/// Allocation-free storage for one pending input edge with a fixed lifetime.
///
/// A new edge always replaces the occupied slot; holding a button must not push
/// again. Expiry occurs only on advancement, including zero-delta advancement:
/// the historical `remaining <= 1e-9` seconds boundary is preserved. Thus a
/// positive lifetime at or below that boundary can be consumed before its first
/// advance, just as with the legacy methods. See the module example for outcomes.
/// Commands need only be `Copy`: admission and outcomes never compare payloads
/// or require them to implement `Debug` or equality traits.
#[derive(Clone, Copy, Debug)]
pub struct CommandBuffer<T: Copy> {
    command: Option<T>,
    remaining: f64,
    lifetime: f64,
}
impl<T: Copy> CommandBuffer<T> {
    pub fn new(lifetime: f32) -> Result<Self, LoopError> {
        let lifetime = seconds(lifetime)?;
        if lifetime == 0. {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            command: None,
            remaining: 0.,
            lifetime,
        })
    }
    /// A new edge replaces the previous command. Holding a button is not an edge.
    pub fn push(&mut self, command: T) {
        let _ = self.push_checked(command);
    }
    /// Admit a fresh edge, returning the displaced value only on replacement.
    ///
    /// The one-slot capacity is deliberate: insertion cannot fail after checked
    /// construction. Even an equal-valued edge replaces and refreshes the slot.
    /// See the module example; this is the implementation used by [`Self::push`].
    pub fn push_checked(&mut self, command: T) -> PushOutcome<T> {
        let previous = self.command.replace(command);
        self.remaining = self.lifetime;
        match previous {
            Some(previous) => PushOutcome::Replaced(previous),
            None => PushOutcome::Queued,
        }
    }
    /// Advance simulation time, discarding the detailed expiry outcome.
    pub fn advance(&mut self, dt: f32) -> Result<(), LoopError> {
        self.advance_checked(dt).map(|_| ())
    }
    /// Advance once per executed tick, reporting a removed expired value once.
    ///
    /// `dt` must be finite and in `0..=3600` seconds, even for an empty buffer.
    /// Invalid input returns [`LoopError::Invalid`] without changing any state.
    /// Expiry uses the existing inclusive `1e-9` seconds boundary, not a rounded
    /// inspection value. No expiry is cached for a later call to report again.
    /// See the module example; [`Self::advance`] shares this implementation.
    pub fn advance_checked(&mut self, dt: f32) -> Result<AdvanceOutcome<T>, LoopError> {
        let dt = seconds(dt)?;
        self.remaining = (self.remaining - dt).max(0.);
        if self.remaining <= 1e-9 {
            let expired = self.command.take();
            self.remaining = 0.;
            return Ok(match expired {
                Some(command) => AdvanceOutcome::Expired(command),
                None => AdvanceOutcome::Empty,
            });
        }
        Ok(AdvanceOutcome::Pending)
    }
    /// Consume if ready, discarding the distinction between empty and gated.
    pub fn take_if(&mut self, ready: bool) -> Option<T> {
        match self.take_if_checked(ready) {
            ConsumeOutcome::Executed(command) => Some(command),
            ConsumeOutcome::Empty | ConsumeOutcome::Gated => None,
        }
    }
    /// Hand off a pending command only when the caller's execution gate is open.
    ///
    /// An empty slot reports `Empty` even for a closed gate. `Gated` changes no
    /// state and invokes no speculative callback. `Executed` clears the command
    /// and remaining life; it does not establish that the caller's game rules
    /// accepted the action. See the module example and [`Self::take_if`].
    pub fn take_if_checked(&mut self, ready: bool) -> ConsumeOutcome<T> {
        match self.command {
            None => ConsumeOutcome::Empty,
            Some(_) if !ready => ConsumeOutcome::Gated,
            Some(command) => {
                self.clear();
                ConsumeOutcome::Executed(command)
            }
        }
    }
    /// Discard any pending command and life, retaining the configured lifetime.
    pub fn clear(&mut self) {
        self.command = None;
        self.remaining = 0.;
    }
    pub fn pending(&self) -> Option<T> {
        self.command
    }
    /// Read remaining seconds without consuming, advancing or checking expiry.
    ///
    /// `None` means the slot is empty. `Some` preserves the internal `f64`
    /// precision used by expiry, including a just-pushed sub-epsilon lifetime.
    /// Repeated inspection and closed gates cannot refresh the lifetime.
    /// See the module example for inspection before and after execution/expiry.
    pub fn remaining_life(&self) -> Option<f64> {
        self.command.map(|_| self.remaining)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_commands_survive_then_execute_once_or_expire() {
        let mut b = CommandBuffer::new(0.12).unwrap();
        b.push(7);
        assert_eq!(b.take_if(false), None);
        b.advance(0.1).unwrap();
        assert_eq!(b.take_if(true), Some(7));
        assert_eq!(b.take_if(true), None);
        b.push(9);
        b.advance(0.2).unwrap();
        assert_eq!(b.take_if(true), None);
        b.push(2);
        assert_eq!(b.advance(f32::NAN), Err(LoopError::Invalid));
        assert_eq!(b.pending(), Some(2));
        b.push(3);
        assert_eq!(b.take_if(true), Some(3));
    }

    #[test]
    fn replacement_returns_only_the_displaced_value_and_refreshes_one_slot() {
        let mut b = CommandBuffer::new(0.125).unwrap();
        assert_eq!(b.push_checked(1), PushOutcome::Queued);
        assert_eq!(b.advance_checked(0.0625), Ok(AdvanceOutcome::Pending));
        assert_eq!(b.push_checked(2), PushOutcome::Replaced(1));
        assert_eq!(b.remaining_life(), Some(0.125));
        assert_eq!(b.push_checked(2), PushOutcome::Replaced(2));
        for next in 3..1000 {
            assert_eq!(b.push_checked(next), PushOutcome::Replaced(next - 1));
        }
        assert_eq!(b.take_if_checked(true), ConsumeOutcome::Executed(999));
        assert_eq!(b.take_if_checked(true), ConsumeOutcome::Empty);
        assert_eq!(b.advance_checked(0.125), Ok(AdvanceOutcome::Empty));
        assert_eq!(b.remaining_life(), None);
        assert_eq!(b.push_checked(1000), PushOutcome::Queued);
    }

    #[test]
    fn inspection_and_gates_preserve_the_exact_command_and_life() {
        let mut b = CommandBuffer::new(0.125).unwrap();
        assert_eq!(b.take_if_checked(false), ConsumeOutcome::<u8>::Empty);
        b.push(7);
        b.advance(0.03125).unwrap();
        for _ in 0..10 {
            assert_eq!(b.pending(), Some(7));
            assert_eq!(b.remaining_life(), Some(0.09375));
            assert_eq!(b.take_if_checked(false), ConsumeOutcome::Gated);
            assert_eq!(b.advance_checked(0.), Ok(AdvanceOutcome::Pending));
        }
        assert_eq!(b.advance_checked(0.09375), Ok(AdvanceOutcome::Expired(7)));
        assert_eq!(b.advance_checked(0.), Ok(AdvanceOutcome::Empty));
        assert_eq!(b.take_if_checked(false), ConsumeOutcome::Empty);
        assert_eq!(b.take_if_checked(true), ConsumeOutcome::Empty);
    }

    #[test]
    fn zero_and_public_epsilon_neighbors_keep_legacy_expiry() {
        assert!(matches!(
            CommandBuffer::<u8>::new(0.),
            Err(LoopError::Invalid)
        ));
        // 1e-9 rounds down as f32; its next representable neighbor rounds up.
        let below = 1e-9_f32;
        let above = f32::from_bits(below.to_bits() + 1);
        assert!(f64::from(below) < 1e-9);
        assert!(f64::from(above) > 1e-9);
        for lifetime in [f32::from_bits(1), below] {
            let mut b = CommandBuffer::new(lifetime).unwrap();
            b.push(1);
            assert_eq!(b.remaining_life(), Some(f64::from(lifetime)));
            assert_eq!(b.take_if_checked(false), ConsumeOutcome::Gated);
            assert_eq!(b.take_if_checked(true), ConsumeOutcome::Executed(1));
            b.push(2);
            assert_eq!(b.advance_checked(0.), Ok(AdvanceOutcome::Expired(2)));
            assert_eq!(b.remaining_life(), None);
        }
        let mut b = CommandBuffer::new(above).unwrap();
        b.push(3);
        assert_eq!(b.advance_checked(0.), Ok(AdvanceOutcome::Pending));
        // Subtracting a valid small delta crosses the existing epsilon boundary.
        assert_eq!(b.advance_checked(below), Ok(AdvanceOutcome::Expired(3)));
        b.push(4);
        assert_eq!(b.advance_checked(above), Ok(AdvanceOutcome::Expired(4)));
    }

    #[test]
    fn exact_internal_epsilon_is_inclusive_and_inspection_does_not_expire_it() {
        let mut b = CommandBuffer::new(0.125).unwrap();
        b.push(8);
        // Pin the exact internal comparison separately from f32 input rounding.
        b.remaining = 1e-9;
        assert_eq!(b.remaining_life(), Some(1e-9));
        assert_eq!(b.pending(), Some(8));
        assert_eq!(b.take_if_checked(false), ConsumeOutcome::Gated);
        assert_eq!(b.advance_checked(0.), Ok(AdvanceOutcome::Expired(8)));
        assert_eq!(b.advance_checked(0.), Ok(AdvanceOutcome::Empty));
    }

    #[test]
    fn invalid_deltas_preserve_all_state_in_empty_and_occupied_buffers() {
        let mut b = CommandBuffer::new(3600.).unwrap();
        for occupied in [false, true] {
            if occupied {
                b.push(9);
                b.advance(0.125).unwrap();
            }
            let before = (b.command, b.remaining, b.lifetime);
            for invalid in [
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                -0.001,
                f32::from_bits(3600_f32.to_bits() + 1),
            ] {
                assert_eq!(b.advance_checked(invalid), Err(LoopError::Invalid));
                assert_eq!((b.command, b.remaining, b.lifetime), before);
                assert_eq!(b.advance(invalid), Err(LoopError::Invalid));
                assert_eq!((b.command, b.remaining, b.lifetime), before);
                assert!(matches!(
                    CommandBuffer::<u8>::new(invalid),
                    Err(LoopError::Invalid)
                ));
            }
        }
        assert_eq!(b.advance_checked(3600.), Ok(AdvanceOutcome::Expired(9)));
        assert_eq!(b.advance_checked(-0.), Ok(AdvanceOutcome::Empty));
    }

    #[test]
    fn clear_resets_pending_life_but_retains_authored_configuration() {
        let mut b = CommandBuffer::new(0.125).unwrap();
        for command in 0..3 {
            b.push(command);
            b.advance(0.0625).unwrap();
            b.clear();
            b.clear();
            assert_eq!(b.pending(), None);
            assert_eq!(b.remaining_life(), None);
            assert_eq!(b.take_if_checked(false), ConsumeOutcome::Empty);
            assert_eq!(b.advance_checked(0.), Ok(AdvanceOutcome::Empty));
            assert_eq!(b.push_checked(command + 1), PushOutcome::Queued);
            assert_eq!(b.remaining_life(), Some(0.125));
        }
    }

    #[test]
    fn legacy_wrappers_share_checked_state_transitions() {
        let mut old = CommandBuffer::new(0.125).unwrap();
        let mut checked = old;
        for command in 0..3 {
            old.push(command);
            checked.push_checked(command);
            assert_eq!(old.advance(0.03125), Ok(()));
            assert_eq!(
                checked.advance_checked(0.03125),
                Ok(AdvanceOutcome::Pending)
            );
            assert_eq!(old.take_if(false), None);
            assert_eq!(checked.take_if_checked(false), ConsumeOutcome::Gated);
            assert_eq!(old.pending(), checked.pending());
            assert_eq!(old.remaining_life(), checked.remaining_life());
        }
        assert_eq!(old.take_if(true), Some(2));
        assert_eq!(checked.take_if_checked(true), ConsumeOutcome::Executed(2));
        old.push(3);
        checked.push_checked(3);
        old.advance(0.125).unwrap();
        assert_eq!(
            checked.advance_checked(0.125),
            Ok(AdvanceOutcome::Expired(3))
        );
        assert_eq!(
            (old.command, old.remaining),
            (checked.command, checked.remaining)
        );
    }

    #[test]
    fn tick_input_edges_refresh_only_on_actual_new_presses() {
        use crate::input::{ActionMap, Binding, InputCode};

        let mut input = ActionMap::default();
        let code = InputCode::Key(32);
        let jump = input.try_action(vec![Binding::digital(code)]).unwrap();
        let mut b = CommandBuffer::new(0.0625).unwrap();
        input.feed(code, 1.);
        assert!(input.peek(jump).pressed); // A zero-tick frame consumes nothing.
        assert_eq!(b.pending(), None);
        assert!(input.consume_tick(jump).pressed);
        assert_eq!(b.push_checked(1), PushOutcome::Queued);
        assert_eq!(b.advance_checked(0.015625), Ok(AdvanceOutcome::Pending));
        assert_eq!(b.take_if_checked(false), ConsumeOutcome::Gated);
        assert!(!input.consume_tick(jump).pressed); // Held in a catch-up update.
        assert_eq!(b.advance_checked(0.015625), Ok(AdvanceOutcome::Pending));
        assert_eq!(b.remaining_life(), Some(0.03125));
        input.feed(code, 0.);
        input.feed(code, 1.);
        assert!(input.consume_tick(jump).pressed);
        assert_eq!(b.push_checked(2), PushOutcome::Replaced(1));
        assert_eq!(b.remaining_life(), Some(0.0625));
        assert_eq!(b.take_if_checked(true), ConsumeOutcome::Executed(2));
    }
}
