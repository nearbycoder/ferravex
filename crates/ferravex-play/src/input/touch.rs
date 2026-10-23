//! Bounded logical-pixel touch capture. Layout order sets hit-test priority;
//! a captured finger keeps its role until end/cancel, even outside its region.
//!
//! Immediate controls are unchanged. Opt in with [`TouchSurface::set_gesture_profile`]
//! and feed caller-supplied seconds to [`TouchSurface::handle_timed`] to observe
//! gestures separately from tick-consumed actions. No clock, queue, new input
//! channel or platform ABI is involved.
use super::{ActionMap, InputCode, InputError};
use ferravex_math::{Rect, Vec2};
pub const MAX_TOUCHES: usize = 16;
pub const MAX_TOUCH_CONTROLS: usize = 16;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Region {
    Circle { center: Vec2, radius: f32 },
    Rect(Rect),
}
impl Region {
    fn valid(self) -> bool {
        match self {
            Self::Circle { center, radius } => {
                bounded(center) && radius.is_finite() && (0.001..=1e5).contains(&radius)
            }
            Self::Rect(r) => {
                bounded(Vec2::new(r.x, r.y))
                    && r.w.is_finite()
                    && r.h.is_finite()
                    && r.w > 0.
                    && r.h > 0.
                    && r.w <= 1e6
                    && r.h <= 1e6
            }
        }
    }
    fn contains(self, p: Vec2) -> bool {
        match self {
            Self::Circle { center, radius } => center.distance_squared(p) <= radius * radius,
            Self::Rect(r) => r.contains(p),
        }
    }
    fn center(self) -> Vec2 {
        match self {
            Self::Circle { center, .. } => center,
            Self::Rect(r) => r.center(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Output {
    Button(u8),
    Stick {
        x: u8,
        y: u8,
        radius: f32,
        floating: bool,
    },
    Look {
        x: u8,
        y: u8,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Control {
    pub region: Region,
    pub output: Output,
}
impl Control {
    fn valid(self) -> bool {
        self.region.valid()
            && match self.output {
                Output::Button(n) => n < 64,
                Output::Stick { x, y, radius, .. } => {
                    x < 64
                        && y < 64
                        && x != y
                        && radius.is_finite()
                        && (0.001..=1e5).contains(&radius)
                }
                Output::Look { x, y } => x < 16 && y < 16 && x != y,
            }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Start,
    Move,
    End,
    Cancel,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub id: u64,
    pub position: Vec2,
    pub phase: Phase,
}
/// Logical-pixel distances and supplied-time seconds for opt-in recognition.
///
/// Distances must be finite, with `0 <= tap_max_distance < drag_min_distance
/// <= swipe_min_distance <= 1e6`. Times must be finite in `0..=3600` seconds.
/// Every threshold is inclusive. Tap uses maximum excursion from the starting
/// point and maximum duration; drag latches once maximum excursion and minimum
/// duration both qualify. Swipe uses **final net displacement** and maximum
/// duration, and takes precedence over drag on End. There is no velocity test.
/// A gap between tap and drag distances can deliberately produce no gesture.
///
/// Default thresholds are provided only for this new opt-in path: 10 px / 0.25 s
/// tap, 12 px / 0 s drag, and 48 px / 0.5 s swipe. Legacy handling never reads them.
///
/// ```
/// use ferravex_play::input::touch::{GestureError, GestureProfile};
/// let profile = GestureProfile { tap_max_seconds: 0.4, ..GestureProfile::default() };
/// profile.validate()?;
/// assert_eq!(GestureProfile { drag_min_distance: 0.0, ..profile }.validate(),
///     Err(GestureError::InvalidProfile));
/// # Ok::<(), GestureError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureProfile {
    pub tap_max_distance: f32,
    pub tap_max_seconds: f64,
    pub drag_min_distance: f32,
    pub drag_min_seconds: f64,
    pub swipe_min_distance: f32,
    pub swipe_max_seconds: f64,
}
impl Default for GestureProfile {
    fn default() -> Self {
        Self {
            tap_max_distance: 10.,
            tap_max_seconds: 0.25,
            drag_min_distance: 12.,
            drag_min_seconds: 0.,
            swipe_min_distance: 48.,
            swipe_max_seconds: 0.5,
        }
    }
}
impl GestureProfile {
    /// Validate all fields without changing a surface. See the type's example.
    pub fn validate(self) -> Result<(), GestureError> {
        let distances = [
            self.tap_max_distance,
            self.drag_min_distance,
            self.swipe_min_distance,
        ];
        let times = [
            self.tap_max_seconds,
            self.drag_min_seconds,
            self.swipe_max_seconds,
        ];
        if distances
            .iter()
            .any(|d| !d.is_finite() || !(0.0..=1e6).contains(d))
            || times
                .iter()
                .any(|t| !t.is_finite() || !(0.0..=3600.0).contains(t))
            || self.tap_max_distance >= self.drag_min_distance
            || self.drag_min_distance > self.swipe_min_distance
        {
            return Err(GestureError::InvalidProfile);
        }
        Ok(())
    }
}

/// Separate recognition errors keep the existing exhaustive InputError intact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureError {
    InvalidProfile,
    Disabled,
    /// Supplied seconds must be finite and nonnegative.
    InvalidTime,
    /// Earlier than the last successfully accepted timed event on this surface.
    TimeWentBackwards,
    Input(InputError),
}
impl std::fmt::Display for GestureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "touch gesture: {self:?}")
    }
}
impl std::error::Error for GestureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            _ => None,
        }
    }
}

