# Orchard Guard

A first-party Ferravex example with a shared WebGL2 browser host and native renderer entry point. See [platform support and verification](../../docs/PLATFORMS.md) for the OS libraries and device checks that apply.

A warm, procedural 2D tower-defense game built on Ferravex. Protect an orchard through four increasingly crowded waves of googly-eyed beetles and speedy "zoomies". The winding dirt path, swaying fruit trees, farmhouse, harvest basket, towers and pests use simple vector geometry with no downloaded assets.

## What makes it fun

- **Readable chaos.** Stopped pests flip onto their backs with x-eyes, burst into leafy splats and pop a `+13` coin; seed hits puff, frost hits sparkle, towers recoil when they fire and bounce in when planted or upgraded. Short rotating pop words ("SPLAT!", "BONK!", "SKID... SPLAT!") never repeat back to back.
- **Lucky bonks.** Every eighth projectile fired in the orchard is a lucky shot for double damage, with a gold ring, a "LUCKY BONK!" (or "BRRRR!") pop and a little camera kick. The cadence comes from the saved shot counter, so restored checkpoints continue it exactly.
- **Wave cards and toasts.** Between waves a card previews what is coming ("INCOMING: 12 BEETLES + 4 ZOOMIES · 78 HP") with a line of flavor. Clearing a wave announces its reward; clearing it without losing a heart is a **PERFECT WAVE**. Each leak shakes the board, flashes the gate, wobbles the heart counter and removes an apple from the harvest basket next to the farmhouse (one apple per heart).
- **Fast-forward.** The **SPEED** button (or **F**) toggles 1×/2×. The simulation still runs the same 60 Hz fixed ticks, just two per frame interval, so a fast run plays out exactly like a normal one.
- **Results with a hook.** Win or lose, the card shows your gardener rank (from *Compost Enthusiast* to *Orchard Legend*), pests stopped, hearts left, perfect waves, lucky bonks, harvest time and your previous best, flags new records, and ends with a concrete next goal ("STOP 6 MORE PESTS TO BEAT YOUR BEST OF 45").
- **A secret.** Rumour says royalty waddles through wave three. Stop it for a special toast and a line on the results card.

```sh
cargo run --release -p orchard-guard
```

Windows from 280 to 899 logical pixels wide use a compact HUD: essential text stays at 14 logical pixels, purchase prices remain visible, and game buttons are at least 44 pixels high. Eight numbered buttons below the board select plots through the existing pointer, touch or keyboard UI; the highlighted button identifies the selected plot. The board retains its authored coordinates and aspect ratio. The browser reserves enough vertical room for this layout, so scroll the game area into view below the host guide. Extremely small native windows can still clip the lower HUD; resize to show the complete panel. Long remapped bindings remain available in Controls.

Click or tap one of the eight numbered plots, then choose a tower in the bottom bar. On a fresh orchard a bouncing **PLANT HERE** arrow marks the selected plot until the first tower exists. The selected plot shows its range; empty plots preview both the seed sling's pale ring and frost hive's teal ring. Buttons use large titles with prices; the line under each button shows its key, what the tower does, the upgrade's next stats, or how many coins are missing. Clicking outside a plot or using an unavailable shortcut explains why the action was rejected without spending money in a pill above the bottom bar.

Send the first wave when ready; initial preparation does not advance simulation time. You can keep building and upgrading while pests move. Before sending a wave, its preview shows the pest and runner counts, regular/runner health and speed, arrival spacing, and completion reward. Runner counts follow the campaign's spawn sequence, including after loading a checkpoint. During combat the HUD shows pests still arriving and on the path. It warns when no defenses have been planted but still lets you launch. The orchard survives as long as at least one of its 12 hearts remains after wave four.

| Control | Action |
| --- | --- |
| Click / tap plot | Select a build or upgrade location |
| Click / tap bottom buttons | Build, upgrade, send a wave, or restart |
| 1 / 2 | Build seed sling / frost hive on the selected plot |
| U | Upgrade the selected tower |
| Space | Send the next wave |
| F / Speed button | Toggle 1× / 2× fast-forward |
| Escape / Pause button | Pause or resume after starting |
| R / Restart / Plant again | Reset all towers, pests, money and hearts for a fresh match |
| Tab / arrows, Enter | Navigate and activate HUD/overlay buttons |
| F2 | Open the shared controls editor |

These are defaults. Canvas hints read the current bindings and device through `PlayerInput::format_hint`, including remaps, imported profiles, resets and unbound actions. Long labels are fitted by the shared UI; open Controls for the complete binding list. Plot numbers label locations, not selection shortcuts. For keyboard plot selection, assign a key to **Point / select** in Controls, move the virtual cursor with W/A/S/D, and use that key. That capture replaces the action's mouse binding. If a HUD button holds keyboard focus, reopen and close Controls to clear it. In the browser, pause first, then use **Shift+Escape** to reach the host's How to play disclosure; exit fullscreen first if it handles Escape.

The selected tower shows its actual damage, range and reload time. Its upgrade button previews the next values or explains missing coins/max level. Combat and these previews use the same stat methods. Default balance:

| Tower | Cost | Damage at levels 1 / 2 / 3 | Range | Reload seconds |
| --- | --- | --- | --- | --- |
| Seed sling | 60 | 22 / 36 / 50 | 158 / 176 / 194 | 0.62 / 0.54 / 0.46 |
| Frost hive | 80 | 8 / 14 / 20 | 170 / 188 / 206 | 0.85 / 0.77 / 0.69 |

