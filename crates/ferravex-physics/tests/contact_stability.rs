#![cfg(all(feature = "dim2", feature = "dim3"))]
use ferravex_math::{Vec2, Vec3};
use ferravex_physics::{SolverSettings, d2, d3};
const DT: f32 = 1. / 120.;
fn stack(world: &mut d3::PhysicsWorld, x: f32, count: usize) -> Vec<d3::RigidBodyHandle> {
    (0..count)
        .map(|i| world.add_box([x, 0.5 + i as f32 * 1.002, 0.], [0.5; 3], true))
        .collect()
}
fn ground(world: &mut d3::PhysicsWorld) -> (d3::RigidBodyHandle, d3::ColliderHandle) {
    world.insert(
        d3::RigidBodyBuilder::fixed().translation(Vec3::Y * -0.5),
        d3::ColliderBuilder::cuboid(20., 0.5, 20.),
    )
}
fn settle(world: &mut d3::PhysicsWorld) {
    for _ in 0..1200 {
        world.step(DT);
    }
}
#[test]
fn persistent_points_seed_impulses_and_do_not_repeat_begin_events() {
    let mut w = d3::PhysicsWorld::new([0., -9.81, 0.]);
    w.set_solver_settings(SolverSettings {
        allow_sleep: false,
        ..Default::default()
    })
    .unwrap();
    ground(&mut w);
    stack(&mut w, 0., 1);
    for _ in 0..120 {
        w.step(DT);
    }
    let ids: Vec<_> = w
        .contact_manifolds()
        .flat_map(|m| m.points.iter().map(|p| p.id))
        .collect();
    assert_eq!(ids.len(), 4);
    let stats = w.step(DT);
    assert_eq!(stats.persisted_points, 4);
    assert_eq!(stats.warm_started_points, 4);
    assert!(w.contact_events().is_empty());
    assert_eq!(
        ids,
        w.contact_manifolds()
            .flat_map(|m| m.points.iter().map(|p| p.id))
            .collect::<Vec<_>>()
    );
    assert!(
        w.contact_manifolds()
            .flat_map(|m| m.points)
            .all(|p| p.normal_impulse > 0. && p.normal_impulse.is_finite())
    );
}
#[test]
fn low_iteration_warm_start_reduces_tall_stack_error() {
    let run = |warm_start, iterations| {
        let mut w = d3::PhysicsWorld::new([0., -9.81, 0.]);
        w.set_solver_settings(SolverSettings {
            allow_sleep: false,
            warm_start,
            velocity_iterations: iterations,
            position_iterations: 8,
            ..Default::default()
        })
        .unwrap();
        ground(&mut w);
        let bodies = stack(&mut w, 0., 12);
        for _ in 0..1200 {
            w.step(DT);
        }
        let error = bodies
            .iter()
            .enumerate()
            .map(|(i, &h)| (w.bodies[h].translation().y - (0.5 + i as f32)).abs())
            .fold(0f32, f32::max);
        let speed = bodies
            .iter()
            .map(|&h| w.bodies[h].linvel().length())
            .fold(0f32, f32::max);
        (error, speed)
    };
    for iterations in [8, 12] {
        let warm = run(true, iterations);
        let cold = run(false, iterations);
        assert!(
            warm.0 < 0.04 && warm.1 < 0.05,
            "warm stack unstable ({iterations} iterations): {warm:?}"
        );
        assert!(
            cold.0 > 1. && cold.0 > warm.0 * 2.,
            "missing measured warm-start benefit: warm={warm:?}, cold={cold:?}"
        );
    }
}
#[test]
fn independent_stacks_sleep_and_wake_as_separate_islands() {
    let mut w = d3::PhysicsWorld::new([0., -9.81, 0.]);
    ground(&mut w);
    let a = stack(&mut w, -4., 4);
    let b = stack(&mut w, 4., 3);
    settle(&mut w);
    assert!(a.iter().chain(&b).all(|&h| w.bodies[h].is_sleeping()));
    assert_eq!(w.last_step.sleeping_islands, 2);
    assert_eq!(w.last_step.velocity_constraints, 0);
    assert!(w.last_step.reused_manifolds > 0);
    w.bodies[a[3]].apply_impulse(Vec3::X * 0.1, true);
    let stats = w.step(DT);
    assert!(a.iter().all(|&h| !w.bodies[h].is_sleeping()));
    assert!(b.iter().all(|&h| w.bodies[h].is_sleeping()));
    assert_eq!(stats.active_islands, 1);
    assert_eq!(stats.sleeping_islands, 1);
}
#[test]
fn removing_support_and_teleporting_floor_wake_prior_contacts() {
    let mut w = d3::PhysicsWorld::new([0., -9.81, 0.]);
    let (floor, fc) = ground(&mut w);
    let bodies = stack(&mut w, 0., 4);
    settle(&mut w);
    w.bodies[floor].set_translation(Vec3::Y * -5.5, false);
    w.step(DT);
    assert!(
        bodies
            .iter()
            .all(|&h| !w.bodies[h].is_sleeping() && w.bodies[h].linvel().y < 0.)
    );
    assert!(
        w.contact_manifolds()
            .all(|m| m.collider1 != fc && m.collider2 != fc)
    );
    let mut w = d3::PhysicsWorld::new([0., -9.81, 0.]);
    ground(&mut w);
    let bodies = stack(&mut w, 0., 4);
    settle(&mut w);
    let collider = w.bodies[bodies[0]].colliders()[0];
    assert!(w.remove_body(bodies[0]));
    assert!(
        w.contact_manifolds()
            .all(|m| m.collider1 != collider && m.collider2 != collider)
    );
    w.step(DT);
    assert!(bodies[1..].iter().all(|&h| !w.bodies[h].is_sleeping()));
    let fresh = w.add_box([10., 0.5, 0.], [0.5; 3], true);
    assert_ne!(fresh, bodies[0]);
}
#[test]
fn material_pose_and_gravity_edits_cold_start_old_contacts() {
    let mut w = d3::PhysicsWorld::new([0., -9.81, 0.]);
    let (_, floor) = ground(&mut w);
    let body = stack(&mut w, 0., 1)[0];
    settle(&mut w);
    let ids: Vec<_> = w
        .contact_manifolds()
        .flat_map(|m| m.points.iter().map(|p| p.id))
        .collect();
    w.colliders[floor].set_friction(0.);
    let stats = w.step(DT);
    assert!(!w.bodies[body].is_sleeping());
    assert_eq!(stats.warm_started_points, 0);
    assert!(
        w.contact_manifolds()
            .flat_map(|m| m.points)
            .all(|p| !ids.contains(&p.id))
    );
    for _ in 0..30 {
        w.step(DT);
    }
    w.gravity = Vec3::ZERO;
    let stats = w.step(DT);
    assert_eq!(stats.warm_started_points, 0);
    assert!(w.bodies[body].linvel().length() < 0.01);
    w.bodies[body].set_translation(Vec3::new(0., 3., 0.), true);
    w.step(DT);
    assert_eq!(w.contact_manifolds().count(), 0);
}
#[test]
fn sensors_do_not_link_sleep_islands_or_cache_impulses() {
    let mut w = d2::PhysicsWorld::new([0., 0.]);
    let (a, _) = w.insert(
        d2::RigidBodyBuilder::dynamic(),
        d2::ColliderBuilder::ball(1.).sensor(true),
    );
    let (b, _) = w.insert(
        d2::RigidBodyBuilder::dynamic().translation(Vec2::X),
        d2::ColliderBuilder::ball(1.).sensor(true),
    );
    for _ in 0..120 {
        w.step(DT);
    }
    assert!(w.bodies[a].is_sleeping() && w.bodies[b].is_sleeping());
    assert_eq!(w.last_step.sleeping_islands, 2);
    assert_eq!(w.contact_manifolds().count(), 0);
    w.bodies[a].apply_impulse(Vec2::X, true);
    w.step(DT);
    assert!(!w.bodies[a].is_sleeping());
    assert!(w.bodies[b].is_sleeping());
    assert_eq!(w.last_step.velocity_constraints, 0);
}
#[test]
fn cache_capacity_falls_back_to_cold_solving_without_dropping_contacts() {
    let mut w = d3::PhysicsWorld::new([0., -9.81, 0.]);
    w.set_solver_settings(SolverSettings {
        max_cached_manifolds: 1,
        allow_sleep: false,
        ..Default::default()
    })
    .unwrap();
    ground(&mut w);
    let bodies: Vec<_> = [-4., 0., 4.]
        .into_iter()
        .map(|x| stack(&mut w, x, 1)[0])
        .collect();
    for _ in 0..240 {
        let s = w.step(DT);
        assert!(s.cached_manifolds <= 1);
    }
    assert!(w.last_step.cache_dropped >= 2);
    assert!(w.last_step.contact_points >= 12);
    assert!(
        bodies
            .iter()
            .all(|&h| (w.bodies[h].translation().y - 0.5).abs() < 0.01)
    );
}
#[test]
fn timestep_changes_do_not_reinject_old_load_or_bounce_energy() {
    let mut w = d3::PhysicsWorld::new([0., -9.81, 0.]);
    w.set_solver_settings(SolverSettings {
        allow_sleep: false,
        ..Default::default()
    })
    .unwrap();
    ground(&mut w);
    let body = stack(&mut w, 0., 1)[0];
    for _ in 0..120 {
        w.step(DT);
    }
    for _ in 0..100 {
        for dt in [1. / 60., 1. / 240., 1. / 30., DT] {
            w.step(dt);
            assert!(w.bodies[body].linvel().length() < 0.02);
            assert!((w.bodies[body].translation().y - 0.5).abs() < 0.01);
        }
    }
    let settings = w.solver_settings();
    assert!(
        w.set_solver_settings(SolverSettings {
            sleep_delay: f32::NAN,
            ..settings
        })
        .is_err()
    );
    assert_eq!(w.solver_settings(), settings);
}

