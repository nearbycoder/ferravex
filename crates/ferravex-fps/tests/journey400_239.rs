use ferravex_fps::spawn::{Config, Policy};
use ferravex_fps::*;
use ferravex_math::Vec3;
use ferravex_physics::character::CharacterError;
use ferravex_play::replay::{Limits, Playback};
use std::cell::Cell;
use std::sync::Arc;

fn arena() -> Arena {
    Arena {
        solids: vec![],
        spawns: vec![Vec3::ZERO],
    }
}
fn input(buttons: u8) -> InputFrame {
    InputFrame {
        buttons,
        ..Default::default()
    }
}
fn game(count: usize) -> Match {
    let mut game = Match::new(Arena {
        solids: vec![],
        spawns: (0..count).map(|i| Vec3::X * (i * 8) as f32).collect(),
    });
    for id in 0..count {
        assert!(game.spawn_player(id as u8));
    }
    game
}
fn checkpoint(game: &Match) -> Vec<u8> {
    let mut bytes = Vec::new();
    game.checkpoint_into(&mut bytes).unwrap();
    bytes
}
fn copy(game: &Match) -> Match {
    Match::from_checkpoint(&checkpoint(game)).unwrap()
}
fn overlap() -> Policy {
    Policy::new(Config {
        minimum_horizontal_distance: 0.,
        forbid_occupied: false,
        ..Default::default()
    })
    .unwrap()
}
fn outcomes(start: bool, complete: bool, jump: bool, fire: bool) -> PlayerTransitions {
    PlayerTransitions {
        reload_started: start,
        reload_completed: complete,
        jumped: jump,
        fired: fire,
    }
}

#[test]
fn invalid_settings_preserve_every_field_alive_or_dead_and_bool_compatibility() {
    let valid = WeaponSettings::default();
    for weapon in [
        WeaponSettings {
            magazine: 0,
            ..valid
        },
        WeaponSettings {
            magazine: MAGAZINE_SIZE + 1,
            ..valid
        },
        WeaponSettings {
            fire_ticks: 0,
            ..valid
        },
        WeaponSettings {
            fire_ticks: 4097,
            ..valid
        },
        WeaponSettings {
            reload_ticks: 0,
            ..valid
        },
        WeaponSettings {
            reload_ticks: 4097,
            ..valid
        },
    ] {
        for dead in [false, true] {
            let mut p = Player::new(0, Vec3::ZERO);
            p.health = if dead { 0 } else { 34 };
            p.ammo = 3;
            p.fire_cooldown = 1;
            p.reload_ticks = 1;
            p.respawn_ticks = 1;
            p.velocity = Vec3::ONE;
            let before = p.clone();
            let mut legacy = p.clone();
            let controls = InputFrame {
                move_x: 1.,
                yaw: 1.,
                pitch: 0.4,
                ..input(BUTTON_FIRE | BUTTON_RELOAD | BUTTON_JUMP)
            };
            let result = step_player_with_settings_detailed(&mut p, controls, &arena(), weapon);
            assert_eq!(result.error, Some(PlayerStepError::InvalidWeaponSettings));
            assert!(result.transitions.is_empty());
            assert_eq!(p, before);
            assert!(!step_player_with_settings(
                &mut legacy,
                controls,
                &arena(),
                weapon
            ));
            assert_eq!(legacy, before);
        }
    }
}

