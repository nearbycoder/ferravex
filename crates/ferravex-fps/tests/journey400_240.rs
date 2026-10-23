use ferravex_fps::*;
use ferravex_math::Vec3;
use ferravex_physics::character::{ClearanceError, JumpState};
use ferravex_play::replay::{Limits, Playback};
use std::{cell::Cell, sync::Arc};

fn game() -> Match {
    let mut game = Match::new(Arena {
        solids: vec![],
        spawns: vec![Vec3::ZERO, Vec3::new(0., 0., -5.), Vec3::new(4., 0., -5.)],
    });
    for id in 0..3 {
        assert!(game.spawn_player(id));
    }
    game
}
fn bytes(game: &Match) -> Vec<u8> {
    let mut bytes = Vec::new();
    game.checkpoint_into(&mut bytes).unwrap();
    bytes
}
fn command(buttons: u8) -> InputFrame {
    InputFrame {
        buttons,
        ..Default::default()
    }
}
fn static_only(motion: RelocationMotion) -> RelocationOptions {
    RelocationOptions {
        motion,
        occupancy: RelocationOccupancy::StaticOnly,
        ..Default::default()
    }
}
fn reject(game: &mut Match, id: u8, at: Vec3, options: RelocationOptions, wanted: RelocationError) {
    let before = bytes(game);
    let pointer = game.players.as_ptr();
    let capacity = game.players.capacity();
    assert_eq!(game.relocate_player(id, at, options), Err(wanted));
    assert_eq!(bytes(game), before);
    assert_eq!(game.players.as_ptr(), pointer);
    assert_eq!(game.players.capacity(), capacity);
}

#[test]
fn motion_policies_preserve_health_weapons_aim_and_reset_only_requested_controller() {
    for motion in [RelocationMotion::Reset, RelocationMotion::Preserve] {
        let mut game = game();
        let p = game.player_mut(1).unwrap();
        p.velocity = Vec3::new(2., 3., 4.);
        p.jump = JumpState {
            coyote: 0.08,
            buffer: 0.07,
            held: true,
        };
        p.grounded = true;
        p.yaw = 0.7;
        p.pitch = 0.3;
        p.health = 37;
        p.ammo = 4;
        p.reload_ticks = 12;
        p.fire_cooldown = 7;
        p.respawn_ticks = 9;
        let before = p.clone();
        let other = game.player(2).unwrap().clone();
        let epochs = game.life_epochs();
        let at = Vec3::new(8., 5., -6.);
        let accepted = game.relocate_player(1, at, static_only(motion)).unwrap();
        assert_eq!(
            accepted,
            Relocation {
                player: 1,
                position: at,
                yaw: before.yaw,
                pitch: before.pitch,
                tick: 0,
                life: 2
            }
        );
        let mut expected = before;
        expected.position = at;
        if motion == RelocationMotion::Reset {
            expected.velocity = Vec3::ZERO;
            expected.grounded = false;
            expected.jump = JumpState::default();
        }
        assert_eq!(game.player(1).unwrap(), &expected);
        assert_eq!(game.player(2).unwrap(), &other);
        assert_eq!(game.life_epochs()[0], epochs[0]);
        assert_eq!(game.life_epochs()[1], epochs[1] + 1);
        assert_eq!(game.life_epochs()[2], epochs[2]);
        assert_eq!(game.tick, 0);
    }
}

#[test]
fn invalid_destination_actor_dead_and_options_are_atomic() {
    let mut game = game();
    game.step(&[]);
    let defaults = RelocationOptions::default();
    for axis in 0..3 {
        for value in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            4096.01,
            -4096.01,
            f32::MAX,
        ] {
            let mut at = Vec3::ZERO;
            at[axis] = value;
            reject(
                &mut game,
                1,
                at,
                defaults,
                RelocationError::InvalidDestination,
            );
        }
    }
    for id in [MAX_PLAYERS as u8, u8::MAX] {
        reject(
            &mut game,
            id,
            Vec3::ZERO,
            defaults,
            RelocationError::InvalidPlayerId,
        );
    }
    reject(
        &mut game,
        4,
        Vec3::ZERO,
        defaults,
        RelocationError::UnknownPlayer,
    );
    reject(
        &mut game,
        1,
        Vec3::ZERO,
        RelocationOptions {
            max_candidates: 1_000_001,
            ..defaults
        },
        RelocationError::InvalidOptions,
    );
    game.player_mut(1).unwrap().health = 0;
    game.player_mut(1).unwrap().respawn_ticks = 0;
    reject(
        &mut game,
        1,
        Vec3::ZERO,
        defaults,
        RelocationError::PlayerDead,
    );
    // Relocation does not take ownership of respawn; the existing tick still does.
    game.step(&[]);
    assert!(game.player(1).unwrap().is_alive());
}

