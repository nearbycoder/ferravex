# Journey400 presentation, gameplay reuse, and agent-workflow proposals

Research snapshot: 2026-10-22, against the current shared working tree, including uncommitted Journey200 work. This is supplementary research, not an implementation receipt. **50 candidate tasks** follow. `P01`–`P50` are local proposal identifiers; the parent should deduplicate across research reports and assign the final **201–400** numbers. This report does not reserve any of those numbers or change task status.

Only this report was written. No sources, manifests, lockfiles, registrations, staged artifacts, or other reports were edited. No build, test suite, staging, delegation, commit, deployment, or browser session was run. Validation below is proposed acceptance work, not a claim of checks performed.

## Five priorities

1. **P01 — Reserved HUD regions.** Make the boundary between the fitted game world and readable screen-space UI a reusable composition operation. This has consumers across the whole catalog.
2. **P08 — Shared HUD/touch geometry.** Preserve the successful Emberline/Neon approach while removing repeated projection and channel-slot bookkeeping from future demo work.
3. **P13 — Explicit buffered-command admission.** Demos need different policies: Neon retains a dash request, Orchard keeps the original selected plot, and Siege deliberately discards a shot during reload. Make those choices explicit without changing defaults.
4. **P31 — Executable recipe catalog for agents.** Give agents a small, checked way to find an existing solution, its actual imports/features, a real consumer, and a narrow verification command before designing another subsystem.
5. **P37 — Neon upgrade choices as an adoption milestone.** A bounded between-wave loadout choice gives progression, upgrade offers, projectile rules, responsive menus, and accepted feedback a visible end-to-end consumer. It is separate from the active portrait supplement.

P01/P08/P13/P31 can begin independently in separate owned paths. P37 follows its selected gameplay/UI prerequisites. “Priority” does not grant permission to modify an active Journey200 owner's files.

## Evidence and exclusions

Read the root `AGENTS.md`, `CONTRIBUTING.md`, `DEPENDENCIES.md`, `docs/AGENT_GUIDE.md`, and `docs/JOURNEY_200.md`. Read all fifteen example READMEs and inspected their actual simulation, presentation, host, and regression-test entry points. Shared source inspection included `ferravex-play` UI/input/gameplay/audio/authoring, renderer canvas/UI/font/input/audio/content, and the developer scripts and their tests.

Path shorthand in the entries is explicit: `gameplay/*.rs` and `authoring/mod.rs` are under `crates/ferravex-play/src/`; `ui/shared.rs`, `content.rs` and renderer `ui.rs` are under `crates/ferravex-render/src/`; `<demo>/src/...` means `examples/<demo>/src/...`. A bare `src/...` in a demo entry is relative to that entry's named example. The proposed `crates/ferravex-play/src/ui/` child-module directory does not exist yet; the current implementation is `crates/ferravex-play/src/ui.rs`. Parent-owned registration would connect any new child modules.

Existing capabilities that these proposals must preserve:

- `crates/ferravex-render/src/canvas.rs::Canvas2D` already fits an aspect-preserving world and projects/unprojects input, including rejection of letterbox input.
- `crates/ferravex-play/src/ui.rs` already has safe insets, `Anchor::place`, `Anchors::resolve`, `Viewport::fit`, **`Viewport::responsive`**, `slots`, **`hud_slots`**, `fit_label`, clipped hit testing, capture, and directional focus. `hud_slots` reflows equal cells and clips excess rows; another equal-cell responsive grid would be redundant.
- `crates/ferravex-render/src/ui/shared.rs::Ui` already owns reusable text/command storage, clips, a modal blocker, screen-space painting independent of the world camera, wrapping text, fixed-size ellipsized labels/buttons, and keyboard/gamepad/touch interaction. Budgets are 1,024 commands/widgets, 32 KiB text, 196,608 estimated vertices, and 16 clip levels. `ui.rs::ButtonOptions` already supplies enabled/selected states. Preserve these bounds.
- `crates/ferravex-render/src/font.rs::Font::try_metrics` and `BitmapFontMetrics` already measure width, baseline, line height, multiline text and unsupported glyphs. `PlayerInput::format_hint` and `take_tick` already provide remap-aware non-consuming hints and actual-tick edge snapshots. No proposal adds another hint type or font-metric system.
- Journey200 already covers checked sessions/physics, availability/interaction/wave/objective previews, scene schema JSON, reload diagnostics, voice admission, live preferences, and structured developer-command results. These are dependencies, not new tasks.
- **Emberline 186 already added `examples/emberline/src/compact.rs::CompactHud`.** Its source uses separate narrow-screen overlay geometry and common drawing/touch rectangles. The 186 receipt distinguishes source tests from the still-required fresh parent Wasm recheck. Do not count “fix Emberline portrait HUD” again.
- **Neon's portrait supplement is active.** `examples/neon-swarm/src/screen.rs::ScreenLayout` is already present with responsive HUD regions, a touch dock, and the original fitted world. Treat it as baseline/in-flight work. P01/P08 describe later extraction/adoption, not a second portrait fix or an assertion that this source has passed integration.
- Siege already has a collision-stopping ballistic arc; Orchard already has tower stats, upgrade affordability, wave previews and target/slow markers; Pocket already has a saved 20 Hz best-lap ghost and bend/recovery cues; Marble already has four courses, ordered checkpoint splits and per-course comparisons. Orbit already has charge, pity drops, magnets, ball saves and precision returns. The demo proposals below add different behavior.

## Acceptance conventions

Every candidate needs one bounded implementation owner, compiling rustdoc for public APIs, and at least one real production adoption or explicitly identified first recipe. Preserve std/first-party-only Rust, stdlib Python and browser-native APIs. Proposed names below are design sketches, **not existing APIs**.

- **PLAY:** `cargo test -p ferravex-play --lib <owned-module-filter> --offline --locked -j 2`, plus scoped doctests for public additions.
- **RENDER:** `cargo test -p ferravex-render --lib <owned-module-filter> --offline --locked -j 2`, plus applicable public doctests. Check layout with pure geometry first; a unit test is not a visual playtest.
- **GAME:** `cargo test -p <catalog-package> --offline --locked -j 2`, adding the owned scenario/filter/features when appropriate. Include actual accepted-input paths, pause/reset, zero/catch-up ticks, and 30/60/144 Hz supplied-frame comparisons where timing changes.
- **TOOL:** `python3 -B scripts/test_<owned_tool>.py` using temporary fixtures, or `node web/tests/<owned-test>.mjs` for host behavior. Test machine-readable output and side effects separately.
- Parent owns cross-file registrations outside assigned paths, shared manifests, all-package builds, shared staging, fresh browser/native visual checks, and final status. A UI adoption requires fresh 320/390/landscape/desktop checks of text, hit regions, focus, resize, and long remaps; record reduced-motion/flash checks and physical-device/audio limitations separately.

## UI composition and input: P01–P12

### P01 — Reserve screen-space HUD regions around a fitted world

- **Current evidence/gap:** `Canvas2D::fit` fits the full supplied screen; `Viewport` maps an independently supplied rectangle. Neither allocates header/footer/action regions and returns the remaining world rectangle. `emberline/src/compact.rs::for_screen`, `neon-swarm/src/screen.rs::ScreenLayout::new`, and `sky-courier/src/lib.rs::hud` perform different local calculations.
- **Bounded addition/paths:** Add a pure `HudRegions`/edge-reservation helper under `crates/ferravex-play/src/ui/`, returning named rectangles, remaining content and explicit insufficient-space results. Add a small renderer adapter for fitting an existing `Canvas2D` into the returned content rectangle. Support four edges and a fixed reservation count, not a general constraint solver.
- **Adoption:** Start with Sky Courier; later extract the established Emberline/Neon compositions after their active work settles. Keep each game's authored world framing and minimum readable text policy explicit.
- **Validate:** PLAY/RENDER: reservations never overlap, order is defined, zero/tiny sizes stay inert, safe-area offsets round-trip, and pointer positions map to the displayed world. Compare short landscape, portrait and ultrawide cases; no silent font scaling.

