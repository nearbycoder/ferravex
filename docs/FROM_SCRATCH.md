# Original engine implementation

The active `ferravex` facade, `ferravex-engine`, `ferravex-core`, `ferravex-fps`, `owned-lab`, and
`owned-orbit` dependency graphs contain **zero third-party Cargo packages**.
`python3 scripts/audit-owned.py` checks this using Cargo's resolved dependency
metadata. Rust's standard library, system services, and browser APIs are the
platform boundary. None of the replacement implementations is vendored code.

The earlier `ferravex-render`/Macroquad and `ferravex-physics`/Rapier adapters and their
thirteen examples remain in the workspace for regression testing and migration.
**The entire repository is not yet free of external dependencies.** Removing
those adapters before their games are ported would discard working functionality.
The default build now selects the owned reference example; legacy features on the
facade require explicit opt-in.

## Run the owned examples

```sh
python3 scripts/audit-owned.py
cargo test -p ferravex-engine -p ferravex-graphics -p ferravex-play -p ferravex-dynamics -p owned-lab -p owned-orbit
cargo run --release -p owned-lab --bin owned-lab -- --frames 640 --output /tmp/courtyard.png
cargo run --release -p owned-orbit -- --autoplay --screenshot /tmp/orbit.png
python3 scripts/build-owned.py
python3 scripts/serve-web.py --port 8000
```

Open `/owned/` for Courtyard or `/owned-orbit/` for Orbit Breaker. These pages
use original loaders; they do not load Macroquad, Miniquad, Wasm-bindgen, npm
packages, external assets, fonts, or CDNs. Courtyard uses persistent WebGL2 mesh
buffers, instanced draws, a shadow map and Rust visibility tests. Orbit writes an
owned RGBA framebuffer to the browser's Canvas2D. Both share their Rust gameplay
with native headless runs.

Courtyard: WASD, mouse aim, click fire, Space jump, R reload, Escape menu. The
inspector edits positions and collision geometry, saves edits locally, exports
input recordings and reloads GLSL after validation. Ground height is fixed by the
FPS motor. Detailed props are decorative; only the authored collision solids
block players. Browser context restoration rebuilds GPU resources.

The native owned example currently exports screenshots and runs simulations; a
native window/GPU/audio host still needs implementation. Cross-platform Rust
code and CI compile checks do not establish runtime support on an iPhone or a
Windows/Mac GPU.

## Twenty feature tracks

Each row describes working code and the material work still required. These
are initial implementations, not a claim that twenty production systems are
finished.

