# Engine boundaries

The default `ferravex` facade exports original implementations. All new engine code
uses Rust std and path dependencies on this repository. Run
`python3 scripts/audit-owned.py` to check the actual resolved dependency graph.

```text
ferravex ── ferravex-core ── fixed clock, ferravex-ecs, ferravex-math
  └──── ferravex-engine
          ├── ferravex-math       scalar vectors, quaternions, matrices
          ├── ferravex-ecs        generational sparse component storage
          ├── ferravex-dynamics   original 2D/3D translational physics + queries
          ├── ferravex-graphics   mesh cache, culling, AO, software raster, OBJ/cooked mesh
          ├── ferravex-play       rounds, animation, skeletons, scenes, UI, navigation, VFX
          └── ferravex-net        codec, reliability, deltas, clocks/history, native UDP

ferravex-fps ── ferravex-math + ferravex-net   authority, shared motor, prediction, rewind
owned-lab ── engine + fps          native headless / WebGL2 browser host
owned-orbit ── engine              native headless / Canvas2D browser host

Legacy path (still used by thirteen earlier examples):
ferravex-render ── Macroquad/Miniquad
ferravex-physics ── Rapier2d/Rapier3d
```

Simulation consumes explicit fixed timesteps; devices and transports stay outside
game rules. Work queues, packet sizes, histories, physics pair counts and pools
have explicit bounds. Trusted convenience APIs may assert their configuration;
fallible methods and untrusted decoders validate before changing live state.

The new software renderer is a CPU fallback/reference. The browser GPU host owns
persistent buffers and per-frame instance uploads, uses the Rust frustum tests,
and submits all shadow casters separately from visible scene objects. Shader
reload installs a newly linked program only after success. Native GPU/window
backends are not implemented yet.

Owned physics supports circle/sphere and axis-aligned box translation, forces,
impulses, friction/restitution, sensors, layers, rays, static-terrain ball CCD and
a sphere slide/step/snap motor. The FPS uses its own prediction-safe static AABB
motor. Neither provides Rapier's rotational/joint/mesh capabilities; existing
Rapier games remain until their actual needs are supported.

The native protocol v2 validates inputs and sessions, bounds work, predicts and
reconciles movement and replicates reliable ordered gameplay events with retry
and ACK. The server coalesces redundant commands while preserving actions. The
rewind API can resolve shots against bounded historical poses while retaining
current wall occlusion and player lifecycle checks. It is opt-in; native wire
shot timestamps and automatic clock integration remain pending. Full snapshots
are still sent; delta and quantization utilities have not yet been wired in.

Native UDP is a trusted-development transport without encryption/authentication.
The owned browser example uses standard WebRTC DTLS/SCTP, manual offer/answer
pairing, one authoritative host and one predicting guest. There is no configured
NAT relay or identity service. Tokens must be exchanged over a trusted channel.
The two transport paths do not currently interoperate.

The [twenty-feature implementation map](FROM_SCRATCH.md) records each working
system and remaining integration. Each crate README describes its exact limits.
Cross-compilation/CI is not a substitute for runtime and thermal testing on
Windows, macOS, iOS or low-end mobile hardware.
