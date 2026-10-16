//! A headless composition of the engine's new synchronization and storage APIs.
use ferravex_engine::{
    assets::Bundle,
    bitset::BitSet,
    commands::CommandBuffer,
    diagnostics::{Diagnostics, Severity},
    ecs::World,
    events::{EventQueue, Overflow},
    frame_events::FrameEvents,
    net::InputFrame,
    pool::Pool,
    random::Random,
    replay::{Frame, Replay},
    resources::Resources,
    scheduler::TickScheduler,
    shuffle::ShuffleBag,
    smoothing::ExponentialSmoother,
    snapshots::SnapshotRing,
    spatial::SpatialGrid,
    state::StateMachine,
    system_order::SystemOrder,
    tags::TagSet,
    time::SimulationTime,
    weighted::WeightedTable,
    work_budget::WorkBudget,
};
use std::time::Duration;
struct Position(f64);
struct Velocity(f64);

fn main() {
    let mut assets = Bundle::default();
    assets.insert("config/weights", &[0, 1, 3]).unwrap();
    let mut retained = Pool::new(1);
    let asset_handle = retained
        .insert(assets.get("config/weights").unwrap())
        .unwrap();
    let table = WeightedTable::new(&[0, 1, 3]).unwrap();
    let mut random = Random::new(42).fork(1);
    let variants = ShuffleBag::new(3, 42).unwrap();
    let mut tasks = TickScheduler::new(3);
    for (index, variant) in variants.take(3).enumerate() {
        tasks.schedule(index as u64 * 2, variant).unwrap();
    }
    let mut order = SystemOrder::default();
    for name in ["spawn", "simulate", "publish"] {
        order.add(name).unwrap();
    }
    order.before("spawn", "simulate").unwrap();
    order.before("simulate", "publish").unwrap();
    assert_eq!(order.resolve().unwrap(), ["spawn", "simulate", "publish"]);
    let mut lifecycle = StateMachine::new("loading");
    lifecycle.allow("loading", "running");
    lifecycle.request("running").unwrap();
    assert_eq!(lifecycle.current(), &"loading");
    lifecycle.commit().unwrap();
    let mut tags = TagSet::default();
    tags.insert("simulation.movement").unwrap();
    assert!(tags.matches("simulation"));

    let mut world = World::new();
    let mut commands = CommandBuffer::new(3);
    let mut events = FrameEvents::new(1);
    let mut recent = EventQueue::new(3, Overflow::EvictOldest);
    let mut history = SnapshotRing::new(3);
    let mut replay = Replay::default();
    let mut dirty = BitSet::new(3);
    let mut grid = SpatialGrid::new(1., 3).unwrap();
    let mut clock = SimulationTime::default();
    let mut smoother = ExponentialSmoother::new(0., 0.1).unwrap();
    let mut diagnostics = Diagnostics::new(8, Severity::Info);
    let mut resources = Resources::default();
    resources.insert(WorkBudget::new(3)).unwrap();
    for tick in 0..8 {
        events.advance();
        let step = clock.advance(Duration::from_millis(16));
        resources.get_mut::<WorkBudget>().unwrap().reset();
        for variant in tasks.drain_ready(tick, 3) {
            assert!(resources.get_mut::<WorkBudget>().unwrap().try_consume(1));
            let speed = table.sample(&mut random) as f64;
            assert!(
                commands
                    .push(move |w: &mut World| {
                        w.spawn((Position(variant as f64), Velocity(speed)));
                    })
                    .is_ok()
            );
        }
        commands.apply(&mut world, 3);
        dirty.clear();
        for (index, (mut position, velocity)) in world
            .query::<(&mut Position, &Velocity)>()
            .into_iter()
            .enumerate()
        {
            position.0 += velocity.0 * step.scaled_delta.as_secs_f64();
            assert!(dirty.set(index, true));
            grid.insert(index as u64, [position.0, 0.]).unwrap();
        }
        let sum = world
            .query::<&Position>()
            .into_iter()
            .map(|p| p.0)
            .sum::<f64>();
        smoother.update(sum, step.scaled_delta.as_secs_f64());
        history.push(tick, sum).unwrap();
        assert!(replay.record(Frame {
            tick,
            input: InputFrame::default(),
            state_hash: sum.to_bits()
        }));
        events.send(world.len()).unwrap();
        recent.send(world.len()).unwrap();
        diagnostics
            .record(
                tick,
                Severity::Info,
                "simulation",
                &format!("{} entities", world.len()),
            )
            .unwrap();
        assert_eq!(dirty.count(), world.len());
        assert_eq!(grid.query([0., 0.], [10., 1.]).unwrap().len(), world.len());
    }
    events.advance();
    assert_eq!(events.read(), [3]);
    assert_eq!(history.len(), 3);
    assert!(history.get(0).is_none());
    assert_eq!(recent.dropped(), 5);
    assert_eq!(
        replay.first_divergence(&Replay::decode(&replay.encode()).unwrap()),
        None
    );
    assert!(smoother.value().is_finite());
    assert_eq!(&**retained.get(asset_handle).unwrap(), &[0, 1, 3]);
    retained.remove(asset_handle).unwrap();
    assert!(retained.get(asset_handle).is_none());
    println!(
        "8 ticks, {} ECS entities, {} snapshots, {} diagnostic records; all pipeline assertions passed",
        world.len(),
        history.len(),
        diagnostics.iter().count()
    );
}
