# Contributing to Ferravex

Start with a runnable example and a reproducible behavior change. Keep simulation independent of rendering and platform services. The entire workspace dependency graph must contain only project-owned crates and Rust std.

Coding agents should read [AGENTS.md](AGENTS.md) and the
[agent guide](docs/AGENT_GUIDE.md) for the repository map, API entry points,
fixed-tick/input patterns and debugging commands. The
[Journey400 ledger](docs/JOURNEY_400.md) and preserved [Journey200 ledger](docs/JOURNEY_200.md) define numbered ownership,
dependencies and validation. Preserve all existing uncommitted work, including
the [Journey50 baseline](docs/JOURNEY_50.md). Inspect actual source, registrations,
feature gates and nearby tests before extending an API; a receipt or planned
task does not prove combined integration. The guide's
[task and API map](docs/AGENT_GUIDE.md#task-and-api-map) locates reusable systems.

For a focused change, select the affected package and tests; use at most two
Cargo jobs in a shared checkout:

```sh
cargo test -p ferravex-play --lib input --offline --locked -j 2
cargo test -p ferravex-play --doc --offline --locked -j 2
cargo clippy -p ferravex-play --all-targets --all-features --offline --locked -j 2 -- -D warnings
python3 -B scripts/audit-owned.py
```

These are command templates, not a record of checks already passed. Replace the
package/filter with your scope; narrow Clippy targets/features where appropriate.
Format only owned files. Include behavioral regression tests and rustdoc usage
examples for public APIs; test
error paths, fixed-step catch-up, pause/reset and input edges when affected.
Keep resource bounds, allocation-sensitive paths and existing interfaces intact.
Implementation agents place tests inline in owned files or in the owning crate's
unique `tests/journey200_NNN.rs`. Report the concrete requirement before changing
manifests or Cargo.lock. Report parent-owned module registrations/reexports with
their exact destination and signature rather than editing outside ownership.

Complete guide examples use `rust,check` with an explicit crate type and must be
compiled and run when claimed runnable. The [documentation checker](scripts/check-docs.py)
checks links and can type-check marked snippets; it never runs them. See the
[example verification workflow](docs/AGENT_GUIDE.md#check-documentation-examples)
for artifact selection and execution. Label excerpts and command-output samples
honestly; keep actual results in the task receipt.

The integration owner runs broader checks after concurrent edits settle:

```sh
cargo fmt --all -- --check
cargo test --workspace --all-features --offline --locked -j 2
cargo clippy --workspace --all-targets --all-features --offline --locked -j 2 -- -D warnings
python3 -B scripts/audit-owned.py
node examples/owned-orbit/tests/platform-input.mjs
CARGO_BUILD_JOBS=2 python3 -B scripts/build-site.py
```

The site build regenerates local `dist/` output, including browser games and
Rustdoc; it requires the Wasm standard-library target. Leave generated artifacts
out of source changes. See the [agent guide's local commands](docs/AGENT_GUIDE.md#local-commands)
for targeted tools and local serving. During Journey200, parent owns whole-workspace,
Wasm, shared staging and visual validation. Implementation agents must not run
broad checks or regenerate shared dist, create branches/PRs, commit, deploy,
schedule work or delegate further. Leave a receipt at
`target/journey-200/receipts/NNN.md` with paths, behavior, exact APIs, registration
needs, commands/results and limits. Record the first relevant cross-owner error
and continue independent work without fixing another owner's file or repeatedly
polling it. Parent verifies combined output and updates task status.

Report exact checks and the platforms actually exercised. Describe limitations plainly. Do not claim cross-platform determinism, device support or Internet readiness without evidence. Builds, host tests and visual playtests establish different things.

Do not add registry/Git dependencies, non-workspace path packages, vendored libraries or CDN loaders. See [the dependency policy](DEPENDENCIES.md). Changes to binary protocols, cooked assets or replay formats must update their version markers, decoders, tests and documentation together.

See [FROM_SCRATCH](docs/FROM_SCRATCH.md) and the affected source for feature
boundaries, and [PLATFORMS](docs/PLATFORMS.md) for platform prerequisites and
verification limits. Site content lives in `website/content` and the crate
READMEs; generated output belongs in `dist` and is not committed.
