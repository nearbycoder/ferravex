# Debug saves, replays and snapshots

Run the [complete debugging example](../crates/ferravex-engine/examples/journey200_diagnostics.rs)
to follow a small recording from asset identification through deliberate failures
and recovery. It uses the registered engine APIs and Rust std, with no new
dependencies, window, wall-clock timing or user-save access. The private memory
backend injects failures into the real `Slot` implementation; it does not exercise
disk or browser durability.

From the repository root:

```sh
cargo run -p ferravex-engine --example journey200_diagnostics --offline --locked -j 2
cargo test -p ferravex-engine --example journey200_diagnostics --offline --locked -j 2
cargo clippy -p ferravex-engine --example journey200_diagnostics --offline --locked -j 2 -- -D warnings
python3 -B scripts/check-docs.py docs/JOURNEY_200_DEBUGGING.md --json
```

The executable takes no configuration and runs a fixed, bounded scenario. On
success stdout is NDJSON: sixteen diagnostic records followed by one summary.
The summary has seventeen occurrences because the input mismatch is logged twice
and consecutively deduplicated. Capacity is 32 and evictions are zero. Expected
errors appear in the report while the command exits successfully; unexpected
outcomes fail assertions or return an error. Cargo's build messages go to stderr.
The example also asserts behavior that is not printed, including invalid snapshot
queries, protected writes and bounded seeking.

## Start with asset identity

[`Bundle::manifest`](../crates/ferravex-engine/src/assets.rs) borrows canonical
paths and yields `AssetManifestEntry { path, byte_len, fingerprint }` in sorted
path order. Lengths and fingerprints describe decoded bytes. Checked edits accept
authoring aliases; the example's `./rules//counter.txt` becomes
`rules/counter.txt`. Strict `Bundle::insert` expects a canonical path.

The example's rules asset is the eighteen-byte string `checked-u64-add/v1`, whose
fingerprint is `7a861afd0e38c200`. Replacing only the final byte with `2` preserves
length but changes the fingerprint to `7a861dfd0e38c719`. Encoding and decoding
the bundle preserves its manifest. Re-inserting identical content retains its
existing payload `Arc`, as the example checks with `Arc::ptr_eq`.

Fingerprints are 64-bit FNV-1a, matching the existing FXPK0001 content checksum;
they detect likely content differences, not malicious changes or proven equality.
Manifest iteration hashes each yielded asset on demand. It allocates no manifest
storage but scans payload bytes, so use it at load/reload or explicit inspection
boundaries rather than on every simulation tick. Retain `LiveBundle::current()`
when inspecting one live generation across subsequent reloads.

This tiny recording uses its sole simulation-rules asset's fingerprint as
`Header.context`. Its title asset affects presentation only. Real games must
define a stable context covering all relevant level/rules inputs; there is no
automatic aggregate-manifest context API. Log schema, version, tick rate, seed
and context with a reproduction. Do not force an incompatible clip to start by
replacing its context with the current one.

## Capture structured evidence

[`Diagnostics`](../crates/ferravex-engine/src/diagnostics.rs) stores bounded local
records, filters by severity, and combines consecutive equal records, including
their context. `record_with_context` returns `Result<bool, DiagnosticInputError>`:
`false` means filtered or disabled, not a recorded failure. Codes are supplied
by the application; the example's `replay.state_mismatch` and `save.protected`
are example conventions, not new library error variants.

Include an operation and useful recovery advice. Add `DiagnosticSource` only
when a real source location is known: line and column are one-based, and columns
count Unicode scalar values. Source paths are identifiers; the collector never
opens them. Runtime mismatch records in the complete example use `source: None`.

This separate complete program attaches the known start of an in-memory asset.
It prints one diagnostic and a summary, and needs only `ferravex_engine` as a
direct external crate:

```rust,check,crate-type=bin
use ferravex_engine::{
    assets::Bundle,
    diagnostics::{DiagnosticContext, DiagnosticSource, Diagnostics, Severity},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut bundle = Bundle::default();
    bundle.insert("rules/counter.txt", b"checked-u64-add/v1")?;
    let entry = bundle.manifest().next().unwrap();
    assert_eq!(entry.fingerprint, 0x7a861afd0e38c200);
    let mut report = Diagnostics::new(4, Severity::Info);
    assert!(report.record_with_context(
        0, Severity::Info, "asset.loaded",
        &format!("{} bytes; fingerprint={:016x}", entry.byte_len, entry.fingerprint),
        DiagnosticContext {
            source: Some(DiagnosticSource { file: entry.path, line: 1, column: 1 }),
            operation: Some("inspect_rules"),
            help: Some("Compare this rules fingerprint with the recording context."),
        },
    )?);
    assert_eq!(report.summary().retained.records, 1);
    print!("{}", report.ndjson());
    Ok(())
}
```

