use ferravex_fps::spawn::{Config, Policy};
use ferravex_fps::*;
use ferravex_math::Vec3;
use ferravex_play::replay::{Limits, Playback};
use std::cell::Cell;
use std::sync::Arc;

fn game(points: &[Vec3], count: usize) -> Match {
    let mut game = Match::new(Arena {
        solids: vec![],
        spawns: points.to_vec(),
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
fn fire() -> InputFrame {
    InputFrame {
        buttons: BUTTON_FIRE,
        ..Default::default()
    }
}
fn all_fire() -> [(u8, InputFrame); MAX_PLAYERS] {
    std::array::from_fn(|i| (i as u8, fire()))
}
fn compare(old: &Match, retained: &Match, legacy: &RewindStep, output: &StepOutput) {
    assert_eq!(legacy.events, output.events());
    assert_eq!(legacy.rejected_shots, output.rejected_shots());
    assert_eq!(checkpoint(old), checkpoint(retained));
    for (a, b) in legacy.events.iter().zip(output.events()) {
        assert_eq!(
            NetworkEvent {
                tick: old.tick,
                event: *a
            }
            .encode()
            .unwrap(),
            NetworkEvent {
                tick: retained.tick,
                event: *b
            }
            .encode()
            .unwrap()
        );
    }
}
fn overlap_policy() -> Policy {
    Policy::new(Config {
        minimum_horizontal_distance: 0.,
        forbid_occupied: false,
        ..Default::default()
    })
    .unwrap()
}
fn corpse(game: &mut Match, id: u8, ticks: u16) {
    let p = game.player_mut(id).unwrap();
    p.health = 0;
    p.respawn_ticks = ticks;
}

#[test]
fn exact_maximum_shots_hits_kills_respawns_and_rejections_are_complete() {
    for policy in [None, Some(overlap_policy())] {
        let mut old = game(&[Vec3::ZERO], MAX_PLAYERS);
        let mut retained = copy(&old);
        let mut output = StepOutput::with_capacity(MAX_STEP_EVENTS, MAX_STEP_REJECTIONS);
        let capacities = output.capacities();
        let legacy = old
            .step_with_spawn_policy(&all_fire(), &[], policy.as_ref())
            .unwrap();
        let counts = retained
            .step_with_spawn_policy_into(&all_fire(), &[], policy.as_ref(), &mut output)
            .unwrap();
        compare(&old, &retained, &legacy, &output);
        assert_eq!(
            counts,
            StepCounts {
                events: MAX_STEP_EVENTS,
                rejected_shots: 0
            }
        );
        for chunk in output.events().chunks_exact(3) {
            assert!(matches!(
                chunk,
                [
                    GameEvent::Shot { .. },
                    GameEvent::Hit { .. },
                    GameEvent::Killed { .. }
                ]
            ));
        }
        assert!(retained.players.iter().all(|p| p.health == 0));
        for id in 0..MAX_PLAYERS as u8 {
            corpse(&mut old, id, 1);
            corpse(&mut retained, id, 1);
        }
        let legacy = old
            .step_with_spawn_policy(&all_fire(), &[], policy.as_ref())
            .unwrap();
        let counts = retained
            .step_with_spawn_policy_into(&all_fire(), &[], policy.as_ref(), &mut output)
            .unwrap();
        compare(&old, &retained, &legacy, &output);
        assert_eq!(counts.events, MAX_PLAYERS);
        assert!(
            output
                .events()
                .iter()
                .enumerate()
                .all(|(id, event)| *event == GameEvent::Respawned { player: id as u8 })
        );
        let shots: [_; MAX_PLAYERS] = std::array::from_fn(|id| ShotTime {
            player: id as u8,
            tick: old.tick.wrapping_add(1),
        });
        let legacy = old
            .step_with_spawn_policy(&all_fire(), &shots, policy.as_ref())
            .unwrap();
        let counts = retained
            .step_with_spawn_policy_into(&all_fire(), &shots, policy.as_ref(), &mut output)
            .unwrap();
        compare(&old, &retained, &legacy, &output);
        assert_eq!(
            counts,
            StepCounts {
                events: 0,
                rejected_shots: MAX_STEP_REJECTIONS
            }
        );
        assert!(
            output
                .rejected_shots()
                .iter()
                .all(|(_, error)| *error == RewindError::FutureTick)
        );
        assert_eq!(output.capacities(), capacities);
        output.clear();
        assert!(output.events().is_empty() && output.rejected_shots().is_empty());
        assert_eq!(output.capacities(), capacities);
        assert_eq!(
            retained.step_into(&[], &mut output).unwrap(),
            StepCounts::default()
        );
    }
}

#[derive(Default)]
struct Rules {
    calls: Cell<usize>,
    frozen: bool,
    no_respawn: bool,
    bad_weapon: bool,
    no_damage: bool,
}
impl Rules {
    fn called(&self) {
        self.calls.set(self.calls.get() + 1);
    }
}
impl CombatRules for Rules {
    fn input(&mut self, _player: &Player, input: InputFrame) -> InputFrame {
        self.called();
        input
    }
    fn frozen(&self, _id: u8) -> bool {
        self.called();
        self.frozen
    }
    fn auto_respawn(&self) -> bool {
        self.called();
        !self.no_respawn
    }
    fn weapon(&self, _id: u8) -> WeaponSettings {
        self.called();
        WeaponSettings {
            magazine: if self.bad_weapon { 0 } else { MAGAZINE_SIZE },
            ..Default::default()
        }
    }
    fn damage(&mut self, _attacker: u8, _victim: &Player, headshot: bool) -> Option<u8> {
        self.called();
        (!self.no_damage).then_some(if headshot { 100 } else { 34 })
    }
}

#[test]
fn zero_and_each_short_capacity_reject_before_tick_capture_or_any_rules_hook() {
    let mut game = game(&[Vec3::ZERO], MAX_PLAYERS);
    let before = checkpoint(&game);
    for (events, rejections) in [
        (0, 0),
        (MAX_STEP_EVENTS - 1, MAX_STEP_REJECTIONS),
        (MAX_STEP_EVENTS, MAX_STEP_REJECTIONS - 1),
    ] {
        let mut output = StepOutput::with_capacity(events, rejections);
        let provided = output.capacities();
        let mut rules = Rules::default();
        assert_eq!(
            game.step_with_rules_into(&all_fire(), &[], &mut rules, &mut output),
            Err(StepError::OutputCapacity {
                required: StepCounts {
                    events: MAX_STEP_EVENTS,
                    rejected_shots: MAX_STEP_REJECTIONS
                },
                provided
            })
        );
        assert_eq!(rules.calls.get(), 0);
        assert_eq!(before, checkpoint(&game));
        assert_eq!(output.capacities(), provided);
        assert!(output.events().is_empty() && output.rejected_shots().is_empty());
        assert!(matches!(
            game.step_with_rules_and_spawn_policy_into(
                &all_fire(),
                &[],
                &mut rules,
                Some(&overlap_policy()),
                &mut output
            ),
            Err(StepError::OutputCapacity { .. })
        ));
        assert_eq!(rules.calls.get(), 0);
        assert_eq!(before, checkpoint(&game));
    }
    // Conservative capacity is documented even for no actors/no generated work.
    let mut empty = Match::default();
    let mut output = StepOutput::with_capacity(0, 0);
    assert!(matches!(
        empty.step_into(&[], &mut output),
        Err(StepError::OutputCapacity { .. })
    ));
    assert_eq!(empty.tick, 0);
}

#[test]
fn invalid_state_and_rewind_preserve_previous_results_capacities_and_rule_state() {
    let mut game = game(&[Vec3::ZERO], 2);
    let mut output = StepOutput::new();
    game.step_into(&all_fire(), &mut output).unwrap();
    let events = output.events().to_vec();
    let capacities = output.capacities();
    let valid = checkpoint(&game);
    let mut rules = Rules::default();
    // A public tick edit leaves a future rewind frame. Reject before recapture.
    game.tick = 0;
    let players = game.players.clone();
    assert!(matches!(
        game.step_with_rules_into(&[], &[], &mut rules, &mut output),
        Err(StepError::InvalidState(_))
    ));
    assert_eq!(game.tick, 0);
    assert_eq!(game.players, players);
    game.tick = 1;
    assert_eq!(valid, checkpoint(&game));
    // Raw duplicate/out-of-range/oversized/nonfinite state cannot defeat bounds.
    for mode in 0..4 {
        let mut invalid = Match::from_checkpoint(&valid).unwrap();
        match mode {
            0 => invalid.players.push(invalid.players[0].clone()),
            1 => invalid.players[0].id = u8::MAX,
            2 => invalid
                .players
                .resize(MAX_PLAYERS + 1, invalid.players[0].clone()),
            _ => invalid.players[0].position.x = f32::NAN,
        }
        assert!(matches!(
            invalid.step_with_rules_into(&[], &[], &mut rules, &mut output),
            Err(StepError::InvalidState(_))
        ));
        assert_eq!(invalid.tick, 1);
        assert_eq!(output.events(), events);
        assert_eq!(output.capacities(), capacities);
    }
    assert_eq!(rules.calls.get(), 0);
}

#[test]
fn historical_current_future_old_and_unavailable_shots_match_through_wrap() {
    for start in [0, u32::MAX - 8] {
        let mut initial = game(&[Vec3::ZERO, Vec3::new(0., 0., -5.)], 2);
        initial.tick = start;
        // An uncaptured past time inside the window is unavailable.
        let mut output = StepOutput::new();
        let unavailable = [ShotTime {
            player: 0,
            tick: start.wrapping_sub(1),
        }];
        initial
            .step_with_rewind_into(&[(0, fire())], &unavailable, &mut output)
            .unwrap();
        assert_eq!(
            output.rejected_shots(),
            [(0, RewindError::HistoryUnavailable)]
        );
        let old_tick = initial.tick;
        for _ in 0..10 {
            initial
                .step_into(
                    &[(
                        1,
                        InputFrame {
                            move_x: 1.,
                            ..Default::default()
                        },
                    )],
                    &mut output,
                )
                .unwrap();
        }
        for (time, expected) in [
            (old_tick, None),
            (initial.tick, None),
            (initial.tick.wrapping_add(1), Some(RewindError::FutureTick)),
            (
                initial.tick.wrapping_sub(MAX_REWIND_TICKS + 1),
                Some(RewindError::TooOld),
            ),
        ] {
            let mut old = copy(&initial);
            let mut retained = copy(&initial);
            let inputs = [(
                0,
                InputFrame {
                    move_x: 1.,
                    ..fire()
                },
            )];
            let shots = [ShotTime {
                player: 0,
                tick: time,
            }];
            let legacy = old.step_with_rewind(&inputs, &shots);
            let counts = retained
                .step_with_rewind_into(&inputs, &shots, &mut output)
                .unwrap();
            compare(&old, &retained, &legacy, &output);
            assert_eq!(counts.rejected_shots, usize::from(expected.is_some()));
            if let Some(error) = expected {
                assert_eq!(output.rejected_shots(), [(0, error)]);
            }
            if time == old_tick {
                assert!(
                    output
                        .events()
                        .iter()
                        .any(|e| matches!(e, GameEvent::Killed { victim: 1, .. }))
                );
            }
            if time == initial.tick {
                assert!(
                    output
                        .events()
                        .iter()
                        .all(|e| !matches!(e, GameEvent::Hit { .. }))
                );
            }
            assert!(retained.player(0).unwrap().position.x > 0.);
        }
    }
}

#[test]
fn legacy_order_sanitization_first_wins_and_raw_state_defaults_remain_compatible() {
    let mut old = game(&[Vec3::ZERO], 4);
    old.players.reverse();
    let mut retained = copy(&old);
    let mut output = StepOutput::new();
    let mut inputs = vec![(3, fire()); MAX_PLAYERS + 2];
    inputs[1] = (
        3,
        InputFrame {
            move_x: 1.,
            ..Default::default()
        },
    );
    inputs.push((
        2,
        InputFrame {
            move_x: f32::NAN,
            buttons: BUTTON_FIRE,
            ..Default::default()
        },
    ));
    let shots = [
        ShotTime {
            player: 3,
            tick: old.tick.wrapping_add(1),
        },
        ShotTime {
            player: 3,
            tick: old.tick,
        },
    ];
    let legacy = old.step_with_rewind(&inputs, &shots);
    retained
        .step_with_rewind_into(&inputs, &shots, &mut output)
        .unwrap();
    compare(&old, &retained, &legacy, &output);
    assert_eq!(output.rejected_shots(), [(3, RewindError::FutureTick)]);
    // Convenience wrappers still tolerate raw oversized passive rosters.
    let mut raw = game(&[], 0);
    raw.players = (0..MAX_PLAYERS + 1)
        .map(|i| Player::new(i as u8, Vec3::ZERO))
        .collect();
    assert!(raw.step(&[]).is_empty());
    assert_eq!(raw.tick, 1);
}

#[test]
fn safe_spawn_reservations_deferral_lives_and_old_life_rewind_match_allocating_path() {
    let policy = Policy::new(Config {
        minimum_horizontal_distance: 0.,
        ..Default::default()
    })
    .unwrap();
    let mut old = game(&[Vec3::ZERO, Vec3::X * 10.], 3);
    old.players.reverse();
    let mut output = StepOutput::new();
    old.step_into(&[], &mut output).unwrap();
    let history_tick = old.tick;
    for id in 0..3 {
        corpse(&mut old, id, 1);
    }
    let mut retained = copy(&old);
    let initial_lives = retained.life_epochs();
    let legacy = old.step_with_spawn_policy(&[], &[], Some(&policy)).unwrap();
    retained
        .step_with_spawn_policy_into(&[], &[], Some(&policy), &mut output)
        .unwrap();
    compare(&old, &retained, &legacy, &output);
    assert_eq!(
        output.events(),
        [
            GameEvent::Respawned { player: 0 },
            GameEvent::Respawned { player: 1 }
        ]
    );
    assert_eq!(retained.life_epochs()[0], initial_lives[0] + 1);
    assert_eq!(retained.life_epochs()[1], initial_lives[1] + 1);
    assert_eq!(retained.life_epochs()[2], initial_lives[2]);
    assert_eq!(retained.player(2).unwrap().respawn_ticks, 0);
    assert_eq!(retained.player(0).unwrap().position, Vec3::ZERO);
    assert_eq!(retained.player(1).unwrap().position, Vec3::X * 10.);
    assert_eq!(
        retained
            .rewind_hit_query(0, history_tick, 0., 0.)
            .unwrap()
            .victim,
        None
    );
    let legacy = old.step_with_spawn_policy(&[], &[], Some(&policy)).unwrap();
    retained
        .step_with_spawn_policy_into(&[], &[], Some(&policy), &mut output)
        .unwrap();
    compare(&old, &retained, &legacy, &output);
    assert!(output.events().is_empty());
    assert_eq!(retained.life_epochs()[2], initial_lives[2]);
    // Remove a live reservation: retry succeeds without a fresh death countdown.
    old.remove_player(0);
    retained.remove_player(0);
    let legacy = old.step_with_spawn_policy(&[], &[], Some(&policy)).unwrap();
    retained
        .step_with_spawn_policy_into(&[], &[], Some(&policy), &mut output)
        .unwrap();
    compare(&old, &retained, &legacy, &output);
    assert_eq!(output.events(), [GameEvent::Respawned { player: 2 }]);
    assert_eq!(retained.life_epochs()[2], initial_lives[2] + 1);
}

#[test]
fn policy_failures_preserve_nonempty_previous_events_and_rejections() {
    let mut game = game(&[Vec3::ZERO], 3);
    let mut output = StepOutput::new();
    game.step_with_rewind_into(&all_fire(), &[ShotTime { player: 2, tick: 1 }], &mut output)
        .unwrap();
    assert!(!output.events().is_empty() && !output.rejected_shots().is_empty());
    let events = output.events().to_vec();
    let rejections = output.rejected_shots().to_vec();
    let capacity = output.capacities();
    let before = checkpoint(&game);
    let bad_policy = Policy::new(Config {
        clearance_candidates: 0,
        ..Default::default()
    })
    .unwrap();
    let mut rules = Rules::default();
    assert!(matches!(
        game.step_with_rules_and_spawn_policy_into(
            &[],
            &[],
            &mut rules,
            Some(&bad_policy),
            &mut output
        ),
        Err(StepError::Spawn(
            ferravex_fps::spawn::Error::Clearance { .. }
        ))
    ));
    assert!(matches!(
        game.step_with_rules_and_spawn_policy_into(
            &[(0, fire()), (0, fire())],
            &[],
            &mut rules,
            Some(&overlap_policy()),
            &mut output
        ),
        Err(StepError::Spawn(
            ferravex_fps::spawn::Error::InvalidCommands
        ))
    ));
    assert_eq!(rules.calls.get(), 0);
    assert_eq!(checkpoint(&game), before);
    assert_eq!(output.events(), events);
    assert_eq!(output.rejected_shots(), rejections);
    assert_eq!(output.capacities(), capacity);
}

#[test]
fn combat_gates_callbacks_and_policy_none_match_both_output_paths() {
    for policy in [None, Some(overlap_policy())] {
        for gate in 0..5 {
            let mut old = game(&[Vec3::ZERO], 3);
            corpse(&mut old, 2, 1);
            let mut retained = copy(&old);
            let lives = retained.life_epochs();
            let make_rules = || Rules {
                frozen: gate == 0,
                bad_weapon: gate == 1,
                no_respawn: gate == 2,
                no_damage: gate == 3,
                ..Default::default()
            };
            let mut old_rules = make_rules();
            let mut new_rules = make_rules();
            let mut output = StepOutput::new();
            let legacy = old
                .step_with_rules_and_spawn_policy(&all_fire(), &[], &mut old_rules, policy.as_ref())
                .unwrap();
            retained
                .step_with_rules_and_spawn_policy_into(
                    &all_fire(),
                    &[],
                    &mut new_rules,
                    policy.as_ref(),
                    &mut output,
                )
                .unwrap();
            compare(&old, &retained, &legacy, &output);
            assert_eq!(old_rules.calls.get(), new_rules.calls.get());
            if gate == 0 || gate == 1 {
                assert!(output.events().is_empty());
            }
            if gate < 3 {
                assert_eq!(retained.player(2).unwrap().health, 0);
                assert_eq!(retained.player(2).unwrap().respawn_ticks, 1);
                assert_eq!(retained.life_epochs()[2], lives[2]);
            }
        }
    }
}

fn fingerprint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
#[test]
fn retained_consumer_matches_frozen_pre237_checkpoint_replay_bytes_and_restarts() {
    let mut old = game(&[Vec3::ZERO, Vec3::new(0., 0., -8.)], 2);
    let mut retained = copy(&old);
    let limits = Limits {
        checkpoint_interval: 17,
        ..Default::default()
    };
    let mut a = replay::Capture::new(&old, 19, limits).unwrap();
    let mut b = replay::Capture::new(&retained, 19, limits).unwrap();
    let mut output = StepOutput::new();
    for sequence in 0..220 {
        let commands = [(0, InputFrame { sequence, ..fire() })];
        let legacy = old.step_with_rewind(&commands, &[]);
        retained
            .step_with_rewind_into(&commands, &[], &mut output)
            .unwrap();
        compare(&old, &retained, &legacy, &output);
        a.push(&old, &commands, &[], &legacy.events).unwrap();
        b.push(&retained, &commands, &[], output.events()).unwrap();
    }
    let recording = b.finish();
    let encoded = recording.encode();
    assert_eq!(a.finish().encode(), encoded);
    let cp = checkpoint(&retained);
    // Immutable pre237 task234 fixture, not a hash invented from this change.
    assert_eq!((cp.len(), fingerprint(&cp)), (1095, 0x247c126c3a7171a3));
    assert_eq!(
        (encoded.len(), fingerprint(&encoded)),
        (27473, 0xd586f86b6dda4c39)
    );
    let mut playback = Playback::new(Arc::new(recording));
    let mut world = replay::World::new(Match::new(retained.arena.clone()));
    for _ in 0..2 {
        playback.reset();
        playback.start(&mut world).unwrap();
        while !playback.finished() {
            playback.step(&mut world).unwrap();
        }
        assert_eq!(checkpoint(&retained), checkpoint(&world.game));
    }
}

#[test]
fn policy_retained_recording_event_order_and_context_match_through_tick_wrap() {
    let policy = overlap_policy();
    for start in [0, u32::MAX - 5] {
        let mut old = game(&[Vec3::ZERO, Vec3::new(0., 0., -8.)], 2);
        old.tick = start;
        old.players.reverse();
        let mut retained = copy(&old);
        let limits = Limits {
            checkpoint_interval: 17,
            ..Default::default()
        };
        let mut a = spawn::Capture::new(&old, policy, 19, limits).unwrap();
        let mut b = spawn::Capture::new(&retained, policy, 19, limits).unwrap();
        let mut output = StepOutput::new();
        let mut kills = 0;
        let mut respawns = 0;
        for sequence in 0..420 {
            let commands = [(0, InputFrame { sequence, ..fire() })];
            let shots = [ShotTime {
                player: 0,
                tick: old.tick.wrapping_sub(sequence % 25),
            }];
            let legacy = old
                .step_with_spawn_policy(&commands, &shots, Some(&policy))
                .unwrap();
            retained
                .step_with_spawn_policy_into(&commands, &shots, Some(&policy), &mut output)
                .unwrap();
            compare(&old, &retained, &legacy, &output);
            kills += output
                .events()
                .iter()
                .filter(|e| matches!(e, GameEvent::Killed { .. }))
                .count();
            respawns += output
                .events()
                .iter()
                .filter(|e| matches!(e, GameEvent::Respawned { .. }))
                .count();
            a.push(&old, &commands, &shots, &legacy.events).unwrap();
            b.push(&retained, &commands, &shots, output.events())
                .unwrap();
        }
        assert!(kills > 0 && respawns > 0);
        let recording = b.finish();
        assert_eq!(a.finish().encode(), recording.encode());
        let mut playback = Playback::new(Arc::new(recording));
        let mut world = spawn::World::new(Match::new(retained.arena.clone()), policy).unwrap();
        for _ in 0..2 {
            playback.reset();
            playback.start(&mut world).unwrap();
            while !playback.finished() {
                playback.step(&mut world).unwrap();
            }
            assert_eq!(checkpoint(&retained), checkpoint(&world.game));
        }
    }
}

use std::alloc::{GlobalAlloc, Layout, System};
struct CountingAllocator;
thread_local! { static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) }; }
fn count_allocation() {
    let _ = ALLOCATIONS.try_with(|v| {
        if let Some(n) = v.get() {
            v.set(Some(n + 1));
        }
    });
}
// Forward System's contract, counting alloc/alloc_zeroed/realloc on this thread.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_allocation();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn actual_warmed_max_roster_ticks_capture_and_complete_outputs_do_not_allocate() {
    for policy in [None, Some(overlap_policy())] {
        let mut game = game(&[Vec3::ZERO; MAX_PLAYERS], MAX_PLAYERS);
        game.arena.solids = vec![
            Obstacle {
                center: Vec3::new(100., 1., 100.),
                half_extents: Vec3::ONE
            };
            256
        ];
        let mut output = StepOutput::new();
        let capacities = output.capacities();
        let commands = all_fire();
        for _ in 0..32 {
            game.step_with_spawn_policy_into(&commands, &[], policy.as_ref(), &mut output)
                .unwrap();
        }
        let mut totals = [0; 5];
        ALLOCATIONS.with(|v| v.set(Some(0)));
        for i in 0..1024 {
            let times: [_; MAX_PLAYERS] = std::array::from_fn(|id| ShotTime {
                player: id as u8,
                tick: if i % 37 == 0 {
                    game.tick.wrapping_add(1)
                } else {
                    game.tick
                },
            });
            game.step_with_spawn_policy_into(&commands, &times, policy.as_ref(), &mut output)
                .unwrap();
            for event in output.events() {
                totals[match event {
                    GameEvent::Shot { .. } => 0,
                    GameEvent::Hit { .. } => 1,
                    GameEvent::Killed { .. } => 2,
                    GameEvent::Respawned { .. } => 3,
                }] += 1;
            }
            totals[4] += output.rejected_shots().len();
        }
        let allocations = ALLOCATIONS.with(|v| v.replace(None).unwrap());
        assert_eq!(allocations, 0);
        assert!(totals.iter().all(|n| *n > 0));
        assert_eq!(capacities, output.capacities());
        assert_eq!(game.tick, 1056);
        println!(
            "policy={} actual warmed ticks=1024 roster=16 solids=256 allocations={allocations} shot/hit/kill/respawn/reject={totals:?}",
            policy.is_some()
        );
    }
    // Distinguish old convenience output allocation from the retained kernel.
    let mut legacy = game(&[Vec3::ZERO], MAX_PLAYERS);
    for _ in 0..32 {
        legacy.step(&[]);
    }
    ALLOCATIONS.with(|v| v.set(Some(0)));
    let events = legacy.step(&all_fire());
    let allocations = ALLOCATIONS.with(|v| v.replace(None).unwrap());
    assert_eq!(events.len(), MAX_STEP_EVENTS);
    assert!(allocations > 0);
    println!(
        "legacy convenience max-event tick allocations={allocations} (setup/formatting excluded)"
    );
}
