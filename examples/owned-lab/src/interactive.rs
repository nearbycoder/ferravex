//! The custom WebGL host shares Rust profiles and pauses only offline training.
use super::*;
use ferravex_engine::render::{
    self,
    input::{
        PlayerInput, Preset,
        touch::{Control, Output, Region},
    },
    prelude::{Rect, Vec2, vec2},
};
use std::cell::RefCell;
struct Controls {
    input: PlayerInput,
    yaw: f32,
    pitch: f32,
    menu: u32,
}
// Leave the bottom 152 pixels to the host's wrapping health/ammo/time strip.
// Drawing and hit regions share these centers so visible actions stay truthful.
fn touch_buttons(w: f32, h: f32) -> [(f32, f32, f32, &'static str, u8); 3] {
    [
        (w - 74., h - 208., 48., "FIRE", 2),
        (w - 64., h - 309., 34., "UP", 3),
        (w - 170., h - 309., 31., "R", 5),
    ]
}
fn resting_stick(h: f32) -> Vec2 {
    vec2(62., h - 208.)
}
thread_local! {static CONTROLS:RefCell<Option<Controls>>=const {RefCell::new(None)};}
pub(super) fn reset_controls() {
    CONTROLS.with(|cell| {
        if let Some(controls) = cell.borrow_mut().as_mut() {
            controls.yaw = 0.;
            controls.pitch = 0.;
            controls.menu = 0;
            // Keep the current profile/preferences, but suppress held gameplay actions.
            controls.input.set_paused(true);
            controls.input.sample(false);
        }
    });
}
/// Discard pending local actions when a menu, focus loss or pointer release interrupts a frame.
/// Aim and control preferences are retained; reset uses `ferravex_training_reset` instead.
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_training_release_input() {
    CONTROLS.with(|cell| {
        if let Some(controls) = cell.borrow_mut().as_mut() {
            controls.menu = 0;
            controls.input.set_paused(true);
        }
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_controls_step(
    dt: f64,
    playing: u32,
    view: u32,
    aspect: f32,
    width: u32,
    height: u32,
) {
    render::input::host_frame(dt as f32, width, height, || {
        CONTROLS.with(|cell| {
            let mut cell = cell.borrow_mut();
            let controls = cell.get_or_insert_with(|| Controls {
                input: PlayerInput::preset("owned-lab", Preset::Shooter),
                yaw: 0.,
                pitch: 0.,
                menu: 0,
            });
            let p = &mut controls.input;
            p.set_paused(playing == 0);
            let w = width.max(1) as f32;
            let h = height.max(1) as f32;
            let buttons = touch_buttons(w, h);
            p.set_touch_layout(&[
                Control {
                    region: Region::Circle {
                        center: vec2(buttons[0].0, buttons[0].1),
                        radius: buttons[0].2,
                    },
                    output: Output::Button(buttons[0].4),
                },
                Control {
                    region: Region::Circle {
                        center: vec2(buttons[1].0, buttons[1].1),
                        radius: buttons[1].2,
                    },
                    output: Output::Button(buttons[1].4),
                },
                Control {
                    region: Region::Circle {
                        center: vec2(buttons[2].0, buttons[2].1),
                        radius: buttons[2].2,
                    },
                    output: Output::Button(buttons[2].4),
                },
                Control {
                    region: Region::Rect(Rect::new(0., 0., w * 0.42, (h - 152.).max(1.))),
                    output: Output::Stick {
                        x: 0,
                        y: 1,
                        radius: 54.,
                        floating: true,
                    },
                },
                Control {
                    region: Region::Rect(Rect::new(w * 0.42, 0., w * 0.58, (h - 152.).max(1.))),
                    output: Output::Look { x: 0, y: 1 },
                },
            ])
            .unwrap();
            if !p.controls_open() {
                render::set_cursor_grab(playing != 0 && view == 1);
                render::show_mouse(playing == 0 || view != 1);
            }
            p.sample(playing != 0);
            controls.menu = if playing != 0 && p.pressed("pause") {
                1
            } else if playing == 0 && p.pressed("confirm") {
                2
            } else {
                0
            };
            if controls.menu == 1 {
                p.set_paused(true);
            }
            let active = p.gameplay_active() && controls.menu != 1;
            let look = p.look(dt as f32);
            if active {
                controls.yaw = (controls.yaw + look.x).rem_euclid(std::f32::consts::TAU);
                controls.pitch = (controls.pitch - look.y).clamp(-1.5, 1.5);
            }
            let movement = p.movement();
            LAB.with(|cell| {
                if let Some(lab) = cell.borrow_mut().as_mut() {
                    lab.reconcile_interactive(active);
                    let neutral = lab.neutral_input();
                    lab.update_sampled(dt, view, aspect, || {
                        if !active {
                            return neutral;
                        }
                        let actions = p.take_tick(["fire", "jump", "reload"]);
                        let mut buttons = 0;
                        for (action, bit) in actions.into_iter().zip([
                            ferravex_fps::BUTTON_FIRE,
                            ferravex_fps::BUTTON_JUMP,
                            ferravex_fps::BUTTON_RELOAD,
                        ]) {
                            if action.pressed || action.value > 0.5 {
                                buttons |= bit;
                            }
                        }
                        InputFrame {
                            move_x: movement.x,
                            move_z: movement.y,
                            yaw: controls.yaw,
                            pitch: controls.pitch,
                            buttons,
                            ..Default::default()
                        }
                    });
                    if playing != 0 && !p.controls_open() {
                        super::hud::draw(lab, p, w, h, view);
                    }
                }
            });
            if playing != 0 && p.touch_seen() && !p.touch_customized() && !p.controls_open() {
                let tint = render::Color::new(0.9, 0.92, 0.83, 0.45);
                let (origin, delta) = p
                    .touch_stick(0, 1)
                    .unwrap_or((resting_stick(h), Vec2::ZERO));
                render::draw_circle_lines(origin.x, origin.y, 54., 2., tint);
                render::draw_circle(
                    origin.x + delta.x * 38.,
                    origin.y + delta.y * 38.,
                    16.,
                    tint,
                );
                for (x, y, r, label, _) in buttons {
                    render::draw_circle_lines(x, y, r - 2., 2., tint);
                    let size = render::measure_text(label, None, 17, 1.);
                    render::draw_text(label, x - size.width * 0.5, y + 6., 17., tint);
                }
            }
            p.draw_controls();
        })
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_menu_requested() -> u32 {
    CONTROLS.with(|cell| cell.borrow().as_ref().map_or(0, |c| c.menu))
}

#[cfg(test)]
mod tests {
    use super::*;
    use render::input::{ActionKind, Binding, InputCode, PadState};

    #[test]
    fn journey200_199_touch_actions_clear_guide_footer_and_each_other() {
        // The two short viewport sizes use a 640px logical host surface and a
        // 44px native pan strip. 390x844 has a 682px surface below its toolbar.
        for (w, h) in [(276., 640.), (276., 640.), (390., 682.), (1280., 737.)] {
            let buttons = touch_buttons(w, h);
            let stick = resting_stick(h);
            assert!(stick.x - 54. >= 0. && stick.x + 54. <= w);
            assert!(stick.y + 54. <= h - 152.);
            // Knob radius 16 plus the full displayed displacement 38 fits
            // the resting ring's radius. A live floating origin is user-chosen.
            assert_eq!(38. + 16., 54.);
            for (i, &(x, y, r, _, _)) in buttons.iter().enumerate() {
                assert!(x - r >= 0. && x + r <= w);
                assert!(
                    (x - stick.x).hypot(y - stick.y) > r + 54.,
                    "resting stick and action rings must be visually separate"
                );
                assert!(
                    y - r >= 294.,
                    "clear the wrapped guide at logical y152..294"
                );
                assert!(y + r <= h - 152., "reserve the wrapping DOM vitals/footer");
                for &(other_x, other_y, other_r, _, _) in &buttons[..i] {
                    assert!((x - other_x).hypot(y - other_y) > r + other_r);
                }
            }
        }
    }

    fn setup_controls() {
        let mut input = PlayerInput::transient("journey200-144-input");
        for (name, button) in [("fire", 0), ("jump", 1), ("reload", 2)] {
            input
                .action(
                    name,
                    name,
                    ActionKind::Button,
                    vec![Binding::digital(InputCode::PadButton(button))],
                )
                .unwrap();
        }
        CONTROLS.with(|cell| {
            *cell.borrow_mut() = Some(Controls {
                input,
                yaw: 0.,
                pitch: 0.,
                menu: 0,
            })
        });
        ferravex_training_init();
        render::input::gamepad_sample(
            0,
            PadState {
                connected: true,
                ..Default::default()
            },
        );
        ferravex_controls_step(0., 1, 1, 1., 320, 480);
        // Reset sampled the old render frame. Establish the new controller's
        // connection epoch before injecting a tap (connection edges are suppressed).
        ferravex_controls_step(0., 1, 1, 1., 320, 480);
    }
    fn fire_button(down: bool) {
        let mut pad = PadState {
            connected: true,
            ..Default::default()
        };
        pad.buttons[0] = f32::from(down);
        assert!(render::input::gamepad_sample(0, pad));
    }
    #[test]
    fn real_input_tap_survives_zero_ticks_and_is_not_repeated_in_catch_up() {
        setup_controls();
        fire_button(true);
        fire_button(false);
        ferravex_controls_step(1. / 128., 1, 1, 1., 320, 480);
        assert_eq!(with_lab(|lab| lab.game.tick), 0);
        assert_eq!(with_lab(|lab| lab.training.shots), 0);
        ferravex_controls_step(15. / 128., 1, 1, 1., 320, 480);
        assert_eq!(with_lab(|lab| lab.game.tick), 8);
        assert_eq!(with_lab(|lab| lab.training.shots), 1);
        assert_eq!(with_lab(|lab| lab.game.player(0).unwrap().ammo), 29);
    }
    #[test]
    fn release_discards_real_queued_edges_and_suppresses_held_fire_until_release() {
        setup_controls();
        fire_button(true);
        ferravex_controls_step(0., 1, 1, 1., 320, 480);
        ferravex_training_release_input();
        ferravex_controls_step(1. / 64., 1, 1, 1., 320, 480);
        assert_eq!(with_lab(|lab| lab.training.shots), 0);
        fire_button(false);
        ferravex_controls_step(0., 1, 1, 1., 320, 480);
        fire_button(true);
        ferravex_controls_step(1. / 64., 1, 1, 1., 320, 480);
        assert_eq!(with_lab(|lab| lab.training.shots), 1);
        assert_eq!(ferravex_training_reset(), 1);
        ferravex_controls_step(1. / 64., 1, 1, 1., 320, 480);
        assert_eq!(
            with_lab(|lab| lab.training.shots),
            0,
            "reset also suppresses held input"
        );
    }

    #[test]
    fn release_preserves_aim_and_reset_clears_it() {
        CONTROLS.with(|cell| {
            *cell.borrow_mut() = Some(Controls {
                input: PlayerInput::preset("owned-lab", Preset::Shooter),
                yaw: 1.2,
                pitch: -0.4,
                menu: 1,
            });
        });
        ferravex_training_release_input();
        CONTROLS.with(|cell| {
            let controls = cell.borrow();
            let controls = controls.as_ref().unwrap();
            assert_eq!(controls.menu, 0);
            assert_eq!(controls.yaw, 1.2, "pause retains aim");
            assert_eq!(controls.pitch, -0.4);
        });
        reset_controls();
        CONTROLS.with(|cell| {
            let controls = cell.borrow();
            let controls = controls.as_ref().unwrap();
            assert_eq!(controls.yaw, 0.);
            assert_eq!(controls.pitch, 0.);
            assert!(!controls.input.gameplay_active());
            assert!(controls.input.bindings("fire").is_some());
        });
    }
}