#[test]
fn movement_errors_report_actual_motor_error_keep_legacy_aim_update_and_gate_weapons() {
    let mut cases = Vec::new();
    let mut bad_state = Player::new(0, Vec3::ZERO);
    bad_state.jump.coyote = 0.6;
    cases.push((bad_state, arena(), CharacterError::InvalidInput));
    let mut invalid = arena();
    invalid.solids.push(Obstacle {
        center: Vec3::ZERO,
        half_extents: Vec3::ZERO,
    });
    cases.push((
        Player::new(0, Vec3::ZERO),
        invalid,
        CharacterError::InvalidObstacle,
    ));
    let mut trapped = arena();
    trapped.solids.push(Obstacle {
        center: Vec3::Y,
        half_extents: Vec3::splat(100.),
    });
    cases.push((
        Player::new(0, Vec3::ZERO),
        trapped,
        CharacterError::UnresolvedPenetration,
    ));
    let mut crowded = arena();
    crowded.solids = vec![
        Obstacle {
            center: Vec3::new(0., -1., 0.),
            half_extents: Vec3::splat(0.99)
        };
        16385
    ];
    cases.push((
        Player::new(0, Vec3::ZERO),
        crowded,
        CharacterError::QueryBudget,
    ));
    for (mut p, arena, error) in cases {
        p.ammo = 0;
        p.reload_ticks = 1;
        p.fire_cooldown = 1;
        let mut expected = p.clone();
        let mut legacy = p.clone();
        let controls = InputFrame {
            yaw: 9.,
            pitch: 9.,
            move_x: 1.,
            ..input(BUTTON_FIRE | BUTTON_RELOAD | BUTTON_JUMP)
        };
        expected.yaw = controls.sanitized().yaw;
        expected.pitch = controls.sanitized().pitch;
        let result = step_player_detailed(&mut p, controls, &arena);
        assert_eq!(result.error, Some(PlayerStepError::Movement(error)));
        assert!(result.transitions.is_empty());
        assert_eq!(p, expected);
        assert!(!step_player(&mut legacy, controls, &arena));
        assert_eq!(legacy, expected);
        let before = expected.clone();
        assert_eq!(try_move_player(&mut expected, controls, &arena), Err(error));
        assert_eq!(expected, before);
    }
}

#[test]
fn held_reload_starts_once_until_completion_and_completion_can_fire_same_tick() {
    let mut p = Player::new(0, Vec3::ZERO);
    p.ammo = 0;
    let weapon = WeaponSettings {
        magazine: 5,
        fire_ticks: 2,
        reload_ticks: 2,
    };
    let controls = input(BUTTON_FIRE | BUTTON_RELOAD | BUTTON_JUMP);
    let expected = [
        outcomes(true, false, true, false),
        outcomes(false, false, false, false),
        outcomes(false, true, false, true),
        outcomes(true, false, false, false),
    ];
    for transitions in expected {
        let mut legacy = p.clone();
        let result = step_player_with_settings_detailed(&mut p, controls, &arena(), weapon);
        assert_eq!(
            result,
            PlayerStepResult {
                transitions,
                error: None
            }
        );
        assert_eq!(
            step_player_with_settings(&mut legacy, controls, &arena(), weapon),
            transitions.fired
        );
        assert_eq!(legacy, p);
    }
    // The fourth tick legitimately starts a new reload after the accepted shot.
    assert_eq!(p.reload_ticks, 2);
    let mut jumps = 0;
    for _ in 0..180 {
        jumps += usize::from(
            step_player_detailed(&mut p, input(BUTTON_JUMP), &arena())
                .transitions
                .jumped,
        );
    }
    assert_eq!(jumps, 0);
    assert!(p.grounded);
}

#[test]
fn timer_ammo_and_full_magazine_gates_preserve_legacy_raw_local_inputs() {
    for cooldown in [0, 1, 2] {
        for ammo in [0, 1, MAGAZINE_SIZE, u8::MAX] {
            let mut p = Player::new(u8::MAX, Vec3::ZERO);
            p.ammo = ammo;
            p.fire_cooldown = cooldown;
            let mut old = p.clone();
            let result = step_player_detailed(&mut p, input(BUTTON_FIRE), &arena());
            assert_eq!(result.error, None);
            assert_eq!(result.transitions.fired, ammo > 0 && cooldown <= 1);
            assert_eq!(
                step_player(&mut old, input(BUTTON_FIRE), &arena()),
                result.transitions.fired
            );
            assert_eq!(old, p); // No checkpoint-style validation silently added locally.
        }
    }
    let mut p = Player::new(0, Vec3::ZERO);
    assert!(
        step_player_detailed(&mut p, input(BUTTON_RELOAD), &arena())
            .transitions
            .is_empty()
    );
    p.reload_ticks = 1;
    let result = step_player_detailed(&mut p, input(BUTTON_RELOAD), &arena());
    assert_eq!(result.transitions, outcomes(false, true, false, false));
    assert_eq!(p.reload_ticks, 0);
    assert!(
        step_player_detailed(&mut p, input(BUTTON_RELOAD), &arena())
            .transitions
            .is_empty()
    );
}

