# Architecture

Ferravex separates simulation, data and platform presentation. Every package depends only on other workspace packages and Rust std. The [dependency policy](../DEPENDENCIES.md) applies to all features and targets. The recovery described here is local; publication and full catalog validation are not implied.

## Engine modules

| Crate | Responsibility |
| --- | --- |
| ferravex-math | Scalar vectors, matrices, quaternions, colors and bounds |
| ferravex-ecs | Generational entities and typed component stores |
| ferravex-core | Fixed clock, frame statistics, collision and entity helpers |
| ferravex-dynamics | Shared 2D/3D translational contacts, queries, CCD and character helpers |
| ferravex-physics | Original rotational rigid-body API used by the restored physics games |
| ferravex-graphics | Mesh generation/cooking, clipping, software rasterization and PNG export |
| ferravex-render | Immediate drawing, procedural meshes, CPU vertex lighting, input, frame scheduling and native/browser presentation ABI |
| ferravex-play | Scenes, animation, navigation, steering, particles and UI data |
| ferravex-net | Packet codecs, reliable events, sequence/queue helpers and native UDP |
| ferravex-engine | Facade over engine modules plus input, audio, assets, replay and profiling |
| ferravex-fps | Authoritative shooter simulation, prediction, interpolation and rewind |
| ferravex | Public convenience facade |
| ferravex-bench | Reproducible simulation benchmarks using the same first-party systems |

## Rendering and hosts

The thirteen restored games/labs use `ferravex-render`. Game code records triangles from 2D primitives, text glyphs and cached 3D meshes. Rust applies model/camera transforms and optional directional vertex lighting with distance fog, then batches clip-space positions and colors into a GPU stream. The browser host in `web/runtime.js` submits that stream directly through WebGL2. The Linux host uses original system X11/GLX bindings, an OpenGL shader and a persistent vertex buffer. It provides keyboard/mouse events, relative pointer capture and GPU screenshot readback; native audio, touch/gamepad and automatic desktop DPI scaling are not implemented. This immediate renderer has no shadow-map pass; low-poly shading and hand-authored geometry establish the look. Its text uses built-in bitmap glyphs rather than downloaded fonts or an external shaping library.

Courtyard retains a separate browser renderer using persistent meshes, instanced objects, Rust visibility tests and a directional shadow map. Browser input, audio and peer sessions use browser APIs. Its native host runs the same simulation and exports a software-rendered frame.

Orbit Breaker runs a 120 Hz arcade simulation and an original Rust RGBA rasterizer. The browser forwards input and presents the framebuffer with Canvas2D. Its native host runs autoplay and exports a PNG. All fifteen examples are listed in `examples/catalog.json`; the catalog drives local web staging and documentation.

## Physics

Two original solvers serve different examples. `ferravex-dynamics` handles circles/spheres and axis-aligned boxes, translational contact impulses, friction, restitution, sensors, filtered queries, bounded ball CCD and a sphere slide/step/snap helper. It has no angular dynamics and remains the solver used by the older headless examples.

`ferravex-physics` provides dynamic circles/spheres and oriented boxes with angular impulses, world inertia, clipped box contact manifolds, friction and restitution. Static/kinematic 2D capsule rails and 3D cylinders collide with balls. The API includes generational handles, removal, sensors, contact start/stop events, layers, current-pose rays and conservative sphere sweeps with at most eight substeps. Sweeps freeze target orientation within a substep and have a forty-iteration bound.

Dynamic capsules/cylinders, capsule/cylinder-versus-box contacts, joints, mesh collisions and swept box time-of-impact are unsupported. Consult the [crate README](../crates/ferravex-physics/README.md) for exact limits. This is an original bounded implementation, not production-engine feature parity. Recovery validation must exercise pinball flippers, rolling marbles, stacks and high-speed cannonballs in addition to compile checks.

## Netcode

The native server runs a 64 Hz authoritative simulation for up to sixteen players. It owns movement, wall-occluded hitscan, health, ammo, reloads and respawns. Clients predict local input, reconcile/replay unacknowledged inputs and interpolate remote players. Redundant inputs, bounded queues, session checks and reliable event acknowledgements constrain work and recover loss.

Native UDP is a trusted-development transport without account authentication or encryption. Historical rewind and delta codecs exist as separate APIs; full wire integration is incomplete. Courtyard's browser WebRTC adapter supports a host and one guest with explicit offer/answer pairing and browser-provided DTLS/SCTP. It has no hosted signaling or STUN/TURN relay. Strike's native UDP mode and Courtyard's browser session are separate transports.

## Platform boundary

Hosts own OS/browser events, GPU submission and the window lifecycle. Linux interactive and browser bindings are implemented. Native Windows, macOS and iOS window/GPU/input hosts and native audio sinks remain unimplemented. Portable simulation compilation is distinct from interactive device support; see [platform status](PLATFORMS.md).