### P02 — Content-sized rows and wrapping action groups

- **Current evidence/gap:** `ui.rs::slots` and `hud_slots` allocate equal cells. They cannot allocate a short icon/action beside a flexible long label or preserve distinct minimum widths. Demo HUDs consequently hand-place mixed content.
- **Bounded addition/paths:** A caller-buffer `flow` helper under `crates/ferravex-play/src/ui/` taking each item's minimum/preferred/maximum extent, gap and optional grow weight; deterministic wrap order and an explicit overflow count. Limit items and avoid a retained layout tree.
- **Adoption:** Orchard's buy/upgrade/action bar; Physics Lab shape/action controls. Compose on P01 where useful, without replacing `hud_slots` for equal cards.
- **Validate:** PLAY: exact-width boundaries, mixed fixed/flexible items, huge gaps, last-row behavior, stable order and no negative rectangles. Render long action names at fixed size and verify focus uses the returned rectangles.

### P03 — Reusable wrapped-text layout with measured block height

- **Current evidence/gap:** `ui/shared.rs::Painter::text` wraps through the private `Lines` iterator during painting and stops at the rectangle height. Public font metrics measure supplied lines, but do not expose that wrapping decision to callers. `crystal-cavern/src/lib.rs::wrapped_text` must provide a height before it knows the painted line count.
- **Bounded addition/paths:** Extract the existing line-breaking algorithm into a borrowed layout iterator in `ui/shared.rs` or a dedicated renderer text-layout module. Return line spans, required height and truncation/hidden-line counts; offer an explicit maximum-line policy. Reuse existing bitmap metrics and unsupported-glyph behavior.
- **Adoption:** Crystal's briefings/blocker explanations and Marble course descriptions; P07 uses it for card height.
- **Validate:** RENDER: measure/paint agree for spaces, newlines, long words and UTF-8 boundaries; clipping and final-line ellipsis are explicit. Test multi-paragraph copy at 320 px without introducing a second font engine.

### P04 — Bounded scroll regions with focus reveal

- **Current evidence/gap:** `Ui::push_clip` clips paint/input, but there is no scroll state or ensure-focused-widget-visible operation. `hud_slots` turns overflow rows into empty rectangles. The current short menus fit through game-specific sizing; longer help, results and future choices need another strategy.
- **Bounded addition/paths:** Pure `ScrollState` under `crates/ferravex-play/src/ui/`, plus renderer wheel/drag/page-navigation adapter using existing capture and clips. Support one vertical axis, clamped content extent and focus reveal; nested scroll routing can remain out of scope.
- **Adoption:** Marble's course information and Physics Lab inspectors, followed by longer controls/help surfaces. Use stable widget IDs across scrolling.
- **Validate:** PLAY/RENDER: focus moves to offscreen items and reveals them, clipped children cannot activate, dragging cancels on focus loss, and resize clamps offsets without activating a different item. Test exhausted scroll extents and touch cancellation.

### P05 — Consistent meters and segmented progress widgets

- **Current evidence/gap:** `Ui` has panels/gradients/text/buttons but no meter composition. `sky-courier/src/lib.rs::hud`, `siege-foundry/src/lib.rs::draw_hud` and `neon-swarm/src/screen.rs` draw health, route, ammo and recharge status separately.
- **Bounded addition/paths:** Renderer `meter` and `segments` compositions over supplied values/labels, fixed bounds and explicit empty/unknown states. Put reusable geometry under play UI and painting under renderer UI. Support threshold markers and text/pattern cues; do not compute gameplay availability or add smoothing to authoritative values.
- **Adoption:** Dune's existing boost/rearm marker and Sky's six-step route strip first; Siege reload next.
- **Validate:** RENDER: zero capacity, full/empty, out-of-range/NaN rejection, tiny bounds, segment caps and readable non-color states. Assert the exact displayed value matches simulation telemetry during pause and catch-up.

### P06 — Responsive label/value and result-summary lists

- **Current evidence/gap:** `ui.rs::stat` is an immediate fixed-position label/value helper. There is no retained, measured list that changes from two columns to stacked rows. `sky-courier/src/lib.rs::hud` and `dune-skimmer/src/lib.rs` manually compose time, penalties, hull, speed and results.
- **Bounded addition/paths:** A `stat_rows` composition over borrowed labels and formatted values with a capped row count, shared label width, stacked fallback and explicit overflow. Use P03 for variable-height descriptions; retain `Ui::text_fmt` storage reuse.
- **Adoption:** Sky/Dune result cards and both Physics Lab inspectors. Keep metric meaning and units supplied by the game.
- **Validate:** RENDER: negative deltas, minute rollover, large counters, long labels and unavailable values remain distinguishable at narrow widths; row rectangles do not overlap or shrink below the chosen text size.

### P07 — Content-driven menu and results cards

- **Current evidence/gap:** Modal blocking and styled buttons already exist. Games still independently budget title, explanation, stats and action rows: `crystal-cavern/src/lib.rs`, `sky-courier/src/lib.rs`, `marble-run/src/course_ui.rs`, and `neon-swarm/src/screen.rs`.
- **Bounded addition/paths:** A renderer card composition accepting borrowed sections and stable action IDs, with measured title/body/action layout, fixed padding, optional P04 scrolling and a required primary-action region. Reuse `Ui::modal`; no new lifecycle or modal manager.
- **Adoption:** Crystal's start/pause/results cards first, then Sky. Preserve game-specific wording and resume/retry semantics.
- **Validate:** RENDER/GAME: long copy, one/three actions, disabled actions, Controls-open gating and very short screens. Verify each button uses exactly its painted region and the primary action remains reachable without scaling the entire card.

### P08 — One geometry description for visible HUD touch controls

- **Current evidence/gap:** `PlayerInput::set_touch_layout` and `TouchLayoutSettings::apply_in_safe_area` already install/adapt controls. Emberline's `CompactHud::touch_layout` manually projects its drawing rectangles into touch controls; Neon's `ScreenLayout::touch_controls` does similar work. There is no shared adapter from an existing UI/world transform and stable output-slot list.
- **Bounded addition/paths:** Renderer geometry adapters from `UiRect`/`Viewport` and `Canvas2D` to existing `touch::Control`, retaining explicit slot/channel identity. Supply draw/hit geometry from the same result; preserve custom touch overrides and their stored indexes.
- **Adoption:** Extract Emberline's proven conversion after 186 integration, then Neon after its supplement; migrate Pocket's four-button layout separately. This is reuse of completed fixes, not new portrait scope.
- **Validate:** RENDER plus actual `PlayerInput` journal tests: safe offsets/DPR-independent coordinates, minimum-size policy, resize cancellation, hidden controls, custom layouts and unchanged slot mappings. Touching each visible region yields exactly its intended edge.

### P09 — Explicit UI-versus-world pointer ownership result

