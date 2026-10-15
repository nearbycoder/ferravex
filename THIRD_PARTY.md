# Third-party components

Ferravex source is MIT licensed. The default ferravex facade, core, FPS authority and owned examples depend only on project crates and Rust std. Run scripts/audit-owned.py to inspect that boundary.

The optional legacy renderer uses Macroquad/Miniquad, and optional legacy physics adapters use Rapier. Those crates and their transitive dependencies retain their own licenses. Cargo.lock pins their versions; their original license files are distributed alongside the documentation site's compiled legacy examples in /third-party/.

The generated /third-party/index.json lists package versions, declared license expressions, repository links and copied license filenames. It is produced from Cargo metadata and original registry package files during the site build. No third-party code is relicensed as Ferravex.

Rust std, browser APIs, system graphics services and development tools are platform/build dependencies, not project-owned implementations.