#[test]
fn buffered_coyote_and_ceiling_jump_results_come_from_actual_motor_acceptance() {
    let mut falling = Player::new(0, Vec3::Y * 0.2);
    falling.grounded = false;
    falling.velocity.y = -5.;
    let first = step_player_detailed(&mut falling, input(BUTTON_JUMP), &arena());
    assert!(!first.transitions.jumped);
    assert!(falling.jump.buffer > 0.);
    let mut jumps = 0;
    for _ in 0..8 {
        let mut motion_only = falling.clone();
        try_move_player(&mut motion_only, InputFrame::default(), &arena()).unwrap();
        let next = step_player_detailed(&mut falling, InputFrame::default(), &arena());
        assert_eq!(falling.position, motion_only.position);
        assert_eq!(falling.velocity, motion_only.velocity);
        assert_eq!(falling.jump, motion_only.jump);
        jumps += usize::from(next.transitions.jumped); // No current jump button.
    }
    assert_eq!(jumps, 1);
    let mut coyote = Player::new(0, Vec3::Y * 10.);
    coyote.grounded = false;
    coyote.jump.coyote = 0.1;
    assert!(
        step_player_detailed(&mut coyote, input(BUTTON_JUMP), &arena())
            .transitions
            .jumped
    );
    let mut ceiling = arena();
    ceiling.solids.push(Obstacle {
        center: Vec3::new(0., 2.3, 0.),
        half_extents: Vec3::new(3., 0.5, 3.),
    });
    let mut p = Player::new(0, Vec3::ZERO);
    let result = step_player_detailed(&mut p, input(BUTTON_JUMP), &ceiling);
    assert_eq!(result.error, None);
    assert!(result.transitions.jumped); // Accepted impulse even though the ceiling stops it.
    assert!(p.velocity.y <= 0.);
}

#[derive(Default)]
struct Rules {
    calls: Cell<usize>,
    frozen: bool,
    no_respawn: bool,
    bad_weapon: bool,
    short_weapon: bool,
    strip_buttons: bool,
}
impl CombatRules for Rules {
    fn input(&mut self, _player: &Player, mut input: InputFrame) -> InputFrame {
        self.calls.set(self.calls.get() + 1);
        if self.strip_buttons {
            input.buttons = 0;
        }
        input
    }
    fn frozen(&self, _id: u8) -> bool {
        self.calls.set(self.calls.get() + 1);
        self.frozen
    }
    fn auto_respawn(&self) -> bool {
        !self.no_respawn
    }
    fn weapon(&self, _id: u8) -> WeaponSettings {
        if self.bad_weapon {
            WeaponSettings {
                magazine: 0,
                ..Default::default()
            }
        } else if self.short_weapon {
            WeaponSettings {
                magazine: 10,
                fire_ticks: 1,
                reload_ticks: 2,
            }
        } else {
            WeaponSettings::default()
        }
    }
    fn damage(&mut self, _: u8, _: &Player, _: bool) -> Option<u8> {
        None
    }
}

