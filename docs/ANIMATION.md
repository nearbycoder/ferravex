# Animation and GPU skinning

Ferravex shares renderer-independent animation players through `ferravex-play::animation`
and GPU skinning through `ferravex-render::skinning`. The engine facade exposes both
under `ferravex_engine::animation`. All tracks, clocks, poses, bindings and shader code
are original first-party implementations using the workspace math crate and Rust std.

## Clips and players

`Track<f32>`, `Vec3Track` and `QuatTrack` validate ordered finite keys. Step and linear
interpolation include normalized shortest-arc quaternion slerp. An `AnimationClip`
contains a duration, clamp/repeat mode, sparse joint translation/rotation/scale tracks
and timed event IDs. Missing properties hold the bank's bind pose. A property shorter
than the clip holds its last key until the whole clip wraps. A joint appears at most
once; put all of its channels in that `JointTrack`.

`AnimationBank` shares immutable bind poses and clips through Arc. An `Animator`
allocates its four pose buffers once. Sampling, play, seek, crossfade, interruption,
layers, graph transitions and event delivery reuse storage. `pose()` returns local
TRS values; `Skeleton::skin_palette_into` computes hierarchical world * inverse-bind
matrices into caller-owned scratch without allocating. Parents precede children.

```rust
use ferravex_play::{animation::*, scene::{Transform, Skeleton, Joint, IDENTITY}};
let track = Vec3Track::new(vec![
    Keyframe { time: 0.0, value: [0.0, 0.0, 0.0] },
    Keyframe { time: 1.0, value: [0.0, 0.1, 0.0] },
], Interpolation::Linear).unwrap();
let clip = AnimationClip::new(1, 1.0, LoopMode::Repeat, vec![JointTrack {
    joint: 0, translation: Some(track), rotation: None, scale: None,
}], vec![AnimationEvent { time: 0.5, id: 7 }]).unwrap();
let bank = AnimationBank::new(vec![Transform::default()], vec![clip]).unwrap();
let mut player = Animator::new(&bank, 0).unwrap();
let mut events = EventBuffer::<16>::default();
player.advance(1.0 / 60.0, &mut events).unwrap();
let skeleton = Skeleton::new(vec![Joint { parent: None, inverse_bind: IDENTITY }]).unwrap();
let mut world = [IDENTITY];
let mut palette = [IDENTITY];
skeleton.skin_palette_into(player.pose(), &mut world, &mut palette).unwrap();
```

`transition(clip, seconds)` keeps an already selected clip running; `play(clip)`
restarts it. A normal fade advances both outgoing and incoming clips. Interrupting a
fade freezes the current base blend as the next source, preserving continuity
without baking an additive layer into that source a second time.
Only the incoming clip emits events. This avoids duplicate footsteps during a fade.
`apply_layer` stacks masked overrides or additive aim/recoil over that base pose.
Layers are visual and silent; the next advance/play/seek replaces them with a fresh
base pose, so reapply active layers each frame. Additive translation and scale use
differences from the authored bind/reference pose; rotation composes its quaternion
delta. Zero reference scale does not require division.

`AnimationGraph` shares bounded states and authored edges. Conditions support scalar
thresholds, one-shot completion and latched triggers, combined with AND and a minimum
exit phase. Authored edge order breaks ties; one edge may win per update. Trigger
conditions consume only a winning edge's triggers. `GraphPlayer::reset` handles
respawn/replay using the same storage. Conditions run before advancing, so completion
becomes eligible on the next update after a one-shot reaches its endpoint. Pause
holds clocks and graph transitions. A zero-speed clip can still leave through a graph
condition. Scalar parameters are finite; triggers have a separate latch namespace.

## Clock and event boundaries

`Timeline` accumulates phase in f64. Speed is forward 0..=8x; reverse playback is not
implemented. Crossfade duration uses unscaled host seconds and freezes on pause or
zero speed. Clip durations are 0.0001..=3,600 seconds; each update accepts 0..=60
seconds and rejects larger/non-finite deltas without advancing. Split larger stalls
explicitly according to the game's pause/catch-up policy.

Events cover `(previous, next]`, plus initial time-zero events. A repeat seam emits
the old cycle's end events before the new cycle's zero events. Seek is silent; restart
rearms the initial events. Pause, zero speed and zero dt emit nothing. `EventBuffer<N>`
retains chronological earliest hits and reports the exact dropped count. Millions of
crossed loops still take bounded work: O(authored event count + output capacity).
This is presentation timing, not a deterministic cross-platform simulation clock.
Authoritative ammo, damage and movement stay in gameplay/netcode.

