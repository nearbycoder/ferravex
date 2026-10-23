//! Bounded action mapping shared by platform hosts. Digital edges and accumulated
//! motion survive until consumed; frames and fixed simulation ticks are separate.
use std::sync::atomic::{AtomicUsize, Ordering};

mod config;
#[cfg(test)]
mod tests;
pub use config::{
    AppliedConfig, BindingConflict, ConfigBinding, ConfigError, ConfigValidation, ControlSettings,
    DecodedConfig, MAX_CONFIG_BYTES, MAX_CONFIG_CONFLICTS, SavedAction, decode_config,
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
/// An opaque action identity owned by one [`ActionMap`].
///
/// Resolve a name once with [`ActionMap::action_handle`], or register with
/// [`ActionMap::named_action_handle`], then sample without string lookups. Copies
/// refer to the same action and share its pending edges and motion; a handle is
/// not an independent input subscription.
///
/// Handles survive moves of their map, new registrations, rebinding, config
/// application, focus changes and context changes. Entries cannot be removed.
/// Replacing or dropping the map makes its handles stale: even an identically
/// configured replacement rejects them with [`InputError::InvalidHandle`].
/// Handles are process-local identities, not save/replay IDs; persist action
/// names instead. Legacy `usize` IDs do not carry this ownership protection.
///
/// ```
/// use ferravex_play::input::{ActionKind, ActionMap, Binding, ContextId, InputCode};
///
/// let mut input = ActionMap::default();
/// input.named_action_handle(
///     "jump", ActionKind::Button, ContextId::GLOBAL,
///     vec![Binding::digital(InputCode::Key(32))],
/// )?;
/// let jump = input.action_handle("jump").expect("registered action");
/// input.feed(InputCode::Key(32), 1.0);
/// assert!(input.peek_handle(jump)?.pressed); // Preview does not consume.
/// assert!(input.consume_tick_handle(jump)?.pressed);
/// let next_tick = input.consume_tick_handle(jump)?;
/// assert_eq!(next_tick.value, 1.0); // A held button stays down.
/// assert!(!next_tick.pressed); // Its edge fires on only one tick.
/// # Ok::<(), ferravex_play::input::InputError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ActionHandle {
    map_id: usize,
    index: usize,
}

/// A rejected atomic group read. Neither input nor caller output is changed.
///
/// Checks run in this order: handle count, output capacity, then handles in
/// caller order. The count includes duplicates and is bounded by [`MAX_ACTIONS`].
/// This separate error leaves the legacy [`InputError`] variants unchanged.
///
/// ```
/// use ferravex_play::input::{Action, ActionMap, ActionSnapshotError};
/// let mut input = ActionMap::default();
/// let id = input.action(vec![]);
/// let handle = input.handle_from_id(id).unwrap();
/// let mut other = ActionMap::default();
/// let id = other.action(vec![]);
/// let foreign = other.handle_from_id(id).unwrap();
/// let sentinel = Action { value: 0.5, ..Action::default() };
/// let mut output = [sentinel; 2];
/// assert_eq!(input.consume_tick_handles_into(&[handle, foreign], &mut output),
///     Err(ActionSnapshotError::InvalidHandle { index: 1 }));
/// assert_eq!(output, [sentinel; 2]);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionSnapshotError {
    /// More than [`MAX_ACTIONS`] handles were requested.
    TooManyHandles { count: usize },
    /// The output cannot hold one snapshot per requested handle.
    OutputTooSmall { required: usize, available: usize },
    /// The handle at this zero-based position is foreign, stale or out of bounds.
    InvalidHandle { index: usize },
}
impl std::fmt::Display for ActionSnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "action snapshot: {self:?}")
    }
}
impl std::error::Error for ActionSnapshotError {}

