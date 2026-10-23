use ferravex_fps::arena_authoring::*;
use ferravex_fps::{Arena, FIXED_DT, InputFrame, Match, Obstacle, PLAYER_HEIGHT, PLAYER_RADIUS};
use ferravex_math::{Vec2, Vec3};
use ferravex_physics::character::{CharacterShape, ClearanceError};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct CountingAllocator;
thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
fn record_allocation() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value + 1));
        }
    });
}
// Forward the System allocator contract, counting only the current test thread.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record_allocation();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn options() -> AuthoringOptions {
    AuthoringOptions {
        clearance: Some(SpawnClearance::default()),
        ..Default::default()
    }
}
fn cube(center: Vec3) -> Obstacle {
    Obstacle {
        center,
        half_extents: Vec3::ONE,
    }
}
fn arena(solids: Vec<Obstacle>, spawns: Vec<Vec3>) -> Arena {
    Arena { solids, spawns }
}
fn clearance_error(
    spawn: SpawnSite,
    solid: Option<usize>,
    error: ClearanceError,
) -> ArenaAuthoringError {
    ArenaAuthoringError::SpawnClearance {
        spawn,
        solid,
        error,
    }
}

#[test]
fn default_arena_is_unchanged_checkpoint_compatible_and_steps_real_match() {
    let raw = Arena::default();
    let expected = raw.spawns.clone();
    let expected_context = Match::new(raw.clone()).context_hash().unwrap();
    let accepted = try_from_arena(raw, options()).unwrap();
    assert_eq!(accepted.spawns, expected);
    let mut game = Match::new(accepted);
    assert_eq!(game.context_hash().unwrap(), expected_context);
    for id in 0..MAX_SPAWNS as u8 {
        assert!(game.spawn_player(id));
    }
    for (id, spawn) in expected.iter().enumerate() {
        assert_eq!(game.player(id as u8).unwrap().position, *spawn);
    }
    let mut bytes = Vec::new();
    game.checkpoint_into(&mut bytes).unwrap();
    let mut restored = Match::from_checkpoint(&bytes).unwrap();
    for _ in 0..64 {
        let inputs = [(
            0,
            InputFrame {
                move_x: 1.,
                ..Default::default()
            },
        )];
        game.step(&inputs);
        restored.step(&inputs);
    }
    assert_eq!(game.tick, 64);
    assert_eq!(FIXED_DT, 1. / 64.);
    assert_eq!(game.players, restored.players);
    assert_ne!(game.player(0).unwrap().position, expected[0]);
}

#[test]
fn all_late_geometry_is_checked_without_losing_owned_vectors() {
    let raw = arena(
        vec![cube(Vec3::X * 20.), cube(Vec3::new(0., f32::NAN, 0.))],
        vec![Vec3::ZERO],
    );
    let ptr = raw.solids.as_ptr();
    let rejected = try_from_arena(raw, options()).unwrap_err();
    assert_eq!(
        rejected.error,
        ArenaAuthoringError::Solid {
            index: 1,
            problem: SolidError::NonFiniteCenter
        }
    );
    assert_eq!(rejected.input.solids.as_ptr(), ptr);
    assert!(rejected.input.solids[1].center.y.is_nan());
    let raw = arena(vec![], vec![Vec3::ZERO, Vec3::splat(f32::INFINITY)]);
    assert_eq!(
        validate_arena(&raw, options()),
        Err(ArenaAuthoringError::Spawn {
            index: 1,
            problem: SpawnError::NonFinite
        })
    );
}