Limits per bank: 256 joints, 128 clips; per clip: 65,536 total property keys and 256
events. Graphs allow 64 states, 256 edges, 32 scalar/trigger slots and eight conditions
per edge. Joint masks contain 0..=1 weights. Invalid inputs leave the visible pose
unchanged; skeleton output buffers are scratch on error and must be used only after
success. Core animation does not extract/apply root motion automatically, perform
retargeting, IK, morph targets, compressed-clip streaming or import external formats.

## Retained skinning

`SkinnedMesh::new(&mesh, &weights, joint_count)` snapshots expanded positions, flat
normals, colors and four joint/weight influences per source vertex. Nonnegative
weights normalize once; zero sums, non-finite weights and out-of-range joints are
rejected. Vertices can use `SkinWeights::rigid(joint)` for authentic faceted limbs.
Clones share immutable geometry and weights, uploaded once per GPU context.

`SkinPalette::new` / `set` validate up to 32 affine invertible matrices and compute
inverse-transpose normal matrices. `draw_skinned(&mesh, model, &palette)` copies the
pose into the current frame. `draw_skinned_shadow` records a hidden first-person body
caster. Later palette edits cannot change an already recorded draw. Material vertex
hooks receive posed local positions/normals; custom materials and shadows use the
same skinning shader body on Linux GL 3.3 and browser WebGL2.

The 32-joint GPU bound fits minimum WebGL2 vertex-uniform limits alongside existing
camera/material inputs. The built-in vertex interface uses at most 237 vectors
with its position/normal palettes, below the 256-vector minimum in
[OpenGL ES 3.0, table 6.31](https://registry.khronos.org/OpenGL/specs/es/3.0/es_spec_3.0.pdf#page=289),
which underlies [WebGL2](https://registry.khronos.org/webgl/specs/latest/2.0/).
Authored shader hooks have the remaining budget and can still exceed device limits.
Core banks may be larger; split large meshes at authoring.
A retained skin accepts up to 65,536 source vertices; all static retained storage
limits still apply, including weights in the 64 MiB CPU snapshot budget. Per frame,
32,768 copied palette joints bound position/normal storage. Adjacent identical
palettes reuse storage and can instance together; independently animated poses
usually need separate draws. Normal skinning blends per-bone inverse-transpose
normals, rather than recomputing deformed triangle normals.

Culling uses conservative deformed bounds from influenced bone boxes, including a
rounding margin. It never culls against only the undeformed bind box. This costs at
most eight transformed corners per used bone, rather than CPU-skinning every vertex.
The CPU `transform_point` helper is a reference for tooling/tests. Palette updates,
bounds and recording allocate zero times after frame buffers warm up. Metrics include
palette uniform upload bytes for color/shadow programs; the static mesh and instance
ABIs remain 40 and 112 bytes, while skin weights add 32 bytes per expanded vertex.

## Game adoption and verification

Strike uses an eight-joint operator with idle, walk, run and airborne states, additive
aim, reload and recoil. Walking/running events emit remote positional footsteps. Its
four-joint first-person weapon animates the magazine and support hand during the
replicated reload interval, with timed mechanical clicks. Late reload snapshots resynchronize silently; weapon
bob playback speed follows movement and muzzle flashes follow the evaluated bone.
Magazine events never
change ammo. Body shadows use the same evaluated pose as the visible operator.

Emberline uses timed procedural sprite IDs for idle breathing, a speed-scaled running
cycle, ascent, fall and landing recovery. Run events emit surface footsteps. Facing
persists when stopping; pause, restart and respawn reset/hold presentation explicitly.
It keeps its original shape-based artwork without adding an atlas or image payload.

Local tests cover quaternion/bind channels, moving/interrupted fades, masked and
additive layers, state/trigger priority, pause/seek/restart, exact event boundaries,
event storms, invalid data, finite palettes and unchanged authoritative game state.
Counting allocators cover core graph/layer/skeleton updates and warmed retained skin
recording. A real Linux GLX readback compares four-weight skinned geometry, nonuniform
normals and shadows with an equivalent static model, then changes its pose and checks
GPU resource reuse/collection. Browser/game smoke, release sizes and performance
results are recorded as iteration 12 validation proceeds; see [performance](PERFORMANCE.md).
