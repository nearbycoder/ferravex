# The Twenty: complete games built with Ferravex

Twenty independent games commissioned on 2026-10-27. Each game has a distinct implementation agent and owns its own rules, content and art. The parent owns the shared host, registration and combined validation. This collection does not resume the deferred Journey400 backlog.

## Collection

| Game | Genre | Campaign |
| --- | --- | --- |
| [Mosslight](../examples/mosslight/README.md) | Turn-based dungeon expedition | Six dungeon floors, telegraphed enemies, relic choices and the Root King. |
| [Signalbox](../examples/signalbox/README.md) | Railway dispatch puzzle | Eight railway shifts and 61 trains, with route switches, signals and deadlines. |
| [Saltwind](../examples/saltwind/README.md) | Sailing and merchant strategy | Six ports and six charters; trade, provision, upgrade and repay your debt. |
| [Hearthharvest](../examples/hearthharvest/README.md) | Farming and village management | Fourteen farming days with crop growth, weather, orders and a harvest festival. |
| [Paper Alibi](../examples/paper-alibi/README.md) | Detective deduction adventure | Six cases, 18 suspects and 42 evidence records to investigate and connect. |
| [Voltage Choir](../examples/voltage-choir/README.md) | Four-lane rhythm campaign | Six original tracks, 760 notes and 124 bars, with chords, timing grades and Encore. |
| [Ironclad Tactics](../examples/ironclad-tactics/README.md) | Turn-based squad tactics | Six missions, three squad roles, AP, cover, rescue and capture objectives. |
| [Deep Pressure](../examples/deep-pressure/README.md) | Submarine rescue survival | Six submarine sorties and 27 survivors, with oxygen, ballast, hazards and refits. |
| [Starweaver](../examples/starweaver/README.md) | Space exploration and crafting | Nine sectors, 13 lanes and six recipes; recover relics and construct a jump gate. |
| [Pantry Panic](../examples/pantry-panic/README.md) | Cooking service management | Seven services, 42 orders and six recipes; prep, cook, plate and serve. |
| [Prism Foundry](../examples/prism-foundry/README.md) | Optical logic puzzle | Twelve optical puzzles with mirrors, splitters, RGB filters, undo and stars. |
| [Mooncourt](../examples/mooncourt/README.md) | Lunar racket sports | Six racket matches with serves, deuce, ballistic shots, stamina and training. |
| [Wildfire Watch](../examples/wildfire-watch/README.md) | Wildfire emergency strategy | Six emergencies and 59 residents; deploy three crew roles against forecast fire spread. |
| [Gilded Deck](../examples/gilded-deck/README.md) | Roguelike deck-building campaign | Nine encounters, 16 playable cards and six relics, with rewards, shops and upgrades. |
| [Gravity Post](../examples/gravity-post/README.md) | Orbital lander delivery | Eight planetary deliveries with wind, gravity, fuel, fragile cargo and landing limits. |
| [Velvet Heist](../examples/velvet-heist/README.md) | Stealth infiltration campaign | Six stealth heists with patrols, vision, hiding, distractions, loot and extraction. |
| [Tin Automata](../examples/tin-automata/README.md) | Robot programming puzzles | Twelve workbenches, 17 parcels and seven opcodes, with editable tapes, switches and execution control. |
| [Fossil Frontier](../examples/fossil-frontier/README.md) | Excavation and research management | Six dig sites and 24 specimens; tools, research and upgrades build a six-part museum skeleton. |
| [Stormbound](../examples/stormbound/README.md) | Naval artillery campaign | Eight naval artillery battles with wind, ballistic shots, ammunition and ship repairs. |
| [Crown & Council](../examples/crown-council/README.md) | Kingdom strategy | Thirty petitions plus nine conditional variants, four projects, four policy endings and collapse conditions. |

## Play locally

Every game is a native executable and a browser WebAssembly library. For example:

```sh
cargo run -p mosslight --offline --locked -j 2
cargo run -p signalbox --offline --locked -j 2
cargo run -p saltwind --offline --locked -j 2
```

Use any package ID from the collection above. The native renderer needs a graphics display. To build the browser catalog:

```sh
CARGO_BUILD_JOBS=2 python3 -B scripts/build-web.py release
python3 -B scripts/serve-web.py --bind 127.0.0.1 --port 8000
```

Open the local catalog and select a game. Existing games retain their routes.

## Shared controls and lifecycle

Enter begins a campaign. The in-game footer and H field guide explain each game’s controls; arrow keys turn guide pages. Esc/P pauses, R begins a fresh campaign, M toggles sound, and F2 opens control remapping. The shared host uses Ferravex GameSession at 60 Hz and consumes input edges only in executed ticks. A completed campaign freezes further simulation until retry. Browser keyboard/pointer and native keyboard/pointer use the same model.

Scores and completed-run records use the existing versioned local storage. Unfinished campaigns are not saved. Autoplay generates legal commands through each game’s ordinary rules and uses transient storage; it does not alter personal records. No external assets, libraries or network services are required.

## Ownership and evidence

The [contract](games20/CONTRACT.md) defines the public host interface and acceptance requirements. [The dispatch ledger](games20/plan.json) records the twenty assignments. Per-game implementation receipts live under `target/games20/receipts/`; parent validation and captures live under `target/games20/`.