#[test]
fn separated_contacts_keep_identity_but_never_an_old_support_impulse() {
    let mut w = d3::PhysicsWorld::new([0.; 3]);
    w.set_solver_settings(SolverSettings {
        allow_sleep: false,
        ..Default::default()
    })
    .unwrap();
    let (_, floor) = ground(&mut w);
    let body = stack(&mut w, 0., 1)[0];
    let collider = w.bodies[body].colliders()[0];
    w.step(DT);
    let ids: Vec<_> = w
        .contact_manifolds()
        .flat_map(|m| m.points.iter().map(|p| p.id))
        .collect();
    assert_eq!(ids.len(), 4);
    w.bodies[body].set_linvel(Vec3::Y * 0.5, true);
    w.step(DT);
    assert!(w.contact_pair(floor, collider).is_none());
    assert!(
        w.contact_manifolds()
            .flat_map(|m| m.points)
            .all(|p| p.normal_impulse == 0. && p.tangent_impulse == Vec3::ZERO)
    );
    w.bodies[body].set_linvel(Vec3::Y * -0.5, true);
    w.step(DT);
    assert!(w.contact_pair(floor, collider).is_some());
    let new: Vec<_> = w
        .contact_manifolds()
        .flat_map(|m| m.points.iter().map(|p| p.id))
        .collect();
    assert_eq!(ids, new);
}
#[test]
fn canonical_pair_normals_survive_broadphase_order_changes() {
    let mut w = d2::PhysicsWorld::new([0.; 2]);
    let (a, ca) = w.insert(
        d2::RigidBodyBuilder::fixed().translation(Vec2::X * -0.5),
        d2::ColliderBuilder::ball(1.).sensor(true),
    );
    let (b, cb) = w.insert(
        d2::RigidBodyBuilder::fixed().translation(Vec2::X * 0.5),
        d2::ColliderBuilder::ball(1.).sensor(true),
    );
    w.step(DT);
    assert!(w.contact_pair(ca, cb).unwrap().normal[0] > 0.);
    w.bodies[a].set_translation(Vec2::X * 0.5, false);
    w.bodies[b].set_translation(Vec2::X * -0.5, false);
    w.step(DT);
    let pair = w.contact_pair(ca, cb).unwrap();
    assert_eq!(pair.collider1, ca);
    assert_eq!(pair.collider2, cb);
    assert!(pair.normal[0] < 0.);
}

