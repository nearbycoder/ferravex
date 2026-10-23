//! Opt-in checked calibration for normalized embedding-host samples.
//!
//! This module has no device reader, hardware discovery, persistence or automatic
//! browser adoption. Linux polling and the existing browser ABI remain unchanged.
//! A custom host owns one [`CalibratedGamepad`] per slot and submits **raw**
//! [`PadState`] samples through it instead of also calling [`super::gamepad_sample`].
//! Never feed the output of [`CalibrationProfile::apply`] back into the adapter.
//! Buttons (including analog triggers and hats) are copied, never inferred from
//! calibrated axes. Unselected axes keep their channel meanings.
//!
//! Profiles have no hardware identity. Selection belongs to this adapter's current
//! connection only: disconnect or a changed host-supplied `PadState::epoch` clears
//! it. A host replacing a device without an observed disconnect **must** call
//! [`CalibratedGamepad::disconnect`] first, or change its supplied epoch. The
//! platform journal owns its own epoch; the supplied epoch is not forwarded as an
//! ABI or hardware ID. Use the adapter and its slot on one host thread, with no
//! competing sample producer; dropping it does not submit a disconnect.
//!
//! A headless embedding-host recipe using the real journal and PlayerInput:
//!
//! ```rust,check
//! use ferravex_core::session::GameSession;
//! use ferravex_render::input::{
//!     self, ActionKind, Binding, InputCode, PadState, PlayerInput,
//!     calibration::{AxisCalibration, CalibrationProfile, CalibratedGamepad, RadialStick},
//! };
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut session = GameSession::try_new(64., 4, true)?;
//! let mut player = PlayerInput::transient("calibration-recipe");
//! player.action("move_x", "Move", ActionKind::Axis,
//!     vec![Binding::digital(InputCode::PadAxis(0))])?;
//! player.action("jump", "Jump", ActionKind::Button,
//!     vec![Binding::digital(InputCode::PadButton(0))])?;
//! // Calibration supplies the radial response. Avoid applying a second deadzone
//! // in PlayerInput; its default response otherwise still applies independently.
//! let mut settings = player.settings();
//! settings.pad_deadzone = 0.;
//! settings.pad_curve = 1.;
//! player.set_settings(settings)?;
//! let mut pad = CalibratedGamepad::new(0)?;
//! let mut raw = PadState { connected: true, ..PadState::default() };
//! raw.axes[0] = 0.125; // supplied normalized drift, not a learned hardware value
//! pad.submit(raw)?;
//! input::host_frame(0., 640, 480, || player.sample(true)); // connection baseline
//! pad.select_profile(CalibrationProfile::try_new(
//!     &[(0, AxisCalibration { min: -0.75, center: 0.125, max: 0.875 })],
//!     &[RadialStick { axes: [0, 1], deadzone: 0.125 }],
//! )?)?;
//! raw.axes[0] = 0.875;
//! raw.buttons[0] = 1.;
//! pad.submit(raw)?;
//! raw.buttons[0] = 0.;
//! pad.submit(raw)?; // quick tap within the same render frame
//! input::host_frame(0., 640, 480, || {
//!     player.sample(true);
//!     assert_eq!(session.advance(0.).steps, 0); // leave gameplay edges pending
//! });
//! input::host_frame(2. / 64., 640, 480, || {
//!     player.sample(true);
//!     assert_eq!(player.value("move_x"), 1.);
//!     let ticks = session.advance(2. / 64.);
//!     assert_eq!(ticks.steps, 2);
//!     for tick in 0..ticks.steps {
//!         // Consume only on an actual simulation tick; share this snapshot.
//!         let [jump] = player.take_tick(["jump"]);
//!         assert_eq!(jump.pressed, tick == 0);
//!         assert_eq!(jump.released, tick == 0);
//!         assert_eq!(jump.value, 0.);
//!     }
//! });
//! pad.reset(); // immediately resubmits the last raw sample with default axes
//! pad.disconnect(); // neutral journal state and no retained profile
//! # Ok(())
//! # }
//! ```

use super::PadState;

/// Number of slots accepted by the existing embedding boundary.
pub const GAMEPAD_SLOTS: usize = 4;
/// Axis count in [`PadState`]. Profiles are not restricted to Linux's six axes.
pub const CALIBRATION_AXES: usize = 16;
/// At most eight disjoint two-axis sticks fit in a sample.
pub const CALIBRATION_STICKS: usize = CALIBRATION_AXES / 2;

/// A signed axis maps `min` to -1, `center` to 0 and `max` to 1, piecewise
/// linearly. Require finite `-1 <= min < center < max <= 1`. Author the values in
/// the host's normalized units, before any application deadzone. For a trigger,
/// opt in only if this signed response is desired; its button value is unchanged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisCalibration {
    pub min: f32,
    pub center: f32,
    pub max: f32,
}