#[test]
fn solid_diagnostics_cover_nonfinite_zero_negative_overflow_and_limits() {
    let cases = [
        (
            Obstacle {
                center: Vec3::splat(f32::INFINITY),
                half_extents: Vec3::ONE,
            },
            SolidError::NonFiniteCenter,
        ),
        (
            Obstacle {
                center: Vec3::ZERO,
                half_extents: Vec3::splat(f32::NAN),
            },
            SolidError::NonFiniteHalfExtents,
        ),
        (
            Obstacle {
                center: Vec3::ZERO,
                half_extents: Vec3::new(1., 0., 1.),
            },
            SolidError::NonPositiveHalfExtents,
        ),
        (
            Obstacle {
                center: Vec3::ZERO,
                half_extents: Vec3::new(1., -1., 1.),
            },
            SolidError::NonPositiveHalfExtents,
        ),
        (
            Obstacle {
                center: Vec3::splat(f32::MAX),
                half_extents: Vec3::splat(f32::MAX),
            },
            SolidError::NonFiniteBounds,
        ),
        (
            Obstacle {
                center: Vec3::splat(-f32::MAX),
                half_extents: Vec3::splat(f32::MAX),
            },
            SolidError::NonFiniteBounds,
        ),
        (cube(Vec3::X * 8193.), SolidError::CenterLimit),
        (
            Obstacle {
                center: Vec3::ZERO,
                half_extents: Vec3::splat(8193.),
            },
            SolidError::HalfExtentsLimit,
        ),
    ];
    for (bad, problem) in cases {
        let raw = arena(vec![cube(Vec3::X * 10.), bad], vec![Vec3::ZERO]);
        assert_eq!(
            validate_arena(&raw, Default::default()),
            Err(ArenaAuthoringError::Solid { index: 1, problem })
        );
    }
    for position in [Vec3::X * 4097., Vec3::splat(f32::MAX)] {
        assert_eq!(
            validate_arena(
                &arena(vec![], vec![Vec3::ZERO, position]),
                Default::default()
            ),
            Err(ArenaAuthoringError::Spawn {
                index: 1,
                problem: SpawnError::PositionLimit
            })
        );
    }
}

#[test]
fn checkpoint_domain_without_clearance_does_not_impose_character_shape_limits() {
    // Tiny positive extent remains checkpoint-compatible; never clamp to motor skin/radius.
    let raw = arena(
        vec![Obstacle {
            center: Vec3::splat(8192.),
            half_extents: Vec3::new(f32::MIN_POSITIVE, 8192., 8192.),
        }],
        vec![Vec3::splat(-4096.)],
    );
    validate_arena(&raw, Default::default()).unwrap();
    let mut bytes = Vec::new();
    Match::new(raw).checkpoint_into(&mut bytes).unwrap();
    // A below-ground spawn is valid raw data but cannot be cleared for a character.
    let raw = arena(vec![], vec![Vec3::new(0., -1., 0.)]);
    validate_arena(&raw, Default::default()).unwrap();
    assert_eq!(
        validate_arena(&raw, options()),
        Err(clearance_error(
            SpawnSite::Authored(0),
            None,
            ClearanceError::Blocked { obstacle: 0 }
        ))
    );
}

