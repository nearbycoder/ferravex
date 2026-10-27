# Strike / Dockyard

The restored low-poly shooter prototype uses the shared authoritative FPS simulation and original Ferravex immediate renderer. Procedural shipping containers, cranes, operators and the rifle are cached meshes with flat normals and restrained directional lighting/fog. No external models, textures, fonts or runtime libraries are loaded.

This recovery is local and unpublished. The native interactive host currently supports Linux X11/GLX; the browser uses the shared WebGL2 host. Native Windows/macOS/iOS game hosts are not implemented, and recovery-wide gameplay validation is pending.

## Play locally

```sh
cargo run --release -p strike
cargo run --release -p strike -- --play --smoke-frames 360 --screenshot /tmp/strike.png
```

Choose **Start practice** (or press Enter on the focused button). Default bindings: WASD moves, mouse aims, left click fires, Space jumps and R reloads. Escape opens the menu, hold Tab for the scoreboard, F2 opens Controls and F3 toggles diagnostics. Controls supports remapping and gamepads; in-game menu, reload and tactical hints follow the current device and saved bindings. Unbound actions are labeled explicitly; long custom binding lists direct you to Controls. Offline practice has five opponents using the same movement, collision and weapons simulation as the player, with automatic respawns.

## The gauntlet

Practice is a two-minute match: you against the five-bot dockyard crew — Barnacle Bob, Forklift Fran, Captain Gull, Tugboat Tina and Rusty. Frag as many as you can before the clock runs out.

- **Scoreline.** The top strip shows your frags, the authoritative match clock and the crew's frags (including the times they shoot each other). The clock turns red for the last ten seconds; 30 seconds left and the final five seconds get a callout and ticks.
- **Hits and frags.** Accepted hits draw a crisp marker: an amber X for body hits, a larger red X with **HEADSHOT**, and **FRAG** with a rising +1 for an elimination. A kill feed lists recent eliminations (HS marks headshots) and each bot gets some last words.
- **Callouts.** Kills within three seconds of each other chain into DOUBLE TAP, TRIPLE THREAT, QUAD SQUAD and DOCKYARD SWEEP. Streaks without dying unlock HEATING UP, FREIGHT TRAIN, UNSTOPPABLE CARGO, KRAKEN MODE and LEGEND OF THE DOCKS. First blood and revenge on your last killer get their own banners.
- **Being fragged.** The elimination panel names your killer and quotes their gloat, then counts down your respawn. The crew holds fire for two seconds after the match starts and after you respawn, fires short readable bursts aimed at the chest rather than rare instant cross-map headshots, and never touches your own commands. This tuning edits only the bots' practice commands before each authority tick, so it is deterministic and captured in replays like any other input.
- **Results.** When the clock hits zero the match stops on its final tick and a results card shows frags, deaths, K/D, accuracy, headshots and best streak, a rank title (from SEAGULL SPECTATOR to KRAKEN OF THE DOCKYARD, with UNSINKABLE for a flawless run), up to three silly awards such as MOST SHOTS INTO A WALL, NEMESIS or HR INCIDENT, your personal best and how many frags the next rank needs. **Play again** restarts instantly; **Briefing** returns to the opening menu. You can still watch the last recording (F5) from the results.
- **Personal best.** Offline matches record your best frag count locally (`Records` owner `strike`, id `gauntlet`). The briefing shows it with its rank and run count; the pause menu shows the clock, score and how many frags beat your best. Automation runs (`--smoke-frames`, `--capture-dir`) never record. Network matches never record or end on this clock.
- **Secret.** Something small and yellow sits on top of one of the containers. Shoot it.

First-match hints show which keys fire and reload until you fire, then how to open the scoreboard after your first frag; each disappears once used. In network matches the same feed, streaks and multi-kills follow replicated events (opponents are neutral OPERATOR numbers, without scripted lines), and the tactical view adds LIVE!, BOMB PLANTED and a once-per-round 1vX CLUTCH TIME callout.

