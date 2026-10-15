use super::{
    platform::{MAX_EVENTS, PlatformEvent},
    storage, *,
};
use ferravex_play::input::touch::{self, TouchSurface};
mod editor;
#[cfg(test)]
mod tests;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlsStatus {
    Defaults,
    Transient,
    Loaded { unknown: usize },
    Saved,
    Invalid(InputError),
    Storage(StorageError),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureDevice {
    KeyboardMouse,
    Gamepad,
}
struct ActionInfo {
    id: usize,
    name: String,
    label: String,
    defaults: Box<[Binding]>,
    global: bool,
    kind: ActionKind,
}
#[derive(Clone, Copy)]
struct Capture {
    action: usize,
    device: CaptureDevice,
}
pub struct PlayerInput {
    owner: String,
    map: ActionMap,
    gameplay: ContextId,
    actions: Vec<ActionInfo>,
    settings: ControlSettings,
    loaded: bool,
    persistent: bool,
    surface: TouchSurface,
    last_frame: Option<u64>,
    focus_epoch: u64,
    focused: bool,
    enabled: bool,
    touch_seen: bool,
    mouse_look_button: Option<u8>,
    selected_pad: usize,
    pad_epoch: u32,
    pad_axes: [f32; 16],
    pad_axes_suppressed: u16,
    pad_buttons_suppressed: u32,
    open: bool,
    paused: bool,
    prior_cursor: (bool, bool),
    capture: Option<Capture>,
    status: ControlsStatus,
    pointer: Vec2,
    cursor: Option<Vec2>,
    cursor_physical: Vec2,
    cursor_virtual: bool,
    ui_click: bool,
    ui_taps: [Vec2; 16],
    ui_tap_len: usize,
    ui_block: bool,
    devices_tab: bool,
    page: usize,
    ui_focus: usize,
    ui_up: bool,
    ui_down: bool,
    ui_accept: bool,
}
impl PlayerInput {
    /// Build a named profile, register actions, then sample once per render frame.
    /// Configuration is loaded lazily after registration on the first sample.
    pub fn new(owner: &str) -> Self {
        assert!(
            !owner.is_empty()
                && owner.len() <= 64
                && owner
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_".contains(&b)),
            "valid controls owner"
        );
        let mut map = ActionMap::default();
        let gameplay = map.create_context("gameplay").unwrap();
        map.set_context(Some(gameplay)).unwrap();
        crate::edit(|s| s.input_enabled = true);
        Self {
            owner: owner.into(),
            map,
            gameplay,
            actions: Vec::new(),
            settings: ControlSettings::default(),
            loaded: false,
            persistent: true,
            surface: TouchSurface::default(),
            last_frame: None,
            focus_epoch: 0,
            focused: true,
            enabled: true,
            touch_seen: false,
            mouse_look_button: None,
            selected_pad: usize::MAX,
            pad_epoch: 0,
            pad_axes: [0.; 16],
            pad_axes_suppressed: 0,
            pad_buttons_suppressed: 0,
            open: false,
            paused: false,
            prior_cursor: (false, true),
            capture: None,
            status: ControlsStatus::Defaults,
            pointer: Vec2::ZERO,
            cursor: None,
            cursor_physical: Vec2::ZERO,
            cursor_virtual: false,
            ui_click: false,
            ui_taps: [Vec2::ZERO; 16],
            ui_tap_len: 0,
            ui_block: false,
            devices_tab: false,
            page: 0,
            ui_focus: 0,
            ui_up: false,
            ui_down: false,
            ui_accept: false,
        }
    }
    /// Session-only profile for tests, tools and hosts that own their storage.
    pub fn transient(owner: &str) -> Self {
        let mut input = Self::new(owner);
        input.persistent = false;
        input.loaded = true;
        input.status = ControlsStatus::Transient;
        input
    }
    pub fn action(
        &mut self,
        name: &str,
        label: &str,
        kind: ActionKind,
        bindings: Vec<Binding>,
    ) -> Result<usize, InputError> {
        self.register(name, label, kind, bindings, false)
    }
    pub fn global_action(
        &mut self,
        name: &str,
        label: &str,
        kind: ActionKind,
        bindings: Vec<Binding>,
    ) -> Result<usize, InputError> {
        self.register(name, label, kind, bindings, true)
    }
    fn register(
        &mut self,
        name: &str,
        label: &str,
        kind: ActionKind,
        bindings: Vec<Binding>,
        global: bool,
    ) -> Result<usize, InputError> {
        if bindings.iter().any(|b| reserved(b.code)) {
            return Err(InputError::InvalidBinding);
        }
        if label.len() > 96 {
            return Err(InputError::InvalidName);
        }
        let id = self.map.named_action(
            name,
            kind,
            if global {
                ContextId::GLOBAL
            } else {
                self.gameplay
            },
            bindings.clone(),
        )?;
        self.actions.push(ActionInfo {
            id,
            name: name.into(),
            label: label.into(),
            defaults: bindings.into_boxed_slice(),
            global,
            kind,
        });
        Ok(id)
    }
    /// Restrict relative mouse look to dragging a button, leaving touch/stick look
    /// independent. None is suitable for pointer-locked first-person games.
    pub fn set_mouse_look_button(&mut self, button: Option<u8>) -> Result<(), InputError> {
        if button.is_some_and(|b| !(1..=5).contains(&b)) {
            return Err(InputError::InvalidBinding);
        }
        self.mouse_look_button = button;
        Ok(())
    }
    pub fn bindings(&self, name: &str) -> Option<&[Binding]> {
        self.map
            .action_id(name)
            .and_then(|id| self.map.bindings(id))
    }
    /// Extend an authored profile before its first sample. The binding also
    /// becomes part of Reset defaults; saved mappings load after registration.
    pub fn add_default_binding(&mut self, name: &str, binding: Binding) -> Result<(), InputError> {
        let index = self
            .actions
            .iter()
            .position(|a| a.name == name)
            .ok_or(InputError::InvalidHandle)?;
        let mut bindings = self.actions[index].defaults.to_vec();
        bindings.push(binding);
        self.set_default_bindings(name, bindings)
    }
    /// Replace authored defaults before the first sample (for a custom profile).
    pub fn set_default_bindings(
        &mut self,
        name: &str,
        bindings: Vec<Binding>,
    ) -> Result<(), InputError> {
        let index = self
            .actions
            .iter()
            .position(|a| a.name == name)
            .ok_or(InputError::InvalidHandle)?;
        self.rebind(name, bindings.clone())?;
        self.actions[index].defaults = bindings.into_boxed_slice();
        Ok(())
    }
    pub fn settings(&self) -> ControlSettings {
        self.settings
    }
    pub fn set_settings(&mut self, settings: ControlSettings) -> Result<(), InputError> {
        if !settings.valid() {
            return Err(InputError::InvalidConfig);
        }
        self.settings = settings;
        Ok(())
    }
    pub fn status(&self) -> ControlsStatus {
        self.status
    }
    /// Single-player convenience: a global `pause` action toggles simulation.
    /// Network clients should call sample directly and continue their clocks.
    pub fn sample_play(&mut self) {
        self.sample(!self.paused);
        if self.pressed("pause") {
            self.paused = !self.paused;
            self.sample(!self.paused);
        }
    }
    pub fn set_paused(&mut self, paused: bool) {
        if self.paused != paused {
            self.paused = paused;
            self.map.suppress_held();
            self.surface.clear(&mut self.map);
        }
    }
    pub fn paused(&self) -> bool {
        self.paused
    }
    pub fn controls_open(&self) -> bool {
        self.open
    }
    pub fn gameplay_active(&self) -> bool {
        self.enabled && self.focused && !self.open
    }
    pub fn touch_active(&self) -> bool {
        self.surface.active() != 0
    }
    pub fn touch_seen(&self) -> bool {
        self.touch_seen
    }
    pub fn simulation_dt(&self, dt: f32) -> f32 {
        if self.gameplay_active() && dt.is_finite() {
            dt.clamp(0., 0.1)
        } else {
            0.
        }
    }
    pub fn set_touch_layout(&mut self, layout: &[touch::Control]) -> Result<(), InputError> {
        self.surface.set_layout(layout, &mut self.map)
    }
    pub fn touch_stick(&self, x: u8, y: u8) -> Option<(Vec2, Vec2)> {
        self.surface.stick(x, y)
    }
    pub fn value(&self, name: &str) -> f32 {
        if self.open || !self.focused {
            0.
        } else {
            self.map
                .action_id(name)
                .map_or(0., |id| self.map.peek(id).value)
        }
    }
    pub fn held(&self, name: &str) -> bool {
        self.value(name).abs() > 0.001
    }
    /// Digital edges remain pending across zero-tick render frames until taken.
    pub fn take(&mut self, name: &str) -> Action {
        if self.open || !self.focused {
            return Action::default();
        }
        self.map
            .action_id(name)
            .map_or(Action::default(), |id| self.map.consume_tick(id))
    }
    pub fn pressed(&mut self, name: &str) -> bool {
        self.take(name).pressed
    }
    /// Mouse/touch position with a movement-stick cursor for pointing games.
    /// Real pointer movement takes over immediately; no synthetic mouse action
    /// is generated. Call once per render frame using logical pixels/second.
    pub fn cursor(&mut self, dt: f32, speed: f32) -> Vec2 {
        let size = vec2(screen_width(), screen_height());
        let physical = self.pointer;
        let mut cursor = self.cursor.unwrap_or_else(|| {
            if physical == Vec2::ZERO {
                size * 0.5
            } else {
                physical
            }
        });
        if physical != self.cursor_physical {
            cursor = physical;
            self.cursor_virtual = false;
        }
        self.cursor_physical = physical;
        let movement = self.movement();
        if dt.is_finite() && speed.is_finite() && speed >= 0. {
            if movement.length_squared() > 0.001 && dt > 0. && speed > 0. {
                self.cursor_virtual = true;
            }
            cursor += vec2(movement.x, -movement.y).clamp_length_max(1.)
                * dt.clamp(0., 0.1)
                * speed.min(10000.);
        }
        cursor = cursor.clamp(Vec2::ZERO, size);
        self.cursor = Some(cursor);
        cursor
    }
    pub fn cursor_is_virtual(&self) -> bool {
        self.cursor_virtual
    }
    pub fn movement(&self) -> Vec2 {
        vec2(
            self.value("move_x") + self.value("move_right") - self.value("move_left"),
            -self.value("move_y") + self.value("move_forward") - self.value("move_back"),
        )
        .clamp(Vec2::splat(-1.), Vec2::ONE)
    }
    /// Local right/down look radians. Call once per render frame before building
    /// fixed-tick commands; the simulation receives the resulting absolute aim.
    pub fn look(&mut self, dt: f32) -> Vec2 {
        let dt = if dt.is_finite() {
            dt.clamp(0., 0.25)
        } else {
            0.
        };
        let mut value = vec2(
            self.take("look_x").value + self.value("look_rate_x") * self.settings.pad_look.x * dt,
            self.take("look_y").value + self.value("look_rate_y") * self.settings.pad_look.y * dt,
        );
        if self.settings.invert_y {
            value.y = -value.y;
        }
        value
    }
    pub fn rebind(&mut self, name: &str, bindings: Vec<Binding>) -> Result<(), InputError> {
        let id = self.map.action_id(name).ok_or(InputError::InvalidHandle)?;
        if bindings.iter().any(|b| reserved(b.code)) {
            return Err(InputError::InvalidBinding);
        }
        self.map.try_rebind(id, bindings)
    }
    pub fn export_config(&self) -> Result<Vec<u8>, InputError> {
        self.map.encode_config(self.settings)
    }
    pub fn import_config(&mut self, bytes: &[u8]) -> Result<AppliedConfig, InputError> {
        // Reserved host controls must remain reachable after importing a profile.
        let config = decode_config(bytes)?;
        if config
            .actions
            .iter()
            .any(|a| a.bindings.iter().any(|b| reserved(b.code)))
        {
            return Err(InputError::InvalidBinding);
        }
        let applied = self.map.apply_config(bytes)?;
        self.settings = applied.settings;
        self.map.suppress_held();
        self.status = ControlsStatus::Loaded {
            unknown: applied.unknown,
        };
        Ok(applied)
    }
    pub fn save(&mut self) -> bool {
        if !self.persistent {
            self.status = ControlsStatus::Transient;
            return false;
        }
        match self.export_config() {
            Err(e) => {
                self.status = ControlsStatus::Invalid(e);
                false
            }
            Ok(bytes) => match storage::save(&self.owner, &bytes) {
                Ok(()) => {
                    self.status = ControlsStatus::Saved;
                    true
                }
                Err(e) => {
                    self.status = ControlsStatus::Storage(e);
                    false
                }
            },
        }
    }
    pub fn load(&mut self) {
        if !self.persistent {
            self.loaded = true;
            self.status = ControlsStatus::Transient;
            return;
        }
        self.loaded = true;
        match storage::load(&self.owner) {
            Ok(Some(bytes)) => {
                if let Err(e) = self.import_config(&bytes) {
                    self.status = ControlsStatus::Invalid(e);
                }
            }
            Ok(None) => self.status = ControlsStatus::Defaults,
            Err(e) => self.status = ControlsStatus::Storage(e),
        }
    }
    pub fn reset_defaults(&mut self) {
        for info in &self.actions {
            self.map
                .try_rebind(info.id, info.defaults.to_vec())
                .expect("validated defaults");
        }
        self.settings = ControlSettings::default();
        self.map.suppress_held();
        self.save();
    }
    pub fn set_controls_open(&mut self, open: bool) {
        if self.open == open {
            return;
        }
        self.open = open;
        self.capture = None;
        self.map.suppress_held();
        self.surface.clear(&mut self.map);
        if open {
            self.prior_cursor = crate::read(|s| (s.grab, s.visible));
            crate::set_cursor_grab(false);
            crate::show_mouse(true);
        } else {
            crate::set_cursor_grab(self.prior_cursor.0);
            crate::show_mouse(self.prior_cursor.1);
        }
        crate::edit(|s| s.controls_open = open);
    }
    pub fn begin_capture(&mut self, name: &str, device: CaptureDevice) -> Result<(), InputError> {
        let action = self
            .actions
            .iter()
            .position(|a| a.name == name)
            .ok_or(InputError::InvalidHandle)?;
        if self.actions[action].kind == ActionKind::Delta && device == CaptureDevice::Gamepad {
            return Err(InputError::InvalidBinding);
        }
        self.capture = Some(Capture { action, device });
        Ok(())
    }
    fn capture_value(&mut self, code: InputCode, value: f32) -> bool {
        let Some(capture) = self.capture else {
            return false;
        };
        let info = &self.actions[capture.action];
        let device = match code {
            InputCode::PadAxis(_) | InputCode::PadButton(_) => CaptureDevice::Gamepad,
            InputCode::Touch(_) | InputCode::TouchDelta(_) => return false,
            _ => CaptureDevice::KeyboardMouse,
        };
        if device != capture.device || code.is_delta() != (info.kind == ActionKind::Delta) {
            return false;
        }
        if reserved(code) || code == InputCode::Key(0xff1b) {
            return false;
        }
        if code.is_delta() {
            if value.abs() < 0.0001 || self.map.physical_value(InputCode::Mouse(1)) != 0. {
                return false;
            }
        } else if value.abs() < 0.65 || self.map.physical_value(code).abs() >= 0.5 {
            return false;
        }
        let mut bindings: Vec<_> = self
            .map
            .bindings(info.id)
            .unwrap()
            .iter()
            .copied()
            .filter(|b| match capture.device {
                CaptureDevice::Gamepad => {
                    !matches!(b.code, InputCode::PadAxis(_) | InputCode::PadButton(_))
                }
                CaptureDevice::KeyboardMouse => !matches!(
                    b.code,
                    InputCode::Key(_)
                        | InputCode::Mouse(_)
                        | InputCode::MouseDelta(_)
                        | InputCode::Wheel(_)
                ),
            })
            .collect();
        bindings.push(Binding {
            code,
            scale: value.signum(),
            deadzone: 0.,
        });
        match self.map.try_rebind(info.id, bindings) {
            Ok(()) => {
                self.capture = None;
                self.ui_block = true;
                true
            }
            Err(e) => {
                self.status = ControlsStatus::Invalid(e);
                false
            }
        }
    }
    fn axis_value(&mut self, axis: usize, value: f32) {
        if self.pad_axes_suppressed & (1 << axis) != 0 {
            if value.abs() <= 0.25 {
                self.pad_axes_suppressed &= !(1 << axis);
            } else {
                self.pad_axes[axis] = 0.;
                return;
            }
        }
        self.pad_axes[axis] = value;
    }
    fn feed_axes(&mut self) {
        let left = self
            .settings
            .stick(vec2(self.pad_axes[0], self.pad_axes[1]));
        let right = self
            .settings
            .stick(vec2(self.pad_axes[2], self.pad_axes[3]));
        for (axis, value) in [left.x, left.y, right.x, right.y].into_iter().enumerate() {
            self.map.feed(InputCode::PadAxis(axis as u8), value);
        }
        for axis in 4..16 {
            self.map
                .feed(InputCode::PadAxis(axis as u8), self.pad_axes[axis]);
        }
    }
    pub fn sample(&mut self, enabled: bool) {
        crate::edit(|s| s.input_enabled = true);
        if !self.loaded {
            self.load();
        }
        self.enabled = enabled;
        let mut events = [PlatformEvent::None; MAX_EVENTS];
        let (frame, epoch, focused, overflow, pads, requested, size, pointer) = crate::read(|s| {
            let values = s.input.events();
            events[..values.len()].copy_from_slice(values);
            (
                s.frame,
                s.input.focus_epoch,
                s.input.focused,
                s.input.overflow,
                s.input.pads,
                s.input.settings_requested,
                vec2(s.w as f32, s.h as f32),
                s.mouse,
            )
        });
        if self.last_frame == Some(frame) {
            self.map
                .set_context(self.gameplay_active().then_some(self.gameplay))
                .unwrap();
            return;
        }
        self.last_frame = Some(frame);
        self.ui_click = false;
        self.ui_tap_len = 0;
        self.ui_block = false;
        self.ui_up = false;
        self.ui_down = false;
        self.ui_accept = false;
        self.pointer = pointer;
        // Global menu actions are render-frame edges. Gameplay edges remain
        // buffered until a fixed simulation tick explicitly consumes them.
        for info in &self.actions {
            if info.global {
                self.map.consume_tick(info.id);
            }
        }
        if epoch != self.focus_epoch || !focused || overflow {
            self.map.lose_focus();
            self.surface.clear(&mut self.map);
            self.pad_epoch = u32::MAX;
        }
        self.focus_epoch = epoch;
        self.focused = focused && !overflow;
        if requested {
            self.set_controls_open(!self.open);
        }
        self.map
            .set_context(self.gameplay_active().then_some(self.gameplay))
            .unwrap();
        let selected = self.settings.gamepad as usize;
        let pad = pads[selected];
        let fresh = selected != self.selected_pad || pad.epoch != self.pad_epoch;
        if fresh {
            self.selected_pad = selected;
            self.pad_epoch = pad.epoch;
            self.pad_axes = [0.; 16];
            self.pad_axes_suppressed = 0;
            self.pad_buttons_suppressed = 0;
            for (i, &v) in pad.axes.iter().enumerate() {
                if v.abs() > 0.25 {
                    self.pad_axes_suppressed |= 1 << i;
                }
                self.map.feed(InputCode::PadAxis(i as u8), 0.);
            }
            for (i, &v) in pad.buttons.iter().enumerate() {
                if v >= 0.5 {
                    self.pad_buttons_suppressed |= 1 << i;
                }
                self.map.feed(InputCode::PadButton(i as u8), 0.);
            }
        }
        let mut changed = false;
        for event in events {
            if event == PlatformEvent::None {
                break;
            }
            if !self.focused {
                continue;
            }
            match event {
                PlatformEvent::Value(InputCode::Key(key), value)
                    if value >= 0.5 && (key == 0xffbf || self.open && key == 0xff1b) =>
                {
                    if key == 0xff1b && self.capture.is_some() {
                        self.capture = None;
                        self.ui_block = true;
                    } else {
                        self.set_controls_open(!self.open);
                    }
                    self.map
                        .set_context(self.gameplay_active().then_some(self.gameplay))
                        .unwrap();
                }
                PlatformEvent::Value(code, value) => {
                    changed |= self.open && self.capture_value(code, value);
                    if self.open && self.capture.is_none() {
                        match code {
                            InputCode::Key(0xff52) if value > 0.5 => self.ui_up = true,
                            InputCode::Key(0xff54) if value > 0.5 => self.ui_down = true,
                            InputCode::Key(0xff09) if value > 0.5 => {
                                if self.map.physical_value(InputCode::Key(0xffe1)) > 0.5
                                    || self.map.physical_value(InputCode::Key(0xffe2)) > 0.5
                                {
                                    self.ui_up = true;
                                } else {
                                    self.ui_down = true;
                                }
                            }
                            InputCode::Key(0xff0d) if value > 0.5 => self.ui_accept = true,
                            _ => {}
                        }
                    }
                    if code == InputCode::Mouse(1) && value >= 0.5 {
                        self.ui_click = true;
                    }
                    self.map.feed(code, value);
                }
                PlatformEvent::MouseMotion(delta) => {
                    if !self.open
                        && self
                            .mouse_look_button
                            .is_some_and(|b| self.map.physical_value(InputCode::Mouse(b)) < 0.5)
                    {
                        continue;
                    }
                    let delta = delta / size * self.settings.mouse_sensitivity;
                    changed |= self.open
                        && (self.capture_value(InputCode::MouseDelta(0), delta.x)
                            || self.capture_value(InputCode::MouseDelta(1), delta.y));
                    self.map.feed_delta(InputCode::MouseDelta(0), delta.x);
                    self.map.feed_delta(InputCode::MouseDelta(1), delta.y);
                }
                PlatformEvent::Touch(event) => {
                    self.touch_seen = true;
                    if event.phase == touch::Phase::Start && self.ui_tap_len < self.ui_taps.len() {
                        self.ui_taps[self.ui_tap_len] = event.position;
                        self.ui_tap_len += 1;
                    }
                    if !self.open
                        && let Err(e) = self.surface.handle_scaled(
                            event,
                            &mut self.map,
                            Vec2::splat(self.settings.touch_sensitivity),
                        )
                    {
                        self.surface.clear(&mut self.map);
                        self.status = ControlsStatus::Invalid(e);
                    }
                }
                PlatformEvent::Pad { index, code, value }
                    if index as usize == selected && pad.connected && !fresh =>
                {
                    match code {
                        InputCode::PadAxis(axis) => {
                            self.axis_value(axis as usize, value);
                            changed |= self.open
                                && self.pad_axes_suppressed & (1 << axis) == 0
                                && self.capture_value(code, self.pad_axes[axis as usize]);
                            self.feed_axes();
                        }
                        InputCode::PadButton(button) => {
                            if value < 0.5 {
                                self.pad_buttons_suppressed &= !(1 << button);
                            }
                            if self.pad_buttons_suppressed & (1 << button) != 0 {
                                continue;
                            }
                            if button == 16
                                && value >= 0.5
                                && !fresh
                                && self.map.physical_value(code) < 0.5
                            {
                                self.set_controls_open(!self.open);
                                self.map
                                    .set_context(self.gameplay_active().then_some(self.gameplay))
                                    .unwrap();
                                continue;
                            }
                            changed |= self.open && self.capture_value(code, value);
                            if self.open
                                && self.capture.is_none()
                                && value > 0.5
                                && self.map.physical_value(code) < 0.5
                            {
                                match button {
                                    12 => self.ui_up = true,
                                    13 => self.ui_down = true,
                                    0 => self.ui_accept = true,
                                    _ => {}
                                }
                            }
                            self.map.feed(code, value);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        if self.focused && pad.connected {
            for (i, &v) in pad.axes.iter().enumerate() {
                self.axis_value(i, v);
            }
            self.feed_axes();
            for (i, &v) in pad.buttons.iter().enumerate() {
                if v < 0.5 {
                    self.pad_buttons_suppressed &= !(1 << i);
                }
                self.map.feed(
                    InputCode::PadButton(i as u8),
                    if self.pad_buttons_suppressed & (1 << i) != 0 {
                        0.
                    } else {
                        v
                    },
                );
            }
        }
        if changed {
            self.save();
        }
    }
}

fn reserved(code: InputCode) -> bool {
    matches!(code, InputCode::Key(0xffbf) | InputCode::PadButton(16))
}