#[test]
fn caps_include_last_entry_reject_excess_and_preserve_stable_indices() {
    let mut b = ArenaBuilder::new(Default::default()).unwrap();
    for index in 0..MAX_SOLIDS {
        assert_eq!(b.try_push_solid(cube(Vec3::X * 20.)).unwrap(), index);
    }
    let rejected = b.try_push_solid(cube(Vec3::Y)).unwrap_err();
    assert_eq!(
        rejected.error,
        ArenaAuthoringError::SolidCapacity {
            index: MAX_SOLIDS,
            limit: MAX_SOLIDS
        }
    );
    assert_eq!(rejected.input.center, Vec3::Y);
    for index in 0..MAX_SPAWNS {
        assert_eq!(b.try_push_spawn(Vec3::ZERO).unwrap(), index);
    }
    let rejected = b.try_push_spawn(Vec3::X).unwrap_err();
    assert_eq!(
        rejected.error,
        ArenaAuthoringError::SpawnCapacity {
            index: MAX_SPAWNS,
            limit: MAX_SPAWNS
        }
    );
    assert_eq!(rejected.input, Vec3::X);
    let mut accepted = b.finish().unwrap();
    let mut bytes = Vec::new();
    Match::new(accepted.clone())
        .checkpoint_into(&mut bytes)
        .unwrap();
    accepted.solids[MAX_SOLIDS - 1].half_extents.x = -1.;
    assert_eq!(
        validate_arena(&accepted, Default::default()),
        Err(ArenaAuthoringError::Solid {
            index: MAX_SOLIDS - 1,
            problem: SolidError::NonPositiveHalfExtents
        })
    );
    accepted.solids[MAX_SOLIDS - 1].half_extents.x = 1.;
    accepted.solids.push(cube(Vec3::ZERO));
    assert_eq!(
        validate_arena(&accepted, Default::default()),
        Err(ArenaAuthoringError::SolidCapacity {
            index: MAX_SOLIDS,
            limit: MAX_SOLIDS
        })
    );
    accepted.solids.pop();
    accepted.spawns[MAX_SPAWNS - 1].x = f32::NAN;
    assert_eq!(
        validate_arena(&accepted, Default::default()),
        Err(ArenaAuthoringError::Spawn {
            index: MAX_SPAWNS - 1,
            problem: SpawnError::NonFinite
        })
    );
    accepted.spawns.push(Vec3::ZERO);
    assert_eq!(
        validate_arena(&accepted, Default::default()),
        Err(ArenaAuthoringError::SpawnCapacity {
            index: MAX_SPAWNS,
            limit: MAX_SPAWNS
        })
    );
}

#[test]
fn rejected_push_finish_reset_and_reuse_preserve_content_options_and_storage() {
    let settings = AuthoringOptions {
        limits: ArenaLimits {
            solids: 2,
            spawns: 2,
        },
        ..options()
    };
    let mut b = ArenaBuilder::new(settings).unwrap();
    let pointers = (b.arena().solids.as_ptr(), b.arena().spawns.as_ptr());
    b.try_push_solid(cube(Vec3::X * 20.)).unwrap();
    let bad = b
        .try_push_solid(Obstacle {
            center: Vec3::X,
            half_extents: -Vec3::ONE,
        })
        .unwrap_err();
    assert_eq!(
        bad.error,
        ArenaAuthoringError::Solid {
            index: 1,
            problem: SolidError::NonPositiveHalfExtents
        }
    );
    assert_eq!(bad.input.half_extents, -Vec3::ONE);
    assert_eq!(b.try_push_solid(cube(Vec3::X * 30.)).unwrap(), 1);
    assert!(b.try_push_spawn(Vec3::splat(f32::NAN)).is_err());
    assert!(b.arena().spawns.is_empty());
    let failed = b.finish().unwrap_err();
    assert_eq!(failed.error, ArenaAuthoringError::EmptySpawns);
    let mut b = failed.input;
    assert_eq!(b.arena().solids.len(), 2);
    b.reset();
    assert_eq!(b.options(), settings);
    assert_eq!(
        (b.arena().solids.as_ptr(), b.arena().spawns.as_ptr()),
        pointers
    );
    assert_eq!(b.try_push_spawn(Vec3::ZERO).unwrap(), 0);
    b.validate().unwrap();
    let accepted = b.finish().unwrap();
    assert_eq!(
        (accepted.solids.as_ptr(), accepted.spawns.as_ptr()),
        pointers
    );
}

