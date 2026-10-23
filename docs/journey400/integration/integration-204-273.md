# Parent integration review — tasks 204 and 273

Both implementations are registered and available through the existing facade.
Parent source review found no actionable defect in the reviewed scope. This is
scoped source/native/Wasm integration evidence, not final demo/browser acceptance.

## Prepared sequential systems (204)

Reviewed `systems.rs` production code, registration, command-flush composition,
stop/error/predicate behavior and its App contextual-hook test. The schedule
delegates ordering to the existing SystemOrder, caches only validated indexes,
invalidates after successful configuration changes, and preserves preparation
after rejected edits. Missing callables prevent any run, including disabled
entries. Disabled predicates are not invoked. Explicit flush entries obey the
same order and stop/error boundaries as other callbacks; no implicit cleanup
flush is introduced. A subsequent run starts at the beginning, with earlier
context mutations retained. This is documented, not rollback or automatic retry.

The 256-name bound and sequential ordering remain those of SystemOrder.
The App test checks scheduled versus executed ticks, including a stopping/error
first update in a four-tick batch. The owner reports 12 behavioral tests, three
allocation tests and six doctests under the relevant feature combinations, plus
strict scoped Clippy. Exact commands and limits remain in `receipts/204.md`.

The granted `pub mod systems;` registration is present. Existing facade wildcard
reexports expose `ferravex::systems` without another root edit. Parent added an
API-map entry describing preparation, executed ticks and explicit flushes.

## Measured wrapped text (273)

Reviewed the full borrowed layout/iterator and the shared painter/queue changes
against the immutable prior source. The existing Lines wrapping algorithm moved
unchanged. UTF-8 ranges retain valid scalar boundaries; the suffix is separately
borrowed static text. Width/size checks, explicit Clip/Ellipsis limits, hidden-line
counting and measured full line-slot heights are documented. Blank-line and
CRLF behavior deliberately preserves the existing UI painter, distinct from the
immediate-font API. Measurement scans hidden text; the line limit is not a CPU
work limit, and hidden counts do not describe geometric clipping.

Queuing checks the existing text-arena bound before measuring, reserves retained
glyph/shadow geometry before mutation, and keeps legacy Ui::text behavior.
Painter iteration uses the same measured lines and suffix. Reviewed regression
coverage includes prior paint geometry, partial clipping, narrow markers, UTF-8,
capacity/reset behavior and warmed allocation checks. The owner reports 14 shared
UI tests, six final integration tests, the existing allocation regression, 11 UI
doctests and seven font tests, plus strict scoped Clippy. See `receipts/273.md` for
commands and default/all-feature distinctions.

The granted `pub mod text_layout;` registration is present. Existing facade paths
expose `ferravex::render::ui::text_layout` and `Ui::wrapped_text`. Parent added an
API-map entry describing measured heights and explicit hidden-content handling.
Task274 owns separate scroll modules; this review did not modify those files.

## Executed parent verification

- `cargo build -p ferravex --all-features --offline --locked -j 2 --message-format=json`
  passed on native Linux. Cargo reported successful completion with no compiler
  diagnostic messages; `facade-204-273-native.jsonl` and `.log`.
- The same command with `--target wasm32-unknown-unknown` passed;
  `facade-204-273-wasm.jsonl` and `.log`.
- Extracted each module's real first rustdoc recipe and changed only its import
  prefix to the public facade. Compiled both against the exact ferravex rlib
  paths returned by those Cargo builds. Both native binaries executed all
  assertions successfully; both Wasm binaries linked successfully. The system
  recipe uses actual tick batches, a queued command and explicit flush; the UI
  recipe measures and queues a briefing then invokes the actual paint path.
  `facade-recipes-204-273/report.json` records exact rustc commands/artifacts and
  execution distinctions. The Wasm recipes were not executed in a browser.
- `python3 -B scripts/check-docs.py docs/AGENT_GUIDE.md --json` passed all 100
  local links, zero errors; `guide-links-204-273.json`. Its two existing marked
  examples were not recompiled by this link-only invocation.
- Scoped parent whitespace checks passed. Relevant production/test hashes are
  recorded in `integration-204-273-source-hashes.json`.

No additional Rust registrations or dependency changes were necessary. The
current Dune visual release intentionally excludes ongoing Journey400 changes;
its browser evidence cannot establish these APIs' rendering or demo adoption.
Later assigned consumers still own adoption, fresh visual checks and final
workspace integration. Neither numbered task is labeled fully integrated here.