`ndjson()` is a borrowing `Display` adapter suitable for `std::io::Write`;
`write_ndjson` accepts a `std::fmt::Write` sink. Both export the entire retained
snapshot without draining it. Each line has `stream_version: 1`; the final
`type: "summary"` line ends the snapshot. Appending a second export repeats
retained records. `summary()` counts retained records and occurrences, not all
events ever seen; `evicted` counts removed records. Ordering is insertion order,
so inspection records can refer back to earlier ticks. A writer failure can
leave an incomplete final line and must be handled by the caller.

The existing `json()`/`to_json()` snapshot has `schema_version: 1` and remains a
different envelope. These serializers do not redact caller-supplied text. Keep
save payloads and personal information out of diagnostic messages.

## Separate replay failure stages

The example uses the current [timeline API](../crates/ferravex-play/src/replay/mod.rs),
reexported through `ferravex_engine::replay`. `Recording` is the FXRT0001 timeline;
the older engine `Replay`/`Frame` API still uses FXRP0001. They are not
interchangeable decoders. See [the replay guide](REPLAYS.md) for game adoption.

The scenario records four one-byte inputs at 60 Hz. Its state is a checked u64
counter, so values after ticks 1–4 are 1, 3, 6, 10. A checkpoint is eight
little-endian bytes; this example's identity state checksum covers its entire
mutable simulation state. Recorder limits are four ticks, zero events, three
checkpoints and a 64-byte payload arena. The seed is unused because the world
has no randomness. Every step is a simulation tick, with no render-time input.

1. **Decode:** corrupting a copy's checksum produces `ReplayError::Checksum`.
   The original recording remains intact. A malformed file is not a state desync.
2. **Compatibility:** changing the rules context causes
   `PlaybackError::Incompatible` before restoring the world. Check schema,
   version, tick rate and context before investigating per-tick differences.
3. **Input:** `step_with_input` compares supplied bytes before stepping. The
   deliberately wrong first input leaves both world and cursor at tick zero.
   `first_mismatch()` reports tick 1 and `ReplayChecksum::Input`, with checksums
   of expected and supplied bytes. It does not retain the bytes for you.
4. **State:** after three verified ticks, the example adds one to the live
   counter, then steps normally. Tick 4 produces expected 10 versus actual 11.
   `first_mismatch()` reports `ReplayChecksum::State`. The failing step already
   ran: the cursor is at tick 4 and the world contains the divergent value 11.
5. **Recovery:** another step returns the retained fault without advancing.
   An explicit seek restores checkpoint 2; `pump_seek` with budget zero does
   nothing, then budget one verifies tick 3. The example finishes that suffix,
   then restarts from the initial checkpoint and verifies all four ticks.

A final-tick mismatch is `PlaybackStatus::Faulted`, with `finished() == false`,
even though the row cursor reached the end. A paused player, an unstarted cursor
and a completed verified replay are distinct states. `reset()` clears the cursor
and diagnostics but does not restore the world; call `start()` before stepping.
Successful restart/seek begins a new verification segment, clearing the old
mismatch. Copy the mismatch before recovery if it belongs in a report. Seeking
can skip earlier ticks; finishing after a seek does not prove that skipped prefix.

In a real adapter, checkpoint restore must validate a temporary replacement
before publishing it. Hash every simulation-relevant field, including RNG,
queues and timers. Consume input edges only on actual ticks. Expected replay
events describe accepted events to compare against new simulation; do not apply
their damage or rewards a second time. Suppress seek-time presentation effects,
and keep local overlays separate from authoritative network/replay advancement.

## Inspect bounded state history

[`SnapshotRing`](../crates/ferravex-engine/src/snapshots.rs) stores caller-owned
states, not automatic engine saves or replay checkpoints. The example captures
only successfully verified counter states into a capacity-three ring. Tick 0 is
evicted after tick 3; the retained ticks are 1, 2 and 3. The divergent tick 4 is
not published into that verified history.

`bracket_checked(tick, fraction)` distinguishes `Empty`, `BeforeOldest`, `Exact`,
`Between` and `AfterNewest`. At tick 2 plus 0.5, it borrows `(2, 3)` and `(3, 6)`
with `alpha == 0.5`. This is an inspection/interpolation bracket, not an
authoritative state of 4.5. A query at 3 plus 0.5 is explicitly after the newest
snapshot; the API does not extrapolate.

