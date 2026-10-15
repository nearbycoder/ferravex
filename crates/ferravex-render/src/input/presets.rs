use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Platformer,
    Shooter,
    Explore,
    HoverRacer,
    Racer,
    Flight,
    TopDown,
    Pinball,
    Turret,
    Strategy,
    Lab2,
    Lab3,
}
fn b(code: InputCode) -> Binding {
    Binding::digital(code)
}
fn key(code: KeyCode) -> Binding {
    b(InputCode::Key(code as u16))
}
impl PlayerInput {
    pub fn preset(owner: &str, preset: Preset) -> Self {
        let mut p = Self::new(owner);
        let mut add =
            |name: &str, label: &str, kind: ActionKind, bindings: Vec<Binding>, global: bool| {
                if global {
                    p.global_action(name, label, kind, bindings)
                } else {
                    p.action(name, label, kind, bindings)
                }
                .expect("valid built-in input profile");
            };
        let directional = matches!(
            preset,
            Preset::Platformer
                | Preset::Shooter
                | Preset::Explore
                | Preset::HoverRacer
                | Preset::Racer
                | Preset::Flight
                | Preset::TopDown
                | Preset::Lab3
                | Preset::Turret
                | Preset::Strategy
                | Preset::Lab2
        );
        if directional {
            for (name, label, a, z, pad) in [
                ("move_left", "Left", KeyCode::A, KeyCode::Left, 14),
                ("move_right", "Right", KeyCode::D, KeyCode::Right, 15),
            ] {
                add(
                    name,
                    label,
                    ActionKind::Axis,
                    vec![
                        key(a),
                        key(z),
                        b(InputCode::PadButton(pad)),
                        b(InputCode::Touch(if name == "move_left" { 12 } else { 13 })),
                    ],
                    false,
                );
            }
            add(
                "move_x",
                "Move / steer X",
                ActionKind::Axis,
                vec![b(InputCode::PadAxis(0)), b(InputCode::Touch(0))],
                false,
            );
        }
        if directional && preset != Preset::Platformer {
            for (name, label, a, z, pad) in [
                ("move_forward", "Forward / up", KeyCode::W, KeyCode::Up, 12),
                ("move_back", "Back / down", KeyCode::S, KeyCode::Down, 13),
            ] {
                let mut bindings = vec![
                    key(a),
                    key(z),
                    b(InputCode::PadButton(pad)),
                    b(InputCode::Touch(if name == "move_forward" {
                        14
                    } else {
                        15
                    })),
                ];
                if matches!(preset, Preset::HoverRacer | Preset::Racer) && name == "move_forward" {
                    bindings.push(b(InputCode::PadButton(7)));
                }
                add(name, label, ActionKind::Axis, bindings, false);
            }
            add(
                "move_y",
                "Vertical axis",
                ActionKind::Axis,
                vec![b(InputCode::PadAxis(1)), b(InputCode::Touch(1))],
                false,
            );
        }
        if matches!(
            preset,
            Preset::Shooter | Preset::Explore | Preset::Lab3 | Preset::Turret
        ) {
            add(
                "look_x",
                "Look right",
                ActionKind::Delta,
                vec![b(InputCode::MouseDelta(0)), b(InputCode::TouchDelta(0))],
                false,
            );
            add(
                "look_y",
                "Look down",
                ActionKind::Delta,
                vec![b(InputCode::MouseDelta(1)), b(InputCode::TouchDelta(1))],
                false,
            );
            add(
                "look_rate_x",
                "Look X stick",
                ActionKind::Axis,
                vec![b(InputCode::PadAxis(2))],
                false,
            );
            add(
                "look_rate_y",
                "Look Y stick",
                ActionKind::Axis,
                vec![b(InputCode::PadAxis(3))],
                false,
            );
        }
        if matches!(preset, Preset::Platformer | Preset::Shooter) {
            let mut bindings = vec![
                key(KeyCode::Space),
                b(InputCode::PadButton(0)),
                b(InputCode::Touch(3)),
            ];
            if preset == Preset::Platformer {
                bindings.extend([key(KeyCode::W), key(KeyCode::Up)]);
            }
            add("jump", "Jump", ActionKind::Button, bindings, false);
        }
        if preset == Preset::Shooter {
            add(
                "reload",
                "Reload",
                ActionKind::Button,
                vec![
                    key(KeyCode::R),
                    b(InputCode::PadButton(3)),
                    b(InputCode::Touch(5)),
                ],
                false,
            );
        }
        if matches!(preset, Preset::Shooter | Preset::TopDown | Preset::Turret) {
            let mut bindings = vec![b(InputCode::PadButton(7)), b(InputCode::Touch(2))];
            if preset == Preset::Turret {
                bindings.push(key(KeyCode::Space));
            } else {
                bindings.push(b(InputCode::Mouse(1)));
            }
            add("fire", "Fire", ActionKind::Button, bindings, false);
        }
        if preset == Preset::Explore {
            add(
                "interact",
                "Interact",
                ActionKind::Button,
                vec![
                    key(KeyCode::E),
                    b(InputCode::PadButton(2)),
                    b(InputCode::Touch(4)),
                ],
                false,
            );
        }
        if matches!(preset, Preset::Racer | Preset::Flight) {
            let mut bindings = vec![
                key(if preset == Preset::Flight {
                    KeyCode::LeftShift
                } else {
                    KeyCode::Space
                }),
                b(InputCode::PadButton(6)),
                b(InputCode::Touch(6)),
            ];
            if preset == Preset::Flight {
                bindings.push(key(KeyCode::RightShift));
            }
            add("brake", "Brake / slow", ActionKind::Axis, bindings, false);
        }
        if matches!(
            preset,
            Preset::HoverRacer | Preset::Flight | Preset::TopDown
        ) {
            let mut bindings = vec![
                key(KeyCode::Space),
                b(InputCode::PadButton(0)),
                b(InputCode::Touch(7)),
            ];
            if preset != Preset::Flight {
                bindings.extend([key(KeyCode::LeftShift), key(KeyCode::RightShift)]);
            }
            add("boost", "Boost / dash", ActionKind::Button, bindings, false);
        }
        if preset == Preset::Pinball {
            for (name, label, a, z, pad, touch) in [
                (
                    "flipper_left",
                    "Left flipper",
                    KeyCode::A,
                    KeyCode::Left,
                    4,
                    0,
                ),
                (
                    "flipper_right",
                    "Right flipper",
                    KeyCode::D,
                    KeyCode::Right,
                    5,
                    1,
                ),
            ] {
                add(
                    name,
                    label,
                    ActionKind::Button,
                    vec![
                        key(a),
                        key(z),
                        b(InputCode::PadButton(pad)),
                        b(InputCode::Touch(touch)),
                    ],
                    false,
                );
            }
            add(
                "launch",
                "Launch",
                ActionKind::Button,
                vec![
                    key(KeyCode::Space),
                    b(InputCode::PadButton(0)),
                    b(InputCode::Touch(8)),
                ],
                false,
            );
        }
        if matches!(preset, Preset::Strategy | Preset::Lab2) {
            add(
                "primary",
                "Point / select",
                ActionKind::Button,
                vec![b(InputCode::Mouse(1)), b(InputCode::PadButton(0))],
                false,
            );
        }
        if preset == Preset::Lab2 {
            add(
                "secondary",
                "Spawn crate",
                ActionKind::Button,
                vec![b(InputCode::Mouse(3)), b(InputCode::PadButton(1))],
                false,
            );
        }
        if matches!(preset, Preset::Strategy | Preset::Lab2 | Preset::Lab3) {
            add(
                "select_one",
                if preset == Preset::Strategy {
                    "Build seed"
                } else if preset == Preset::Lab3 {
                    "Drop crate"
                } else {
                    "Choose ball"
                },
                ActionKind::Button,
                vec![
                    key(KeyCode::Key1),
                    b(InputCode::PadButton(if preset == Preset::Lab3 {
                        0
                    } else {
                        2
                    })),
                ],
                false,
            );
            add(
                "select_two",
                if preset == Preset::Strategy {
                    "Build frost"
                } else if preset == Preset::Lab3 {
                    "Drop ball"
                } else {
                    "Choose crate"
                },
                ActionKind::Button,
                vec![
                    key(KeyCode::Key2),
                    b(InputCode::PadButton(if preset == Preset::Lab3 {
                        1
                    } else {
                        3
                    })),
                ],
                false,
            );
        }
        if preset == Preset::Strategy {
            add(
                "upgrade",
                "Upgrade",
                ActionKind::Button,
                vec![key(KeyCode::U), b(InputCode::PadButton(5))],
                false,
            );
        }
        if preset == Preset::TopDown {
            for (name, label, axis) in [("aim_x", "Aim stick X", 2), ("aim_y", "Aim stick Y", 3)] {
                add(
                    name,
                    label,
                    ActionKind::Axis,
                    vec![b(InputCode::PadAxis(axis))],
                    false,
                );
            }
        }
        if preset == Preset::Lab3 {
            add(
                "zoom",
                "Zoom",
                ActionKind::Delta,
                vec![b(InputCode::Wheel(1))],
                false,
            );
        }
        if preset == Preset::Lab3 {
            add(
                "zoom_rate",
                "Zoom stick triggers",
                ActionKind::Axis,
                vec![
                    b(InputCode::PadButton(6)),
                    Binding {
                        code: InputCode::PadButton(7),
                        scale: -1.,
                        deadzone: 0.,
                    },
                ],
                false,
            );
        }
        if preset == Preset::Strategy {
            add(
                "wave",
                "Start wave",
                ActionKind::Button,
                vec![key(KeyCode::Space), b(InputCode::PadButton(4))],
                false,
            );
        }
        if preset != Preset::Shooter {
            add(
                "restart",
                "Restart",
                ActionKind::Button,
                vec![
                    key(KeyCode::R),
                    b(InputCode::PadButton(8)),
                    b(InputCode::Touch(9)),
                ],
                true,
            );
        }
        if !matches!(preset, Preset::Lab2 | Preset::Lab3) {
            add(
                "confirm",
                "Play / confirm",
                ActionKind::Button,
                vec![
                    key(KeyCode::Enter),
                    b(InputCode::PadButton(0)),
                    b(InputCode::Touch(10)),
                ],
                true,
            );
        }
        let pause_key = if matches!(preset, Preset::Lab2 | Preset::Lab3) {
            KeyCode::Space
        } else {
            KeyCode::Escape
        };
        add(
            "pause",
            "Pause / menu",
            ActionKind::Button,
            vec![
                key(pause_key),
                b(InputCode::PadButton(9)),
                b(InputCode::Touch(11)),
            ],
            true,
        );
        p
    }
}
