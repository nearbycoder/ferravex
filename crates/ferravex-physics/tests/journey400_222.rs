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
// SAFETY: The unchanged allocation contract is forwarded to System. Counters
// are thread-local, do not allocate, and ignore destruction-time TLS failures.
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

macro_rules! replacement_tests {
    ($module:ident, $dim:literal, $box:expr, $rotation:expr, $angular:expr, $angular_norm:expr) => {
        mod $module {
            use ferravex_physics::$module::{
                ColliderBuilder, ColliderShapeError, PhysicsWorld, QueryOptions, RigidBodyBuilder,
                SensorPolicy, Vector,
            };
            use ferravex_physics::{ContactEvent, MAX_CHILDREN, MAX_COLLIDERS, PhysicsError};
            const DT: f32 = 1. / 60.;
            fn point(x: f32, y: f32) -> [f32; $dim] {
                let mut p = [0.; $dim];
                p[0] = x;
                p[1] = y;
                p
            }
            fn close(a: Vector, b: Vector) {
                assert!((a - b).length() < 1e-4, "{a:?} != {b:?}");
            }
            fn snapshot(w: &PhysicsWorld) -> String {
                format!("{:?}{:?}{:?}{:?}{:?}", w.bodies, w.colliders,
                    w.sleep_stats(), w.contact_events(), w.last_step)
            }

            #[test]
            fn identity_settings_and_origin_motion_survive_mass_center_change() {
                let mut w = PhysicsWorld::default();
                let body = w.insert_compound(
                    RigidBodyBuilder::dynamic().translation(Vector::Y * 5.)
                        .rotation($angular(0.3)).angvel($angular(2.))
                        .linvel(Vector::X * 3.).ccd_enabled(true),
                    [ColliderBuilder::ball(0.5).translation(-Vector::X)
                        .rotation($rotation(0.2)).density(2.).friction(0.8)
                        .sensor(true).collision_layers(0b10, 0b100),
                     ColliderBuilder::ball(0.5).translation(Vector::X)],
                ).unwrap();
                let children = w.bodies[body].colliders().to_vec();
                let child = children[0];
                let other = format!("{:?}", w.colliders[children[1]]);
                let origin = w.bodies[body].translation();
                let rotation = *w.bodies[body].rotation();
                let velocity = w.bodies[body].velocity_at_point(origin);
                let old_center = w.bodies[body].center_of_mass();
                let old_linvel = w.bodies[body].linvel();
                let mass = w.bodies[body].mass();
                let local_pose = w.collider_world_pose(child).unwrap();
                let material = w.colliders[child].material();
                // All of these incoming non-geometric settings must be ignored.
                w.replace_collider_shape(child, ColliderBuilder::ball(1.)
                    .density(0.).translation(Vector::Y * 99.)
                    .sensor(false).collision_layers(0, 0).friction(0.)).unwrap();
                assert_eq!(w.bodies[body].colliders(), children.as_slice());
                assert_eq!(w.colliders[child].parent(), Some(body));
                assert_eq!(w.collider_world_pose(child).unwrap(), local_pose);
                assert_eq!(w.colliders[child].density(), 2.);
                assert_eq!(w.colliders[child].material(), material);
                assert!(w.colliders[child].is_sensor());
                assert_eq!(format!("{:?}", w.colliders[children[1]]), other);
                assert_eq!(w.bodies[body].translation(), origin);
                assert_eq!(*w.bodies[body].rotation(), rotation);
                close(w.bodies[body].velocity_at_point(origin), velocity);
                assert_eq!(w.bodies[body].angvel(), $angular(2.));
                assert!(w.bodies[body].center_of_mass().distance(old_center) > 0.2);
                assert!(w.bodies[body].linvel().distance(old_linvel) > 0.2);
                let ratio = (2. * 2_f32.powi($dim) + 1.) / 3.;
                assert!((w.bodies[body].mass() - mass * ratio).abs() < 1e-4);
                assert!(w.bodies[body].is_ccd_enabled());
                let p = local_pose.0;
                let mut probe = [0.; $dim];
                probe.copy_from_slice(&p.to_array()[..$dim]);
                assert!(w.overlap_point_with_options(probe,
                    QueryOptions::new(0.).layers(0b10).sensors(SensorPolicy::Only)).is_some());
                assert!(w.overlap_point_with_options(probe,
                    QueryOptions::new(0.).layers(0b1).sensors(SensorPolicy::Include)).is_none());
                assert!(w.overlap_point_with_options(probe, QueryOptions::new(0.)).is_none());
            }

            #[test]
            fn casts_and_overlap_read_new_primitive_and_current_child_pose_immediately() {
                let mut w = PhysicsWorld::default();
                let (body, child) = w.insert(
                    RigidBodyBuilder::fixed().translation(Vector::X * 5.),
                    ColliderBuilder::ball(0.5).translation(Vector::Y * 2.),
                );
                let origin = point(0., 2.);
                let direction = point(1., 0.);
                assert!((w.raycast(origin, direction, 20.).unwrap().distance - 4.5).abs() < 1e-5);
                w.replace_collider_shape(child, $box()).unwrap();
                let ray = w.raycast(origin, direction, 20.).unwrap();
                assert_eq!(ray.collider, child);
                assert_eq!(ray.body, Some(body));
                assert!((ray.distance - 3.).abs() < 1e-5);
                let sphere = w.spherecast_with_options(origin, direction, 0.25,
                    QueryOptions::new(20.)).unwrap();
                assert!((sphere.distance - 2.75).abs() < 1e-5);
                assert!(w.overlap_point_with_options(point(3.5, 2.), QueryOptions::new(0.)).is_some());
                let mut output = [None; 1];
                assert_eq!(w.overlap_sphere_into(point(2.75, 2.), 0.25,
                    QueryOptions::new(0.), &mut output).unwrap().total, 1);
                assert_eq!(output[0].unwrap().collider, child);
                assert!(w.contact_events().is_empty());
            }

            #[test]
            fn invalid_mass_and_pending_zero_density_are_atomic_then_recover() {
                let mut w = PhysicsWorld::default();
                let (body, child) = w.insert(RigidBodyBuilder::dynamic()
                    .angvel($angular(1.)), ColliderBuilder::ball(1.));
                w.bodies[body].add_force(Vector::X, true);
                w.bodies[body].set_next_kinematic_translation(Vector::Y * 4.);
                let before = snapshot(&w);
                for radius in [f32::MIN_POSITIVE, 1e20] {
                    assert_eq!(w.replace_collider_shape(child, ColliderBuilder::ball(radius)),
                        Err(ColliderShapeError::InvalidMass));
                    assert_eq!(snapshot(&w), before);
                }
                w.colliders[child].set_density(0.);
                let before = snapshot(&w);
                assert_eq!(w.replace_collider_shape(child, ColliderBuilder::ball(2.).density(9.)),
                    Err(ColliderShapeError::InvalidMass));
                assert_eq!(snapshot(&w), before);
                w.colliders[child].set_density(2.);
                w.replace_collider_shape(child, ColliderBuilder::ball(2.)).unwrap();
                let mass = w.bodies[body].mass();
                w.refresh_mass_properties().unwrap();
                assert_eq!(w.bodies[body].mass(), mass);
                assert_eq!(w.bodies[body].colliders(), &[child]);
            }

            #[test]
            fn zero_density_child_can_change_when_another_child_supplies_mass() {
                let mut w = PhysicsWorld::default();
                let body = w.insert_compound(RigidBodyBuilder::dynamic(), [
                    ColliderBuilder::ball(1.).density(0.), ColliderBuilder::ball(1.)
                ]).unwrap();
                let mass = w.bodies[body].mass();
                let child = w.bodies[body].colliders()[0];
                w.replace_collider_shape(child, $box()).unwrap();
                assert_eq!(w.bodies[body].mass(), mass);
                assert_eq!(w.colliders[child].density(), 0.);
            }

            #[test]
            fn fixed_and_kinematic_massless_shapes_keep_targets_and_forces_survive_dynamic_edit() {
                for (builder, kinematic) in [(RigidBodyBuilder::fixed(), false), (RigidBodyBuilder::kinematic_position_based(), true)] {
                    let mut w = PhysicsWorld::default();
                    let (body, child) = w.insert(builder, ColliderBuilder::ball(1.).density(0.));
                    w.bodies[body].set_next_kinematic_translation(Vector::X * 3.);
                    w.bodies[body].set_next_kinematic_rotation($rotation(0.4));
                    w.replace_collider_shape(child, $box()).unwrap();
                    assert_eq!(w.colliders[child].density(), 0.);
                    assert_eq!(w.bodies[body].translation(), Vector::ZERO);
                    w.step(DT);
                    if kinematic {
                        close(w.bodies[body].translation(), Vector::X * 3.);
                        assert_eq!(*w.bodies[body].rotation(), $rotation(0.4));
                    } else {
                        assert_eq!(w.bodies[body].translation(), Vector::ZERO);
                    }
                }
                let mut w = PhysicsWorld::default();
                let (body, child) = w.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(1.));
                w.bodies[body].add_force(Vector::X * 10., true);
                w.bodies[body].add_torque($angular(3.), true);
                w.replace_collider_shape(child, ColliderBuilder::ball(2.)).unwrap();
                let mass = w.bodies[body].mass();
                let mut expected = w.bodies[body].clone();
                expected.apply_torque_impulse($angular(3. * DT), true);
                w.step(DT);
                close(w.bodies[body].linvel(), Vector::X * (10. * DT / mass));
                assert!($angular_norm(w.bodies[body].angvel() - expected.angvel()) < 1e-7);
                let velocity = w.bodies[body].linvel();
                w.step(DT);
                assert_eq!(w.bodies[body].linvel(), velocity); // Consumed force did not reappear.
            }

            #[test]
            fn removed_handles_never_edit_reused_slots() {
                let mut w = PhysicsWorld::default();
                let (old_body, stale) = w.insert(RigidBodyBuilder::fixed(), ColliderBuilder::ball(1.));
                w.remove_collider(stale).unwrap();
                let live = w.attach_collider(old_body, ColliderBuilder::ball(1.)).unwrap();
                assert_ne!(stale, live);
                let before = snapshot(&w);
                assert_eq!(w.replace_collider_shape(stale, $box()), Err(ColliderShapeError::InvalidHandle));
                assert_eq!(snapshot(&w), before);
                w.replace_collider_shape(live, $box()).unwrap();
                w.remove_body(old_body);
                let (_, replacement) = w.insert(RigidBodyBuilder::fixed(), ColliderBuilder::ball(1.));
                let before = snapshot(&w);
                assert_eq!(w.replace_collider_shape(live, $box()), Err(ColliderShapeError::InvalidHandle));
                assert_eq!(snapshot(&w), before);
                assert_ne!(live, replacement);
            }

            #[test]
            fn replacement_at_all_capacities_allocates_nothing() {
                let mut w = PhysicsWorld::default();
                let mut selected = None;
                for i in 0..4096 {
                    let count = if i < MAX_COLLIDERS / MAX_CHILDREN { MAX_CHILDREN } else { 0 };
                    let body = w.insert_compound(RigidBodyBuilder::fixed(),
                        (0..count).map(|_| ColliderBuilder::ball(1.))).unwrap();
                    if i == 0 { selected = Some(w.bodies[body].colliders()[0]); }
                }
                let child = selected.unwrap();
                let parent = w.colliders[child].parent().unwrap();
                assert_eq!(w.bodies.len(), 4096);
                assert_eq!(w.colliders.len(), MAX_COLLIDERS);
                assert_eq!(w.bodies[parent].colliders().len(), MAX_CHILDREN);
                assert_eq!(w.attach_collider(parent, ColliderBuilder::ball(1.)), Err(PhysicsError::ColliderCapacity));
                super::ALLOCATIONS.with(|count| count.set(Some(0)));
                for _ in 0..64 {
                    w.replace_collider_shape(child, $box()).unwrap();
                }
                let allocations = super::ALLOCATIONS.with(|count| count.replace(None).unwrap());
                assert_eq!(allocations, 0);
                assert_eq!(w.bodies.len(), 4096);
                assert_eq!(w.colliders.len(), MAX_COLLIDERS);
                assert_eq!(w.bodies[parent].colliders()[0], child);
            }

            #[test]
            fn identical_shape_invalidates_only_affected_contacts_and_keeps_event_lifecycle() {
                let mut w = PhysicsWorld::new(point(0., -9.81));
                let floor = w.insert(RigidBodyBuilder::fixed(), $box()).1;
                let (body, child) = w.insert(RigidBodyBuilder::dynamic()
                    .translation(Vector::Y * 1.5), ColliderBuilder::ball(1.));
                let (_, other) = w.insert(RigidBodyBuilder::dynamic()
                    .translation(Vector::X * 10.), ColliderBuilder::ball(1.));
                w.insert(RigidBodyBuilder::fixed().translation(Vector::X * 10. + Vector::Y * -1.5), $box());
                for _ in 0..120 { w.step(DT); }
                let ids = |world: &PhysicsWorld, target| world.contact_manifolds()
                    .filter(|m| m.collider1 == target || m.collider2 == target)
                    .flat_map(|m| m.points.iter().map(|p| p.id)).collect::<Vec<_>>();
                let old = ids(&w, child);
                let unaffected = ids(&w, other);
                assert!(!old.is_empty());
                assert!(!unaffected.is_empty());
                let events = w.contact_events().to_vec();
                w.replace_collider_shape(child, ColliderBuilder::ball(1.)).unwrap();
                assert_eq!(ids(&w, child), old); // Last-substep snapshot.
                assert_eq!(w.contact_events(), events);
                assert!(!w.bodies[body].is_sleeping());
                w.step(DT);
                assert!(ids(&w, child).iter().all(|id| !old.contains(id)));
                assert_eq!(ids(&w, other), unaffected);
                assert!(w.contact_events().is_empty());
                w.replace_collider_shape(child, ColliderBuilder::ball(0.1)).unwrap();
                w.step(DT);
                assert!(w.contact_events().iter().any(|event| matches!(event,
                    ContactEvent::Stopped(a, b) if (*a == child && *b == floor) || (*b == child && *a == floor))));
                w.step(DT);
                assert!(w.contact_events().is_empty());
            }

            #[test]
            fn full_dynamic_compound_validates_every_child_and_collects_pending_density() {
                let mut w = PhysicsWorld::default();
                let body = w.insert_compound(RigidBodyBuilder::dynamic(),
                    (0..MAX_CHILDREN).map(|i| ColliderBuilder::ball(1.)
                        .density(if i == 0 { 1. } else { 0. }))).unwrap();
                let children = w.bodies[body].colliders().to_vec();
                let before = snapshot(&w);
                assert_eq!(w.replace_collider_shape(children[0], ColliderBuilder::ball(f32::MIN_POSITIVE)),
                    Err(ColliderShapeError::InvalidMass));
                assert_eq!(snapshot(&w), before);
                w.colliders[children[1]].set_density(1.);
                w.replace_collider_shape(children[0], $box()).unwrap();
                let expected = ferravex_physics::MassProperties::try_from_colliders(
                    children.iter().map(|h| &w.colliders[*h]), $dim == 2).unwrap();
                assert_eq!(w.bodies[body].mass(), expected.mass());
                let state = snapshot(&w);
                w.refresh_mass_properties().unwrap();
                assert_eq!(snapshot(&w), state);
                assert_eq!(w.bodies[body].colliders(), children.as_slice());
                assert_eq!(w.attach_collider(body, ColliderBuilder::ball(1.)), Err(PhysicsError::ChildCapacity));
            }

            #[test]
            fn fixed_shape_growth_wakes_new_solid_contacts_but_sensors_stay_isolated() {
                for sensor in [false, true] {
                    let mut w = PhysicsWorld::default();
                    let (body, _) = w.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(0.5));
                    let (unrelated, _) = w.insert(RigidBodyBuilder::dynamic()
                        .translation(Vector::X * 20.), ColliderBuilder::ball(0.5));
                    let (_, edited) = w.insert(RigidBodyBuilder::fixed()
                        .translation(Vector::X * 2.), ColliderBuilder::ball(0.5).sensor(sensor));
                    for _ in 0..120 { w.step(DT); }
                    assert!(w.bodies[body].is_sleeping());
                    assert!(w.bodies[unrelated].is_sleeping());
                    w.replace_collider_shape(edited, ColliderBuilder::ball(2.)).unwrap();
                    w.step(DT);
                    assert_eq!(w.bodies[body].is_sleeping(), sensor);
                    assert!(w.bodies[unrelated].is_sleeping());
                    assert_eq!(w.contacts().next().unwrap().sensor, sensor);
                }
            }

            #[test]
            fn shrinking_fixed_support_wakes_previous_sleeping_contact_island() {
                let mut w = PhysicsWorld::new(point(0., -9.81));
                let (_, support) = w.insert(RigidBodyBuilder::fixed(), ColliderBuilder::ball(1.));
                let (body, _) = w.insert(RigidBodyBuilder::dynamic()
                    .translation(Vector::Y * 1.5), ColliderBuilder::ball(0.5));
                for _ in 0..120 { w.step(DT); }
                assert!(w.bodies[body].is_sleeping());
                w.replace_collider_shape(support, ColliderBuilder::ball(0.1)).unwrap();
                let height = w.bodies[body].translation()[1];
                w.step(DT);
                assert!(!w.bodies[body].is_sleeping());
                assert!(w.bodies[body].translation()[1] < height);
            }

            #[test]
            fn rejected_and_successful_dynamic_replacements_allocate_nothing() {
                let mut w = PhysicsWorld::default();
                let (body, child) = w.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(1.));
                let (_, stale) = w.insert(RigidBodyBuilder::fixed(), ColliderBuilder::ball(1.));
                w.remove_collider(stale).unwrap();
                super::ALLOCATIONS.with(|count| count.set(Some(0)));
                for _ in 0..64 {
                    assert_eq!(w.replace_collider_shape(child, ColliderBuilder::ball(1e20)), Err(ColliderShapeError::InvalidMass));
                    assert_eq!(w.replace_collider_shape(stale, $box()), Err(ColliderShapeError::InvalidHandle));
                    w.replace_collider_shape(child, $box()).unwrap();
                    w.replace_collider_shape(child, ColliderBuilder::ball(1.)).unwrap();
                }
                let allocations = super::ALLOCATIONS.with(|count| count.replace(None).unwrap());
                assert_eq!(allocations, 0);
                assert_eq!(w.bodies[body].colliders(), &[child]);
            }
        }
    };
}
#[cfg(feature = "dim2")]
replacement_tests!(
    d2,
    2,
    || ColliderBuilder::cuboid(2., 0.5),
    ferravex_physics::Rotation2::from_angle,
    |angle| angle,
    |v: f32| v.abs()
);
#[cfg(feature = "dim3")]
replacement_tests!(
    d3,
    3,
    || ColliderBuilder::cuboid(2., 0.5, 0.25),
    ferravex_math::Quat::from_rotation_z,
    |angle| ferravex_math::Vec3::Z * angle,
    |v: ferravex_math::Vec3| v.length()
);