#[test]
fn every_policy_gate_clears_prior_transitions_without_fabricating_acceptance() {
    for policy in [None, Some(overlap())] {
        for mode in 0..7 {
            let mut game = game(1);
            let mut output = StepOutput::new();
            game.step_into(&[(0, input(BUTTON_FIRE))], &mut output)
                .unwrap();
            assert_eq!(output.transition_count(), 1);
            let p = game.player_mut(0).unwrap();
            p.fire_cooldown = 0;
            p.ammo = 0;
            p.reload_ticks = 1;
            let mut rules = Rules::default();
            match mode {
                0 => rules.frozen = true,
                1 => rules.bad_weapon = true,
                2 => {
                    p.health = 0;
                    p.respawn_ticks = 1;
                    rules.no_respawn = true;
                }
                3 => {
                    p.health = 0;
                    p.respawn_ticks = 2;
                }
                4 => {
                    p.health = 0;
                    p.respawn_ticks = 1;
                }
                5 => {
                    p.health = 0;
                    p.respawn_ticks = 0;
                    rules.bad_weapon = true;
                }
                _ => {
                    rules.strip_buttons = true;
                    p.reload_ticks = 0;
                }
            }
            let before = game.player(0).unwrap().clone();
            let epochs = game.life_epochs();
            game.step_with_rules_and_spawn_policy_into(
                &[(0, input(BUTTON_FIRE | BUTTON_RELOAD | BUTTON_JUMP))],
                &[],
                &mut rules,
                policy.as_ref(),
                &mut output,
            )
            .unwrap();
            assert!(output.transitions().is_empty(), "mode={mode}");
            assert_eq!(output.transition_count(), 0);
            assert_eq!(output.transition_capacity(), MAX_STEP_TRANSITIONS);
            let after = game.player(0).unwrap();
            if matches!(mode, 1 | 5) {
                assert_eq!(*after, before);
            }
            if matches!(mode, 0 | 2) {
                assert_eq!(after.reload_ticks, 1);
            }
            if mode == 3 {
                assert_eq!(after.respawn_ticks, 1);
            }
            if mode == 4 {
                assert!(after.is_alive());
                assert_eq!(output.events(), [GameEvent::Respawned { player: 0 }]);
                assert_eq!(game.life_epochs()[0], epochs[0] + 1);
            } else {
                assert_eq!(game.life_epochs(), epochs);
                assert!(output.events().is_empty());
            }
        }
    }
}

#[test]
fn authority_movement_failure_keeps_aim_semantics_and_emits_no_transition() {
    for policy in [None, Some(overlap())] {
        let mut game = game(1);
        game.arena.solids.push(Obstacle {
            center: Vec3::Y,
            half_extents: Vec3::splat(100.),
        });
        let p = game.player_mut(0).unwrap();
        p.ammo = 0;
        p.reload_ticks = 1;
        p.fire_cooldown = 1;
        let mut old = copy(&game);
        let command = InputFrame {
            yaw: 1.,
            pitch: 0.5,
            ..input(BUTTON_FIRE | BUTTON_RELOAD | BUTTON_JUMP)
        };
        let mut expected = game.player(0).unwrap().clone();
        expected.yaw = command.sanitized().yaw;
        expected.pitch = command.sanitized().pitch;
        let mut output = StepOutput::new();
        game.step_with_spawn_policy_into(&[(0, command)], &[], policy.as_ref(), &mut output)
            .unwrap();
        let legacy = old
            .step_with_spawn_policy(&[(0, command)], &[], policy.as_ref())
            .unwrap();
        assert_eq!(checkpoint(&old), checkpoint(&game));
        assert_eq!(output.events(), legacy.events);
        assert!(output.events().is_empty());
        assert!(output.transitions().is_empty());
        assert_eq!(*game.player(0).unwrap(), expected);
        assert_eq!(game.tick, 1);
    }
}

#[test]
fn living_no_auto_respawn_actor_and_later_killed_shooters_still_report_accepted_fire() {
    for policy in [None, Some(overlap())] {
        let mut game = game(2);
        for p in &mut game.players {
            p.position = Vec3::ZERO;
        }
        let commands = [(0, input(BUTTON_FIRE)), (1, input(BUTTON_FIRE))];
        let mut output = StepOutput::new();
        // Disabling automatic respawn does not freeze living actors.
        let mut rules = Rules {
            no_respawn: true,
            ..Default::default()
        };
        game.step_with_rules_and_spawn_policy_into(
            &commands,
            &[],
            &mut rules,
            policy.as_ref(),
            &mut output,
        )
        .unwrap();
        assert_eq!(output.transition_count(), 2);
        assert!(
            output
                .transitions()
                .iter()
                .all(|record| record.transitions.fired)
        );
        // A fresh real combat tick emits fire before resolving later deaths.
        let mut game = game_from_overlap();
        game.step_with_spawn_policy_into(&commands, &[], policy.as_ref(), &mut output)
            .unwrap();
        assert!(game.players.iter().all(|p| !p.is_alive()));
        assert_eq!(output.transition_count(), 2);
        assert!(
            output
                .transitions()
                .iter()
                .all(|record| record.life == 1 && record.transitions.fired)
        );
        assert_eq!(
            output
                .events()
                .iter()
                .filter(|e| matches!(e, GameEvent::Killed { .. }))
                .count(),
            2
        );
    }
}
fn game_from_overlap() -> Match {
    let mut game = game(2);
    for p in &mut game.players {
        p.position = Vec3::ZERO;
    }
    game
}

