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
    pub fn apply_config(&mut self, bytes: &[u8]) -> Result<AppliedConfig, InputError> {
        let decoded = decode_config(bytes)?;
        for action in &decoded.actions {
            if let Some(id) = self.action_id(&action.name) {
                validate_bindings(self.entries[id].kind, &action.bindings)?;
            }
        }
        let mut matched = 0;
        let mut unknown = 0;
        for action in decoded.actions {
            if let Some(id) = self.action_id(&action.name) {
                self.try_rebind(id, action.bindings)
                    .expect("prevalidated bindings");
                matched += 1;
            } else {
                unknown += 1;
            }
        }
        Ok(AppliedConfig {
            settings: decoded.settings,
            matched,
            unknown,
        })
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