#[test]
fn explicit_empty_policy_checks_legacy_origin_and_keeps_empty_vector() {
    let raw = arena(vec![], vec![]);
    assert_eq!(
        validate_arena(&raw, Default::default()),
        Err(ArenaAuthoringError::EmptySpawns)
    );
    let legacy = AuthoringOptions {
        empty_spawns: EmptySpawnPolicy::LegacyOrigin,
        ..options()
    };
    validate_arena(&raw, legacy).unwrap();
    assert!(raw.spawns.is_empty());
    assert_eq!(raw.spawn(15), Vec3::ZERO);
    let blocked = arena(vec![cube(Vec3::Y)], vec![]);
    assert_eq!(
        validate_arena(&blocked, legacy),
        Err(clearance_error(
            SpawnSite::LegacyOrigin,
            Some(0),
            ClearanceError::Blocked { obstacle: 1 }
        ))
    );
    validate_arena(
        &blocked,
        AuthoringOptions {
            clearance: None,
            ..legacy
        },
    )
    .unwrap();
    let zero = AuthoringOptions {
        limits: ArenaLimits {
            solids: 0,
            spawns: 0,
        },
        ..legacy
    };
    let mut b = ArenaBuilder::new(zero).unwrap();
    assert!(matches!(
        b.try_push_solid(cube(Vec3::ZERO)).unwrap_err().error,
        ArenaAuthoringError::SolidCapacity { index: 0, limit: 0 }
    ));
    assert!(matches!(
        b.try_push_spawn(Vec3::ZERO).unwrap_err().error,
        ArenaAuthoringError::SpawnCapacity { index: 0, limit: 0 }
    ));
    b.finish().unwrap();
}

#[test]
fn late_blocked_spawns_and_solids_are_checked_with_stable_source_indices() {
    let mut b = ArenaBuilder::new(options()).unwrap();
    b.try_push_spawn(Vec3::ZERO).unwrap();
    b.try_push_spawn(Vec3::X * 10.).unwrap();
    b.try_push_solid(cube(Vec3::new(20., 1., 0.))).unwrap();
    b.try_push_solid(cube(Vec3::new(10., 1., 0.))).unwrap();
    assert_eq!(
        b.validate(),
        Err(clearance_error(
            SpawnSite::Authored(1),
            Some(1),
            ClearanceError::Blocked { obstacle: 2 }
        ))
    );
    // Overlapping authored spawns are allowed: this validates static terrain,
    // not live-player safety or task 234's selection policy.
    validate_arena(&arena(vec![], vec![Vec3::ZERO; MAX_SPAWNS]), options()).unwrap();
    let raw = arena(vec![cube(Vec3::Y), cube(Vec3::Y)], vec![Vec3::ZERO]);
    assert_eq!(
        validate_arena(&raw, options()),
        Err(clearance_error(
            SpawnSite::Authored(0),
            Some(0),
            ClearanceError::Blocked { obstacle: 1 }
        ))
    );
}

#[test]
fn capsule_and_box2_use_actual_touching_tolerance_and_ground_adapter() {
    assert_eq!(
        SpawnClearance::default().shape,
        CharacterShape::Capsule {
            radius: PLAYER_RADIUS,
            height: PLAYER_HEIGHT
        }
    );
    for shape in [
        SpawnClearance::default().shape,
        CharacterShape::Box2 {
            half: Vec2::new(0.3, 0.9),
        },
    ] {
        let settings = AuthoringOptions {
            clearance: Some(SpawnClearance {
                shape,
                max_candidates: 257,
            }),
            ..options()
        };
        // Top of ground-level box is exactly the feet; ceiling bottom exactly head height.
        let raw = arena(
            vec![
                Obstacle {
                    center: Vec3::new(0., -1., 0.),
                    half_extents: Vec3::ONE,
                },
                Obstacle {
                    center: Vec3::new(0., 2.8, 0.),
                    half_extents: Vec3::ONE,
                },
            ],
            vec![Vec3::ZERO],
        );
        validate_arena(&raw, settings).unwrap();
        validate_arena(&arena(vec![], vec![Vec3::Y * -0.000001]), settings).unwrap();
        assert_eq!(
            validate_arena(&arena(vec![], vec![Vec3::Y * -0.0001]), settings),
            Err(clearance_error(
                SpawnSite::Authored(0),
                None,
                ClearanceError::Blocked { obstacle: 0 }
            ))
        );
        let raw = arena(
            vec![Obstacle {
                center: Vec3::new(0., 2.7, 0.),
                half_extents: Vec3::ONE,
            }],
            vec![Vec3::ZERO],
        );
        assert_eq!(
            validate_arena(&raw, settings),
            Err(clearance_error(
                SpawnSite::Authored(0),
                Some(0),
                ClearanceError::Blocked { obstacle: 1 }
            ))
        );
    }
}

