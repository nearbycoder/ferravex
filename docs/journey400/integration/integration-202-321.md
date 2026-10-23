# Parent review and narrow integration: 202 and 321

202: read the complete production interpolation implementation and handoff.
The two-value storage, finite inclusive alpha check, callback-before-metadata
commit, explicit hold/resume, authoritative commit behavior and snap generation
match the documented contract. No additional material defect was established.
Root registration is present. Its source, registration and allocation-test hashes
match the immutable compact release source inventory exactly. That release built
the registered engine for Wasm in both runtime and content variants; this is
compilation evidence, not game adoption or runtime execution of every new method.
The agent's 15 behavioral tests, two allocation tests and seven doctests retain
their own exact command evidence. Marble Run and other consumers remain later
owned tasks; no existing game was silently migrated here.

321: reran `python3 -B scripts/test_api_index.py`; all 26 tests passed. Prepared
inputs for ferravex-core, generated fresh library Rustdoc with Cargo offline,
locked and two jobs into `321-parent-docs`, then built a new index. It contains
78 declarations, with eight unsupported pages explicitly skipped. Exact search
for GameSession and fuzzy search for GameSesion each returned the real
`ferravex_core::session::GameSession`, its declaration, source lines 144–147,
and verified document/input hashes. Evidence is in `321-parent-exact.json`,
`321-parent-fuzzy.json`, `321-parent-tests.log` and `321-parent-docs.log`.
Feature metadata remains caller-declared, as designed. Later source/docs edits
intentionally invalidate this demonstration index; it is not a permanent current
API artifact. Final whole-library indexing, wrapper commands (task 322) and shared
validation remain integration handoffs. A parent validator dry run confirmed
`python.test_api_index` is already discovered by the existing test-file glob, so
no shared registration patch is needed. After subsequent roadmap/source edits,
the original index correctly returned `stale_index` (exit 2); the captured result
is `321-parent-stale-after-dispatch.json`.

Both numbered tasks are returned with bounded parent evidence. Neither is marked
fully integrated across all downstream consumers or platforms.