All twenty distinct implementation agents completed their assignments. Parent integration finished on 2026-10-28. The catalog now contains 39 entries, preserving the original nineteen. No engine crates or earlier Journey implementations were changed.

## Combined validation

- **315 Rust checks passed:** 284 game behavior tests, ten shared-host tests and 21 executed rustdoc examples. Every game proves a complete winning campaign from a fresh model using ordinary input commands. Strict all-target Clippy and scoped formatting checks passed for all twenty games and the shared host.
- **All twenty compiled browser campaigns won.** `web/tests/wasm-arcade.mjs` exercises each release artifact through legal autoplay, checks its actual ending and terminal freeze, and verifies that demonstration runs leave personal records unchanged. It also checks start, pause, help, controls and restart at 960×640, 1280×720 and 390×608. These are forty integration tests, with the lifecycle test running at three sizes per game.
- **574 browser tests passed, zero failed.** One optional native-status fixture test was skipped because its fixture was not supplied. This full suite includes the existing games and the new collection. The Python catalog suite passed 46 tests; site source tests passed nine; generated-site tests passed ten, including every game route, guide link, media asset and stale-input rejection.
- **Native executables, release WebAssembly and site built.** All twenty games were rendered and visually reviewed on this Linux workstation. The complete 39-entry browser catalog and fresh workspace Rustdoc were staged into `dist/site`. The dependency audit passed for all 54 packages, all features and targets, with zero external Cargo packages.
- **Gameplay media is current.** Each new game has a native screenshot, browser poster, three stills and a gameplay loop. All twenty recorded Wasm hashes match the canonical release build. Ordinary browser input checks also covered Mosslight's menus, Saltwind's market, Starweaver's panels, Prism Foundry's first puzzle solved with the pointer, and Crown & Council's decrees, project purchase and chronicle.

The combined Rust commands used all twenty package IDs plus `ferravex-arcade`, with `--offline --locked -j 2`; Clippy also used `--all-targets -- -D warnings`. Exact command arrays and results are in `target/games20/validation/checks.json`, with the final counts in `target/games20/validation/summary.json`. Per-game READMEs and receipts describe their implementation agent's checks; this section records the subsequent parent integration.

To repeat the browser and site checks after building:

```sh
node --test --test-concurrency=2 web/tests/*.mjs
python3 -B scripts/audit-owned.py
python3 -B scripts/audit-catalog.py
python3 -B scripts/build-site.py --self-test
python3 -B scripts/test_site.py
```

The generated documentation site can be played locally with:

```sh
python3 -B scripts/serve-site.py --bind 127.0.0.1 --port 8000
```

Open `http://127.0.0.1:8000/examples/` and select a game. A fresh full site build uses `CARGO_BUILD_JOBS=2 python3 -B scripts/build-site.py`.

## Validation limits

These are complete finite campaigns with authored content, progression, endings and retry flows. Automated winning routes and graphical checks do not establish human balance, accessibility of every interaction, or recovery from every possible mistake. Initial integration validated desktop keyboard/pointer; the mobile follow-up adds touch controls and the checks below. Physical mobile devices, gamepads, audio-device listening and other operating systems were not verified. Unfinished campaigns are not saved; completed-run records use local storage. Publication is tracked separately from this validation report.

## Mobile follow-up

The common host now binds actual touch contacts to its game actions and canvas
buttons. Touch the canvas to reveal movement arrows, Act (Space/Enter), X/C,
choices 1–5, Pause and Guide. Controls support simultaneous holds, including
movement plus thrust or cooking. They occupy the side margins in wide landscape
views and a separate bottom dock on narrower screens, with targets at least 44
screen pixels. The pause card also has a tappable New Campaign action.

The expanded browser player follows the visible viewport, preserves notch/home
indicator safe areas, and locks/restores page scrolling. This fallback works
without a native fullscreen API; browser-owned address bars can remain visible.
The phone header keeps the menu button in one row above the canvas.

`web/tests/wasm-arcade-touch.mjs` checks touch-only start, pause, guide navigation
and resume for every game at 390×844, 844×390 and 320×480, plus a Prism Foundry
puzzle solved with real touch inputs. Chromium touch emulation checks the browser
input bridge and expanded player. This Linux environment has no iOS simulator;
those checks do not substitute for a physical iPhone Chrome playtest.

Workspace Rustdoc completed with one existing `owned-lab` warning about a public documentation link to the private `GRADE_THRESHOLDS` item. That unrelated implementation was preserved.

## Gameplay text cleanup

All twenty games now use short objectives, functional labels and compact mission briefings. Repeated instructions and decorative story panels have been removed or reduced; optional rules and requested hints remain available. Detective evidence and statements retain the facts needed to solve every case. Crown & Council advances directly after each choice, with resource changes kept in optional History. Shared title, pause and result screens show only the actions and results needed there. The mobile touch dock and fullscreen behavior are preserved.

The cleanup passed 319 Rust unit and documentation checks, strict all-target Clippy, and 595 browser checks with one optional fixture skipped. All twenty release-Wasm campaigns reached victory; all twenty retained touch-only menu navigation at phone portrait, landscape and small-screen sizes. Native captures were reviewed across the collection, with browser checks of the shorter title/Help screens and immediate council advancement. Validation evidence is under `target/text-cleanup/`.
