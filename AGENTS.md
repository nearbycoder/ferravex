# Working in Ferravex

[Journey400](docs/JOURNEY_400.md) defines tasks201–400. New numbered dispatches are paused at the user's requested stopping point;117 planned tasks are deferred. Finish and validate only the already-dispatched batch until the user resumes it.
[Journey200](docs/JOURNEY_200.md), tasks 001–200, retains its unfinished final
integration and assigned ownership. Preserve all Journey50/Journey200 changes.
Read the assigned journey's plan and contract; do not duplicate or overwrite an
active owner. Journey400 agents write `target/journey-400/receipts/NNN.md`;
Journey200 agents retain `target/journey-200/receipts/NNN.md`. The parent owns registrations outside assigned
paths, task status, shared builds/staging and final integration. Implementation
agents must not delegate further, commit, create branches/PRs, deploy, schedule
work or edit another task's paths. Read your numbered task and dependency
receipts, then check actual source, tests and registrations; a returned receipt
does not establish combined integration.

Read [CONTRIBUTING.md](CONTRIBUTING.md), [DEPENDENCIES.md](DEPENDENCIES.md), and the
[coding-agent guide](docs/AGENT_GUIDE.md). Inspect the affected implementation and
its tests before designing a new API; substantial reusable systems already exist.

- Keep the resolved Rust graph first-party/std-only, including optional, test and
  target-specific dependencies. Python tools use stdlib; game hosts use browser
  APIs directly. The documented website-only Qikbar exception is not permission
  to add engine/game dependencies.
- Follow the task's path ownership. Preserve unrelated and concurrent edits;
  avoid repository-wide rewrites, lockfile regeneration and generated artifacts.
  Report a concrete requirement to parent before changing manifests or Cargo.lock.
  Prefer additive APIs; improve an existing API's demonstrated gap instead of
  duplicating it. Wire-format changes require explicit versioning and coordinated
  decoder/test/documentation changes.
- Keep simulation independent of rendering, wall clocks and platform I/O. Use
  fixed ticks, consume gameplay input edges only on actual ticks, and emit
  feedback from accepted game events. Local menus must not stop authoritative
  network/replay progression. See the guide for restart and pause details.
- Validate narrowly: `cargo test -p PACKAGE --offline --locked -j 2`, adding the
  relevant test/filter/features. Format only owned files; use check-only
  formatting when reviewing. Run `python3 -B scripts/audit-owned.py` for dependency
  changes. Do not run workspace builds, broad tests or shared dist/site generation
  as an implementation agent. Report exact checks and limitations; compilation
  is not a playtest. For a cross-owner failure, record the command and first
  relevant error, continue independent work and hand off without patching or polling.
- Add behavioral regression tests for changed semantics and rustdoc examples for
  public APIs. Keep bounded storage, error handling and allocation-sensitive
  paths intact. Do not infer platform support or determinism from old results.
- Put tests inline in owned files or a unique `tests/journey400_NNN.rs` (or the
  assigned Journey200 equivalent) in the
  owning crate. Mark complete guide examples `rust,check`; type-check and execute
  claimed runnable examples as described in the [guide](docs/AGENT_GUIDE.md#check-documentation-examples).
  Receipts must list changed paths, concrete behavior, exact public APIs, required
  registration, commands/results and unverified limits. Parent owns final status.

The [Journey50 ledger](docs/JOURNEY_50.md) and its
[frozen session contract](docs/JOURNEY_50.md#shared-session-contract-frozen-for-concurrent-implementation)
remain historical references for the preserved baseline. Active ownership and
handoff use the assigned journey above, with cross-journey sequencing owned by
the parent.
