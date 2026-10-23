# Parent integration — disjoint resources (203)

Reviewed the returned receipt and the production error types, five callback
methods, and private disjoint lookup. No actionable defect established.

Duplicate TypeIds are rejected before the safe standard-library disjoint lookup;
missing entries are checked in argument order before any callback runs. The
mixed-reference wrappers narrow the mutable references without changing the
store. Successful callbacks keep ordinary Rust borrow scopes; their mutations
are deliberately not rolled back on a returned error or panic. Shared arguments
also require distinct types, as documented. Existing registration and facade
module exposure already make the API reachable.

The owner reports 24 default-feature and 24 all-feature resource tests, 12 runnable
doctests, one compile-fail check and strict production/test Clippy. Parent inspected
the resource implementation at SHA-256
`ff32af6cd95d64d9fdb591d0ebafa5ab0f5e06c9b67f39c82481f744ab79b792`.

Parent executed:

- `cargo check -p ferravex-engine --all-features --target wasm32-unknown-unknown --offline --locked -j 2`
  passed; log `203-parent-wasm.log`. This compiles the actual registered API with
  the optional content feature, without claiming browser execution.
- Added the five access methods, ResourceAccessError and the runnable
  work-budget/event-queue recipe to `docs/AGENT_GUIDE.md`'s task/API map. Also
  corrected its active journey link while preserving Journey200 ownership.
- `python3 -B scripts/check-docs.py docs/AGENT_GUIDE.md --json` passed 94 local
  links, zero diagnostics; `203-api-map-links.json`. Existing snippets were not
  changed or re-executed by this link check.
- Scoped `git diff --check` passed.

No source registration or manifest changes were necessary. A downstream system
can now use the checked composition recipe. Broad workspace, final facade/client
adoption, allocation benchmarks and cross-platform runtime verification remain
separate work. The frozen Journey200 Marble release deliberately retains the
previous compact snapshot's resources implementation, so it is not evidence for
this new API.
