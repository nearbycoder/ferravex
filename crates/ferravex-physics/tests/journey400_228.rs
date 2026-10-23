use ferravex_math::{Quat, Vec2, Vec3};
use ferravex_physics::character::{
    Character, CharacterSettings, CharacterShape, Clearance, ClearanceError, JumpState, Obstacle,
    ObstacleShape, QueryBounds, Support, Terrain, check_clearance,
};
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
// Forward the allocator contract unchanged; only count calls on the test thread.
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

fn shapes(height: f32) -> [CharacterShape; 2] {
    [
        CharacterShape::Capsule {
            radius: 0.3,
            height,
        },
        CharacterShape::Box2 {
            half: Vec2::new(0.3, height * 0.5),
        },
    ]
}

fn motor() -> Character {
    Character {
        position: Vec3::Y * 0.002,
        velocity: Vec3::new(3., -2., 0.),
        grounded: true,
        ground_normal: Vec3::new(-0.6, 0.8, 0.),
        jump: JumpState {
            coyote: 0.05,
            buffer: 0.1,
            held: true,
        },
        support: Some(Support {
            id: 7,
            local_anchor: Vec3::new(1., 0.1, 0.),
            previous_anchor: Vec3::new(2., 3., 0.),
        }),
    }
}

fn ceiling(id: u64, bottom: f32) -> Obstacle {
    Obstacle::cuboid(id, Vec3::Y * (bottom + 0.1), Vec3::new(2., 0.1, 2.))
}

#[test]
fn ceiling_rejection_is_atomic_and_shrinking_can_clear_an_existing_overlap() {
    let c = motor();
    let saved = c;
    let world = [ceiling(9, 1.4)];
    for (mut shape, short) in shapes(1.8).into_iter().zip(shapes(1.)) {
        // The initial tall shape penetrates, but shrinking checks only its target.
        assert_eq!(
            c.try_set_height(&world[..], &mut shape, 1., u32::MAX, 1),
            Ok(Clearance { candidates: 0 })
        );
        assert_eq!(shape, short);
        assert_eq!(c, saved);
        assert_eq!(
            c.try_set_height(&world[..], &mut shape, 1.8, u32::MAX, 1),
            Err(ClearanceError::Blocked { obstacle: 9 })
        );
        assert_eq!(shape, short);
        assert_eq!(c, saved);
        c.try_set_height(&[][..], &mut shape, 1.8, u32::MAX, 0)
            .unwrap();
        assert_eq!(shape.half_extents().x, short.half_extents().x);
        assert_eq!(shape.half_extents().y, 0.9);
        assert_eq!(c, saved);
    }
}

#[test]
fn clearance_uses_geometry_accepts_touching_and_checks_even_unchanged_height() {
    for mut shape in shapes(1.8) {
        let c = Character::new(Vec3::ZERO);
        let world = [
            Obstacle::cuboid(1, -Vec3::Y * 0.1, Vec3::new(4., 0.1, 4.)),
            ceiling(2, 1.8),
        ];
        assert_eq!(
            check_clearance(&world[..], shape, c.position, u32::MAX, 2),
            Ok(Clearance { candidates: 2 })
        );
        let original = shape;
        assert_eq!(
            c.try_set_height(&[ceiling(9, 1.4)][..], &mut shape, 1.8, u32::MAX, 1),
            Err(ClearanceError::Blocked { obstacle: 9 })
        );
        assert_eq!(shape, original);
        // Shrinking is still checked, and cannot authorize a penetrated target.
        assert_eq!(
            c.try_set_height(&[ceiling(9, 0.8)][..], &mut shape, 1., u32::MAX, 1),
            Err(ClearanceError::Blocked { obstacle: 9 })
        );
        assert_eq!(shape, original);
    }
    let ball = Obstacle {
        shape: ObstacleShape::Ball { radius: 0.3 },
        position: Vec3::new(0.5, -0.2, 0.),
        ..ceiling(5, 0.)
    };
    let shape = shapes(1.8)[0];
    assert!(shape.bounds(Vec3::ZERO).intersects(ball.bounds()));
    assert_eq!(
        check_clearance(&[ball][..], shape, Vec3::ZERO, u32::MAX, 1),
        Ok(Clearance { candidates: 1 })
    );
}

