//! Presentation adapters for the existing procedural effects and shared input.
//! No preference storage, simulation clock or duplicate host ABI lives here.
use super::{EmberAnimation, Follow2, Game, GameAudio, SessionState};
use ferravex_play::feedback::FeedbackPolicy;
use ferravex_render::{
    get_default_font,
    input::{InputCode, PlayerInput},
    preferences,
};

pub(super) fn touch_button_matches(input: &PlayerInput, action: &str, channel: u8) -> bool {
    input.bindings(action).is_some_and(|bindings| {
        bindings
            .iter()
            .any(|binding| binding.code == InputCode::Touch(channel) && binding.scale > 0.)
    })
}

pub(super) fn motion_scale(policy: FeedbackPolicy) -> f32 {
    if policy.reduced_motion() {
        0.
    } else {
        policy.intensity()
    }
}

// Called before any ticks and drawing, including menus and zero-tick frames.
// Emberline has procedural accents, not a FeedbackController. Read the same
// shared policy directly; a second event router or preference store is unneeded.
pub(super) fn sync_preferences(
    game: &mut Game,
    motion: &mut EmberAnimation,
    camera: &mut Follow2,
) -> FeedbackPolicy {
    let policy = preferences::feedback_policy();
    apply_policy(game, motion, policy);
    let mut settings = camera.settings();
    settings.look_ahead_time = 0.05 * motion_scale(policy);
    camera
        .set_settings(settings)
        .expect("validated presentation policy and authored camera");
    policy
}

// Also apply after accepted ticks, before playing their feedback. Clear timers
// instead of merely hiding draws: enabling effects again must not revive them.
pub(super) fn apply_policy(game: &mut Game, motion: &mut EmberAnimation, policy: FeedbackPolicy) {
    let moving = motion_scale(policy) > 0.;
    if !moving {
        game.landing_feedback = 0.;
    }
    if !moving || policy.reduced_flash() {
        game.jump_feedback = 0.;
    }
    motion.set_motion_enabled(moving);
}

pub(super) fn sync_audio(sound: &mut GameAudio, state: SessionState, gameplay_active: bool) {
    // Completion is allowed to finish its accepted success cue. It creates no
    // new gameplay cues; the shared host still owns device/focus suspension.
    if matches!(state, SessionState::Ready | SessionState::Paused)
        || state == SessionState::Playing && !gameplay_active
    {
        sound.pause();
    } else {
        sound.resume();
    }
}

// Fixed storage and a whole-label fallback keep large imported bindings from
// overrunning cards or showing a truncated, misleading key name.
pub(super) struct ControlHint {
    bytes: [u8; 128],
    len: usize,
}
impl ControlHint {
    pub(super) fn new(input: &PlayerInput, action: &str) -> Self {
        let mut hint = Self {
            bytes: [0; 128],
            len: 0,
        };
        let metrics = get_default_font()
            .try_metrics(15, 1.)
            .expect("authored font size");
        hint.len = match input.format_hint(action, &mut hint.bytes) {
            Ok(text)
                if metrics
                    .measure_line(text)
                    .is_ok_and(|size| size.width <= 270.) =>
            {
                text.len()
            }
            _ => {
                let fallback = b"SEE CONTROLS";
                hint.bytes[..fallback.len()].copy_from_slice(fallback);
                fallback.len()
            }
        };
        hint
    }

