# Scoped parent integration:205 and274

Both terminal child results were harvested with no pending runs. Reviewed
receipts and actual command failure/panic/tail semantics plus registered scroll
API/consumer contracts. Existing module registrations are present.

The missing scroll source that blocked205 is now implemented. Parent repaired
the reported shared.rs test-only needless_range_loop using array enumeration,
preserving every clip-bound assertion. No production behavior changed in that
shared file. Exact source hashes are in integration-205-274/source-hashes.json.

Nine package integration commands pass, with exact argv/logs in that directory's
report.json: command library tests;205 integration tests; all-feature command
doctests; all-feature205 tests; all-feature renderer UI tests; all-feature274
integration tests; strict all-feature engine and renderer Clippy (renderer
includes all tests); all-feature engine/play/render Wasm compilation.
Allocation/panic/recipe evidence remains scoped to each child receipt.

This resolves the concrete cross-owner compilation/lint blockers and releases
registered downstream use. It does not claim game adoption, physical devices,
GPU/browser behavior or a combined staged Journey400 release. Journey200's
served build still uses its immutable accepted shared sources; these new APIs
are not silently included in those artifacts.