#[test]
fn zero_exact_and_exhausted_candidate_limits_preserve_both_values() {
    let c = motor();
    let saved = c;
    let sensor = Obstacle {
        sensor: true,
        ..ceiling(4, 0.5)
    };
    let second_sensor = Obstacle { id: 5, ..sensor };
    for mut shape in shapes(1.) {
        let original = shape;
        assert_eq!(
            check_clearance(&[][..], shape, c.position, u32::MAX, 0),
            Ok(Clearance { candidates: 0 })
        );
        for (world, limit) in [
            (&[sensor][..], 0),
            (&[sensor, second_sensor][..], 1),
            (&[ceiling(9, 1.4), sensor][..], 1),
        ] {
            assert_eq!(
                c.try_set_height(world, &mut shape, 1.8, u32::MAX, limit),
                Err(ClearanceError::QueryBudget)
            );
            assert_eq!(shape, original);
            assert_eq!(c, saved);
        }
        assert_eq!(
            c.try_set_height(&[sensor, second_sensor][..], &mut shape, 1.8, u32::MAX, 2),
            Ok(Clearance { candidates: 2 })
        );
        assert_eq!(c, saved);
    }
}

struct RepeatedSensor {
    count: usize,
    calls: Cell<usize>,
}
impl Terrain for RepeatedSensor {
    fn visit(&self, _: QueryBounds, visitor: &mut dyn FnMut(Obstacle)) {
        self.calls.set(self.calls.get() + 1);
        let sensor = Obstacle {
            sensor: true,
            ..ceiling(1, 0.5)
        };
        for id in 0..self.count {
            visitor(Obstacle {
                id: id as u64,
                ..sensor
            });
        }
    }
    fn obstacle(&self, _: u64) -> Option<Obstacle> {
        panic!("clearance must not look up or refresh support")
    }
}

#[test]
fn full_candidate_bound_and_oversized_limits_are_checked_without_scratch_storage() {
    let world = RepeatedSensor {
        count: 1_000_000,
        calls: Cell::new(0),
    };
    let shape = shapes(1.8)[0];
    assert_eq!(
        check_clearance(&world, shape, Vec3::ZERO, u32::MAX, 1_000_000),
        Ok(Clearance {
            candidates: 1_000_000
        })
    );
    assert_eq!(
        check_clearance(&world, shape, Vec3::ZERO, u32::MAX, 999_999),
        Err(ClearanceError::QueryBudget)
    );
    let calls = world.calls.get();
    for limit in [1_000_001, usize::MAX] {
        assert_eq!(
            check_clearance(&world, shape, Vec3::ZERO, u32::MAX, limit),
            Err(ClearanceError::InvalidInput)
        );
    }
    assert_eq!(world.calls.get(), calls);
}

#[test]
fn filtering_matches_motor_and_blocker_selection_is_order_independent() {
    for shape in shapes(1.8) {
        let sensor = Obstacle {
            sensor: true,
            ..ceiling(1, 0.5)
        };
        let excluded = Obstacle {
            membership: 2,
            ..ceiling(2, 0.5)
        };
        assert_eq!(
            check_clearance(&[sensor, excluded][..], shape, Vec3::ZERO, 1, 2),
            Ok(Clearance { candidates: 2 })
        );
        for world in [
            [ceiling(9, 0.5), ceiling(3, 1.)],
            [ceiling(3, 1.), ceiling(9, 0.5)],
        ] {
            assert_eq!(
                check_clearance(&world[..], shape, Vec3::ZERO, u32::MAX, 2),
                Err(ClearanceError::Blocked { obstacle: 3 })
            );
        }
        let malformed = Obstacle {
            position: Vec3::splat(f32::INFINITY),
            ..ceiling(1, 0.5)
        };
        assert_eq!(
            check_clearance(&[ceiling(9, 0.5), malformed][..], shape, Vec3::ZERO, 1, 2),
            Err(ClearanceError::InvalidObstacle)
        );
        // As in movement, invalid sensors and excluded layers are not evaluated.
        assert!(
            check_clearance(
                &[Obstacle {
                    sensor: true,
                    ..malformed
                }][..],
                shape,
                Vec3::ZERO,
                1,
                1
            )
            .is_ok()
        );
    }
}