/// An explicit pair, processed after per-axis calibration. Radius <= deadzone
/// becomes zero; (deadzone, 1) maps linearly to (0, 1); radius >= 1 becomes a unit
/// vector. Direction is preserved. Require finite `0 <= deadzone < 1` and two
/// distinct in-range axes which occur in no other pair. Zero still bounds
/// diagonals to the unit disk; omit the pair to preserve square fullscale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadialStick {
    pub axes: [usize; 2],
    pub deadzone: f32,
}

/// Validation errors never partially update a profile, adapter or journal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CalibrationError {
    InvalidSlot { slot: usize },
    TooManyAxes,
    TooManySticks,
    InvalidAxis { axis: usize },
    DuplicateAxis { axis: usize },
    InvalidRange { axis: usize },
    InvalidDeadzone { stick: usize },
    OverlappingStick { axis: usize },
    NonFiniteAxis { axis: usize },
    NonFiniteButton { button: usize },
    Disconnected,
}
impl std::fmt::Display for CalibrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "gamepad calibration: {self:?}")
    }
}
impl std::error::Error for CalibrationError {}

/// Immutable, checked, fixed-storage configuration. [`Default`] has no axis
/// corrections or radial pairs. Configuration is caller-owned and not serialized.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CalibrationProfile {
    axes: [Option<AxisCalibration>; CALIBRATION_AXES],
    sticks: [Option<RadialStick>; CALIBRATION_STICKS],
}
impl CalibrationProfile {
    /// Validate the entire bounded description before returning a profile.
    /// Duplicate axis corrections and overlapping stick pairs are rejected.
    /// An axis correction may also occur in one radial pair (correction first).
    /// See the module recipe for construction, selection and reset.
    pub fn try_new(
        axes: &[(usize, AxisCalibration)],
        sticks: &[RadialStick],
    ) -> Result<Self, CalibrationError> {
        if axes.len() > CALIBRATION_AXES {
            return Err(CalibrationError::TooManyAxes);
        }
        if sticks.len() > CALIBRATION_STICKS {
            return Err(CalibrationError::TooManySticks);
        }
        let mut result = Self::default();
        for &(axis, range) in axes {
            if axis >= CALIBRATION_AXES {
                return Err(CalibrationError::InvalidAxis { axis });
            }
            if result.axes[axis].is_some() {
                return Err(CalibrationError::DuplicateAxis { axis });
            }
            if !range.min.is_finite()
                || !range.center.is_finite()
                || !range.max.is_finite()
                || range.min < -1.
                || range.max > 1.
                || range.min >= range.center
                || range.center >= range.max
            {
                return Err(CalibrationError::InvalidRange { axis });
            }
            result.axes[axis] = Some(range);
        }
        let mut paired = [false; CALIBRATION_AXES];
        for (stick, &pair) in sticks.iter().enumerate() {
            if !pair.deadzone.is_finite() || !(0. ..1.).contains(&pair.deadzone) {
                return Err(CalibrationError::InvalidDeadzone { stick });
            }
            for axis in pair.axes {
                if axis >= CALIBRATION_AXES {
                    return Err(CalibrationError::InvalidAxis { axis });
                }
                if paired[axis] {
                    return Err(CalibrationError::OverlappingStick { axis });
                }
                paired[axis] = true;
            }
            result.sticks[stick] = Some(pair);
        }
        Ok(result)
    }

    /// Inspect every axis correction; `None` preserves that normalized channel.
    pub fn axes(&self) -> &[Option<AxisCalibration>; CALIBRATION_AXES] {
        &self.axes
    }

    /// Inspect pairs in authored order, followed by unused `None` entries.
    pub fn sticks(&self) -> &[Option<RadialStick>; CALIBRATION_STICKS] {
        &self.sticks
    }

