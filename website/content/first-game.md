# Your first simulation

Keep simulation independent of rendering. Ferravex applications supply elapsed time explicitly; fixed updates receive the session's fixed `dt`. This complete headless program drops a circle onto a static platform, checks that it settles, and exercises pause and restart without a window or wall clock.

## Create a world and a checked session

Use the local `ferravex` dependency from [getting started](getting-started.md). Save the following complete program as your project's `src/main.rs`. It uses the existing translational solver and checked session configuration; no renderer initialization is needed.

```rust,check,crate-type=bin
use ferravex::core::session::{GameSession, SessionConfig, SessionState};
use ferravex::dynamics::{BodyDesc, PhysicsWorld, Shape};
use ferravex::World;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = SessionConfig::try_new(120.0, 8, false)?;
    assert!(SessionConfig::try_new(f64::NAN, 8, false).is_err());
    let mut session = GameSession::from_config(config);
    assert_eq!(session.state(), SessionState::Ready);
    assert_eq!(session.advance(10.0).steps, 0);

    let mut entities = World::new();
    let mut physics = PhysicsWorld::<2>::new([0.0, -9.81]);
    physics.insert(BodyDesc::fixed(
        Shape::cuboid([5.0, 0.5]), [0.0, -0.5],
    ))?;
    let ball = physics.insert(BodyDesc::dynamic(
        Shape::ball(0.5), [0.0, 3.0], 1.0,
    ))?;
    entities.spawn((ball,));

    assert!(session.start());
    for _ in 0..240 {
        let frame = session.advance(1.0 / 120.0);
        for _ in 0..frame.steps {
            physics.step(frame.dt)?;
        }
    }
    assert_eq!(session.tick(), 240);
    let settled = physics.body(ball).expect("ball is still present").position();
    assert!((settled[1] - 0.5).abs() < 0.02);

    assert!(session.set_paused(true));
    assert_eq!(session.advance(30.0).steps, 0);
    assert_eq!(session.tick(), 240);
    assert_eq!(physics.body(ball).unwrap().position(), settled);

    // Restart resets session timing, not the caller-owned entities or physics.
    session.restart();
    assert_eq!(session.state(), SessionState::Playing);
    assert_eq!(session.tick(), 0);
    assert_eq!(physics.body(ball).unwrap().position(), settled);
    Ok(())
}
```

For a project with its lockfile already in place, run:

```sh
cargo run --offline --locked --jobs 2
```

The scaffold includes a seeded lockfile. For a manually created project, first resolve and review its local dependency graph with `cargo generate-lockfile --offline` from that project directory. Successful execution returns without output; the assertions check the scenario. This example establishes headless behavior, not a rendering, device or cross-platform determinism result.

## Separate host time from rules

In an interactive host, sample input once per render frame, call `session.advance(elapsed_seconds)`, execute the returned fixed updates, then render once. Consume gameplay edges inside actual ticks so a tap survives a zero-tick frame and is not repeated across catch-up ticks. The [input guide](../../docs/INPUT.md) explains the existing `PlayerInput` and `ActionMap` APIs.

`FrameStep.steps` is an issued budget. If a game reaches a terminal state during one update, stop executing the rest of that batch. The session clock has already accounted for the issued ticks; keep a separate executed-update count when needed. `ferravex::app::App::frame_report` exposes actual execution counts for the application lifecycle adapter.

Track previous/current simulation poses and interpolate with `FrameStep.alpha`. Inspect `FrameStep.dropped_seconds` when a frame exceeds the catch-up limit. Reset both poses, pending input, entities and effects when restarting a real game: the example deliberately shows that `session.restart()` alone leaves the physics state intact. Local menus may suppress player commands, but must not freeze authoritative network or replay progression.

## Add a renderer or rotating physics

The [facade quickstart](../../docs/QUICKSTART.md) uses the existing `ferravex::render::game!` host and qualified drawing APIs. For standalone projects, the [template guide](../../docs/JOURNEY_200_TEMPLATES.md) covers the generated 2D/3D game paths. [Orbit Breaker](../../examples/owned-orbit/README.md) demonstrates the software framebuffer host; [Courtyard](../../examples/owned-lab/README.md) demonstrates its separate instanced WebGL2 host.

`ferravex::dynamics` is the translational solver used above: circles/spheres and axis-aligned boxes, without rigid-body rotation. The separate [rotational physics crate](../../crates/ferravex-physics/README.md), also available through `ferravex::physics`, already implements rotating rigid bodies, joints and compound bodies in feature-selected `d2`/`d3` APIs. Those body/world types are not interchangeable with `PhysicsWorld` above. Choose the implementation matching your game; no new solver is needed merely to obtain rotation.
