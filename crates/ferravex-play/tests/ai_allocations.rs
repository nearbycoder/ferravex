use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! {static RECORD:Cell<bool>=const {Cell::new(false)};static CALLS:Cell<usize>=const {Cell::new(0)};}
struct Allocator;
fn allocated(p: *mut u8) {
    if !p.is_null() {
        let _ = RECORD.try_with(|on| {
            if on.get() {
                let _ = CALLS.try_with(|n| n.set(n.get() + 1));
            }
        });
    }
}
// SAFETY: Every operation forwards the identical allocation contract to System;
// thread-local counters do not allocate and ignore destruction-time TLS failures.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        allocated(p);
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(l) };
        allocated(p);
        p
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let p = unsafe { System.realloc(p, l, n) };
        allocated(p);
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
#[test]
fn shared_gameplay_systems_allocate_nothing_after_construction() {
    use ferravex_math::Vec3;
    use ferravex_play::{ai::*, navigation::*};
    let mut abilities = ferravex_play::ability::AbilityBank::<2>::new(
        1,
        &[ferravex_play::ability::AbilitySpec {
            charges: 2,
            recharge_seconds: 0.1,
            cost: 0.,
        }],
    )
    .unwrap();
    let mut resource = ferravex_play::ability::ResourcePool::new(0., 0., 0.).unwrap();
    let mut statuses = ferravex_play::attributes::EffectSet::<4>::new().unwrap();
    let mut feedback =
        ferravex_play::feedback::FeedbackController::<16>::new(Default::default(), 1).unwrap();
    let grid = Grid::new(16, 16).unwrap();
    let mut planner = PathPlanner::new(256, 8).unwrap();
    let graph = DecisionGraph::new(
        2,
        0,
        &[
            DecisionEdge {
                from: Some(0),
                to: 1,
                all: 1,
                none: 0,
                after: 0.,
            },
            DecisionEdge {
                from: Some(1),
                to: 0,
                all: 0,
                none: 1,
                after: 0.,
            },
        ],
    )
    .unwrap();
    let mut state = graph.initial();
    let mut enemy = EnemyController::new(EnemyConfig::ranged(), 1).unwrap();
    let mut memory = Memory::new(MemorySettings::default()).unwrap();
    let mut sounds = NoiseBuffer::new(8).unwrap();
    let mut route = Route3::new(64).unwrap();
    let mut follower = WaypointFollower::default();
    let mut cursor = PathCursor::default();
    let mut scheduler = RoundRobin::default();
    let candidate = Candidate {
        id: 1,
        position: Vec3::Z,
        groups: 1,
    };
    let sight = Sight::new(10., 2., 1).unwrap();
    let hearing = Hearing::new(10., 1, 0.5).unwrap();
    let mut points = [Vec3::ZERO; 64];
    RECORD.with(|v| v.set(true));
    for tick in 0..1000 {
        abilities.advance(1. / 60.).unwrap();
        let _ = abilities.activate(0, &mut resource, true);
        statuses.advance(1. / 60.).unwrap();
        statuses
            .apply(
                ferravex_play::attributes::EffectSpec::modifier(1, 0, 0.5, 1.),
                0,
            )
            .unwrap();
        assert_eq!(statuses.value(0, 100., 0., 1000.).unwrap(), 50.);
        let _ = feedback.emit(
            tick + 1,
            ferravex_play::feedback::FeedbackEvent {
                kind: ferravex_play::feedback::FeedbackKind::Hurt,
                intensity: 1.,
                position: Vec3::ZERO,
            },
        );
        feedback.advance(1. / 60.).unwrap();
        ferravex_play::tuning::TuningProfile::rally()
            .fingerprint()
            .unwrap();
        let mut budget = WorkBudget::new(64, 32, 64);
        memory.advance(1. / 60.).unwrap();
        sounds.advance(1. / 60.).unwrap();
        sounds
            .push(
                Noise {
                    source: 1,
                    position: Vec3::Z,
                    groups: 1,
                    radius: 10.,
                },
                0.1,
            )
            .unwrap();
        assert_eq!(
            sight
                .test(
                    Observer {
                        position: Vec3::ZERO,
                        forward: Vec3::Z
                    },
                    candidate,
                    &mut budget,
                    &mut Unoccluded
                )
                .unwrap(),
            Detection::Detected
        );
        memory.observe(candidate, Sense::Sight).unwrap();
        for noise in sounds.iter() {
            hearing
                .test(Vec3::ZERO, noise, &mut budget, Some(&mut Unoccluded))
                .unwrap();
            memory
                .observe(
                    Candidate {
                        id: noise.source,
                        position: noise.position,
                        groups: noise.groups,
                    },
                    Sense::Hearing,
                )
                .unwrap();
        }
        state.update(&graph, tick % 2, 1. / 60.).unwrap();
        for slot in scheduler.next(8, 3) {
            planner.request(PathRequest {
                agent: slot as u64,
                start: Cell { x: 0, y: 0 },
                end: Cell { x: 15, y: 15 },
                connectivity: Connectivity::Eight,
            });
        }
        planner.step(&grid, &mut budget);
        if let Some((_, Ok(path))) = planner.result() {
            for (out, c) in points.iter_mut().zip(path.iter().rev()) {
                *out = Vec3::new(c.x as f32, 0., c.y as f32);
            }
            route.set(&points[..path.len()]).unwrap();
            follower.reset();
            cursor.reset();
        }
        if planner.result().is_some() {
            planner.acknowledge();
        }
        follower.target(&route, Vec3::ZERO, 0.1, 8).unwrap();
        cursor.advance(&route, 2., 1. / 60.).unwrap();
        assert!(memory.best(1).unwrap().visible);
        enemy
            .tick(
                EnemyInput {
                    position: Vec3::ZERO,
                    forward: Vec3::Z,
                    up: Vec3::Y,
                    patrol_goal: Vec3::X,
                    target: memory.best(1).copied(),
                    health_fraction: 1.,
                    weapon_ready: true,
                    alive: true,
                },
                1. / 60.,
            )
            .unwrap();
    }
    RECORD.with(|v| v.set(false));
    assert_eq!(CALLS.with(|v| v.get()), 0, "hot AI allocated");
}
