# Shared game-quality series

Twenty first-party engine updates, implemented in order with separate pull requests.
Commits and merges use `[skip ci]`; validation runs locally. Systems must work with
native and WebAssembly game hosts, bound runtime work/storage, and retain existing
save/network semantics or explicitly version changed gameplay rules. Physical
platform checks are recorded only when actually performed.

| # | Engine system | State |
| --- | --- | --- |
| 1 | Gameplay tuning profiles | [PR 24](https://github.com/nearbycoder/ferravex/pull/24), merged |
| 2 | Ability system | Implemented; locally validated |
| 3 | Attributes and status effects | Pending |
| 4 | Combat feedback controller | Pending |
| 5 | Attack timelines and telegraphs | Pending |
| 6 | Squad tactics and cover selection | Pending |
| 7 | Encounter director | Pending |
| 8 | Navigation meshes and traversal links | Pending |
| 9 | Vehicle and flight controllers | Pending |
| 10 | Racing AI | Pending |
| 11 | Richer physics materials | Pending |
| 12 | Fast-body collision robustness | Pending |
| 13 | Breakable mechanisms | Pending |
| 14 | Camera anticipation and authored zones | Pending |
| 15 | Procedural contact animation | Pending |
| 16 | Procedural environment builders | Pending |
| 17 | Acoustic zones and adaptive music | Pending |
| 18 | Editable touch and accessibility layouts | Pending |
| 19 | Objective and puzzle graphs | Pending |
| 20 | Adaptive performance and playtest diagnostics | Pending |

## 1. Gameplay tuning profiles

`ferravex_play::tuning` (also `ferravex_engine::tuning`) separates validated motion,
difficulty and assists from graphics settings. Rally and arena presets preserve
current handling by default. Load/save uses the existing versioned typed save
format; edits validate before replacement and incompatible host rules IDs fail.
`fingerprint()` identifies the complete rules configuration for record/replay
contexts. Applying profiles, hashing and reading values allocate nothing.

Pocket Rally consumes acceleration, braking, speed, steering response and traction;
optional steering assistance follows the actual track tangent. Its reset retains
the profile, and its default record context includes the profile identity. Neon
Swarm consumes speed/input response and difficulty-scaled contact damage, with a
validated custom-profile constructor. Hosts must include custom fingerprints in
custom record contexts and adopt edits at safe reset/checkpoint boundaries.
Assists are opt-in host capabilities; setting an aim or jump-buffer field does
not silently add aim/jump behavior to games that do not implement it.

Local checks: shared profile serialization/invalid-edit/rules/record-identity
regressions, real Rally acceleration and existing complete-race tests, existing
Neon progression, strict Clippy and native/Wasm compilation of changed consumers.

## 2. Abilities and resource reservations

`ferravex_play::ability` / `ferravex_engine::ability` offers fixed-capacity ability
banks (up to 64 configured slots), sequential charge recharge, resource costs and
regeneration. Instant activation is atomic. Begin/commit/cancel supports delayed
abilities, single-use reservations and exactly-once refunds. Hosts supply enabled
conditions, unique generational owner IDs and the actual gameplay effect; tokens
must stay with their originating owner/bank/resource account. Timers use simulation
time and charging work is bounded by slots, without per-update allocation.
Neon Swarm's actual shot and dash actions now use this system; its HUD reads the
shared dash timer. Cancellation, denied activation, resource conservation, stale
and foreign tokens, charge saturation and clock partitioning have local regressions.
