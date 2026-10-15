use ferravex_net::{BUTTON_FIRE, BUTTON_JUMP, BUTTON_RELOAD, InputFrame};
use ferravex_render::prelude::*;

use ferravex_render::input::{
    ActionKind, Binding, InputCode, PlayerInput, Preset,
    touch::{Control, Output, Region},
};

pub struct Controls {
    pub yaw: f32,
    pub pitch: f32,
    pub touch_mode: bool,
    pub input: PlayerInput,
    stick_origin: Vec2,
    pub stick_delta: Vec2,
}
impl Default for Controls {
    fn default() -> Self {
        let mut input = PlayerInput::preset("strike", Preset::Shooter);
        for (name, label, key) in [
            ("roster", "Players", KeyCode::Tab),
            ("diagnostics", "Diagnostics", KeyCode::F3),
        ] {
            input
                .global_action(
                    name,
                    label,
                    ActionKind::Button,
                    vec![Binding::digital(InputCode::Key(key as u16))],
                )
                .unwrap();
        }
        Self {
            yaw: 0.,
            pitch: 0.,
            touch_mode: false,
            input,
            stick_origin: Vec2::ZERO,
            stick_delta: Vec2::ZERO,
        }
    }
}
impl Controls {
    pub fn prepare(&mut self, active: bool) {
        let w = screen_width();
        let h = screen_height();
        self.input
            .set_touch_layout(&[
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
        self.input.sample(active);
        self.touch_mode = self.input.touch_seen();
        let (origin, delta) = self
            .input
            .touch_stick(0, 1)
            .unwrap_or((vec2(90., h - 108.), Vec2::ZERO));
        self.stick_origin = origin;
        self.stick_delta = delta;
    }
    pub fn sample(&mut self, active: bool) -> InputFrame {
        self.input.sample(active);
        let movement = self.input.movement();
        let look = self.input.look(get_frame_time());
        self.yaw = (self.yaw + look.x).rem_euclid(std::f32::consts::TAU);
        self.pitch = (self.pitch - look.y).clamp(-1.45, 1.45);
        let mut input = InputFrame {
            yaw: self.yaw,
            pitch: self.pitch,
            move_x: movement.x,
            move_z: movement.y,
            ..Default::default()
        };
        for (name, flag) in [
            ("fire", BUTTON_FIRE),
            ("jump", BUTTON_JUMP),
            ("reload", BUTTON_RELOAD),
        ] {
            let action = self.input.take(name);
            if action.pressed || action.value.abs() > 0.5 {
                input.buttons |= flag;
            }
        }
        input
    }
    pub fn draw_touch(&self) {
        if !self.touch_mode {
            return;
        }
        let w = screen_width();
        let h = screen_height();
        let c = Color::new(0.9, 0.92, 0.83, 0.45);
        let origin = self.stick_origin;
        draw_circle_lines(origin.x, origin.y, 55., 2., c);
        let thumb = origin + self.stick_delta * 42.;
        draw_circle(thumb.x, thumb.y, 18., Color::new(0.9, 0.92, 0.83, 0.22));
        for (x, y, r, label) in [
            (w - 74., h - 96., 46., "FIRE"),
            (w - 170., h - 66., 32., "UP"),
            (w - 64., h - 197., 29., "R"),
        ] {
            draw_circle_lines(x, y, r, 2., c);
            let size = measure_text(label, None, 17, 1.);
            draw_text(label, x - size.width * 0.5, y + 6., 17., c);
        }
    }
}