#[test]
fn invalid_height_shape_pose_and_motor_history_are_rejected_atomically() {
    let world = RepeatedSensor {
        count: 0,
        calls: Cell::new(0),
    };
    let c = motor();
    for mut shape in shapes(1.8) {
        let original = shape;
        for height in [f32::NAN, f32::INFINITY, -1., 0., 0.001, 2001.] {
            assert_eq!(
                c.try_set_height(&world, &mut shape, height, u32::MAX, 1),
                Err(ClearanceError::InvalidInput)
            );
            assert_eq!(shape, original);
        }
        for feet in [Vec3::splat(f32::NAN), Vec3::X * 1_000_001.] {
            assert_eq!(
                check_clearance(&world, shape, feet, u32::MAX, 1),
                Err(ClearanceError::InvalidInput)
            );
        }
        for invalid in [
            Character {
                velocity: Vec3::X * f32::INFINITY,
                ..c
            },
            Character {
                ground_normal: Vec3::Y * f32::INFINITY,
                ..c
            },
            Character {
                jump: JumpState {
                    coyote: -0.1,
                    ..c.jump
                },
                ..c
            },
            Character {
                support: Some(Support {
                    local_anchor: Vec3::Y * 1_000_001.,
                    ..c.support.unwrap()
                }),
                ..c
            },
        ] {
            assert_eq!(
                invalid.try_set_height(&world, &mut shape, 1., u32::MAX, 1),
                Err(ClearanceError::InvalidInput)
            );
            assert_eq!(shape, original);
        }
    }
    let mut shape = CharacterShape::Capsule {
        radius: 0.,
        height: 1.8,
    };
    let saved = shape;
    assert_eq!(
        c.try_set_height(&world, &mut shape, 1., u32::MAX, 1),
        Err(ClearanceError::InvalidInput)
    );
    assert_eq!(shape, saved);
    assert_eq!(world.calls.get(), 0);
    assert_eq!(c, motor());
    let mut shape = shapes(1.8)[0];
    assert_eq!(
        c.try_set_height(&world, &mut shape, 0.599, u32::MAX, 1),
        Err(ClearanceError::InvalidInput)
    );
    c.try_set_height(&world, &mut shape, 0.6, u32::MAX, 1)
        .unwrap();
}

#[test]
fn planar_limits_and_unsupported_obstacles_use_explicit_invalid_outcomes() {
    let mut shape = shapes(1.)[1];
    let original = shape;
    for c in [
        Character::new(Vec3::Z),
        Character {
            velocity: Vec3::Z,
            ..Character::new(Vec3::ZERO)
        },
    ] {
        assert_eq!(
            c.try_set_height(&[][..], &mut shape, 1.8, u32::MAX, 1),
            Err(ClearanceError::InvalidInput)
        );
        assert_eq!(shape, original);
    }
    let c = Character::new(Vec3::ZERO);
    for obstacle in [
        Obstacle {
            shape: ObstacleShape::Cylinder {
                radius: 0.3,
                half_height: 0.3,
            },
            ..ceiling(9, 0.5)
        },
        Obstacle {
            rotation: Quat::from_rotation_x(0.2),
            ..ceiling(9, 0.5)
        },
    ] {
        assert_eq!(
            c.try_set_height(&[obstacle][..], &mut shape, 1.8, u32::MAX, 1),
            Err(ClearanceError::InvalidObstacle)
        );
        assert_eq!(shape, original);
    }
    c.try_set_height(&[][..], &mut shape, 2000., u32::MAX, 0)
        .unwrap();
    assert_eq!(shape.half_extents(), Vec3::new(0.3, 1000., 0.));
}

#[test]
fn moving_rotating_support_is_preserved_and_carry_is_applied_once_on_the_next_tick() {
    for mut shape in shapes(1.8) {
        let planar = matches!(shape, CharacterShape::Box2 { .. });
        let settings = CharacterSettings::default();
        let mut platform = Obstacle::cuboid(7, -Vec3::Y * 0.1, Vec3::new(4., 0.1, 4.));
        let mut c = Character::new(Vec3::new(0.6, settings.skin, 0.));
        c.teleport(&[platform][..], shape, settings, c.position)
            .unwrap();
        c.velocity.x = 2.;
        c.jump.held = true;
        c.jump.buffer = 0.08;
        let saved = c;
        platform.position.x += 0.1;
        if !planar {
            platform.rotation = Quat::from_rotation_y(0.2);
        }
        let support = saved.support.unwrap();
        let expected =
            platform.position + platform.rotation * support.local_anchor - support.previous_anchor;
        c.try_set_height(&[platform][..], &mut shape, 1., u32::MAX, 1)
            .unwrap();
        assert_eq!(c, saved);
        let tick = c
            .step(
                &[platform][..],
                shape,
                settings,
                Default::default(),
                1. / 120.,
            )
            .unwrap();
        assert!((tick.carried - expected).length() < 1e-5);
        assert_eq!(tick.motion.support_motion.unwrap().id, 7);
        assert!(
            tick.jumped,
            "the height change must retain the pending buffer"
        );
        assert!(
            c.velocity.x > 10.,
            "jump must inherit pending platform velocity"
        );
    }
}

#[test]
fn reset_discards_support_after_a_height_change_and_clearance_has_no_history() {
    for mut shape in shapes(1.8) {
        let mut c = motor();
        c.try_set_height(&[][..], &mut shape, 1., u32::MAX, 0)
            .unwrap();
        c.reset(shape, Vec3::Y * 10.).unwrap();
        assert_eq!(c, Character::new(Vec3::Y * 10.));
        c.try_set_height(&[][..], &mut shape, 1.8, u32::MAX, 0)
            .unwrap();
        let tick = c
            .step(
                &[][..],
                shape,
                Default::default(),
                Default::default(),
                1. / 120.,
            )
            .unwrap();
        assert_eq!(tick.motion.support_motion, None);
        assert!(!tick.jumped);
        assert_eq!(tick.motion.recovery.distance, 0.);
    }
}