/// Borrowed metadata for building hints from an action's current bindings.
///
/// Obtain this view from [`ActionMap::action_descriptor`] or
/// [`ActionMap::action_descriptors`]. Names and bindings borrow the map without
/// allocating or consuming input. Reacquire the view after rebinding, applying
/// a config or changing contexts; the borrow prevents concurrent map mutation.
/// Keep the [`ActionHandle`] if a longer-lived identity is needed.
///
/// ```
/// use ferravex_play::input::{ActionKind, ActionMap, Binding, ContextMode, InputCode};
///
/// let mut input = ActionMap::default();
/// let gameplay = input.create_context("gameplay")?;
/// let jump = input.named_action_handle(
///     "jump", ActionKind::Button, gameplay,
///     vec![Binding::digital(InputCode::Key(32))],
/// )?;
/// input.push_context(gameplay, ContextMode::Exclusive)?;
/// let hint = input.action_descriptor(jump)?;
/// assert_eq!(hint.handle, jump);
/// assert_eq!(hint.name, Some("jump"));
/// assert_eq!(hint.kind, ActionKind::Button);
/// assert_eq!(hint.context, gameplay);
/// assert_eq!(hint.context_name, "gameplay");
/// assert!(hint.context_active);
/// assert_eq!(hint.bindings[0].code, InputCode::Key(32));
/// # Ok::<(), ferravex_play::input::InputError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActionDescriptor<'a> {
    /// Identity owned by the map that produced this view, including for unnamed actions.
    pub handle: ActionHandle,
    /// Registered name, or `None` for legacy unnamed actions.
    pub name: Option<&'a str>,
    /// How the action interprets its bindings and consumes input.
    pub kind: ActionKind,
    /// Context used to gate this action in its owning map.
    pub context: ContextId,
    /// Registered context name; global actions use `"global"`.
    pub context_name: &'a str,
    /// Whether the context stack enables this action. Global actions are always
    /// active. This does not indicate focus, held-button suppression, a connected
    /// device or whether the action has any bindings.
    pub context_active: bool,
    /// Current bindings, validated for this action's kind on registration,
    /// rebinding or config application. At most [`MAX_BINDINGS`] entries, in
    /// authored order, including duplicates and zero scales. An empty slice
    /// means unbound. Inspection does not filter by device availability.
    pub bindings: &'a [Binding],
}

