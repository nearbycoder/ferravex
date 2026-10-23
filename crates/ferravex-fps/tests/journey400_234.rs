use ferravex_fps::*;
use ferravex_math::Vec3;
use ferravex_play::replay::Limits;
fn arena() -> Arena {
    Arena {
        solids: vec![],
        spawns: vec![Vec3::ZERO, Vec3::new(0., 0., -8.)],
    }
}
fn fingerprint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
#[test]
fn legacy_bytes_fixture() {
    let mut g = Match::new(arena());
    g.spawn_player(0);
    g.spawn_player(1);
    let mut capture = replay::Capture::new(
        &g,
        19,
        Limits {
            checkpoint_interval: 17,
            ..Limits::default()
        },
    )
    .unwrap();
    for i in 0..220 {
        let commands = [(
            0,
            InputFrame {
                sequence: i,
                buttons: BUTTON_FIRE,
                ..InputFrame::default()
            },
        )];
        let events = g.step(&commands);
        capture.push(&g, &commands, &[], &events).unwrap();
    }
    let mut cp = vec![];
    g.checkpoint_into(&mut cp).unwrap();
    let tape = capture.finish().encode();
    assert_eq!((cp.len(), fingerprint(&cp)), (1095, 0x247c126c3a7171a3));
    assert_eq!(
        (tape.len(), fingerprint(&tape)),
        (27473, 0xd586f86b6dda4c39)
    );
}

