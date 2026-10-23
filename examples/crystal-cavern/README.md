# Crystal Cavern

Restored locally with first-party Ferravex APIs. Native play currently uses Linux X11/GLX; browser play uses the shared WebGL2 host. This recovery has not been published, and prior backend test results must be rechecked.

A compact first-person exploration puzzle in a warm, faceted cave. Press **Enter** or select **Begin expedition**. Recover three mint crystals from the side chambers, awaken altar **I**, then altar **II**, and walk through the opened seal into daylight. The altar markings are carved bars on their tops.

Press **E** near an object to interact; aiming directly at it is unnecessary. The nearest visible target receives a steady ground ring and a matching prompt: mint means ready, amber means a prerequisite is missing. Focus stays with that object until another is more than 20 cm closer, or the current object is collected, out of reach or obstructed. The HUD reads progress and blockers from the objective graph: crystals recovered, altars awakened, remaining prerequisites, and a steady six-part completion bar. The final segment requires reaching daylight; opening the seal alone does not finish the expedition. Completion freezes the timer, reports all six objectives complete, and offers **Explore again** with an explicit reset explanation.

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

`PlayerInput::sample_play()` samples platform input once per render frame and retains the manual pause reason. Interaction edges stay in its focus-aware action map until the first actual tick; a quick tap survives zero-tick frames and is spent once during catch-up. No separate game-side input queue survives focus loss. The bounded shared `TargetSelector` retains interaction focus on ticks and rechecks range and line of sight for prompts and actions. `ObjectiveGraph::inspect` / `iter` provide readiness, blockers and progress without advancing objectives or changing snapshots. Accepted collections, altar activations, and completion drive the existing audio cues and host diagnostics; rejected interactions emit neither. Prompts and menu descriptions wrap using the shared UI, and the menu button is disabled while Controls is open.

Wrapped overlay text is explicitly painted before Controls. Menu guidance keeps the reset explanation visible at 320px; in-canvas binding labels longer than 12 bytes use **See Controls**, where the full bindings remain available. This changes the displayed hint only, not the bindings or accepted inputs.

The example uses swept character collision, shared AABB line-of-sight queries, navigation for autoplay, flat-normal meshes, baked ambient occlusion, point lights, adaptive music and cave acoustics. Cached procedural meshes need no downloaded textures or models. The authored crystal shader and its live-reload interface are unchanged.

Every presentation frame reads `ferravex_render::preferences::feedback_policy()` before accepted feedback and drawing, including menus and zero-tick frames. Reduced motion freezes crystal bobbing and crystal/altar rotation; reduced flash independently bypasses the animated crystal shader (freezing its time alone would still vary brightness as crystals bob). Separate cosmetic phases advance only with executed gameplay time, discard suppressed time, freeze when paused and reset on retry. Re-enabling resumes ambient animation without catching up hidden time. There are no transient particles, camera-shake impulses, screen flashes or `FeedbackController` in this demo. Preferences change presentation only; movement, accepted objectives, timer and audio remain active under either preference. Actual fresh-Wasm/browser preference verification belongs to parent integration.

```sh
cargo test -p crystal-cavern --offline --locked -j 2
cargo clippy -p crystal-cavern --all-targets --offline --locked -j 2 -- -D warnings
cargo check -p crystal-cavern --target wasm32-unknown-unknown --offline --locked -j 2
cargo run -p crystal-cavern --offline --locked -j 2 -- --autoplay --smoke-frames 6000 --screenshot /tmp/crystal-cavern.png
```

Autoplay starts immediately and navigates the map with movement, turning and interaction inputs through the same simulation as a player. Headless tests verify completion with both regular and irregular frame times, collision, ordered gate prerequisites, matching prompts/targets, one interaction per edge, quick taps, focus loss, pause/resume, terminal-state freezing and full retry. Migration regressions additionally cover focus hysteresis and invalidation, remapping/unbound/oversized hints, restored objective blockers, accepted-only progress, independent presentation policies on zero-tick frames and phase reset. Native/browser visual and device checks are separate from these behavioral tests.

The crystal material can be edited in the browser with `?material-tools`. Native `FERRAVEX_CRYSTAL_SHADER=/path/to/crystal.frag` polls a fragment-hook file; failed replacements keep the last valid program. See the [material guide](../../docs/MATERIALS.md).

## Browser player update

Open **Details → Player tools** for screenshots, clean sharing links, frame limits, session timing, screen wake lock, accessibility preferences and settings backups. All shared player upgrades apply to this example. Read the [player guide](../../docs/PLAYER_GUIDE.md) for behavior and browser limits.
