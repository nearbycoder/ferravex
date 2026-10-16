# Original engine implementation

Every package in the Cargo workspace uses **zero external Cargo packages**, including all optional features and platform targets. The audit checks the complete resolved graph, not a selected subset. See the [dependency policy](../DEPENDENCIES.md).

The local catalog contains fifteen examples: thirteen original games/labs recovered from this repository and ported to first-party APIs, plus Courtyard and Orbit Breaker. Browser hosts use standard APIs directly; Rust std and browser/system APIs are the platform boundary. The [iteration series](ITERATION_PLAN.md) records merged improvements and current local validation; the deployed site may lag local work.

## Run the examples locally

```sh
python3 scripts/audit-owned.py
cargo test --workspace --all-features --offline --locked
cargo run --release -p owned-lab --bin owned-lab -- --frames 640 --output /tmp/courtyard.png
cargo run --release -p owned-orbit -- --autoplay --screenshot /tmp/orbit.png
python3 scripts/build-web.py release
python3 scripts/serve-web.py --port 8000
```

Open the local root page for all fifteen examples. `/owned/` is Courtyard and `/owned-orbit/` is Orbit Breaker. The thirteen restored games/labs use the shared direct-WebGL2 host and original `ferravex-render` runtime: Rust records immediate primitives/text and retained mesh instances. Retained meshes use GPU transforms/fragment lighting/fog; immediate primitives retain the CPU path. The retained path shares directional shadows, bounded point lights and baked vertex AO; immediate primitives keep their CPU path. Courtyard retains persistent WebGL2 mesh buffers, instancing, a shadow map and Rust visibility tests. Orbit writes an original RGBA framebuffer to Canvas2D. No external runtime libraries, fonts or CDN loaders are used.

Courtyard: WASD, mouse aim, click fire, Space jump, R reload, Escape menu. The
inspector edits positions and collision geometry, saves edits locally, exports
input recordings and reloads GLSL after validation. Ground height is fixed by the
FPS motor. Detailed props are decorative; only the authored collision solids
block players. Browser context restoration rebuilds GPU resources.

The restored examples have an original Linux X11/GLX window/GPU host. Run, for example, `cargo run --release -p neon-swarm` or `cargo run --release -p marble-run` with an X display and compatible OpenGL driver. Courtyard and Orbit retain their native headless PNG paths. Original Windows/macOS/iOS window/input/GPU hosts and native audio sinks are now in source. Linux/web runtime checks pass and Windows/macOS Rust cross-checks pass; iOS remains source-only with SDK/device checks excluded. See [Platforms](PLATFORMS.md) for exact status; cross-target compilation does not establish interactive device support.

## Twenty feature tracks

Each row describes working code and the material work still required. These
are initial implementations, not a claim that twenty production systems are
finished.

