# Particles, trails and effects

The original `ferravex-play::particles` module supplies bounded simulation, seeded
emitters, trails, ribbons and beams. `ferravex-render::effects` draws them on native
GL and browser WebGL2; `ferravex_engine::effects` exposes both through the engine
facade. No image, texture, sample asset, external crate or vendor runtime is added.

## Particle motion and emission

`ParticlePool` reuses storage after construction, recycles expired entries and
reports direct capacity drops and discarded numeric overflow. Its original
`spawn(ParticleDesc)` API still works. `spawn_with` adds initial rotation, angular
velocity and nonnegative linear drag; constant-force motion uses an analytic
solution from birth state instead of frame-dependent damping. Positions, velocities,
color, size, rotation and normalized age are available through `iter()`.

`Emitter` combines a validated template with local point/box/sphere/disc emission,
optional direction cone, velocity/lifetime/size/rotation variation, inherited
motion and an emission rate. A transform rotates the spawn shape and initial
velocity; acceleration stays in world space so rotating a nozzle does not rotate
gravity. Call `pool.update(dt)` first, then `emitter.update(dt, transform, pool)`.
Birth times interpolate the moving emitter and newborn particles catch up within
the current frame. `burst` emits at one birth time without changing rate phase.
`teleport` resets positional interpolation and rate phase; `reset(seed)` also
resets the independent request sequence/counters.

```rust
use ferravex_play::particles::*;
let mut particles = ParticlePool::new(256);
let mut impact = Emitter::new(presets::sparks([1.0, 0.6, 0.2, 0.9]), 42).unwrap();
particles.update(1.0 / 60.0).unwrap();
let report = impact.burst(12, EmitterTransform::at([2.0, 0.5, 1.0]), &mut particles).unwrap();
assert_eq!(report.spawned, 12);
```

Each requested particle has its own counter-seeded random stream. Capacity and
per-call budgets do not perturb samples assigned to later requests, and an effect
never consumes a gameplay RNG. Changing rate or the requested event sequence still
changes the visual sequence. This is presentation behavior, not authoritative
physics or a cross-platform bitwise replay guarantee.

Overload keeps the newest bounded samples and advances skipped demand without a
catch-up loop. `EmissionStats` distinguishes requested/spawned, budget drops,
capacity drops, expired births and numeric rejection. These counts sum to demand.
Use rate/capacity/budgets for quality levels; Dune Skimmer scales dust rate with the
existing quality setting. Pool `dropped()` counts direct full-pool spawn attempts;
emitter capacity/budget drops are reported by the emitter itself.

Limits: 65,536 particles per pool, lifetime up to 3,600 seconds, drag 0..=128,
angular speed up to 1,024 radians/second, emitter rate up to 65,536/second and at most
256 processed emissions per update/burst. Emitter updates accept 0..=60 seconds;
split larger stalls according to the game's pause policy. ParticlePool's existing
update accepts any finite nonnegative delta and expires entries normally. Use
`try_new` for untrusted capacities; trusted `new` asserts the published limit.

## Trails, ribbons and beams

`Trail` stores a fixed ring and distance-resamples motion, with within-frame ages,
width/color interpolation and a current head. Stops and teleports insert explicit
breaks. Existing history fades by lifetime; it never connects across a restart or
long teleport. Capacity includes the visible head. `TrailConfig::fade_width` chooses
width tapering or fixed-width tire marks. One extra head value is stored outside the
ring. Sampling work is bounded and reports overwritten points, budget drops,
expiration and teleport counts.

`RibbonTriangles` accepts a `RibbonSource` such as a Trail, a bounded `RibbonPath`,
or an authored slice. It generates planar or camera-facing triangles with endpoint
widths/colors and bounded miter joins. Duplicate points, reversals, view-aligned
segments and explicit breaks are handled without NaNs. UVs expose path distance and
side for tooling; the current color renderer does not sample a texture.