- **Current evidence/gap:** `UiState` captures/hit-tests widgets and `Ui::finish_player` reads platform input and a global confirm. There is no public per-frame interaction-ownership result for game picking/firing. Orchard and the labs manually reconcile menu/button clicks with scene actions.
- **Bounded addition/paths:** Return or expose a bounded `UiInteraction` result identifying consumed pointer activation/capture and keyboard confirmation, with a documented order for sampling UI then translating remaining scene actions. Additive renderer API; do not consume all gameplay edges or replace `ActionMap` contexts.
- **Adoption:** Orchard plot selection and Physics 2D drop/drag modes. Keep accepted scene commands queued until actual ticks.
- **Validate:** RENDER/GAME: one click cannot both buy and select/drop, dragging off a button cannot fire a world action, disabled/occluded widgets have a defined policy, and unrelated keyboard movement remains held. Cover zero-tick frames and modal close/release.

### P10 — Read-only UI semantic and geometry inspection

- **Current evidence/gap:** `Ui` retains widget IDs, clipped rectangles, labels and state internally; external automation largely sees a canvas. The 186 receipt used screenshot coordinates for Controls cells. Existing runtime diagnostics and catalog capability records do not describe the actual currently composed widgets.
- **Bounded addition/paths:** Opt-in borrowed/caller-buffer inspection of submitted widget ID, role, full label, enabled/focused state, logical bounds and visible clipped bounds in renderer UI. Add a bounded debug exporter adapter only where a host opts in. This is UI state inspection, not another scene-schema export or an action-execution backdoor.
- **Adoption:** P07 cards and both lab inspectors; let agent tests locate actual visible actions and report hidden/overflow rows. Software-rendered Orbit needs a separate adapter if adopted later.
- **Validate:** RENDER: inspection changes neither input nor paint, disabled/fully clipped widgets are identified, long labels remain bounded, stale IDs disappear next frame and opt-out retains existing storage behavior. Parent checks actual exported geometry against a fresh screenshot.

### P11 — Atomic multi-widget composition

- **Current evidence/gap:** Individual `Ui` commands reject atomically, but a card/meter/stat group can append some commands and then hit the text/widget/vertex budget. There is no public grouped rollback/preflight. Current callers frequently ignore individual UI results.
- **Bounded addition/paths:** A bounded composition transaction/checkpoint in `ui/shared.rs`, restoring commands, text, estimated vertices, clip depth and new widget registrations on failure. Preserve existing single-command semantics and avoid cloning the whole UI arena.
- **Adoption:** P05/P06/P07 use it so a rejected group can be replaced by a small explicit fallback, rather than leave half a menu. First production consumer: Sky result card.
- **Validate:** RENDER: force exhaustion at every constituent command; verify no partial group paint/hit region survives and prior groups/focus remain valid. Include duplicate IDs, invalid colors and nested clip restoration; verify warmed storage reuse.

### P12 — Bounded accepted-event notices with priority and coalescing

- **Current evidence/gap:** Crystal has a single `String` notice/time, Pinball coalesces a score notice locally, and Emberline uses separate checkpoint/respawn timers. `FeedbackController` deduplicates event responses but is not a text-notice queue or a visibility policy.
- **Bounded addition/paths:** Renderer-independent `NoticeQueue<N>` in play UI, with caller-owned message payloads, explicit priority/key/coalescing policy, bounded lifetime and dismissal. The host chooses presentation or simulation delta; no hidden wall clock and no accepted-action inference from health/score differences.
- **Adoption:** Pinball's combined recovered-points notice and Crystal's objective messages, followed by engine captions in P26. Preserve must-read result panels outside the transient queue.
- **Validate:** PLAY/GAME: burst saturation retains critical notices, coalescing sums only accepted events, rejected actions cannot masquerade as success, expiry/reset are deterministic and reduced motion does not erase essential text.

## Less-touched gameplay helpers: P13–P24

### P13 — Explicit single-slot command replacement and execution policies

- **Current evidence/gap:** `gameplay/buffer.rs::CommandBuffer<T>` stores one pending command; `push` always replaces it and `take_if(true)` removes it before the host knows whether execution succeeded. Neon uses it for dash; Orchard's host uses custom pending commands to preserve the original plot; Siege intentionally drops reload-period fire edges.
- **Bounded addition/paths:** Add compatible first-wins/last-wins admission variants with displaced/rejected-command outcomes and an acceptance-based take operation that retains the original command on a rejected attempt. Keep the default `push` behavior, one-slot bound and caller-driven expiry. Do not make every rejected action buffer automatically.
- **Adoption:** Neon dash and Orchard's deferred purchase path; document Siege's deliberate no-buffer policy as a counterexample.
- **Validate:** PLAY/GAME: adjacent edges, original plot identity, expiry exactly at a tick, unavailable/rejected attempts, invalid delta, pause/focus clear and once-only spend across catch-up. Cover payload preservation without adding heap allocation.

### P14 — Score-chain transition receipts

- **Current evidence/gap:** `gameplay/combo.rs::ScoreChain::award` returns only awarded points; `advance` returns unit and timeout silently calls `break_chain`. Emberline, Neon and Orbit already use the chain but must compare fields to identify tier/timeout changes.
- **Bounded addition/paths:** Add accepted award/advance result structs with old/new multiplier, awarded points, tier crossing and expiration; optional explicit caller-supplied break reason. Preserve `award` and `advance` wrappers and saturating total behavior. Include remaining-window data needed by a meter in the result, not a second score store.
- **Adoption:** Orbit's volley feedback and Pocket's proposed drift challenge; Emberline may add a readable chain timer without changing its compact layout.
- **Validate:** PLAY: zero-base events cannot refresh, exact tier boundaries, one expiration event, total saturation, break/restart semantics and identical legacy point sequences. GAME verifies notices arise from these outcomes only.

### P15 — Per-level promotion records and award overflow accounting

- **Current evidence/gap:** `gameplay/progression.rs::Progression::award` returns a promotion count, caps work at 1,023 promotions, and retains XP at the cap. Neon loops that count and applies rank zero. There is no ordered per-level reward receipt or report of XP lost to `u64` saturation.
- **Bounded addition/paths:** Add `award_with`/caller-buffer promotion records plus a summary of previous/final level, retained XP and saturation. One callback per earned level, no callback for repeated awards at cap. Keep legacy `award` results and curve arithmetic.
- **Adoption:** Neon's future choice-point awards (P37); a small headless composition recipe demonstrates a multi-level reward batch.
- **Validate:** PLAY/GAME: partitioned versus single awards, multi-level skips, final-level cost, zero award, huge awards, bounded output and exactly-once reward credits. No preview or alternate progression curve is required.

### P16 — Pity rewards with deferred-delivery settlement

- **Current evidence/gap:** `gameplay/pity.rs::PityCounter::attempt` returns only a boolean and immediately updates misses. Orbit's `hit_block` correctly guards `attempt` with `drops.len() < 8`; the natural RNG roll occurs separately. There is no common way to distinguish natural/guaranteed rewards or keep the counter unchanged when delivery fails after admission.
- **Bounded addition/paths:** Add an accepted-attempt outcome and a small transactional delivery method/policy: ineligible/deferred attempts do not advance misses, failed insertion does not consume the guarantee, successful insertion reports natural versus guaranteed. Keep RNG ownership/consumption explicit; do not silently rewind or change existing rolls.
- **Adoption:** Orbit's bounded capsule path, preserving its current sequence unless an explicit rules-version change is chosen. Reuse in future enemy rewards.
- **Validate:** PLAY/GAME: limit one, natural success at guarantee boundary, full pool, insertion rejection, retry and single delivery. Assert current Orbit's full-pool counter behavior before migration; do not label it as an existing lost-guarantee bug.

### P17 — Charge rearming policy and threshold events

