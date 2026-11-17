# Crystal Cavern

A first-party Ferravex example with a shared WebGL2 browser host and native renderer entry point. See [platform support and verification](../../docs/PLATFORMS.md) for the OS libraries and device checks that apply.

A compact first-person exploration puzzle in a warm, faceted cave with a dry sense of humor. Press **Enter** or select **Begin expedition**. Recover three mint crystals from the side chambers, awaken altar **I**, then altar **II**, and walk through the opened seal into daylight. The altar markings are carved bars on their tops; the seal's three sockets light up as their crystals are recovered.

## What makes it fun

- **Show, don't tell.** The cave explains itself with pictograms: the game's faceted gem for crystals, the carved I/II plaques for altars, a sun for the way out, a lock for what is not ready yet, key caps for actions and plain numbers for stats. Words are limited to place names, three-word callouts, button labels and Wendell's note.
- **Warmer / colder.** The top bar's hum meter (a speaker and signal bars) fills and ticks faster as you approach the nearest missing crystal, and each crystal's light brightens as you close in.
- **The cave talks back.** Named areas (Base Camp, the Drip Gallery, the Crossroads, the Echoing Grotto, the Glitter Pocket, the Hall of Altars) announce themselves with a map pin the first time you enter; Base Camp shows the whole expedition as pictograms (three crystals, two altars, daylight). A dry echo comments in three words or fewer on pickups, locked altars, a sealed door and walls you walk into ("BONK!"), each line followed by its own fading echo. Lines rotate and never repeat back to back.
- **Escalating whispers.** After 30 s without progress the hum meter glows and the echo whispers (`PSST...`) a short direction, after 55 s a gold heading marker under the bar points at the objective (a crystal in plain view first), and after 85 s it names the area. Each whisper counts toward the results card.
- **Juice from accepted events.** Pickups burst into mint sparkles with a FOV punch, a rising gem `+1` pop, a HUD gem bounce and a soft edge glow; altars throw a gold column; the seal rumbles open in a cloud of dust. Rejected uses shake the reticle and the prompt, flare its lock and get a dull knock.
- **Secrets.** Two optional finds sit outside the objective graph: Steve, a shy violet crystal, and a lost miner's four-page note (with his hat). Neither changes the puzzle. The note is the one text read during play: each page appears on a paper slip, with page pips.
- **Results card.** Clock, hint-bulb and secret-gem counts, a silly rank title with its one-line quip (from *Scenic Route Surveyor* to *Legend of the Deep*), a crown comparison with your personal best (FIRST CLEAR, NEW BEST, TIED or the gap) and a next-goal pictogram row (a shy violet gem and a question mark, the note, zero hints, or a faster clock). The Ready card shows three crystals, two altars and one way out as numbers beside pictograms, and your stored best under a crown (or the shy-crystal rumour). Autoplay runs are never recorded.

Press **E** near an object to interact; aiming directly at it is unnecessary. The nearest visible target receives a steady ground ring and a matching prompt pill: mint shows the interact key cap beside the target's pictogram (a gem, an altar plaque, Steve, or the note with its page pips; an arrow and the sun once the seal is open); amber shows a lock beside what is missing (the empty crystal sockets, altar I, or both altars). After the first accepted use the key cap quiets so the target leads. Focus stays with that object until another is more than 20 cm closer, or the current object is collected, out of reach or obstructed. The slim top bar reads progress from the objective graph: three gem sockets, the two altar plaques, the daylight sun, the hum meter and the clock, over a steady six-part completion strip; the current objective breathes (empty sockets, then plaque I, plaque II and the sun). The final segment requires reaching daylight; opening the seal alone does not finish the expedition. Completion freezes the timer and shows the results card with **Explore again**. Movement/look key caps with pictograms sit at the bottom until you have walked a few metres.

```sh
cargo run -p crystal-cavern --offline --locked -j 2
```

| Control | Action |
| --- | --- |
| Enter / overlay button | Begin, resume, or explore again after completion |
| WASD | Move and strafe |
| Arrow keys or left-button drag | Look around |
| E | Recover a crystal / activate an altar |
| Escape | Pause / resume |
| R | Restart the expedition |
| H | Toggle the controls hint |
| F2 / Controls | Remap controls and adjust player settings |

The browser build uses the same defaults and does not require pointer lock. The shared Explore preset includes gamepad bindings (left stick to move, right stick to look, west face button to interact, south face button to confirm) and touch bindings (left-side movement stick, right-side look, **USE** button). Overlay buttons support clicking/tapping. These device paths require separate device verification. Interaction, confirm, retry, pause and help labels use `PlayerInput::format_hint`, following current bindings and the most recently used input family. Unbound actions say **Unbound**; oversized binding lists say **See Controls**. Movement and look bindings remain available in Controls. On the browser host, **Tab / Shift+Tab** navigate page controls and the expedition guide; Tab is reserved even if assigned to gameplay. Pause before reading the guide: moving keyboard focus within the page does not itself pause the expedition.

`ferravex_core::session::{GameSession, SessionState}` owns the host's Ready/Playing/Paused/Won lifecycle and 60 Hz clock with at most eight catch-up steps. The existing objective graph remains authoritative for crystal, altar, and gate rules. Pausing, loss of focus, and opening Controls suspend gameplay; returning restores play unless Escape manually paused it. Inactive frames and the first resume frame do not advance expedition time. Retry resets objectives, motion, the clock, pilot route, camera, pitch, notices, and footstep travel.