Every eighth projectile fired anywhere is a lucky shot for double damage. Both upgrades cost 65, then 100 coins. Frost hits slow their target to 45% speed for 1.65 seconds; nearby pests within 46 units receive a 1.15-second slow without splash damage. A ring and **SLOW** countdown show the actual active effect, independently of hit flashes. Seed hits briefly stagger their target. Both towers prioritize the living pest closest to the exit within range. The selected tower marks that target with a ring and **TARGET** label; target identity survives list reordering, and dead or out-of-range pests are not marked. Each pest stopped earns 13, and completing a default wave earns 45. Escaped pests cost one heart. Pests cannot be blocked, so cover multiple bends and combine frost with seed slings.

The game uses Ferravex's `GameSession` at 60 Hz, aspect-preserving `Canvas2D` for drawing and pointer input, portable renderer, and native `Demo` smoke harness. `Ready` permits initial preparation; sending a wave starts the session. Pause freezes spawn timers, movement, reloads and the harvest clock, with no elapsed-time catch-up. Controls/focus interruptions also suspend interactive play; closing them preserves an explicit pause. Win/loss freezes the result and offers a fresh start. Restart clears simulation, clock, pause and feedback state and consumes that frame's other inputs. Preparation, plot selection and lifecycle commands are immediate. During active waves, gameplay keys are taken only on actual ticks; one pending HUD purchase retains its original plot across zero-tick frames. Duplicate UI/key commands charge once during catch-up, and pause, focus loss, reload and restart discard pending commands.

The simulation uses fixed ticks, with at most 32 live pests and 96 projectiles. Every projectile tracks a unique pest ID, and invalidated targets remove their projectile. Entity storage remains bounded through long matches and restarts.

Each presentation frame reads the shared `render::preferences::feedback_policy`, including paused/menu and zero-tick frames. Juice reacts only to accepted simulation events: the game records a bounded event log (at most 128 entries per frame, never reallocated) that presentation drains for particles, rings, pops, toasts, shake and audio. Reduced motion clears and blocks particles, rings, shake, tower squash, leg wiggle, tree sway, drifting cloud shadows and butterflies; pops and toasts stay readable but stop rising. Reduced flash independently clears the bounded hit flashes and the gate/heart flash. Suppressed impacts are remembered, so changing the setting back cannot replay an old flash. Effects freeze while paused; combat, projectile travel, slow durations and spawn timing remain simulation-owned. Restart and content reload clear presentation state. There is no separate preference store or export; the speed toggle lasts for the session and survives restart.

The existing scene loader, `Route3`/`PathCursor` navigation, status effects and validated save slots remain in use. Native `--content path.fxscene` edits reload into a fresh preparation state; invalid edits retain the previous valid scene. Safe checkpoints save between waves, including purchases and upgrades. Reopening restores the latest safe checkpoint, while a loss retains it for another attempt; **Plant again** deliberately clears it. A win clears the checkpoint and records the result once; the results card compares it with the records that existed before the run. Perfect waves are not saved: a checkpoint with no leaks restores them all, otherwise counting resumes from the checkpoint. Restoring reconstructs spawn identity from the completed wave totals, preserving fast-pest timing without changing the version-1 save format. The footer reports unavailable storage; autoplay/smoke saves are transient.

```sh
cargo test -p orchard-guard --all-features --offline --locked -j 2
cargo clippy -p orchard-guard --all-targets --all-features --offline --locked -j 2 -- -D warnings
cargo check -p orchard-guard --target wasm32-unknown-unknown
cargo run --release -p orchard-guard -- --autoplay --smoke-frames 420 --screenshot orchard.png
```

Debug native builds also read `ORCHARD_WINDOW=390x720`, `ORCHARD_PAUSE_AT=N`, `ORCHARD_SPEED_AT=N`, `ORCHARD_TURBO=K`, `ORCHARD_DEFENSELESS=1`, `ORCHARD_LAZY_FROM=N`, `ORCHARD_SELECT=P` and `ORCHARD_FAKE_BEST=SCORE` to capture specific screens for visual review. The hook module is not compiled into release or Wasm builds.

`--autoplay` executes the same purchases, upgrades, wave launches, and combat rules as human play. It preserves unattended focus-independent stepping for smoke runs; explicit pause still freezes it. Tests cover lucky-shot cadence, the bounded event log and its milestone counts, perfect-wave accounting, fast-forward tick parity, rank/goal/quip selection, reduced-motion/flash juice handling, record comparison, the authored four-wave win, defenseless loss, bounded entities, rejected purchases, zero-tick taps through real `PlayerInput`, duplicate input, queued plot identity, pause/focus recovery, clean restart, exactly-once rewards/results, target retirement, tower stat/shot agreement, previewed runner counts, preference changes and checkpoint continuation with matching pest identities and combat. Native smoke flags use a deterministic render timestep; browser launches are interactive. These checks do not establish cross-platform determinism. Browser/Wasm and visual verification are separate from the headless regressions.

## Browser player update

Open **Game menu → Settings → Player tools** for screenshots, clean sharing links, frame limits, session timing, screen wake lock, accessibility preferences and settings backups. All shared player upgrades apply to this example. Read the [player guide](../../docs/PLAYER_GUIDE.md) for behavior and browser limits.

## Compact browser controls

On narrow screens, tap **Plot** to open the numbered plot palette, or select a
plot on the board. The palette closes after selection. Prices remain on the
build/upgrade buttons; the wave, pause and restart actions share one row.
Default instructions and expanded wave-stat paragraphs are omitted from the
playfield; the plot row ends with a compact **>>1×** speed toggle (kept out of the browser's fullscreen menu corner), the notice strip
shows toasts, and the results card lists your rank, best and next goal. The shared game menu provides help and remapping. Fullscreen uses the
actual viewport, with the board and action dock fitting together.
