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
    pub show_interact: bool,
    interacting: bool,
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
            ("record", "Record practice", KeyCode::F4),
            ("replay", "Watch last replay", KeyCode::F5),
            ("ghost", "Show last-run ghost", KeyCode::F6),
            ("replay_pause", "Pause replay", KeyCode::P),
            ("replay_back", "Seek back one second", KeyCode::Left),
            ("replay_forward", "Seek forward one second", KeyCode::Right),
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
        for (name, label, key) in [
            ("buy_rifle", "Buy rifle", KeyCode::B),
            ("buy_sniper", "Buy sniper", KeyCode::N),
            ("buy_armor", "Buy armor", KeyCode::V),
            ("buy_kit", "Buy defuse kit", KeyCode::K),
            ("drop_bomb", "Drop bomb", KeyCode::G),
        ] {
            input
                .action(
                    name,
                    label,
                    ActionKind::Button,
                    vec![Binding::digital(InputCode::Key(key as u16))],
                )
                .unwrap();
        }
        input
            .action(
                "interact",
                "Hold to plant / defuse / pick up",
                ActionKind::Button,
                vec![
                    Binding::digital(InputCode::Key(KeyCode::E as u16)),
                    Binding::digital(InputCode::PadButton(3)),
                    Binding::digital(InputCode::Touch(4)),
                ],
            )
            .unwrap();
        Self {
            yaw: 0.,
            pitch: 0.,
            touch_mode: false,
            show_interact: false,
            interacting: false,
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
        let mut layout = [
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
                region: Region::Circle {
                    center: vec2(w - 166., h - 172.),
                    radius: 31.,
                },
                output: Output::Button(4),
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
        ];
        let count = if self.show_interact {
            6
        } else {
            layout.swap(3, 5);
            5
        };
        self.input.set_touch_layout(&layout[..count]).unwrap();
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
    pub fn tactical_actions(
        &mut self,
        view: Option<&ferravex_fps::tactical::View>,
        local: &ferravex_fps::Player,
        active: bool,
    ) -> [Option<ferravex_play::rounds::Action>; 8] {
        use ferravex_play::rounds::{Action, Bomb, Team, Weapon};
        let mut out = [None; 8];
        let interact = self.input.take("interact");
        if !active || !self.input.gameplay_active() {
            if self.interacting {
                out[6] = Some(Action::Cancel);
                self.interacting = false;
            }
            return out;
        }
        let Some(view) = view else {
            return out;
        };
        for (slot, (name, action)) in out.iter_mut().zip([
            ("buy_rifle", Action::BuyWeapon(Weapon::Rifle)),
            ("buy_sniper", Action::BuyWeapon(Weapon::Sniper)),
            ("buy_armor", Action::BuyArmor),
            ("buy_kit", Action::BuyDefuseKit),
            ("drop_bomb", Action::DropBomb),
        ]) {
            if self.input.pressed(name) {
                *slot = Some(action);
            }
        }
        if interact.pressed
            && let Some(player) = view.player(local.id)
        {
            out[5] = match view.bomb {
                Bomb::Carried(id) if id == u16::from(local.id) && player.team == Team::Attack => {
                    Some(Action::Plant(usize::from(local.position.x < 0.)))
                }
                Bomb::Dropped(_) if player.team == Team::Attack => Some(Action::PickUpBomb),
                Bomb::Planted { .. } if player.team == Team::Defend => Some(Action::Defuse),
                _ => None,
            };
        }
        if interact.pressed {
            self.interacting = matches!(out[5], Some(Action::Plant(_) | Action::Defuse));
        }
        if interact.released {
            out[6] = Some(Action::Cancel);
            self.interacting = false;
        }
        out
    }
    pub fn draw_touch(&self) {
        if !self.touch_mode || self.input.touch_customized() {
            return;
        }
        let w = screen_width();
        let h = screen_height();
        let c = Color::new(0.9, 0.92, 0.83, 0.45);
        let origin = self.stick_origin;
        draw_circle_lines(origin.x, origin.y, 55., 2., c);
        let thumb = origin + self.stick_delta * 42.;
        draw_circle(thumb.x, thumb.y, 18., Color::new(0.9, 0.92, 0.83, 0.22));
        if self.show_interact {
            draw_circle_lines(w - 166., h - 172., 29., 2., c);
            draw_text("E", w - 172., h - 166., 17., c);
        }
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
