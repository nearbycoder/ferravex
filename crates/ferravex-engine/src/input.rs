//! Platform-independent action mapping. Edges survive until a simulation tick consumes them.
//! At most 256 actions, 64 bindings per action, and 1024 active physical values.
use std::collections::BTreeMap;
const MAX_ACTIONS: usize = 256;
const MAX_BINDINGS: usize = 64;
const MAX_ACTIVE_CODES: usize = 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum InputCode {
    Key(u16),
    Mouse(u8),
    PadButton(u8),
    PadAxis(u8),
    Touch(u8),
}
#[derive(Clone, Copy, Debug)]
pub struct Binding {
    pub code: InputCode,
    pub scale: f32,
    pub deadzone: f32,
}
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Action {
    pub value: f32,
    pub pressed: bool,
    pub released: bool,
}
struct Entry {
    bindings: Box<[Binding]>,
    state: Action,
}
#[derive(Default)]
pub struct ActionMap {
    entries: Vec<Entry>,
    values: BTreeMap<InputCode, f32>,
}
impl ActionMap {
    /// Registers trusted configuration. Use try_action for externally supplied mappings.
    pub fn action(&mut self, bindings: Vec<Binding>) -> usize {
        self.try_action(bindings)
            .expect("action or binding capacity exceeded")
    }
    pub fn try_action(&mut self, bindings: Vec<Binding>) -> Option<usize> {
        if self.entries.len() >= MAX_ACTIONS || bindings.len() > MAX_BINDINGS {
            return None;
        }
        let id = self.entries.len();
        self.entries.push(Entry {
            bindings: bindings.into_boxed_slice(),
            state: Action::default(),
        });
        self.recompute();
        Some(id)
    }
    /// Invalid ids or oversized replacements leave the old mapping untouched.
    pub fn rebind(&mut self, id: usize, bindings: Vec<Binding>) -> bool {
        if bindings.len() > MAX_BINDINGS {
            return false;
        }
        let Some(entry) = self.entries.get_mut(id) else {
            return false;
        };
        entry.bindings = bindings.into_boxed_slice();
        self.recompute();
        true
    }
    pub fn feed(&mut self, code: InputCode, value: f32) {
        let value = if value.is_finite() {
            value.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        if value == 0.0 {
            self.values.remove(&code);
        } else {
            if self.values.len() >= MAX_ACTIVE_CODES && !self.values.contains_key(&code) {
                return;
            }
            self.values.insert(code, value);
        }
        self.recompute();
    }
    fn recompute(&mut self) {
        for entry in &mut self.entries {
            // Wider accumulation avoids inf + -inf from finite, extreme binding scales.
            let value = entry
                .bindings
                .iter()
                .map(|binding| {
                    let raw = f64::from(self.values.get(&binding.code).copied().unwrap_or(0.0));
                    let dead = if binding.deadzone.is_finite() {
                        f64::from(binding.deadzone.clamp(0.0, 0.99))
                    } else {
                        0.0
                    };
                    if raw.abs() <= dead || !binding.scale.is_finite() {
                        0.0
                    } else {
                        raw.signum() * (raw.abs() - dead) / (1.0 - dead) * f64::from(binding.scale)
                    }
                })
                .sum::<f64>()
                .clamp(-1.0, 1.0) as f32;
            let down = value.abs() > 0.001;
            let was = entry.state.value.abs() > 0.001;
            entry.state.pressed |= down && !was;
            entry.state.released |= !down && was;
            entry.state.value = value;
        }
    }
    /// Invalid handles have a neutral state; action ids are local to this map.
    pub fn peek(&self, id: usize) -> Action {
        self.entries
            .get(id)
            .map_or(Action::default(), |entry| entry.state)
    }
    pub fn consume_tick(&mut self, id: usize) -> Action {
        let Some(entry) = self.entries.get_mut(id) else {
            return Action::default();
        };
        let state = entry.state;
        entry.state.pressed = false;
        entry.state.released = false;
        state
    }
    pub fn lose_focus(&mut self) {
        self.values.clear();
        self.recompute();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn binding(code: u16, scale: f32) -> Binding {
        Binding {
            code: InputCode::Key(code),
            scale,
            deadzone: 0.0,
        }
    }
    #[test]
    fn quick_tap_survives_no_simulation_and_edges_fire_once() {
        let mut map = ActionMap::default();
        let jump = map.action(vec![binding(32, 1.0)]);
        map.feed(InputCode::Key(32), 1.0);
        map.feed(InputCode::Key(32), 0.0);
        assert_eq!(
            map.consume_tick(jump),
            Action {
                value: 0.0,
                pressed: true,
                released: true
            }
        );
        assert_eq!(map.consume_tick(jump), Action::default());
        map.feed(InputCode::Key(32), 1.0);
        map.lose_focus();
        assert_eq!(map.peek(jump).value, 0.0);
    }
    #[test]
    fn finite_extreme_scales_and_nonfinite_inputs_cannot_poison_actions() {
        let mut map = ActionMap::default();
        let id = map.action(vec![
            binding(1, f32::MAX),
            binding(1, f32::MAX),
            binding(1, -f32::MAX),
            binding(1, -f32::MAX),
        ]);
        map.feed(InputCode::Key(1), 1.0);
        assert_eq!(map.peek(id).value, 0.0);
        assert!(map.rebind(id, vec![binding(1, 1.0)]));
        assert_eq!(map.peek(id).value, 1.0);
        map.feed(InputCode::Key(1), f32::NAN);
        assert_eq!(map.peek(id).value, 0.0);
    }
    #[test]
    fn released_codes_free_capacity_and_registration_observes_held_input() {
        let mut map = ActionMap::default();
        for code in 0..1100 {
            map.feed(InputCode::Key(code), 1.0);
            map.feed(InputCode::Key(code), 0.0);
        }
        map.feed(InputCode::Key(1200), 1.0);
        let id = map.action(vec![binding(1200, 1.0)]);
        assert!(map.peek(id).pressed);
        assert_eq!(map.peek(id).value, 1.0);
        assert_eq!(map.values.len(), 1);
    }
    #[test]
    fn rebind_and_handle_limits_are_transactional() {
        let mut map = ActionMap::default();
        let id = map.action(vec![binding(1, 1.0)]);
        assert!(!map.rebind(id, vec![binding(2, 1.0); MAX_BINDINGS + 1]));
        map.feed(InputCode::Key(1), 1.0);
        assert_eq!(map.peek(id).value, 1.0);
        assert!(
            map.try_action(vec![binding(2, 1.0); MAX_BINDINGS + 1])
                .is_none()
        );
        assert!(!map.rebind(999, vec![]));
        assert_eq!(map.consume_tick(999), Action::default());
    }
}