#[test]
fn planar_constraints_are_optional_and_report_actual_error_and_solid_index() {
    let settings = AuthoringOptions {
        clearance: Some(SpawnClearance {
            shape: CharacterShape::Box2 {
                half: Vec2::new(0.3, 0.9),
            },
            max_candidates: 257,
        }),
        ..options()
    };
    let raw = arena(
        vec![cube(Vec3::X * 20.), cube(Vec3::new(0., 1., 0.5))],
        vec![Vec3::ZERO],
    );
    validate_arena(&raw, Default::default()).unwrap();
    assert_eq!(
        validate_arena(&raw, settings),
        Err(clearance_error(
            SpawnSite::Authored(0),
            Some(1),
            ClearanceError::InvalidObstacle
        ))
    );
    let raw = arena(vec![], vec![Vec3::ZERO, Vec3::Z]);
    validate_arena(&raw, Default::default()).unwrap();
    assert_eq!(
        validate_arena(&raw, settings),
        Err(clearance_error(
            SpawnSite::Authored(1),
            None,
            ClearanceError::InvalidInput
        ))
    );
}

#[test]
fn capsule_corner_clearance_uses_the_existing_narrowphase() {
    // The capsule's AABB overlaps this corner, but its circular section clears.
    let raw = arena(
        vec![Obstacle {
            center: Vec3::new(0.35, 0.9, 0.35),
            half_extents: Vec3::new(0.1, 0.5, 0.1),
        }],
        vec![Vec3::ZERO],
    );
    validate_arena(&raw, options()).unwrap();
    let short = AuthoringOptions {
        clearance: Some(SpawnClearance {
            shape: CharacterShape::Capsule {
                radius: 0.3,
                height: 0.6,
            },
            ..Default::default()
        }),
        ..options()
    };
    let ceiling = arena(
        vec![Obstacle {
            center: Vec3::Y * 1.4,
            half_extents: Vec3::new(2., 0.1, 2.),
        }],
        vec![Vec3::ZERO],
    );
    assert!(validate_arena(&ceiling, options()).is_err());
    validate_arena(&ceiling, short).unwrap();
}

#[test]
fn candidate_budget_zero_exact_exhausted_and_maximum_never_imply_clearance() {
    let with_budget = |max_candidates| AuthoringOptions {
        clearance: Some(SpawnClearance {
            max_candidates,
            ..Default::default()
        }),
        ..options()
    };
    let empty = arena(vec![], vec![Vec3::Y * 100.]);
    assert_eq!(
        validate_arena(&empty, with_budget(0)),
        Err(clearance_error(
            SpawnSite::Authored(0),
            None,
            ClearanceError::QueryBudget
        ))
    );
    validate_arena(&empty, with_budget(1)).unwrap();
    validate_arena(&empty, with_budget(1_000_000)).unwrap();
    let raw = arena(vec![cube(Vec3::Y), cube(Vec3::Y)], vec![Vec3::ZERO]);
    assert_eq!(
        validate_arena(&raw, with_budget(2)),
        Err(clearance_error(
            SpawnSite::Authored(0),
            None,
            ClearanceError::QueryBudget
        ))
    );
    assert_eq!(
        validate_arena(&raw, with_budget(3)),
        Err(clearance_error(
            SpawnSite::Authored(0),
            Some(0),
            ClearanceError::Blocked { obstacle: 1 }
        ))
    );
    // Each spawn gets its own budget; it is not silently shared/exhausted by earlier queries.
    validate_arena(&arena(vec![], vec![Vec3::ZERO; MAX_SPAWNS]), with_budget(1)).unwrap();
    assert_eq!(
        validate_arena(&empty, with_budget(1_000_001)),
        Err(ArenaAuthoringError::InvalidClearance)
    );
}

