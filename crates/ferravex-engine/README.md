# ferravex-engine

The original engine system facade. Depends exclusively on Ferravex's path crates and
Rust std; check the resolved graph with `python3 scripts/audit-owned.py`.

Reexports `math`, `ecs`, `dynamics`, `graphics`, `play`, and `net`. Additional
owned modules provide buffered input actions, profiling, bundles/hot reload,
PCM WAV audio/mixing, animation playback/state graphs/GPU skinning, reusable effects, AI perception/decisions/navigation, and input replay files.
See [animation](../../docs/ANIMATION.md) for clip banks, pose layers and retained skins,
[effects](../../docs/EFFECTS.md) for bounded particles, trails/ribbons and GPU billboards,
and [AI](../../docs/AI.md) for sensing, work budgets and current-pose rigid occlusion.

```rust
use ferravex_engine::{dynamics::{PhysicsWorld,BodyDesc,Shape},input::{ActionMap,Binding,InputCode}};
let mut world=PhysicsWorld::<2>::new([0.,-9.81]);
let ball=world.insert(BodyDesc::dynamic(Shape::ball(0.5),[0.,3.],1.)).unwrap();
world.step(1./120.).unwrap();
assert!(world.body(ball).unwrap().position()[1]<3.);
let mut actions=ActionMap::default();
let jump=actions.action(vec![Binding{code:InputCode::Key(32),scale:1.,deadzone:0.}]);
actions.feed(InputCode::Key(32),1.);
actions.feed(InputCode::Key(32),0.);
assert!(actions.consume_tick(jump).pressed);
```

Configuration limits: bundles up to 64 MiB and 4096 files; decoded WAV up to 64 MiB and 10 minutes;
128 mixer voices; 256 input actions, 64 bindings per action and 1024 active codes;
128 profiler counters with 120 samples each; one million replay frames. Typed/fallible methods
validate untrusted data. Trusted convenience methods can assert invalid IDs or
configuration. Retaining `Arc` asset generations keeps those allocations alive.
Audio render does not allocate new buffers, but retiring a final clip reference
may free memory. It is not yet a fully realtime-safe device callback.

`Profiler::measure` uses native `Instant`. In a browser use
`measure_with_clock` and supply an external monotonic clock. Duration counters
are CPU microseconds; the browser WebGL host has separate asynchronous GPU
queries. A GPU timestamp is never inferred from CPU time.

Checksummed bundle/replay files detect accidental corruption and are not secure
containers. Native sockets remain a development transport. The browser host
uses browser DTLS; there is no project-owned cryptographic implementation.

See [the twenty-feature map](../../docs/FROM_SCRATCH.md) for exact integration
status and platform limits, [Courtyard](../../examples/owned-lab) and
[Orbit Breaker](../../examples/owned-orbit) for runnable examples. The original
physics/math/graphics/ECS/gameplay crates each document numerical and feature
limits separately.