use ferravex_fps::spawn::{self, Config, Error, Policy};
use ferravex_physics::character::ClearanceError;
use ferravex_play::replay::{Playback, PlaybackError, Recording, ReplayWorld};
use std::sync::Arc;
fn policy() -> Policy {
    Policy::new(Config {
        minimum_horizontal_distance: 0.,
        ..Config::default()
    })
    .unwrap()
}
fn game_at(points: &[Vec3]) -> Match {
    Match::new(Arena {
        solids: vec![],
        spawns: points.to_vec(),
    })
}
fn checkpoint(game: &Match) -> Vec<u8> {
    let mut out = vec![];
    game.checkpoint_into(&mut out).unwrap();
    out
}
fn corpse(game: &mut Match, id: u8, ticks: u16) {
    let p = game.player_mut(id).unwrap();
    p.health = 0;
    p.respawn_ticks = ticks;
}
#[test]
fn empty_all_blocked_and_one_safe_have_no_fallback_or_rejection_mutation() {
    let p = policy();
    let mut g = game_at(&[]);
    let before = checkpoint(&g);
    assert_eq!(p.select(&g, 0), Ok(None));
    assert_eq!(g.spawn_player_with_policy(0, &p), Ok(None));
    assert_eq!(before, checkpoint(&g));
    g.arena.spawns = vec![Vec3::ZERO, Vec3::new(3., 0., 0.)];
    g.arena.solids = vec![Obstacle {
        center: Vec3::new(0., 1., 0.),
        half_extents: Vec3::new(5., 1., 1.),
    }];
    assert_eq!(g.spawn_player_with_policy(0, &p), Ok(None));
    assert!(g.players.is_empty());
    assert_eq!(g.life_epochs(), [0; MAX_PLAYERS]);
    g.arena.solids[0].half_extents.x = 1.;
    let selected = g.spawn_player_with_policy(0, &p).unwrap().unwrap();
    assert_eq!(
        (selected.index, selected.position),
        (1, Vec3::new(3., 0., 0.))
    );
    assert_eq!(g.life_epochs()[0], 1);
    let before = checkpoint(&g);
    assert_eq!(
        g.spawn_player_with_policy(0, &p),
        Err(Error::PlayerUnavailable)
    );
    assert_eq!(
        g.spawn_player_with_policy(MAX_PLAYERS as u8, &p),
        Err(Error::InvalidPlayerId)
    );
    assert_eq!(p.select(&g, 255), Err(Error::InvalidPlayerId));
    assert_eq!(before, checkpoint(&g));
}
#[test]
fn static_capsule_narrowphase_ground_ceiling_and_vertical_clearance() {
    let p = policy();
    let mut g = game_at(&[Vec3::ZERO]);
    // Both X/Z capsule bounds overlap, but its circular section clears the corner.
    g.arena.solids = vec![Obstacle {
        center: Vec3::new(1., 1., 1.),
        half_extents: Vec3::new(0.72, 1., 0.72),
    }];
    assert_eq!(p.select(&g, 0).unwrap().unwrap().index, 0);
    g.arena.solids[0].half_extents.x = 0.9;
    g.arena.solids[0].half_extents.z = 0.9;
    assert_eq!(p.select(&g, 0), Ok(None));
    g.arena.solids.clear();
    g.arena.spawns[0].y = -0.01;
    assert_eq!(p.select(&g, 0), Ok(None));
    g.arena.spawns[0] = Vec3::ZERO;
    g.arena.solids = vec![Obstacle {
        center: Vec3::new(0., 2., 0.),
        half_extents: Vec3::new(1., 0.2, 1.),
    }];
    assert!(p.select(&g, 0).unwrap().is_some()); // Head touches at 1.8.
    g.arena.solids[0].center.y = 1.99;
    assert_eq!(p.select(&g, 0), Ok(None));
    g.arena.spawns.push(Vec3::new(0., 3., 0.));
    assert_eq!(p.select(&g, 0).unwrap().unwrap().index, 1); // Same XZ; actual Y clears.
}
#[test]
fn living_distance_dead_absent_and_spawning_actor_eligibility() {
    let p = policy();
    let mut g = game_at(&[Vec3::ZERO, Vec3::new(10., 0., 0.)]);
    g.spawn_player(0);
    g.spawn_player(1);
    g.spawn_player(2);
    g.player_mut(0).unwrap().position = Vec3::new(10., 0., 0.);
    g.player_mut(1).unwrap().position = Vec3::new(1., 0., 0.);
    corpse(&mut g, 2, 100);
    // Actor 0 does not exclude its own location; corpse 2 at candidate 0 is ignored.
    assert_eq!(p.select(&g, 0).unwrap().unwrap().index, 1);
    corpse(&mut g, 1, 100);
    assert_eq!(p.select(&g, 0).unwrap().unwrap().index, 0);
    g.remove_player(1);
    g.players.reverse();
    assert_eq!(p.select(&g, 0).unwrap().unwrap().index, 0);
}
#[test]
fn horizontal_minimum_and_occupied_volume_are_distinct_with_exact_boundaries() {
    let mut g = game_at(&[Vec3::ZERO]);
    g.spawn_player(1);
    g.player_mut(1).unwrap().position = Vec3::new(0., 3., 0.);
    assert!(policy().select(&g, 0).unwrap().is_some()); // No vertical overlap.
    let separated = Policy::new(Config {
        minimum_horizontal_distance: 1.,
        ..Config::default()
    })
    .unwrap();
    assert_eq!(separated.select(&g, 0), Ok(None)); // Minimum is explicitly horizontal.
    g.player_mut(1).unwrap().position = Vec3::X;
    assert!(separated.select(&g, 0).unwrap().is_some()); // Exact minimum eligible.
    g.player_mut(1).unwrap().position = Vec3::ZERO;
    assert_eq!(policy().select(&g, 0), Ok(None));
    let overlap = Policy::new(Config {
        minimum_horizontal_distance: 0.,
        forbid_occupied: false,
        ..Config::default()
    })
    .unwrap();
    assert!(overlap.select(&g, 0).unwrap().is_some());
    g.player_mut(1).unwrap().position = Vec3::X * (2. * PLAYER_RADIUS);
    assert!(policy().select(&g, 0).unwrap().is_some()); // Open-volume touching allowed.
}
#[test]
fn deterministic_source_ties_shuffled_roster_and_immutable_recreation() {
    let mut g = game_at(&[Vec3::new(-5., 0., 0.), Vec3::new(5., 0., 0.)]);
    g.spawn_player(7);
    g.spawn_player(2);
    g.player_mut(7).unwrap().position = Vec3::new(0., 0., 1.);
    g.player_mut(2).unwrap().position = Vec3::new(0., 0., -1.);
    let p = policy();
    let selection = p.select(&g, 0).unwrap().unwrap();
    assert_eq!(selection.index, 0);
    let before = checkpoint(&g);
    for _ in 0..20 {
        assert_eq!(p.select(&g, 0), Ok(Some(selection)));
    }
    assert_eq!(before, checkpoint(&g));
    g.players.reverse();
    assert_eq!(
        Policy::new(p.config()).unwrap().select(&g, 0),
        Ok(Some(selection))
    );
}
#[test]
fn optional_los_uses_actual_3d_eyes_and_preserves_scoring() {
    let mut g = game_at(&[Vec3::new(-5., 0., 0.), Vec3::new(5., 0., 0.)]);
    g.spawn_player(1);
    g.player_mut(1).unwrap().position = Vec3::ZERO;
    let p = Policy::new(Config {
        minimum_horizontal_distance: 0.,
        exposure_penalty: Some(100.),
        ..Config::default()
    })
    .unwrap();
    let selected = p.select(&g, 0).unwrap().unwrap();
    assert_eq!(
        (selected.index, selected.score, selected.exposed_players),
        (0, -75., 1)
    );
    // Covers right ray at full eye height; no candidate capsule overlaps it.
    g.arena.solids = vec![Obstacle {
        center: Vec3::new(2.5, 1.6, 0.),
        half_extents: Vec3::new(0.1, 0.2, 1.),
    }];
    let selected = p.select(&g, 0).unwrap().unwrap();
    assert_eq!(
        (selected.index, selected.score, selected.exposed_players),
        (1, 25., 0)
    );
    assert_eq!(policy().select(&g, 0).unwrap().unwrap().index, 0); // LOS disabled.
    // Raising the target makes the 3D eye segment pass above this low wall.
    g.player_mut(1).unwrap().position.y = 3.;
    assert_eq!(p.select(&g, 0).unwrap().unwrap().exposed_players, 1);
    g.arena.spawns = vec![Vec3::ZERO];
    g.arena.solids.clear();
    g.player_mut(1).unwrap().position = Vec3::new(0., -3., 0.);
    assert_eq!(p.select(&g, 0).unwrap().unwrap().exposed_players, 0); // Ground crossing.
}
#[test]
fn complete_query_budgets_and_late_invalid_data_preserve_tick_rules_and_lives() {
    let mut g = game_at(&[Vec3::ZERO, Vec3::X * 5.]);
    g.spawn_player(0);
    corpse(&mut g, 0, 1);
    let before = checkpoint(&g);
    struct Count(usize);
    impl CombatRules for Count {
        fn input(&mut self, _: &Player, i: InputFrame) -> InputFrame {
            self.0 += 1;
            i
        }
    }
    let mut rules = Count(0);
    let zero = Policy::new(Config {
        clearance_candidates: 0,
        ..Config::default()
    })
    .unwrap();
    assert!(matches!(
        g.step_with_rules_and_spawn_policy(&[], &[], &mut rules, Some(&zero)),
        Err(Error::Clearance {
            index: 0,
            error: ClearanceError::QueryBudget
        })
    ));
    assert_eq!(before, checkpoint(&g));
    assert_eq!(rules.0, 0);
    // First candidate clears; second visits ground and a blocker. Exhaustion is
    // a typed failure even though the observed box is definitely blocking.
    g.arena.solids.push(Obstacle {
        center: Vec3::new(5., 1., 0.),
        half_extents: Vec3::ONE,
    });
    let before = checkpoint(&g);
    let one = Policy::new(Config {
        clearance_candidates: 1,
        ..Config::default()
    })
    .unwrap();
    assert!(matches!(
        g.step_with_spawn_policy(&[], &[], Some(&one)),
        Err(Error::Clearance {
            index: 1,
            error: ClearanceError::QueryBudget
        })
    ));
    assert_eq!(before, checkpoint(&g));
    let exact = Policy::new(Config {
        clearance_candidates: 2,
        minimum_horizontal_distance: 0.,
        ..Config::default()
    })
    .unwrap();
    assert_eq!(exact.select(&g, 0).unwrap().unwrap().index, 0);
    let los = Policy::new(Config {
        exposure_penalty: Some(1.),
        max_los_tests: 3,
        ..Config::default()
    })
    .unwrap();
    assert_eq!(
        g.step_with_spawn_policy(&[], &[], Some(&los)).unwrap_err(),
        Error::LosBudget {
            required: 4,
            provided: 3
        }
    );
    assert_eq!(before, checkpoint(&g));
    let exact_los = Policy::new(Config {
        exposure_penalty: Some(1.),
        max_los_tests: 4,
        ..Config::default()
    })
    .unwrap();
    assert!(exact_los.select(&g, 0).unwrap().is_some());
    g.arena.spawns[1].y = f32::NAN;
    let players = g.players.clone();
    let tick = g.tick;
    let epochs = g.life_epochs();
    assert!(matches!(
        g.step_with_spawn_policy(&[], &[], Some(&policy())),
        Err(Error::Arena(arena_authoring::ArenaAuthoringError::Spawn {
            index: 1,
            ..
        }))
    ));
    assert_eq!(players, g.players);
    assert_eq!(tick, g.tick);
    assert_eq!(epochs, g.life_epochs());
}
#[test]
fn invalid_settings_capacities_and_full_player_state_are_checked() {
    for config in [
        Config {
            minimum_horizontal_distance: f32::NAN,
            ..Config::default()
        },
        Config {
            minimum_horizontal_distance: -1.,
            ..Config::default()
        },
        Config {
            minimum_horizontal_distance: 8193.,
            ..Config::default()
        },
        Config {
            exposure_penalty: Some(f32::INFINITY),
            ..Config::default()
        },
        Config {
            exposure_penalty: Some(-1.),
            ..Config::default()
        },
        Config {
            exposure_penalty: Some(1e8 + 16.),
            ..Config::default()
        },
        Config {
            clearance_candidates: 1_000_001,
            ..Config::default()
        },
        Config {
            max_los_tests: spawn::MAX_LOS_TESTS + 1,
            ..Config::default()
        },
    ] {
        assert_eq!(Policy::new(config), Err(Error::InvalidConfig));
    }
    let p = policy();
    let mut g = game_at(&vec![Vec3::ZERO; MAX_PLAYERS + 1]);
    assert!(matches!(
        p.select(&g, 0),
        Err(Error::Arena(
            arena_authoring::ArenaAuthoringError::SpawnCapacity { .. }
        ))
    ));
    g.arena.spawns = vec![Vec3::ZERO];
    g.arena.solids = vec![
        Obstacle {
            center: Vec3::ZERO,
            half_extents: Vec3::ONE
        };
        257
    ];
    assert!(matches!(
        p.select(&g, 0),
        Err(Error::Arena(
            arena_authoring::ArenaAuthoringError::SolidCapacity { .. }
        ))
    ));
    g.arena.solids.clear();
    g.players = vec![Player::new(0, Vec3::ZERO); MAX_PLAYERS + 1];
    assert_eq!(p.select(&g, 0), Err(Error::PlayerCapacity));
    g.players = vec![Player::new(0, Vec3::ZERO), Player::new(0, Vec3::ZERO)];
    assert_eq!(
        p.select(&g, 0),
        Err(Error::Checkpoint(checkpoint::CheckpointError::Invalid))
    );
    g.players = vec![Player::new(0, Vec3::ZERO)];
    g.players[0].health = 0;
    g.players[0].velocity.y = f32::NAN;
    assert_eq!(
        p.select(&g, 1),
        Err(Error::Checkpoint(checkpoint::CheckpointError::Invalid))
    ); // Dead state validated too.
    g.players[0] = Player::new(0, Vec3::ZERO);
    let before = checkpoint(&g);
    for inputs in [
        vec![(0, InputFrame::default()); MAX_PLAYERS + 1],
        vec![(16, InputFrame::default())],
        vec![(0, InputFrame::default()); 2],
    ] {
        assert_eq!(
            g.step_with_spawn_policy(&inputs, &[], Some(&p))
                .unwrap_err(),
            Error::InvalidCommands
        );
        assert_eq!(before, checkpoint(&g));
    }
    assert_eq!(
        g.step_with_spawn_policy(&[], &[ShotTime { player: 0, tick: 0 }; 2], Some(&p))
            .unwrap_err(),
        Error::InvalidCommands
    );
    assert_eq!(before, checkpoint(&g));
}
#[test]
fn simultaneous_respawns_reserve_by_id_and_deferral_retries_without_life_reset() {
    let p = policy();
    let points = [Vec3::ZERO, Vec3::X * 4.];
    let mut a = game_at(&points);
    a.spawn_player(7);
    a.spawn_player(2);
    a.spawn_player(9);
    for id in [7, 2, 9] {
        corpse(&mut a, id, 1);
    }
    let mut b = Match::from_checkpoint(&checkpoint(&a)).unwrap();
    b.players.reverse();
    let epochs = a.life_epochs();
    let first = a.step_with_spawn_policy(&[], &[], Some(&p)).unwrap();
    let second = b.step_with_spawn_policy(&[], &[], Some(&p)).unwrap();
    assert_eq!(
        first.events,
        vec![
            GameEvent::Respawned { player: 2 },
            GameEvent::Respawned { player: 7 }
        ]
    );
    assert_eq!(first.events, second.events);
    for id in [2, 7, 9] {
        assert_eq!(a.player(id), b.player(id));
    }
    assert_eq!(a.player(2).unwrap().position, points[0]);
    assert_eq!(a.player(7).unwrap().position, points[1]);
    assert!(!a.player(9).unwrap().is_alive());
    assert_eq!(a.life_epochs()[9], epochs[9]);
    for _ in 0..4 {
        assert!(
            a.step_with_spawn_policy(&[], &[], Some(&p))
                .unwrap()
                .events
                .is_empty()
        );
    }
    assert_eq!(a.life_epochs()[9], epochs[9]);
    a.remove_player(2);
    assert_eq!(
        a.step_with_spawn_policy(&[], &[], Some(&p)).unwrap().events,
        vec![GameEvent::Respawned { player: 9 }]
    );
    assert_eq!(a.life_epochs()[9], epochs[9] + 1);
    a.remove_player(9);
    assert!(a.spawn_player_with_policy(9, &p).unwrap().is_some());
    assert_eq!(a.life_epochs()[9], epochs[9] + 2);
}
#[test]
fn occupied_policy_can_explicitly_allow_simultaneous_shared_points() {
    let mut g = game_at(&[Vec3::ZERO]);
    g.spawn_player(0);
    g.spawn_player(1);
    corpse(&mut g, 0, 1);
    corpse(&mut g, 1, 1);
    let p = Policy::new(Config {
        minimum_horizontal_distance: 0.,
        forbid_occupied: false,
        exposure_penalty: Some(1.),
        ..Config::default()
    })
    .unwrap();
    let events = g.step_with_spawn_policy(&[], &[], Some(&p)).unwrap().events;
    assert_eq!(
        events,
        vec![
            GameEvent::Respawned { player: 0 },
            GameEvent::Respawned { player: 1 }
        ]
    );
    assert_eq!(g.player(0).unwrap().position, g.player(1).unwrap().position);
}
#[test]
fn combat_gates_and_input_processing_are_preserved() {
    struct Rules {
        frozen: bool,
        auto: bool,
        weapon: bool,
        inputs: usize,
    }
    impl CombatRules for Rules {
        fn input(&mut self, _: &Player, mut input: InputFrame) -> InputFrame {
            self.inputs += 1;
            input.yaw = 1.;
            input
        }
        fn frozen(&self, _: u8) -> bool {
            self.frozen
        }
        fn auto_respawn(&self) -> bool {
            self.auto
        }
        fn weapon(&self, _: u8) -> WeaponSettings {
            WeaponSettings {
                magazine: if self.weapon { 30 } else { 0 },
                ..WeaponSettings::default()
            }
        }
    }
    let p = policy();
    let mut g = game_at(&[Vec3::ZERO]);
    g.spawn_player(0);
    corpse(&mut g, 0, 2);
    let mut r = Rules {
        frozen: true,
        auto: true,
        weapon: true,
        inputs: 0,
    };
    for _ in 0..2 {
        g.step_with_rules_and_spawn_policy(&[], &[], &mut r, Some(&p))
            .unwrap();
    }
    assert_eq!(g.player(0).unwrap().respawn_ticks, 2);
    assert!((g.player(0).unwrap().yaw - 1.).abs() < 1e-6);
    r.frozen = false;
    r.auto = false;
    g.step_with_rules_and_spawn_policy(&[], &[], &mut r, Some(&p))
        .unwrap();
    assert_eq!(g.player(0).unwrap().respawn_ticks, 2);
    r.auto = true;
    r.weapon = false;
    g.step_with_rules_and_spawn_policy(&[], &[], &mut r, Some(&p))
        .unwrap();
    assert_eq!(g.player(0).unwrap().respawn_ticks, 2);
    r.weapon = true;
    assert!(
        g.step_with_rules_and_spawn_policy(&[], &[], &mut r, Some(&p))
            .unwrap()
            .events
            .is_empty()
    );
    assert_eq!(
        g.step_with_rules_and_spawn_policy(&[], &[], &mut r, Some(&p))
            .unwrap()
            .events,
        vec![GameEvent::Respawned { player: 0 }]
    );
    assert_eq!(r.inputs, 6);
    // Respawn tick never consumes a fire/reload/movement command as a live tick.
    corpse(&mut g, 0, 1);
    let input = [(
        0,
        InputFrame {
            buttons: BUTTON_FIRE | BUTTON_RELOAD,
            move_x: 1.,
            ..InputFrame::default()
        },
    )];
    let step = g.step_with_spawn_policy(&input, &[], Some(&p)).unwrap();
    assert_eq!(step.events.len(), 1);
    assert_eq!(g.player(0).unwrap(), &Player::new(0, Vec3::ZERO));
}
#[test]
fn disabled_policy_retains_exact_legacy_order_state_events_and_rejections() {
    let mut legacy = Match::new(arena());
    legacy.spawn_player(1);
    legacy.spawn_player(0);
    let mut disabled = Match::from_checkpoint(&checkpoint(&legacy)).unwrap();
    for i in 0..260 {
        let input = [(
            0,
            InputFrame {
                sequence: i,
                buttons: BUTTON_FIRE,
                ..InputFrame::default()
            },
        )];
        let shot = [ShotTime {
            player: 0,
            tick: if i % 2 == 0 {
                i + 1
            } else {
                i.saturating_sub(2)
            },
        }];
        let a = legacy.step_with_rewind(&input, &shot);
        let b = disabled
            .step_with_spawn_policy(&input, &shot, None)
            .unwrap();
        assert_eq!(a.events, b.events);
        assert_eq!(a.rejected_shots, b.rejected_shots);
        assert_eq!(checkpoint(&legacy), checkpoint(&disabled));
    }
    // None even retains legacy duplicate-input sanitization and first-wins semantics.
    let bad = [
        (
            0,
            InputFrame {
                yaw: f32::NAN,
                ..InputFrame::default()
            },
        ),
        (0, InputFrame::default()),
    ];
    assert_eq!(
        legacy.step(&bad),
        disabled
            .step_with_spawn_policy(&bad, &[], None)
            .unwrap()
            .events
    );
    assert_eq!(checkpoint(&legacy), checkpoint(&disabled));
}
#[test]
fn accepted_respawn_cuts_rewind_life_while_deferred_respawn_preserves_it() {
    let p = policy();
    let mut g = Match::new(arena());
    g.spawn_player(0);
    g.spawn_player(1);
    g.step_with_spawn_policy(&[], &[], Some(&p)).unwrap();
    let old = g.tick;
    assert_eq!(g.rewind_hit_query(0, old, 0., 0.).unwrap().victim, Some(1));
    let events = g
        .step_with_spawn_policy(
            &[(
                0,
                InputFrame {
                    buttons: BUTTON_FIRE,
                    ..InputFrame::default()
                },
            )],
            &[],
            Some(&p),
        )
        .unwrap()
        .events;
    assert!(events.contains(&GameEvent::Killed {
        attacker: 0,
        victim: 1
    }));
    let epoch = g.life_epochs()[1];
    // Bring countdown to zero via actual fixed ticks, then occupy both candidates.
    g.arena.solids = vec![Obstacle {
        center: Vec3::new(0., 1., -8.),
        half_extents: Vec3::ONE,
    }];
    for _ in 0..RESPAWN_TICKS {
        assert!(
            !g.step_with_spawn_policy(&[], &[], Some(&p))
                .unwrap()
                .events
                .iter()
                .any(|e| matches!(e, GameEvent::Respawned { .. }))
        );
    }
    assert_eq!(g.player(1).unwrap().respawn_ticks, 0);
    assert_eq!(g.life_epochs()[1], epoch);
    g.arena.solids.clear();
    let before_respawn = g.tick;
    assert_eq!(
        g.step_with_spawn_policy(&[], &[], Some(&p)).unwrap().events,
        vec![GameEvent::Respawned { player: 1 }]
    );
    assert_eq!(g.life_epochs()[1], epoch + 1);
    assert_eq!(
        g.rewind_hit_query(0, before_respawn, 0., 0.)
            .unwrap()
            .victim,
        None
    );
    assert_eq!(
        g.rewind_hit_query(0, g.tick, 0., 0.).unwrap().victim,
        Some(1)
    );
}
#[test]
fn policy_replay_real_combat_respawn_seek_reset_and_context_rejection() {
    let p = Policy::new(Config {
        minimum_horizontal_distance: 0.,
        exposure_penalty: Some(5.),
        ..Config::default()
    })
    .unwrap();
    let mut g = Match::new(arena());
    g.spawn_player_with_policy(0, &p).unwrap().unwrap();
    g.spawn_player_with_policy(1, &p).unwrap().unwrap();
    let initial = checkpoint(&g);
    let mut duplicate = Match::from_checkpoint(&initial).unwrap();
    let mut duplicate_capture = spawn::Capture::new(
        &duplicate,
        p,
        19,
        Limits {
            checkpoint_interval: 17,
            ..Limits::default()
        },
    )
    .unwrap();
    let mut capture = spawn::Capture::new(
        &g,
        p,
        19,
        Limits {
            checkpoint_interval: 17,
            ..Limits::default()
        },
    )
    .unwrap();
    let mut respawns = 0;
    let mut kills = 0;
    for i in 0..420 {
        let commands = [(
            0,
            InputFrame {
                sequence: i,
                buttons: if i % 200 < 100 {
                    BUTTON_FIRE
                } else {
                    BUTTON_RELOAD
                },
                ..InputFrame::default()
            },
        )];
        let shots = [ShotTime {
            player: 0,
            tick: g.tick.saturating_sub(2),
        }];
        let step = g
            .step_with_spawn_policy(&commands, &shots, Some(&p))
            .unwrap();
        respawns += step
            .events
            .iter()
            .filter(|e| matches!(e, GameEvent::Respawned { .. }))
            .count();
        kills += step
            .events
            .iter()
            .filter(|e| matches!(e, GameEvent::Killed { .. }))
            .count();
        capture.push(&g, &commands, &shots, &step.events).unwrap();
        let repeated = duplicate
            .step_with_spawn_policy(&commands, &shots, Some(&p))
            .unwrap();
        assert_eq!(repeated.events, step.events);
        assert_eq!(repeated.rejected_shots, step.rejected_shots);
        duplicate_capture
            .push(&duplicate, &commands, &shots, &repeated.events)
            .unwrap();
    }
    assert!(respawns > 0 && kills > 0);
    assert_eq!(capture.tick_count(), 420);
    let encoded = capture.finish().encode();
    assert_eq!(encoded, duplicate_capture.finish().encode()); // Same seed/order/state/config.
    let tape = Arc::new(Recording::decode(&encoded).unwrap());
    assert_eq!(
        (
            tape.header().schema,
            tape.header().version,
            tape.header().seed
        ),
        (spawn::SCHEMA, 1, 19)
    );
    assert_ne!(tape.header().schema, replay::SCHEMA);
    let mut world = spawn::World::new(Match::new(g.arena.clone()), p).unwrap();
    let mut playback = Playback::new(tape.clone());
    assert_eq!(world.policy(), p);
    playback.start(&mut world).unwrap();
    assert_eq!(checkpoint(&world.game), initial);
    while !playback.finished() {
        playback.step(&mut world).unwrap();
    }
    assert_eq!(checkpoint(&world.game), checkpoint(&g));
    for target in [0, 1, 191, 200, 419, 420] {
        playback.seek(&mut world, target).unwrap();
        while playback.seeking() {
            assert!(playback.pump_seek(&mut world, 3).unwrap() <= 3);
        }
        while !playback.finished() {
            playback.step(&mut world).unwrap();
        }
        assert_eq!(checkpoint(&world.game), checkpoint(&g));
    }
    playback.reset();
    playback.start(&mut world).unwrap();
    while !playback.finished() {
        playback.step(&mut world).unwrap();
    }
    assert_eq!(checkpoint(&world.game), checkpoint(&g));
    let c = p.config();
    for c in [
        Config {
            minimum_horizontal_distance: 1.,
            ..c
        },
        Config {
            forbid_occupied: false,
            ..c
        },
        Config {
            exposure_penalty: None,
            ..c
        },
        Config {
            exposure_penalty: Some(6.),
            ..c
        },
        Config {
            clearance_candidates: c.clearance_candidates - 1,
            ..c
        },
        Config {
            max_los_tests: c.max_los_tests - 1,
            ..c
        },
    ] {
        let different = Policy::new(c).unwrap();
        let mut world = spawn::World::new(Match::new(g.arena.clone()), different).unwrap();
        let before = checkpoint(&world.game);
        let mut playback = Playback::new(tape.clone());
        assert_eq!(playback.start(&mut world), Err(PlaybackError::Incompatible));
        assert_eq!(before, checkpoint(&world.game));
    }
    let mut legacy = replay::World::new(Match::new(g.arena.clone()));
    let before = checkpoint(&legacy.game);
    assert_eq!(
        Playback::new(tape).start(&mut legacy),
        Err(PlaybackError::Incompatible)
    );
    assert_eq!(before, checkpoint(&legacy.game));
}
#[test]
fn policy_capture_failures_preserve_prefix_and_world_rejects_legacy_schema() {
    let p = policy();
    let mut g = Match::new(arena());
    g.spawn_player(0);
    let mut capture = spawn::Capture::new(
        &g,
        p,
        1,
        Limits {
            ticks: 1,
            ..Limits::default()
        },
    )
    .unwrap();
    let step = g.step_with_spawn_policy(&[], &[], Some(&p)).unwrap();
    capture.push(&g, &[], &[], &step.events).unwrap();
    let next = g.step_with_spawn_policy(&[], &[], Some(&p)).unwrap();
    assert_eq!(
        capture.push(&g, &[], &[], &next.events),
        Err(Error::Replay(replay::Error::Timeline(
            ferravex_play::replay::ReplayError::Capacity
        )))
    );
    assert_eq!(capture.tick_count(), 1);
    let prefix = capture.finish();
    assert_eq!(prefix.tick_count(), 1);
    let mut capture = spawn::Capture::new(&g, p, 2, Limits::default()).unwrap();
    g.arena.spawns[0].x += 1.;
    let before = g.state_hash();
    assert_eq!(capture.push(&g, &[], &[], &[]), Err(Error::ContextChanged));
    assert_eq!(capture.tick_count(), 0);
    assert_eq!(before, g.state_hash());
    let mut capture = spawn::Capture::new(&g, p, 2, Limits::default()).unwrap();
    g.step_with_spawn_policy(&[], &[], Some(&p)).unwrap();
    assert_eq!(
        capture.push(&g, &[(0, InputFrame::default()); 2], &[], &[]),
        Err(Error::Replay(replay::Error::InvalidInput))
    );
    assert_eq!(capture.tick_count(), 0);
    capture.push(&g, &[], &[], &[]).unwrap();
    assert_eq!(capture.tick_count(), 1);
    let legacy = Arc::new(
        replay::Capture::new(&g, 1, Limits::default())
            .unwrap()
            .finish(),
    );
    let mut w = spawn::World::new(Match::new(g.arena.clone()), p).unwrap();
    let before = checkpoint(&w.game);
    assert_eq!(
        Playback::new(legacy).start(&mut w),
        Err(PlaybackError::Incompatible)
    );
    assert_eq!(before, checkpoint(&w.game));
    let wrong = checkpoint(&Match::new(Arena::default()));
    assert_eq!(w.restore(&wrong), Err(Error::ContextChanged));
    assert_eq!(before, checkpoint(&w.game));
}
#[test]
fn exact_capacity_and_initial_join_reuse_preserve_generations() {
    let mut g = game_at(
        &(0..MAX_PLAYERS)
            .map(|i| Vec3::X * (i as f32 * 2.))
            .collect::<Vec<_>>(),
    );
    // Max solids far from all spawn volumes; max authored count remains usable.
    g.arena.solids = vec![
        Obstacle {
            center: Vec3::new(100., 1., 100.),
            half_extents: Vec3::ONE
        };
        256
    ];
    let p = Policy::new(Config {
        minimum_horizontal_distance: 0.,
        exposure_penalty: Some(0.),
        ..Config::default()
    })
    .unwrap();
    for id in 0..MAX_PLAYERS as u8 {
        assert!(g.spawn_player_with_policy(id, &p).unwrap().is_some());
    }
    assert_eq!(g.players.len(), MAX_PLAYERS);
    assert!(g.life_epochs().iter().all(|epoch| *epoch == 1));
    assert_eq!(
        g.spawn_player_with_policy(0, &p),
        Err(Error::PlayerUnavailable)
    );
    g.remove_player(3);
    assert!(g.spawn_player_with_policy(3, &p).unwrap().is_some());
    assert_eq!(g.life_epochs()[3], 2);
    g.step_with_spawn_policy(&[], &[], Some(&p)).unwrap();
    assert_eq!(g.tick, 1);
    let restored = Match::from_checkpoint(&checkpoint(&g)).unwrap();
    assert_eq!(restored.state_hash(), g.state_hash());
}
#[test]
fn recent_previous_alive_life_is_never_hit_after_policy_respawn() {
    let p = policy();
    let mut g = Match::new(arena());
    g.spawn_player(0);
    g.spawn_player(1);
    g.step_with_spawn_policy(&[], &[], Some(&p)).unwrap();
    let old = g.tick;
    let killed = g
        .step_with_spawn_policy(
            &[(
                0,
                InputFrame {
                    buttons: BUTTON_FIRE,
                    ..InputFrame::default()
                },
            )],
            &[],
            Some(&p),
        )
        .unwrap();
    assert!(killed.events.contains(&GameEvent::Killed {
        attacker: 0,
        victim: 1
    }));
    // Short countdown fixture keeps the old alive pose within the real rewind window.
    g.player_mut(1).unwrap().respawn_ticks = 1;
    let epoch = g.life_epochs()[1];
    assert_eq!(
        g.step_with_spawn_policy(&[], &[], Some(&p)).unwrap().events,
        vec![GameEvent::Respawned { player: 1 }]
    );
    assert_eq!(g.life_epochs()[1], epoch + 1);
    assert_eq!(g.rewind_hit_query(0, old, 0., 0.).unwrap().victim, None);
    assert_eq!(
        g.rewind_hit_query(0, g.tick, 0., 0.).unwrap().victim,
        Some(1)
    );
}
#[test]
fn empty_policy_respawn_defers_but_legacy_still_uses_origin_and_no_sticky_policy() {
    let p = policy();
    let mut g = game_at(&[]);
    assert!(g.spawn_player(0));
    corpse(&mut g, 0, 1);
    let epoch = g.life_epochs()[0];
    for _ in 0..3 {
        assert!(
            g.step_with_spawn_policy(&[], &[], Some(&p))
                .unwrap()
                .events
                .is_empty()
        );
    }
    assert_eq!(g.life_epochs()[0], epoch);
    assert_eq!(g.player(0).unwrap().health, 0);
    assert_eq!(g.tick, 3);
    assert_eq!(g.step(&[]), vec![GameEvent::Respawned { player: 0 }]);
    assert_eq!(g.life_epochs()[0], epoch + 1);
    assert_eq!(g.player(0).unwrap().position, Vec3::ZERO);
}
#[test]
fn replay_shared_validation_rejects_bad_input_and_event_before_a_tick() {
    use ferravex_play::replay::{EventInput, Header, Recorder};
    let p = policy();
    let game = game_at(&[Vec3::ZERO]);
    let mut world = spawn::World::new(game, p).unwrap();
    let before = checkpoint(&world.game);
    let mut recorder = Recorder::new(
        Header {
            schema: spawn::SCHEMA,
            version: 1,
            context: p.context_hash(&world.game).unwrap(),
            tick_hz: 64,
            seed: 0,
        },
        Limits::default(),
        0,
        &before,
        world.game.state_hash().unwrap(),
    )
    .unwrap();
    let mut payload = vec![];
    NetworkEvent {
        tick: 2,
        event: GameEvent::Respawned { player: 0 },
    }
    .encode_into(&mut payload)
    .unwrap();
    recorder
        .push(
            1,
            &[0, 0],
            &[EventInput {
                kind: 1,
                payload: &payload,
            }],
            None,
            0,
        )
        .unwrap();
    let wrong_tick = recorder.finish();
    assert_eq!(
        world.step(&[0, 0], wrong_tick.events_at(1)),
        Err(Error::Replay(replay::Error::InvalidInput))
    );
    assert_eq!(before, checkpoint(&world.game));
    let empty = wrong_tick.events_at(0);
    for input in [
        &[][..],
        &[17][..],
        &[0, 17][..],
        &[0, 0, 0][..],
        &[1, 255][..],
    ] {
        assert!(world.step(input, empty.clone()).is_err());
        assert_eq!(before, checkpoint(&world.game));
    }
    // A well-formed but unaccepted Respawned event is detected after simulation,
    // exactly like legacy replay's event comparator (no invented authority spawn).
    payload.clear();
    NetworkEvent {
        tick: 1,
        event: GameEvent::Respawned { player: 0 },
    }
    .encode_into(&mut payload)
    .unwrap();
    let mut recorder = Recorder::new(
        wrong_tick.header(),
        Limits::default(),
        0,
        &before,
        world.game.state_hash().unwrap(),
    )
    .unwrap();
    recorder
        .push(
            1,
            &[0, 0],
            &[EventInput {
                kind: 1,
                payload: &payload,
            }],
            None,
            0,
        )
        .unwrap();
    let fake = recorder.finish();
    assert_eq!(
        world.step(&[0, 0], fake.events_at(1)),
        Err(Error::Replay(replay::Error::EventsDiffer))
    );
    assert_eq!(world.game.tick, 1);
    assert!(world.game.players.is_empty());
}

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
struct CountingAllocator;
thread_local! { static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) }; }
fn count_allocation() {
    let _ = ALLOCATIONS.try_with(|v| {
        if let Some(n) = v.get() {
            v.set(Some(n + 1));
        }
    });
}
// Forward the system allocator's contract, counting only this test thread.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        count_allocation();
        unsafe { System.realloc(p, l, n) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
#[test]
fn selection_context_and_rejected_tick_are_allocation_free_after_setup() {
    let mut g = game_at(
        &(0..MAX_PLAYERS)
            .map(|i| Vec3::X * i as f32 * 2.)
            .collect::<Vec<_>>(),
    );
    g.arena.solids = vec![
        Obstacle {
            center: Vec3::new(100., 1., 100.),
            half_extents: Vec3::ONE
        };
        256
    ];
    g.spawn_player(0);
    let p = Policy::new(Config {
        minimum_horizontal_distance: 0.,
        exposure_penalty: Some(1.),
        ..Config::default()
    })
    .unwrap();
    let invalid = Policy::new(Config {
        clearance_candidates: 0,
        ..Config::default()
    })
    .unwrap();
    p.select(&g, 1).unwrap();
    p.context_hash(&g).unwrap();
    ALLOCATIONS.with(|n| n.set(Some(0)));
    for _ in 0..100 {
        assert!(p.select(&g, 1).unwrap().is_some());
        assert!(p.context_hash(&g).is_ok());
        assert!(matches!(
            g.step_with_spawn_policy(&[], &[], Some(&invalid)),
            Err(Error::Clearance {
                error: ClearanceError::QueryBudget,
                ..
            })
        ));
        assert!(
            Policy::new(p.config())
                .unwrap()
                .select(&g, 1)
                .unwrap()
                .is_some()
        );
    }
    let allocations = ALLOCATIONS.with(|n| n.replace(None).unwrap());
    assert_eq!(allocations, 0);
    assert_eq!(g.tick, 0);
}
#[test]
fn blocked_candidate_never_masks_later_query_exhaustion_at_maximum_counts() {
    let mut g = game_at(&[Vec3::ZERO]);
    g.spawn_player(0);
    corpse(&mut g, 0, 1);
    g.arena.solids = vec![
        Obstacle {
            center: Vec3::new(0., 1., 0.),
            half_extents: Vec3::ONE
        };
        256
    ];
    let before = checkpoint(&g);
    let short = Policy::new(Config {
        clearance_candidates: 256,
        ..Config::default()
    })
    .unwrap();
    assert_eq!(
        g.step_with_spawn_policy(&[], &[], Some(&short))
            .unwrap_err(),
        Error::Clearance {
            index: 0,
            error: ClearanceError::QueryBudget
        }
    );
    assert_eq!(before, checkpoint(&g));
    assert_eq!(policy().select(&g, 0), Ok(None)); // Ground plus all 256 blockers completed.
    g.arena.solids.clear();
    g.arena.spawns = (0..MAX_PLAYERS).map(|i| Vec3::X * i as f32 * 2.).collect();
    for id in 1..MAX_PLAYERS as u8 {
        g.spawn_player(id);
    }
    g.player_mut(0).unwrap().health = 100;
    let before = checkpoint(&g);
    let short = Policy::new(Config {
        exposure_penalty: Some(0.),
        max_los_tests: 511,
        ..Config::default()
    })
    .unwrap();
    assert_eq!(
        g.step_with_spawn_policy(&[], &[], Some(&short))
            .unwrap_err(),
        Error::LosBudget {
            required: 512,
            provided: 511
        }
    );
    assert_eq!(before, checkpoint(&g));
}
#[test]
fn policy_replay_and_accepted_respawn_cross_tick_wrap_without_checkpoint_changes() {
    let p = policy();
    let mut g = Match::new(arena());
    g.tick = u32::MAX - 1;
    g.spawn_player(0);
    g.spawn_player(1);
    corpse(&mut g, 1, 2);
    let mut capture = spawn::Capture::new(
        &g,
        p,
        9,
        Limits {
            checkpoint_interval: 2,
            ..Limits::default()
        },
    )
    .unwrap();
    for _ in 0..4 {
        let events = g.step_with_spawn_policy(&[], &[], Some(&p)).unwrap().events;
        capture.push(&g, &[], &[], &events).unwrap();
    }
    assert_eq!(g.tick, 2);
    assert_eq!(g.life_epochs()[1], 2);
    let tape = Arc::new(Recording::decode(&capture.finish().encode()).unwrap());
    let mut w = spawn::World::new(Match::new(g.arena.clone()), p).unwrap();
    let mut playback = Playback::new(tape);
    playback.start(&mut w).unwrap();
    while !playback.finished() {
        playback.step(&mut w).unwrap();
    }
    assert_eq!(checkpoint(&g), checkpoint(&w.game));
}
