//! Screen-pixel touch controls use the existing PlayerInput touch surface.
use crate::*;
use ferravex_render::input::{
    PlayerInput,
    touch::{Control, Output, Region},
};

pub(crate) const TAP: u8 = 14;
const CHANNELS: [u8; 14] = [0, 2, 3, 1, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
const LABELS: [&str; 14] = [
    "<", "Up", "Dn", ">", "Act", "X", "C", "1", "2", "3", "4", "5", "Pause", "Guide",
];

pub(crate) struct TouchDock {
    pub inset: f32,
    pub controls: Vec<Control>,
}
impl TouchDock {
    pub fn new(w: f32, h: f32, visible: bool) -> Self {
        let visible = visible && w >= 320. && h >= 240.;
        let columns = (((w - 12.) / 48.).floor() as usize).clamp(1, 7);
        let rows = 14_usize.div_ceil(columns);
        let viewport = crate::paint::Viewport::new(w, h);
        let side_columns = (((viewport.offset.x - 4.) / 48.).floor() as usize).min(2);
        let sides =
            visible && side_columns > 0 && 7_usize.div_ceil(side_columns) as f32 * 48. + 8. <= h;
        let inset = if visible && !sides {
            12. + rows as f32 * 48.
        } else {
            0.
        };
        // Slot zero is always the playfield tap. Its identity stays stable when
        // the first touch reveals the trailing dock slots. It cannot cover them.
        let mut controls = vec![Control {
            region: Region::Rect(if sides {
                Rect::new(viewport.offset.x, 0., w - 2. * viewport.offset.x, h)
            } else {
                Rect::new(0., 0., w.max(1.), (h - inset).max(1.))
            }),
            output: Output::Button(TAP),
        }];
        if visible {
            let width = (w - 16. - (columns - 1) as f32 * 4.) / columns as f32;
            for (i, channel) in CHANNELS.into_iter().enumerate() {
                let region = if sides {
                    let local = i % 7;
                    let left = if i < 7 {
                        4.
                    } else {
                        w - viewport.offset.x + 4.
                    };
                    let top = (h - 7_usize.div_ceil(side_columns) as f32 * 48.) * 0.5;
                    Rect::new(
                        left + (local % side_columns) as f32 * 48.,
                        top + (local / side_columns) as f32 * 48.,
                        44.,
                        44.,
                    )
                } else {
                    Rect::new(
                        8. + (i % columns) as f32 * (width + 4.),
                        h - inset + 4. + (i / columns) as f32 * 48.,
                        width,
                        44.,
                    )
                };
                controls.push(Control {
                    region: Region::Rect(region),
                    output: Output::Button(channel),
                });
            }
        }
        Self { inset, controls }
    }
    pub fn draw(&self, input: &PlayerInput, accent: Color) {
        if self.controls.len() == 1 || input.controls_open() {
            return;
        }
        panel(
            0.,
            screen_height() - self.inset,
            screen_width(),
            self.inset,
            INK,
        );
        let settings = input.touch_settings();
        let size = vec2(screen_width().max(64.), screen_height().max(64.));
        for (i, control) in self.controls.iter().enumerate().skip(1) {
            // Resolve the same saved geometry that set_touch_layout installs.
            let Ok(control) = settings.apply(*control, i, size) else {
                continue;
            };
            let Region::Rect(r) = control.region else {
                continue;
            };
            // The painted target is exactly the touch region (browser tests
            // locate these quads); a soft top highlight and accent rim sit on it.
            panel(
                r.x,
                r.y,
                r.w,
                r.h,
                Color::new(0.10, 0.17, 0.22, settings.opacity),
            );
            panel(
                r.x + 1.,
                r.y + 1.,
                r.w - 2.,
                r.h * 0.45,
                Color::new(1., 1., 1., 0.04 * settings.opacity),
            );
            draw_rectangle_lines(
                r.x,
                r.y,
                r.w,
                r.h,
                1.,
                crate::paint::with_alpha(accent, 0.6 * settings.opacity),
            );
            let label = LABELS[i - 1];
            crate::paint::text_centered(
                label,
                r.x + r.w * 0.5,
                r.y + r.h * 0.5 + 6.,
                15_f32.min((r.w - 8.) / label.len() as f32 * 1.3),
                PAPER,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_render::input::{
        ActionMap, Binding, InputCode,
        touch::{Event, Phase, TouchSurface},
    };
    #[test]
    fn phone_targets_are_large_separate_and_outside_the_game() {
        for (w, h) in [(320., 480.), (390., 844.), (844., 390.), (568., 320.)] {
            let dock = TouchDock::new(w, h, true);
            assert_eq!(dock.controls.len(), 15);
            let Region::Rect(game) = dock.controls[0].region else {
                unreachable!()
            };
            for (i, c) in dock.controls.iter().enumerate().skip(1) {
                let Region::Rect(r) = c.region else {
                    unreachable!()
                };
                assert!(r.w >= 44. && r.h >= 44.);
                assert!(r.x >= 0. && r.x + r.w <= w && r.y + r.h <= h);
                assert!(!r.overlaps(&game));
                for other in dock.controls.iter().skip(i + 1) {
                    let Region::Rect(s) = other.region else {
                        unreachable!()
                    };
                    assert!(!r.overlaps(&s));
                }
            }
            let viewport = crate::paint::Viewport::with_bottom_inset(w, h, dock.inset);
            let p = vec2(370., 210.);
            assert!((viewport.pointer(p * viewport.scale + viewport.offset) - p).length() < 0.001);
            assert!(viewport.offset.y + HEIGHT * viewport.scale <= h - dock.inset + 0.001);
        }
    }
    #[test]
    fn wide_phones_keep_the_full_game_height_between_control_rails() {
        for (w, h) in [(844., 390.), (667., 375.)] {
            let dock = TouchDock::new(w, h, true);
            assert_eq!(dock.inset, 0.);
            assert_eq!(dock.controls.len(), 15);
        }
    }
    #[test]
    fn simultaneous_holds_cancel_independently_and_canvas_taps_do_not_press_act() {
        let mut map = ActionMap::default();
        let movement = map
            .try_action(vec![Binding::digital(InputCode::Touch(0))])
            .unwrap();
        let action = map
            .try_action(vec![Binding::digital(InputCode::Touch(4))])
            .unwrap();
        let click = map
            .try_action(vec![Binding::digital(InputCode::Touch(TAP))])
            .unwrap();
        let dock = TouchDock::new(390., 844., true);
        let mut surface = TouchSurface::default();
        surface.set_layout(&dock.controls, &mut map).unwrap();
        let position = |channel| {
            dock.controls
                .iter()
                .find_map(|c| {
                    if c.output != Output::Button(channel) {
                        return None;
                    }
                    let Region::Rect(r) = c.region else {
                        return None;
                    };
                    Some(r.center())
                })
                .unwrap()
        };
        let event = |id, channel, phase| Event {
            id,
            position: position(channel),
            phase,
        };
        surface.handle(event(1, 0, Phase::Start), &mut map).unwrap();
        surface.handle(event(2, 4, Phase::Start), &mut map).unwrap();
        assert_eq!(map.physical_value(InputCode::Touch(0)), 1.);
        assert_eq!(map.physical_value(InputCode::Touch(4)), 1.);
        surface
            .handle(event(1, 0, Phase::Cancel), &mut map)
            .unwrap();
        assert_eq!(map.physical_value(InputCode::Touch(0)), 0.);
        assert_eq!(map.physical_value(InputCode::Touch(4)), 1.);
        surface.clear(&mut map);
        surface
            .handle(event(3, TAP, Phase::Start), &mut map)
            .unwrap();
        assert_eq!(map.physical_value(InputCode::Touch(TAP)), 1.);
        assert_eq!(map.physical_value(InputCode::Touch(4)), 0.);
        let _ = (movement, action, click);
    }
}
