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
    let mut actions =
        ferravex_play::action::ActionTimeline::new(ferravex_play::action::ActionSpec {
            windup: 0.1,
            active: 0.1,
            recovery: 0.2,
            cancel_before: 0.05,
            buffer_seconds: 0.1,
        })
        .unwrap();
    let mut hit_window = ferravex_play::action::HitWindow::<4>::new().unwrap();
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
        for event in actions.step(1. / 60., tick % 20 == 0).unwrap().events() {
            if event.edge == ferravex_play::action::ActionEdge::Activated {
                hit_window.begin(event.serial);
                hit_window.accept(event.serial, 1).unwrap();
            }
        }
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
#[test]
fn new_quality_systems_reuse_all_hot_storage() {
    use ferravex_math::{Vec2, Vec3};
    use ferravex_play::{
        ai::*, animation::*, audio::*, camera::*, environment::*, input::touch::*, navigation::*,
        objective::*, performance::*, racing::*, vehicle::*,
    };
    let mut squad = SquadBoard::new([Vec3::X, Vec3::Z]).unwrap();
    let mut director = ferravex_play::encounter::EncounterDirector::new(
        ferravex_play::encounter::EncounterConfig {
            credits: 1000,
            max_alive: 10,
            interval: 0.1,
            first_delay: 0.,
            recovery_seconds: 1.,
        },
    )
    .unwrap();
    let mesh = NavMesh::welded(vec![
        NavTriangle([Vec3::ZERO, Vec3::X, Vec3::Z]),
        NavTriangle([Vec3::X, Vec3::X + Vec3::Z, Vec3::Z]),
    ])
    .unwrap();
    let mut search = mesh.search(Vec3::ZERO, Vec3::X + Vec3::Z).unwrap();
    let mut driver = RacingDriver::new(1);
    let path = [Vec2::ZERO, Vec2::X * 20., Vec2::splat(20.), Vec2::Y * 20.];
    let mut composer = CameraComposer::<0>::new([], 0.2, 2., 0.1).unwrap();
    let camera = Input3 {
        target: Target3::at(Vec3::ZERO),
        mode: Mode3::Follow {
            eye_offset: Vec3::Y * 5.,
            look_offset: Vec3::ZERO,
        },
        fov_y: 1.,
    };
    let mut foot = FootPlant::new(0.3, 0.1).unwrap();
    let mut music = MusicConductor::new(120).unwrap();
    let acoustic = AcousticScene::<0>::new([]).unwrap();
    let layout = TouchLayoutSettings {
        mirrored: true,
        ..Default::default()
    };
    let control = Control {
        region: Region::Circle {
            center: Vec2::splat(50.),
            radius: 24.,
        },
        output: Output::Button(0),
    };
    let base = ObjectiveGraph::new(
        1,
        [ObjectiveSpec {
            id: 1,
            all: 0,
            any: 0,
            exclusive_group: 0,
            kind: ObjectiveKind::Event { id: 1, count: 2 },
        }],
    )
    .unwrap();
    let mut budget = RenderBudget::default();
    budget.set_enabled(true);
    let mut trace = PlaytestTrace::<16>::new().unwrap();
    let mut plan = EnvironmentPlan::new(16).unwrap();
    plan.stairs(Vec3::ZERO, 2., 0.2, 0.5, 4, 0).unwrap();
    let mut flight = FlightState {
        yaw: 0.,
        pitch: 0.,
        bank: 0.,
        turn_rate: 0.,
        speed: 20.,
    };
    CALLS.with(|v| v.set(0));
    RECORD.with(|v| v.set(true));
    for frame in 0..1000 {
        squad.advance(0.01).unwrap();
        squad
            .report(Candidate {
                id: 1,
                position: Vec3::Z,
                groups: 1,
            })
            .unwrap();
        squad
            .reserve(
                1,
                Vec3::ZERO,
                Vec3::Z,
                &mut WorkBudget::new(4, 4, 0),
                &mut Unoccluded,
                |_| true,
            )
            .unwrap();
        director.advance(0.01, 0.2).unwrap();
        director.admit(1, 0);
        search.step(&mesh, &mut WorkBudget::new(0, 0, 3)).unwrap();
        driver
            .step(
                &path,
                DriverSettings::default(),
                DriverInput {
                    position: Vec2::ZERO,
                    forward: Vec2::X,
                    velocity: Vec2::X * 10.,
                    rivals: &[],
                },
                0.01,
            )
            .unwrap();
        flight
            .step(FlightHandling::default(), 0.1, 0.1, 24., 0.01)
            .unwrap();
        composer.update(camera, Vec3::ZERO, 0.01).unwrap();
        let target = foot
            .update(
                Vec3::ZERO,
                Some(FootContact {
                    position: Vec3::Y * 0.1,
                    normal: Vec3::Y,
                    support: 1,
                    support_delta: Vec3::ZERO,
                }),
                true,
                0.01,
            )
            .unwrap();
        TwoBone {
            upper: 0.5,
            lower: 0.5,
        }
        .solve(Vec3::Y, target.position, Vec3::Z)
        .unwrap();
        music.update(0.01, 0.5).unwrap();
        acoustic.sample(Vec3::ZERO, false).unwrap();
        layout.apply(control, 0, Vec2::new(320., 200.)).unwrap();
        let mut graph = base.clone();
        graph.event(1, 1, 1).unwrap();
        graph.advance(0.01).unwrap();
        graph.restore(graph.snapshot()).unwrap();
        let d = budget
            .observe(BudgetSample {
                frame_ms: 30.,
                cpu_ms: 2.,
                gpu_ms: Some(24.),
                paused: false,
            })
            .unwrap();
        trace
            .record(PlaytestEvent {
                frame,
                tick: Some(frame),
                replay: Some(1),
                kind: DiagnosticKind::Fidelity,
                value: d.scale,
                frame_ms: 30.,
                cpu_ms: 2.,
                gpu_ms: Some(24.),
            })
            .unwrap();
        for piece in plan.pieces() {
            std::hint::black_box(piece.bounds());
        }
    }
    RECORD.with(|v| v.set(false));
    assert_eq!(CALLS.with(|v| v.get()), 0);
}
