# Coding-agent guide

Start at [CONTRIBUTING](../CONTRIBUTING.md) and the [dependency policy](../DEPENDENCIES.md).
Run commands from the repository root. Read the implementation before adding a
system: the engine already has input, feedback, gameplay rules, assets, scenes,
replays, saves, diagnostics and platform hosts. A backlog item is not evidence
that an API exists or that a demo has passed its checks.

## Find the owner and entry point

- [Cargo.toml](../Cargo.toml) defines packages/features; the default member is
  `owned-lab`. Always select `-p PACKAGE` for targeted work. The
  [catalog](../examples/catalog.json) maps fifteen package IDs to browser host
  directories; Courtyard is package `owned-lab` and host `owned`.
- [ferravex](../crates/ferravex/src/lib.rs) is the facade: its small
  [prelude](../crates/ferravex/src/prelude.rs) exposes `GameSession`, fixed timing,
  math, cameras and input; use qualified `render::` calls for drawing/scheduling.
  It also exposes `core`, `World` and engine exports. Existing aliases include
  `ferravex::render`, `ferravex::play`, `ferravex::physics`, and `ferravex::math`.
- [ferravex-core](../crates/ferravex-core/src/lib.rs) owns fixed timing,
  [session lifecycle](../crates/ferravex-core/src/session.rs) and telemetry;
  [ferravex-ecs](../crates/ferravex-ecs/src/lib.rs) owns entities and
  components; [ferravex-math](../crates/ferravex-math/src/lib.rs) owns math types.
- [ferravex-engine](../crates/ferravex-engine/src/lib.rs) composes systems and
  exposes `assets`, `resources`, `events`, `frame_events`, `system_order`, `systems`, `state`,
  `random`, and `diagnostics`. Their module docs are the API reference.
- [ferravex-play](../crates/ferravex-play/src/lib.rs) contains renderer-independent
  rules: `input::ActionMap`, `feedback::FeedbackController`, `scene`, `authoring`,
  `gameplay`, `camera`, `audio`, `persistence`, and `replay`. Engine aliases reuse
  these systems; inspect them before creating a second implementation.
- [ferravex-render](../crates/ferravex-render/src/lib.rs) owns window/GPU/host
  integration. Use `prelude`, `Conf`, `next_frame`, and `game!(config, game)` for
  its existing async games. `config` returns `Conf`; `game` returns a future with
  output `()`. The macro supplies native `run` and Wasm `ferravex_init`. Keep the
  crate's `main.rs` calling its generated `run`, as in
  [Emberline](../examples/emberline/src/main.rs). Runtime adapters include
  `input::PlayerInput`, `retained`, `ui`, `audio`, and `storage`.
- [ferravex-physics](../crates/ferravex-physics/src/lib.rs) provides rotating rigid
  bodies through feature-selected `d2`/`d3` APIs and character motors.
  [ferravex-dynamics](../crates/ferravex-dynamics/src/lib.rs) is a separate
  translational solver; its `PhysicsWorld` is not interchangeable with the
  rotating-body API. [ferravex-graphics](../crates/ferravex-graphics/src/lib.rs)
  includes the software rendering foundation used by the owned reference games.
- [ferravex-net](../crates/ferravex-net/src/lib.rs) and
  [ferravex-fps](../crates/ferravex-fps/src/lib.rs) contain networking and shooter
  simulation. Preserve authority, tick ordering and protocol compatibility.
- [examples/](../examples/) contains game libraries, native entry points and
  controls/feature READMEs. [web/](../web/) holds shared browser hosts and tests;
  [scripts/](../scripts/) holds stdlib developer tools. Website sources live in
  [website/content/](../website/content/) and crate READMEs; `dist/` is generated.

