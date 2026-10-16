//! The custom WebGL host shares Rust profiles and keeps its live simulation clock.
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
    pending: u8,
    menu: u32,
}
thread_local! {static CONTROLS:RefCell<Option<Controls>>=const {RefCell::new(None)};}
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
                pending: 0,
                menu: 0,
            });
            let p = &mut controls.input;
            let w = width.max(1) as f32;
            let h = height.max(1) as f32;
            p.set_touch_layout(&[
                Control {
                    region: Region::Circle {
                        center: vec2(w - 74., h - 96.),
                        radius: 48.,
                    },
                    output: Output::Button(2),
                },
                Control {
                    region: Region::Circle {
                        center: vec2(w - 170., h - 66.),
                        radius: 34.,
                    },
                    output: Output::Button(3),
                },
                Control {
                    region: Region::Circle {
                        center: vec2(w - 64., h - 197.),
                        radius: 31.,
                    },
                    output: Output::Button(5),
                },
                Control {
                    region: Region::Rect(Rect::new(0., 0., w * 0.42, h)),
                    output: Output::Stick {
                        x: 0,
                        y: 1,
                        radius: 54.,
                        floating: true,
                    },
                },
                Control {
                    region: Region::Rect(Rect::new(w * 0.42, 0., w * 0.58, h)),
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
            let active = p.gameplay_active() && controls.menu != 1;
            let look = p.look(dt as f32);
            controls.yaw = (controls.yaw + look.x).rem_euclid(std::f32::consts::TAU);
            controls.pitch = (controls.pitch - look.y).clamp(-1.5, 1.5);
            let movement = p.movement();
            let mut buttons = 0;
            for (name, bit) in [
                ("fire", ferravex_fps::BUTTON_FIRE),
                ("jump", ferravex_fps::BUTTON_JUMP),
                ("reload", ferravex_fps::BUTTON_RELOAD),
            ] {
                let action = p.take(name);
                if action.pressed || action.value > 0.5 {
                    buttons |= bit;
                }
            }
            if active {
                controls.pending |= buttons;
            } else {
                controls.pending = 0;
            }
            let input = InputFrame {
                move_x: if active { movement.x } else { 0. },
                move_z: if active { movement.y } else { 0. },
                yaw: controls.yaw,
                pitch: controls.pitch,
                buttons: controls.pending,
                ..Default::default()
            };
            if let Some(lab) = LAB.lock().unwrap().as_mut() {
                let tick = lab.game.tick;
                lab.update(dt, input, view, aspect);
                if lab.game.tick != tick {
                    controls.pending = 0;
                }
            }
            if playing != 0 && p.touch_seen() && !p.touch_customized() && !p.controls_open() {
                let tint = render::Color::new(0.9, 0.92, 0.83, 0.45);
                let (origin, delta) = p
                    .touch_stick(0, 1)
                    .unwrap_or((vec2(90., h - 108.), Vec2::ZERO));
                render::draw_circle_lines(origin.x, origin.y, 54., 2., tint);
                render::draw_circle(
                    origin.x + delta.x * 38.,
                    origin.y + delta.y * 38.,
                    16.,
                    tint,
                );
                for (x, y, r, label) in [
                    (w - 74., h - 96., 46., "FIRE"),
                    (w - 170., h - 66., 32., "UP"),
                    (w - 64., h - 197., 29., "R"),
                ] {
                    render::draw_circle_lines(x, y, r, 2., tint);
                    let size = render::measure_text(label, None, 17, 1.);
                    render::draw_text(label, x - size.width * 0.5, y + 6., 17., tint);
                }
            }
            p.draw_settings();
        })
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_menu_requested() -> u32 {
    CONTROLS.with(|cell| cell.borrow().as_ref().map_or(0, |c| c.menu))
}
