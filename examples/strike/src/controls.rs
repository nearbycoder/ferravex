use ferravex_net::{BUTTON_FIRE, BUTTON_JUMP, BUTTON_RELOAD, InputFrame};
use ferravex_render::prelude::*;

use ferravex_render::input::{
    ActionKind, Binding, InputCode, PlayerInput, Preset,
    touch::{Control, Output, Region},
};

// The shared formatter owns binding/device labels. Long custom profiles remain
// bounded and direct the player to the full editor instead of showing stale keys.
pub(super) fn hint<'a>(input: &PlayerInput, action: &str, buffer: &'a mut [u8]) -> &'a str {
    input.format_hint(action, buffer).unwrap_or("Controls")
}

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
    pub fn cut(&mut self) {
        self.input.sample(false);
        self.interacting = false;
    }
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
        InputFrame {
            yaw: self.yaw,
            pitch: self.pitch,
            move_x: movement.x,
            move_z: movement.y,
            ..Default::default()
        }
    }
    /// Called only for an actual authority tick. The shared input journal keeps
    /// taps through zero-tick frames and retires edges after the first catch-up tick.
    pub fn tick(&mut self, mut input: InputFrame) -> InputFrame {
        for (action, flag) in self
            .input
            .take_tick(["fire", "jump", "reload"])
            .into_iter()
            .zip([BUTTON_FIRE, BUTTON_JUMP, BUTTON_RELOAD])
        {
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
            draw_text("USE", w - 181., h - 166., 17., c);
        }
        for (x, y, r, label) in [
            (w - 74., h - 96., 46., "FIRE"),
            (w - 170., h - 66., 32., "UP"),
            (w - 64., h - 197., 29., "LOAD"),
        ] {
            draw_circle_lines(x, y, r, 2., c);
            let size = measure_text(label, None, 17, 1.);
            draw_text(label, x - size.width * 0.5, y + 6., 17., c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_render::input::{
        host_frame,
        touch::{Event, Phase},
        touch_event,
    };

    fn controls() -> Controls {
        let mut input = PlayerInput::transient("journey200-strike-controls");
        for (name, id) in [("fire", 2), ("jump", 3), ("reload", 5)] {
            input
                .action(
                    name,
                    name,
                    ActionKind::Button,
                    vec![Binding::digital(InputCode::Touch(id))],
                )
                .unwrap();
        }
        Controls {
            input,
            ..Default::default()
        }
    }
    fn touch(phase: Phase) {
        touch_event(Event {
            id: 141,
            phase,
            position: vec2(726., 504.),
        });
    }

    #[test]
    fn short_taps_survive_zero_ticks_but_not_settings_or_pause() {
        let mut controls = controls();
        let mut host = crate::host::Host::new(true);
        let dt = 1. / ferravex_fps::TICK_RATE as f64;
        host_frame(0., 800, 600, || controls.prepare(true));
        touch(Phase::Start);
        touch(Phase::End);
        host_frame((dt / 2.) as f32, 800, 600, || {
            controls.prepare(true);
            let input = controls.sample(true);
            assert_eq!(input.buttons, 0, "render sampling must not consume fire");
            assert_eq!(
                host.advance(crate::host::Mode::Practice, dt / 2., true, true)
                    .steps,
                0
            );
        });
        host_frame((dt * 2.5) as f32, 800, 600, || {
            controls.prepare(true);
            let input = controls.sample(true);
            let frame = host.advance(crate::host::Mode::Practice, dt * 2.5, true, true);
            assert_eq!(frame.steps, 3);
            let buttons: Vec<_> = (0..frame.steps)
                .map(|_| controls.tick(input).buttons)
                .collect();
            assert_eq!(buttons, [BUTTON_FIRE, 0, 0]);
        });
        // A held trigger continues firing across catch-up ticks.
        touch(Phase::Start);
        host_frame(0., 800, 600, || {
            controls.prepare(true);
            let input = controls.sample(true);
            for _ in 0..3 {
                assert_eq!(controls.tick(input).buttons, BUTTON_FIRE);
            }
            // Reconcile a local menu in the same render frame: stale held
            // buttons cannot reactivate when the menu closes.
            controls.prepare(false);
            controls.prepare(true);
            assert_eq!(controls.tick(input).buttons, 0);
        });
        touch(Phase::End);
        host_frame(0., 800, 600, || controls.prepare(true));
        touch(Phase::Start);
        touch(Phase::End);
        host_frame(0., 800, 600, || {
            controls.prepare(true);
            controls.input.set_controls_open(true);
            assert_eq!(controls.tick(InputFrame::default()).buttons, 0);
            controls.input.set_controls_open(false);
            assert_eq!(controls.tick(InputFrame::default()).buttons, 0);
        });
        // A seek/restart cut discards pending gameplay edges as well.
        touch(Phase::Start);
        touch(Phase::End);
        host_frame(0., 800, 600, || {
            controls.prepare(true);
            controls.cut();
            controls.sample(true);
            assert_eq!(controls.tick(InputFrame::default()).buttons, 0);
        });
    }

    #[test]
    fn hints_follow_remaps_and_unbound_devices_without_consuming_taps() {
        let mut controls = controls();
        host_frame(0., 800, 600, || controls.prepare(true));
        controls
            .input
            .rebind(
                "reload",
                vec![Binding::digital(InputCode::Key(KeyCode::Q as u16))],
            )
            .unwrap();
        let mut text = [0; 64];
        assert_eq!(hint(&controls.input, "reload", &mut text), "Q");
        controls.input.rebind("reload", vec![]).unwrap();
        assert_eq!(hint(&controls.input, "reload", &mut text), "Unbound");
        assert_eq!(hint(&controls.input, "missing", &mut text), "Controls");
        touch(Phase::Start);
        touch(Phase::End);
        host_frame(0., 800, 600, || {
            controls.prepare(true);
            assert_eq!(
                controls.input.current_device(),
                ferravex_render::input::HintDevice::Touch
            );
            assert_eq!(hint(&controls.input, "fire", &mut text), "Touch 2");
            assert_eq!(hint(&controls.input, "fire", &mut []), "Controls");
            assert_eq!(controls.tick(InputFrame::default()).buttons, BUTTON_FIRE);
            assert_eq!(controls.tick(InputFrame::default()).buttons, 0);
        });
    }
}