The [Journey400 ledger](JOURNEY_400.md) defines the additional tasks 201–400;
the [Journey200 ledger](JOURNEY_200.md) retains its assigned integration work.
Read the assigned plan for active ownership, dependencies and validation.
The [Journey50 ledger](JOURNEY_50.md) and its
[GameSession contract](JOURNEY_50.md#shared-session-contract-frozen-for-concurrent-implementation)
describe the preserved baseline. The registered
[headless lifecycle module](../crates/ferravex-engine/src/app.rs) exposes
`ferravex_engine::app::{App, Game}` (also `ferravex::app`); the
[quickstart](QUICKSTART.md) and [migration cookbook](MIGRATION_50.md) show how to
use it. Existing async renderer games remain valid. Inspect the implementation
before adopting additional methods.

## Task and API map

Use the task number to find ownership, then follow the source module and its
tests. These are discovery entry points, not claims that every consumer or
platform has passed integration. Task receipts under
`target/journey-200/receipts/NNN.md` describe owner checks; read
`target/journey-200/integration.md` for parent follow-up when present. Neither
replaces checking the current implementation and calling code.

| Task area | Existing entry points to inspect before adding an API |
| --- | --- |
| 001–004: lifecycle and timing | [`core::session`](../crates/ferravex-core/src/session.rs): `SessionConfig::try_new`, `GameSession::try_new`, `from_config`, `step_paused`; [`FrameStats`](../crates/ferravex-core/src/lib.rs); [`app`](../crates/ferravex-engine/src/app.rs): `App::frame_report`, `last_frame_report`; [`time`](../crates/ferravex-engine/src/time.rs): `SimulationTimer`. |
| 005–024: bounded engine systems and ECS | [`engine` modules](../crates/ferravex-engine/src/lib.rs), including [`TickScheduler::drain_ready_iter`](../crates/ferravex-engine/src/scheduler.rs), [`CommandBuffer::apply_report`](../crates/ferravex-engine/src/commands.rs), [`SnapshotRing::bracket_checked`](../crates/ferravex-engine/src/snapshots.rs), [`SpatialGrid::query_into`](../crates/ferravex-engine/src/spatial.rs); [`ecs`](../crates/ferravex-ecs/src/lib.rs). Inspect capacity, ordering and partial-failure contracts individually. |
| 025–037: math, images and physics | [`math`](../crates/ferravex-math/src/lib.rs), [`image`](../crates/ferravex-graphics/src/image.rs), [`dynamics`](../crates/ferravex-dynamics/src/lib.rs), [`physics`](../crates/ferravex-physics/src/lib.rs) and [`character`](../crates/ferravex-physics/src/character/mod.rs). Check the selected dimension/features and the existing checked constructors/query helpers. |
| 038–050, 085–090: game rules | [`play` modules](../crates/ferravex-play/src/lib.rs): `ability`, `action`, `attributes`, `rounds`, `encounter`, `objective`, `environment`, `vehicle`, `racing`, `motion`, `tuning`, `performance`, `ui`, and [`gameplay`](../crates/ferravex-play/src/gameplay/mod.rs). Query previews/status without mutating; perform actual rules before emitting feedback. |
| 051–053, 092: input and hints | [`ActionMap`](../crates/ferravex-play/src/input/mod.rs): checked handles, borrowed descriptors and tick consumption; [`touch`](../crates/ferravex-play/src/input/touch.rs); [`PlayerInput::take_tick`](../crates/ferravex-render/src/input/player.rs) and [`format_hint`](../crates/ferravex-render/src/input/player/editor.rs). Hint reads do not consume gameplay edges. |
| 054–066: camera, feedback, audio and animation | [`camera`](../crates/ferravex-play/src/camera/mod.rs), [`FeedbackController`](../crates/ferravex-play/src/feedback.rs), [`audio`](../crates/ferravex-play/src/audio/mod.rs), [`animation`](../crates/ferravex-play/src/animation.rs). Keep simulation events separate from presentation sampling. |
| 067–077: navigation, AI and particles | [`SearchScratch::step_budgeted`](../crates/ferravex-play/src/navigation/search.rs), [`route`](../crates/ferravex-play/src/navigation/route.rs), [`TargetSelector`](../crates/ferravex-play/src/ai/perception.rs), [`particles`](../crates/ferravex-play/src/particles.rs). A bounded result buffer does not necessarily bound search work. |
| 078–084: content, preferences and replays | [`authoring`](../crates/ferravex-play/src/authoring/mod.rs), [`scene`](../crates/ferravex-play/src/scene.rs), [`persistence`](../crates/ferravex-play/src/persistence/mod.rs), [`replay`](../crates/ferravex-play/src/replay/mod.rs). Read version/error/boundary rules before changing a decoder or interpolation policy. |
| 091, 093–100: runtime adapters | [`render`](../crates/ferravex-render/src/lib.rs): `ui`, `content`, `visibility`, `material`, `retained`, font measurements, `storage`; [`GameAudio`](../crates/ferravex-render/src/audio/game.rs); [`preferences::sync_feedback`](../crates/ferravex-render/src/preferences.rs). Runtime audio and preference state belong to the calling thread. |
| 101–130: tools and browser hosts | [`ferravex.py`](../scripts/ferravex.py), [`validate.py`](../scripts/validate.py), [`check-docs.py`](../scripts/check-docs.py), [`catalog`](../examples/catalog.json), [`web`](../web/). Inspect actual `--help`, source tests and artifact prerequisites before invoking a tool. |
| 131–145: game migrations | Follow package/host pairs in the [`catalog`](../examples/catalog.json), then each game's library, tests and README. Read the caller using a shared API, not just the helper; later host/gameplay passes have their own ownership. |
| Journey400 203: resource composition | [`Resources`](../crates/ferravex-engine/src/resources.rs): `with2_mut`, `with2_mut_ref`, `with3_mut`, `with3_mut_mut_ref`, `with3_mut_ref_ref`. `ResourceAccessError` identifies duplicate or missing types before the callback runs. The `with3_mut_mut_ref` rustdoc recipe combines a work budget, accepted-event queue and shared job configuration. Callback errors and panics retain prior mutations; use distinct resource types, including for shared arguments. |
| Journey400 204: sequential tick systems | [`systems::SystemSchedule`](../crates/ferravex-engine/src/systems.rs), also `ferravex::systems`: register callbacks and explicit flush boundaries, declare ordering, then prepare once. Run once per executed tick. Edits invalidate preparation; stop/error suppresses the remaining callbacks without flushing or rolling back. The module recipe composes actual tick contexts and deferred commands. |
| Journey400 205: fallible deferred commands | [`FallibleCommandBuffer`](../crates/ferravex-engine/src/commands.rs) accepts caller IDs and explicit stop/continue policies. Error callbacks receive owned failures; unexecuted commands remain in FIFO order. A failed command is consumed and may have partially mutated the world. Follow the prepared-flush recipe before using results downstream. |
| Journey400 206: deferred ECS authoring | [`EcsCommandBuffer`](../crates/ferravex-engine/src/ecs_commands.rs) queues typed structural changes while queries are borrowed; drop guards before applying. Prepare sparse entity/component budgets before spawning. Buffer-local spawn tokens survive partial flushes until reset; claim mappings once and emit feedback from accepted records. Reset cancels queued work, preserves spawned entities and expires tokens. Keep one world per epoch; accepted authoring may allocate. |
| Journey400 207: lifecycle-scoped delayed jobs | [`TickScheduler`](../crates/ferravex-engine/src/scheduler.rs) supports bounded `ScopeId` lifetimes. Cancel a scope before replacing its scene/round or resetting its clock; stale and foreign scope handles are rejected. Schedule from individual executed tick contexts, as shown in the respawn recipe. Dropping a handle does not cancel its jobs. |
| Journey400 208: selected pooled actors | [`Pool::get2_mut` and `get_many_mut`](../crates/ferravex-engine/src/pool.rs) validate all selected handles and reject duplicate slots before returning references in caller order. Access uses bounded stack storage; handles still belong to their originating pool. The actor transfer recipe preserves live handles. |
| Journey400 209: distance-ranked targets | [`SpatialGrid::query_radius_into` and `query_nearest_into`](../crates/ferravex-engine/src/spatial.rs) return stored points and squared distances in distance/ID order. All candidates in the bounded circle are considered before truncation; choose cell size/radius within the 4096-cell ceiling and inspect total versus written. |
| Journey400 210: optional ECS components | [`World`](../crates/ferravex-ecs/src/lib.rs): `Option<&T>` and `Option<&mut T>` work in `get`, queries and `get_many`. Absence yields `None`; a conflicting borrow still returns an error. Required members filter entities before borrowing. Follow the `World::query` fixed-step recipe for heterogeneous entities. |
| Journey400 211: component presence filters | [`World::query_filtered`](../crates/ferravex-ecs/src/lib.rs) composes fetched accesses with `With`, `Without`, `All`, `Any` and `Not`. Structural predicates do not borrow marker values. Groups have at most four clauses and may nest; fetched aliases are validated even when the filter excludes every entity. |
| Journey400 212: wider actor composition | [`Bundle` and `Query`](../crates/ferravex-ecs/src/lib.rs) accept up to eight tuple entries; `Entity` counts toward query width. Bundle prevalidation uses stack metadata. The actor recipe combines position, velocity, health, faction and presentation while preserving optional/filter borrowing rules. |
| Journey400 213: prepared spawn batches | [`World::prepare_entities`, `prepare_component` and `try_spawn_batch`](../crates/ferravex-ecs/src/lib.rs) use explicit total sparse-index budgets. Reported batch errors return owned inputs and preserve output/entities. Ordinary spawn growth does not increase prepared budgets, and panic rollback is outside the contract. |
| Journey400 214: actor-template duplication | [`World::plan_duplication`](../crates/ferravex-ecs/src/lib.rs) captures only explicitly selected Clone components after validating every source read. Its owned plan spawns within prepared budgets and returns a fixed old/new map; rejected capacity retains the plan. Remap embedded references explicitly. Capture is a snapshot; Clone side effects and panics are outside rollback. |
| Journey400 215: screen anchors and picking | [`PreparedProjector3D`](../crates/ferravex-math/src/projection.rs) caches a checked inverse with explicit viewport, screen Y and depth conventions. [`graphics::Camera::prepare_projector`](../crates/ferravex-graphics/src/camera.rs) supplies its camera's actual clip depth. Rays begin at the near plane; declare far-plane intent explicitly. Infinite far is currently rejected; logical-pixel conversion and physics distance caps remain with the caller. |
| Journey400 216: checked affine mesh preparation | [`PreparedAffine3D`](../crates/ferravex-math/src/affine.rs) caches an exact-affine model and linear inverse transpose. Point/vector/normalized-normal slice errors leave output unchanged. Mirrored normals keep inverse-transpose orientation; callers choose winding policy. This checked authoring path rejects zero normals and near-affine rows; inspect renderer fallback rules before replacing a draw path. |
| Journey400 217: authored roll limits | [`Quat::try_swing_twist` and `try_clamp_twist`](../crates/ferravex-math/src/quaternion.rs) decompose swing then local-axis twist and preserve aim while limiting signed roll. Inspect the explicit singular flag and documented near-half-turn conditioning; limits use an ordinary interval in radians, with no history or solver policy. |
| Journey400 218: closest segment witnesses | [`segments`](../crates/ferravex-math/src/segments.rs) provides checked 2D/3D point and segment-pair distances with clamped parameters and f64 witness arrays. Convert witnesses to presentation vectors explicitly; rounded f32 positions may not retain the reported distance. Degenerate segments are points, and ties use ordered parameters. Route topology, gate order and swept collision remain caller policies. |
| Journey400 219: bounded cubic authoring | [`CubicBezier3`](../crates/ferravex-math/src/curves.rs) samples checked positions/tangents and tessellates into caller storage with explicit tolerance, work and depth limits. Only `Complete` covers the whole curve; partial outcomes describe an accepted prefix. Pass only that reported prefix to existing `Route3`, requiring completion for a whole authored route. |
| Journey400 220: proximity and placement probes | [`physics::d2/d3::PhysicsWorld`](../crates/ferravex-physics/src/api.rs) has direction-free point/sphere overlap queries with signed surface separation, witnesses and bounded ordered output. Inspect truncation; a child penetration correction is not a whole-world safe destination. Queries read current poses without stepping. |
| Journey400 221: accepted body resets | [`RigidBody::try_teleport` / `PhysicsWorld::try_teleport_body`](../crates/ferravex-physics/src/api.rs) validate before mutation and require an explicit velocity policy. Default reset options clear forces and pending kinematic targets. Snap presentation history from the returned pose only after success; body/collider/joint identity remains stable and contact caches refresh in the next ordinary step. |
| Journey400 222: collider geometry edits | [`PhysicsWorld::replace_collider_shape`](../crates/ferravex-physics/src/world/compounds.rs) accepts checked collider geometry and preserves the selected handle, local pose and existing material/layer/sensor/density settings. Dynamic compound mass is preflighted before mutation; visual geometry changes belong after success. Body-origin point velocity follows existing mass-edit conventions, rather than preserving momentum. |
| Journey400 223: world-space joints | [`WorldJointFrame` / `joint_desc_from_world`](../crates/ferravex-physics/src/api.rs) transform current world pivots/axes into existing local joint descriptions. Choose Preserve or Authored explicitly; preserving initial geometry does not disable motors, limits or later constraint correction. Off-center powered long-lever stability remains a known solver limitation. |
| Journey400 224: authored mechanisms | [`physics::assembly::Assembly`](../crates/ferravex-physics/src/assembly.rs) stages bounded existing body/collider/joint descriptors with explicit local IDs. Validate or insert the whole plan; ordinary capacity/validation errors precede world mutation. Plans are consumed on insertion and success may allocate. Returned mappings connect local IDs to live handles; allocator/panic rollback is excluded. |
| Journey400 225: actor-level sensor occupancy | [`physics::triggers::SensorOccupancyTracker`](../crates/ferravex-physics/src/triggers.rs) caches generational pair identities so compound children yield one enter/final exit even after collider removal. Resolve sensor/actor meaning explicitly. Retry an unconsumed failed suffix or freeze on lost input; reconciliation/reset do not invent gameplay exits. |
| Journey400 226: observed solver impacts | [`World::set_impact_settings` / `impacts`](../crates/ferravex-physics/src/world/cache.rs) opt into bounded outer-step snapshots of peak single-point new normal impulse above explicit thresholds. Consume once per executed tick and assign game-owned feedback IDs. Warm-start replay is excluded; within-substep acceleration from rest can be omitted. Drop counts are rejected pair/substep candidates, not unique lost hits or acoustic energy. |
| Journey400 227: checked radial impulses | [`radial::apply_radial_impulse`](../crates/ferravex-physics/src/radial.rs) selects complete overlap candidates, deduplicates compound bodies and preflights all effects before applying linear COM impulses. Choose per-body or total scalar budget, surface falloff, zero-distance direction and sleeping policy explicitly. Truncation/work/body-capacity errors apply nothing; reuse caller scratch and derive feedback from accepted output. |
| Journey400 228: height clearance | [`character::check_clearance` / `Character::try_set_height`](../crates/ferravex-physics/src/character/motion.rs) query a target shape at unchanged feet and commit only its height after complete clearance. Retain the accepted shape for later steps; motor velocity, jump and support history remain unchanged. Distinguish blocked from exhausted/invalid queries. Current-pose clearance does not sweep future platform motion or add crouch input/protocol fields. |
| Journey400 229: exact snapshot baselines | [`SenderBaselines` / `ReceiverBaselines`](../crates/ferravex-net/src/baselines.rs) retain bounded immutable per-tick bytes separately for sent and installed snapshots. Only an exact retained sender tick can authorize a delta; eviction selects full recovery. Validate the outer session and reconstructed application state before install/ACK, and reset both stores on reconnect. Tick-only ACKs do not prove a session epoch or detect wrong same-length bytes. |
| Journey400 321: local symbol search | [`api-index.py`](../scripts/api-index.py) prepares source provenance, indexes freshly generated Rustdoc and answers bounded exact/fuzzy JSON queries with signatures and source evidence. It rejects stale inputs. Run it directly; CLI forwarding task322 is deferred. Build documentation after prepare and keep inputs stable through indexing/search. |
| Journey400 230: supplied-time authority | [`fps::server_state::ServerState`](../crates/ferravex-fps/src/network/server_state.rs) is the production core behind native and browser GameServer adapters. Supply monotonic microsecond time, fresh host cookies and complete-datagram output admission. Call receive per datagram, poll through overlays, and tick once per executed authority update. Refused output does not undo a tick; inspect last_report on errors before deciding whether to advance again. |
| Journey400 231: supplied-time prediction | [`fps::client_state::ClientState`](../crates/ferravex-fps/src/network/client_state.rs) powers both GameClient adapters. Receive datagrams and poll through overlays; tick only for executed input updates. Accept complete datagrams, retain exact baseline ACKs, and inspect last_report after errors. Reconnect with a fresh host nonce; output refusal does not roll back prediction. |
| Journey400 232: local network inspector | [`local_link::LocalLink`](../crates/ferravex-net/src/local_link.rs) routes bounded original datagrams with supplied time and explicit delay/loss. Run [`local_session`](../crates/ferravex-fps/examples/local_session.rs) to compose production authority/prediction cores. Complete local admission differs from eventual delivery; poll through menus and keep bounded work. |
| Journey400 233: checked arena authoring | [`ArenaBuilder` / `validate_arena`](../crates/ferravex-fps/src/arena_authoring.rs) produce the existing Arena with indexed solid/spawn errors and explicit caps/empty policy. Optional clearance uses the real character Terrain and its implicit ground; exhausted queries cannot authorize spawns. Keep last-good data on rejection and revalidate later public-vector edits. This does not select live safe spawns or change practice rules. |
| Journey400 234: opt-in safe FPS respawns | [`fps::spawn::Policy`](../crates/ferravex-fps/src/spawn.rs) preflights actual 3D clearance and explicit XZ distance/exposure policy over authored positions. Treat no candidate as deferred admission, and emit feedback only for accepted events. Use the separate policy-bound Capture/World replay adapter; legacy practice recordings do not encode this mode. |
| Journey400 237: retained authority output | [`StepOutput`](../crates/ferravex-fps/src/rewind.rs) retains complete ordered event/rejection buffers across checked Match ticks. Prepare48 event/16 rejection capacity, keep state checkpoint-valid, and handle errors before publishing a tick. Deliver borrowed output before reuse; legacy allocating wrappers and safe-spawn replay context remain unchanged. |
| Journey400 239: accepted FPS feedback | [`PlayerStepResult` / `StepTransition`](../crates/ferravex-fps/src/combat.rs) report real reload/jump/fire outcomes; retained authority output adds actor/resulting-tick/full-life identity. Local predictions remain speculative. Confirm feedback once per host session/actor/life/tick, avoid duplicate Shot/sidecar fire cues, and keep silent replay seeks silent. Existing codecs do not carry the sidecar. |
| Journey400 240: authority relocation | [`Match::relocate_player`](../crates/ferravex-fps/src/rewind.rs) preflights a living actor destination, explicit motion/occupancy policy and complete clearance before changing pose/life/history. Snap presentation only on Ok; repeated same-tick moves refresh only that actor. External relocation is not recorded by old Capture: start a new recording segment from the accepted checkpoint. |
| Journey400 241: expiring command outcomes | [`CommandBuffer`](../crates/ferravex-play/src/gameplay/buffer.rs) reports replacement, expiry, gating and command handoff through checked methods; `remaining_life` never consumes or refreshes. Push fresh edges and age only on executed updates. `Executed` means removal from the buffer, so apply real game rules before accepted feedback. Explicitly clear pending work at lifecycle resets. |
| Journey400 242: accepted combo receipts | [`ScoreChain::award_with_receipt`](../crates/ferravex-play/src/gameplay/combo.rs) retains legacy saturation while reporting nominal points and actual helper-total change separately. `try_award` rejects counter overflow before mutation; previews grant nothing. Choose an explicit chain/run/all reset. If a game owns a separate narrower score, its own accepted delta remains authoritative. |
| Journey400 243: progression awards and loading | [`Progression::award_report` / `restore`](../crates/ferravex-play/src/gameplay/progression.rs) preserve saturation-before-promotion and validate exact curve/state snapshots atomically. Reward only promotions from a newly accepted award. Preview and restore reports grant nothing; restore upgrades/objectives/event watermarks consistently or keep a durable reward ledger across rollback. |
| Journey400 244: distinct weighted offers | [`RewardTable::roll_distinct_into`](../crates/ferravex-play/src/gameplay/loot.rs) returns a bounded prefix of unique table indices and exact sample use. Handle budget/table exhaustion, retain offers across redraws and spend only earned points after stale/cap checks. A new call restarts the pool; legacy single-roll sequences remain unchanged. |
| Journey400 245: owed reward delivery | [`PityDelivery`](../crates/ferravex-play/src/gameplay/pity.rs) retains one natural/guaranteed offer across destination refusal. Keep its payload and retry without rerolling; validate pending before insertion and settle the actual result once. Caller-scoped stream IDs, destination ownership and durable recovery remain host responsibilities. |
| Journey400 246: edge-aware charge requests | [`ChargeAction::step_edges`](../crates/ferravex-play/src/gameplay/charge.rs) preserves held/pressed/released independently. Use the released payload for the old action and report power for the new charge; aggregate flags have no sub-tick timing. Consume once per executed tick, apply actual game admission before feedback, and preserve authored auto-charge defaults. |
| Journey400 247: shield state and edits | [`Shield::state` / `reconfigure`](../crates/ferravex-play/src/gameplay/shield.rs) separates broken, delayed, full and recharge eligibility. PreserveValue growth grants capacity only; PreserveRatio deliberately scales charge. Detailed receipts retain nominal damage routing and stored endpoints; accepted advance receipts drive recharge cues. Invalid edits are atomic and current waits never extend. |
| Journey400 248: stagger recovery resistance | [`Stagger::with_recovery_policy`](../crates/ferravex-play/src/gameplay/stagger.rs) opts into grace that rejects poise without extending timers. Only hit_report Onset interrupts; advance_report returns one-shot recovery/grace-end offsets. active() is false during grace. Preserve zero-grace default enemies and author tougher variants explicitly. |
| Journey400 249: projectile contact outcomes | [`ProjectileBudget::target_report` / `wall_report`](../crates/ferravex-play/src/gameplay/projectile.rs) distinguishes duplicates, inactive budgets, final hits, separation and bounces. Supply full host generation IDs after geometry confirmation; budget admission alone is not damage acceptance. Preserve traversal work/progress caps and emit feedback only for accepted game effects. |
| Journey400 250: explainable spawn assessments | [`SpawnSelector::assess_into` / `choose_checked_into`](../crates/ferravex-play/src/gameplay/spawn.rs) report bounded source-index exclusions and caller-authored scores without fabricating a fallback. Read-only assessment does not rotate ties; commit copied selection state only after actual spawn admission. This helper is planar Vec2: full 3D geometry, budgets, obstruction and hostility remain authoritative caller checks. |
| Journey400 251: atomic upgrade choices | [`UpgradeChoices`](../crates/ferravex-play/src/gameplay/upgrades.rs) owns ranks, earned points and one cached offer. Credit only new accepted awards, retain offer IDs across redraws, and submit the original token with a choice. Acceptance spends one point/rank once; stale or invalid choices are atomic. Reset keeps serials; fresh objects require fresh caller namespaces. |
| Journey400 271–272, 286: responsive layout | [`ui::layout`](../crates/ferravex-play/src/ui/layout.rs) reserves safe HUD regions; [`ui::flow`](../crates/ferravex-play/src/ui/flow.rs) lays out bounded content-sized rows; [`Canvas2D`](../crates/ferravex-render/src/canvas.rs) fits the world into the remaining rectangle. Share returned rectangles between paint and input. Handle flow overflow explicitly and submit only the written prefix; helpers do not choose which essential actions to omit. |
| Journey400 273: measured wrapped copy | [`render::ui::text_layout::WrappedText`](../crates/ferravex-render/src/ui/text_layout.rs) borrows copy and measures full line slots; `Ui::wrapped_text` queues it with explicit `LineLimit`. Size cards from measured height or handle hidden lines deliberately. Hidden-line counts describe the line policy, not geometric clipping; immediate-font whitespace conventions remain distinct. |
| Journey400 274: bounded scroll regions | Pure [`ScrollState`](../crates/ferravex-play/src/ui/scroll.rs) and renderer [`ScrollRegion`](../crates/ferravex-render/src/ui/scroll.rs) retain content coordinates, clip paint/input, and reveal focused items. Route enabled input to one region and use a dedicated paint-only `Ui`; the runnable recipe shows measured text and touch composition. |
| Journey400 275: explicit progress meters | Pure [`ui::meters`](../crates/ferravex-play/src/ui/meters.rs) validates continuous values and bounded integer cells; renderer [`MeterLayout`](../crates/ferravex-render/src/ui/meters.rs) reserves complete authored copy and submits paint atomically. Distinguish known zero, Empty and Unknown; supply real telemetry, units and availability instead of parsing displayed text. Handle capacity/layout errors with a readable fallback. |
| Journey400 276: readable stat lists | [`StatRowsLayout`](../crates/ferravex-render/src/ui/stats.rs) measures borrowed label/value/description rows, choosing shared columns or stacked fixed-size text. Inspect required height and hidden rows before submission; select scrolling, expansion or an explicit fallback for essential values. Submission shares paint/inspection rectangles and rolls back with its containing composition. |
| Journey400 277: complete menu/result cards | [`CardLayout`](../crates/ferravex-render/src/ui/cards.rs) borrows title/body/stats and stable action IDs, reserving fixed readable actions before content. Explicit omission reports require caller policy; supply matching `ScrollState` to keep all content reachable. Submit atomically, then finish input and apply accepted game requests outside composition. Fixed card styling and ancestor visibility remain separate concerns. |
| Journey400 278: shared touch drawing/hit geometry | [`input::touch_layout::TouchLayout`](../crates/ferravex-render/src/input/touch_layout.rs) projects stable authored slots through a UI viewport or canvas. Install authored controls, then resolve current saved settings for drawing; installing resolved controls would apply edits twice. Only hidden suffixes are supported. Coordinates are logical pixels; changed layouts cancel captures when rebuilt and installed. |
| Journey400 279: UI versus world input | [`Ui::interaction`](../crates/ferravex-render/src/ui/shared.rs) reports primary-pointer consumption/capture and accepted confirmation sources after finish. Classify fresh scene requests after UI, then queue them until actual ticks. A consumed UI gesture does not retroactively remove unrelated action edges; caller policy must preserve that ordering and cancel scene gestures on blocked input. |
| Journey400 280: agent-readable UI | [`Ui::inspect` / `write_inspection`](../crates/ferravex-render/src/ui/shared.rs) expose opted-in borrowed semantic labels and logical/clipped bounds, with bounded caller-buffer JSON. Finish input before reading focus; inspect registration/enabled/clipped bounds. Bounds are not proof of painted visibility, and unavailable full labels are explicit. This does not execute actions. |
| Journey400 281: complete UI groups | [`Ui::compose` / `UiComposition`](../crates/ferravex-render/src/ui/shared.rs) submit through a restricted composition builder. Errors roll back paint, hit registration, inspection text, clips and modal scope together; ignored direct errors still poison the group. Handle a nested rejection with an explicit small fallback. Input/paint/lifecycle and external callback effects remain outside composition. |
| Journey400 282: accepted-event notices | [`NoticeQueue`](../crates/ferravex-play/src/ui/notices.rs) holds bounded typed payloads with priority, explicit keyed replacement/coalescing and caller-driven expiry. Feed accepted events once, choose saturating numeric merge policy explicitly, and retain mandatory outcomes outside transient notices. |
| Journey400 283: supplied minimap geometry | [`Minimap`](../crates/ferravex-play/src/ui/minimap.rs) uniformly fits explicitly authored world bounds, projects rotated markers and off-map directions, and inversely picks through a logical clip. Omit unknown locations before batching and authorize picked waypoints in the game. Projection edges are closed; pointer panel/clip edges remain half-open. Geometry never reveals or discovers rooms. |
| Journey400 284: atomic resident instances | [`retained::draw_residents_checked`](../crates/ferravex-render/src/retained.rs) preflights a borrowed mesh/model slice before recording any entry. Inspect exact accepted/culled/rejected counts and the failing index. Reported validation/capacity failure preserves buffers while incrementing one rejection-activity counter; allocation/panic rollback is outside the contract. |
| Journey400 285: checked inspector proposals | [`InspectorLayout`](../crates/ferravex-render/src/ui/inspector.rs) borrows one field snapshot and validated theme, submits controls atomically, finishes existing input once and returns a validated change. Keep validators pure; apply Accepted outside the UI and revalidate delayed requests. Numeric precision/units remain caller-owned, invalid/disabled fields are explicit, and every row must fit or select a readable fallback. |
| Journey400 287: presentation camera blends | [`camera::transition::Transition3`](../crates/ferravex-play/src/camera/transition.rs) blends from the displayed Pose3, holds/resumes presentation time and honors explicit cuts. Match lens conventions for positive-duration blends; keep collision observations separate and reset authoritative interpolation after accepted teleports. Handle NumericalLimit with a deliberate cut policy. |
| Journey400 288: optional authored camera rails | [`camera::rail`](../crates/ferravex-play/src/camera/rail.rs) samples bounded cubic/Route3 paths with authored view keys and caller-owned playhead. Require Complete tessellation before replacing a route, freeze/rebase time during presentation holds, honor cut/generation and explicitly return to the live rig. Reduced motion selects the final authored key; it does not complete gameplay. |
| Journey400 289: prepared pose remapping | [`PreparedRetarget`](../crates/ferravex-play/src/animation/retarget.rs) binds real source/target Skeletons plus caller names/rest locals. It validates hierarchy, prepares local right-delta offsets and maps into caller storage without hot name lookup. Use explicit strict/rest-fallback policy, positive uniform scales and target-order world/skin evaluation; it does not perform IK or infer mislabeled pose identity. |
| Journey400 290: opt-in UI themes | [`ui::theme::UiTheme`](../crates/ferravex-play/src/ui/theme.rs) validates caller-owned type/spacing tokens and opaque sRGB semantic palettes. Choose color encoding explicitly for contrast math and a backdrop for alpha; measurements do not prove display accessibility. Apply tokens only through supported style arguments, draw the non-color cues, and reflow larger text. CardLayout currently has fixed styling and does not consume a theme. |
| Journey400 291: atomic typed tick input | [`ActionMap::consume_tick_handles`](../crates/ferravex-play/src/input/mod.rs) validates every handle before consuming edges; duplicates share the pre-consumption snapshot and caller-buffer variants preserve tails. Feed/peek on render frames, consume only for executed updates, and share that snapshot across systems. Existing PlayerInput unknown-name behavior is unchanged. |
| Journey400 292: visible controls and readable remapping | [`PlayerInput::format_visible_hint` / `format_visible_hint_for`](../crates/ferravex-render/src/input/player/editor.rs) validate `VisibleTouchLabel` slot/code descriptors against installed controls before naming touch regions. Resolve current geometry first; labels do not make hidden controls visible. The shared Controls editor wraps full action/binding text across pages, names navigation keys, and requires at least a 304 × 324 logical host surface. |
| Journey400 293: explicit controller calibration | [`input::calibration`](../crates/ferravex-render/src/input/calibration.rs) validates signed axis/radial profiles and submits complete raw samples through the existing journal. Use one producer per slot, select only for the current connection and avoid double deadzones/calibration. Reset restores raw defaults; new epochs discard selection. Automatic device polling is unchanged. |
| Journey400 294: optional touch observations | [`TouchSurface::handle_timed`](../crates/ferravex-play/src/input/touch.rs) reuses immediate controls while returning supplied-time tap/drag/swipe observations after explicit profile opt-in. Use one monotonic time domain and exactly one handler per event. Reset caller drag presentation with captures; consume mapped actions only in executed updates. PlayerInput still uses its legacy untimed path. |
| Journey400 295: live text/contrast themes | [`preferences::ui_preferences`](../crates/ferravex-render/src/preferences.rs) returns a validated UI snapshot with `theme()` tokens. The three browser hosts synchronize `player.uiPreferencesV1` through `ferravex_ui_preferences_v1`; games must opt into theme-driven layout and reflow. World scale and motion/flash preferences remain separate. Handle explicit overflow at large text sizes. |
| Journey400 296: read-only semantic status | [`status::StatusBridge`](../crates/ferravex-render/src/status.rs) publishes game-authored facts and explicitly approved Ui labels with bounded omissions, UTF-8 truncation and lossless IDs. Opt into `publish_browser`, then use [`bindStatusV1`](../web/status.js) from an existing host update. No publisher means unavailable; stale reads retain last validated content. Dispose the reader and clear the same-thread publisher before replacing the game. The bridge executes no actions. |
| Journey400 297: accepted audio batches | [`audio::batch::CueBatch`](../crates/ferravex-render/src/audio/batch.rs) exclusively borrows one `GameAudio` for a presentation frame. Queue only accepted events across executed ticks, then flush once. Coalescing requires equal category and position; critical retention does not guarantee mixer admission. Cancel on pause/reset and release the batch before resuming. |
| Journey400 311: accepted reliable sends | [`EventSender::send_due`](../crates/ferravex-net/src/reliable.rs) lends due packets under packet/payload-byte budgets. Return true only after local outbound admission; refusal leaves that packet and the tail immediately eligible. ACKs remain individual receiver evidence. Payload bytes exclude envelope overhead, and WouldBlock must not be treated as accepted transport output. |
| Journey400 312: retained reliable delivery | [`EventReceiver::admit`, `drain_into` and `drain_ready`](../crates/ferravex-net/src/reliable.rs) separate successful retained receipt/duplicate ACKs from ordered consumption. Full destinations or refused callbacks keep the unconsumed tail. Drain again when downstream space opens, even without a new datagram; preserve the receiver until all ACKed work is delivered. Callback side effects are outside rollback. |
| Journey400 313: reusable snapshot deltas | [`Delta::encode_into`, `between_into` and `apply_into`](../crates/ferravex-net/src/delta.rs) preserve existing inner wire bytes while using caller slices or preallocated delta storage. Full validation precedes writes; success preserves the unused slice tail. Exact baseline/session acknowledgement remains the caller's responsibility; inner-byte reuse does not make the outer packet codec allocation-free. |
| Journey400 314: precise retained history | [`History::bracket_precise`](../crates/ferravex-net/src/history.rs) compares integer tick plus fraction before calculating an approximate interpolation weight. Match exact/interpolated/range/stale outcomes explicitly; clamping is presentation policy, not rewind validation. Supply a separately trusted freshness cutoff and clear history at session reset. Splitting an already rounded float target cannot recover lost precision. |
| Journey400 315: explainable clock samples | [`ClientClock::observe_detailed` / `diagnostics`](../crates/ferravex-net/src/clock.rs) report first rejection reasons and supplied-time probe/sample age without changing rejected estimator state. Unknown/overwritten/consumed probes are deliberately combined. Reset clock and TickUnwrapper with the connection epoch, retain outer session checks and use the same local time domain; nonces are not session identities. |
| Journey400 316: input admission diagnostics | [`InputQueue::push_checked` / `push_timed_checked`](../crates/ferravex-net/src/lib.rs) report invalid, full, newest-duplicate or out-of-order rejection without changing pending controls or watermark. Legacy bool methods already count once. Read queue-local metrics separately from simulation acceptance; coalescing preserves recovered fire aim/time. Pending clear retains the watermark, while explicit session reset forgets it and retains metrics/storage. |
| Journey400 317: checked packet buffers | [`encoded_client_len` / `encode_client_into` and server counterparts](../crates/ferravex-net/src/lib.rs) validate all v5 fields before writing the exact initialized prefix, preserving the remaining slice. `BorrowedEvent` encodes reliable events/commands directly inside send_due without payload cloning. Wire length includes the envelope; send only the returned prefix and accept only complete outbound admission. Encoding does not authorize receipt, snapshot installation or ACKs. |

For example, these read-only searches locate the real tick-input adapter and its
consumers without building or changing files:

```sh
rg -n 'pub fn take_tick|pub fn sample|pub fn format_hint' crates/ferravex-render/src/input
rg -n 'take_tick|step_paused|sync_feedback' examples/physics-2d/src examples/emberline/src
rg -n 'pub mod input|pub use .*PlayerInput|mod player' crates/ferravex-render/src
```

Follow reexports and feature gates all the way to the intended import path.
The facade prelude is intentionally small: checked session types are available
through `ferravex::core::session`, not assumed prelude additions. Public methods
inside a private module may need a reexport; a source file alone is not a usable
API. Prefer adding a checked method, observable outcome or missing edge-case
behavior to the existing system over creating a parallel subsystem.

For retained `render::ui::Ui`, call `begin`, queue the frame's widgets/copy,
resolve input with `finish` or `finish_player`, then call `paint` explicitly.
Finishing input does not submit drawing. Paint a game overlay before
`PlayerInput::draw_controls` so the Controls editor remains on top.

## Keep edits bounded

Read `git status --short`, the assigned paths, nearby tests and feature gates.
Search with `rg`/`rg --files`. Work only within the assigned paths; do not reset,
revert, stage or format unrelated changes. A new module may need a parent-owned
`pub mod name;`: report the exact declaration and destination instead of editing
another owner's crate root. Preserve public interfaces unless an additive API
cannot meet the requirement. Protocols, replay/save formats, authored assets,
exports and smoke flags are compatibility boundaries.

During Journey200, use at most two Cargo jobs and targeted offline/locked checks.
Report a concrete requirement before changing manifests or Cargo.lock. Tests
belong inline in owned files or in `tests/journey200_NNN.rs` under the owning
crate. Do not create branches/PRs, commit, deploy, schedule work or delegate
further as an implementation agent. If another
agent's unfinished code blocks compilation, record the first relevant error,
file and command; continue independent work without patching or polling that
owner. Parent runs whole-workspace, Wasm and visual integration checks. Your
receipt belongs in `target/journey-200/receipts/NNN.md`; report changed paths,
concrete behavior, exact public signatures, registration destinations,
commands/exit results and deferred validation. Record the package, features,
target, test filter, executed test count and failures/skips, plus artifact/log
paths where applicable. A successful command with zero selected tests does not
validate the behavior. Do not mark your task fully integrated or change task
statuses; parent owns combined verification and shared staging.

## Fixed ticks, input and feedback

The renderer supplies elapsed time; simulation receives fixed `dt`. Sample host
input before simulation, execute `FrameStep.steps` updates with `FrameStep.dt`,
then render once using `FrameStep.alpha` between previous/current poses. A frame
may have zero or multiple ticks. Preserve the last tick's previous/current pair
and reset both poses on teleport/restart. Log `dropped_seconds` to distinguish a
catch-up budget limit from slow game rules. `FixedClock::tick()` reflects the
whole batch after `advance`; it is not a distinct ID for each loop iteration.

`ferravex_core::session::{GameSession, SessionState}` wraps that clock.
`new(hz, max_steps, autoplay)` begins Ready unless autoplay is true. `start()`
only starts Ready; `restart()` clears the clock and enters Playing; `reset()`
clears it and enters Ready. `set_paused`/`toggle_pause` only affect Playing/Paused;
`finish(won)` only moves Playing/Paused to Won/Lost. `advance(elapsed_seconds)`
produces no steps or accumulated elapsed time while inactive. It retains any
fractional time accumulated before pausing. Measure elapsed time every host
frame, including inactive frames; if callbacks themselves stop, rebase the host
timestamp on resume. Do not submit the entire suspension as one active frame.

Use `GameSession::try_new` or reusable `SessionConfig::try_new` for checked setup:
finite rates in `1..=1000` Hz and a nonzero catch-up budget. `step_paused()` issues
one tick only in Paused, leaves the stored fraction intact and returns
`alpha = 1.0` for the current inspector pose. It schedules work; the caller must
execute the update. Ready, Playing and terminal states reject manual stepping.

This complete headless example uses exactly representable 64 Hz deltas to show
paused-time discard, manual work and fractional resume without wall-clock sleeps:

```rust,check crate-type=bin
use ferravex_core::session::{GameSession, SessionConfig, SessionState};

fn main() {
    assert!(SessionConfig::try_new(f64::NAN, 4, true).is_err());
    assert!(GameSession::try_new(64.0, 0, true).is_err());
    let config = SessionConfig::try_new(64.0, 4, true).unwrap();
    let mut session = GameSession::from_config(config);
    assert_eq!(session.advance(0.5 / 64.0).steps, 0);
    assert!(session.set_paused(true));
    assert_eq!(session.advance(30.0).steps, 0);

    let manual = session.step_paused().unwrap();
    let mut simulated_seconds = 0.0;
    for dt in manual.ticks() {
        simulated_seconds += dt;
    }
    assert_eq!(simulated_seconds, 1.0 / 64.0);
    assert_eq!(manual.alpha, 1.0);
    assert_eq!(session.state(), SessionState::Paused);
    assert_eq!(session.tick(), 1);
    assert!(session.set_paused(false));
    assert_eq!(session.advance(0.5 / 64.0).steps, 1);
    assert_eq!(session.tick(), 2);
    assert!(session.finish(true));
    assert!(session.step_paused().is_err());
    session.restart();
    assert_eq!(session.tick(), 0);
    assert_eq!(session.advance(0.5 / 64.0).steps, 0);
}
```

If an update finishes the game, gate subsequent updates in the already-budgeted
batch on the authoritative game phase. Restart also resets game entities,
pending input, feedback, camera history and relevant timers; the session does
not own those. Keep simulation-specific phases where they encode real game
rules. For remote matches/replay, continue the authoritative clock while local
UI suppresses player commands; a local overlay must not freeze the match. The
session tick counts the whole issued batch even if you stop early. If using
`App`, its `AppFrameReport` distinguishes scheduled work from executed hooks;
executed includes a hook returning Stop or Err, not only committed updates.
`last_frame_report()` retains a partial report after a hook error. Advance
`SimulationTimer` once per executed tick with caller-supplied simulation time;
do not also advance it with the render delta. Its repeating report bounds
expirations and reports skipped intervals while retaining the duration remainder.

[PlayerInput](../crates/ferravex-render/src/input/player.rs) adapts the
[ActionMap](../crates/ferravex-play/src/input/mod.rs). Choose one pause owner:
`sample_play()` supplies legacy single-player input pause handling;
`sample(enabled)` lets the game/session own lifecycle. Reconcile menu, focus and
controls-panel state before advancing a single-player session. Consume global
menu actions in the render frame; sample held movement once and read gameplay
edges only within actual fixed updates. The following complete headless example
tests a quick tap across a zero-tick frame and two catch-up ticks. It also shows
why a terminal transition must stop the remaining already-issued batch:

```rust,check crate-type=bin
use ferravex_core::session::GameSession;
use ferravex_play::input::{ActionMap, Binding, InputCode};

fn main() {
    let mut input = ActionMap::default();
    let code = InputCode::Key(32);
    let jump = input.try_action(vec![Binding::digital(code)]).unwrap();
    let mut session = GameSession::try_new(64.0, 4, true).unwrap();
    input.feed(code, 1.0);
    input.feed(code, 0.0);
    assert_eq!(session.advance(0.5 / 64.0).steps, 0);

    let frame = session.advance(2.0 / 64.0);
    assert_eq!(frame.steps, 2);
    let mut requests = 0;
    for _ in 0..frame.steps {
        let action = input.consume_tick(jump); // Share this snapshot with systems.
        requests += u32::from(action.pressed);
    }
    assert_eq!(requests, 1);

    let frame = session.advance(3.0 / 64.0);
    assert_eq!(frame.steps, 3);
    let mut executed = 0;
    for _ in 0..frame.steps {
        if !session.is_playing() {
            break;
        }
        executed += 1;
        assert!(session.finish(true)); // The first actual update won the game.
    }
    assert_eq!(executed, 1);
    assert_eq!(session.tick(), 5); // Two earlier ticks plus three scheduled ticks.
}
```

Do not consume `pressed` in the render frame and repeat its boolean for every
catch-up tick: that loses zero-tick taps and repeats multi-tick actions. Use
`PlayerInput::take_tick(["jump", "fire"])` inside each tick to capture a fixed
array of actions; share that snapshot with rules, animation and UI. Duplicate
names in one call share the same snapshot, unknown names are neutral and only
named actions are consumed. `take(name)` also captures both pressed/released
edges for one action. Do not read the same consuming action again for sound or
UI. Use `format_hint`/`format_hint_for` with a caller buffer for remapped labels;
descriptor/hint queries do not consume edges. `look(dt)` consumes relative motion;
call once per render frame and feed absolute aim to simulation/network commands.
Focus/context transitions deliberately discard stale edges and suppress held
buttons until released. See [input details](INPUT.md) and the runtime input tests
for pause, focus, quick taps and controller reconnection.

Route feedback after an accepted shot, hit, pickup or landing. With
[`FeedbackController<N>`](../crates/ferravex-play/src/feedback.rs), call
`emit(id, event)` with a stable, nonzero event ID and handle its
`Result<Option<FeedbackResponse>, FeedbackError>`. Only `Some` should trigger the
associated cue/particles. Its bounded deduplication history supplements network
acknowledgments; it is not an unbounded record of all past events. Advance feedback
with a finite presentation delta within its documented limit and clear it on
restart. Motion/flash preferences change presentation, never damage, scoring or
ammo. Online feedback must not stop the authoritative clock. Reuse
[gameplay rules](GAMEPLAY_RECIPES.md) to decide accepted actions before feedback.

Use `emit_if_accepted` when an acceptance boolean is already available. At the
runtime boundary, call `ferravex_render::preferences::sync_feedback` before
accepted-event emission and presentation sampling on every frame, including
zero-tick/menu frames. Read `feedback_policy()` for effects owned outside the
controller; cancel disabled pending effects so re-enabling does not revive them.
Reduced motion and reduced flash are independent. The common Wasm export
`ferravex_feedback_preferences(motion: u32, flash: u32) -> u32` accepts only 0/1
flags, returning 1 even when unchanged and 0 without mutation when invalid.
Successful publication alone does not prove a game consumed that policy.

`GameAudio` belongs to its creating thread and uses the thread-local mixer; do
not move a controller between threads or wrap one in a cross-thread global. Its
pause/reset discard that instance's gameplay voices; reset preserves pause. UI clicks and
other instances' voices are independent. Admission is not proof of device
audibility. See the actual [audio implementation](../crates/ferravex-render/src/audio/game.rs)
and ownership tests before changing lifecycle code.

Browser player backup export is version 2; imports strictly accept v1's five
settings or v2's additional independent `flash` setting. The storage key remains
`ferravex.player.v1.<game>`. Sparse imports retain unspecified current choices;
Preview/Cancel must not persist changes. Keep the [player](../web/player.js),
[visual helper](../web/visuals.js) and game consumer separate from authoritative
network/replay progression. Use Strike's actual
[`ferravex_replay_finished() -> u32`](../examples/strike/src/replay.rs) signal for
completion; a paused/local-menu frame is not evidence of playback completion.

## Local commands

These commands are scoped templates, not recorded validation results; replace
the package/test for your task and execute only the checks your change needs.
Rust/Cargo must satisfy the workspace toolchain requirements. Python tools need
Python 3.11+ for standard-library TOML support; Node host tests need Node 22+.
Native interactive runs require the platform's display/graphics libraries; browser builds require
the installed `wasm32-unknown-unknown` standard-library target. See
[platform boundaries](PLATFORMS.md); record what ran in this environment.

```sh
cargo test -p ferravex-core --offline --locked -j 2
cargo test -p ferravex-play --lib input --offline --locked -j 2
cargo test -p ferravex-play --doc --offline --locked -j 2
cargo clippy -p ferravex-play --all-targets --all-features --offline --locked -j 2 -- -D warnings
cargo run -p ferravex-play --example gameplay_recipe --offline --locked -j 2
cargo run -p emberline --release --offline --locked -j 2
python3 -B scripts/audit-owned.py
python3 -B scripts/test_audit_owned.py
node examples/owned-orbit/tests/platform-input.mjs
node crates/ferravex-render/tests/browser-input.mjs
```

For the parent/integration owner staging the current browser artifacts:

```sh
CARGO_BUILD_JOBS=2 python3 -B scripts/build-web.py release
python3 -B scripts/serve-web.py --bind 127.0.0.1 --port 8000
```

The builder recreates `dist/web` using the catalog and shared hosts; open
`http://127.0.0.1:8000/` for the catalog. It builds optional authoring hosts
separately to avoid exporting their editing ABI into every game. Do not replace
this with an all-features workspace Wasm build for release artifacts. For docs,
`CARGO_BUILD_JOBS=2 python3 -B scripts/build-site.py` builds web/Rustdoc/site output;
`python3 -B scripts/serve-site.py --bind 127.0.0.1 --port 8080` serves `dist/site`.
These builds produce local artifacts; they do not publish a site.

The [developer CLI](../scripts/ferravex.py) routes the existing local commands.
List games, inspect prerequisites with the read-only [doctor](../scripts/doctor.py),
or preview a targeted Cargo command:

```sh
python3 -B scripts/ferravex.py list --json
python3 -B scripts/ferravex.py doctor --profile web --json
python3 -B scripts/ferravex.py test -p ferravex-core session --dry-run --result-json
python3 -B scripts/ferravex.py run owned-orbit --dry-run -- --autoplay --frames 120
```

CLI Cargo commands use `--offline --locked` and at most two jobs. `test` and
`check` select the whole workspace unless narrowed with `-p`; both still enable
all features when a package is selected. Use Cargo directly for narrower feature
or target choices. `run` defaults to release. Arguments after `--` belong to the
game/test harness.
Doctor's `core`, `web`, `native`, and default `all` profiles select required
prerequisites; passing a profile does not establish that a game runs correctly.

The versioned `--result-json` wrapper emits one object on stdout and streams
child output to stderr. Check `schema_version`, `status`, `exit_code`,
`child_returncode` and `error.code`; retain stderr for the failure details. The
dry-run above has `status: "dry_run"`, `exit_code: 0` and
`child_returncode: null`: no Cargo check ran. Its `plan.argv` is:

```text
["cargo", "test", "--offline", "--locked", "--jobs", "2", "--package", "ferravex-core", "--all-features", "--profile", "dev", "session"]
```

This is the argv field only, not the complete JSON response. `plan.cwd` is the
checkout and `plan.env` contains overrides, not a dump of the process environment.
`list --json` returns a catalog array; helper flags such as `doctor --json` retain
their own meaning. Do not parse human log prose as the result schema.
For an unknown package, the wrapper returns exit 2 and `error.code:
"unknown_package"`, with both `plan` and `child_returncode` null; no test ran.

The [scaffolder](../scripts/new-game.py) creates a standalone 2D or 3D project
with a local facade dependency and its own workspace/lockfile:

```sh
python3 -B scripts/ferravex.py new moon-garden ../moon-garden --template 2d --tick-rate 64 --window-title "Moon Garden" --dry-run
```

Remove `--dry-run` only when creating the assigned project. The destination must
not exist and its parent must exist. Use `--template 3d` for the 3D view;
`--tick-rate` accepts finite 1–1000 Hz and `--window-title` accepts 1–120 printable,
nonblank characters. Creation does not register a new workspace member or catalog
entry; those edits need their own ownership. Follow the generated README: from
that project directory,
`cargo test --offline --locked --jobs 2` checks the rules, and
`python3 -B scripts/build-web.py` builds release Wasm and stages `target/web`.
That helper always uses the generated project's target directory. Serve it with
`python3 -m http.server 8000 --bind 127.0.0.1 --directory target/web`.
The native launcher requires a display; the rule tests do not.

The [catalog audit](../scripts/audit-catalog.py) checks source/host consistency;
the [validation runner](VALIDATION_50.md) combines checks and records JSON/logs.
Preview scope before asking parent to execute a validation plan:

```sh
python3 -B scripts/validate.py --mode changed --changed-path docs/AGENT_GUIDE.md --dry-run --json
```

Changed mode requires explicit repository-relative paths or a NUL-delimited file
via `--changed-paths-file`; include untracked files and both sides of renames.
It does not discover changes from Git. Changed mode can select broad checks and
rebuild shared Wasm/site/native output, so it is not a shortcut around ownership.
Its schema-2 report distinguishes `summary.complete` (selected checks) from
`summary.full_complete` (fresh unfiltered full validation). Dry runs execute no
checks and establish neither. `--require-complete` covers only selected checks.
Without `--group`, fast mode also runs workspace Rust checks/tests. The parent
owns these broad runs during Journey200.

For local source work, choose a relevant script/test directly. For example,
`python3 -B scripts/audit-catalog.py --require-session` checks static session-use
evidence, and `node web/tests/visuals.mjs --source-only` checks its source harness.
The default visuals/details suites consume staged artifacts; don't run them
against stale dist and call that source validation. Use `--help` on each Python
tool for its flags and `--dry-run` on the CLI/runner to inspect planned commands.
Focused tool suites include `scripts/test_ferravex_cli.py`, `scripts/test_doctor.py`,
`scripts/test_new_game.py`, `scripts/test_catalog.py`, and
`scripts/test_validate.py`; run them with `python3 -B`.

## Check documentation examples

Complete runnable Rust snippets in this guide use `rust,check crate-type=bin`
and contain `fn main()`. The [checker](../scripts/check-docs.py) defaults to
edition 2024 and crate type lib when omitted; it does not wrap fragments in a
function or infer dependencies. Mark complete library examples with
`rust,check crate-type=lib`. Label noncompiling API signatures/excerpts as such
and leave them unmarked. Ordinary `rust`, `ignore`, `no_run` and `compile_fail`
fences are not selected by this tool.

Run local link checks and extract the exact marked sources into a new,
task-specific directory (the example destination must not already exist):

```sh
python3 -B scripts/check-docs.py AGENTS.md CONTRIBUTING.md docs/AGENT_GUIDE.md --json
python3 -B scripts/check-docs.py docs/AGENT_GUIDE.md --extract target/agent-guide-examples-178
```

The first command reports files, links, marked example counts and diagnostics;
without `--check-rust`, each example remains `not_checked`. Exit 0 means the
requested checks passed, 1 means findings/compiler/output failures and 2 means
invalid options. Local paths and anchors are checked; external URLs are not
fetched. Default anchor style is the site's convention; use `--anchor-style
github` when checking GitHub heading anchors.

For this guide's two binaries, build only `ferravex-core` and `ferravex-play`
libraries automatically with the separate [trusted-example runner](../scripts/check-guide-examples.py):

```sh
python3 -B scripts/check-guide-examples.py docs/AGENT_GUIDE.md -p ferravex-core -p ferravex-play --output target/agent-guide-run --target-dir target/agent-guide-run/cargo --json
```

The output directory must be new. This command compiles and executes marked
binary examples with timeouts, selects native artifacts from Cargo JSON, and
retains source hashes, commands and results. It runs trusted repository code
with normal process permissions. The full validator's `rust.guide-examples`
step uses a fresh directory for all eight selected guides on every run.

For manual metadata-only checks, build only those two
libraries with `cargo build -p ferravex-core -p ferravex-play --lib --offline
--locked -j 2 --message-format=json`. Use a task-specific `--target-dir` to keep
artifacts isolated. Select the fresh `.rmeta` paths from that command's
`compiler-artifact` records for type-checking; their parent `deps` directories
contain the matching transitive artifacts. Do not select arbitrary hashed files
left by an older build. The checker needs repeatable
`--extern ferravex_core=PATH` and `--extern ferravex_play=PATH` arguments together
with `--check-rust`; it adds the externs' parent directories to dependency lookup.
No renderer, facade or engine extern is needed for these examples.

`--check-rust` only runs `rustc --emit=metadata`, even for a bin. To establish
runtime behavior, compile each extracted `manifest.json` entry again with its
edition/crate name/type, the `.rlib` externs from the same Cargo records and
`-L dependency=PATH` for the recorded `deps` directories. Omit `--emit=metadata`,
select a binary output and execute it with a timeout.
Record both compilation and execution exit codes and the extracted source hash.
Manifest argv are type-check commands, not runnable-example commands. No example
here needs a display, audio device, network or sleep. A passed assertion validates
these injected input/time cases on the tested host; it does not establish
cross-platform determinism or a working browser game.

When introducing a guide, report its exact path and required externs to parent
for `scripts/validate.py::DOC_CHECK_PATHS` and any Rust-example validation wiring.
Do not edit that shared validator from a documentation-only task. Link validation,
metadata compilation and binary execution are separate evidence; a receipt must
say which actually ran.

## Debug the failing boundary

- **Compilation blocked outside ownership:** capture the first relevant error,
  command, feature/target and file location. Continue independent edits/checks
  without patching another owner's source, weakening tests, changing features to
  hide the failure or regenerating Cargo.lock. Send parent the exact required
  declaration/fix and keep the blocked command in the receipt even if an isolated
  check succeeds. Retry when a relevant fix arrives, not by repeated polling.
- **A command fails before tests start:** use CLI `error.code`/child status and
  stderr to distinguish invalid package/flags, missing tools and test failures.
  Inspect the selected doctor profile and the tool's `--help`. Offline failure
  is not permission to fetch dependencies or change manifests. Preserve a failed
  command's output; don't report a dry-run or old passing artifact in its place.
- **Wrong simulation speed or repeated actions:** reproduce at 30/60/144 Hz and
  with a long frame; inspect fixed tick count, dropped time and edge consumption.
  Test zero-tick taps, multiple catch-up ticks, pause/resume and restart. Inject
  elapsed time and inputs in headless tests rather than sleeping.
- **Input dies after a menu/focus change:** inspect `sample(enabled)`,
  `gameplay_active()`, `controls_open()`, context transitions and pointer capture.
  Separate global menu edges from gameplay edges and keep remote ticks running.
- **Wasm loads but gameplay fails:** check the actual host path from the catalog,
  HTTP status/MIME for the module, browser console and exported ABI. Loader/input
  mocks test their boundary; they do not prove the actual module rendered.
- **Black frame or missing native window:** distinguish a missing display/driver
  from simulation failure. Use the owned headless hosts for software checks;
  they do not validate the thirteen shared-renderer games. With a working display,
  build only your package and use `python3 -B scripts/smoke-games.py --help` to
  choose its profile/game/output flags. Inspect a fresh capture, not an old PNG.
- **Malformed scene, asset or save:** use `authoring`, `assets` and `storage`
  error results, preserve the last-good state, and test failure before success.
  See [scenes](SCENES.md), [saves](SAVES.md) and [replays](REPLAYS.md). Do not
  silently change a serialized format to fix a decoder.
- **Performance or capacity regression:** report measured work and the reached
  bound using existing telemetry/diagnostics; preserve error/backpressure
  semantics. See [performance](PERFORMANCE.md). A compile check cannot establish
  frame rate, allocation behavior, device support or cross-platform determinism.