| # | Feature | Working implementation | Remaining work |
|---|---|---|---|
| 1 | Profiling and budgets | Fixed-size CPU samples, budget flags, p99; browser CPU frame and asynchronous GPU timer measurements; binary size/audit scripts | Trace timelines and device budgets |
| 2 | Retained rendering | Shared native/WebGL2 retained buffers, opaque-pass instancing, conservative frustum/distance tests and authored hysteretic LOD; Courtyard static buffers/instancing and transformed bounds/frustum culling; headless clipped depth rasterizer | Windows/Apple runtime validation, Metal and GPU-driven submission |
| 3 | Character movement | Shared upright capsule and planar box motors, continuous sweeps, slopes/steps, moving supports, jump grace/buffer, prediction snapshots; adopted by Strike, Crystal Cavern and Emberline | Dynamic-body pushing, mesh terrain and multiplayer platform pose history |
| 4 | Input actions | Bounded remappable action map, deadzones, buffered edges; browser keyboard/mouse/gamepad/touch adapters; native host events, shared rebinding UI and persistent bindings | Windows/Apple controller discovery and physical-device testing |
| 5 | Spatial audio | Strict PCM16 WAV decoder, bounded resampling stereo mixer, distance attenuation/panning; browser AudioWorklet and native ALSA/WinMM/AudioQueue sinks, procedural surface/weapon cues | Occlusion, streaming/compressed audio and Windows/Apple sink verification |
| 6 | Reliable events | Actual v4 bidirectional session-bound events/commands, ACK/retry/order/dedup, bounded queues and slow-peer disconnect; typed gameplay/tactical state delivery | Congestion-aware scheduling and long-running Internet tests |
| 7 | Lag compensation | Bounded authority history, correlated wire clock probes, rendered-tick shot stamps, historical-hit damage with current-wall/lifecycle validation and invalid-claim disarming | Sub-tick/dynamic-world rewind, stronger anti-cheat policy |
| 8 | Compact replication | Sparse/full snapshot deltas with exact ACKed baselines and missing-base recovery; standalone centimeter quantizer | Quantized wire snapshots, adaptive interest/bandwidth scheduling |
| 9 | Secure multiplayer | Browser WebRTC DTLS/SCTP adapter; host authority, guest prediction/reconciliation and input redundancy | Native encrypted transport, service identity, accounts, NAT relay/discovery and session resumption |
| 10 | Tactical rounds | Playable native/browser Strike buy/live/plant/defuse/round/match state, teams, economy, armor, kits, distinct weapon timings, interruption/range validation and disconnect forfeits | Spectators, larger content/weapon inventory, Internet services |
| 11 | Asset cooking | Bounded OBJ importer, cooked binary mesh codec, PCM WAV validation, deterministic compressed bundles, `ferravex-cook` tool | glTF, image/font pipelines, richer compression and dependency tracking |
| 12 | Scenes and prefabs | Generational scene graph, cycle checks, hierarchical transforms, clone-subtree prefabs, strict binary scene codec | Asset references, scene streaming and game-specific schema evolution |
| 13 | Editor | Courtyard browser scene inspector with live position editing, save/load and reset | Viewport picking/gizmos, undo/redo and full prefab/property editing |
| 14 | Hot reload | Transactional live asset bundles; shared material revision/parameter API, native bounded file polling and browser last-valid vertex/fragment hook reload/editor; Courtyard last-valid GLSL replacement with visible errors | OS event-based filesystem watching and full content dependency invalidation |
| 15 | Animation | Scalar/vector/quaternion tracks, sprite clips, skeleton world poses and inverse-bind skin palettes, GPU skinning, bounded blending/state transitions and timed events | Richer animation authoring and importer integration |
| 16 | Low-poly lighting | Shared retained directional shadows, point lights, BVH vertex AO and Low/Medium/High tiers; Courtyard retains its separate renderer | Baked lightmaps/probes, cascades and point-light shadows |
| 17 | Navigation and AI | Weighted deterministic A*, no corner cutting, steering, bounded LOS/FOV perception and decisions adopted by bots | Navmesh baking and richer visual behavior authoring |
| 18 | VFX | Bounded particles and curves, instanced billboards, ribbons/beams and shared combat effects | GPU simulation and richer effect authoring |
| 19 | Responsive UI | Shared HUD/menu commands, anchors, clipping/hit tests, focus/capture and bitmap glyphs integrated in games/settings | General text shaping/font loading and richer accessibility |
| 20 | Replay and regressions | Bounded authority checkpoints/events/hashes, practice replay viewer, seeking/import/export, shared 2D/3D ghosts, corruption and network loss/reorder regressions | Tactical/network match recordings, device farm and long soak tests |

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

Native UDP protocol v3 remains a trusted-development transport without encryption.
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

See each crate README for numerical, allocation and feature limits. `ferravex-dynamics` remains a translational solver for circles/spheres and axis-aligned boxes. The restored physics games use separate original `ferravex-physics`: dynamic primitives, rotated compound parts with full mass tensors, fixed/hinge/distance/spring joints, angular impulses, capsule rails and cylinders against balls, persistent contacts/islands, events/layers, rays and bounded sphere CCD. Capsule/cylinder contacts with boxes, mesh collisions and swept box TOI remain unsupported; consult its README for numerical and CCD limits. It is not a claim of production-engine feature parity. Run the recovered game tests and interactive checks before relying on prior results.