// Never recycle identities: handles may outlive their map. Relaxed ordering is
// enough because this counter allocates identities, not shared input state.
static NEXT_MAP_ID: AtomicUsize = AtomicUsize::new(1);

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
/// reuse bounded storage. [`ActionHandle`] access checks map ownership and bounds
/// in constant time without allocating. Legacy `usize` access remains available.
pub struct ActionMap {
    map_id: usize,
    entries: Vec<Entry>,
    values: Vec<(InputCode, f32)>,
    contexts: Vec<String>,
    stack: Vec<(ContextId, ContextMode)>,
}
impl Default for ActionMap {
    fn default() -> Self {
        // Atomic::try_update is newer than the workspace Rust 1.88 minimum.
        #[allow(deprecated)]
        let map_id = NEXT_MAP_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("action map identity space exhausted");
        Self {
            map_id,
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
    /// Register a named action and return its ownership-checked handle.
    ///
    /// Names, bindings, capacity and contexts follow [`Self::named_action`]. A
    /// failed registration leaves existing actions and handles unchanged.
    ///
    /// ```
    /// use ferravex_play::input::{ActionKind, ActionMap, ContextId};
    /// let mut input = ActionMap::default();
    /// let jump = input.named_action_handle("jump", ActionKind::Button, ContextId::GLOBAL, vec![])?;
    /// assert_eq!(input.action_handle("jump"), Some(jump));
    /// # Ok::<(), ferravex_play::input::InputError>(())
    /// ```
    pub fn named_action_handle(
        &mut self,
        name: &str,
        kind: ActionKind,
        context: ContextId,
        bindings: Vec<Binding>,
    ) -> Result<ActionHandle, InputError> {
        let index = self.named_action(name, kind, context, bindings)?;
        Ok(ActionHandle {
            map_id: self.map_id,
            index,
        })
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
    /// Resolve an exact registered name once; missing names return `None`.
    ///
    /// This lookup scans registered names. Subsequent [`Self::peek_handle`] and
    /// [`Self::consume_tick_handle`] calls use constant-time validated indexing.
    /// Resolution also works while the action's context is inactive. See
    /// [`ActionHandle`] for a complete sampling example.
    pub fn action_handle(&self, name: &str) -> Option<ActionHandle> {
        self.action_id(name).and_then(|id| self.handle_from_id(id))
    }
    /// Convert an existing in-bounds index into a handle owned by this map.
    ///
    /// For migration of legacy/unnamed registrations. A raw index has no owner;
    /// this cannot detect an index copied from another map. Pass an index from
    /// this map, or prefer [`Self::action_handle`] for named lookup.
    ///
    /// ```
    /// use ferravex_play::input::ActionMap;
    /// let mut input = ActionMap::default();
    /// let id = input.action(vec![]);
    /// let handle = input.handle_from_id(id).unwrap();
    /// assert_eq!(input.peek_handle(handle)?, input.peek(id));
    /// assert!(input.handle_from_id(usize::MAX).is_none());
    /// # Ok::<(), ferravex_play::input::InputError>(())
    /// ```
    pub fn handle_from_id(&self, id: usize) -> Option<ActionHandle> {
        self.entries.get(id).map(|_| ActionHandle {
            map_id: self.map_id,
            index: id,
        })
    }
    fn validate_handle(&self, handle: ActionHandle) -> Result<usize, InputError> {
        if handle.map_id != self.map_id || handle.index >= self.entries.len() {
            Err(InputError::InvalidHandle)
        } else {
            Ok(handle.index)
        }
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
    /// Inspect bindings without allocating or consuming pending input.
    ///
    /// Stale/foreign handles return [`InputError::InvalidHandle`]. See
    /// [`Self::try_rebind_handle`] for a usage example.
    pub fn bindings_handle(&self, handle: ActionHandle) -> Result<&[Binding], InputError> {
        let id = self.validate_handle(handle)?;
        Ok(self.entries[id].bindings.as_ref())
    }
    /// Inspect metadata and current bindings using an ownership-checked handle.
    ///
    /// Stale or foreign handles return [`InputError::InvalidHandle`], even if
    /// their index/name matches an action here. Inactive and unbound actions are
    /// valid. This does not allocate, consume edges/motion or alter suppression.
    /// Work is bounded by the context stack, not the number of actions.
    ///
    /// ```
    /// use ferravex_play::input::{ActionKind, ActionMap, Binding, ContextId, InputCode};
    /// let mut input = ActionMap::default();
    /// let jump = input.named_action_handle("jump", ActionKind::Button, ContextId::GLOBAL, vec![])?;
    /// assert!(input.action_descriptor(jump)?.bindings.is_empty());
    /// input.try_rebind_handle(jump, vec![Binding::digital(InputCode::Key(74))])?;
    /// assert_eq!(input.action_descriptor(jump)?.bindings[0].code, InputCode::Key(74));
    /// # Ok::<(), ferravex_play::input::InputError>(())
    /// ```
    pub fn action_descriptor(
        &self,
        handle: ActionHandle,
    ) -> Result<ActionDescriptor<'_>, InputError> {
        let id = self.validate_handle(handle)?;
        Ok(self.descriptor_at(id, self.active_mask()))
    }
    /// Enumerate all actions in registration order without allocating or consuming input.
    ///
    /// Includes unnamed, inactive and unbound actions, up to [`MAX_ACTIONS`]. Each
    /// descriptor carries a handle owned by this map and a borrowed slice of its
    /// validated bindings. Filter on [`ActionDescriptor::context_active`] for
    /// context-aware hints, or keep inactive actions for a controls editor.
    ///
    /// ```
    /// use ferravex_play::input::{ActionKind, ActionMap, Binding, ContextId, InputCode};
    /// let mut input = ActionMap::default();
    /// let jump = input.named_action_handle(
    ///     "jump", ActionKind::Button, ContextId::GLOBAL,
    ///     vec![Binding::digital(InputCode::Key(32))],
    /// )?;
    /// input.feed(InputCode::Key(32), 1.0);
    /// for hint in input.action_descriptors().filter(|hint| hint.context_active) {
    ///     assert_eq!(hint.handle, jump);
    ///     for binding in hint.bindings {
    ///         assert_eq!(binding.code, InputCode::Key(32));
    ///     }
    /// }
    /// assert!(input.consume_tick_handle(jump)?.pressed); // Hints leave the edge queued.
    /// # Ok::<(), ferravex_play::input::InputError>(())
    /// ```
    pub fn action_descriptors(
        &self,
    ) -> impl ExactSizeIterator<Item = ActionDescriptor<'_>> + DoubleEndedIterator + '_ {
        let active = self.active_mask();
        (0..self.entries.len()).map(move |id| self.descriptor_at(id, active))
    }
    fn descriptor_at(&self, id: usize, active: u16) -> ActionDescriptor<'_> {
        let entry = &self.entries[id];
        ActionDescriptor {
            handle: ActionHandle {
                map_id: self.map_id,
                index: id,
            },
            name: entry.name.as_deref(),
            kind: entry.kind,
            context: entry.context,
            context_name: &self.contexts[entry.context.0 as usize],
            context_active: active & (1 << entry.context.0) != 0,
            bindings: &entry.bindings,
        }
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
    /// Rebind only if the handle belongs to this map and the bindings are valid.
    ///
    /// Failure leaves bindings and pending input unchanged. Success preserves
    /// the handle and follows [`Self::try_rebind`]: pending input is reset, and
    /// already held buttons must release before triggering the new binding.
    ///
    /// ```
    /// use ferravex_play::input::{ActionKind, ActionMap, Binding, ContextId, InputCode};
    /// let mut input = ActionMap::default();
    /// let jump = input.named_action_handle("jump", ActionKind::Button, ContextId::GLOBAL, vec![])?;
    /// input.try_rebind_handle(jump, vec![Binding::digital(InputCode::Key(32))])?;
    /// assert_eq!(input.bindings_handle(jump)?[0].code, InputCode::Key(32));
    /// # Ok::<(), ferravex_play::input::InputError>(())
    /// ```
    pub fn try_rebind_handle(
        &mut self,
        handle: ActionHandle,
        bindings: Vec<Binding>,
    ) -> Result<(), InputError> {
        let id = self.validate_handle(handle)?;
        self.try_rebind(id, bindings)
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
    /// Read a snapshot without clearing edges or accumulated motion.
    ///
    /// Stale/foreign handles return [`InputError::InvalidHandle`], distinct from
    /// a valid neutral action. Context masking and held-button suppression are
    /// identical to [`Self::peek`]; validity does not depend on context activity.
    /// See [`ActionHandle`] for an example.
    pub fn peek_handle(&self, handle: ActionHandle) -> Result<Action, InputError> {
        let id = self.validate_handle(handle)?;
        Ok(self.entries[id].state)
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
    /// Sample one action for a simulation tick, clearing its queued edges.
    ///
    /// Held axis/button values persist. Delta values are cleared. A press and
    /// release between ticks are both returned once, including through copied
    /// handles or mixed legacy/typed access: consumption is per action, not per
    /// handle. Other actions bound to the same source retain their own input.
    /// Stale/foreign handles return [`InputError::InvalidHandle`] without
    /// consuming anything. See [`ActionHandle`] for an example.
    pub fn consume_tick_handle(&mut self, handle: ActionHandle) -> Result<Action, InputError> {
        let id = self.validate_handle(handle)?;
        Ok(self.consume_tick(id))
    }
    /// Snapshot a fixed group without consuming edges or relative motion.
    ///
    /// Duplicate handles receive identical snapshots. All handles must belong
    /// to this map, including inactive/unbound actions. At most [`MAX_ACTIONS`]
    /// handles (counting duplicates) are accepted; an empty group is valid.
    /// No allocation occurs. See [`Self::peek_handles_into`] for caller storage.
    ///
    /// ```
    /// use ferravex_play::input::{ActionMap, Binding, InputCode};
    /// let mut input = ActionMap::default();
    /// let id = input.action(vec![Binding::digital(InputCode::Key(32))]);
    /// let jump = input.handle_from_id(id).unwrap();
    /// input.feed(InputCode::Key(32), 1.0);
    /// let [a, b] = input.peek_handles([jump, jump])?;
    /// assert_eq!(a, b);
    /// assert!(input.consume_tick_handle(jump)?.pressed);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn peek_handles<const N: usize>(
        &self,
        handles: [ActionHandle; N],
    ) -> Result<[Action; N], ActionSnapshotError> {
        let mut output = [Action::default(); N];
        self.peek_handles_into(&handles, &mut output)?;
        Ok(output)
    }
    /// Copy snapshots into `output[..handles.len()]`, preserving its tail.
    ///
    /// Prevalidates the count, output capacity and **every** handle before any
    /// output write. On error the entire output and all input remain unchanged.
    /// Duplicates, context masking and held suppression follow [`Self::peek_handles`].
    /// Work is O(handle count), bounded by [`MAX_ACTIONS`], without allocation.
    /// A larger output buffer is allowed, including for an empty request.
    ///
    /// ```
    /// use ferravex_play::input::{Action, ActionMap};
    /// let mut input = ActionMap::default();
    /// let id = input.action(vec![]);
    /// let handle = input.handle_from_id(id).unwrap();
    /// let sentinel = Action { value: 0.5, pressed: true, released: true };
    /// let mut output = [sentinel; 2];
    /// input.peek_handles_into(&[handle], &mut output)?;
    /// assert_eq!(output, [Action::default(), sentinel]);
    /// # Ok::<(), ferravex_play::input::ActionSnapshotError>(())
    /// ```
    pub fn peek_handles_into(
        &self,
        handles: &[ActionHandle],
        output: &mut [Action],
    ) -> Result<(), ActionSnapshotError> {
        self.validate_snapshot(handles, output.len())?;
        for (handle, slot) in handles.iter().zip(output) {
            *slot = self.entries[handle.index].state;
        }
        Ok(())
    }
    /// Capture a fixed group for one **executed** simulation tick.
    ///
    /// Like renderer `PlayerInput::take_tick`, every duplicate receives the same
    /// pre-consumption snapshot. Only requested actions are consumed; held
    /// axis/button values persist while pressed/released edges and delta values
    /// clear. Other actions, even on the same source, retain their own input.
    /// Edges coalesce into flags, not a tap count. Context/focus/suppression
    /// policy is unchanged from [`Self::consume_tick_handle`].
    ///
    /// The count (at most [`MAX_ACTIONS`], including duplicates) and every handle
    /// are validated before consuming anything. Empty groups succeed. There is
    /// no allocation or host/time polling. Share the returned array across tick
    /// systems; consume again only for the next executed tick. Zero-tick frames
    /// must only feed/preview input, so pending taps and motion survive.
    ///
    /// This runnable host-loop recipe injects two zero-tick frames followed by a
    /// catch-up batch. Issued steps may exceed executed updates if gameplay ends;
    /// consumption belongs after the decision to execute each update.
    ///
    /// ```
    /// use ferravex_play::input::{ActionKind, ActionMap, Binding, ContextId, InputCode};
    /// let mut input = ActionMap::default();
    /// let jump = input.named_action_handle("jump", ActionKind::Button,
    ///     ContextId::GLOBAL, vec![Binding::digital(InputCode::Key(32))])?;
    /// let pan = input.named_action_handle("pan", ActionKind::Delta,
    ///     ContextId::GLOBAL, vec![Binding::digital(InputCode::MouseDelta(0))])?;
    /// let mut executed = 0;
    /// let mut requests = 0;
    /// let mut motion = 0.0;
    /// let mut playing = true;
    /// // (host button level, relative motion, issued fixed steps)
    /// for (level, delta, issued) in [(1.0, 3.0, 0), (0.0, 5.0, 0), (0.0, 0.0, 3)] {
    ///     input.feed(InputCode::Key(32), level);
    ///     input.feed_delta(InputCode::MouseDelta(0), delta);
    ///     for _ in 0..issued {
    ///         if !playing { break; }
    ///         let [jump, pan] = input.consume_tick_handles([jump, pan])?;
    ///         // Share these values with rules/animation; emit feedback only if accepted.
    ///         requests += u32::from(jump.pressed);
    ///         motion += pan.value;
    ///         executed += 1;
    ///         if executed == 2 { playing = false; }
    ///     }
    /// }
    /// assert_eq!((executed, requests, motion), (2, 1, 8.0));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn consume_tick_handles<const N: usize>(
        &mut self,
        handles: [ActionHandle; N],
    ) -> Result<[Action; N], ActionSnapshotError> {
        let mut output = [Action::default(); N];
        self.consume_tick_handles_into(&handles, &mut output)?;
        Ok(output)
    }
    /// Capture a tick group in caller storage, preserving the unused output tail.
    ///
    /// The complete count, output capacity and every handle are checked before
    /// **any** input/output mutation. On error nothing changes. On success all
    /// snapshots are copied before clearing any input, so duplicates match
    /// [`Self::consume_tick_handles`]. O(handle count) work and no allocation.
    ///
    /// ```
    /// use ferravex_play::input::{Action, ActionMap, Binding, InputCode};
    /// let mut input = ActionMap::default();
    /// let id = input.action(vec![Binding::digital(InputCode::Key(32))]);
    /// let jump = input.handle_from_id(id).unwrap();
    /// input.feed(InputCode::Key(32), 1.0);
    /// input.feed(InputCode::Key(32), 0.0);
    /// let mut output = [Action::default(); 2];
    /// input.consume_tick_handles_into(&[jump, jump], &mut output)?;
    /// assert_eq!(output[0], output[1]);
    /// assert!(output[0].pressed && output[0].released);
    /// assert_eq!(input.peek_handle(jump)?, Action::default());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn consume_tick_handles_into(
        &mut self,
        handles: &[ActionHandle],
        output: &mut [Action],
    ) -> Result<(), ActionSnapshotError> {
        self.peek_handles_into(handles, output)?;
        // Copy every state before clearing any. Re-clearing a duplicate is
        // idempotent, avoiding a search, scratch allocation or quadratic work.
        for handle in handles {
            self.consume_tick(handle.index);
        }
        Ok(())
    }
    fn validate_snapshot(
        &self,
        handles: &[ActionHandle],
        output_len: usize,
    ) -> Result<(), ActionSnapshotError> {
        if handles.len() > MAX_ACTIONS {
            return Err(ActionSnapshotError::TooManyHandles {
                count: handles.len(),
            });
        }
        if output_len < handles.len() {
            return Err(ActionSnapshotError::OutputTooSmall {
                required: handles.len(),
                available: output_len,
            });
        }
        for (index, &handle) in handles.iter().enumerate() {
            self.validate_handle(handle)
                .map_err(|_| ActionSnapshotError::InvalidHandle { index })?;
        }
        Ok(())
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
#[cfg(test)]
#[test]
fn journey400_291_corrupt_late_index_rejects_without_input_or_output_mutation() {
    let mut map = ActionMap::default();
    let id = map.action(vec![Binding::digital(InputCode::Key(1))]);
    let handle = map.handle_from_id(id).unwrap();
    // Only internal tests can construct an in-map, out-of-bounds handle.
    let corrupt = ActionHandle {
        map_id: map.map_id,
        index: usize::MAX,
    };
    map.feed(InputCode::Key(1), 1.);
    let before = map.peek(id);
    let mut output = [Action::default(); 3];
    let expected = Err(ActionSnapshotError::InvalidHandle { index: 2 });
    assert_eq!(
        map.peek_handles_into(&[handle, handle, corrupt], &mut output),
        expected
    );
    assert_eq!(output, [Action::default(); 3]);
    assert_eq!(
        map.consume_tick_handles_into(&[handle, handle, corrupt], &mut output),
        expected
    );
    assert_eq!(output, [Action::default(); 3]);
    assert_eq!(map.peek(id), before);
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

#[cfg(test)]
mod journey200_051 {
    use super::*;

    #[test]
    fn descriptors_borrow_all_metadata_and_preserve_binding_order() {
        let mut map = ActionMap::default();
        assert_eq!(map.action_descriptors().len(), 0);
        assert_eq!(map.action_descriptors().next_back(), None);
        let game = map.create_context("game").unwrap();
        let axis_bindings = vec![
            Binding {
                code: InputCode::PadAxis(0),
                scale: -2.,
                deadzone: 0.25,
            },
            Binding::digital(InputCode::Key(65)),
            Binding::digital(InputCode::Key(65)),
            Binding {
                code: InputCode::Touch(63),
                scale: 0.,
                deadzone: 0.,
            },
        ];
        let legacy = map.action(axis_bindings.clone());
        let jump = map
            .named_action_handle(
                "jump",
                ActionKind::Button,
                game,
                vec![
                    Binding::digital(InputCode::Mouse(1)),
                    Binding::digital(InputCode::PadButton(0)),
                ],
            )
            .unwrap();
        let look = map
            .named_action_handle(
                "look",
                ActionKind::Delta,
                ContextId::GLOBAL,
                vec![
                    Binding::digital(InputCode::MouseDelta(0)),
                    Binding::digital(InputCode::TouchDelta(1)),
                    Binding::digital(InputCode::Wheel(1)),
                ],
            )
            .unwrap();
        let unbound = map
            .named_action_handle("unbound", ActionKind::Button, ContextId::GLOBAL, vec![])
            .unwrap();

        let mut descriptors = map.action_descriptors();
        assert_eq!(descriptors.len(), 4);
        let axis = descriptors.next().unwrap();
        assert_eq!(axis.handle, map.handle_from_id(legacy).unwrap());
        assert_eq!(axis.name, None);
        assert_eq!(axis.kind, ActionKind::Axis);
        assert_eq!(axis.context, ContextId::GLOBAL);
        assert_eq!(axis.context_name, "global");
        assert!(axis.context_active);
        assert_eq!(axis.bindings, axis_bindings);
        assert!(std::ptr::eq(axis.bindings, map.bindings(legacy).unwrap()));

        let last = descriptors.next_back().unwrap();
        assert_eq!(last.handle, unbound);
        assert!(last.bindings.is_empty());
        assert!(last.context_active);
        assert_eq!(descriptors.len(), 2);
        let button = descriptors.next().unwrap();
        assert_eq!(button, map.action_descriptor(jump).unwrap());
        assert_eq!(button.name, Some("jump"));
        assert_eq!(button.kind, ActionKind::Button);
        assert_eq!(button.context, game);
        assert_eq!(button.context_name, "game");
        assert!(!button.context_active);
        assert!(std::ptr::eq(
            button.bindings,
            map.bindings_handle(jump).unwrap()
        ));
        let delta = descriptors.next().unwrap();
        assert_eq!(delta.handle, look);
        assert_eq!(delta.kind, ActionKind::Delta);
        assert_eq!(delta.bindings, map.bindings_handle(look).unwrap());
        assert_eq!(descriptors.len(), 0);
        assert_eq!(descriptors.next(), None);
    }

    #[test]
    fn descriptor_lookup_rejects_foreign_stale_and_out_of_bounds_handles() {
        let mut original = ActionMap::default();
        let original_id = original.action(vec![Binding::digital(InputCode::Key(32))]);
        let old = original.action_descriptors().next().unwrap().handle;
        let mut replacement = ActionMap::default();
        let replacement_id = replacement.action(vec![Binding::digital(InputCode::Key(32))]);
        assert_eq!(original_id, replacement_id);
        let current = replacement.action_descriptors().next().unwrap().handle;
        replacement.feed(InputCode::Key(32), 1.);
        assert_eq!(
            replacement.action_descriptor(old),
            Err(InputError::InvalidHandle)
        );
        assert_eq!(
            original.action_descriptor(current),
            Err(InputError::InvalidHandle)
        );
        original = replacement;
        assert_eq!(
            original.action_descriptor(old),
            Err(InputError::InvalidHandle)
        );
        let mut moved = Box::new(original);
        moved.action(vec![]);
        assert_eq!(moved.action_descriptor(current).unwrap().handle, current);
        for index in [2, usize::MAX] {
            let invalid = ActionHandle {
                map_id: moved.map_id,
                index,
            };
            assert_eq!(
                moved.action_descriptor(invalid),
                Err(InputError::InvalidHandle)
            );
        }
        assert!(moved.consume_tick_handle(current).unwrap().pressed);
    }

    #[test]
    fn descriptors_follow_context_stack_without_changing_held_suppression() {
        let mut map = ActionMap::default();
        let game = map.create_context("game").unwrap();
        let hud = map.create_context("hud").unwrap();
        let menu = map.create_context("menu").unwrap();
        let tooltip = map.create_context("tooltip").unwrap();
        for (name, context) in [
            ("pause", ContextId::GLOBAL),
            ("jump", game),
            ("hint", hud),
            ("accept", menu),
            ("dismiss", tooltip),
        ] {
            map.named_action_handle(
                name,
                ActionKind::Button,
                context,
                vec![Binding::digital(InputCode::Key(32))],
            )
            .unwrap();
        }
        let active_names = |map: &ActionMap| {
            map.action_descriptors()
                .filter(|d| d.context_active)
                .map(|d| d.name.unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        let jump = map.action_handle("jump").unwrap();
        assert_eq!(active_names(&map), ["pause"]);
        map.feed(InputCode::Key(32), 1.);
        map.push_context(game, ContextMode::Exclusive).unwrap();
        assert_eq!(active_names(&map), ["pause", "jump"]);
        assert!(map.action_descriptor(jump).unwrap().context_active);
        assert_eq!(map.consume_tick_handle(jump).unwrap(), Action::default());
        map.push_context(hud, ContextMode::Passthrough).unwrap();
        assert_eq!(active_names(&map), ["pause", "jump", "hint"]);
        map.push_context(menu, ContextMode::Exclusive).unwrap();
        assert_eq!(active_names(&map), ["pause", "accept"]);
        map.push_context(tooltip, ContextMode::Passthrough).unwrap();
        assert_eq!(active_names(&map), ["pause", "accept", "dismiss"]);
        map.pop_context();
        map.pop_context();
        assert_eq!(active_names(&map), ["pause", "jump", "hint"]);
        assert_eq!(map.consume_tick_handle(jump).unwrap(), Action::default());
        map.feed(InputCode::Key(32), 0.);
        map.feed(InputCode::Key(32), 1.);
        let pending = map.peek_handle(jump).unwrap();
        assert!(pending.pressed);
        assert_eq!(active_names(&map), ["pause", "jump", "hint"]);
        assert_eq!(map.consume_tick_handle(jump).unwrap(), pending);
        map.suppress_held();
        assert_eq!(active_names(&map), ["pause", "jump", "hint"]);
        assert_eq!(map.consume_tick_handle(jump).unwrap(), Action::default());
        map.lose_focus();
        assert_eq!(active_names(&map), ["pause", "jump", "hint"]);
        map.clear_contexts();
        assert_eq!(active_names(&map), ["pause"]);
        map.set_context(Some(menu)).unwrap();
        assert_eq!(active_names(&map), ["pause", "accept"]);
    }

    #[test]
    fn hint_reads_leave_zero_tick_taps_and_accumulated_motion_for_one_tick() {
        let mut map = ActionMap::default();
        let jump = map
            .named_action_handle(
                "jump",
                ActionKind::Button,
                ContextId::GLOBAL,
                vec![Binding::digital(InputCode::Key(32))],
            )
            .unwrap();
        let look = map
            .named_action_handle(
                "look",
                ActionKind::Delta,
                ContextId::GLOBAL,
                vec![Binding::digital(InputCode::MouseDelta(0))],
            )
            .unwrap();
        map.feed(InputCode::Key(32), 1.);
        map.feed(InputCode::Key(32), 0.);
        for motion in [3., 5.] {
            map.feed_delta(InputCode::MouseDelta(0), motion);
            for hint in map.action_descriptors() {
                assert_eq!(map.action_descriptor(hint.handle).unwrap(), hint);
            }
        }
        let tap = Action {
            value: 0.,
            pressed: true,
            released: true,
        };
        assert_eq!(map.consume_tick_handle(jump).unwrap(), tap);
        let delta = map.consume_tick_handle(look).unwrap();
        assert_eq!(delta.value, 8.);
        assert!(delta.pressed);
        assert_eq!(map.consume_tick_handle(jump).unwrap(), Action::default());
        assert_eq!(map.consume_tick_handle(look).unwrap(), Action::default());
    }

    #[test]
    fn descriptors_track_rebinding_and_config_without_changing_identity() {
        let mut map = ActionMap::default();
        let original = Binding::digital(InputCode::Key(32));
        let jump = map
            .named_action_handle(
                "jump",
                ActionKind::Button,
                ContextId::GLOBAL,
                vec![original],
            )
            .unwrap();
        let saved = map.encode_config(ControlSettings::default()).unwrap();
        let remapped = [
            Binding::digital(InputCode::PadButton(0)),
            Binding::digital(InputCode::Key(74)),
        ];
        map.try_rebind_handle(jump, remapped.to_vec()).unwrap();
        assert_eq!(map.action_descriptor(jump).unwrap().bindings, remapped);
        map.feed(InputCode::Key(74), 1.);
        let pending = map.peek_handle(jump).unwrap();
        for (bindings, error) in [
            (
                vec![Binding::digital(InputCode::Key(0))],
                InputError::InvalidBinding,
            ),
            (
                vec![Binding {
                    scale: f32::NAN,
                    ..original
                }],
                InputError::InvalidBinding,
            ),
            (
                vec![Binding::digital(InputCode::MouseDelta(0))],
                InputError::InvalidBinding,
            ),
            (
                vec![original; MAX_BINDINGS + 1],
                InputError::BindingCapacity,
            ),
        ] {
            assert_eq!(map.try_rebind_handle(jump, bindings), Err(error));
            let hint = map.action_descriptor(jump).unwrap();
            assert_eq!(hint.handle, jump);
            assert_eq!(hint.bindings, remapped);
            assert_eq!(map.peek_handle(jump).unwrap(), pending);
        }
        assert!(map.apply_config(b"invalid").is_err());
        assert_eq!(map.action_descriptor(jump).unwrap().bindings, remapped);
        assert_eq!(map.peek_handle(jump).unwrap(), pending);
        map.feed(InputCode::Key(32), 1.);
        map.apply_config(&saved).unwrap();
        assert_eq!(map.action_handle("jump"), Some(jump));
        assert_eq!(map.action_descriptor(jump).unwrap().bindings, &[original]);
        assert_eq!(map.consume_tick_handle(jump).unwrap(), Action::default());
        map.feed(InputCode::Key(32), 0.);
        map.feed(InputCode::Key(32), 1.);
        assert!(map.action_descriptor(jump).unwrap().context_active);
        assert!(map.consume_tick_handle(jump).unwrap().pressed);
        map.try_rebind_handle(jump, vec![]).unwrap();
        assert!(map.action_descriptor(jump).unwrap().bindings.is_empty());
    }

    #[test]
    fn enumeration_observes_existing_action_binding_and_context_bounds() {
        let mut map = ActionMap::default();
        let mut context = ContextId::GLOBAL;
        for i in 1..MAX_CONTEXTS {
            context = map.create_context(&format!("context_{i}")).unwrap();
        }
        let bindings = vec![Binding::digital(InputCode::Key(32)); MAX_BINDINGS];
        for i in 0..MAX_ACTIONS {
            map.named_action_handle(
                &format!("action_{i}"),
                ActionKind::Button,
                context,
                bindings.clone(),
            )
            .unwrap();
        }
        assert_eq!(
            map.named_action_handle("overflow", ActionKind::Button, context, vec![]),
            Err(InputError::ActionCapacity)
        );
        assert_eq!(map.action_descriptors().len(), MAX_ACTIONS);
        assert!(map.action_descriptors().all(|d| !d.context_active));
        map.set_context(Some(context)).unwrap();
        for (i, descriptor) in map.action_descriptors().enumerate() {
            assert_eq!(descriptor.name, Some(format!("action_{i}").as_str()));
            assert_eq!(descriptor.handle, map.handle_from_id(i).unwrap());
            assert!(descriptor.context_active);
            assert_eq!(descriptor.bindings.len(), MAX_BINDINGS);
            assert_eq!(descriptor.bindings, bindings);
        }
    }
}