#[test]
fn shot_time_rejection_gates_only_fire_and_retains_jump_reload_completion_and_tick() {
    for start in [0, u32::MAX] {
        for policy in [None, Some(overlap())] {
            let mut game = game(1);
            game.tick = start;
            game.player_mut(0).unwrap().ammo = 0;
            game.player_mut(0).unwrap().reload_ticks = 1;
            let mut output = StepOutput::new();
            let command = InputFrame {
                sequence: 4321,
                move_x: 1.,
                ..input(BUTTON_FIRE | BUTTON_RELOAD | BUTTON_JUMP)
            };
            game.step_with_spawn_policy_into(
                &[(0, command)],
                &[ShotTime {
                    player: 0,
                    tick: start.wrapping_add(1),
                }],
                policy.as_ref(),
                &mut output,
            )
            .unwrap();
            assert_eq!(output.rejected_shots(), [(0, RewindError::FutureTick)]);
            assert!(output.events().is_empty());
            assert_eq!(
                output.transitions(),
                [StepTransition {
                    player: 0,
                    tick: start.wrapping_add(1),
                    life: 1,
                    transitions: outcomes(false, true, true, false)
                }]
            );
            assert_eq!(game.player(0).unwrap().ammo, MAGAZINE_SIZE);
            assert!(game.player(0).unwrap().position.x > 0.);
            assert_eq!(output.transitions()[0].tick, game.tick);
        }
    }
}

#[test]
fn maximum_aggregate_bound_and_legacy_or_policy_order_are_exact() {
    for policy in [None, Some(overlap())] {
        let mut game = game(MAX_PLAYERS);
        game.players.reverse();
        for p in &mut game.players {
            p.ammo = 0;
            p.reload_ticks = 1;
        }
        let commands: [_; MAX_PLAYERS] =
            std::array::from_fn(|i| (i as u8, input(BUTTON_FIRE | BUTTON_JUMP | BUTTON_RELOAD)));
        let mut output = StepOutput::new();
        let capacities = output.capacities();
        game.step_with_spawn_policy_into(&commands, &[], policy.as_ref(), &mut output)
            .unwrap();
        assert_eq!(output.transition_count(), MAX_STEP_TRANSITIONS);
        for (i, record) in output.transitions().iter().enumerate() {
            let id = if policy.is_some() {
                i
            } else {
                MAX_PLAYERS - 1 - i
            };
            assert_eq!(
                *record,
                StepTransition {
                    player: id as u8,
                    tick: 1,
                    life: 1,
                    transitions: outcomes(false, true, true, true)
                }
            );
        }
        assert_eq!(
            output
                .events()
                .iter()
                .filter(|e| matches!(e, GameEvent::Shot { .. }))
                .count(),
            MAX_PLAYERS
        );
        game.step_with_spawn_policy_into(&[], &[], policy.as_ref(), &mut output)
            .unwrap();
        assert!(output.transitions().is_empty());
        assert_eq!(capacities, output.capacities());
        output.clear();
        assert!(output.events().is_empty() && output.rejected_shots().is_empty());
        assert_eq!(output.transition_count(), 0);
        assert_eq!(output.transition_capacity(), MAX_STEP_TRANSITIONS);
    }
    // Third capacity is inline and cannot be undersized by old constructors.
    let output = StepOutput::with_capacity(0, 0);
    assert_eq!(output.transition_capacity(), MAX_PLAYERS);
    assert_eq!(output.transition_count(), 0);
}

