# Parent integration — optional ECS access and wrapping action rows

Both owners returned registered production implementations with scoped native
validation. Parent reviewed the production paths and relevant tests, confirmed
their final source hashes, and found no actionable material defect. Downstream
demo adoption and combined runtime validation remain separate work.

## 210 — optional component access

Reviewed both new sealed Access implementations, the existing signature checks,
tuple matching/fetching, `World::get_many` initialization and iterator cleanup.
Optionality changes matching and the item shape, while preserving TypeId and
mutable-access metadata. Invalid identities fail before presence is checked.
`Option<Result<guard, Error>>::transpose` preserves the distinction between
absence and dynamic borrow failure. The entire required tuple matches before
iteration borrows anything. Partial guards unwind through existing safe arrays
and tuples; there is no unsafe code or structural identity change.

Owner evidence: 41 unit tests, six runnable doctests plus two compile-fail checks,
strict all-target Clippy and scoped formatting. Source hash:
`35fc63010cf765109eb8d37114a5b5727cdaf84b16084af4c0af1a7b4bb7fc58`.
No additional registration was needed. The World query rustdoc supplies a
fixed-step consumer with required Position and optional Velocity/Presentation.

## 272 — flow layout

Reviewed the complete production layout, row placement, weighted saturation and
rounding paths, plus boundary/growth/prefix tests. The registered module preserves
input order, validates hidden suffixes, calculates into bounded stack scratch,
then copies only the successful prefix. Insufficient room and short output
buffers report overflow; invalid or unrepresentable layouts preserve all output.
Grow weights apply after preferred widths, with cap redistribution. Rectangles
retain the absolute logical coordinate space needed by existing paint/input.

Owner evidence: 15 new behavioral integration tests, 34 existing UI tests,
13 runnable UI doctests, strict scoped Clippy and formatting. Its receipt also
contains a compiled/executed headless renderer recipe with real label measurement,
`Ui::hud_button`, CPU paint, pointer/focus geometry and safe-area composition.
This does not establish GPU visual quality or a game migration.

Verified final hashes:

- `ui.rs`: `b8725f70d1045f52716e0b8ab2e2d9afcaf1782a454cd2034f6cf85a11ca4c7f`
- `ui/flow.rs`: `77e3560f3a401de11b042670ed0e35c552f47d99f95b6d5d5bfc579fc9a639ef`
- `tests/journey400_272.rs`: `f58d4733bb434e10ccbff463f157176692b25e2a5d883c44a051b16dc67b7be9`

The explicitly authorized `pub mod flow;` registration exists. No facade/prelude
addition is needed: the qualified path remains `ferravex::play::ui::flow`.

## Parent validation and handoff

`cargo check -p ferravex-ecs -p ferravex-play --all-features --target wasm32-unknown-unknown --offline --locked -j 2`
passed against the final hashes above; log `210-272-parent-wasm.log`. These two
crates have no distinct optional feature matrix. Compilation is not a browser
playtest. The current served Marble snapshot intentionally excludes these APIs.

Added the optional-query recipe and layout ownership/overflow rules to the
coding-agent API map. Link checking and scoped whitespace validation are recorded
in `210-272-api-map-links.json`; existing guide snippets were unchanged.

The next independent owners are 204 executable system scheduling and 273 shared
wrapped-text measurement. They do not modify these reviewed source files.
Later ECS filters can build on 210 when their slot opens; later game/presentation
owners must adopt these actual APIs and preserve the accepted Journey200 fixes.
No new dependencies, manifests, global formatting or shared staging changes were
needed for this parent integration.
