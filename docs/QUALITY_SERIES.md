# Shared game-quality series

Twenty first-party engine updates, implemented in order with separate pull requests.
Commits and merges use `[skip ci]`; validation runs locally. Systems must work with
native and WebAssembly game hosts, bound runtime work/storage, and retain existing
save/network semantics or explicitly version changed gameplay rules. Physical
platform checks are recorded only when actually performed.

| # | Engine system | State |
| --- | --- | --- |
| 1 | Gameplay tuning profiles | [PR 24](https://github.com/nearbycoder/ferravex/pull/24), merged |
| 2 | Ability system | [PR 25](https://github.com/nearbycoder/ferravex/pull/25), merged |
| 3 | Attributes and status effects | [PR 26](https://github.com/nearbycoder/ferravex/pull/26), merged |
| 4 | Combat feedback controller | Implemented; locally validated |
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

## 3. Attributes and status effects

`attributes::EffectSet<N>` composes additive and multiplicative stat modifiers,
refresh/strongest/capped-stack policies, tag immunities and expiry in fixed storage
(up to 32 effects). Periodic effects aggregate elapsed pulses into at most N reports;
refresh preserves pulse phase, and no post-expiry damage is emitted. Hosts apply
the reports through their authoritative rules. `Vitals` supplies eight resistance
channels, flat armor, shields, bounded health damage and healing. Attribute IDs,
effect IDs and resistance channels are host-defined, without renderer or ECS ties.
Orchard Guard now runs real frost slows and seed flinches through the shared set,
while keeping its public HUD timers and collision-driven path progress.
Local regressions cover stacking/expiry, repeated refresh, periodic boundaries,
immunity, capacity, invalid edits, damage conservation and the existing four-wave
Orchard autoplay. Strict Clippy and Wasm checks run locally; CI stays skipped.

## 4. Confirmed-event feedback

`feedback::FeedbackController<N>` routes accepted fire/hit/hurt/landing/dash/pickup/
explosion events to shared sound, particle-count and animation-reaction responses.
It combines the existing seeded camera-shake implementation with flash, rumble and
optional single-player hit-stop envelopes. It never applies damage, ammo changes
or a simulation time scale. Networked mode forcibly disables hit-stop. Motion,
flash and rumble have independent accessibility scales; turning a channel off
also clears its existing output. Platform rumble and animation/particle playback
remain host adapters, not fabricated device support.

Events are bounded per rendered update and deduplicated over an explicit N-event
session history; reliable transport remains responsible for older duplicate
rejection. Neon Swarm now routes accepted sounds through this controller and
shows a brief damage border. Its gameplay clock is unchanged. Local tests cover
bounded bursts, duplicate events, decay, invalid inputs, channel suppression and
online no-hit-stop behavior; Clippy and native/Wasm consumer checks also run.
