# Data-driven scenes and prefabs

Ferravex's first-party authoring system loads typed components, scene hierarchy and
hierarchical prefabs from bounded UTF-8 data. It builds the existing exact-matrix
`Scene` graph and exposes validated components in `SceneData`. No executable script,
JSON/TOML dependency, reflection library or imported content framework is used.

`ferravex_play::authoring` and `ferravex_engine::authoring` provide the format and
schema APIs. The optional `ferravex-render/content` feature supplies native file
watching and a direct browser/Wasm edit bridge. The `content` feature on the main
engine facade enables that host support too. Applications register the component
schemas and construct gameplay/graphics resources after validation.

## Write a scene

```text
ferravex_scene 1;
prefab checkpoint {
  node root {
    component checkpoint { order int 0; route ref /way0; }
  }
  node flag {
    transform -1 2 0   0 0 0 1   1 1 1;
    component prop {
      shape text box;
      size vec3 1 0.5 0.05;
      color rgba 62 143 133 255;
    }
  }
}
node way0 { component waypoint { order int 0; } }
node first {
  use checkpoint;
  transform 10 1 -12   0 0 0 1   1 1 1;
  override flag {
    component prop { color rgba 236 175 65 255; }
  }
}
```

A `transform` lists translation XYZ, quaternion XYZW and scale XYZ. Missing local
transforms use identity; a prefab instance's explicit transform replaces its root
local transform. Parent matrices compose exactly, retaining shear where applicable.
Game-specific adapters may reject shear, negative scales or unsupported dimensions
when constructing physical shapes rather than approximating them.

`node` identifiers and component/field names use 1..64 ASCII letters, digits, `_`
or `-`. Identifiers are case sensitive. Each prefab has one node named `root`;
other nodes default to that root as parent, or specify `parent local_name;`.
Top-level nodes default to no parent. Parents may be declared later; cycles and
missing parents are rejected. Prefab roots cannot declare a parent, and prefab
parents cannot escape their instance's scope. All definitions, including unused
prefabs, are checked for malformed fields, dependency/parent cycles and bounds.
Required fields and final references are checked on concrete instances, allowing
partial component templates to receive required values from an instance.

An instance named `first` expands children to `first/flag`. Nested instances expand
further without aliasing another instance's children. `override relative/path {}`
can change an existing descendant's transform or merge component fields. Root
fields merge with template defaults; later explicit override fields win. Overrides
cannot silently create a misspelled descendant or change parent relationships.

Values have explicit types: `bool true|false`, `int` (exact signed i64), `number`
(finite f64), `text`, `vec2`, `vec3`, `quat`, `rgba` (four bytes), and `ref`.
Vector/quaternion components must fit finite f32 values. Zero quaternions are
invalid. Numbers accept decimal/scientific notation. Strings may be bare tokens or
quoted UTF-8; quoted strings support `\"`, `\\`, `\n`, `\r` and `\t`. Other escapes,
raw controls, NUL, non-finite numbers, overflow, duplicate declarations/fields,
unknown syntax and unsupported versions are rejected. `#` starts a line comment
outside quotes. Semicolons terminate values and node commands; after a closing
brace they are optional.

References in a prefab resolve inside its scope, so `ref flag` becomes
`first/flag`. Instance overrides use the surrounding scope; `ref @flag` explicitly
selects the instance's own child. `ref /world_node` is absolute. `root` denotes the
current prefab root in local paths. Schemas can require that a reference points
to a node with a particular component. IDs and handles belong to their scene
owner; remap through a new `SceneData` when adopting a new generation.

## Register a schema and load

```rust
use ferravex_play::authoring::*;
let mut schema = Schema::new();
schema.register("waypoint", &[
    FieldRule::required("order", ValueKind::Integer).integer_range(0, 127),
]).unwrap();
schema.register("checkpoint", &[
    FieldRule::required("order", ValueKind::Integer).integer_range(0, 15),
    FieldRule::required("route", ValueKind::Reference).target("waypoint"),
]).unwrap();
// Register prop and every other component used by the document too.
```

Schemas reject unknown components and fields, missing required fields, type
mismatches, invalid ranges and references. Numeric/vector ranges use finite f64
bounds; integer ranges stay i64 without conversion to floating point. Optional
fields are absent unless authored; application code supplies any fallback.
`Document::load(text, &schema)` parses, expands, validates and returns `SceneData`.
Structured errors contain a kind, context and source line/column/byte offset.
Game-specific validation then checks singleton roles, ordered collections, counts,
physical dimensions and other domain rules before creating live state.

