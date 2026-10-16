use super::*;
use ferravex_play::input::touch;
pub const MAX_EVENTS: usize = 256;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PadState {
    pub connected: bool,
    pub epoch: u32,
    pub axes: [f32; 16],
    pub buttons: [f32; 32],
}
impl Default for PadState {
    fn default() -> Self {
        Self {
            connected: false,
            epoch: 0,
            axes: [0.; 16],
            buttons: [0.; 32],
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PlatformEvent {
    None,
    Value(InputCode, f32),
    MouseMotion(Vec2),
    PointerPosition(Vec2),
    Touch(touch::Event),
    Pad {
        index: u8,
        code: InputCode,
        value: f32,
    },
}
pub(crate) struct PlatformInput {
    events: [PlatformEvent; MAX_EVENTS],
    len: usize,
    pub overflow: bool,
    pub focus_epoch: u64,
    pub focused: bool,
    pub pads: [PadState; 4],
    pub settings_requested: bool,
}
impl Default for PlatformInput {
    fn default() -> Self {
        Self {
            events: [PlatformEvent::None; MAX_EVENTS],
            len: 0,
            overflow: false,
            focus_epoch: 0,
            focused: true,
            pads: [PadState::default(); 4],
            settings_requested: false,
        }
    }
}
impl PlatformInput {
    pub fn events(&self) -> &[PlatformEvent] {
        &self.events[..self.len]
    }
    /// Overflow discards the complete packet and requests neutralization. Hosts
    /// must clear their legacy held sets too when this returns false.
    pub fn push(&mut self, event: PlatformEvent) -> bool {
        if !self.focused || self.overflow {
            return false;
        }
        if self.len == MAX_EVENTS {
            self.len = 0;
            self.overflow = true;
            self.focus_epoch = self.focus_epoch.wrapping_add(1);
            return false;
        }
        self.events[self.len] = event;
        self.len += 1;
        true
    }
    pub fn focus(&mut self, focused: bool) {
        if self.focused == focused {
            return;
        }
        self.focused = focused;
        self.len = 0;
        self.focus_epoch = self.focus_epoch.wrapping_add(1);
        self.overflow = false;
    }
    pub fn finish_frame(&mut self) {
        self.len = 0;
        self.overflow = false;
        self.settings_requested = false;
    }
    pub fn pad_connected(&mut self, index: usize, connected: bool) {
        let Some(pad) = self.pads.get_mut(index) else {
            return;
        };
        if pad.connected == connected {
            return;
        }
        *pad = PadState {
            connected,
            epoch: pad.epoch.wrapping_add(1),
            ..Default::default()
        };
    }
    pub fn pad_axis(&mut self, index: usize, axis: usize, value: f32) {
        let Some(pad) = self.pads.get_mut(index) else {
            return;
        };
        if !pad.connected || axis >= pad.axes.len() {
            return;
        }
        let value = if value.is_finite() {
            value.clamp(-1., 1.)
        } else {
            0.
        };
        if pad.axes[axis] == value {
            return;
        }
        pad.axes[axis] = value;
        self.push(PlatformEvent::Pad {
            index: index as u8,
            code: InputCode::PadAxis(axis as u8),
            value,
        });
    }
    pub fn pad_button(&mut self, index: usize, button: usize, value: f32) {
        let Some(pad) = self.pads.get_mut(index) else {
            return;
        };
        if !pad.connected || button >= pad.buttons.len() {
            return;
        }
        let value = if value.is_finite() {
            value.clamp(0., 1.)
        } else {
            0.
        };
        if pad.buttons[button] == value {
            return;
        }
        pad.buttons[button] = value;
        self.push(PlatformEvent::Pad {
            index: index as u8,
            code: InputCode::PadButton(button as u8),
            value,
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn journal_preserves_taps_and_fails_neutral_on_overflow() {
        let mut p = PlatformInput::default();
        assert!(p.push(PlatformEvent::Value(InputCode::Key(32), 1.)));
        assert!(p.push(PlatformEvent::Value(InputCode::Key(32), 0.)));
        assert_eq!(p.events().len(), 2);
        for _ in 2..MAX_EVENTS {
            assert!(p.push(PlatformEvent::MouseMotion(Vec2::X)));
        }
        assert!(!p.push(PlatformEvent::MouseMotion(Vec2::X)));
        assert!(p.overflow);
        assert!(p.events().is_empty());
        assert_eq!(p.focus_epoch, 1);
        p.finish_frame();
        assert!(p.push(PlatformEvent::Value(InputCode::Key(32), 0.)));
        p.focus(false);
        assert!(p.events().is_empty());
        assert!(!p.push(PlatformEvent::Value(InputCode::Key(32), 1.)));
        p.focus(true);
        assert!(p.push(PlatformEvent::Value(InputCode::Key(32), 1.)));
    }
    #[test]
    fn gamepad_bounds_disconnects_and_invalid_values_are_neutral() {
        let mut p = PlatformInput::default();
        p.pad_connected(0, true);
        p.pad_axis(0, 0, 2.);
        p.pad_button(0, 0, 1.);
        assert_eq!(p.pads[0].axes[0], 1.);
        assert_eq!(p.events().len(), 2);
        p.pad_axis(99, 0, 1.);
        p.pad_axis(0, 99, 1.);
        p.pad_button(0, 99, 1.);
        p.pad_axis(0, 1, f32::NAN);
        assert_eq!(p.events().len(), 2);
        p.pad_connected(0, false);
        assert_eq!(p.pads[0].axes, [0.; 16]);
        assert_eq!(p.pads[0].buttons, [0.; 32]);
        assert_eq!(p.pads[0].epoch, 2);
    }
}