- **Current evidence/gap:** `gameplay/charge.rs::ChargeAction::step` automatically begins charging after cooldown while held. `cancel` clears held charge but leaves cooldown. Orbit automatically charges its ready serve. There is no opt-in release-to-rearm mode or explicit full-charge crossing event.
- **Bounded addition/paths:** Add a policy constructor/adapter for automatic versus fresh-hold rearm and a step outcome carrying begun/full/released/cancelled transitions. Keep the current automatic behavior as default and preserve recovery time across cancel.
- **Adoption:** Orbit retains automatic serve charging but consumes one full-charge cue; future charged input recipes demonstrate explicit rearm. P50 can reuse the same charge behavior after an earned paddle catch.
- **Validate:** PLAY/GAME: short tap, full hold, release at minimum, holding through cooldown, menu cancellation, no phantom release, exactly one full-charge event and zero/catch-up frame input.

### P18 — Shield hit/recovery transactions and capacity upgrades

- **Current evidence/gap:** `gameplay/shield.rs::Shield` already absorbs damage and regenerates after a quiet delay. It exposes only `value`, unabsorbed damage and unit advancement; maximum/rate/delay are private and cannot be changed for a run upgrade. Neon has a 20-point shield and already correctly emits shield-only hit feedback.
- **Bounded addition/paths:** Add an absorption receipt (absorbed/overflow/broke), regeneration boundary result, and checked reconfiguration with an explicit preserve-absolute/preserve-fraction policy. Capacity growth must not imply free healing unless requested. Keep the legacy absorption/delay behavior.
- **Adoption:** Neon's chassis choice and a break/recharge cue derived from accepted shield events; use P05 for the resulting values.
- **Validate:** PLAY/GAME: conservation, break once, damage to an empty shield resetting delay, zero-damage no-op, exact recovery threshold, smaller/larger capacities and transactional invalid configuration. Same combat input must retain existing default outcomes.

### P19 — Projectile budget outcomes that explain accepted collisions

- **Current evidence/gap:** `gameplay/projectile.rs::ProjectileBudget` has lifetime target deduplication and pierce/bounce counts. `HitResult::{Ignored,Continue,Stop}` and `wall -> Option<Vec2>` conflate several terminal/ignored reasons; only `alive` is exposed. No demo currently adopts this helper in the inspected paths.
- **Bounded addition/paths:** Add compatible target/wall receipts identifying duplicate target, inactive projectile, accepted hit, exhausted pierce/bounce and separating contact, including remaining budgets. Do not create another collision query or change perfect-reflection defaults.
- **Adoption:** P20's traversal and P37's piercing/ricochet weapon options provide the first live consumer. A standalone helper without that adoption should be lower priority.
- **Validate:** PLAY: repeated generational target IDs, distinct generations, separating/zero/invalid normals, exhaustion and no feedback for ignored contacts. Test invariance of the legacy API's hit sequence.

### P20 — Bounded multi-contact projectile traversal

- **Current evidence/gap:** Neon's `game.rs::update_bullets` already sweeps to the nearest enemy/cover and despawns the shot on its first impact. `ProjectileBudget` governs accepted target/bounce budgets but does not spend remaining motion after a pierce or ricochet. Orbit has its own bounded continuous-impact loop.
- **Bounded addition/paths:** A pure 2D traversal helper adjacent to `gameplay/projectile.rs` that consumes caller-provided nearest-hit queries, a displacement, a contact budget and P19 policy. Return accepted contacts, residual motion and a work-cap outcome. Define tie ordering, initial overlap and separating-contact progress; reuse existing sweep implementations.
- **Adoption:** Neon's optional weapon branches first. Orbit is a later comparison consumer only after proving that its special paddle/armor/return semantics are retained.
- **Validate:** PLAY/GAME: two enemies in one tick, cover before/after an enemy, two walls, duplicate contact, zero-distance contact and cap exhaustion; never tunnel or loop forever. Compare regular/irregular frame traces through actual Neon firing.

### P21 — Single-use upgrade offers and earned-point spending

- **Current evidence/gap:** `gameplay/upgrades.rs::Upgrades::offers` already creates distinct seeded non-maxed offers; `apply` only changes a rank and explicitly requires the host to spend its point. There is no offer lifetime/identity, chosen-ID membership check, or atomic offer consumption with earned points.
- **Bounded addition/paths:** A bounded `UpgradeOffer<K>`/choice-session adapter using existing `Upgrades` and `RewardRng`, with stable offer serial, earned-point balance and an accepted choice receipt. Failed/capped/stale/unoffered choices spend nothing; redraw does not reroll.
- **Adoption:** Neon's between-wave choice (P37), paired with P15 promotion receipts. No new weighted-selection implementation or inventory/crafting preview is needed.
- **Validate:** PLAY/GAME: repeat click/keypress, stale modal callback, capped rank, all-capped pool, fewer choices, zero points, deterministic seed and restart. Acceptance must charge exactly once at a real game transition.

### P22 — Bounded reason-tagged time penalties

- **Current evidence/gap:** `gameplay/trial.rs::Trial<N>` handles elapsed time, ordered splits and medals only. Dune manually adds three seconds per recovery; Sky subtracts five seconds per impact and derives flown/penalty summaries locally. Neither needs another session clock.
- **Bounded addition/paths:** Add a small penalty accumulator in `gameplay/trial.rs` or a child module, keyed by caller-defined reason and accepted event serial, returning charged duration and totals. Support race-added and deadline-subtracted use through caller policy; bound history/counts and surface arithmetic saturation.
- **Adoption:** Dune recovery and Sky impact accounting while their existing authoritative clocks remain in charge. Deduplication horizon must be explicit and appropriate to the event source.
- **Validate:** PLAY/GAME: duplicate event, simultaneous terminal timeout, repeated recovery, pause, restart and large totals. Display driving time plus penalties must equal the existing final total; rejected impacts charge nothing.

### P23 — Crumble-platform transition output

- **Current evidence/gap:** `gameplay/platforms.rs::CrumblePlatform::step` may cross warning→absent→solid in one large update and returns unit. Inspecting only before/after phase can miss transitions. Emberline already uses two crumble platforms and resets them on death.
- **Bounded addition/paths:** Add a fixed-size transition report with phase, local crossing offset and final state, reusing the existing at-most-two-transition bound. Preserve `step` as a compatibility wrapper; accepted warning/collapse/regrowth cues derive from the report.
- **Adoption:** Emberline's existing platforms and the optional relay challenge P36. No replacement platform simulation or wall clock.
- **Validate:** PLAY/GAME: both transitions in one step, exact boundaries, stale continuous contact, invalid delta, death reset and partitioned equivalent states. Ensure reports do not create duplicate collision or cue events.

### P24 — Optional post-stagger resistance window

- **Current evidence/gap:** `gameplay/stagger.rs::Stagger` blocks extension while active, but the next threshold hit can immediately stagger again after recovery. Neon's heavy enemies use this helper and cancel their attack on a threshold hit. The default rule is intentional; there is no authored post-recovery resistance policy.
- **Bounded addition/paths:** Add an opt-in recovery grace/resistance policy and transition receipt to the existing helper, keeping default zero grace. During grace, define whether poise is rejected or accumulated without triggering; choose one explicit, tested policy.
- **Adoption:** A tougher Neon enemy/loadout balance variant after weapon upgrades; show the short resistance state with a steady cue. Do not silently rebalance current brutes.
- **Validate:** PLAY/GAME: concentrated versus spaced hits, exact end of stagger/grace, large steps, invalid inputs and no indefinite stun lock. Default configuration must retain the current regression sequence.

