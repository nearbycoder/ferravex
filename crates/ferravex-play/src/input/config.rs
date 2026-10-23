//! Versioned controls only. Checksums detect accidental corruption; hosts own storage.
use super::*;
use ferravex_math::Vec2;
pub const MAX_CONFIG_BYTES: usize = 256 * 1024;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlSettings {
    /// Radians per normalized viewport-width/height mouse delta.
    pub mouse_sensitivity: f32,
    /// Radians per logical display pixel for a touch-look region.
    pub touch_sensitivity: f32,
    /// Radians/second for a fully deflected right stick.
    pub pad_look: Vec2,
    pub pad_deadzone: f32,
    pub pad_curve: f32,
    pub invert_y: bool,
    pub gamepad: u8,
}
impl Default for ControlSettings {
    fn default() -> Self {
        Self {
            mouse_sensitivity: 1.9,
            touch_sensitivity: 0.004,
            pad_look: Vec2::new(2.5, 2.),
            pad_deadzone: 0.18,
            pad_curve: 1.,
            invert_y: false,
            gamepad: 0,
        }
    }
}
impl ControlSettings {
    pub fn valid(self) -> bool {
        self.mouse_sensitivity.is_finite()
            && (0.01..=20.).contains(&self.mouse_sensitivity)
            && self.touch_sensitivity.is_finite()
            && (0.00005..=0.1).contains(&self.touch_sensitivity)
            && self.pad_look.is_finite()
            && self.pad_look.min_element() >= 0.
            && self.pad_look.max_element() <= 20.
            && self.pad_deadzone.is_finite()
            && (0. ..=0.95).contains(&self.pad_deadzone)
            && self.pad_curve.is_finite()
            && (0.25..=4.).contains(&self.pad_curve)
            && self.gamepad < 4
    }
    /// Radial response preserves direction and bounds diagonals to the unit disk.
    pub fn stick(self, raw: Vec2) -> Vec2 {
        if !self.valid() || !raw.is_finite() {
            return Vec2::ZERO;
        }
        let raw = raw.clamp(Vec2::splat(-1.), Vec2::ONE);
        let length = raw.length();
        if length <= self.pad_deadzone || length < 1e-8 {
            return Vec2::ZERO;
        }
        raw / length
            * ((length.min(1.) - self.pad_deadzone) / (1. - self.pad_deadzone)).powf(self.pad_curve)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct SavedAction {
    pub name: String,
    pub bindings: Vec<Binding>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedConfig {
    pub settings: ControlSettings,
    pub actions: Vec<SavedAction>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppliedConfig {
    pub settings: ControlSettings,
    pub matched: usize,
    pub unknown: usize,
}

/// Maximum number of shared-source pairs retained by [`ActionMap::validate_config`].
pub const MAX_CONFIG_CONFLICTS: usize = 256;

/// A binding's location in a proposed configuration, including host-owned context.
///
/// IDs are local to the inspected map; names are stable registration names.
/// `binding_index` addresses imported bindings for a matched action, or retained
/// live bindings for an omitted/unnamed action. See [`ActionMap::validate_config`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigBinding {
    pub action_id: usize,
    pub action_name: Option<String>,
    pub action_kind: ActionKind,
    pub context: ContextId,
    pub context_name: String,
    pub binding_index: usize,
}

/// Two nonzero-scale bindings using the same source in currently active contexts.
///
/// This includes duplicate bindings within one action. Different scales and
/// deadzones still share a source; intentional sharing can use the permissive
/// [`ActionMap::apply_config`]. See [`ActionMap::validate_config`] for an example.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingConflict {
    pub code: InputCode,
    pub first: ConfigBinding,
    pub second: ConfigBinding,
}

/// Non-mutating preview of a valid file merged with this host's registrations.
///
/// Reports at most [`MAX_CONFIG_CONFLICTS`] pairs in source-code, action-ID, then
/// binding-index order. `conflicts_truncated` means additional pairs exist, not
/// that validation of the file was skipped. See [`ActionMap::validate_config`].
#[derive(Clone, Debug, PartialEq)]
pub struct ConfigValidation {
    pub applied: AppliedConfig,
    /// Retired/unrecognized names, in file order; their bindings are ignored.
    pub unknown_actions: Vec<String>,
    pub conflicts: Vec<BindingConflict>,
    pub conflicts_truncated: bool,
}

/// A config decoding, host compatibility, or strict-import rejection.
///
/// [`ActionMap::validate_config`] returns conflicts in a successful preview;
/// only [`ActionMap::apply_config_checked`] returns `Conflicts`. The boxed report
/// bounds the size of this error while preserving diagnostics for the caller.
#[derive(Clone, Debug, PartialEq)]
pub enum ConfigError {
    /// The existing decoder rejected the file, before host matching.
    Input(InputError),
    /// A decoded binding uses a delta source for a held action, or vice versa.
    IncompatibleBinding(ConfigBinding),
    /// Strict import rejected shared sources in the current active contexts.
    Conflicts(Box<ConfigValidation>),
}
impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Input(error) => write!(f, "{error}"),
            Self::IncompatibleBinding(binding) => write!(
                f,
                "input config: binding {} of action {:?} (id {}, {:?}) in context {:?} is incompatible",
                binding.binding_index,
                binding.action_name,
                binding.action_id,
                binding.action_kind,
                binding.context_name,
            ),
            Self::Conflicts(report) => write!(
                f,
                "input config: {}{} active binding conflicts",
                report.conflicts.len(),
                if report.conflicts_truncated { "+" } else { "" },
            ),
        }
    }
}
impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            _ => None,
        }
    }
}
fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c9dc5_u32, |h, b| {
        (h ^ u32::from(*b)).wrapping_mul(0x01000193)
    })
}
fn code_parts(code: InputCode) -> (u8, u16) {
    match code {
        InputCode::Key(n) => (0, n),
        InputCode::Mouse(n) => (1, n as u16),
        InputCode::PadButton(n) => (2, n as u16),
        InputCode::PadAxis(n) => (3, n as u16),
        InputCode::Touch(n) => (4, n as u16),
        InputCode::MouseDelta(n) => (5, n as u16),
        InputCode::TouchDelta(n) => (6, n as u16),
        InputCode::Wheel(n) => (7, n as u16),
    }
}
fn parse_code(tag: u8, n: u16) -> Result<InputCode, InputError> {
    if tag != 0 && n > u8::MAX as u16 {
        return Err(InputError::InvalidConfig);
    }
    let code = match tag {
        0 => InputCode::Key(n),
        1 => InputCode::Mouse(n as u8),
        2 => InputCode::PadButton(n as u8),
        3 => InputCode::PadAxis(n as u8),
        4 => InputCode::Touch(n as u8),
        5 => InputCode::MouseDelta(n as u8),
        6 => InputCode::TouchDelta(n as u8),
        7 => InputCode::Wheel(n as u8),
        _ => return Err(InputError::InvalidConfig),
    };
    if code.valid() {
        Ok(code)
    } else {
        Err(InputError::InvalidConfig)
    }
}
impl ActionMap {
    /// Save named mappings by stable action name; host registration order can change.
    pub fn encode_config(&self, settings: ControlSettings) -> Result<Vec<u8>, InputError> {
        if !settings.valid() {
            return Err(InputError::InvalidConfig);
        }
        let count = self.entries.iter().filter(|e| e.name.is_some()).count();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"FXIN");
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&(count as u16).to_le_bytes());
        for v in [
            settings.mouse_sensitivity,
            settings.touch_sensitivity,
            settings.pad_look.x,
            settings.pad_look.y,
            settings.pad_deadzone,
            settings.pad_curve,
        ] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        bytes.extend_from_slice(&[u8::from(settings.invert_y), settings.gamepad, 0, 0]);
        for entry in &self.entries {
            let Some(name) = &entry.name else {
                continue;
            };
            bytes.extend_from_slice(&[name.len() as u8, entry.bindings.len() as u8]);
            bytes.extend_from_slice(name.as_bytes());
            for b in &entry.bindings {
                let (tag, n) = code_parts(b.code);
                bytes.push(tag);
                bytes.extend_from_slice(&n.to_le_bytes());
                bytes.extend_from_slice(&b.scale.to_le_bytes());
                bytes.extend_from_slice(&b.deadzone.to_le_bytes());
            }
        }
        let sum = checksum(&bytes);
        bytes.extend_from_slice(&sum.to_le_bytes());
        if bytes.len() > MAX_CONFIG_BYTES {
            return Err(InputError::InvalidConfig);
        }
        Ok(bytes)
    }
    /// Validate the whole file before mutating mappings. Unknown retired action
    /// names are counted/ignored; omitted new actions retain host defaults.
    /// Shared sources remain allowed for compatibility. Use
    /// [`Self::validate_config`] to preview them or [`Self::apply_config_checked`]
    /// to reject conflicts in the current active contexts.
    pub fn apply_config(&mut self, bytes: &[u8]) -> Result<AppliedConfig, InputError> {
        let decoded = decode_config(bytes)?;
        let applied = self
            .validate_decoded(&decoded)
            .map_err(|_| InputError::InvalidBinding)?;
        self.commit_config(decoded);
        Ok(applied)
    }

    /// Preview an import without changing bindings, contexts, held suppression,
    /// pending edges, or accumulated motion. Settings are returned, never applied
    /// to a platform host. The existing FXIN v1 format remains unchanged.
    ///
    /// All decoded bindings must be structurally valid; matched names must also
    /// match the host's action kind, including inactive actions. Unknown names
    /// are ignored, and omitted/unnamed actions keep their current bindings.
    /// Conflicts are computed from that complete proposed mapping.
    ///
    /// Only contexts active under the **current** stack participate: global is
    /// always active, exclusive contexts hide lower contexts, and passthrough
    /// contexts allow overlap. Reuse across inactive contexts is legitimate for
    /// this snapshot. Revalidate after changing the stack; this is not a promise
    /// about future context combinations. Physical values and transient held
    /// suppression do not affect diagnostics. Zero-scale bindings are ignored.
    ///
    /// This configuration-time operation allocates bounded scratch storage for
    /// at most `MAX_ACTIONS * MAX_BINDINGS` bindings and retains at most
    /// [`MAX_CONFIG_CONFLICTS`] diagnostic pairs. It does not run on input feeds.
    ///
    /// ```
    /// use ferravex_play::input::{ActionKind, ActionMap, Binding, ContextId,
    ///     ContextMode, ControlSettings, InputCode};
    /// let mut map = ActionMap::default();
    /// let game = map.create_context("game")?;
    /// let menu = map.create_context("menu")?;
    /// let space = Binding::digital(InputCode::Key(32));
    /// map.named_action("jump", ActionKind::Button, game, vec![space])?;
    /// map.named_action("accept", ActionKind::Button, menu, vec![space])?;
    /// let bytes = map.encode_config(ControlSettings::default())?;
    /// map.set_context(Some(game))?;
    /// map.push_context(menu, ContextMode::Exclusive)?;
    /// assert!(map.validate_config(&bytes)?.conflicts.is_empty());
    /// map.pop_context();
    /// map.push_context(menu, ContextMode::Passthrough)?;
    /// let report = map.validate_config(&bytes)?;
    /// assert_eq!(report.applied.matched, 2);
    /// let conflict = &report.conflicts[0];
    /// assert_eq!(conflict.code, InputCode::Key(32));
    /// assert_eq!(conflict.first.action_name.as_deref(), Some("jump"));
    /// assert_eq!(conflict.second.context_name, "menu");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn validate_config(&self, bytes: &[u8]) -> Result<ConfigValidation, ConfigError> {
        let decoded = decode_config(bytes).map_err(ConfigError::Input)?;
        self.inspect_config(&decoded)
    }

    /// Import only if the entire file is valid, compatible, and has no active
    /// binding conflicts. Any error leaves the map and all pending input intact.
    ///
    /// Uses the same current-context snapshot as [`Self::validate_config`],
    /// including retained defaults and unnamed actions. Success follows
    /// [`Self::apply_config`]: matched actions discard pending edges/motion,
    /// held buttons require release, and omitted actions retain their state.
    /// Hosts can deliberately accept shared sources using `apply_config`.
    ///
    /// ```
    /// use ferravex_play::input::{ActionKind, ActionMap, Binding, ContextId,
    ///     ControlSettings, InputCode};
    /// let mut map = ActionMap::default();
    /// let space = Binding::digital(InputCode::Key(32));
    /// let jump = map.named_action("jump", ActionKind::Button, ContextId::GLOBAL, vec![space])?;
    /// map.named_action("fire", ActionKind::Button, ContextId::GLOBAL, vec![space])?;
    /// let bytes = map.encode_config(ControlSettings::default())?;
    /// map.feed(InputCode::Key(32), 1.0);
    /// assert!(map.apply_config_checked(&bytes).is_err());
    /// assert!(map.consume_tick(jump).pressed); // Rejection preserved the edge.
    /// # Ok::<(), ferravex_play::input::InputError>(())
    /// ```
    pub fn apply_config_checked(&mut self, bytes: &[u8]) -> Result<AppliedConfig, ConfigError> {
        let decoded = decode_config(bytes).map_err(ConfigError::Input)?;
        let report = self.inspect_config(&decoded)?;
        if !report.conflicts.is_empty() {
            return Err(ConfigError::Conflicts(Box::new(report)));
        }
        self.commit_config(decoded);
        Ok(report.applied)
    }

    fn config_binding(&self, action_id: usize, binding_index: usize) -> ConfigBinding {
        let entry = &self.entries[action_id];
        ConfigBinding {
            action_id,
            action_name: entry.name.clone(),
            action_kind: entry.kind,
            context: entry.context,
            context_name: self.contexts[entry.context.0 as usize].clone(),
            binding_index,
        }
    }

    // The decoder has already checked all scalar values and storage bounds.
    // Host kind compatibility is the only remaining binding validity check.
    fn validate_decoded(&self, decoded: &DecodedConfig) -> Result<AppliedConfig, ConfigBinding> {
        let mut matched = 0;
        for action in &decoded.actions {
            if let Some(id) = self.action_id(&action.name) {
                let delta = self.entries[id].kind == ActionKind::Delta;
                if let Some(index) = action
                    .bindings
                    .iter()
                    .position(|binding| binding.code.is_delta() != delta)
                {
                    return Err(self.config_binding(id, index));
                }
                matched += 1;
            }
        }
        Ok(AppliedConfig {
            settings: decoded.settings,
            matched,
            unknown: decoded.actions.len() - matched,
        })
    }

    fn inspect_config(&self, decoded: &DecodedConfig) -> Result<ConfigValidation, ConfigError> {
        let applied = self
            .validate_decoded(decoded)
            .map_err(ConfigError::IncompatibleBinding)?;
        let mut report = ConfigValidation {
            applied,
            unknown_actions: Vec::new(),
            conflicts: Vec::new(),
            conflicts_truncated: false,
        };
        let mut proposed: Vec<&[Binding]> = self
            .entries
            .iter()
            .map(|entry| entry.bindings.as_ref())
            .collect();
        for action in &decoded.actions {
            if let Some(id) = self.action_id(&action.name) {
                proposed[id] = &action.bindings;
            } else {
                report.unknown_actions.push(action.name.clone());
            }
        }

        let active = self.active_mask();
        let mut sources = Vec::new();
        for (id, bindings) in proposed.iter().enumerate() {
            if active & (1 << self.entries[id].context.0) == 0 {
                continue;
            }
            for (index, binding) in bindings.iter().enumerate() {
                if binding.scale != 0. {
                    sources.push((binding.code, id, index));
                }
            }
        }
        sources.sort_unstable();
        let mut start = 0;
        'groups: while start < sources.len() {
            let code = sources[start].0;
            let mut end = start + 1;
            while end < sources.len() && sources[end].0 == code {
                end += 1;
            }
            for first in start..end {
                for second in first + 1..end {
                    if report.conflicts.len() == MAX_CONFIG_CONFLICTS {
                        report.conflicts_truncated = true;
                        break 'groups;
                    }
                    report.conflicts.push(BindingConflict {
                        code,
                        first: self.config_binding(sources[first].1, sources[first].2),
                        second: self.config_binding(sources[second].1, sources[second].2),
                    });
                }
            }
            start = end;
        }
        Ok(report)
    }

    fn commit_config(&mut self, decoded: DecodedConfig) {
        // Finish all allocation before touching live bindings or pending input.
        let replacements: Vec<_> = decoded
            .actions
            .into_iter()
            .filter_map(|action| {
                self.action_id(&action.name)
                    .map(|id| (id, action.bindings.into_boxed_slice()))
            })
            .collect();
        if replacements.is_empty() {
            return;
        }
        for (id, bindings) in replacements {
            let entry = &mut self.entries[id];
            entry.bindings = bindings;
            entry.state = Action::default();
            entry.suppressed =
                entry.kind == ActionKind::Button && raw_value(entry, &self.values).abs() >= 0.5;
        }
        self.recompute();
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], InputError> {
        let end = self
            .cursor
            .checked_add(n)
            .ok_or(InputError::InvalidConfig)?;
        let out = self
            .bytes
            .get(self.cursor..end)
            .ok_or(InputError::InvalidConfig)?;
        self.cursor = end;
        Ok(out)
    }
    fn byte(&mut self) -> Result<u8, InputError> {
        Ok(self.take(1)?[0])
    }
    fn word(&mut self) -> Result<u16, InputError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn float(&mut self) -> Result<f32, InputError> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
}
pub fn decode_config(bytes: &[u8]) -> Result<DecodedConfig, InputError> {
    if bytes.len() < 40 || bytes.len() > MAX_CONFIG_BYTES {
        return Err(InputError::InvalidConfig);
    }
    let payload = &bytes[..bytes.len() - 4];
    let sum = u32::from_le_bytes(bytes[bytes.len() - 4..].try_into().unwrap());
    if checksum(payload) != sum {
        return Err(InputError::ConfigChecksum);
    }
    let mut r = Reader {
        bytes: payload,
        cursor: 0,
    };
    if r.take(4)? != b"FXIN" {
        return Err(InputError::InvalidConfig);
    }
    if r.word()? != 1 {
        return Err(InputError::ConfigVersion);
    }
    let count = r.word()? as usize;
    if count > MAX_ACTIONS {
        return Err(InputError::ActionCapacity);
    }
    let mouse_sensitivity = r.float()?;
    let touch_sensitivity = r.float()?;
    let pad_look = Vec2::new(r.float()?, r.float()?);
    let pad_deadzone = r.float()?;
    let pad_curve = r.float()?;
    let invert = r.byte()?;
    let gamepad = r.byte()?;
    if invert > 1 || r.word()? != 0 {
        return Err(InputError::InvalidConfig);
    }
    let settings = ControlSettings {
        mouse_sensitivity,
        touch_sensitivity,
        pad_look,
        pad_deadzone,
        pad_curve,
        invert_y: invert == 1,
        gamepad,
    };
    if !settings.valid() {
        return Err(InputError::InvalidConfig);
    }
    let mut actions: Vec<SavedAction> = Vec::with_capacity(count);
    for _ in 0..count {
        let name_len = r.byte()? as usize;
        let n = r.byte()? as usize;
        if n > MAX_BINDINGS {
            return Err(InputError::BindingCapacity);
        }
        let name = std::str::from_utf8(r.take(name_len)?).map_err(|_| InputError::InvalidName)?;
        if !valid_name(name) {
            return Err(InputError::InvalidName);
        }
        if actions.iter().any(|a| a.name == name) {
            return Err(InputError::DuplicateName);
        }
        let mut bindings = Vec::with_capacity(n);
        for _ in 0..n {
            let tag = r.byte()?;
            let code = parse_code(tag, r.word()?)?;
            let binding = Binding {
                code,
                scale: r.float()?,
                deadzone: r.float()?,
            };
            if !binding.valid() {
                return Err(InputError::InvalidBinding);
            }
            bindings.push(binding);
        }
        actions.push(SavedAction {
            name: name.to_string(),
            bindings,
        });
    }
    if r.cursor != payload.len() {
        return Err(InputError::InvalidConfig);
    }
    Ok(DecodedConfig { settings, actions })
}
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct LiveState {
        entries: Vec<(Vec<Binding>, Action, bool)>,
        values: Vec<(InputCode, f32)>,
        stack: Vec<(ContextId, ContextMode)>,
    }
    fn live_state(map: &ActionMap) -> LiveState {
        LiveState {
            entries: map
                .entries
                .iter()
                .map(|entry| (entry.bindings.to_vec(), entry.state, entry.suppressed))
                .collect(),
            values: map.values.clone(),
            stack: map.stack.clone(),
        }
    }

    fn map() -> ActionMap {
        let mut m = ActionMap::default();
        m.named_action(
            "jump",
            ActionKind::Button,
            ContextId::GLOBAL,
            vec![Binding::digital(InputCode::Key(32))],
        )
        .unwrap();
        m.named_action(
            "look_x",
            ActionKind::Delta,
            ContextId::GLOBAL,
            vec![Binding::digital(InputCode::MouseDelta(0))],
        )
        .unwrap();
        m
    }
    fn resign(bytes: &mut [u8]) {
        let n = bytes.len() - 4;
        let sum = checksum(&bytes[..n]);
        bytes[n..].copy_from_slice(&sum.to_le_bytes());
    }
    #[test]
    fn roundtrip_uses_stable_names_and_preserves_new_defaults() {
        let m = map();
        let settings = ControlSettings {
            invert_y: true,
            pad_curve: 1.5,
            ..Default::default()
        };
        let bytes = m.encode_config(settings).unwrap();
        assert_eq!(decode_config(&bytes).unwrap().settings, settings);
        let mut target = ActionMap::default();
        let added = target
            .named_action(
                "new_action",
                ActionKind::Axis,
                ContextId::GLOBAL,
                vec![Binding::digital(InputCode::Key(97))],
            )
            .unwrap();
        let look = target
            .named_action("look_x", ActionKind::Delta, ContextId::GLOBAL, vec![])
            .unwrap();
        let jump = target
            .named_action("jump", ActionKind::Button, ContextId::GLOBAL, vec![])
            .unwrap();
        let result = target.apply_config(&bytes).unwrap();
        assert_eq!(
            result,
            AppliedConfig {
                settings,
                matched: 2,
                unknown: 0
            }
        );
        assert_eq!(
            target.bindings(look).unwrap()[0].code,
            InputCode::MouseDelta(0)
        );
        assert_eq!(target.bindings(jump).unwrap()[0].code, InputCode::Key(32));
        assert_eq!(target.bindings(added).unwrap()[0].code, InputCode::Key(97));
    }
    #[test]
    fn malformed_and_incompatible_payloads_leave_live_bindings_untouched() {
        let mut m = map();
        let bytes = m.encode_config(ControlSettings::default()).unwrap();
        for len in 0..bytes.len() {
            assert!(decode_config(&bytes[..len]).is_err());
        }
        let mut corrupt = bytes.clone();
        corrupt[10] ^= 1;
        assert_eq!(m.apply_config(&corrupt), Err(InputError::ConfigChecksum));
        let mut version = bytes.clone();
        version[4] = 2;
        resign(&mut version);
        assert_eq!(m.apply_config(&version), Err(InputError::ConfigVersion));
        let mut settings = bytes.clone();
        settings[8..12].copy_from_slice(&f32::NAN.to_le_bytes());
        resign(&mut settings);
        assert!(m.apply_config(&settings).is_err());
        let mut other = ActionMap::default();
        other
            .named_action(
                "jump",
                ActionKind::Delta,
                ContextId::GLOBAL,
                vec![Binding::digital(InputCode::MouseDelta(0))],
            )
            .unwrap();
        assert!(
            m.apply_config(&other.encode_config(ControlSettings::default()).unwrap())
                .is_err()
        );
        assert_eq!(m.encode_config(ControlSettings::default()).unwrap(), bytes);
    }
    #[test]
    fn unknown_actions_are_reported_without_discarding_settings() {
        let bytes = map().encode_config(ControlSettings::default()).unwrap();
        let mut empty = ActionMap::default();
        let result = empty.apply_config(&bytes).unwrap();
        assert_eq!((result.matched, result.unknown), (0, 2));
    }
    #[test]
    fn conflicts_follow_exclusive_passthrough_and_global_context_activity() {
        let mut map = ActionMap::default();
        let game = map.create_context("game").unwrap();
        let menu = map.create_context("menu").unwrap();
        let key = Binding::digital(InputCode::Key(32));
        let jump = map
            .named_action("jump", ActionKind::Button, game, vec![key])
            .unwrap();
        let accept = map
            .named_action("accept", ActionKind::Button, menu, vec![key])
            .unwrap();
        let bytes = map.encode_config(ControlSettings::default()).unwrap();
        assert!(map.validate_config(&bytes).unwrap().conflicts.is_empty());
        map.push_context(game, ContextMode::Exclusive).unwrap();
        assert!(map.validate_config(&bytes).unwrap().conflicts.is_empty());
        map.push_context(menu, ContextMode::Exclusive).unwrap();
        assert!(map.validate_config(&bytes).unwrap().conflicts.is_empty());
        map.apply_config_checked(&bytes).unwrap();
        map.pop_context();
        map.push_context(menu, ContextMode::Passthrough).unwrap();
        map.feed(InputCode::Key(32), 1.);
        let before = live_state(&map);
        let report = map.validate_config(&bytes).unwrap();
        assert_eq!(live_state(&map), before);
        assert_eq!(report.conflicts.len(), 1);
        assert!(!report.conflicts_truncated);
        assert_eq!(
            report.conflicts[0],
            BindingConflict {
                code: InputCode::Key(32),
                first: ConfigBinding {
                    action_id: jump,
                    action_name: Some("jump".into()),
                    action_kind: ActionKind::Button,
                    context: game,
                    context_name: "game".into(),
                    binding_index: 0,
                },
                second: ConfigBinding {
                    action_id: accept,
                    action_name: Some("accept".into()),
                    action_kind: ActionKind::Button,
                    context: menu,
                    context_name: "menu".into(),
                    binding_index: 0,
                },
            }
        );
        assert_eq!(
            map.apply_config_checked(&bytes),
            Err(ConfigError::Conflicts(Box::new(report)))
        );
        assert_eq!(live_state(&map), before);

        // Held-button suppression does not hide a mapping collision.
        map.suppress_held();
        assert_eq!(map.validate_config(&bytes).unwrap().conflicts.len(), 1);
        let global = map
            .named_action("global", ActionKind::Button, ContextId::GLOBAL, vec![key])
            .unwrap();
        map.set_context(Some(menu)).unwrap();
        let report = map.validate_config(&bytes).unwrap();
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].first.action_id, accept);
        assert_eq!(report.conflicts[0].second.action_id, global);
        assert_eq!(report.conflicts[0].second.context_name, "global");
        map.clear_contexts();
        assert!(map.validate_config(&bytes).unwrap().conflicts.is_empty());
    }

    #[test]
    fn preview_merges_defaults_and_unnamed_actions_but_ignores_retired_names() {
        let mut saved = ActionMap::default();
        for name in ["jump", "retired"] {
            saved
                .named_action(
                    name,
                    ActionKind::Button,
                    ContextId::GLOBAL,
                    vec![Binding::digital(InputCode::Key(32))],
                )
                .unwrap();
        }
        let settings = ControlSettings {
            invert_y: true,
            ..Default::default()
        };
        let bytes = saved.encode_config(settings).unwrap();
        let mut target = map();
        let jump = target.action_id("jump").unwrap();
        target
            .try_rebind(jump, vec![Binding::digital(InputCode::Key(65))])
            .unwrap();
        let added = target
            .named_action(
                "new_action",
                ActionKind::Button,
                ContextId::GLOBAL,
                vec![Binding::digital(InputCode::Key(32))],
            )
            .unwrap();
        let unnamed = target.action(vec![Binding::digital(InputCode::Key(32))]);
        target.feed(InputCode::Key(65), 1.);
        target.feed(InputCode::Key(65), 0.);
        target.feed_delta(InputCode::MouseDelta(0), 5.);
        let before = live_state(&target);
        let report = target.validate_config(&bytes).unwrap();
        assert_eq!(
            report.applied,
            AppliedConfig {
                settings,
                matched: 1,
                unknown: 1,
            }
        );
        assert_eq!(report.unknown_actions, ["retired"]);
        let pairs: Vec<_> = report
            .conflicts
            .iter()
            .map(|conflict| (conflict.first.action_id, conflict.second.action_id))
            .collect();
        assert_eq!(pairs, [(jump, added), (jump, unnamed), (added, unnamed)]);
        assert_eq!(report.conflicts[1].second.action_name, None);
        assert_eq!(live_state(&target), before);
        assert_eq!(
            target.apply_config_checked(&bytes),
            Err(ConfigError::Conflicts(Box::new(report.clone())))
        );
        assert_eq!(live_state(&target), before);
        // Existing callers can intentionally accept the reported sharing.
        assert_eq!(target.apply_config(&bytes).unwrap(), report.applied);
        assert_eq!(target.bindings(jump).unwrap()[0].code, InputCode::Key(32));
        assert_eq!(target.peek(target.action_id("look_x").unwrap()).value, 5.);
    }

    #[test]
    fn late_incompatible_inactive_action_rejects_all_changes_with_location() {
        let mut source = ActionMap::default();
        source
            .named_action(
                "jump",
                ActionKind::Button,
                ContextId::GLOBAL,
                vec![Binding::digital(InputCode::Key(82))],
            )
            .unwrap();
        source
            .named_action(
                "look_x",
                ActionKind::Axis,
                ContextId::GLOBAL,
                vec![Binding::digital(InputCode::Key(65))],
            )
            .unwrap();
        let bytes = source.encode_config(ControlSettings::default()).unwrap();
        let mut target = ActionMap::default();
        let inactive = target.create_context("inactive").unwrap();
        let look = target
            .named_action(
                "look_x",
                ActionKind::Delta,
                inactive,
                vec![Binding::digital(InputCode::MouseDelta(0))],
            )
            .unwrap();
        target
            .named_action(
                "jump",
                ActionKind::Button,
                ContextId::GLOBAL,
                vec![Binding::digital(InputCode::Key(32))],
            )
            .unwrap();
        target.feed(InputCode::Key(32), 1.);
        target.feed(InputCode::Key(32), 0.);
        let before = live_state(&target);
        let error = ConfigError::IncompatibleBinding(ConfigBinding {
            action_id: look,
            action_name: Some("look_x".into()),
            action_kind: ActionKind::Delta,
            context: inactive,
            context_name: "inactive".into(),
            binding_index: 0,
        });
        assert_eq!(target.validate_config(&bytes), Err(error.clone()));
        assert_eq!(target.apply_config_checked(&bytes), Err(error.clone()));
        assert_eq!(target.apply_config(&bytes), Err(InputError::InvalidBinding));
        assert_eq!(live_state(&target), before);
        assert!(error.to_string().contains("look_x"));
        assert!(error.to_string().contains("inactive"));
    }

    #[test]
    fn malformed_previews_and_imports_preserve_pending_input() {
        let mut target = map();
        target.feed(InputCode::Key(32), 1.);
        target.feed_delta(InputCode::MouseDelta(0), 7.);
        let before = live_state(&target);
        let bytes = target.encode_config(ControlSettings::default()).unwrap();
        let mut checksum = bytes.clone();
        checksum[10] ^= 1;
        let mut version = bytes.clone();
        version[4] = 2;
        resign(&mut version);
        let mut nonfinite = bytes;
        nonfinite[8..12].copy_from_slice(&f32::NAN.to_le_bytes());
        resign(&mut nonfinite);
        for (bytes, reason) in [
            (Vec::new(), InputError::InvalidConfig),
            (checksum, InputError::ConfigChecksum),
            (version, InputError::ConfigVersion),
            (nonfinite, InputError::InvalidConfig),
            (vec![0; MAX_CONFIG_BYTES + 1], InputError::InvalidConfig),
        ] {
            assert_eq!(
                target.validate_config(&bytes),
                Err(ConfigError::Input(reason))
            );
            assert_eq!(
                target.apply_config_checked(&bytes),
                Err(ConfigError::Input(reason))
            );
            assert_eq!(target.apply_config(&bytes), Err(reason));
            assert_eq!(live_state(&target), before);
        }
        assert!(
            std::error::Error::source(&ConfigError::Input(InputError::InvalidConfig)).is_some()
        );
    }

    #[test]
    fn checked_import_swaps_sources_and_resets_only_matched_pending_input() {
        let mut target = map();
        let jump = target.action_id("jump").unwrap();
        let look = target.action_id("look_x").unwrap();
        let fire = target
            .named_action(
                "fire",
                ActionKind::Button,
                ContextId::GLOBAL,
                vec![Binding::digital(InputCode::Key(65))],
            )
            .unwrap();
        let retained = target
            .named_action(
                "retained",
                ActionKind::Delta,
                ContextId::GLOBAL,
                vec![Binding::digital(InputCode::TouchDelta(1))],
            )
            .unwrap();
        let unnamed = target.action(vec![Binding::digital(InputCode::Key(99))]);
        target.feed(InputCode::Key(32), 1.);
        target.feed(InputCode::Key(65), 1.);
        target.feed(InputCode::Key(99), 1.);
        target.feed(InputCode::Key(99), 0.);
        target.feed_delta(InputCode::MouseDelta(0), 4.);
        target.feed_delta(InputCode::TouchDelta(1), 9.);
        let retained_state = target.peek(retained);
        let unnamed_state = target.peek(unnamed);

        // Each replacement conflicts with an old binding, but the complete
        // proposed swap does not. Registration order differs from the target.
        let mut source = ActionMap::default();
        for (name, code) in [("fire", 32), ("jump", 65)] {
            source
                .named_action(
                    name,
                    ActionKind::Button,
                    ContextId::GLOBAL,
                    vec![Binding::digital(InputCode::Key(code))],
                )
                .unwrap();
        }
        source
            .named_action(
                "look_x",
                ActionKind::Delta,
                ContextId::GLOBAL,
                vec![Binding::digital(InputCode::MouseDelta(1))],
            )
            .unwrap();
        let bytes = source.encode_config(ControlSettings::default()).unwrap();
        let preview = target.validate_config(&bytes).unwrap();
        assert!(preview.conflicts.is_empty());
        assert_eq!(
            target.apply_config_checked(&bytes).unwrap(),
            preview.applied
        );
        assert_eq!(target.peek(jump), Action::default());
        assert_eq!(target.peek(fire), Action::default());
        assert_eq!(target.peek(look), Action::default());
        assert_eq!(target.peek(retained), retained_state);
        assert_eq!(target.peek(unnamed), unnamed_state);
        target.feed(InputCode::Key(65), 0.);
        target.feed(InputCode::Key(65), 1.);
        assert!(target.consume_tick(jump).pressed);
        assert!(!target.consume_tick(fire).pressed);
        target.feed_delta(InputCode::MouseDelta(0), 3.);
        assert_eq!(target.peek(look), Action::default());
        target.feed_delta(InputCode::MouseDelta(1), 2.);
        assert_eq!(target.consume_tick(look).value, 2.);
    }

    #[test]
    fn duplicate_sources_include_signed_scales_but_ignore_zero_scales_and_other_devices() {
        let mut map = ActionMap::default();
        map.action(vec![
            Binding::digital(InputCode::Key(1)),
            Binding::digital(InputCode::Mouse(1)),
            Binding {
                code: InputCode::Key(1),
                scale: -2.,
                deadzone: 0.5,
            },
            Binding {
                code: InputCode::Key(1),
                scale: -0.,
                deadzone: 0.,
            },
        ]);
        let report = map
            .validate_config(&map.encode_config(ControlSettings::default()).unwrap())
            .unwrap();
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].code, InputCode::Key(1));
        assert_eq!(report.conflicts[0].first.action_id, 0);
        assert_eq!(report.conflicts[0].second.action_id, 0);
        assert_eq!(report.conflicts[0].first.binding_index, 0);
        assert_eq!(report.conflicts[0].second.binding_index, 2);
        assert_eq!(report.conflicts[0].first.action_name, None);
    }

    #[test]
    fn conflict_cap_distinguishes_exactly_full_and_truncated_reports() {
        let mut map = ActionMap::default();
        // C(16,2) + C(16,2) + C(6,2) + C(2,2) = 256 pairs.
        let mut bindings = Vec::new();
        for (code, count) in [(1, 16), (2, 16), (3, 6), (4, 2)] {
            bindings.extend(vec![Binding::digital(InputCode::Key(code)); count]);
        }
        let id = map
            .named_action(
                "many",
                ActionKind::Axis,
                ContextId::GLOBAL,
                bindings.clone(),
            )
            .unwrap();
        let bytes = map.encode_config(ControlSettings::default()).unwrap();
        let full = map.validate_config(&bytes).unwrap();
        assert_eq!(full.conflicts.len(), MAX_CONFIG_CONFLICTS);
        assert!(!full.conflicts_truncated);
        assert_eq!(map.validate_config(&bytes).unwrap(), full);
        bindings.push(Binding::digital(InputCode::Key(4)));
        map.try_rebind(id, bindings).unwrap();
        let bytes = map.encode_config(ControlSettings::default()).unwrap();
        let report = map.validate_config(&bytes).unwrap();
        assert_eq!(report.conflicts.len(), MAX_CONFIG_CONFLICTS);
        assert!(report.conflicts_truncated);
        assert_eq!(report.conflicts, full.conflicts);
        let before = live_state(&map);
        assert_eq!(
            map.apply_config_checked(&bytes),
            Err(ConfigError::Conflicts(Box::new(report)))
        );
        assert_eq!(live_state(&map), before);
    }
    #[test]
    fn radial_deadzones_curves_and_nonfinite_sticks_are_bounded() {
        let s = ControlSettings {
            pad_curve: 2.,
            ..Default::default()
        };
        assert_eq!(s.stick(Vec2::splat(0.1)), Vec2::ZERO);
        let d = s.stick(Vec2::ONE);
        assert!((d.length() - 1.).abs() < 1e-5);
        assert!((d.x - d.y).abs() < 1e-6);
        assert!(s.stick(Vec2::X * 0.5).x < 0.5);
        assert_eq!(s.stick(Vec2::new(f32::NAN, 0.)), Vec2::ZERO);
    }
}
