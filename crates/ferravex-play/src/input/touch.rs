//! Bounded logical-pixel touch capture. Layout order sets hit-test priority;
//! a captured finger keeps its role until end/cancel, even outside its region.
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
#[derive(Clone, Copy, Debug)]
struct Capture {
    id: u64,
    control: usize,
    origin: Vec2,
    position: Vec2,
}
pub struct TouchSurface {
    controls: Vec<Control>,
    captures: [Option<Capture>; MAX_TOUCHES],
}
impl Default for TouchSurface {
    fn default() -> Self {
        Self {
            controls: Vec::with_capacity(MAX_TOUCH_CONTROLS),
            captures: [None; MAX_TOUCHES],
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
            self.flush(map);
        }
        Ok(())
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
    pub fn clear(&mut self, map: &mut ActionMap) {
        self.captures.fill(None);
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
pub use layout::{TouchEdit, TouchLayoutSettings};