Duplicate or decreasing pushes return the rejected state and leave history
unchanged. Fractions outside `[0, 1]`, including NaN, are rejected. Exactly 1
carries to the next tick, with overflow checked. The example asserts duplicate
rejection and NaN handling. For rollback, discard future history explicitly with
`truncate_after` before appending a new branch. Restore game state separately;
neither a query nor truncation performs a restore.

## Preserve saves while diagnosing failures

The example uses existing `RecordBook` data through
[`Slot::with_backend`](../crates/ferravex-render/src/storage/mod.rs). The private
`MemoryStore` accepts one owner/slot and at most `RecordBook::MAX_BYTES`. Its
`Loaded` status means loaded from this scratch backend, not persisted to a disk.
It demonstrates these actual slot semantics:

- An injected `Unavailable` write keeps the accepted value, generation and
  backend bytes unchanged. The candidate is held separately and an explicit
  successful retry advances generation from 1 to 2. Inspect the `commit` result:
  `status()` is still `Loaded` after that failed write.
- `DurabilityUncertain` is a special error: complete replacement happened but
  durability was not confirmed. `Slot` accepts the new value and generation
  despite returning this error. The memory backend simulates the contract; it
  does not simulate an actual filesystem sync failure.
- Corrupt binary bytes yield `LoadStatus::Rejected(Invalid(InvalidData))` and
  usable defaults. Ordinary `commit` returns `Protected` before calling the
  backend, leaving the original rejected bytes untouched.
- Explicit `reset` replaces the disposable slot, sets generation to 1 and allows
  a successful reload. In a real tool, preserve the rejected file and make reset
  a deliberate user recovery decision. It is not an automatic retry strategy.

See [saves and records](SAVES.md) and the [binary persistence source](../crates/ferravex-play/src/persistence/mod.rs)
for schema/version validation, typed bounds and native storage paths. The binary
envelope remains FXSV0001. Browser transport remains `fxsv1:` under
`ferravex.save.v1.<game>.<slot>` keys. Synchronous load/commit belongs at explicit
event boundaries, not inside warmed per-tick profiling or automatic retry loops.

## Browser support evidence is separate

The source [browser storage bridge](../web/storage.js) exposes
`bindStorage(...).status()` with operation, code, key, bytes, limit, message and
action, without rereading storage. Its codes distinguish quota, unavailable and
corrupt transport. The preserved numeric Wasm imports have fewer categories;
Rust `StorageError::Unavailable` alone cannot identify browser quota versus
blocked storage. Inspect the bridge result at the host boundary. Rust validates
the binary save after successful transport; a transport `corrupt` result is a
different layer from a rejected `SaveData` document.

The Details panel's **Download diagnostics JSON** control is implemented in
[`web/details.js`](../web/details.js). Its bounded support snapshot uses
`schema: "ferravex.runtime-diagnostics"`, `version: 1`, with counters and capability
states and an 8192-byte download limit. Missing/unavailable measurements are not
zero-valued success. It contains neither save data nor replay state, and is not
the Rust NDJSON report or a `SnapshotRing` dump.

Player settings backup is yet another format: [`web/player.js`](../web/player.js)
exports version 2 and strictly imports supported version 1/2 settings, retaining
the existing local-storage key. Motion and flash preferences are independent;
settings backup does not include game progress or control bindings. Do not
interpret its version as the FXSV0001 save version.

These browser descriptions are source-backed integration pointers. This example
does not load Wasm, open the Details panel, exercise storage permissions/quota,
or validate Strike's replay panel. Fresh browser and combined integration checks
belong to the integration owner.

## Validation scope

The native example was compiled and run on Linux, its three focused tests passed,
and its example-target Clippy check passed with warnings denied. The tests cover
the full failure/recovery sequence, transactional malformed/overflowing example
world input, and scratch-backend key/size bounds. The complete marked program
above was separately compiled and executed; its output was parsed as JSON.
Local links were checked with the repository documentation checker.

The checker selects `rust,check` fences only. For `--check-rust`, pass
`--extern ferravex_engine=PATH` using the matching `.rlib` in the fresh Cargo
artifact's dependency directory, so its sibling first-party dependencies are
discoverable. `--extract` writes sources into a new directory; `--check-rust`
type-checks and never runs them. Actual execution of the extracted program is a
separate validation step. See [`scripts/check-docs.py`](../scripts/check-docs.py)
and its `--help` for implemented options.

This is evidence for the bounded native example and its adapters. It does not
establish cross-platform determinism, physical-device support, game playability,
filesystem durability, browser behavior, or full Journey200 integration. No
wire format, public library API, manifest, lockfile or platform export changed.
