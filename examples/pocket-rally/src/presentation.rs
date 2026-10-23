//! Presentation helpers; none advances the race or modifies its records.
use ferravex_play::{
    particles::Trail,
    persistence::{RecordComparison, RecordDelta},
};
use ferravex_render::{
    input::{
        HintDevice, InputCode, PlayerInput,
        touch::{Control, Output, Region},
    },
    prelude::*,
};

// Channel order also identifies saved layout slots. Share the visible labels
// with the hints so a touch player sees GAS rather than an internal channel ID.
pub(crate) const TOUCH_BUTTONS: [(u8, &str); 4] = [(12, "<"), (13, ">"), (14, "GAS"), (6, "BRAKE")];

/// Read the host policy on *every* render frame, including pause and zero ticks.
/// Existing skid geometry is discarded, so restoring motion cannot revive it.
/// There are no camera impulses, particles or screen flashes in this game.
pub(crate) fn sync_marks(marks: &mut [Trail; 2]) -> bool {
    let policy = ferravex_render::preferences::feedback_policy();
    let enabled = !policy.reduced_motion() && policy.intensity() > 0.;
    if !enabled {
        for trail in marks {
            trail.clear();
        }
    }
    enabled
}

pub(crate) fn touch_layout(width: f32, height: f32) -> [Control; 4] {
    let lane = (width / 4.).min(100.);
    let top = (height - 130.).max(0.);
    let extent = height.min(130.);
    // Keep original control indexes: saved touch customization addresses them.
    let positions = [0., lane, width - lane, width - lane * 2.];
    std::array::from_fn(|i| Control {
        region: Region::Rect(Rect::new(positions[i], top, lane, extent)),
        output: Output::Button(TOUCH_BUTTONS[i].0),
    })
}

fn hint_for(input: &PlayerInput, name: &str, device: HintDevice) -> String {
    let mut buffer = [0; 256];
    let Ok(label) = input.format_hint_for(name, Some(device), &mut buffer) else {
        return "See Controls".to_owned();
    };
    // Only name buttons that this game actually draws. Customized layouts hide
    // these labels, and unknown channels/gestures must retain the shared hint.
    if device == HintDevice::Touch
        && !input.touch_customized()
        && let Some(binding) = input.bindings(name).and_then(|bindings| {
            bindings.iter().find(|binding| {
                binding.scale != 0.
                    && matches!(binding.code, InputCode::Touch(_) | InputCode::TouchDelta(_))
            })
        })
        && let InputCode::Touch(channel) = binding.code
        && let Some((_, visible)) = TOUCH_BUTTONS.iter().find(|(code, _)| *code == channel)
    {
        return if binding.scale == 1. {
            (*visible).to_owned()
        } else {
            format!("{visible} x{}", binding.scale)
        };
    }
    label.split(" / ").next().unwrap_or(label).to_owned()
}

pub(crate) fn retry_hint(input: &PlayerInput) -> String {
    retry_hint_for(input, input.current_device())
}

fn retry_hint_for(input: &PlayerInput, device: HintDevice) -> String {
    if device == HintDevice::Touch {
        // The result accepts a canvas tap. The driving layout does not contain
        // the preset's Touch 9/10 restart/confirm channels.
        "Tap to retry".to_owned()
    } else {
        format!(
            "{} / {} / click to retry",
            hint_for(input, "restart", device),
            hint_for(input, "confirm", device)
        )
    }
}

pub(crate) fn text_width(label: &str, size: f32) -> f32 {
    get_default_font()
        .try_metrics(8, size / 8.)
        .and_then(|metrics| metrics.measure_line(label))
        .map_or(0., |measure| measure.width)
}