#[test]
fn actual_capsule_clearance_budget_and_touching_are_checked_without_recovery() {
    let mut game = game();
    let options = static_only(RelocationMotion::Reset);
    reject(
        &mut game,
        1,
        Vec3::ZERO,
        RelocationOptions {
            max_candidates: 0,
            ..options
        },
        RelocationError::Clearance(ClearanceError::QueryBudget),
    );
    reject(
        &mut game,
        1,
        Vec3::new(10., -0.1, 0.),
        options,
        RelocationError::Clearance(ClearanceError::Blocked { obstacle: 0 }),
    );
    // Elevated obstacle outside the character must not be flattened to XZ.
    game.arena.solids.push(Obstacle {
        center: Vec3::new(10., 4., 0.),
        half_extents: Vec3::ONE,
    });
    let accepted = game
        .relocate_player(1, Vec3::new(10., 0., 0.), options)
        .unwrap();
    assert_eq!(accepted.position, Vec3::new(10., 0., 0.));
    game.arena.solids.push(Obstacle {
        center: Vec3::new(10., 1., 0.),
        half_extents: Vec3::ONE,
    });
    reject(
        &mut game,
        1,
        Vec3::new(10., 0., 0.),
        RelocationOptions {
            max_candidates: 1,
            ..options
        },
        RelocationError::Clearance(ClearanceError::QueryBudget),
    );
    reject(
        &mut game,
        1,
        Vec3::new(10., 0., 0.),
        options,
        RelocationError::Clearance(ClearanceError::Blocked { obstacle: 2 }),
    );
    // Exact side contact is clear; do not change the accepted feet coordinate.
    let touch = Vec3::new(11. + PLAYER_RADIUS, 0., 0.);
    assert_eq!(
        game.relocate_player(1, touch, options).unwrap().position,
        touch
    );
}

#[test]
fn dynamic_occupancy_is_explicit_conservative_and_ignores_self_and_dead() {
    let mut game = game();
    let options = RelocationOptions::default();
    let current = game.player(1).unwrap().position;
    assert!(game.relocate_player(1, current, options).is_ok());
    reject(
        &mut game,
        1,
        Vec3::ZERO,
        options,
        RelocationError::Occupied { player: 0 },
    );
    assert!(
        game.relocate_player(1, Vec3::ZERO, static_only(RelocationMotion::Preserve))
            .is_ok()
    );
    // Exact AABB contact on X or Y is allowed.
    assert!(
        game.relocate_player(1, Vec3::X * (2. * PLAYER_RADIUS), options)
            .is_ok()
    );
    assert!(
        game.relocate_player(1, Vec3::Y * PLAYER_HEIGHT, options)
            .is_ok()
    );
    game.player_mut(0).unwrap().health = 0;
    assert!(game.relocate_player(1, Vec3::ZERO, options).is_ok());
    // Roster order does not affect which living blocker is reported.
    game.player_mut(0).unwrap().health = 100;
    game.player_mut(2).unwrap().position = Vec3::ZERO;
    game.players.reverse();
    reject(
        &mut game,
        1,
        Vec3::ZERO,
        options,
        RelocationError::Occupied { player: 0 },
    );
}