#[test]
fn query_and_transition_success_and_failures_allocate_nothing_with_slice_terrain() {
    for mut shape in shapes(1.) {
        let c = motor();
        let world = [ceiling(9, 1.4)];
        ALLOCATIONS.with(|count| count.set(Some(0)));
        let clear = check_clearance(&world[..], shape, c.position, u32::MAX, 1);
        let changed = c.try_set_height(&world[..], &mut shape, 0.8, u32::MAX, 1);
        let blocked = c.try_set_height(&world[..], &mut shape, 1.8, u32::MAX, 1);
        let exhausted = c.try_set_height(&world[..], &mut shape, 1.8, u32::MAX, 0);
        let invalid = c.try_set_height(&world[..], &mut shape, f32::NAN, u32::MAX, 1);
        let allocations = ALLOCATIONS.with(|count| count.replace(None).unwrap());
        assert_eq!(allocations, 0);
        assert!(clear.is_ok());
        assert!(changed.is_ok());
        assert_eq!(blocked, Err(ClearanceError::Blocked { obstacle: 9 }));
        assert_eq!(exhausted, Err(ClearanceError::QueryBudget));
        assert_eq!(invalid, Err(ClearanceError::InvalidInput));
    }
}

#[test]
fn a_platform_moved_into_the_feet_is_checked_without_recovery_or_anchor_changes() {
    for mut shape in shapes(1.8) {
        let settings = CharacterSettings::default();
        let mut platform = Obstacle::cuboid(7, -Vec3::Y * 0.1, Vec3::new(4., 0.1, 4.));
        let mut c = Character::new(Vec3::Y * settings.skin);
        c.teleport(&[platform][..], shape, settings, c.position)
            .unwrap();
        let saved = c;
        let saved_shape = shape;
        platform.position.y += 0.05;
        assert_eq!(
            c.try_set_height(&[platform][..], &mut shape, 1., u32::MAX, 1),
            Err(ClearanceError::Blocked { obstacle: 7 })
        );
        assert_eq!(c, saved);
        assert_eq!(shape, saved_shape);
        // Existing motor recovery/carry is still the next tick's responsibility.
        let tick = c
            .step(
                &[platform][..],
                shape,
                settings,
                Default::default(),
                1. / 120.,
            )
            .unwrap();
        assert!((tick.carried.y - 0.05).abs() < 1e-5);
    }
}

macro_rules! rigid_clearance_tests {
    ($module:ident, $dim:literal, $shape:expr) => {
        mod $module {
            use super::*;
            use ferravex_physics::$module::{PhysicsWorld, Vector};

            fn point(x: f32, y: f32) -> [f32; $dim] {
                let mut p = [0.; $dim];
                p[0] = x;
                p[1] = y;
                p
            }

            #[test]
            fn existing_world_adapter_reads_immediate_pose_edits_and_retains_support() {
                let mut world = PhysicsWorld::new([0.; $dim]);
                let mut half = [4.; $dim];
                half[1] = 0.1;
                let platform = world.add_box(point(0., -0.1), half, false);
                let ceiling = world.add_box(point(0., 1.5), half, false);
                let settings = CharacterSettings::default();
                let mut shape = shapes(1.)[$shape];
                let mut c = Character::new(Vec3::Y * settings.skin);
                c.teleport(&world, shape, settings, c.position).unwrap();
                let saved = c;
                assert!(matches!(
                    c.try_set_height(&world, &mut shape, 1.8, u32::MAX, 2),
                    Err(ClearanceError::Blocked { .. })
                ));
                assert_eq!(c, saved);
                world.bodies[ceiling].set_translation(Vector::from_array(point(0., 10.)), true);
                c.try_set_height(&world, &mut shape, 1.8, u32::MAX, 1)
                    .unwrap();
                assert_eq!(c, saved);
                world.bodies[platform].set_translation(Vector::from_array(point(0.1, -0.1)), true);
                c.try_set_height(&world, &mut shape, 1., u32::MAX, 1)
                    .unwrap();
                assert_eq!(c, saved);
                let tick = c
                    .step(&world, shape, settings, Default::default(), 1. / 120.)
                    .unwrap();
                assert!((tick.carried.x - 0.1).abs() < 1e-5);
                assert!(c.grounded);
                let next = c
                    .step(&world, shape, settings, Default::default(), 1. / 120.)
                    .unwrap();
                assert!(next.carried.length() < 1e-5);
            }
        }
    };
}

#[cfg(feature = "dim2")]
rigid_clearance_tests!(d2, 2, 1);
#[cfg(feature = "dim3")]
rigid_clearance_tests!(d3, 3, 0);