## Audio, content and machine-readable workflows: P25–P35

### P25 — Accepted-cue batching above the existing mixer

- **Current evidence/gap:** `GameAudio::cue_with_outcome`/`at_with_outcome` and mixer voice admission already exist. Orbit coalesces accepted event categories into at most five cues per presentation frame; Pinball also aggregates nearby scoring feedback locally. There is no reusable per-owner cue-batch policy.
- **Bounded addition/paths:** A fixed-capacity cue accumulator in `crates/ferravex-render/src/audio/`, with explicit category coalescing, retained critical cues and admitted/coalesced/dropped counts. Flush through `GameAudio`; pause/reset discards the batch. Do not replace voice priorities, audio-ring recovery or event deduplication.
- **Adoption:** Orbit first, then Pinball/Neon contact bursts, preserving spatial cues where coalescing would lose useful direction.
- **Validate:** RENDER/GAME: multi-tick event bursts, empty render frames, mute versus admission, instance ownership, restart and critical terminal cues. Offline mixing tests establish finite output and counts; actual listening remains a separate check.

### P26 — Gameplay captions from accepted semantic events

- **Current evidence/gap:** Audio cues are synthesized and gameplay notices exist, but `GameAudio` does not carry a caption/meaning channel and the inspected demos have no shared sound-caption facility. A successful voice admission is not evidence that the player heard it.
- **Bounded addition/paths:** An optional bounded caption adapter consuming accepted gameplay event categories with priority, expiry and optional coarse direction, implemented on P12. Captions follow the accepted event even if sound is muted or a voice is denied; they must not infer hidden game information from arbitrary audio positions.
- **Adoption:** Sky gate/recovery warnings and Pinball save/drain/contact categories first. Keep static text available with both motion/flash reductions; a preference can be added through existing settings composition with coordinated storage handling.
- **Validate:** PLAY/RENDER/GAME: muted/unavailable audio, repeated contacts, full queues, paused terminal cues and no duplicate caption on render-only frames. Check readable overlap-free placement and screen-reader host integration separately if included.

### P27 — Source-preserving transform patches for authored scenes

- **Current evidence/gap:** `authoring/mod.rs::Document` parses/compiles scenes but keeps node structures private; success retains no source. Editing is text replacement through content tools. Schema introspection and source-aware diagnostics already exist; they do not safely apply a small structured transform edit while retaining unrelated authoring text.
- **Bounded addition/paths:** An authoring helper limited initially to transforms of explicitly authored top-level node IDs: locate token spans, check expected old value/source fingerprint, apply a bounded non-overlapping patch batch, then reparse/validate before returning text. Preserve comments and unrelated bytes. Explicitly reject prefab-generated paths in this first version.
- **Adoption:** Marble course/platform placement and Orchard plot/path authoring tools; give an agent a precise edit operation without a full serializer or schema redesign.
- **Validate:** PLAY: comments, duplicate/missing IDs, stale source, overlapping edits, UTF-8 spans and invalid final domain values. A failed patch returns no replacement. Cooked bytes change only when the accepted authored values change.

### P28 — Headless game-domain content validation

- **Current evidence/gap:** `examples/marble-run/src/level.rs::Level::parse` and Orchard's equivalent already apply schema plus game-specific checks. `examples/owned-lab/src/bin/ferravex-cook.rs` packs OBJ/WAV/bin assets; it is not a game-scene validator. Browser reload currently discovers a game's invalid geometry through a live session.
- **Bounded addition/paths:** Add a narrow check-content entry point/example to each of Marble and Orchard that calls its actual `Level::parse`, emits structured diagnostics, and exits without a window, session restart or staging. A Python wrapper may select these existing package entry points; do not reimplement their schemas in Python.
- **Adoption:** Agent-authored P27 scene edits and CI for the existing four Marble course files/Orchard scene. Parent coordinates any bin/example manifest registration.
- **Validate:** GAME/TOOL: real valid content, invalid path connectivity/checkpoint bounds, parse errors with locations, missing/oversized file and no mutation of live saves. Expose validation-only semantics; parsing success alone is not proof of course completion.

### P29 — Revision-guarded content submissions at the Rust boundary

- **Current evidence/gap:** `content.rs::queue(source)` stores one pending source without an expected document/generation. `web/content-tools.js` already tracks document identity, dirty state and conflicts and refreshes before submit. The underlying submission API still lacks a transactional compare-and-submit contract for another host or asynchronous caller.
- **Bounded addition/paths:** Add a `queue_if_current(document, generation, source)` operation with explicit stale-document/stale-generation/pending outcomes. Preserve legacy `queue` and its ABI. If exported to Wasm, add coordinated versioned/additive exports and host tests rather than reinterpret existing status values.
- **Adoption:** The content editor can use the stronger boundary after selecting another Marble course; agent-driven editor integrations use it for stale-edit rejection. This extends the existing conflict UI rather than rebuilding it.
- **Validate:** RENDER/TOOL: document switch, accepted reload before delayed submit, two same-generation writers, failed builder, unchanged content and queued-during-builder behavior. The last-good source and pending accepted edit survive every rejected stale request.

### P30 — Discoverable developer-command descriptions

- **Current evidence/gap:** `scripts/ferravex.py::_parser` defines options and forwards helper commands; `--result-json` already describes execution outcomes and `--dry-run` emits argv/cwd/env. Agents still need human help text/source to discover allowed commands, positional arguments, defaults and side effects before constructing a call.
- **Bounded addition/paths:** Add a read-only `describe` command returning command/argument descriptors and execution category from the same parser/explicit helper descriptors. Bound the description and version it; forwarded flags that the wrapper does not understand must remain marked opaque.
- **Adoption:** Planning clients can discover package selection, separators and build/staging effects without trying a failing command. Integrate recipe selection P31 with these descriptors.
- **Validate:** `scripts/test_ferravex_cli.py`: parity with argparse choices/defaults, no subprocess or file writes, unknown subcommand, helper opacity, shell-like literal values and stable order. This is command discovery, not another result-JSON task.

### P31 — Executable recipe catalog with adoption anchors

- **Current evidence/gap:** `docs/AGENT_GUIDE.md` has a prose API map, `scripts/site_search.py` ranks symbols, `audit-catalog.py` records static game capabilities, and `check-guide-examples.py` runs marked examples. None provides a queryable recipe record joining a task intent to exact imports, features, runnable example, current consumer and validation command.
- **Bounded addition/paths:** Add a small explicit recipe manifest under `docs/` and a stdlib query/check tool under `scripts/`, initially covering 10–15 common compositions: fixed-tick edges, pause authority, HUD card, accepted feedback, save, reload, projectile, upgrade choice. Reuse the documentation/example checker for proof; do not attempt a general Rust parser.
- **Adoption:** Agent guide and generated-project guidance can point to recipe IDs; P01–P29 additions should register their first actual consumer. Returns bounded records with source anchors and evidence status, not inferred platform claims.
- **Validate:** TOOL: stale symbol/consumer path, missing example, wrong feature declaration, duplicate ID, changed source hash and nonexecuted examples. An unverified recipe remains queryable but clearly unverified; the tool never converts a static source match into a runtime pass.

### P32 — Implementation-owner-safe narrow check planning