#[test]
fn stale_target_life_is_excluded_same_tick_refresh_and_other_actor_history_survives() {
    let mut game = game();
    game.step(&[]);
    let old = game.tick;
    assert_eq!(
        game.rewind_hit_query(0, old, 0., 0.).unwrap().victim,
        Some(1)
    );
    let other_yaw = 4.0f32.atan2(5.);
    assert_eq!(
        game.rewind_hit_query(0, old, other_yaw, 0.).unwrap().victim,
        Some(2)
    );
    game.step(&[]);
    let tick = game.tick;
    let options = static_only(RelocationMotion::Reset);
    let first = game
        .relocate_player(1, Vec3::new(7., 0., -5.), options)
        .unwrap();
    assert_eq!(game.rewind_hit_query(0, old, 0., 0.).unwrap().victim, None);
    assert_eq!(
        game.rewind_hit_query(0, old, other_yaw, 0.).unwrap().victim,
        Some(2)
    );
    assert_eq!(game.rewind_hit_query(0, tick, 0., 0.).unwrap().victim, None);
    let latest = game
        .relocate_player(1, Vec3::new(0., 0., -3.), options)
        .unwrap();
    assert_eq!((latest.tick, latest.life), (first.tick, first.life + 1));
    assert_eq!(
        game.rewind_hit_query(0, tick, 0., 0.).unwrap().victim,
        Some(1)
    );
    assert_eq!(game.rewind_hit_query(0, old, 0., 0.).unwrap().victim, None);
    assert_eq!(
        game.rewind_hit_query(0, old, other_yaw, 0.).unwrap().victim,
        Some(2)
    );
    // Ordinary untimed trace also uses the accepted destination, on actual fire.
    let mut output = StepOutput::new();
    game.step_into(&[(0, command(BUTTON_FIRE))], &mut output)
        .unwrap();
    assert!(
        output
            .events()
            .iter()
            .any(|e| matches!(e, GameEvent::Hit { victim: 1, .. }))
    );
}

#[test]
fn stale_timed_shot_cannot_damage_relocated_target_but_other_history_is_valid() {
    let mut game = game();
    game.step(&[]);
    let old = game.tick;
    game.step(&[]);
    let old_health = game.player(1).unwrap().health;
    game.relocate_player(
        1,
        Vec3::new(0., 0., -3.),
        static_only(RelocationMotion::Reset),
    )
    .unwrap();
    let mut output = StepOutput::new();
    game.step_with_rewind_into(
        &[(0, command(BUTTON_FIRE))],
        &[ShotTime {
            player: 0,
            tick: old,
        }],
        &mut output,
    )
    .unwrap();
    assert_eq!(game.player(1).unwrap().health, old_health);
    assert!(output.rejected_shots().is_empty());
    assert!(
        !output
            .events()
            .iter()
            .any(|e| matches!(e, GameEvent::Hit { victim: 1, .. }))
    );
}

#[test]
fn retained_output_survives_relocation_and_next_tick_movement_uses_new_generation_both_paths() {
    for policy in [
        None,
        Some(spawn::Policy::new(spawn::Config::default()).unwrap()),
    ] {
        let mut game = game();
        let mut output = StepOutput::new();
        game.step_with_rewind_into(
            &[
                (
                    0,
                    InputFrame {
                        yaw: std::f32::consts::PI,
                        ..command(BUTTON_FIRE | BUTTON_JUMP)
                    },
                ),
                (
                    2,
                    InputFrame {
                        yaw: std::f32::consts::PI,
                        ..command(BUTTON_FIRE)
                    },
                ),
            ],
            &[ShotTime { player: 0, tick: 1 }],
            &mut output,
        )
        .unwrap();
        let events = output.events().to_vec();
        let rejections = output.rejected_shots().to_vec();
        let records = output.transitions().to_vec();
        assert!(!events.is_empty() && !rejections.is_empty() && !records.is_empty());
        let capacities = output.capacities();
        let pointer = output.events().as_ptr();
        let accepted = game
            .relocate_player(1, Vec3::new(10., 0., -5.), RelocationOptions::default())
            .unwrap();
        assert_eq!(output.events(), events);
        assert_eq!(output.rejected_shots(), rejections);
        assert_eq!(output.transitions(), records);
        assert_eq!(output.capacities(), capacities);
        reject(
            &mut game,
            1,
            Vec3::ZERO,
            RelocationOptions {
                max_candidates: 0,
                ..Default::default()
            },
            RelocationError::Clearance(ClearanceError::QueryBudget),
        );
        assert_eq!(output.events(), events);
        assert_eq!(output.rejected_shots(), rejections);
        assert_eq!(output.transitions(), records);
        assert_eq!(output.events().as_ptr(), pointer);
        game.step_with_spawn_policy_into(
            &[(
                1,
                InputFrame {
                    move_x: 1.,
                    ..command(BUTTON_FIRE | BUTTON_JUMP)
                },
            )],
            &[],
            policy.as_ref(),
            &mut output,
        )
        .unwrap();
        let moved = game.player(1).unwrap();
        assert!(moved.position.x > accepted.position.x);
        let record = output.transitions().iter().find(|r| r.player == 1).unwrap();
        assert_eq!(
            (record.life, record.tick),
            (accepted.life, accepted.tick.wrapping_add(1))
        );
        assert!(record.transitions.fired);
        output.clear();
        assert!(output.transitions().is_empty());
        assert_eq!(output.capacities(), capacities);
    }
}

