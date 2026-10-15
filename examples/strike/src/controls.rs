use ferravex_net::{BUTTON_FIRE, BUTTON_JUMP, BUTTON_RELOAD, InputFrame};
use macroquad::prelude::*;

#[derive(Default)]
pub struct Controls {
    pub yaw: f32,
    pub pitch: f32,
    pub touch_mode: bool,
    stick: Option<(u64, Vec2)>,
    look: Option<(u64, Vec2)>,
    pub stick_delta: Vec2,
}
impl Controls {
    pub fn sample(&mut self, active: bool) -> InputFrame {
        let mut input = InputFrame {
            yaw: self.yaw,
            pitch: self.pitch,
            ..Default::default()
        };
        if !active {
            self.stick = None;
            self.look = None;
            self.stick_delta = Vec2::ZERO;
            return input;
        }
        input.move_x = is_key_down(KeyCode::D) as u8 as f32 - is_key_down(KeyCode::A) as u8 as f32;
        input.move_z = is_key_down(KeyCode::W) as u8 as f32 - is_key_down(KeyCode::S) as u8 as f32;
        if is_key_down(KeyCode::Space) || is_key_pressed(KeyCode::Space) {
            input.buttons |= BUTTON_JUMP;
        }
        if is_key_down(KeyCode::R) || is_key_pressed(KeyCode::R) {
            input.buttons |= BUTTON_RELOAD;
        }
        if !self.touch_mode {
            let delta = mouse_delta_position();
            self.yaw -= delta.x * 1.9;
            self.pitch += delta.y * 1.9;
            if is_mouse_button_down(MouseButton::Left) || is_mouse_button_pressed(MouseButton::Left)
            {
                input.buttons |= BUTTON_FIRE;
            }
        }
        let w = screen_width();
        let h = screen_height();
        for t in ferravex_render::input::logical_touches() {
            self.touch_mode = true;
            let done = matches!(t.phase, TouchPhase::Ended | TouchPhase::Cancelled);
            if done {
                if self.stick.is_some_and(|(id, _)| id == t.id) {
                    self.stick = None;
                    self.stick_delta = Vec2::ZERO;
                }
                if self.look.is_some_and(|(id, _)| id == t.id) {
                    self.look = None;
                }
                continue;
            }
            if t.position.distance(vec2(w - 74., h - 96.)) < 48. {
                input.buttons |= BUTTON_FIRE;
                continue;
            }
            if t.position.distance(vec2(w - 170., h - 66.)) < 34. {
                input.buttons |= BUTTON_JUMP;
                continue;
            }
            if t.position.distance(vec2(w - 64., h - 197.)) < 31. {
                input.buttons |= BUTTON_RELOAD;
                continue;
            }
            if t.phase == TouchPhase::Started {
                if t.position.x < w * 0.42 && self.stick.is_none() {
                    self.stick = Some((t.id, t.position));
                } else if self.look.is_none() {
                    self.look = Some((t.id, t.position));
                }
            }
            if let Some((id, origin)) = self.stick
                && id == t.id
            {
                self.stick_delta = ((t.position - origin) / 54.).clamp_length_max(1.);
                input.move_x = self.stick_delta.x;
                input.move_z = -self.stick_delta.y;
            }
            if let Some((id, previous)) = self.look
                && id == t.id
            {
                let d = t.position - previous;
                self.yaw += d.x * 0.004;
                self.pitch -= d.y * 0.004;
                self.look = Some((id, t.position));
            }
        }
        self.pitch = self.pitch.clamp(-1.45, 1.45);
        self.yaw = self.yaw.rem_euclid(std::f32::consts::TAU);
        input.yaw = self.yaw;
        input.pitch = self.pitch;
        input
    }
    pub fn draw_touch(&self) {
        if !self.touch_mode {
            return;
        }
        let w = screen_width();
        let h = screen_height();
        let c = Color::new(0.9, 0.92, 0.83, 0.45);
        let origin = self.stick.map(|(_, p)| p).unwrap_or(vec2(90., h - 108.));
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