- **Current evidence/gap:** `scripts/validate.py::changed_path_groups` maps changes to coarse groups; changed mode can still run workspace checks and shared builds. The guide explicitly warns implementation agents not to use it as a narrow validator. Existing task ownership is expressed in roadmap prose.
- **Bounded addition/paths:** Add a separate advisory owned-scope planning mode/tool taking explicit owned paths, package and optional test filter. Emit only candidate package-local/source checks plus a distinct parent-integration requirement list; include refused broad/staging commands. Do not weaken or relabel the existing authoritative full/changed modes.
- **Adoption:** All Journey400 implementation agents, especially UI/gameplay changes with parent-owned root registrations. An explicit small ownership manifest can be provided by the parent rather than scraped ambiguously from Markdown.
- **Validate:** TOOL: deleted/new paths, overlapping ownership, cross-package edit, feature-specific source, missing registration and unknown paths. No subprocess/files in plan mode, at most two jobs, `--offline --locked`, and never claim full coverage from the narrow plan.

### P33 — Evidence joins for adoption and fresh visual coverage

- **Current evidence/gap:** Build manifests already hash source/staged bytes; smoke reports already record capture freshness; catalog capabilities distinguish static evidence. Journey200 186 shows the practical gap: source tests passed while the served Wasm still lacked the fix. These separate facts require manual reconciliation per game and feature.
- **Bounded addition/paths:** A read-only stdlib evidence joiner consuming existing build/smoke/validation reports and explicit visual observations, outputting per-feature/per-game states: implemented, source-checked, built-current, visually-observed-current, blocked/unknown. Use hashes and recorded configurations; never use an image filename or receipt's “pass” prose as proof.
- **Adoption:** Parent's 15-demo migration checklist and recipe evidence P31. This aggregates existing provenance, not another hash generator or autonomous browser runner.
- **Validate:** TOOL fixtures: source changes after capture, shared API changes, mismatched profile/Wasm/host hashes, absent observations, stale screenshot and real unresolved limit. Preserve negative findings and distinguish viewport automation from physical-device/audio evidence.

### P34 — Typed Wasm host-contract audit

- **Current evidence/gap:** `scripts/build-owned.py::_content_exports` checks authoring export names, and `scripts/test_template_parity.py::wasm_exports` checks names/kinds. Those checks do not establish function parameter/result signatures or required import types. Actual hosts consume several distinct ABIs, particularly Courtyard and Orbit.
- **Bounded addition/paths:** Extend the existing bounded Wasm section reader into a reusable signature audit (type/import/function/export sections) and compare against explicit per-host contract data. Reuse existing readers rather than add an unrelated parser. No instantiation, build or staging is necessary to audit a supplied module.
- **Adoption:** Runtime/authoring variants and P29/P10 optional exports; include both custom hosts and the shared renderer contract. Parent owns canonical contract registration.
- **Validate:** TOOL: correct name/wrong signature, imported-function index offsets, truncated/oversized LEB values, duplicate sections, optional versus required exports and forbidden authoring exports. Report bytes/hash audited; matching ABI does not claim successful rendering.

### P35 — Precise features, targets and doctests in the developer CLI

- **Current evidence/gap:** `scripts/ferravex.py` appends `--all-features` to check/test and lacks explicit target/feature selection; `docs/AGENT_GUIDE.md` tells agents to use Cargo directly for narrower choices. Structured results already work and should be preserved.
- **Bounded addition/paths:** Add mutually validated `--features`, `--no-default-features`, `--all-features`, `--target`, and test `--doc` selection to the wrapper. Keep existing defaults for compatibility or expose an explicit documented narrow mode; distinguish wrapper flags from arguments after `--`.
- **Adoption:** 2D/3D physics consumers, public API rustdoc checks, and source-only native verification that should not accidentally enable authoring/Wasm paths. P30 describes these options; P32 plans them.
- **Validate:** `scripts/test_ferravex_cli.py`: exact argv, conflicting selections, unknown target syntax, doctest/lib incompatibility, literal feature names, no launch on invalid options and unchanged result schema/child exit propagation.

## One substantive new feature per demo: P36–P50

These proposals are optional additions to the existing playable mode. They are not fifteen repeats of Journey200's generic migration/visual-polish passes. Each feature should adopt the shared compositions it actually needs and retain the existing baseline regressions. All cited example paths are relative to the repository root.

### P36 — Emberline: optional lantern-spark relay

- **Existing behavior/evidence:** `examples/emberline/README.md`, `src/lib.rs::Game` and `src/compact.rs`: three course sections, checkpoint lanterns, optional embers, five-second chains, earned air jump, three springboards, two crumble platforms and ordered trial splits already exist. There is no carried delivery objective with an expiry.
- **Bounded feature/paths:** Add one optional spark on an existing high side route, carried to the second lantern within an authored simulation-time window for a once-only bonus. Falls/expiry return only that challenge to its start; normal embers, checkpoint recovery and the main finish remain unchanged. Own Emberline source/README/tests, with a small private relay module if useful.
- **Reuse/adoption:** Existing inventory/objective/trial rules, P23 platform transitions and P12 notices. Use the established compact HUD rather than doing portrait work again; a steady timer/marker survives reduced effects.
- **Validate:** GAME `emberline`: complete relay through real movement, timeout/fall/retry, already-passed lantern, no repeat reward, pause/time partitioning and default autoplay still finishing the original course. Parent checks the optional route is legible and humanly reachable.

### P37 — Neon Swarm: one bounded upgrade choice between waves

- **Existing behavior/evidence:** `examples/neon-swarm/src/game.rs` has `Progression`, `Upgrades<1>`, automatic damage ranks, a regenerating shield, dash buffer, 1.6-second wave breaks and single-hit bullets. The README describes automatic 24→28→32→36 damage, not player-chosen loadouts.
- **Bounded feature/paths:** Add a separate selectable run variant with at most one choice at each of the two wave breaks: a damage, projectile or shield branch with explicit rank caps. Keep normal run balance/records intact; use a distinct rules identity for the variant. Own Neon simulation/run/screen/README/tests after the portrait supplement finishes.
- **Reuse/adoption:** P15/P21 for earned choices; P19/P20 for a bounded pierce/ricochet option, P18 for shield capacity; P07 menu and P05 meters. Autoplay chooses a deterministic offered ID through the same command path. Do not reroll on redraw or freeze unrelated host authority.
- **Validate:** GAME `neon-swarm`: each branch changes actual combat, stale/double choice spends once, full caps produce a valid fallback, no gameplay edge leaks from confirm, retry resets the build, all variants remain bounded and at least one full ordinary-input completion is demonstrated per branch.

### P38 — Pocket Rally: scored drift sectors

- **Existing behavior/evidence:** `examples/pocket-rally/src/sim.rs::{Car::step,Race::step}` already implement lateral tire motion, handbrake, ordered gates and best-lap recording; the README includes ghost, bend and wrong-way guidance. There is no drift score or marked scoring zone.
- **Bounded feature/paths:** Add an optional challenge over three authored bends: score controlled lateral slip only above a speed threshold, on road, facing forward and making gate/route progress. Cap reward per sector/lap so circles cannot farm points. Keep the normal three-lap timing/ghost path unchanged and records distinct if persisted.
- **Reuse/adoption:** `Route3::project`, accepted gate crossings, P14 chains and P06 result rows. A steady zone outline and slip/score indication provide readable feedback without requiring particles.
- **Validate:** GAME `pocket-rally`: real handbrake drift earns, straight driving/reverse/donuts/off-road do not, zone boundary/reentry cannot double award, pause/restart clear challenge state and ghost/save compatibility remains intact.

### P39 — Salvage Pinball: nudge with a tilt consequence

