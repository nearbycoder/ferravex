# Your first simulation

Keep simulation independent of rendering. Ferravex applications supply time explicitly, so the same rules can run in a browser, a replay or a headless server.

## Create a world

This example drops a circle onto a static platform using only the original physics solver:

```rust
use ferravex::{FixedClock, World};
use ferravex::dynamics::{BodyDesc, PhysicsWorld, Shape};

fn main() {
    let mut entities = World::new();
    let mut physics = PhysicsWorld::<2>::new([0.0, -9.81]);
    physics.insert(BodyDesc::fixed(
        Shape::cuboid([5.0, 0.5]), [0.0, -0.5],
    )).unwrap();
    let ball = physics.insert(BodyDesc::dynamic(
        Shape::ball(0.5), [0.0, 3.0], 1.0,
    )).unwrap();
    entities.spawn((ball,));

    let mut clock = FixedClock::new(120.0, 8);
    for _ in 0..240 {
        let frame = clock.advance(1.0 / 120.0);
        for _ in 0..frame.steps {
            physics.step(frame.dt).unwrap();
        }
    }
    let position = physics.body(ball).unwrap().position();
    assert!((position[1] - 0.5).abs() < 0.02);
}
```

## Separate the host

In an interactive host, call the fixed clock with real elapsed time. Read input once per frame, retain action edges until a simulation tick consumes them, update the world, then render.

The clock caps catch-up steps. Inspect dropped simulation time when diagnosing slow frames; increasing the catch-up budget without measuring can make a stall worse.

## Add a renderer

The graphics crate provides mesh primitives, reusable handles, visibility tests and a clipped software renderer. The Courtyard host adds a direct WebGL2 GPU backend with persistent buffers, instancing and shadows.

Start with [Orbit Breaker](/examples/owned-orbit/) for an original 2D framebuffer host or [Courtyard](/examples/owned-lab/) for an original 3D host. Both expose a small Wasm ABI and use browser standards directly.

## Understand physics scope

The original solver supports circles/spheres and axis-aligned boxes. It does not yet simulate rigid-body rotation, joints or mesh terrain. Games requiring those features need further first-party solver implementation.