    /// Pure transformation; never feeds the journal or mutates the supplied sample.
    /// Validate **all** connected axes/buttons for finiteness before calculating.
    /// Finite axis extremes clamp to [-1, 1]; buttons are copied bit-for-bit and
    /// retain the existing platform button clamping at submission. f64 arithmetic
    /// avoids overflow/underflow for finite f32 samples and narrow valid ranges.
    /// Disconnected samples ignore even invalid stale values and return neutral
    /// axes/buttons, retaining only the caller's epoch. No history or learning.
    pub fn apply(&self, raw: PadState) -> Result<PadState, CalibrationError> {
        if !raw.connected {
            return Ok(PadState {
                epoch: raw.epoch,
                ..PadState::default()
            });
        }
        for (axis, value) in raw.axes.iter().enumerate() {
            if !value.is_finite() {
                return Err(CalibrationError::NonFiniteAxis { axis });
            }
        }
        for (button, value) in raw.buttons.iter().enumerate() {
            if !value.is_finite() {
                return Err(CalibrationError::NonFiniteButton { button });
            }
        }
        let mut result = raw;
        for (axis, value) in result.axes.iter_mut().enumerate() {
            *value = match self.axes[axis] {
                None => value.clamp(-1., 1.),
                Some(range) => {
                    let v = f64::from(*value);
                    let center = f64::from(range.center);
                    let distance = v - center;
                    let span = if distance < 0. {
                        center - f64::from(range.min)
                    } else {
                        f64::from(range.max) - center
                    };
                    (distance / span).clamp(-1., 1.) as f32
                }
            };
        }
        for pair in self.sticks.iter().flatten() {
            let [a, b] = pair.axes;
            let x = f64::from(result.axes[a]);
            let y = f64::from(result.axes[b]);
            let radius = (x * x + y * y).sqrt();
            let deadzone = f64::from(pair.deadzone);
            let scale = if radius <= deadzone {
                0.
            } else {
                (radius.min(1.) - deadzone) / ((1. - deadzone) * radius)
            };
            result.axes[a] = (x * scale) as f32;
            result.axes[b] = (y * scale) as f32;
        }
        Ok(result)
    }
}

/// Explicit embedding-host adapter for one slot. No automatic device association,
/// global selection, hidden native polling, allocations or button synthesis.
/// It retains one last-good **raw** sample for immediate profile changes/reset.
/// Errors preserve that sample, selection and existing journal state. Like
/// [`super::gamepad_sample`], success means accepted submission, not consumption;
/// normal journal capacity/focus rules and PlayerInput held suppression still apply.
#[derive(Debug)]
pub struct CalibratedGamepad {
    slot: usize,
    profile: Option<CalibrationProfile>,
    last_raw: PadState,
}
impl CalibratedGamepad {
    /// Create a disconnected adapter with no profile. Does not touch the host.
    pub fn new(slot: usize) -> Result<Self, CalibrationError> {
        if slot >= GAMEPAD_SLOTS {
            return Err(CalibrationError::InvalidSlot { slot });
        }
        Ok(Self {
            slot,
            profile: None,
            last_raw: PadState::default(),
        })
    }

    /// The fixed journal slot. Host ownership is explicit, not a hardware ID.
    pub fn slot(&self) -> usize {
        self.slot
    }

    /// Selected connection-local profile, or `None` for existing defaults.
    pub fn profile(&self) -> Option<&CalibrationProfile> {
        self.profile.as_ref()
    }

    /// Last accepted uncalibrated sample, neutral when disconnected.
    pub fn last_raw(&self) -> &PadState {
        &self.last_raw
    }

    /// Calibrate exactly once and submit through [`super::gamepad_sample`].
    /// A disconnect or changed supplied epoch clears selection; a changed epoch
    /// also submits a disconnect/reconnect so existing PlayerInput held suppression
    /// applies. A new connection starts with defaults until explicitly selected
    /// again. Invalid connected samples do not clear selection, even when their
    /// epoch changed: no part of
    /// an invalid sample is accepted. A disconnected sample always succeeds.
    pub fn submit(&mut self, raw: PadState) -> Result<(), CalibrationError> {
        let profile =
            if raw.connected && self.last_raw.connected && raw.epoch == self.last_raw.epoch {
                self.profile
            } else {
                None
            };
        let output = profile.unwrap_or_default().apply(raw)?;
        // The slot was checked at construction. The complete output has been
        // validated before the existing boundary invokes any individual feed.
        if self.last_raw.connected && raw.connected && raw.epoch != self.last_raw.epoch {
            super::gamepad_sample(self.slot, PadState::default());
        }
        super::gamepad_sample(self.slot, output);
        self.profile = profile;
        self.last_raw = if raw.connected { raw } else { output };
        Ok(())
    }

    /// Select for the current connection and immediately reapply the last raw
    /// sample, including neutral axes. Replacing pairs cannot leave old axes
    /// stuck, and unchanged buttons generate no new journal edges. Fails while
    /// disconnected; profiles cannot silently wait for an unknown next device.
    pub fn select_profile(&mut self, profile: CalibrationProfile) -> Result<(), CalibrationError> {
        if !self.last_raw.connected {
            return Err(CalibrationError::Disconnected);
        }
        let output = profile.apply(self.last_raw)?;
        super::gamepad_sample(self.slot, output);
        self.profile = Some(profile);
        Ok(())
    }