- **Existing behavior/evidence:** `examples/salvage-pinball/src/lib.rs::Table::step` uses real flipper contacts, fixed launch impulse, powered bumpers, one ball save per life and a 34 m/s cap. README explicitly says no tilt/nudge. Input and per-ball accounting already exist.
- **Bounded feature/paths:** Add one remappable nudge action applying a small bounded physical impulse to the active ball. Accumulate a simulation-time tilt budget; overuse disables flippers for the remainder of that ball and shows a persistent tilt explanation. Saves/lane returns must have an explicit same-life tilt policy. No multiball or new table is required.
- **Reuse/adoption:** Existing physics impulses, `PlayerInput` tick edges, an ability/cooldown rule and P05/P12 HUD composition. Reuse audio accepted-event routing, not proximity guesses.
- **Validate:** GAME `salvage-pinball`: nudge cannot serve a waiting ball, repeated held/catch-up input fires once, tilt is earned by accepted nudges, disabled flippers cannot secretly move, pause freezes budget, drain resets the appropriate state and normal autoplay/contact scoring still works.

### P40 — Orchard Guard: player-selectable tower targeting priority

- **Existing behavior/evidence:** `examples/orchard-guard/src/game.rs::Game::step` scores `TargetSelector` candidates by path distance, so all towers prefer the pest nearest the exit. Stats, upgrade previews, wave details and target rings already exist. Target choice is not configurable.
- **Bounded feature/paths:** Per-tower First/Strongest/Nearest policies with deterministic ID tie breaks and a selected-tower control. Default remains First; in-flight projectiles retain their accepted target ID. Own `src/game.rs`, presentation/host UI, save adapter, README and tests.
- **Reuse/adoption:** Existing `TargetSelector`, actual tower stat methods, P02 action rows and P09 pointer ownership. If priority is saved, explicitly add a versioned payload and v1→default-First migration; never reinterpret version-1 bytes.
- **Validate:** GAME `orchard-guard`: policies select the correct living in-range enemy, equal scores are stable, dead/reordered pests invalidate cleanly, changing priority does not duplicate fire or spend coins, save/resume preserves choices and original autoplay wins with defaults.

### P41 — Marble Run: isolated checkpoint practice

- **Existing behavior/evidence:** `examples/marble-run/src/sim.rs` has safe recovery anchors, preserved crystals on falls and cumulative splits; `timing.rs` already compares per-course session splits. All four courses are selectable. There is no separate practice segment that excludes ranked records.
- **Bounded feature/paths:** A Practice entry for the current course allowing selection of an existing checkpoint and a run to the next checkpoint/finish. Define a fresh countdown, deterministic hazard phase, bounded local attempts and an explicit practice label. Practice must never submit a medal/best-time or overwrite campaign checkpoint/split references.
- **Reuse/adoption:** Existing safe anchors, `Trial` session host, P04 picker/detail scrolling and P06 segment results. Keep ordinary campaign selection/recovery semantics intact.
- **Validate:** GAME `marble-run`: every checkpoint start is safe, next segment can finish using roll/brake inputs, recovery repeats that segment, practice records stay isolated, returning to normal starts a clearly defined fresh trial and content reload invalidates obsolete segment choices.

### P42 — Dune Skimmer: a second technical circuit

- **Existing behavior/evidence:** `examples/dune-skimmer/src/race.rs::Course::new` derives one analytic route with fourteen gates and rocks; `Race` always runs three laps. Boost/rearm, centered-gate recharge, penalties and route telemetry already exist. There is no course-selection flow or second authored route.
- **Bounded feature/paths:** Extract a bounded course definition and add one short technical circuit with tighter but traversable bends and different boost opportunities. Select at Ready/finished state; preserve the original circuit as default. Own Dune course/race/session/presentation/README/tests; no procedural generator or opponent AI.
- **Reuse/adoption:** Existing `Route3`, vehicle handling and gate aperture tests, P07 menu/P01 regions and P22 penalty summaries. Per-course tuning/rules identity must prevent future record/ghost mixing.
- **Validate:** GAME `dune-skimmer`: both routes close correctly, gates are ordered/forward-only, all recovery anchors are safe, normal-control autoplay completes both and switching resets clock, energy, input and camera. Parent tests human turn readability and boosted corner recovery.

### P43 — Siege Foundry: demolition with protected cargo

- **Existing behavior/evidence:** `examples/siege-foundry/src/lib.rs::{Range::new,fire,step,refresh_preview}` already construct breakable fixed-joint towers, launch eight real balls, count six downed targets and show a collision-stopping arc. The result only considers targets/ammo/settling; there is no collateral objective.
- **Bounded feature/paths:** Add one optional contract placing a clearly marked protected cargo body among supports. Win requires all designated targets down and cargo below an authored displacement/rotation damage limit; define failure during the same final-shot settling window. Keep the existing range as the default contract and keep the existing arc.
- **Reuse/adoption:** Existing physics/joints, `ObjectiveGraph` for target/cargo conjunction, P05 objective strip and P07 contract/result cards. Cargo damage derives from actual body state/contact, not decorative particle events.
- **Validate:** GAME `siege-foundry`: normal default autoplay still wins, a careful real ballistic sequence completes the new contract, direct/indirect cargo movement fails once, pause freezes assessment and rebuild restores every body/contract flag without spending a shot.

### P44 — Sky Courier: a two-stop mail itinerary

- **Existing behavior/evidence:** `examples/sky-courier/src/lib.rs::{Flight::tick,damage}` traverse one static five-ring route and one low-speed final delivery. The README already has navigation, 90-second deadline, hull penalties and recovery. There is no intermediate accepted delivery or second leg.
- **Bounded feature/paths:** Add one optional two-stop mission with two small route slices and low-speed delivery zones; accept each parcel once, then expose the next leg without teleporting the plane. Carry hull and a clearly specified overall deadline across legs. Original single delivery stays selectable/default.
- **Reuse/adoption:** Existing flight motor, swept gate capture, checkpoints, an objective graph, P22 penalties and P07/P06 briefing/results. New mission markers remain useful with reduced motion/flash.
- **Validate:** GAME `sky-courier`: fly both legs through actual controls, reject wrong-order/too-fast delivery, no double time/reward extension from loitering, recovery targets the current leg's earned beacon, timeout/impact priority is explicit and restart clears both parcels.

### P45 — Crystal Cavern: discovered-room map and player waypoint

- **Existing behavior/evidence:** `examples/crystal-cavern/src/lib.rs::Game` uses a fixed tile `MAP`, walls, three crystals, two ordered altars, focus selection and an internal autoplay route. The player receives objective prompts but no visited-room map or placeable waypoint.
- **Bounded feature/paths:** Add a compact map that reveals visited/visible tiles and lets the player choose one waypoint only on discovered walkable space. Show a bearing to that waypoint while exploring and mark discovered objective locations. Do not expose the autopilot's full solution or unvisited crystals through walls.
- **Reuse/adoption:** Bounded bitset/tile storage, existing cave visibility/query geometry, P04 map panel, P09 UI/world input routing and P01 reserved HUD space. Opening the offline map uses existing pause ownership.
- **Validate:** GAME `crystal-cavern`: walls block discovery, waypoint bounds/visibility are enforced, interacting with the map cannot collect an object, gate opening updates known traversability, retry clears exploration and normal objective completion remains unchanged.

### P46 — Strike: timed offline combat drill with accuracy results