/// Classification carried only by the explicit observation return value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureKind {
    Tap,
    Drag,
    Swipe,
}

/// One accepted event's observation; never inserted into ActionMap.
///
/// Tap and Swipe occur only at End. Drag occurs at each qualifying Move and at
/// End (including an End that crosses the threshold without an earlier Move).
/// An End produces at most one observation: Swipe, else latched/qualifying Drag,
/// else Tap, else none. Start/Cancel never produce an observation. A prior Drag
/// may finish as Swipe. Consumers must discard drag presentation on Cancel or
/// explicit clear/layout/profile reset; resets do not synthesize terminal events.
/// `control` is the captured layout index, not an InputCode or saved stable slot.
/// See [`TouchSurface::handle_timed`] for an executable example.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureObservation {
    pub id: u64,
    pub control: usize,
    pub kind: GestureKind,
    pub phase: Phase,
    /// Actual touch start, including for nonfloating sticks.
    pub origin: Vec2,
    pub position: Vec2,
    pub elapsed_seconds: f64,
}

#[derive(Clone, Copy, Debug)]
struct GestureCapture {
    origin: Vec2,
    started_seconds: f64,
    max_distance_squared: f64,
    dragging: bool,
}
impl GestureCapture {
    fn observe(
        &mut self,
        capture: Capture,
        event: Event,
        seconds: f64,
        profile: GestureProfile,
    ) -> Option<GestureObservation> {
        // Widen before subtraction/squaring: all validated f32 thresholds,
        // including subnormal distances near zero, retain their ordering.
        let dx = f64::from(event.position.x) - f64::from(self.origin.x);
        let dy = f64::from(event.position.y) - f64::from(self.origin.y);
        let distance_squared = dx * dx + dy * dy;
        self.max_distance_squared = self.max_distance_squared.max(distance_squared);
        let elapsed = seconds - self.started_seconds;
        self.dragging |= self.max_distance_squared >= f64::from(profile.drag_min_distance).powi(2)
            && elapsed >= profile.drag_min_seconds;
        let kind = if event.phase == Phase::End
            && distance_squared >= f64::from(profile.swipe_min_distance).powi(2)
            && elapsed <= profile.swipe_max_seconds
        {
            GestureKind::Swipe
        } else if self.dragging {
            GestureKind::Drag
        } else if event.phase == Phase::End
            && self.max_distance_squared <= f64::from(profile.tap_max_distance).powi(2)
            && elapsed <= profile.tap_max_seconds
        {
            GestureKind::Tap
        } else {
            return None;
        };
        Some(GestureObservation {
            id: event.id,
            control: capture.control,
            kind,
            phase: event.phase,
            origin: self.origin,
            position: event.position,
            elapsed_seconds: elapsed,
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct Capture {
    id: u64,
    control: usize,
    origin: Vec2,
    position: Vec2,
    gesture: Option<GestureCapture>,
}
pub struct TouchSurface {
    controls: Vec<Control>,
    captures: [Option<Capture>; MAX_TOUCHES],
    gesture_profile: Option<GestureProfile>,
    last_gesture_seconds: Option<f64>,
}
impl Default for TouchSurface {
    fn default() -> Self {
        Self {
            controls: Vec::with_capacity(MAX_TOUCH_CONTROLS),
            captures: [None; MAX_TOUCHES],
            gesture_profile: None,
            last_gesture_seconds: None,
        }
    }
}
fn bounded(p: Vec2) -> bool {
    p.is_finite() && p.abs().max_element() <= 1e6
}
impl TouchSurface {
    /// Identical layouts preserve captures. A changed/resized layout releases
    /// all old output codes before installing the new one, without reallocating.
    pub fn controls(&self) -> &[Control] {
        &self.controls
    }
    pub fn set_layout(
        &mut self,
        controls: &[Control],
        map: &mut ActionMap,
    ) -> Result<(), InputError> {
        if controls.len() > MAX_TOUCH_CONTROLS {
            return Err(InputError::TouchCapacity);
        }
        if controls.iter().any(|c| !c.valid()) {
            return Err(InputError::InvalidTouch);
        }
        if self.controls == controls {
            return Ok(());
        }
        self.clear(map);
        self.controls.clear();
        self.controls.extend_from_slice(controls);
        Ok(())
    }
    pub fn handle(&mut self, event: Event, map: &mut ActionMap) -> Result<(), InputError> {
        self.handle_scaled(event, map, Vec2::ONE)
    }
    /// Apply a host's per-pixel sensitivity before motion reaches bound actions.
    pub fn handle_scaled(
        &mut self,
        event: Event,
        map: &mut ActionMap,
        look_scale: Vec2,
    ) -> Result<(), InputError> {
        if !look_scale.is_finite() || look_scale.abs().max_element() > 100. {
            return Err(InputError::InvalidTouch);
        }
        if !bounded(event.position) {
            return Err(InputError::InvalidTouch);
        }
        let old = self
            .captures
            .iter()
            .position(|c| c.is_some_and(|c| c.id == event.id));
        if matches!(event.phase, Phase::End | Phase::Cancel) {
            if let Some(i) = old {
                self.captures[i] = None;
                self.flush(map);
            }
            return Ok(());
        }
        if event.phase == Phase::Start {
            let Some(control) = self
                .controls
                .iter()
                .position(|c| c.region.contains(event.position))
            else {
                if let Some(i) = old {
                    self.captures[i] = None;
                    self.flush(map);
                }
                return Ok(());
            };
            let slot = old
                .or_else(|| self.captures.iter().position(Option::is_none))
                .ok_or(InputError::TouchCapacity)?;
            let origin = match self.controls[control].output {
                Output::Stick {
                    floating: false, ..
                } => self.controls[control].region.center(),
                _ => event.position,
            };
            self.captures[slot] = Some(Capture {
                id: event.id,
                control,
                origin,
                position: event.position,
                gesture: None,
            });
            self.flush(map);
            return Ok(());
        }
        if let Some(i) = old {
            let capture = self.captures[i].as_mut().unwrap();
            if let Output::Look { x, y } = self.controls[capture.control].output {
                let delta = (event.position - capture.position) * look_scale;
                map.feed_delta(InputCode::TouchDelta(x), delta.x);
                map.feed_delta(InputCode::TouchDelta(y), delta.y);
            }
            capture.position = event.position;
            capture.gesture = None;
            self.flush(map);
        }
        Ok(())
    }
    /// Current opt-in profile; None by default. See [`Self::set_gesture_profile`].
    pub fn gesture_profile(&self) -> Option<GestureProfile> {
        self.gesture_profile
    }

    /// Validate before mutation. A different profile (including enable/disable)
    /// clears all captures, releases their held outputs and resets the supplied
    /// time watermark. An identical profile preserves captures/time. Unrelated
    /// queued input edges and accumulated motion are not consumed or erased.
    /// Invalid profiles preserve all state, even with active captures.
    ///
    /// ```
    /// use ferravex_play::input::{ActionMap, touch::{TouchSurface, GestureProfile}};
    /// let mut map = ActionMap::default();
    /// let mut touch = TouchSurface::default();
    /// assert_eq!(touch.gesture_profile(), None);
    /// touch.set_gesture_profile(Some(GestureProfile::default()), &mut map)?;
    /// assert!(touch.gesture_profile().is_some());
    /// touch.set_gesture_profile(None, &mut map)?;
    /// assert_eq!(touch.gesture_profile(), None);
    /// # Ok::<(), ferravex_play::input::touch::GestureError>(())
    /// ```
    pub fn set_gesture_profile(
        &mut self,
        profile: Option<GestureProfile>,
        map: &mut ActionMap,
    ) -> Result<(), GestureError> {
        if let Some(profile) = profile {
            profile.validate()?;
        }
        if self.gesture_profile != profile {
            self.clear(map);
            self.gesture_profile = profile;
        }
        Ok(())
    }

    /// Handle immediate controls and return an optional gesture using supplied
    /// seconds, without consuming any action edges. Equivalent to
    /// [`Self::handle_timed_scaled`] with unit look sensitivity.
    ///
    /// ```
    /// use ferravex_math::{Rect, Vec2};
    /// use ferravex_play::input::{ActionMap, Binding, InputCode, touch::*};
    /// let mut map = ActionMap::default();
    /// let jump = map.try_action(vec![Binding::digital(InputCode::Touch(0))]).unwrap();
    /// let mut touch = TouchSurface::default();
    /// touch.set_layout(&[Control { region: Region::Rect(Rect::new(0., 0., 100., 100.)),
    ///     output: Output::Button(0) }], &mut map)?;
    /// touch.set_gesture_profile(Some(GestureProfile::default()), &mut map)?;
    /// let event = |phase| Event { id: 7, position: Vec2::splat(20.), phase };
    /// assert!(touch.handle_timed(event(Phase::Start), 10.0, &mut map)?.is_none());
    /// let observed = touch.handle_timed(event(Phase::End), 10.125, &mut map)?.unwrap();
    /// assert_eq!((observed.kind, observed.elapsed_seconds), (GestureKind::Tap, 0.125));
    /// // No actual simulation tick yet: both immediate button edges remain.
    /// assert!(map.peek(jump).pressed && map.peek(jump).released);
    /// let tick = map.consume_tick(jump); // Only inside an actual executed update.
    /// assert!(tick.pressed && tick.released);
    /// assert!(!map.consume_tick(jump).pressed);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn handle_timed(
        &mut self,
        event: Event,
        seconds: f64,
        map: &mut ActionMap,
    ) -> Result<Option<GestureObservation>, GestureError> {
        self.handle_timed_scaled(event, seconds, map, Vec2::ONE)
    }

    /// Supplied-time observation with the same capture ownership, layout priority
    /// and immediate outputs as [`Self::handle_scaled`]. Look scaling affects
    /// only existing TouchDelta outputs; recognition uses unscaled logical pixels.
    /// See [`Self::handle_timed`] for an executable example.
    ///
    /// Supply finite nonnegative seconds in one caller-owned monotonic domain
    /// (prefer seconds since session start). Equal timestamps are allowed. Time
    /// must not decrease across **any** successfully accepted timed events on this
    /// surface, including uncaptured IDs. No wall clock is inferred. Caller f64
    /// precision determines time resolution. Clear, changed layout or changed
    /// profile forgets this watermark, so focus loss needs no timestamp.
    ///
    /// Error precedence: disabled profile, invalid seconds, decreasing seconds,
    /// then legacy position/scale/capture-capacity validation. Every error leaves
    /// map, captures, recognition and watermark unchanged. Uncaptured events have
    /// no observation. There are at most MAX_TOUCHES captures; a repeated Start
    /// replaces that ID's old capture/gesture (or drops it if outside the layout),
    /// even at capacity. Cancel always drops recognition without a tap/swipe.
    /// End uses its own position, even without a Move; legacy control release still
    /// ignores End motion. Repeated End cannot deliver the observation again.
    ///
    /// A capture is observed only if its Start used this timed path. Successfully
    /// accepted untimed events drop recognition for their affected capture while
    /// preserving legacy output behavior; later timed events cannot revive it.
    /// Mixing timed and untimed pointers is allowed. Recognition storage is inline,
    /// with no queue/backpressure: each call returns at most one observation that
    /// the caller must handle immediately or retain in its own bounded storage.
    pub fn handle_timed_scaled(
        &mut self,
        event: Event,
        seconds: f64,
        map: &mut ActionMap,
        look_scale: Vec2,
    ) -> Result<Option<GestureObservation>, GestureError> {
        let profile = self.gesture_profile.ok_or(GestureError::Disabled)?;
        if !seconds.is_finite() || seconds < 0. {
            return Err(GestureError::InvalidTime);
        }
        if self.last_gesture_seconds.is_some_and(|last| seconds < last) {
            return Err(GestureError::TimeWentBackwards);
        }
        let old = self
            .captures
            .iter()
            .flatten()
            .find(|c| c.id == event.id)
            .copied();
        self.handle_scaled(event, map, look_scale)
            .map_err(GestureError::Input)?;
        self.last_gesture_seconds = Some(seconds);
        let mut observation = None;
        let gesture = match event.phase {
            Phase::Start => Some(GestureCapture {
                origin: event.position,
                started_seconds: seconds,
                max_distance_squared: 0.,
                dragging: false,
            }),
            Phase::Move | Phase::End => old.and_then(|capture| {
                capture.gesture.map(|mut gesture| {
                    observation = gesture.observe(capture, event, seconds, profile);
                    gesture
                })
            }),
            Phase::Cancel => None,
        };
        if let Some(capture) = self
            .captures
            .iter_mut()
            .flatten()
            .find(|c| c.id == event.id)
        {
            capture.gesture = gesture;
        }
        Ok(observation)
    }

    fn flush(&self, map: &mut ActionMap) {
        let mut values = [0_f32; 64];
        for c in self.captures.iter().flatten() {
            match self.controls[c.control].output {
                Output::Button(n) => values[n as usize] = 1.,
                Output::Stick { x, y, radius, .. } => {
                    let delta = ((c.position - c.origin) / radius).clamp_length_max(1.);
                    values[x as usize] = (values[x as usize] + delta.x).clamp(-1., 1.);
                    values[y as usize] = (values[y as usize] + delta.y).clamp(-1., 1.);
                }
                Output::Look { .. } => {}
            }
        }
        for (i, &value) in values.iter().enumerate() {
            map.feed(InputCode::Touch(i as u8), value);
        }
    }
    /// Release captures and discard recognition/time history, retaining the
    /// configured profile. Valid for focus loss with no supplied timestamp.
    /// Does not consume queued action edges or reset unrelated input sources.
    pub fn clear(&mut self, map: &mut ActionMap) {
        self.captures.fill(None);
        self.last_gesture_seconds = None;
        self.flush(map);
    }
    pub fn active(&self) -> usize {
        self.captures.iter().filter(|c| c.is_some()).count()
    }
    pub fn stick(&self, x: u8, y: u8) -> Option<(Vec2, Vec2)> {
        self.captures
            .iter()
            .flatten()
            .find_map(|c| match self.controls[c.control].output {
                Output::Stick {
                    x: cx,
                    y: cy,
                    radius,
                    ..
                } if (cx, cy) == (x, y) => Some((
                    c.origin,
                    ((c.position - c.origin) / radius).clamp_length_max(1.),
                )),
                _ => None,
            })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{ActionKind, Binding, ContextId};
    fn event(id: u64, x: f32, y: f32, phase: Phase) -> Event {
        Event {
            id,
            position: Vec2::new(x, y),
            phase,
        }
    }
    #[test]
    fn quick_taps_and_two_fingers_keep_independent_roles() {
        let mut map = ActionMap::default();
        let jump = map
            .named_action(
                "jump",
                ActionKind::Button,
                ContextId::GLOBAL,
                vec![Binding::digital(InputCode::Touch(0))],
            )
            .unwrap();
        let mut touch = TouchSurface::default();
        touch
            .set_layout(
                &[Control {
                    region: Region::Rect(Rect::new(0., 0., 100., 100.)),
                    output: Output::Button(0),
                }],
                &mut map,
            )
            .unwrap();
        touch
            .handle(event(1, 10., 10., Phase::Start), &mut map)
            .unwrap();
        touch
            .handle(event(1, 10., 10., Phase::End), &mut map)
            .unwrap();
        let tap = map.consume_tick(jump);
        assert!(tap.pressed && tap.released);
        assert_eq!(tap.value, 0.);
        for id in [1, 2] {
            touch
                .handle(event(id, 10., 10., Phase::Start), &mut map)
                .unwrap();
        }
        touch
            .handle(event(1, 200., 200., Phase::Cancel), &mut map)
            .unwrap();
        assert_eq!(map.peek(jump).value, 1.);
        touch
            .handle(event(2, 200., 200., Phase::End), &mut map)
            .unwrap();
        assert_eq!(map.peek(jump).value, 0.);
    }
    #[test]
    fn floating_stick_look_capture_and_resize_do_not_stick() {
        let mut map = ActionMap::default();
        let look = map
            .named_action(
                "look",
                ActionKind::Delta,
                ContextId::GLOBAL,
                vec![Binding::digital(InputCode::TouchDelta(0))],
            )
            .unwrap();
        let mut touch = TouchSurface::default();
        let layout = [
            Control {
                region: Region::Rect(Rect::new(0., 0., 100., 100.)),
                output: Output::Stick {
                    x: 0,
                    y: 1,
                    radius: 20.,
                    floating: true,
                },
            },
            Control {
                region: Region::Rect(Rect::new(100., 0., 100., 100.)),
                output: Output::Look { x: 0, y: 1 },
            },
        ];
        touch.set_layout(&layout, &mut map).unwrap();
        touch
            .handle(event(1, 50., 50., Phase::Start), &mut map)
            .unwrap();
        touch
            .handle(event(2, 150., 50., Phase::Start), &mut map)
            .unwrap();
        touch
            .handle(event(1, 150., 50., Phase::Move), &mut map)
            .unwrap();
        touch
            .handle(event(2, 165., 45., Phase::Move), &mut map)
            .unwrap();
        assert_eq!(map.physical_value(InputCode::Touch(0)), 1.);
        assert_eq!(map.consume_tick(look).value, 15.);
        assert_eq!(touch.stick(0, 1).unwrap().1, Vec2::X);
        touch.set_layout(&layout, &mut map).unwrap();
        assert_eq!(touch.active(), 2);
        touch.set_layout(&[], &mut map).unwrap();
        assert_eq!(touch.active(), 0);
        assert_eq!(map.physical_value(InputCode::Touch(0)), 0.);
    }
    #[test]
    fn invalid_layout_or_event_never_loses_existing_capture() {
        let mut map = ActionMap::default();
        let mut touch = TouchSurface::default();
        touch
            .set_layout(
                &[Control {
                    region: Region::Circle {
                        center: Vec2::ZERO,
                        radius: 50.,
                    },
                    output: Output::Button(0),
                }],
                &mut map,
            )
            .unwrap();
        touch
            .handle(event(1, 0., 0., Phase::Start), &mut map)
            .unwrap();
        assert!(
            touch
                .handle(event(1, f32::NAN, 0., Phase::End), &mut map)
                .is_err()
        );
        assert_eq!(touch.active(), 1);
        assert!(
            touch
                .set_layout(
                    &[Control {
                        region: Region::Circle {
                            center: Vec2::ZERO,
                            radius: 0.
                        },
                        output: Output::Button(0)
                    }],
                    &mut map
                )
                .is_err()
        );
        assert_eq!(touch.active(), 1);
        touch.clear(&mut map);
        assert_eq!(map.physical_value(InputCode::Touch(0)), 0.);
    }
}

mod layout;
pub use layout::{TouchEdit, TouchLayoutDiagnostics, TouchLayoutSettings};