#[test]
fn safe_deferral_respawn_and_reused_slot_have_correct_nonemission_and_life_identity() {
    let policy = Policy::new(Config::default()).unwrap();
    let mut game = game(2);
    game.arena.spawns = vec![Vec3::ZERO];
    let p = game.player_mut(1).unwrap();
    p.health = 0;
    p.respawn_ticks = 1;
    let mut output = StepOutput::new();
    let controls = [(1, input(BUTTON_FIRE | BUTTON_RELOAD | BUTTON_JUMP))];
    let epochs = game.life_epochs();
    for _ in 0..2 {
        game.step_with_spawn_policy_into(&controls, &[], Some(&policy), &mut output)
            .unwrap();
        assert!(output.transitions().is_empty());
        assert!(output.events().is_empty());
        assert!(!game.player(1).unwrap().is_alive());
        assert_eq!(game.player(1).unwrap().respawn_ticks, 0);
        assert_eq!(game.life_epochs(), epochs);
    }
    game.remove_player(0); // The occupied candidate becomes available.
    game.step_with_spawn_policy_into(&controls, &[], Some(&policy), &mut output)
        .unwrap();
    assert!(output.transitions().is_empty());
    assert_eq!(output.events(), [GameEvent::Respawned { player: 1 }]);
    game.step_with_spawn_policy_into(&[(1, input(BUTTON_FIRE))], &[], Some(&policy), &mut output)
        .unwrap();
    let old_life = output.transitions()[0].life;
    assert_eq!(old_life, u64::from(epochs[1]) + 1);
    game.remove_player(1);
    assert!(game.spawn_player_with_policy(1, &policy).unwrap().is_some());
    game.step_with_spawn_policy_into(&[(1, input(BUTTON_FIRE))], &[], Some(&policy), &mut output)
        .unwrap();
    assert_eq!(output.transitions()[0].life, old_life + 1);
}

#[test]
fn preflight_failure_keeps_real_nonempty_sidecar_and_never_invokes_rules() {
    let mut game = game(1);
    let mut output = StepOutput::new();
    game.step_into(&[(0, input(BUTTON_FIRE))], &mut output)
        .unwrap();
    let records = output.transitions().to_vec();
    let events = output.events().to_vec();
    let pointer = output.transitions().as_ptr();
    let capacities = output.capacities();
    let before = checkpoint(&game);
    let mut rules = Rules::default();
    game.tick = 0; // Existing history is now in the future.
    assert!(matches!(
        game.step_with_rules_into(&[], &[], &mut rules, &mut output),
        Err(StepError::InvalidState(_))
    ));
    assert_eq!(game.tick, 0);
    assert_eq!(output.transitions(), records);
    game.tick = 1;
    assert_eq!(before, checkpoint(&game));
    let policy = overlap();
    assert!(matches!(
        game.step_with_rules_and_spawn_policy_into(
            &[(0, InputFrame::default()), (0, InputFrame::default())],
            &[],
            &mut rules,
            Some(&policy),
            &mut output
        ),
        Err(StepError::Spawn(_))
    ));
    assert_eq!(before, checkpoint(&game));
    assert_eq!(output.transitions(), records);
    assert_eq!(output.events(), events);
    assert_eq!(output.transitions().as_ptr(), pointer);
    assert_eq!(output.capacities(), capacities);
    assert_eq!(rules.calls.get(), 0);
    output.clear();
    assert!(output.transitions().is_empty());
    game.step_into(&[], &mut output).unwrap();
    assert!(output.transitions().is_empty());
}

#[test]
fn repeated_prediction_and_authority_replay_observe_identical_completion_but_feedback_once() {
    for start in [0, u32::MAX - 1] {
        let mut game = game(1);
        game.tick = start;
        game.player_mut(0).unwrap().ammo = 0;
        game.player_mut(0).unwrap().reload_ticks = 1;
        let initial = checkpoint(&game);
        let snapshot = game.player(0).unwrap().snapshot();
        let command = input(BUTTON_FIRE);
        let mut output = StepOutput::new();
        let mut seen = [None; 4];
        let mut cursor = 0;
        let mut confirmed_completions = 0;
        let mut observations = Vec::new();
        for session in [17, 17, 18] {
            // A fresh session intentionally owns new feedback.
            let mut local = Player::from_snapshot(&snapshot);
            for _ in 0..2 {
                let mut predicted = local.clone();
                let result = step_player_detailed(&mut predicted, command, &game.arena);
                assert_eq!(result.transitions, outcomes(false, true, false, true));
                assert_eq!(result.error, None);
                local = Player::from_snapshot(&snapshot); // Reconcile to the same actual snapshot.
            }
            game = Match::from_checkpoint(&initial).unwrap();
            game.step_into(&[(0, command)], &mut output).unwrap();
            let observation = output.transitions()[0];
            observations.push(observation);
            let key = (
                session,
                observation.player,
                observation.life,
                observation.tick,
            );
            if !seen.contains(&Some(key)) {
                seen[cursor] = Some(key);
                cursor = (cursor + 1) % seen.len();
                confirmed_completions += usize::from(observation.transitions.reload_completed);
            }
        }
        assert!(observations.windows(2).all(|w| w[0] == w[1]));
        assert_eq!(confirmed_completions, 2); // Exactly once in each of two sessions.
        assert_eq!(game.tick, start.wrapping_add(1));
        assert_eq!(game.player(0).unwrap().ammo, MAGAZINE_SIZE - 1);
        game.step_into(&[(0, command)], &mut output).unwrap();
        assert!(output.transitions().is_empty()); // Consecutive tick has no second completion.
    }
}