#[test]
fn checkpoint_restore_tick_wrap_and_new_recording_segment_preserve_boundary() {
    for start in [0, u32::MAX] {
        let mut game = game();
        game.tick = start; // Empty history: explicit wrap test fixture.
        game.step(&[]);
        let old = game.tick;
        game.step(&[]);
        let accepted = game
            .relocate_player(1, Vec3::new(0., 0., -3.), RelocationOptions::default())
            .unwrap();
        let checkpoint = bytes(&game);
        let mut restored = Match::from_checkpoint(&checkpoint).unwrap();
        assert_eq!(bytes(&restored), checkpoint);
        assert_eq!(
            restored.rewind_hit_query(0, old, 0., 0.).unwrap().victim,
            None
        );
        assert_eq!(
            restored
                .rewind_hit_query(0, accepted.tick, 0., 0.)
                .unwrap()
                .victim,
            Some(1)
        );
        let again = restored
            .relocate_player(1, Vec3::new(0., 0., -4.), RelocationOptions::default())
            .unwrap();
        assert_eq!(again.life, accepted.life + 1);
        // Unrecorded external operation is BEFORE the fresh Capture checkpoint.
        let mut capture = replay::Capture::new(&restored, 19, Limits::default()).unwrap();
        let mut output = StepOutput::new();
        for sequence in 0..20 {
            let commands = [(
                1,
                InputFrame {
                    sequence,
                    move_x: 0.5,
                    ..Default::default()
                },
            )];
            restored.step_into(&commands, &mut output).unwrap();
            capture
                .push(&restored, &commands, &[], output.events())
                .unwrap();
        }
        let mut playback = Playback::new(Arc::new(capture.finish()));
        let mut world = replay::World::new(game);
        for _ in 0..2 {
            playback.start(&mut world).unwrap();
            while !playback.finished() {
                playback.step(&mut world).unwrap();
            }
            assert_eq!(world.game.state_hash(), restored.state_hash());
            playback.reset();
        }
    }
}

