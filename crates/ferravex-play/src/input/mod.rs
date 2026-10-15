//! Bounded action mapping shared by platform hosts. Digital edges and accumulated
//! motion survive until consumed; frames and fixed simulation ticks are separate.
mod config;
#[cfg(test)]
mod tests;
pub use config::{
    AppliedConfig, ControlSettings, DecodedConfig, MAX_CONFIG_BYTES, SavedAction, decode_config,
};
pub mod touch;
pub const MAX_ACTIONS: usize = 256;
pub const MAX_BINDINGS: usize = 64;
pub const MAX_ACTIVE_CODES: usize = 1024;
pub const MAX_CONTEXTS: usize = 16;
pub const MAX_CONTEXT_DEPTH: usize = 8;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum InputCode {
    Key(u16),
    Mouse(u8),
    PadButton(u8),
    PadAxis(u8),
    Touch(u8),
    /// Relative logical-pixel motion, accumulated rather than held.
    MouseDelta(u8),
    TouchDelta(u8),
    Wheel(u8),
}
impl InputCode {
    pub fn is_delta(self) -> bool {
        matches!(
            self,
            Self::MouseDelta(_) | Self::TouchDelta(_) | Self::Wheel(_)
        )
    }
    pub fn valid(self) -> bool {
        match self {
            Self::Key(k) => k != 0,
            Self::Mouse(b) => (1..=5).contains(&b),
            Self::PadButton(b) => b < 32,
            Self::PadAxis(a) => a < 16,
            Self::Touch(t) => t < 64,
            Self::MouseDelta(a) | Self::Wheel(a) => a < 2,
            Self::TouchDelta(a) => a < 16,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Binding {
    pub code: InputCode,
    pub scale: f32,
    pub deadzone: f32,
}
impl Binding {
    pub fn digital(code: InputCode) -> Self {
        Self {
            code,
            scale: 1.,
            deadzone: 0.,
        }
    }
    fn valid(self) -> bool {
        self.code.valid()
            && self.scale.is_finite()
            && self.deadzone.is_finite()
            && (0. ..=0.99).contains(&self.deadzone)
            && (!self.code.is_delta() || self.deadzone == 0.)
    }
}
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Action {
    pub value: f32,
    pub pressed: bool,
    pub released: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    Axis,
    Button,
    Delta,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContextId(u8);
impl ContextId {
    /// Global actions remain active below every stack entry.
    pub const GLOBAL: Self = Self(0);
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextMode {
    Exclusive,
    Passthrough,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputError {
    InvalidName,
    DuplicateName,
    InvalidBinding,
    InvalidHandle,
    ActionCapacity,
    BindingCapacity,
    ContextCapacity,
    ContextDepth,
    InvalidContext,
    InvalidTouch,
    TouchCapacity,
    InvalidConfig,
    ConfigVersion,
    ConfigChecksum,
}
impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "input: {self:?}")
    }
}
impl std::error::Error for InputError {}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-.".contains(&b))
}
struct Entry {
    name: Option<String>,
    kind: ActionKind,
    context: ContextId,
    bindings: Box<[Binding]>,
    state: Action,
    suppressed: bool,
}
/// Registration/rebinding may allocate. Feeds, consumption and context changes
/// reuse bounded storage. Handles are map-local and stay stable for its lifetime.
pub struct ActionMap {
    entries: Vec<Entry>,
    values: Vec<(InputCode, f32)>,
    contexts: Vec<String>,
    stack: Vec<(ContextId, ContextMode)>,
}
impl Default for ActionMap {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            values: Vec::with_capacity(MAX_ACTIVE_CODES),
            contexts: vec!["global".into()],
            stack: Vec::with_capacity(MAX_CONTEXT_DEPTH),
        }
    }
}
impl ActionMap {
    pub fn action(&mut self, bindings: Vec<Binding>) -> usize {
        self.try_action(bindings)
            .expect("valid actions within capacities")
    }
    pub fn try_action(&mut self, bindings: Vec<Binding>) -> Option<usize> {
        self.insert(None, ActionKind::Axis, ContextId::GLOBAL, bindings)
            .ok()
    }
    pub fn named_action(
        &mut self,
        name: &str,
        kind: ActionKind,
        context: ContextId,
        bindings: Vec<Binding>,
    ) -> Result<usize, InputError> {
        if !valid_name(name) {
            return Err(InputError::InvalidName);
        }
        if self.action_id(name).is_some() {
            return Err(InputError::DuplicateName);
        }
        self.insert(Some(name.to_string()), kind, context, bindings)
    }
    fn insert(
        &mut self,
        name: Option<String>,
        kind: ActionKind,
        context: ContextId,
        bindings: Vec<Binding>,
    ) -> Result<usize, InputError> {
        if self.entries.len() >= MAX_ACTIONS {
            return Err(InputError::ActionCapacity);
        }
        if context.0 as usize >= self.contexts.len() {
            return Err(InputError::InvalidContext);
        }
        validate_bindings(kind, &bindings)?;
        let id = self.entries.len();
        self.entries.push(Entry {
            name,
            kind,
            context,
            bindings: bindings.into_boxed_slice(),
            state: Action::default(),
            suppressed: false,
        });
        self.recompute();
        Ok(id)
    }
    pub fn action_id(&self, name: &str) -> Option<usize> {
        self.entries
            .iter()
            .position(|e| e.name.as_deref() == Some(name))
    }
    pub fn named_actions(&self) -> impl Iterator<Item = (usize, &str, ActionKind, ContextId)> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(id, e)| e.name.as_deref().map(|name| (id, name, e.kind, e.context)))
    }
    pub fn bindings(&self, id: usize) -> Option<&[Binding]> {
        self.entries.get(id).map(|e| e.bindings.as_ref())
    }
    pub fn create_context(&mut self, name: &str) -> Result<ContextId, InputError> {
        if !valid_name(name) {
            return Err(InputError::InvalidName);
        }
        if self.contexts.iter().any(|n| n == name) {
            return Err(InputError::DuplicateName);
        }
        if self.contexts.len() >= MAX_CONTEXTS {
            return Err(InputError::ContextCapacity);
        }
        let id = ContextId(self.contexts.len() as u8);
        self.contexts.push(name.to_string());
        Ok(id)
    }
    pub fn context(&self, name: &str) -> Option<ContextId> {
        self.contexts
            .iter()
            .position(|n| n == name)
            .map(|n| ContextId(n as u8))
    }
    pub fn push_context(&mut self, id: ContextId, mode: ContextMode) -> Result<(), InputError> {
        if id == ContextId::GLOBAL
            || id.0 as usize >= self.contexts.len()
            || self.stack.iter().any(|v| v.0 == id)
        {
            return Err(InputError::InvalidContext);
        }
        if self.stack.len() >= MAX_CONTEXT_DEPTH {
            return Err(InputError::ContextDepth);
        }
        let old = self.active_mask();
        self.stack.push((id, mode));
        self.transition(old);
        Ok(())
    }
    pub fn pop_context(&mut self) -> Option<ContextId> {
        let old = self.active_mask();
        let id = self.stack.pop()?.0;
        self.transition(old);
        Some(id)
    }
    pub fn clear_contexts(&mut self) {
        let old = self.active_mask();
        self.stack.clear();
        self.transition(old);
    }
    /// Replace the stack with one exclusive context, or clear gameplay contexts.
    /// Repeating the active setting does not discard edges or suppress input.
    pub fn set_context(&mut self, id: Option<ContextId>) -> Result<(), InputError> {
        if id.is_some_and(|id| id == ContextId::GLOBAL || id.0 as usize >= self.contexts.len()) {
            return Err(InputError::InvalidContext);
        }
        if id.is_none() && self.stack.is_empty()
            || self.stack.len() == 1
                && id == Some(self.stack[0].0)
                && self.stack[0].1 == ContextMode::Exclusive
        {
            return Ok(());
        }
        let old = self.active_mask();
        self.stack.clear();
        if let Some(id) = id {
            self.stack.push((id, ContextMode::Exclusive));
        }
        self.transition(old);
        Ok(())
    }
    fn active_mask(&self) -> u16 {
        let mut mask = 1;
        for &(id, mode) in self.stack.iter().rev() {
            mask |= 1 << id.0;
            if mode == ContextMode::Exclusive {
                break;
            }
        }
        mask
    }
    fn transition(&mut self, old: u16) {
        let new = self.active_mask();
        for entry in &mut self.entries {
            let was = old & (1 << entry.context.0) != 0;
            let active = new & (1 << entry.context.0) != 0;
            if was != active {
                // UI/capture edges must never leak back into gameplay. A held
                // button must go neutral before a newly activated context sees it.
                entry.state.pressed = false;
                entry.state.released = false;
                if entry.kind == ActionKind::Delta {
                    entry.state.value = 0.;
                }
                if active && entry.kind == ActionKind::Button {
                    entry.suppressed = raw_value(entry, &self.values).abs() >= 0.5;
                }
            }
        }
        self.recompute();
    }
    pub fn rebind(&mut self, id: usize, bindings: Vec<Binding>) -> bool {
        self.try_rebind(id, bindings).is_ok()
    }
    pub fn try_rebind(&mut self, id: usize, bindings: Vec<Binding>) -> Result<(), InputError> {
        let entry = self.entries.get_mut(id).ok_or(InputError::InvalidHandle)?;
        validate_bindings(entry.kind, &bindings)?;
        entry.bindings = bindings.into_boxed_slice();
        entry.state = Action::default();
        entry.suppressed =
            entry.kind == ActionKind::Button && raw_value(entry, &self.values).abs() >= 0.5;
        self.recompute();
        Ok(())
    }
    /// Held values normalize to [-1,1]. Relative codes delegate to feed_delta.
    /// Unknown/overflowing/nonfinite physical data cannot grow the map without bound.
    pub fn feed(&mut self, code: InputCode, value: f32) {
        if !code.valid() {
            return;
        }
        if code.is_delta() {
            self.feed_delta(code, value);
            return;
        }
        let value = if value.is_finite() {
            value.clamp(-1., 1.)
        } else {
            0.
        };
        match self.values.binary_search_by_key(&code, |v| v.0) {
            Ok(i) => {
                if self.values[i].1 == value {
                    return;
                }
                if value == 0. {
                    self.values.remove(i);
                } else {
                    self.values[i].1 = value;
                }
            }
            Err(i) => {
                if value == 0. || self.values.len() >= MAX_ACTIVE_CODES {
                    return;
                }
                self.values.insert(i, (code, value));
            }
        }
        self.recompute();
    }
    /// Motion accumulates across render frames with no simulation ticks. Consuming
    /// one delta action does not erase another action bound to the same source.
    pub fn feed_delta(&mut self, code: InputCode, value: f32) {
        if !code.valid() || !code.is_delta() || !value.is_finite() || value == 0. {
            return;
        }
        let active = self.active_mask();
        for entry in &mut self.entries {
            if entry.kind != ActionKind::Delta || active & (1 << entry.context.0) == 0 {
                continue;
            }
            let delta = entry
                .bindings
                .iter()
                .filter(|b| b.code == code)
                .map(|b| f64::from(b.scale) * f64::from(value))
                .sum::<f64>();
            let was = entry.state.value.abs() > 0.000001;
            entry.state.value = (f64::from(entry.state.value) + delta).clamp(-1e6, 1e6) as f32;
            let down = entry.state.value.abs() > 0.000001;
            entry.state.pressed |= down && !was;
            entry.state.released |= !down && was;
        }
    }
    fn recompute(&mut self) {
        let active = self.active_mask();
        for entry in &mut self.entries {
            if entry.kind == ActionKind::Delta {
                continue;
            }
            let raw = raw_value(entry, &self.values);
            if entry.suppressed && raw.abs() < 0.5 {
                entry.suppressed = false;
            }
            let mut value = if active & (1 << entry.context.0) != 0 && !entry.suppressed {
                raw
            } else {
                0.
            };
            if entry.kind == ActionKind::Button {
                value = if value.abs() >= 0.5 {
                    value.signum()
                } else {
                    0.
                };
            }
            let down = value.abs() > 0.001;
            let was = entry.state.value.abs() > 0.001;
            entry.state.pressed |= down && !was;
            entry.state.released |= !down && was;
            entry.state.value = value;
        }
    }
    pub fn peek(&self, id: usize) -> Action {
        self.entries.get(id).map_or(Action::default(), |e| e.state)
    }
    pub fn consume_tick(&mut self, id: usize) -> Action {
        let Some(entry) = self.entries.get_mut(id) else {
            return Action::default();
        };
        let state = entry.state;
        entry.state.pressed = false;
        entry.state.released = false;
        if entry.kind == ActionKind::Delta {
            entry.state.value = 0.;
        }
        state
    }
    pub fn physical_value(&self, code: InputCode) -> f32 {
        self.values
            .binary_search_by_key(&code, |v| v.0)
            .ok()
            .map_or(0., |i| self.values[i].1)
    }
    pub fn active_codes(&self) -> usize {
        self.values.len()
    }
    /// Discard queued edges/motion and require already held buttons to release.
    /// Axes can resume at their current level without a synthetic press.
    pub fn suppress_held(&mut self) {
        for entry in &mut self.entries {
            entry.state.pressed = false;
            entry.state.released = false;
            if entry.kind == ActionKind::Delta {
                entry.state.value = 0.;
            }
            if entry.kind == ActionKind::Button {
                entry.suppressed = raw_value(entry, &self.values).abs() >= 0.5;
            }
        }
        self.recompute();
        for entry in &mut self.entries {
            entry.state.pressed = false;
            entry.state.released = false;
        }
    }
    /// Neutralize held values and discard queued presses/motion on focus loss.
    pub fn lose_focus(&mut self) {
        self.values.clear();
        for e in &mut self.entries {
            let released = e.kind != ActionKind::Delta && e.state.value.abs() > 0.001;
            e.state = Action {
                value: 0.,
                pressed: false,
                released,
            };
            e.suppressed = false;
        }
    }
}
fn validate_bindings(kind: ActionKind, bindings: &[Binding]) -> Result<(), InputError> {
    if bindings.len() > MAX_BINDINGS {
        return Err(InputError::BindingCapacity);
    }
    if bindings
        .iter()
        .any(|b| !b.valid() || b.code.is_delta() != (kind == ActionKind::Delta))
    {
        return Err(InputError::InvalidBinding);
    }
    Ok(())
}
fn raw_value(entry: &Entry, values: &[(InputCode, f32)]) -> f32 {
    entry
        .bindings
        .iter()
        .map(|b| {
            let raw = values
                .binary_search_by_key(&b.code, |v| v.0)
                .ok()
                .map_or(0., |i| f64::from(values[i].1));
            let dead = f64::from(b.deadzone);
            if raw.abs() <= dead {
                0.
            } else {
                raw.signum() * (raw.abs() - dead) / (1. - dead) * f64::from(b.scale)
            }
        })
        .sum::<f64>()
        .clamp(-1., 1.) as f32
}
