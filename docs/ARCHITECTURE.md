# Architecture

Ferravex separates simulation, data and platform presentation. Every package in the workspace depends only on other workspace packages and Rust std. The [dependency policy](../DEPENDENCIES.md) applies to all features and targets.

## Engine modules

| Crate | Responsibility |
| --- | --- |
| ferravex-math | Scalar vectors, matrices, quaternions, colors and bounds |
| ferravex-ecs | Generational entities and typed component stores |
| ferravex-core | Fixed clock, frame statistics and entity helpers |
| ferravex-dynamics | Shared 2D/3D translational contacts, queries, CCD and character helpers |
| ferravex-graphics | Mesh generation/cooking, clipping, software rasterization and PNG export |
| ferravex-play | Scenes, animation, navigation, steering, particles and UI data |
| ferravex-net | Packet codecs, reliable events, sequence/queue helpers and native UDP |
| ferravex-engine | Facade over engine modules plus input, audio, assets, replay and profiling |
| ferravex-fps | Authoritative shooter simulation, prediction, interpolation and rewind |
| ferravex | Public convenience facade |
| ferravex-bench | Reproducible simulation benchmarks using the same first-party systems |

## Hosts and examples

Courtyard owns the WebAssembly ABI and a browser host using WebGL2 directly. It submits persistent meshes and instanced objects, computes visibility in Rust, and uses a directional shadow map. Browser input, audio and peer sessions use browser APIs. The native host runs the same simulation and exports a software-rendered frame.

Orbit Breaker owns its 120 Hz arcade simulation and Rust RGBA rasterizer. The browser forwards input and presents the framebuffer with Canvas2D. Its native host runs autoplay and exports a PNG. Both games are listed in examples/catalog.json; no other game binaries are staged.

## Physics

The original solver supports circles/spheres and axis-aligned boxes, contact impulses, friction, restitution, sensors, filtered queries, bounded ball CCD and a sphere slide/step/snap helper. It does not provide angular dynamics, joints, arbitrary convex colliders or mesh terrain. Features are implemented explicitly in ferravex-dynamics; there is no external fallback.

## Netcode

The native server runs a 64 Hz authoritative simulation for up to sixteen players. It owns movement, wall-occluded hitscan, health, ammo, reloads and respawns. Clients predict local input, reconcile/replay unacknowledged inputs and interpolate remote players. Redundant inputs, bounded queues, session checks and reliable event acknowledgements constrain work and recover loss.

Native UDP is a trusted-development transport without account authentication or encryption. Historical rewind and delta codecs exist as separate APIs; full wire integration is incomplete. Courtyard's browser WebRTC adapter supports a host and one guest with explicit offer/answer pairing and browser-provided DTLS/SCTP. It has no hosted signaling or STUN/TURN relay.

## Platform boundary

Hosts own the frame loop and input/audio/window lifecycle. Browser presentation calls standard APIs through original JavaScript bindings. Rust game and engine code uses std and internal crates. Native window, GPU and audio FFI hosts remain to be implemented; headless native compilation is not a claim of interactive device support.