#[test]
fn invalid_full_state_and_capacity_do_not_capture_or_repair() {
    let mut game = game();
    game.players.push(game.players[0].clone()); // Duplicate ID, otherwise finite.
    let before = game.players.clone();
    assert!(matches!(
        game.relocate_player(1, Vec3::X, RelocationOptions::default()),
        Err(RelocationError::InvalidState(_))
    ));
    assert_eq!(game.players, before);
    assert_eq!(game.tick, 0);
    game.players.pop();
    game.players
        .extend((3..=MAX_PLAYERS).map(|id| Player::new(id as u8, Vec3::X * id as f32)));
    let before = game.players.clone();
    assert_eq!(
        game.relocate_player(1, Vec3::X, RelocationOptions::default()),
        Err(RelocationError::InvalidState(
            checkpoint::CheckpointError::Capacity
        ))
    );
    assert_eq!(game.players, before);
    let mut game = Match::new(Arena {
        solids: vec![],
        spawns: vec![],
    });
    assert!(game.spawn_player(0)); // Preserved explicit legacy origin allowance.
    assert!(
        game.relocate_player(0, Vec3::X, RelocationOptions::default())
            .is_ok()
    );
    game.arena.solids.push(Obstacle {
        center: Vec3::ZERO,
        half_extents: Vec3::ZERO,
    });
    let players = game.players.clone();
    assert!(matches!(
        game.relocate_player(0, Vec3::X * 2., RelocationOptions::default()),
        Err(RelocationError::Arena(_))
    ));
    assert_eq!(game.players, players);
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
fn warmed_max_roster_relocation_rejections_and_retained_ticks_allocate_zero() {
    let mut game = Match::new(Arena {
        solids: vec![
            Obstacle {
                center: Vec3::new(500., 1., 500.),
                half_extents: Vec3::ONE
            };
            256
        ],
        spawns: (0..MAX_PLAYERS).map(|i| Vec3::X * (i * 8) as f32).collect(),
    });
    for id in 0..MAX_PLAYERS {
        assert!(game.spawn_player(id as u8));
    }
    let mut output = StepOutput::new();
    let capacities = output.capacities();
    for _ in 0..32 {
        game.step_into(&[], &mut output).unwrap();
    }
    let pointer = game.players.as_ptr();
    let capacity = game.players.capacity();
    ALLOCATIONS.with(|v| v.set(Some(0)));
    for i in 0..512 {
        for id in 0..MAX_PLAYERS {
            let at = Vec3::new((id * 8) as f32, 0., if i % 2 == 0 { 4. } else { -4. });
            game.relocate_player(id as u8, at, RelocationOptions::default())
                .unwrap();
            game.relocate_player(id as u8, at, static_only(RelocationMotion::Preserve))
                .unwrap();
            assert_eq!(
                game.relocate_player(
                    id as u8,
                    at,
                    RelocationOptions {
                        max_candidates: 0,
                        ..Default::default()
                    }
                ),
                Err(RelocationError::Clearance(ClearanceError::QueryBudget))
            );
        }
        game.step_into(&[(0, command(BUTTON_FIRE))], &mut output)
            .unwrap();
    }
    let allocations = ALLOCATIONS.with(|v| v.replace(None).unwrap());
    assert_eq!(allocations, 0);
    assert_eq!(output.capacities(), capacities);
    assert_eq!(game.players.as_ptr(), pointer);
    assert_eq!(game.players.capacity(), capacity);
    println!(
        "512 warmed ticks + 16384 accepted relocations + 8192 rejected queries: allocations={allocations}"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn actual_udp_snapshots_deliver_relocation_epoch_to_both_remote_interpolation_paths() {
    use std::time::{Duration, Instant};
    let arena = game().arena;
    let mut server = GameServer::with_arena("127.0.0.1:0", arena.clone()).unwrap();
    let mut client = GameClient::with_arena(server.local_addr().unwrap(), arena).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !client.is_connected() {
        server.poll().unwrap();
        client.poll().unwrap();
        assert!(Instant::now() < deadline, "loopback connection timeout");
        std::thread::sleep(Duration::from_millis(1));
    }
    let remote = if client.player_id() == Some(0) { 1 } else { 0 };
    assert!(server.game.spawn_player(remote));
    for _ in 0..3 {
        server.tick().unwrap();
    }
    let wanted = server.game.tick;
    let deadline = Instant::now() + Duration::from_secs(2);
    while client.server_tick() != wanted {
        client.poll().unwrap();
        assert!(Instant::now() < deadline, "initial snapshot timeout");
        std::thread::sleep(Duration::from_millis(1));
    }
    let before = client
        .players()
        .iter()
        .find(|p| p.id == remote)
        .unwrap()
        .position;
    // Small move avoids the old >5-unit distance fallback masking missing epochs.
    let accepted = server
        .game
        .relocate_player(
            remote,
            before + Vec3::X * 0.25,
            RelocationOptions::default(),
        )
        .unwrap();
    for _ in 0..3 {
        server.tick().unwrap();
    }
    let wanted = server.game.tick;
    let deadline = Instant::now() + Duration::from_secs(2);
    while client.server_tick() != wanted {
        client.poll().unwrap();
        assert!(Instant::now() < deadline, "relocation snapshot timeout");
        std::thread::sleep(Duration::from_millis(1));
    }
    for alpha in [0., 0.5, 1.] {
        assert_eq!(
            client
                .interpolated_players(alpha)
                .iter()
                .find(|p| p.id == remote)
                .unwrap()
                .position,
            accepted.position
        );
    }
    assert_eq!(
        client
            .rendered_players()
            .iter()
            .find(|p| p.id == remote)
            .unwrap()
            .position,
        accepted.position
    );
}

#[test]
fn complete_state_preflight_rejects_nonfinite_unrelated_players_and_late_arena_values() {
    for field in 0..8 {
        let mut game = game();
        let p = game.player_mut(2).unwrap();
        match field {
            0 => p.position.y = f32::NAN,
            1 => p.velocity.z = f32::INFINITY,
            2 => p.yaw = f32::NAN,
            3 => p.pitch = f32::NEG_INFINITY,
            4 => p.jump.coyote = f32::NAN,
            5 => p.jump.buffer = f32::INFINITY,
            6 => game.arena.spawns.push(Vec3::splat(f32::NAN)),
            _ => game.arena.solids.push(Obstacle {
                center: Vec3::splat(f32::INFINITY),
                half_extents: Vec3::ONE,
            }),
        }
        let before = format!("{:?}{:?}", game.players, game.arena);
        let epochs = game.life_epochs();
        let error = game
            .relocate_player(1, Vec3::new(10., 0., 0.), RelocationOptions::default())
            .unwrap_err();
        if field < 6 {
            assert!(matches!(error, RelocationError::InvalidState(_)));
        } else {
            assert!(matches!(error, RelocationError::Arena(_)));
        }
        assert_eq!(format!("{:?}{:?}", game.players, game.arena), before);
        assert_eq!(game.life_epochs(), epochs);
        assert_eq!(game.tick, 0);
        assert_eq!(
            game.rewind_hit_query(0, 0, 0., 0.),
            Err(RewindError::HistoryUnavailable)
        );
    }
}

#[test]
fn preserve_policy_retains_accepted_jump_continuity_reset_clears_it() {
    for motion in [RelocationMotion::Preserve, RelocationMotion::Reset] {
        let mut game = game();
        let player = game.player_mut(1).unwrap();
        player.grounded = false;
        player.jump = JumpState {
            coyote: 0.08,
            buffer: 0.07,
            held: true,
        };
        player.velocity = Vec3::new(2., -0.1, 3.);
        let at = Vec3::new(10., 3., -4.);
        let accepted = game.relocate_player(1, at, static_only(motion)).unwrap();
        let mut local = game.player(1).unwrap().clone();
        let predicted = step_player_detailed(&mut local, InputFrame::default(), &game.arena);
        let mut output = StepOutput::new();
        game.step_into(&[], &mut output).unwrap();
        assert_eq!(game.player(1).unwrap(), &local);
        let jumped = output
            .transitions()
            .iter()
            .any(|r| r.player == 1 && r.life == accepted.life && r.transitions.jumped);
        assert_eq!(predicted.transitions.jumped, jumped);
        assert_eq!(jumped, motion == RelocationMotion::Preserve);
    }
}

#[test]
fn external_relocation_is_not_an_old_capture_input_but_new_policy_segment_replays() {
    use ferravex_play::replay::{PlaybackError, ReplayChecksum};
    let policy = spawn::Policy::new(spawn::Config::default()).unwrap();
    let mut game = game();
    let mut old_capture = replay::Capture::new(&game, 19, Limits::default()).unwrap();
    game.relocate_player(1, Vec3::new(10., 0., -4.), RelocationOptions::default())
        .unwrap();
    let mut output = StepOutput::new();
    game.step_into(&[], &mut output).unwrap();
    old_capture.push(&game, &[], &[], output.events()).unwrap();
    let mut playback = Playback::new(Arc::new(old_capture.finish()));
    let mut world = replay::World::new(Match::from_checkpoint(&bytes(&game)).unwrap());
    playback.start(&mut world).unwrap();
    assert!(matches!(
        playback.step(&mut world),
        Err(PlaybackError::Desync { .. })
    ));
    assert_eq!(
        playback.first_mismatch().unwrap().checksum,
        ReplayChecksum::State
    );
    // Old Capture cannot detect an external operation; consumers must segment it.
    let mut capture = spawn::Capture::new(&game, policy, 19, Limits::default()).unwrap();
    for sequence in 0..20 {
        let commands = [(
            1,
            InputFrame {
                sequence,
                move_x: 0.5,
                ..Default::default()
            },
        )];
        game.step_with_spawn_policy_into(&commands, &[], Some(&policy), &mut output)
            .unwrap();
        capture
            .push(&game, &commands, &[], output.events())
            .unwrap();
    }
    let mut playback = Playback::new(Arc::new(capture.finish()));
    let mut world = spawn::World::new(game, policy).unwrap();
    let final_hash = world.game.state_hash().unwrap();
    for _ in 0..2 {
        playback.start(&mut world).unwrap();
        while !playback.finished() {
            playback.step(&mut world).unwrap();
        }
        assert_eq!(world.game.state_hash().unwrap(), final_hash);
        playback.reset();
    }
}