In the browser, choose **Start practice / Resume**, then click the view to capture the mouse. The browser uses Escape to release capture; use your current Menu binding (Escape by default) or F2 for the game menu/settings. Remapping Menu does not remap the browser capture-release key. Opening settings or losing window focus also opens the menu; close settings and explicitly Resume to continue. The page explains capture errors and how to retry. Touch uses a left movement stick, right-side swipe aiming, and FIRE / UP / LOAD buttons (USE for tactical interaction); it does not require mouse capture. Physical-device behavior still needs testing.

The browser game view has a 480-pixel minimum height; scroll the page to reach the whole view on short screens. Narrow menus use a shared row for Restart and End practice, with complete 14-pixel guidance and 44-pixel action targets. End practice returns to the briefing. Binding labels that cannot fit direct to Controls (F2), where the full saved mapping is available; Unbound remains explicit. Replay status has a separate readable area above the vitals; the routine “fresh practice” notice after a restart appears only in the optional browser replay panel, so retries do not leave a banner over the yard. Exceptionally long diagnostic messages show an ellipsis there; the optional replay panel retains their full text.

Practice uses `ferravex_core::session::GameSession` at 64 Hz. The briefing, pause menu, settings and focus loss do not advance the practice world or accumulate catch-up time. **Restart practice** (and **Play again** on the results) resets players, bots, match clock, score and visual feedback. **End practice / briefing** saves any recording and returns to the opening menu. A central dot and outlined hit marker remain readable against the yard; **HIT** appears only for an authoritative hit. Fire cues, muzzle effects, first-person recoil and damage flashes follow accepted FPS events, including acknowledged network events, rather than predicted ammo/health differences. Reload timing still follows the FPS weapon state. Empty ammunition prompts reload. Tactical matches show a round countdown, final team result, and “wait for the next round” on elimination instead of a false respawn countdown.

## Native network mode

Start the local authority and connect a Linux client:

```sh
cargo run --release -p ferravex-fps --bin ferravex-server -- --bind 127.0.0.1:4000
cargo run --release -p strike -- --connect 127.0.0.1:4000 --play
```

The 64 Hz authority owns movement, collision, shots, health, ammunition, reload and respawns. Clients predict and reconcile local movement and interpolate remote players. UDP is a trusted-development/LAN transport without encryption or account authentication. Browser **Multiplayer** connects through the existing WebSocket gateway to the same Rust authority; see the [FPS server guide](../../crates/ferravex-fps/README.md) for server setup.

**The multiplayer menu does not pause the match.** Local input becomes neutral, while native clients and browser clients/hosts continue ticking and processing authoritative state. You remain vulnerable. Practice retry/end actions are unavailable in network mode. Use the browser Multiplayer panel's Disconnect control to return to offline practice; native `--connect` selects network mode for that run.

Tactical servers expose buy phases and bomb objectives. With default bindings, B buys a rifle, N a sniper, V armor, K a defuse kit, G drops the bomb, and holding E interacts with the objective. Scores, phase, timer and round completion feedback follow the server's replicated view. This remains a small FPS slice; production matchmaking and broader native platform hosts remain unfinished. The migration does not change multiplayer messages or replay/save formats.

## Practice replay

With default bindings, F4 starts/stops recording, F5 switches between the last clip and live practice, F6 toggles a ghost, P pauses playback, and Left/Right seek one second. The replay menu also offers **Return to practice**. Native runs support `--record clip.fxrt` and `--replay clip.fxrt`; both require offline mode. Browser replay tools are available with `?replay-tools`.

Replay ticks and seeks remain owned by the recorded stream, separately from the practice session and network clock. Opening local menus/settings does not stop playback: use the dedicated Pause replay action or replay panel to pause it. The overlay distinguishes REPLAY, PAUSED, SEEKING, FINISHED and MISMATCH / ERROR from the actual replay driver. Finishing a clip does not require pausing it, and being paused or stationary does not imply completion. A practice pause contributes no recorded ticks. Restart saves the previous segment and, if recording was active, starts a new recording from the fresh world. End practice stops recording. Returning from playback restores the existing live practice world. Existing `--play`, `--smoke-frames`, `--screenshot` and `--capture-dir` flags remain supported; browser `?autoplay` starts practice directly.

