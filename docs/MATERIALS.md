# Materials, shaders and hot reload

`ferravex-render::material` owns reusable shader handles, per-draw parameters,
blend metadata, bounded source storage and compiler status. Linux OpenGL and
browser WebGL2 hosts compile the same authored hooks against shared templates.
No shader, rendering or asset dependency is installed.

```rust,ignore
use ferravex_render::material::{ShaderMaterial, ShaderSource, MaterialOptions, BlendMode};
let material = ShaderMaterial::new(
    ShaderSource::fragment(r#"
        vec4 ferravex_fragment(vec4 color, vec3 world, vec3 normal) {
            float pulse = 0.9 + 0.1 * sin(u_params[0].x * 2.0 + world.y);
            return vec4(color.rgb * pulse, color.a);
        }
    "#),
    MaterialOptions { blend: BlendMode::Opaque, bounds_padding: 0.0, ..Default::default() },
)?;
// In the frame loop, after selecting the camera and lighting:
material.begin([[time, 0.0, 0.0, 0.0], [0.0;4], [0.0;4], [0.0;4]])?;
mesh.draw(position, rotation);
material.end();
```

Materials affect retained meshes, including retained meshes drawn with a 2D
camera. Immediate primitives and text keep their existing path. Bindings remain
active until end or replacement; they are not a nested push/pop stack. Four
`vec4` parameters (`u_params[4]`) are captured with each draw, so changes between
objects cannot accidentally merge different material instances. Nonfinite
parameters are rejected. World position and normalized vertex normal are passed
to the fragment hook after built-in lighting/fog. Interpolated fragment normals
can be normalized again by the hook when required.

`ShaderSource::fragment` supplies identity vertex hooks. A complete ShaderSource
can also define `vec3 ferravex_vertex(vec3 position)` and
`vec3 ferravex_normal(vec3 normal)`. These operate in local coordinates; vertex
hooks can also read the declared `a_position`, `a_normal` and instance attributes.
The engine then applies model/view transforms and inverse-transpose normals.
Authored `bounds_padding` must cover local vertex displacement for conservative
visibility. Recompute normals in the normal hook when deformation requires it.
Native hooks compile as GLSL 330; browser hooks compile as GLSL ES 300. Supply
functions rather than a version directive or main function. Platform-specific
GLSL features still need validation on each intended host.

BlendMode::Opaque is an explicit promise that shader output is opaque. It enables
depth writes and compatible opaque-pass regrouping. Alpha (the default) uses
straight-alpha blending; Additive uses source-alpha-weighted addition. Both keep
the camera's depth test, disable depth writes and preserve submission barriers.
Only opaque materials cast directional shadows. MaterialOptions also has casts_shadows and receives_shadows flags (both true by default); see [lighting](LIGHTING.md).
There is no automatic transparent-object sorting; draw them in the desired order.
Blend state is restored before following HUD/immediate draws.

## Reload and diagnostics

`material.reload(source)` queues a new revision. Source/metadata errors leave the
request unchanged. Actual compilation occurs at the next shared GPU host presentation, including
live materials whose meshes are currently off-screen. The latest source request applies at presentation time;
queue changes between frames when a frame must use one revision consistently.

Hosts replace their color/depth program pair only after both programs compile and link successfully.
A failed replacement keeps that host's last valid program. If the initial source
fails, the built-in material is used. `material.status()` reports Pending, Ready,
or Failed with a bounded compiler message, the requested revision, and the latest
successful feedback revision. Stale feedback is ignored. Compiler results are
context/device observations, not an offline portability guarantee. A stopped host cannot process pending revisions; an active host prepares live
material revisions before its draw commands.

Storage is bounded to 128 live materials, 64 KiB combined source per material,
16 finite parameter floats per draw and 4096 diagnostic bytes. Handles share
source/status; hosts release obsolete GPU programs after successful replacement
and collect dead handles during subsequent retained submission. Shader code is
authored application content, not a sandbox for arbitrary third-party programs.

## Native and browser editing

MaterialFileWatcher polls native file metadata at most twice per second, reads
bounded UTF-8 source, and queues changes transactionally. A missing, oversized or
invalid file preserves the previous request. It accepts an optional vertex-hook
file and a required fragment-hook file. Crystal Cavern exposes this directly:

```sh
FERRAVEX_CRYSTAL_SHADER="$PWD/examples/crystal-cavern/shaders/crystal.frag" cargo run --release -p crystal-cavern
```

Browser games expose an optional editor at `?material-tools`, or
`?autoplay&material-tools` for a moving demonstration. Select a live material,
edit either hook and apply it. Compiler errors appear beside the code while the
last valid program continues rendering. Reload source reads the current requested
source; reload the page to return to the game's authored version. Blend mode and
displacement bounds remain authored in Rust. The editor is absent from ordinary
game URLs and typing in it does not feed game input.

Crystal Cavern uses a subtle crystal pulse; Dune Skimmer uses vertex/fragment
hooks for its exhaust. These keep the existing low-poly meshes and palette.
Tests cover actual native GPU reload failure/recovery, parameter binding, vertex
hooks, alpha/depth behavior, additive/HUD isolation, batching boundaries, bounded
file reload, stale feedback and source limits. Browser editor failure/recovery is
also exercised on the actual WebGL host. Texture binding, arbitrary named
uniforms, compute shaders and an offline shader compiler are not part of this API.


## Recovering GPU state

Each live material retains its last successfully compiled vertex/fragment hooks
on the CPU. A failed later edit leaves that copy intact. A reconstructed native
context or restored WebGL context can rebuild the good program while preserving
the rejected revision's error/status. Both programs still need to compile on the
new device; otherwise the host reports the rebuild failure and uses its fallback.
The extra source copy is bounded by the existing 64 KiB/material source limit
and 128-material registry limit; it is released with the last material owner.
The Wasm ABI exposes requested hooks in stages 0/1 and last-good hooks in stages
2/3. Invalid stages return an empty result; stale compilation feedback is ignored.

Native pixel tests verify recovery after a failed edit and continued shader
reloads. An actual-Wasm contract verifies the source ABI, and the browser's
`?graphics-tools&material-tools` panels exercise real context loss/recovery.
See [platform status](PLATFORMS.md) for OS verification boundaries.