`SceneData::nodes`, `node`, `of_kind`, `handle` and `hierarchy` expose the validated
result. Each node includes its local TRS, parent, cached exact world matrix and
components; typed `Value` accessors return `Option`, not unchecked casts. Loading
allocates bounded maps/buffers. Traversal and component lookup do not parse data or
allocate new storage. This is explicit schema-based data, not automatic arbitrary
Rust-component serialization or an ECS reflection registry.

## Cook and validate binary data

`SceneData::encode` writes deterministic `FXSC0001` data: node IDs/parents, local
TRS, sorted component/property names and tagged typed values, followed by an FNV-1a
64-bit checksum. `SceneData::decode(bytes, &schema)` validates sizes, UTF-8, tags,
booleans, numeric values, checksum, hierarchy, schema and references again. Derived
world matrices are recomputed. Corrupt/truncated/trailing bytes are rejected.
The checksum detects accidental corruption; it does not authenticate content.

Cooking flattens prefab instances and removes templates/source formatting. An
application that loads only cooked data can omit calls to the text parser and
allow the linker to discard it. These two editable example games embed their
original authoring text and load it once, retaining it for the developer editor;
they prioritize inspectable live examples over eliminating every authoring byte.
No source or binary data is parsed during ordinary gameplay frames.

## Reload locally

```sh
cargo run --release -p orchard-guard -- \
  --content examples/orchard-guard/content/orchard.fxscene
cargo run --release -p marble-run -- \
  --content examples/marble-run/content/aqueduct.fxscene
```

These games poll only an explicitly supplied file, at most twice per accumulated host
second using the unpaused frame dt. The bounded read compares actual bytes,
so a same-length save with an unchanged/coarse modification timestamp is detected.
Invalid reads or content leave the last valid source, generation and game intact.
The embedded scene is the initial fallback. A successful edit constructs the new
level, simulation and graphics before replacing the live state. It deliberately
restarts the game, resets its clock/camera where needed, and does not attempt to
migrate an in-progress match. Normal restart reuses the currently accepted level.

For the browser, add `?content-tools` to either game URL. The first-party editor
loads only with that option and exposes the source, Apply and Restore controls.
Ctrl/Cmd+Enter applies an edit; failures display diagnostics and retain the live
game. Restore reads the last accepted source, preserving edits until explicitly
requested. Editor focus releases gameplay controls; clicking the canvas resumes
them. Source remains local to that browser session and is not saved or uploaded.

The render host's `ContentSource::poll(dt, builder)` supports the same transactional
pattern for other games. The trusted builder must validate its domain and finish
constructing replacement resources before returning `Ok`; it should not mutate
live gameplay while staging. Wasm upload/source/status functions are a bounded
first-party platform ABI. The normal shipping build script isolates the two
editing-enabled games into a separate Cargo feature cohort, so their exported
editing ABI is not linked into the other thirteen Wasm games. A direct workspace
build may unify dependency features; use the supplied staging script for those
isolated artifacts.

## Current content and limits

Orchard Guard's data drives its route, one to eight planting plots, autoplay build/
upgrade priority, one to sixteen waves, spawn intervals, health/speed variants,
rewards and starting economy. Tower weapon definitions remain game code. Rendering,
path cursors, targeting and combat consume the same accepted level.

Marble Run's data drives platform/obstacle geometry and colliders, spawn, route
waypoints, checkpoint-to-route references, pickups, finish and rolling/gravity/
friction/damping tuning. Checkpoint flags are hierarchical prefab props. Identical
prop/obstacle geometry shares immutable resident meshes. Decorative trim follows
platform transforms; rendered and physical platform dimensions/orientation agree.
The original six-gem course and complete tower campaign remain winnable in tests.

| Limit | Maximum |
|---|---:|
| UTF-8 source | 2 MiB |
| Cooked data / expanded payload budget | 8 MiB |
| Authored nodes / expanded concrete nodes | 4,096 each |
| Prefab definitions / nesting depth | 256 / 16 |
| Hierarchy depth | 128 |
| Components per node / fields per component | 32 / 64 |
| Total typed values (authored and expanded budgets) | 65,536 each |
| Text value bytes / expanded node path bytes | 1,024 / 512 |

Marble additionally bounds platforms/obstacles to 32 each, checkpoints to 16,
pickups to 64, route points to 128 and authored props to 128, with effective size/
position checks before creating meshes or colliders. Structurally valid authored
levels are not automatically proven reachable or balanced. There is no visual
world editor, collaborative storage, script execution or automatic state migration.
Validation, available-platform checks and size/performance results are recorded in
[PERFORMANCE.md](PERFORMANCE.md).