The additive Wasm export `ferravex_replay_finished() -> u32` returns 1 only for an active, successfully verified terminal replay (including a verified empty clip). It returns 0 during seeking, on verification faults, after reset/return to live, or before completion. Like the existing tick/status exports, it is synchronized after the normal game frame, including import and UI command processing. Existing status bits and `.fxrt` / save formats are unchanged. The optional browser panel consumes this signal after each normal frame and shows **Finished**, including paused completion. Seeking and verification errors take precedence. To review a finished clip, seek backward and resume if paused; to restart it, choose **Return to live**, then **Watch replay**:

```js
// Read after the normal ferravex_frame call; do not infer completion from pause.
const finished = wasm.exports.ferravex_replay_finished() === 1;
```

## Presentation preferences

Shared Player tools motion and flash settings are read every presentation frame, including paused menus and frames without simulation ticks. Reduced motion removes first-person weapon bob/recoil/reload motion, crosshair expansion, decorative menu-camera sway, cosmetic particles, the camera-only damage kick, bouncing/rising callout text and the secret's flight. Reduced flash independently removes the damage tint, muzzle light, additive shot particles/traces and the gold edge glow for multi-kills and streaks. Already-active effects are cleared; turning settings back on does not revive them. Health, ammunition, accepted hit confirmation, callout/feed text and authoritative movement remain visible and unchanged. Callouts, the feed and score pops age with presentation time that stops while offline practice is paused.

Gameplay audio uses the shared `GameAudio` pause/resume/reset lifecycle. A local menu suppresses owned gameplay sounds while networking and replay continue. Resuming accepts fresh cues without replaying suppressed sounds. Practice restart and replay cuts clear owned audio/feedback and pending gameplay input. Physical audio output is not validated by unit tests.

## Validation

```sh
cargo test -p strike --offline --locked -j 2
```

Targeted tests cover the deterministic match tracker (multi-kills, streaks, revenge, wall/sky misses, the secret, the exact final tick), real bot gauntlets scoring identically, bot grace/burst tuning, rank/award/personal-best copy, results and info-card layout at 320×480 through 1600×1000, callout priority and reduced-motion clearing, tactical clutch callouts, finished-match host state, ready/autoplay/pause/resume timing, neutral network input without pausing network ticks, actual input-journal taps across zero/catch-up ticks, remapped/unbound hints, replay completion and mismatch/seek/import/reset boundaries, replay progression under local menus, retry recording continuity, accepted combat feedback, live independent preference changes, weapon pose clearing, round labels and small-menu layout. Test native/browser menu input, pointer capture, shooting/reload, death/respawn and network behavior separately from a short screenshot smoke run. Native `--capture-dir directory` writes frame images for local recording workflows; encoding a video is a separate step.

Debug native builds (not release or Wasm) also read screenshot hooks: `STRIKE_WINDOW=WxH`, `STRIKE_AIMBOT=1`, `STRIKE_STILL=1`, `STRIKE_DUCK=1`, `STRIKE_MATCH_SECONDS=N`, `STRIKE_PAUSE_AT=frame`, `STRIKE_ROSTER_AT=frame`, `STRIKE_FAKE_BEST=frags`, `STRIKE_LEARNED=1` and `STRIKE_KEEP_FOCUS=1`, for example:

```sh
STRIKE_AIMBOT=1 STRIKE_MATCH_SECONDS=15 STRIKE_KEEP_FOCUS=1 \
  cargo run -p strike -- --play --smoke-frames 1000 --screenshot /tmp/strike-results.png
```

Visual polish: pistol/rifle/sniper use distinct original meshes and matching muzzle offsets.
The animated foreground weapon does not cast or receive world shadows and stays visible
near walls. Dockyard uses the shared sun shadow instead of overlapping baked shadow quads.
Use Controls → Devices → Visual style to compare palettes, and Graphics quality for Low
(no dynamic shadows), Medium or High. See [visual presets](../../docs/VISUALS.md).

## Browser player update

Open **Details → Player tools** for screenshots, clean sharing links, frame limits, session timing, screen wake lock, accessibility preferences and settings backups. All shared player upgrades apply to this example. Read the [player guide](../../docs/PLAYER_GUIDE.md) for behavior and browser limits.