#[cfg(all(feature = "dim2", feature = "dim3"))]
#[test]
fn cross_dimension_geometry_is_checked_before_live_edit() {
    use ferravex_physics::{d2, d3};
    let mut w = d2::PhysicsWorld::default();
    let (_, child) = w.insert(d2::RigidBodyBuilder::fixed(), d2::ColliderBuilder::ball(1.));
    let before = format!("{:?}{:?}", w.bodies, w.colliders);
    assert_eq!(
        w.replace_collider_shape(child, d3::ColliderBuilder::cylinder(1., 1.)),
        Err(d2::ColliderShapeError::InvalidCollider)
    );
    assert_eq!(format!("{:?}{:?}", w.bodies, w.colliders), before);
}

#[cfg(feature = "dim2")]
#[test]
fn capsule_endpoints_change_com_without_changing_origin_velocity() {
    use ferravex_math::Vec2;
    use ferravex_physics::d2::{ColliderBuilder, PhysicsWorld, RigidBodyBuilder};
    let mut w = PhysicsWorld::default();
    let (body, child) = w.insert(
        RigidBodyBuilder::dynamic().angvel(2.),
        ColliderBuilder::ball(0.5),
    );
    let origin = w.bodies[body].translation();
    let velocity = w.bodies[body].velocity_at_point(origin);
    w.replace_collider_shape(
        child,
        ColliderBuilder::try_capsule_from_endpoints(Vec2::X, Vec2::X * 3., 0.5).unwrap(),
    )
    .unwrap();
    assert!((w.bodies[body].local_center_of_mass() - Vec2::X * 2.).length() < 1e-5);
    assert!((w.bodies[body].velocity_at_point(origin) - velocity).length() < 1e-5);
    assert_eq!(w.raycast([0.; 2], [1., 0.], 10.).unwrap().collider, child);
}

#[cfg(feature = "dim3")]
#[test]
fn cylinder_replacement_updates_queries_and_torque_response() {
    use ferravex_math::Vec3;
    use ferravex_physics::d3::{ColliderBuilder, PhysicsWorld, RigidBodyBuilder};
    let mut w = PhysicsWorld::default();
    let (body, child) = w.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(0.5));
    w.replace_collider_shape(child, ColliderBuilder::try_cylinder(2., 0.5).unwrap())
        .unwrap();
    let mass = w.bodies[body].mass();
    assert!((mass - std::f32::consts::PI).abs() < 1e-5);
    let hit = w.raycast([0., 5., 0.], [0., -1., 0.], 10.).unwrap();
    assert_eq!(hit.collider, child);
    assert!((hit.distance - 3.).abs() < 1e-5);
    w.bodies[body].apply_torque_impulse(Vec3::Y, true);
    assert!((w.bodies[body].angvel().y - 8. / mass).abs() < 1e-5);
}