    /// Clear selection and immediately resubmit the last raw sample using default
    /// clamping. Does not disconnect, erase held buttons, or consume tick edges.
    pub fn reset(&mut self) {
        // Last raw is already validated (or disconnected), so this cannot fail.
        let output = CalibrationProfile::default()
            .apply(self.last_raw)
            .expect("accepted calibration sample remains finite");
        super::gamepad_sample(self.slot, output);
        self.profile = None;
    }

    /// Explicitly end this device's slot ownership: neutralize the journal and
    /// forget both profile and last raw sample. Repeated calls are harmless.
    /// Call before replacing hardware when no physical disconnect was observed.
    pub fn disconnect(&mut self) {
        self.submit(PadState::default())
            .expect("disconnected samples are always accepted");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{InputCode, PlatformEvent};

    #[test]
    fn journey400_293_actual_button_journal_is_unchanged_by_profiles_and_reset() {
        fn run(calibrated: bool) -> Vec<PlatformEvent> {
            crate::edit(|s| *s = crate::State::default());
            let mut adapter = CalibratedGamepad::new(0).unwrap();
            let mut raw = PadState {
                connected: true,
                ..Default::default()
            };
            if calibrated {
                adapter.submit(raw).unwrap();
                adapter
                    .select_profile(
                        CalibrationProfile::try_new(
                            &[(
                                4,
                                AxisCalibration {
                                    min: -1.,
                                    center: 0.25,
                                    max: 1.,
                                },
                            )],
                            &[RadialStick {
                                axes: [0, 1],
                                deadzone: 0.2,
                            }],
                        )
                        .unwrap(),
                    )
                    .unwrap();
            }
            for (button, value) in [(0, 1.), (0, 0.), (6, 0.6), (7, 0.9), (14, 1.), (16, 0.7)] {
                raw.buttons[button] = value;
                raw.axes[0] = 0.1;
                raw.axes[4] = 0.8;
                if calibrated {
                    adapter.submit(raw).unwrap();
                    adapter.reset();
                    adapter
                        .select_profile(CalibrationProfile::default())
                        .unwrap();
                } else {
                    super::super::gamepad_sample(0, raw);
                }
            }
            crate::read(|s| {
                s.input
                    .events()
                    .iter()
                    .copied()
                    .filter(|event| {
                        matches!(
                            event,
                            PlatformEvent::Pad {
                                code: InputCode::PadButton(_),
                                ..
                            }
                        )
                    })
                    .collect()
            })
        }
        let direct = run(false);
        assert_eq!(direct.len(), 6);
        assert_eq!(run(true), direct);
    }

    #[test]
    fn journey400_293_rejected_final_axis_or_button_feeds_nothing() {
        crate::edit(|s| *s = crate::State::default());
        let mut adapter = CalibratedGamepad::new(0).unwrap();
        let mut raw = PadState {
            connected: true,
            ..Default::default()
        };
        adapter.submit(raw).unwrap();
        let before = crate::read(|s| (s.input.pads, s.input.events().to_vec()));
        raw.axes[0] = 0.9;
        raw.axes[15] = f32::NAN;
        assert_eq!(
            adapter.submit(raw),
            Err(CalibrationError::NonFiniteAxis { axis: 15 })
        );
        raw.axes[15] = 0.;
        raw.buttons[31] = f32::INFINITY;
        assert_eq!(
            adapter.submit(raw),
            Err(CalibrationError::NonFiniteButton { button: 31 })
        );
        assert_eq!(
            crate::read(|s| (s.input.pads, s.input.events().to_vec())),
            before
        );
        assert_eq!(adapter.last_raw().axes, [0.; 16]);
    }

    #[test]
    fn journey400_293_default_adapter_matches_full_direct_journal() {
        fn run(adapted: bool) -> (PadState, Vec<PlatformEvent>) {
            crate::edit(|s| *s = crate::State::default());
            let mut adapter = CalibratedGamepad::new(0).unwrap();
            let mut raw = PadState {
                connected: true,
                axes: std::array::from_fn(|i| (i as f32 - 8.) / 4.),
                buttons: std::array::from_fn(|i| (i as f32 - 1.) / 16.),
                ..Default::default()
            };
            for value in [1., 0., 0.8] {
                raw.buttons[0] = value;
                if adapted {
                    adapter.submit(raw).unwrap();
                } else {
                    super::super::gamepad_sample(0, raw);
                }
            }
            crate::read(|s| (s.input.pads[0], s.input.events().to_vec()))
        }
        let expected = run(false);
        assert!(!expected.1.is_empty());
        assert_eq!(run(true), expected);
    }
}