- **Existing behavior/evidence:** `examples/strike/src/lib.rs::practice` creates six players, `PracticeBots` supplies five opponents, and `apply_game_events` consumes authoritative Shot/Hit/Killed events. `host.rs` separates Practice/Replay/Network. Existing practice is open-ended; replay/network/tactical modes already exist.
- **Bounded feature/paths:** Add an optional 60-second offline drill with accepted shots, hits, eliminations and deaths, ending in a result card and explicit retry. Derive accuracy from accepted events and define multi-hit counting; retain free practice. No network protocol or authoritative FPS weapon changes.
- **Reuse/adoption:** Existing match simulation and event stream, `GameSession` practice timing, P06/P07 results and P25 cue batching. Keep drill bookkeeping disabled during network play and replay unless a separately defined recorded scenario supports it.
- **Validate:** GAME `strike`: dry fire/cooldown cannot inflate shots, nonlocal shots cannot inflate accuracy, simultaneous end-tick events count once, offline menu pauses the drill, network menus still tick normally and retry resets bots/drill/recording boundaries through existing paths.

### P47 — 2D Physics Lab: spring-based body dragging

- **Existing behavior/evidence:** `examples/physics-2d/src/lib.rs::{Sandbox::spawn,mechanisms,Lab::select_body}` already support bounded drops, pinned inspection, spring/hinge/fixed/distance mechanisms, impulses and manual stepping. Pointer interaction currently places/inspects; it does not drag an existing body through a physical constraint.
- **Bounded feature/paths:** Add an explicit Grab mode with at most one temporary spring constraint to a bounded kinematic anchor. Pick a real dynamic body at the current pose, move the anchor from pointer/virtual cursor input, and release/cancel cleanly. Keep Drop mode and paused inspection; do not teleport a running body to the pointer.
- **Reuse/adoption:** Existing query/joint APIs, `Ui` capture, P09 routing and P08 touch geometry. The physics library already provides the joints; this task is a lab feature, not another checked-physics API.
- **Validate:** GAME `physics-2d`: grabbing an empty/static location changes nothing, constraint/body capacity stays bounded, focus/pause/reset/cancel remove the temporary joint, stale selections do not panic, manual stepping shows real constrained motion and spring forces stay bounded.

### P48 — 3D Physics Lab: controlled surface comparison experiment

- **Existing behavior/evidence:** `examples/physics-3d/src/lib.rs::Sandbox::new/spawn` build one yard with fixed per-shape restitution, ramps/stairs, compound bodies and mechanisms. The inspector already shows contact/sleep/ray counters. There is no controlled repeated comparison of surface settings.
- **Bounded feature/paths:** Add one selectable experiment with three parallel identical ramps and equal bodies released together over low/medium/high friction surfaces. Show displacement and settling time for each lane; explicit Reset Experiment recreates the same initial conditions. Retain the original yard as a preset.
- **Reuse/adoption:** Existing material mixing and checked body builders, `EnvironmentPlan`, fixed stepping, P05/P06 inspector rows and P07 preset selection. Report observations of this solver, not a general physical-accuracy benchmark.
- **Validate:** GAME `physics-3d`: initial masses/poses match across lanes except the intended material, reset is repeatable on the tested host, pause/single-step advances all lanes equally, finite results and body caps hold, and the original yard/joint tests remain intact.

### P49 — Courtyard: ordered cover-to-cover training route

- **Existing behavior/evidence:** `examples/owned-lab/src/training.rs::objectives` defines four independent move/fire/reload/hit drills; `sync_training` accepts early actions. `orientation.rs` supplies compass/visible-target guidance and `scene_edit.rs` supports transactional edits. There is no ordered spatial training route.
- **Bounded feature/paths:** Add one optional offline course of three marked cover stations requiring actual arrival, then a specified accepted action at that station. Track splits and final time separately from the existing independent drills; retain free training and 2D overview. Disable the course during peer matches and invalidate/restart it deliberately if scene edits move its geometry.
- **Reuse/adoption:** Existing objective prerequisites, movement distance/contact authority, `Trial` and P06 results; render through the existing custom HUD/backend. Do not assume renderer `Ui::paint` can replace Courtyard's custom rendering without an adapter.
- **Validate:** GAME `owned-lab`: early/wrong-station actions do not complete ordered objectives, movement into a wall cannot count as arrival, default drills retain independence, peer clocks/packets are unchanged, menus freeze only offline training and reset clears the new route state.

### P50 — Orbit Breaker: limited sticky-paddle catch and release

- **Existing behavior/evidence:** `examples/owned-orbit/src/game.rs` already has continuous ball sweeps, charge serves, reinforced panels, ball saves, volley chains, pity capsules and anti-farming precision returns. Every ordinary paddle collision currently reflects immediately; there is no player-earned catch resource.
- **Bounded feature/paths:** Add an optional challenge with one catch charge per sector. Holding a remappable Catch action during a real paddle contact consumes it and parks the ball on the paddle; a fresh launch releases through the existing charge rule. Catch cannot rescue a missed paddle, grant a life or farm precision points. Keep original mode/default inputs and software raster path.
- **Reuse/adoption:** Existing `ChargeAction` with P17 outcomes, inventory/resource count, actual collision events, P14 chain receipts and P12 captions/notices. Keep native CLI and legacy held-bit API compatible; expose any extra host control additively and document it rather than repurposing existing button bits silently.
- **Validate:** GAME `owned-orbit` plus `examples/owned-orbit/tests/platform-input.mjs`: catch only at confirmed paddle collision, consume once, require release/fresh launch across catch-up and pause, preserve lives/sector timing, no precision reward without panel progress and default two-sector autoplay still wins. Parent checks opaque RGBA output and browser control geometry.

## Integration and sequencing notes

The candidates deliberately separate reusable engine capability from substantive game behavior. For example, P21 supplies an upgrade-choice transaction; P37 supplies Neon's actual branches, balancing, UI and finish scenarios. Do not count merely exposing an API and writing its receipt as the game adoption.

Suggested waves:

1. Pure composition P01–P03 and gameplay P13–P19/P21–P24; independently start P30/P31/P32/P35 tooling. Keep P14/P15/P16 changes in their existing small modules.
2. Renderer adapters/compositions P04–P12 and P25/P26; P20 follows P19. Content P27–P29 need coordinated authoring/renderer ownership. P34 and P33 are useful before broad migration evidence is collected.
3. Implement the fifteen demo features with the relevant prerequisites, then migrate repeated presentation code where the new shared API demonstrably replaces it. Preserve the already-finished baseline fixes and game-specific physics.
4. Parent joins implementation, registration, source tests, builds and fresh visual evidence for all fifteen demos. Unrun physical-device, audio-listening, sustained-performance or cross-platform tests remain explicit gaps.

Several UI candidates mention `ui/shared.rs`, and P29 touches shared content ABI. Do not launch overlapping owners there. Prefer dedicated new child modules plus parent-owned registration, or sequence edits to shared types. Likewise P35/P30 both touch the CLI: implement them sequentially or assign disjoint support modules with one integration owner. P27/P28 need agreed source/domain boundaries; only the domain parser can declare a game scene valid.

The initial shared-tree status contained extensive concurrent modifications. Line numbers would be unstable; evidence above uses exact existing paths and symbols. New module/type names are proposals. Recheck the actual source and dependency receipts when scheduling, particularly the active Neon supplement and the parent rebuild following Emberline 186. No returned receipt establishes combined integration.

## Report verification

Research used read-only file listing/search, source/README/contract reads and the current working-tree status. Existing inline and package regression tests were inspected to understand already-covered behavior; none were executed. A read-only `python3 -B` report check passed: 50 unique ordered proposal IDs, fifteen demo feature entries, and clean trailing whitespace/final newline. Literal repository-path references were checked for existence; the only absent directory was the explicitly proposed play UI child-module directory noted above. No implementation or playability outcome is asserted by this document.