#[test]
fn invalid_options_reject_before_reservation_even_for_empty_input() {
    for limits in [
        ArenaLimits {
            solids: MAX_SOLIDS + 1,
            spawns: 0,
        },
        ArenaLimits {
            solids: 0,
            spawns: MAX_SPAWNS + 1,
        },
        ArenaLimits {
            solids: usize::MAX,
            spawns: usize::MAX,
        },
    ] {
        let settings = AuthoringOptions {
            limits,
            ..Default::default()
        };
        assert_eq!(
            ArenaBuilder::new(settings).unwrap_err(),
            ArenaAuthoringError::InvalidLimits { limits }
        );
    }
    for shape in [
        CharacterShape::Capsule {
            radius: f32::NAN,
            height: 1.8,
        },
        CharacterShape::Capsule {
            radius: 0.3,
            height: 0.1,
        },
        CharacterShape::Box2 { half: Vec2::ZERO },
    ] {
        let settings = AuthoringOptions {
            clearance: Some(SpawnClearance {
                shape,
                max_candidates: 257,
            }),
            ..options()
        };
        assert_eq!(
            ArenaBuilder::new(settings).unwrap_err(),
            ArenaAuthoringError::InvalidClearance
        );
    }
}

#[test]
fn bounded_builder_operations_validation_errors_and_reset_allocate_nothing_after_new() {
    let mut b = ArenaBuilder::new(options()).unwrap();
    let raw = arena(vec![cube(Vec3::new(0., 1., 0.5))], vec![Vec3::ZERO]);
    let invalid_owned = arena(vec![], vec![Vec3::splat(f32::NAN)]);
    let planar = AuthoringOptions {
        clearance: Some(SpawnClearance {
            shape: CharacterShape::Box2 {
                half: Vec2::new(0.3, 0.9),
            },
            max_candidates: 257,
        }),
        ..options()
    };
    ALLOCATIONS.with(|count| count.set(Some(0)));
    for _ in 0..2 {
        for _ in 0..MAX_SOLIDS {
            b.try_push_solid(cube(Vec3::X * 20.)).unwrap();
        }
        for _ in 0..MAX_SPAWNS {
            b.try_push_spawn(Vec3::ZERO).unwrap();
        }
        assert!(b.try_push_solid(cube(Vec3::ZERO)).is_err());
        assert!(b.try_push_spawn(Vec3::ZERO).is_err());
        b.validate().unwrap();
        b.reset();
        assert!(
            b.try_push_solid(Obstacle {
                center: Vec3::ZERO,
                half_extents: -Vec3::ONE
            })
            .is_err()
        );
        b.try_push_solid(cube(Vec3::Y)).unwrap();
        b.try_push_spawn(Vec3::ZERO).unwrap();
        assert!(b.validate().is_err());
        let rejected = b.finish().unwrap_err();
        b = rejected.input;
        assert!(matches!(
            rejected.error,
            ArenaAuthoringError::SpawnClearance { .. }
        ));
        assert!(validate_arena(&raw, planar).is_err());
        b.reset();
    }
    let rejected = try_from_arena(invalid_owned, options()).unwrap_err();
    assert!(rejected.input.spawns[0].x.is_nan());
    let budget = AuthoringOptions {
        clearance: Some(SpawnClearance {
            max_candidates: 0,
            ..Default::default()
        }),
        ..options()
    };
    assert!(validate_arena(&raw, budget).is_err());
    b.try_push_spawn(Vec3::ZERO).unwrap();
    let accepted = b.finish().unwrap();
    let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
    assert_eq!(allocations, 0);
    assert_eq!(accepted.spawns, [Vec3::ZERO]);
}