`PlayerInput::sample_play()` samples platform input once per render frame and retains the manual pause reason. Interaction edges stay in its focus-aware action map until the first actual tick; a quick tap survives zero-tick frames and is spent once during catch-up. No separate game-side input queue survives focus loss. The bounded shared `TargetSelector` retains interaction focus on ticks and rechecks range and line of sight for prompts and actions. `ObjectiveGraph::inspect` / `iter` provide readiness, blockers and progress without advancing objectives or changing snapshots. Accepted collections, altar activations, and completion drive the audio cues and host diagnostics; rejected interactions emit only a soft knock and an echo line, never an objective diagnostic. Prompts and card stats are fixed-size pictogram rows that fit a 320px canvas whole, and the menu button is disabled while Controls is open.

Overlay rows are explicitly painted before Controls. The pause card shows progress as sockets, plaques and the clock, and the restart key cap beside its pictogram at 320px; in-canvas binding labels longer than 12 bytes use **See Controls**, where the full bindings remain available. This changes the displayed hint only, not the bindings or accepted inputs.

The example uses swept character collision, shared AABB line-of-sight queries, navigation for autoplay, flat-normal meshes, baked ambient occlusion, point lights, adaptive music and cave acoustics. Cached procedural meshes need no downloaded textures or models. The authored crystal shader and its live-reload interface are unchanged.

Every presentation frame reads `ferravex_render::preferences::feedback_policy()` before accepted feedback and drawing, including menus and zero-tick frames. Reduced motion freezes crystal bobbing, Steve's wobble and crystal/altar rotation, and clears and blocks particles, camera rumble, the FOV punch, HUD icon bounces, the reticle shake and rising pops (pops and callouts stay readable in place). Reduced flash independently bypasses the animated crystal shader (freezing its time alone would still vary brightness as crystals bob) and clears and blocks edge glows and the HUM meter pulse. Cosmetic phases and juice (`src/juice.rs`) advance only with executed gameplay time, discard suppressed time, freeze when paused and reset on retry; only the results-card intro uses render time. Preferences change presentation only; movement, accepted objectives, timer and audio remain active under either preference. Actual fresh-Wasm/browser preference verification belongs to parent integration.

Personality copy lives in `src/quips.rs` as pure functions of accepted counters, so a given run always reads the same way. Bonks, zones, rejected uses, secrets and hint levels are deterministic simulation facts reported through `Progress`; none of them feed back into the objective graph, so the authored course, save snapshots and replay comparisons are unchanged. Personal bests use the shared `ferravex_render::storage::Records` under owner `crystal-cavern` (fastest time, most secrets). Accepted collections, altar activations, the seal, secrets, notes, whispers, rejected uses and bonks each have an audio cue; the HUM ticks are soft clicks.

```sh
cargo test -p crystal-cavern --offline --locked -j 2
cargo clippy -p crystal-cavern --all-targets --offline --locked -j 2 -- -D warnings
cargo check -p crystal-cavern --target wasm32-unknown-unknown --offline --locked -j 2
cargo run -p crystal-cavern --offline --locked -j 2 -- --autoplay --smoke-frames 6000 --screenshot /tmp/crystal-cavern.png
```

Autoplay starts immediately and navigates the map with movement, turning and interaction inputs through the same simulation as a player; it never detours into a secret and needs no hints. Debug builds (not release or Wasm) read `CAVERN_WINDOW`, `CAVERN_PAUSE_AT`, `CAVERN_STALL_AT`/`CAVERN_YAW`/`CAVERN_STUCK`, `CAVERN_SECRETS_AT`, `CAVERN_FAKE_BEST` and the `CAVERN_STAGE_PICKUP`/`CAVERN_STAGE_LOCKED`/`CAVERN_STAGE_NOTE` staging hooks for screenshot review (see `src/capture.rs`). Headless tests verify completion with both regular and irregular frame times, collision, ordered gate prerequisites, matching prompts/targets, one interaction per edge, quick taps, focus loss, pause/resume, terminal-state freezing and full retry. Migration regressions additionally cover focus hysteresis and invalidation, remapping/unbound/oversized hints, restored objective blockers, accepted-only progress, independent presentation policies on zero-tick frames and phase reset. Polish regressions (`src/polish_tests.rs` and inline tests in `juice.rs`, `quips.rs` and `hud.rs`) cover secrets staying outside the objective graph, a pilot run that discovers every area without secrets or hints, one bonk per impact (not per tick, and not for glancing slides), rejected-use reporting, escalating and resetting whispers, resonance and visible-first hint targets, juice routing and its reduced-motion/flash channels, non-repeating quips, rank ordering, the personal-best comparison and pictogram rows that fit 320 px. Native/browser visual and device checks are separate from these behavioral tests.

The crystal material can be edited in the browser with `?material-tools`. Native `FERRAVEX_CRYSTAL_SHADER=/path/to/crystal.frag` polls a fragment-hook file; failed replacements keep the last valid program. See the [material guide](../../docs/MATERIALS.md).

## Browser player update

Open **Details → Player tools** for screenshots, clean sharing links, frame limits, session timing, screen wake lock, accessibility preferences and settings backups. All shared player upgrades apply to this example. Read the [player guide](../../docs/PLAYER_GUIDE.md) for behavior and browser limits.