#[test]
fn cached_friction_always_stays_inside_the_current_normal_load_cone() {
    let mut w = d3::PhysicsWorld::new([0., -9.81, 0.]);
    w.set_solver_settings(SolverSettings {
        allow_sleep: false,
        ..Default::default()
    })
    .unwrap();
    ground(&mut w);
    let body = stack(&mut w, 0., 1)[0];
    for i in 0..600 {
        if i % 20 == 0 {
            w.bodies[body].apply_impulse(Vec3::new(0.3, 0.05, 0.), true);
        }
        w.step(DT);
        for point in w.contact_manifolds().flat_map(|m| m.points) {
            assert!(point.tangent_impulse.length() <= 0.6 * point.normal_impulse + 1e-5);
            assert!(point.tangent_impulse.dot(point.normal).abs() < 1e-5);
        }
    }
}

#[test]
fn inserting_a_cloned_body_does_not_share_collider_ownership_or_cache_lifetime() {
    let mut w = d3::PhysicsWorld::new([0., -9.81, 0.]);
    ground(&mut w);
    let original = stack(&mut w, 0., 1)[0];
    settle(&mut w);
    let old = w.bodies[original].colliders()[0];
    let mut clone = w.bodies[original].clone();
    clone.set_translation(Vec3::new(4., 0.5, 0.), true);
    let (fresh, collider) = w.insert(clone, d3::ColliderBuilder::cuboid(0.5, 0.5, 0.5));
    assert_eq!(w.bodies[fresh].colliders(), &[collider]);
    assert!(!w.bodies[fresh].is_sleeping());
    assert!(w.remove_body(fresh));
    assert!(w.colliders.get(old).is_some());
    w.step(DT);
    assert!(w.bodies[original].is_sleeping());
    assert_eq!(w.contact_manifolds().count(), 1);
}
