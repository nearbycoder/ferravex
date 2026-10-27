//! Interactive host adapter; the headless/software simulation keeps its API.
use super::*;
use ferravex_engine::render::audio::{Bus, Cue, GameAudio, VoiceParams};
use ferravex_engine::render::storage::{Records, run_micros};
use ferravex_engine::render::{
    self,
    input::{
        ActionKind, PlayerInput,
        touch::{Control, Output, Region},
    },
    prelude::{KeyCode, Rect, vec2},
};
use std::cell::RefCell;
thread_local! {static SOUND:RefCell<Option<GameAudio>>=const {RefCell::new(None)};static CONTROLS:RefCell<Option<PlayerInput>>=const {RefCell::new(None)};
// Personal bests are optional: `Some(None)` means storage was unavailable.
static RECORDS:RefCell<Option<Option<Records>>>=const {RefCell::new(None)};}
/// Records context for the two-sector campaign scored by these rules.
const RECORD_RULES: u64 = 0x4f52_4249_0002;
const RECORD_ID: &str = "campaign";

/// Open records once (on the first host frame) and record each finished run.
fn sync_records(app: &mut Orbit) {
    RECORDS.with(|cell| {
        let mut cell = cell.borrow_mut();
        let records = cell.get_or_insert_with(|| {
            let records = Records::new("owned-orbit").ok();
            let best = records
                .as_ref()
                .and_then(|r| r.get(RECORD_ID, RECORD_RULES))
                .map(|record| record.score.min(u64::from(u32::MAX)) as u32);
            app.set_personal_best(best);
            records
        });
        if let Some(run) = app.take_finished_run()
            && let Some(records) = records.as_mut()
        {
            let time = if run.won {
                run_micros(run.seconds)
            } else {
                None
            };
            let _ = records.attempt(RECORD_ID, RECORD_RULES, u64::from(run.score), time);
        }
    });
}
fn profile() -> PlayerInput {
    let mut p = PlayerInput::new("owned-orbit");
    for (name, label, codes, global) in [
        (
            "move_left",
            "Left",
            vec![
                InputCode::Key(KeyCode::A as u16),
                InputCode::Key(KeyCode::Left as u16),
                InputCode::PadButton(14),
            ],
            false,
        ),
        (
            "move_right",
            "Right",
            vec![
                InputCode::Key(KeyCode::D as u16),
                InputCode::Key(KeyCode::Right as u16),
                InputCode::PadButton(15),
            ],
            false,
        ),
        ("move_x", "Paddle stick", vec![InputCode::PadAxis(0)], false),
        (
            "launch",
            "Launch / retry",
            vec![
                InputCode::Key(32),
                InputCode::Key(KeyCode::Enter as u16),
                InputCode::Mouse(1),
                InputCode::PadButton(0),
                InputCode::Touch(8),
            ],
            false,
        ),
        (
            "restart",
            "Restart",
            vec![InputCode::Key(KeyCode::R as u16), InputCode::PadButton(8)],
            true,
        ),
        (
            "pause",
            "Pause",
            vec![
                InputCode::Key(KeyCode::Escape as u16),
                InputCode::PadButton(9),
            ],
            true,
        ),
    ] {
        let bindings = codes.into_iter().map(Binding::digital).collect();
        let kind = if name.starts_with("move_") {
            ActionKind::Axis
        } else {
            ActionKind::Button
        };
        if global {
            p.global_action(name, label, kind, bindings)
        } else {
            p.action(name, label, kind, bindings)
        }
        .unwrap();
    }
    p
}
#[unsafe(no_mangle)]
pub extern "C" fn orbit_controls_frame(dt: f32, width: u32, height: u32) {
    render::input::host_frame(dt, width, height, || {
        CONTROLS.with(|cell| {
            let mut cell = cell.borrow_mut();
            let p = cell.get_or_insert_with(profile);
            p.set_touch_layout(&[Control {
                region: Region::Rect(Rect::new(0., 0., width.max(1) as f32, height.max(1) as f32)),
                output: Output::Button(8),
            }])
            .unwrap();
            p.sample_play();
            if let Some(app) = APP.lock().unwrap().as_mut() {
                let restart = p.pressed("restart");
                if restart {
                    p.set_paused(false);
                    p.sample(true);
                }
                let resumed = app.session_state() == SessionState::Paused && p.gameplay_active();
                app.set_paused(!p.gameplay_active());
                let mut axis = p.movement().x;
                let pointer = render::input::logical_touches()
                    .into_iter()
                    .find(|t| {
                        !matches!(
                            t.phase,
                            render::prelude::TouchPhase::Ended
                                | render::prelude::TouchPhase::Cancelled
                        )
                    })
                    .map(|t| t.position)
                    .or_else(|| {
                        render::is_mouse_button_down(render::prelude::MouseButton::Left)
                            .then(|| vec2(render::mouse_position().0, render::mouse_position().1))
                    });
                if p.gameplay_active()
                    && let Some(pointer) = pointer
                {
                    axis = ((pointer.x / width.max(1) as f32 * WIDTH as f32 - app.game.paddle_x)
                        / 18.)
                        .clamp(-1., 1.);
                }
                app.presentation.hints.update(p);
                // Sample edges exactly once, including zero-tick frames. Orbit
                // latches launch and consumes it only on the first catch-up tick.
                app.advance_input(
                    if resumed { 0. } else { f64::from(dt) },
                    Input {
                        axis,
                        launch: p.pressed("launch"),
                        restart,
                    },
                );
                sync_records(app);
                let chime = app.presentation.juice.chime;
                SOUND.with(|cell| {
                    let mut cell = cell.borrow_mut();
                    let sound = cell.get_or_insert_with(GameAudio::new);
                    for (cue, gain) in app.presentation.take_cues() {
                        match (cue, chime) {
                            // Panel chimes climb in pitch as a volley grows.
                            (Cue::Pickup, Some(pitch)) if pitch > 1. => {
                                let _ = ferravex_engine::render::audio::play(
                                    cue,
                                    VoiceParams {
                                        gain,
                                        pitch,
                                        priority: 2,
                                        bus: Bus::Effects,
                                        ..Default::default()
                                    },
                                );
                            }
                            _ => sound.cue(cue, gain),
                        }
                    }
                });
                app.render();
            }
            p.draw_controls();
        })
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn orbit_pause(value: u32) {
    CONTROLS.with(|cell| {
        cell.borrow_mut()
            .get_or_insert_with(profile)
            .set_paused(value != 0);
    });
    if let Some(app) = APP.lock().unwrap().as_mut() {
        app.set_paused(value != 0);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn orbit_paused() -> u32 {
    CONTROLS.with(|cell| u32::from(cell.borrow().as_ref().is_some_and(|p| p.paused())))
}
#[unsafe(no_mangle)]
pub extern "C" fn orbit_restart() {
    CONTROLS.with(|cell| {
        if let Some(p) = cell.borrow_mut().as_mut() {
            // Suppress a press that opened the menu or was held at restart.
            p.set_paused(true);
            p.set_paused(false);
        }
    });
    if let Some(app) = APP.lock().unwrap().as_mut() {
        app.restart();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::ControlHints;

    #[test]
    fn hints_follow_real_orbit_rebinds_unbound_actions_and_reset() {
        let mut input = profile();
        let mut hints = ControlHints::default();
        hints.update(&input);
        assert_eq!(hints.launch, "SPACE / ENTER / MOUSE 1");
        input
            .rebind("launch", vec![Binding::digital(InputCode::Key(74))])
            .unwrap();
        input
            .rebind("pause", vec![Binding::digital(InputCode::Key(80))])
            .unwrap();
        input
            .rebind("move_left", vec![Binding::digital(InputCode::Key(74))])
            .unwrap();
        hints.update(&input);
        assert_eq!(hints.launch, "J");
        assert_eq!(hints.pause, "P");
        assert!(hints.movement.starts_with("J /"));
        input.rebind("launch", Vec::new()).unwrap();
        hints.update(&input);
        assert_eq!(hints.launch, "UNBOUND");
        // Long remaps cannot create unbounded HUD strings or retain stale keys.
        input
            .rebind(
                "launch",
                (1..=64)
                    .map(|code| Binding::digital(InputCode::Key(code)))
                    .collect(),
            )
            .unwrap();
        hints.update(&input);
        assert_eq!(hints.launch, "SEE CONTROLS");
        input.reset_defaults();
        hints.update(&input);
        assert_eq!(hints.launch, "SPACE / ENTER / MOUSE 1");
    }

    #[test]
    fn readable_status_uses_the_same_current_bindings_without_sampling_or_consuming_input() {
        let mut input = profile();
        let mut app = Orbit::new();
        input
            .rebind("launch", vec![Binding::digital(InputCode::Key(74))])
            .unwrap();
        app.presentation.hints.update(&input);
        assert!(
            app.status_text_v1()
                .lines()
                .next()
                .unwrap()
                .contains("Launch: J.")
        );
        assert!(!app.status_text_v1().contains("SPACE"));
        for tick in 0..3840 {
            app.advance_input(
                1. / 120.,
                Input {
                    axis: 1.,
                    launch: [240, 1440, 2640].contains(&tick),
                    restart: false,
                },
            );
        }
        assert_eq!(app.game.phase, game::Phase::Lost);
        assert!(
            app.status_text_v1()
                .lines()
                .next()
                .unwrap()
                .contains("Retry: J. Release, then press J again to launch.")
        );
        assert!(!app.status_text_v1().contains("SPACE"));
        app.restart();
        input.rebind("launch", Vec::new()).unwrap();
        app.presentation.hints.update(&input);
        assert!(
            app.status_text_v1()
                .contains("Launch is unbound. Open Controls to bind it.")
        );
        input
            .rebind(
                "launch",
                (1..=64)
                    .map(|code| Binding::digital(InputCode::Key(code)))
                    .collect(),
            )
            .unwrap();
        app.presentation.hints.update(&input);
        assert!(
            app.status_text_v1()
                .contains("Open Controls for the current launch binding.")
        );
        assert!(!app.status_text_v1().contains("press SEE CONTROLS"));
        input.reset_defaults();
        app.presentation.hints.update(&input);
        assert!(
            app.status_text_v1()
                .contains("Launch: SPACE / ENTER / MOUSE 1.")
        );
    }
}