    pub(super) fn text(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len]).expect("ASCII input hints")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Character, Input, feet, vec2};
    use ferravex_render::audio::Cue;
    use ferravex_render::input::{ActionKind, Binding, InputCode};

    const DT: f32 = 1. / 120.;

    struct RestorePolicy(FeedbackPolicy);
    impl Drop for RestorePolicy {
        fn drop(&mut self) {
            preferences::set_feedback_policy(self.0);
        }
    }

    #[test]
    fn live_preferences_clear_existing_effects_in_menus_and_zero_tick_frames() {
        let _restore = RestorePolicy(preferences::feedback_policy());
        for state in [
            SessionState::Ready,
            SessionState::Playing,
            SessionState::Paused,
            SessionState::Won,
        ] {
            let mut game = Game::new(state != SessionState::Ready);
            if state == SessionState::Paused {
                game.session.set_paused(true);
            } else if state == SessionState::Won {
                game.session.finish(true);
            }
            let mut motion = EmberAnimation::new();
            game.motor.grounded = false;
            motion.update(&game, DT);
            game.motor.grounded = true;
            motion.update(&game, DT);
            assert_eq!(motion.frame(), 10);
            game.jump_feedback = 0.18;
            game.landing_feedback = 0.24;
            game.checkpoint_notice = 2.5;
            game.respawn_notice = 0.6;
            let mut camera = Follow2::default();
            let before = (
                game.position(),
                game.velocity(),
                game.trial.elapsed(),
                game.session.tick(),
            );
            preferences::set_feedback_policy(FeedbackPolicy::new(false, true, 1.).unwrap());
            let flash_only = sync_preferences(&mut game, &mut motion, &mut camera);
            assert!(flash_only.reduced_flash());
            assert!(!flash_only.reduced_motion());
            assert_eq!(game.jump_feedback, 0.);
            assert_eq!(game.landing_feedback, 0.24);
            assert_eq!(motion.frame(), 10);
            assert_eq!(camera.settings().look_ahead_time, 0.05);

            preferences::set_feedback_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
            sync_preferences(&mut game, &mut motion, &mut camera);
            assert_eq!(game.landing_feedback, 0.);
            assert_eq!(motion.frame(), 0);
            assert_eq!(camera.settings().look_ahead_time, 0.);
            assert_eq!(
                game.checkpoint_notice, 2.5,
                "static checkpoint text remains readable"
            );
            assert_eq!(
                game.respawn_notice, 0.6,
                "respawn text is not a screen flash"
            );
            assert_eq!(
                game.advance(0., |_| panic!("no tick"), |_, _| panic!("no event")),
                0.
            );

            preferences::set_feedback_policy(FeedbackPolicy::default());
            sync_preferences(&mut game, &mut motion, &mut camera);
            assert_eq!((game.jump_feedback, game.landing_feedback), (0., 0.));
            assert_eq!(
                motion.frame(),
                0,
                "an old landing pose cannot revive while paused"
            );
            assert_eq!(
                before,
                (
                    game.position(),
                    game.velocity(),
                    game.trial.elapsed(),
                    game.session.tick()
                )
            );
            assert_eq!(game.session.state(), state);
        }
    }

    #[test]
    fn effects_emitted_while_disabled_never_revive_but_new_events_can_play() {
        let mut game = Game::new(true);
        let mut motion = EmberAnimation::new();
        let silent = FeedbackPolicy::new(false, false, 0.).unwrap();
        apply_policy(&mut game, &mut motion, silent);
        game.update(Input::default(), DT);
        game.update(
            Input {
                jump: true,
                jump_held: true,
                ..Input::default()
            },
            DT,
        );
        assert!(game.feedback.jumped);
        apply_policy(&mut game, &mut motion, silent);
        assert_eq!(game.jump_feedback, 0.);
        game.motor = Character::new(feet(vec2(1410., 490.)));
        game.motor.velocity.y = -400.;
        let mut landings = 0;
        for _ in 0..20 {
            game.update(Input::default(), DT);
            motion.update(&game, DT);
            landings += usize::from(game.feedback.landed);
            apply_policy(&mut game, &mut motion, silent);
        }
        assert_eq!(landings, 1);
        assert!(game.checkpoint_notice > 0.);
        assert_eq!(game.landing_feedback, 0.);
        apply_policy(&mut game, &mut motion, FeedbackPolicy::default());
        assert_eq!((game.jump_feedback, game.landing_feedback), (0., 0.));
        game.update(
            Input {
                jump: true,
                jump_held: true,
                ..Input::default()
            },
            DT,
        );
        apply_policy(&mut game, &mut motion, FeedbackPolicy::default());
        assert!(game.feedback.jumped);
        assert!(game.jump_feedback > 0.);
    }

    #[test]
    fn audio_tracks_pause_controls_restart_and_allows_the_completion_cue() {
        let mut sound = GameAudio::new();
        for (state, active) in [
            (SessionState::Ready, false),
            (SessionState::Paused, true),
            (SessionState::Playing, false),
        ] {
            sync_audio(&mut sound, state, active);
            assert!(sound.is_paused());
            assert_eq!(sound.cue_with_outcome(Cue::Jump, 0.5), None);
            sound.reset();
            assert!(sound.is_paused(), "reset does not override suspension");
        }
        sync_audio(&mut sound, SessionState::Playing, true);
        assert!(!sound.is_paused());
        assert!(
            sound
                .cue_with_outcome(Cue::Jump, 0.5)
                .unwrap()
                .handle()
                .is_some()
        );
        sync_audio(&mut sound, SessionState::Won, false);
        assert!(
            !sound.is_paused(),
            "completion must not immediately retire its success cue"
        );
    }

    #[test]
    fn hints_follow_rebinding_import_and_reset_with_bounded_fallbacks() {
        let mut input = PlayerInput::transient("emberline-hints");
        input
            .action(
                "jump",
                "Jump",
                ActionKind::Button,
                vec![Binding::digital(InputCode::Key(32))],
            )
            .unwrap();
        assert_eq!(ControlHint::new(&input, "jump").text(), "Space");
        let original = input.export_config().unwrap();
        input
            .rebind("jump", vec![Binding::digital(InputCode::Key(b'j' as u16))])
            .unwrap();
        assert_eq!(ControlHint::new(&input, "jump").text(), "J");
        input.import_config(&original).unwrap();
        assert_eq!(ControlHint::new(&input, "jump").text(), "Space");
        input.rebind("jump", vec![]).unwrap();
        assert_eq!(ControlHint::new(&input, "jump").text(), "Unbound");
        // Both a label wider than the HUD and one exceeding scratch capacity
        // produce the same complete, honest fallback.
        for count in [4, 32] {
            input
                .rebind("jump", vec![Binding::digital(InputCode::Key(32)); count])
                .unwrap();
            assert_eq!(ControlHint::new(&input, "jump").text(), "SEE CONTROLS");
        }
        input.reset_defaults();
        assert_eq!(ControlHint::new(&input, "jump").text(), "Space");
        assert_eq!(ControlHint::new(&input, "absent").text(), "SEE CONTROLS");
    }

    #[test]
    fn touch_hint_queries_preserve_the_pending_gameplay_edge() {
        use ferravex_render::input::touch::{Control, Event, Output, Phase, Region};
        let mut input = PlayerInput::transient("emberline-hint-edge");
        input
            .action(
                "jump",
                "Jump",
                ActionKind::Button,
                vec![Binding::digital(InputCode::Touch(3))],
            )
            .unwrap();
        input
            .set_touch_layout(&[Control {
                region: Region::Rect(crate::Rect::new(0., 0., 100., 100.)),
                output: Output::Button(3),
            }])
            .unwrap();
        ferravex_render::input::touch_event(Event {
            id: 131,
            phase: Phase::Start,
            position: vec2(50., 50.),
        });
        input.sample(true);
        assert_eq!(input.current_device(), crate::HintDevice::Touch);
        assert!(touch_button_matches(&input, "jump", 3));
        for _ in 0..3 {
            assert_eq!(ControlHint::new(&input, "jump").text(), "Touch 3");
        }
        assert!(input.pressed("jump"));
        assert!(!input.pressed("jump"));
        input
            .rebind("jump", vec![Binding::digital(InputCode::Touch(5))])
            .unwrap();
        assert!(
            !touch_button_matches(&input, "jump", 3),
            "do not label an unmapped jump region"
        );
        assert!(touch_button_matches(&input, "jump", 5));
        assert_eq!(ControlHint::new(&input, "jump").text(), "Touch 5");
        ferravex_render::input::touch_event(Event {
            id: 131,
            phase: Phase::End,
            position: vec2(50., 50.),
        });
    }
}