pub(crate) fn lap_comparison(comparison: Option<RecordDelta>, best: bool) -> String {
    let Some(comparison) = comparison else {
        return if best { "NEW BEST" } else { "LAP COMPLETE" }.to_owned();
    };
    match (comparison.comparison, comparison.delta) {
        (_, None) => "FIRST BEST".to_owned(),
        (RecordComparison::Tie, _) => "TIED BEST".to_owned(),
        (RecordComparison::NewBest, Some(delta)) => {
            format!("NEW BEST  {:+.2}s", delta as f64 / 1_000_000.)
        }
        (RecordComparison::Worse, Some(delta)) => {
            format!("{:+.2}s TO BEST", delta as f64 / 1_000_000.)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_play::{
        feedback::FeedbackPolicy,
        particles::{TrailConfig, TrailSample},
    };
    use ferravex_render::{
        input::{ActionKind, Binding, InputCode, touch},
        preferences,
    };

    #[test]
    fn live_policy_changes_during_pause_and_zero_tick_frames_leave_race_identical() {
        use crate::{
            session::RaceSession,
            sim::{Controls, Race, Track},
        };
        let previous = preferences::feedback_policy();
        let run = |changing: bool| {
            let track = Track::new();
            let mut race = Race::new(&track);
            let mut session = RaceSession::new();
            let mut marks = std::array::from_fn(|_| Trail::new(TrailConfig::default()).unwrap());
            for i in 0..1000 {
                let reduced = changing && (i / 31) % 2 == 0;
                preferences::set_feedback_policy(
                    FeedbackPolicy::new(reduced, changing && i % 2 == 0, 1.).unwrap(),
                );
                sync_marks(&mut marks);
                let active = !(400..450).contains(&i);
                let frame = session.advance(1. / 240., active);
                for _ in 0..frame.steps {
                    let controls = if session.countdown() == 0 {
                        race.autopilot(&track)
                    } else {
                        Controls::default()
                    };
                    session.step(&mut race, &track, controls, frame.dt);
                }
            }
            (
                race.car.pose.position,
                race.car.velocity,
                race.ticks,
                race.total_time,
                race.laps,
                race.next_gate,
            )
        };
        assert_eq!(run(false), run(true));
        preferences::set_feedback_policy(previous);
    }

    #[test]
    fn motion_clears_old_skids_without_ticks_and_flash_is_independent() {
        let previous = preferences::feedback_policy();
        let mut marks = std::array::from_fn(|_| Trail::new(TrailConfig::default()).unwrap());
        for trail in &mut marks {
            trail
                .update(
                    1. / 120.,
                    Some(TrailSample {
                        position: [10., 20., 0.],
                        color: [0.2; 4],
                        width: 2.,
                    }),
                )
                .unwrap();
        }
        preferences::set_feedback_policy(FeedbackPolicy::new(false, true, 1.).unwrap());
        assert!(sync_marks(&mut marks));
        assert!(marks.iter().all(|trail| !trail.is_empty()));
        preferences::set_feedback_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
        assert!(!sync_marks(&mut marks));
        assert!(marks.iter().all(Trail::is_empty));
        preferences::set_feedback_policy(FeedbackPolicy::default());
        assert!(sync_marks(&mut marks));
        assert!(marks.iter().all(Trail::is_empty));
        preferences::set_feedback_policy(previous);
    }

    #[test]
    fn portrait_controls_fit_without_overlapping_opposite_inputs() {
        for width in [240., 320., 390., 540., 1024.] {
            let controls = touch_layout(width, 600.);
            let mut rects = controls.map(|control| match control.region {
                Region::Rect(rect) => rect,
                _ => panic!("rally uses rectangular controls"),
            });
            rects.sort_by(|a, b| a.x.total_cmp(&b.x));
            for (index, rect) in rects.iter().enumerate() {
                assert!(rect.x >= 0. && rect.x + rect.w <= width);
                assert_eq!((rect.y, rect.h), (470., 130.));
                if index > 0 {
                    assert!(rects[index - 1].x + rects[index - 1].w <= rect.x);
                }
            }
            assert!(matches!(controls[2].output, Output::Button(14)));
            assert!(matches!(controls[3].output, Output::Button(6)));
            // Exercise actual capture, including simultaneous left + brake on
            // narrow screens where the old 100px regions overlapped.
            use ferravex_play::input::{
                ActionMap, ContextId,
                touch::{Event, Phase, TouchSurface},
            };
            let mut map = ActionMap::default();
            let ids = [12, 13, 14, 6].map(|code| {
                map.named_action(
                    &format!("button-{code}"),
                    ActionKind::Button,
                    ContextId::GLOBAL,
                    vec![Binding::digital(InputCode::Touch(code))],
                )
                .unwrap()
            });
            let mut touch = TouchSurface::default();
            touch.set_layout(&controls, &mut map).unwrap();
            for (i, control) in controls.iter().enumerate() {
                let Region::Rect(rect) = control.region else {
                    unreachable!()
                };
                touch
                    .handle(
                        Event {
                            id: i as u64,
                            position: rect.center(),
                            phase: Phase::Start,
                        },
                        &mut map,
                    )
                    .unwrap();
                let state = map.consume_tick(ids[i]);
                assert!(state.pressed);
                assert_eq!(state.value, 1.);
            }
            touch.clear(&mut map);
            assert!(ids.into_iter().all(|id| map.consume_tick(id).value == 0.));
        }
    }

    #[test]
    fn hints_follow_remaps_and_unbound_actions_without_default_key_claims() {
        let mut input = PlayerInput::transient("rally-hint-test");
        let binding = |key| Binding {
            code: InputCode::Key(key),
            scale: 1.,
            deadzone: 0.,
        };
        input
            .action(
                "brake",
                "Handbrake",
                ActionKind::Axis,
                vec![binding(KeyCode::Space as u16)],
            )
            .unwrap();
        assert_eq!(hint_for(&input, "brake", input.current_device()), "Space");
        input
            .rebind("brake", vec![binding(KeyCode::H as u16)])
            .unwrap();
        assert_eq!(hint_for(&input, "brake", input.current_device()), "H");
        input.rebind("brake", vec![]).unwrap();
        assert_eq!(hint_for(&input, "brake", input.current_device()), "Unbound");
    }

    #[test]
    fn touch_hints_name_visible_buttons_and_follow_actual_remaps() {
        let mut input = PlayerInput::transient("rally-visible-touch-hints");
        input
            .action(
                "move_forward",
                "Gas",
                ActionKind::Axis,
                vec![
                    Binding::digital(InputCode::Key(KeyCode::W as u16)),
                    Binding::digital(InputCode::Touch(14)),
                ],
            )
            .unwrap();
        assert_eq!(hint_for(&input, "move_forward", HintDevice::Touch), "GAS");
        assert_eq!(
            hint_for(&input, "move_forward", input.current_device()),
            "W"
        );
        for (channel, label) in TOUCH_BUTTONS {
            input
                .rebind(
                    "move_forward",
                    vec![Binding::digital(InputCode::Touch(channel))],
                )
                .unwrap();
            assert_eq!(hint_for(&input, "move_forward", HintDevice::Touch), label);
        }
        input
            .rebind(
                "move_forward",
                vec![
                    Binding {
                        scale: 0.,
                        ..Binding::digital(InputCode::Touch(14))
                    },
                    Binding {
                        scale: -0.5,
                        ..Binding::digital(InputCode::Touch(6))
                    },
                ],
            )
            .unwrap();
        assert_eq!(
            hint_for(&input, "move_forward", HintDevice::Touch),
            "BRAKE x-0.5"
        );
        input
            .set_touch_settings(touch::TouchLayoutSettings {
                mirrored: true,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            hint_for(&input, "move_forward", HintDevice::Touch),
            "Touch 6 x-0.5"
        );
        input.set_touch_settings(Default::default()).unwrap();
        input
            .rebind("move_forward", vec![Binding::digital(InputCode::Touch(7))])
            .unwrap();
        assert_eq!(
            hint_for(&input, "move_forward", HintDevice::Touch),
            "Touch 7"
        );
        input.rebind("move_forward", vec![]).unwrap();
        assert_eq!(
            hint_for(&input, "move_forward", HintDevice::Touch),
            "Unbound"
        );
    }

    #[test]
    fn retry_hint_uses_the_available_tap_and_live_keyboard_bindings() {
        let mut input = PlayerInput::transient("rally-retry-hints");
        for (name, key, channel) in [("restart", KeyCode::R, 9), ("confirm", KeyCode::Enter, 10)] {
            input
                .action(
                    name,
                    name,
                    ActionKind::Button,
                    vec![
                        Binding::digital(InputCode::Key(key as u16)),
                        Binding::digital(InputCode::Touch(channel)),
                    ],
                )
                .unwrap();
        }
        assert_eq!(retry_hint(&input), "R / Enter / click to retry");
        assert_eq!(retry_hint_for(&input, HintDevice::Touch), "Tap to retry");
        input
            .rebind(
                "restart",
                vec![Binding::digital(InputCode::Key(KeyCode::T as u16))],
            )
            .unwrap();
        assert_eq!(retry_hint(&input), "T / Enter / click to retry");
        assert_eq!(retry_hint_for(&input, HintDevice::Touch), "Tap to retry");
    }

    #[test]
    fn comparison_copy_distinguishes_first_tie_and_both_delta_signs() {
        use ferravex_play::persistence::RecordBook;
        let mut book = RecordBook::default();
        let first = book
            .record_with_summary("lap", 0, 0, Some(20_000_000))
            .unwrap();
        assert_eq!(lap_comparison(first.time_us, true), "FIRST BEST");
        for (micros, expected) in [
            (20_000_000, "TIED BEST"),
            (19_500_000, "NEW BEST  -0.50s"),
            (21_250_000, "+1.25s TO BEST"),
        ] {
            let summary = book.compare("lap", 0, 0, Some(micros)).unwrap();
            assert_eq!(
                lap_comparison(summary.time_us, micros < 20_000_000),
                expected
            );
        }
    }
}