#[test]
fn accepted_sidecar_preserves_full_state_game_events_wire_and_replay_bytes_on_both_paths() {
    for policy in [None, Some(overlap())] {
        let mut old = game(2);
        old.player_mut(0).unwrap().ammo = 0;
        let mut retained = copy(&old);
        let mut a = replay::Capture::new(&old, 19, Limits::default()).unwrap();
        let mut b = replay::Capture::new(&retained, 19, Limits::default()).unwrap();
        let mut pa = policy.map(|p| spawn::Capture::new(&old, p, 19, Limits::default()).unwrap());
        let mut pb =
            policy.map(|p| spawn::Capture::new(&retained, p, 19, Limits::default()).unwrap());
        let mut output = StepOutput::new();
        let mut totals = [0; 4];
        for sequence in 0..360 {
            let mut buttons = BUTTON_FIRE;
            if retained.player(0).unwrap().ammo == 0 {
                buttons |= BUTTON_RELOAD;
            }
            if sequence % 100 == 0 {
                buttons |= BUTTON_JUMP;
            }
            let commands = [(
                0,
                InputFrame {
                    sequence,
                    move_x: 0.25,
                    ..input(buttons)
                },
            )];
            let before = retained.player(0).unwrap().snapshot();
            let mut predicted = Player::from_snapshot(&before);
            let local = step_player_detailed(&mut predicted, commands[0].1, &retained.arena);
            let legacy = old
                .step_with_spawn_policy(&commands, &[], policy.as_ref())
                .unwrap();
            retained
                .step_with_spawn_policy_into(&commands, &[], policy.as_ref(), &mut output)
                .unwrap();
            assert_eq!(predicted, *retained.player(0).unwrap());
            assert_eq!(legacy.events, output.events());
            assert_eq!(legacy.rejected_shots, output.rejected_shots());
            assert_eq!(checkpoint(&old), checkpoint(&retained));
            assert_eq!(
                output.transition_count(),
                usize::from(!local.transitions.is_empty())
            );
            if let Some(record) = output.transitions().first() {
                assert_eq!(record.transitions, local.transitions);
                for (i, flag) in [
                    record.transitions.reload_started,
                    record.transitions.reload_completed,
                    record.transitions.jumped,
                    record.transitions.fired,
                ]
                .into_iter()
                .enumerate()
                {
                    totals[i] += usize::from(flag);
                }
            }
            for (old_event, event) in legacy.events.iter().zip(output.events()) {
                assert_eq!(
                    NetworkEvent {
                        tick: old.tick,
                        event: *old_event
                    }
                    .encode()
                    .unwrap(),
                    NetworkEvent {
                        tick: retained.tick,
                        event: *event
                    }
                    .encode()
                    .unwrap()
                );
            }
            if let (Some(a), Some(b)) = (&mut pa, &mut pb) {
                a.push(&old, &commands, &[], &legacy.events).unwrap();
                b.push(&retained, &commands, &[], output.events()).unwrap();
            } else {
                a.push(&old, &commands, &[], &legacy.events).unwrap();
                b.push(&retained, &commands, &[], output.events()).unwrap();
            }
        }
        assert!(totals.iter().all(|v| *v > 0));
        let (ra, rb) = if let (Some(a), Some(b)) = (pa, pb) {
            (a.finish(), b.finish())
        } else {
            (a.finish(), b.finish())
        };
        assert_eq!(ra.encode(), rb.encode());
        let mut playback = Playback::new(Arc::new(rb));
        if let Some(policy) = policy {
            let mut world = spawn::World::new(Match::new(retained.arena.clone()), policy).unwrap();
            for _ in 0..2 {
                playback.reset();
                playback.start(&mut world).unwrap();
                while !playback.finished() {
                    playback.step(&mut world).unwrap();
                }
                assert_eq!(retained.state_hash(), world.game.state_hash());
            }
        } else {
            let mut world = replay::World::new(Match::new(retained.arena.clone()));
            for _ in 0..2 {
                playback.reset();
                playback.start(&mut world).unwrap();
                while !playback.finished() {
                    playback.step(&mut world).unwrap();
                }
                assert_eq!(retained.state_hash(), world.game.state_hash());
            }
        }
        println!(
            "policy={} exact state/event/wire/tape equivalence; start/complete/jump/fire={totals:?}",
            policy.is_some()
        );
    }
}