| # | Feature | Working implementation | Remaining work |
|---|---|---|---|
| 1 | Profiling and budgets | Fixed-size CPU samples, budget flags, p99; browser CPU frame and asynchronous GPU timer measurements; binary size/audit scripts | Allocator instrumentation, trace timelines, device budgets |
| 2 | Retained rendering | CPU mesh handles/generations; WebGL static buffers, instancing, transformed bounds/frustum culling; headless clipped depth rasterizer | General LOD policy, native GPU hosts, GPU-driven submission |
| 3 | Character movement | Owned sphere slide/step/snap helper; existing shared FPS AABB motor and prediction | Unified capsule motor, slopes, moving platforms, body pushing |
| 4 | Input actions | Bounded remappable action map, deadzones, buffered edges; browser keyboard/mouse/gamepad/touch hosts | Interactive rebinding UI and physical-device testing |
| 5 | Spatial audio | Strict PCM16 WAV decoder, bounded resampling stereo mixer, distance attenuation/panning; browser AudioWorklet output | Occlusion, streaming/compressed audio, native sinks and authored footsteps |
| 6 | Reliable events | Actual UDP v2 session-bound event/ACK codec, retry/order/dedup, bounded queues and slow-peer disconnect; Strike consumes replicated events | Congestion-aware scheduling and long-running Internet tests |
| 7 | Lag compensation | Bounded authority history, clock estimator, opt-in historical-hit damage with current-wall occlusion and lifecycle checks | Validated shot timestamps in the UDP wire format and automatic clock integration |
| 8 | Compact replication | Bounded sparse/full snapshot byte deltas with exact baselines and centimeter quantization | Baseline ACK integration, quantized wire snapshots, adaptive interest/bandwidth scheduling |
| 9 | Secure multiplayer | Browser WebRTC DTLS/SCTP adapter; host authority, guest prediction/reconciliation and input redundancy | Native encrypted transport, service identity, accounts, NAT relay/discovery and session resumption |
| 10 | Tactical rounds | Authoritative buy/live/plant/defuse/round/match state, teams, economy, armor, kits and interruption/range validation | Integrating these rules into the playable shooter, full spectator/buy UI |
| 11 | Asset cooking | Bounded OBJ importer, cooked binary mesh codec, PCM WAV validation, deterministic compressed bundles, `ferravex-cook` tool | glTF, image/font pipelines, richer compression and dependency tracking |
| 12 | Scenes and prefabs | Generational scene graph, cycle checks, hierarchical transforms, clone-subtree prefabs, strict binary scene codec | Asset references, scene streaming and game-specific schema evolution |
| 13 | Editor | Browser scene inspector with live position editing, save/load and reset | Viewport picking/gizmos, undo/redo and full prefab/property editing |
| 14 | Hot reload | Transactional live asset bundles, last-valid GLSL program replacement with visible errors | Native filesystem watchers, full content dependency invalidation |
| 15 | Animation | Scalar/vector/quaternion tracks, sprite clips, skeleton world poses and inverse-bind skin palettes | GPU skinning, blend/state graphs, event tracks and importer integration |
| 16 | Low-poly lighting | Flat normals, vertex AO baking, directional shading, browser PCF shadows and atmospheric fog | Baked lightmaps/probes, cascades and platform quality tiers |
| 17 | Navigation and AI | Weighted deterministic A*, no corner cutting, arrival/separation steering | Navmesh baking, perception, behavior/state tooling and bot integration |
| 18 | VFX | Bounded particle pool, constant-acceleration integration, lifetime/color/size curves; shot impacts in Courtyard | GPU simulation, trails and effect authoring |
| 19 | Responsive UI | Anchor layout, clipping/hit tests, focus/capture, browser safe areas and platform text | Native text shaping/rasterization, retained UI renderer and richer accessibility |
| 20 | Replay and regressions | Bounded checksummed input files; scene save/load; corruption, gameplay, dropped-input, event-loss, rewind and legacy regressions | Full server-event replay, replay viewer, device farm and long soak tests |

## Browser multiplayer

Open Courtyard in two browsers/tabs on machines with a direct network route.
Open Scene tools → Two-player LAN session. The host generates an offer token;
the guest pastes it and selects Join offer. The guest sends its answer back;
the host pastes it and selects Accept answer. Both then enter training.

The browser negotiates encrypted DTLS/SCTP using SDP certificate fingerprints.
Exchange the tokens over a trusted channel: this prototype provides no account
identity or matchmaking. It configures no external STUN/TURN service, so
connections through restrictive NAT/firewalls can fail. Session tokens contain
connection candidates and should be treated as private connection information.
The host owns movement, health, shots and bots. The guest predicts its own
movement and reconciles against host snapshots. One guest is supported. The
browser transport currently sends full snapshots at up to 20 Hz; native reliable
UDP events and historical rewind are separate capabilities.

Native UDP protocol v2 remains a trusted-development transport without encryption.
No custom cryptographic primitive was invented or substituted for a secure native
transport. See the FPS/net READMEs for exact APIs and validation limits.

## Offline cooking

```sh
cargo run --release -p owned-lab --bin ferravex-cook -- /tmp/assets.ferravexpak model.obj shot.wav config.bin
```

OBJ becomes validated `GMSH0001` mesh data; WAV is validated PCM; other files are
bounded opaque bytes. The packer rejects duplicate basenames and unsafe asset
names. Bundles use sorted names, a versioned envelope, per-file and whole-file
checksums and an original RLE codec only where it reduces size. Checksums detect
corruption and are not authentication. The packer does not execute asset code.

See each new crate README for numerical, allocation and feature limits. In
particular, the owned physics solver is translational: axis-aligned boxes and
circles/spheres, without angular rigid bodies, oriented boxes, mesh shapes,
joints or sleeping. It is not yet a replacement for every Rapier example.