`BeamPool` holds short-lived start/end segments with gradient color and lifetime
fade. Choose `DropNewest` or `ReplaceOldest`; counters expose overload. Beams reuse
the same camera-facing ribbon geometry. All updates and geometry iteration reuse
storage. Trail capacities are 2..=2,048 with 1..=256 resampled points per update;
ribbon sources allow at most 4,096 points and miter factors 1..=4; beam pools allow
up to 4,096 entries. Input validation is transactional. Trail clear/stop operations
preserve lifetime diagnostic counters; reset those explicitly when desired.

## Rendering and order

Construct a `ParticleRenderer` once with scratch capacity, then draw against the
current camera. Three-dimensional alpha particles sort by camera depth within that
pool with an allocation-free sort and explicit tie order. Additive particles do not
need sorting. Conservative clip-space bounds cull offscreen sprites without expiring simulation
entries. Shapes are square, diamond and triangle; authored spin and projected
velocity stretch preserve the faceted style. Colors clamp to normalized RGBA.

The GPU uses a shared static corner mesh and a **64-byte instance** containing a
clip-space center, two clip-space axes and tint. A quad would otherwise need six
32-byte expanded color vertices, or 192 bytes. The instance path uploads one third
of that per billboard, plus a one-time 72-byte corner buffer shared by all shapes.
This is a layout/transfer comparison, not a whole-game frame-rate claim. The shader
body is generated from the same Rust source for native GL 3.3 and WebGL2.

`draw_ribbon` and `draw_beams` use the shared triangle color stream. `EffectStyle`
selects alpha/additive blend, camera/explicit depth testing and optional depth writes.
Transparent effects default to depth testing without writing, so opaque surfaces
occlude them and later HUD drawing restores its own state. Two-dimensional alpha
ribbons can merge with ordinary 2D geometry when their states match. Instanced
billboards use separate batches; independently styled groups remain ordering barriers.

Draw opaque scene geometry first, effects afterward, then HUDs. Sorting is per
particle pool, not a global sort of all transparent scene materials/ribbons/pools.
Ribbons and effects are unlit and do not cast shadows. The current implementation
has no textures, soft depth intersections, volumetrics, GPU simulation or particle
collisions. `particle_triangles` provides reference geometry for software/tooling.

The frame budget examines at most 65,536 effect triangles (196,608 logical vertices)
across expanded ribbons and instances. Invalid samples consume budget too. A draw
reports truncation without iterating discarded work; renderer scratch overflow is
separate from the simulation pool's capacity. `effects::stats()` and browser canvas
`data-effect-vertices` expose work; native/browser upload metrics include 64 bytes
per sprite instance and expanded ribbon bytes. GPU resources initialize lazily,
reuse buffers and are owned by their context. Static retained mesh/instance ABIs
remain unchanged.

## Game integrations and validation

- Strike uses bounded accepted-shot beams, oriented impact sparks and bone-positioned
  muzzle flashes. Effects never create a hit confirmation or change damage/ammo.
- Neon Swarm replaces its separate particle vector/damping loop with seeded shared
  bursts and instanced stretched sparks.
- Dune Skimmer replaces analytically respawned dust cubes with moving seeded dust
  emitters and sorted instanced billboards, including quality/recovery handling.
- Pocket Rally replaces its growing skid-mark deque with two distance-sampled tire
  ribbons that fade, stop and break across recovery/restart.
- Sky Courier uses fading wingtip ribbons and a restrained engine exhaust emitter.

Courtyard and Orbit Breaker's custom software/browser hosts continue using the
compatible ParticlePool view API. Other catalog games compile against the shared
runtime without being forced to add effects to their art.

Local tests cover drag/frame partition, emission timing and saturation, independent
random request sequences, shape/cone bounds, world gravity, trail lifetime/rings/
teleport breaks, finite ribbon joins and beam overflow. Counting allocators cover
simulation and warmed sorted billboard/ribbon recording. Real GLX readback checks
alpha/additive/depth/HUD behavior and compares instanced shapes against CPU geometry,
including buffer reuse and upload accounting. Game/browser/release performance
validation proceeds in iteration 13; results are recorded in [performance](PERFORMANCE.md).