struct CountingAllocator;
thread_local! { static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) }; }
unsafe impl std::alloc::GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        ALLOCATIONS.with(|v| {
            if let Some(n) = v.get() {
                v.set(Some(n + 1));
            }
        });
        unsafe { std::alloc::System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        ALLOCATIONS.with(|v| {
            if let Some(n) = v.get() {
                v.set(Some(n + 1));
            }
        });
        unsafe { std::alloc::System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.with(|v| {
            if let Some(n) = v.get() {
                v.set(Some(n + 1));
            }
        });
        unsafe { std::alloc::System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn all_transitions_at_max_roster_allocate_zero_after_warmup_legacy_and_safe_spawn() {
    for policy in [None, Some(Policy::new(Config::default()).unwrap())] {
        let mut game = game(MAX_PLAYERS);
        game.arena.solids = vec![
            Obstacle {
                center: Vec3::new(100., 1., 100.),
                half_extents: Vec3::ONE
            };
            256
        ];
        let mut rules = Rules {
            short_weapon: true,
            ..Default::default()
        };
        let mut output = StepOutput::new();
        let capacities = output.capacities();
        let mut commands: [_; MAX_PLAYERS] =
            std::array::from_fn(|i| (i as u8, input(BUTTON_FIRE | BUTTON_RELOAD)));
        for _ in 0..32 {
            game.step_with_rules_and_spawn_policy_into(
                &commands,
                &[],
                &mut rules,
                policy.as_ref(),
                &mut output,
            )
            .unwrap();
        }
        let mut totals = [0; 4];
        let mut peak = 0;
        ALLOCATIONS.with(|v| v.set(Some(0)));
        for tick in 0..1024 {
            for (_, command) in &mut commands {
                command.buttons =
                    BUTTON_FIRE | BUTTON_RELOAD | if tick % 64 == 0 { BUTTON_JUMP } else { 0 };
            }
            game.step_with_rules_and_spawn_policy_into(
                &commands,
                &[],
                &mut rules,
                policy.as_ref(),
                &mut output,
            )
            .unwrap();
            peak = peak.max(output.transition_count());
            for record in output.transitions() {
                for (i, flag) in [
                    record.transitions.reload_started,
                    record.transitions.reload_completed,
                    record.transitions.jumped,
                    record.transitions.fired,
                ]
                .into_iter()
                .enumerate()
                {
                    totals[i] += usize::from(flag);
                }
            }
        }
        let allocations = ALLOCATIONS.with(|v| v.replace(None).unwrap());
        assert_eq!(allocations, 0);
        assert!(totals.iter().all(|v| *v > 0));
        assert_eq!(peak, MAX_STEP_TRANSITIONS);
        assert_eq!(capacities, output.capacities());
        assert_eq!(output.transition_capacity(), MAX_STEP_TRANSITIONS);
        println!(
            "policy={} actual warmed ticks=1024 roster=16 solids=256 allocations={allocations} peak={peak} start/complete/jump/fire={totals:?}",
            policy.is_some()
        );
    }
}
