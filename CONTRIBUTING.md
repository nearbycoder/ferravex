# Contributing to Ferravex

Start with a runnable example and a reproducible behavior change. Keep simulation independent of rendering and platform services. The entire workspace dependency graph must contain only project-owned crates and Rust std.

Before opening a change:

```sh
cargo fmt --all -- --check
cargo test --workspace --all-features --offline --locked
cargo clippy --workspace --all-targets --all-features --offline --locked -- -D warnings
python3 scripts/audit-owned.py
node examples/owned-orbit/tests/platform-input.mjs
python3 scripts/build-site.py
```

Include tests for behavioral regressions, measured performance claims, and the platforms actually exercised. Describe limitations plainly. Do not claim cross-platform determinism, device support or Internet readiness without evidence.

Do not add registry/Git dependencies, non-workspace path packages, vendored libraries or CDN loaders. See [the dependency policy](DEPENDENCIES.md). Changes to binary protocols, cooked assets or replay formats must update their version markers, decoders, tests and documentation together.

See docs/FROM_SCRATCH.md for current feature boundaries. Site content lives in website/content and the crate READMEs; generated output belongs in dist and is not committed.
