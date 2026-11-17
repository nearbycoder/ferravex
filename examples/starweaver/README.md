# Starweaver

A complete, finite exploration and fabrication expedition. You command **The Little Light**, a survey cutter escorting a colony ark away from a dying star. Recover three ancient relics, manufacture their gate mechanisms, return to Loom Dock and open the gate before watch 52. Zero hull ends the expedition. Restart immediately with R.

The game uses Ferravex's shared arcade host for its title screen, field manual, pause/restart, accepted-event audio, terminal screen and local best-score storage. Simulation is separate from drawing and runs through fixed 60 Hz inputs. Planning is untimed: watches advance only when expedition work is accepted. Lane travel animates for 120 ticks without accepting another command.

## Reading the screen

Starweaver follows the catalog's [visual language](../../docs/VISUAL_LANGUAGE.md): icons and numbers instead of labels, previews instead of instructions, and effects where things happen.

- **Header.** Four pips count the gate parts (the fourth is opening the gate) beside the current sector's name. Below, a small gate ring lights one arc per built part, three relic badges glow gold when recovered and gain a check once forged, and an arrow points at the next step: a pin and a sector, a hammer and a part, home, or the gate. On the right, a dial counts the watches left (calm, then gold, then red) beside hull, fuel, credits, alloy (gear), crystal (gem), circuits (bolt) and the hold.
- **Tabs.** Key caps 1–5 with a map, magnifier, hammer, rocket and flag. A dot on a tab means something worth doing waits there: a useful survey or a claimable landmark, a forgeable gate part, low hull or no fuel, or a gate ready to open.
- **Chart.** The dying star in the corner swells, reddens and beats faster as watches pass. Current sector: cyan ring and the ship. Selection: turning gold arcs and a marching route along the lanes. Surveyed sectors show a radar sweep; each landmark has a badge (a check once claimed); a red triangle marks unvisited first-entry damage; gold sonar rings mark the current goal. Hovering a star previews the route and its total fuel, watches and first-entry hull.
- **Panels.** Every action row shows its consequence as icons: deposit pips and the haul (`+2`), costs with an arrow to gains, red for what you lack, a lock until surveyed. The confirm button names the action and shows its cost, or why it is refused, in a dark well. Focusing a work row ghosts the haul into the hold slots; the Forge pulses the gate arc or ship bay a recipe fills; the Mission panel previews the escape bonus for crossing on the next watch.
- **Feedback.** Mining bursts in the ore's color, surveys ring out, relics and rescues earn a callout, damage shakes and pops a heart at the star, forging sparkles on the gate, and costs rise from the header stats. A refused command shakes, rings red on the thing that refused and pops its reason (`crate Full`, `lock Survey`, `fuel Need 2`). Nothing narrates in sentences.

## Content

- **9 distinct sectors**, connected by **13 bidirectional lanes** with branching routes.
- **3 resources** with finite deposits; **8 unique recoverable landmarks**, including **3 relics** and the optional rescue of **9 engineers**.
- **3 branching encounters**, each offering **3 approaches**: the Hollow Choir, the Sentinel and Copper Reef's privateers.
- **6 fabrication recipes**: **3 mandatory gate components** and **3 optional permanent ship upgrades**.
- **2 trading stations**, **6 ship services**, a one-use distress tow and **5 interactive panels**.
- A completed gate-opening ending that acknowledges the optional rescue, two survival failure causes, immediate restart and an efficiency-based score.
- Original procedural chart art: planets, a comet tail, debris belts, a fractured moon, shipwreck, rotating satellites, a ringed sentinel, the old gate and an animated survey ship. No downloaded artwork or external dependencies.

### The chart

Deposits below are the original alloy / crystal / circuit quantities. Mining and salvage never regenerate them. First-entry damage happens only on the first visit; extraction damage applies to each accepted mining or salvage operation. Both are halved, rounding up, by a Radiant Shield.

| Sector | Deposits A / C / CIR | First-entry / extraction damage | Landmark |
| --- | --- | --- | --- |
| Loom Dock | 0 / 0 / 0 | 0 / 0 | Home gate and trading station |
| Cinder Belt | 8 / 1 / 1 | 0 / 0 | One freight cache: 2 alloy |
| Glass Comet | 1 / 7 / 0 | 0 / 0 | One collector: up to 4 fuel |
| Wayfarer | 5 / 0 / 5 | 6 / 0 | Navigator's key and 20 credits |
| Hollow Choir | 0 / 7 / 2 | 10 / 2 | Singing lens encounter |
| Verdant Archive | 2 / 5 / 2 | 0 / 0 | Trading station; 2 seed crystals and 15 credits |
| Copper Reef | 5 / 2 / 6 | 8 / 1 | Privateer encounter |
| Silent Crown | 5 / 5 / 5 | 14 / 2 | Sentinel core encounter |
| Far Beacon | 1 / 3 / 2 | 0 / 0 | Rescue 9 engineers: +10 hull, +25 credits |

The chart names every sector and marks lanes, hazards, landmarks, surveys and the goal; the Chart panel shows the selected sector's landmark and rewards, deposit stocks as pips, shielded hazard costs, station services and the route there with its total cost. Stocks are visible before surveying to support route planning.

## Controls

| Input | Action |
| --- | --- |
| 1–5 / click a tab | Chart / Survey / Forge / Ship / Mission |
| Arrows or WASD | Select a star, action row or encounter approach |
| Space or Enter | Accept the selected action |
| Click a star, then click it again (or the jump button) | Select it, previewing the route; then travel along a connected lane |
| Click an action row or bottom button | Accept that action |
| X or E on a Survey resource row | Discard one unit of that cargo; no reward or watch cost |
| X or E on Ship | Cycle the resource to sell |
| H | Shared paginated field manual |
| P or Escape | Shared pause menu |
| R | Restart the entire expedition with fresh stores |
| M | Shared sound toggle |

During transit, arrival progresses automatically and other gameplay commands are ignored. In an encounter, arrows/Space or clicking an approach resolves the choice; each approach shows its costs and rewards, and unaffordable ones are dimmed with the missing store in red. Mission information is read-only except for the explicit gate-opening action. Hover highlights use the same hit regions as clicks.

## Rules and fabrication

Start at Loom with **100 hull, 8/10 fuel, 60 credits, 1 alloy, 1 circuit and a 12-unit hold**. Jumping costs one fuel and one watch. Survey a sector once before extracting resources or exploring its landmark. An extraction transfers up to two units, bounded by the remaining deposit and free hold space. Relics and fitted components occupy dedicated mounts, not the cargo hold.

Every accepted jump, survey, extraction, completed discovery, build or ordinary service spends **one watch**. The distress tow spends six. Declined or unaffordable actions, menu navigation, idle planning and the Choir's withdrawal spend none. Reaching watch 52 loses, including attempting to open the gate on that watch. Hull damage may end a run before another watch is spent.

| Recipe | Alloy | Crystal | Circuit | Requirement or effect |
| --- | ---: | ---: | ---: | --- |
| Beacon spindle | 3 | 2 | 1 | Navigator's key from Wayfarer |
| Phase loom | 2 | 3 | 1 | Singing lens from Hollow Choir |
| Heart engine | 3 | 2 | 3 | Sentinel core from Silent Crown |
| Radiant Shield | 2 | 0 | 1 | Halve all hull damage, rounding up |
| Cargo Cradle | 3 | 0 | 1 | Increase hold capacity from 12 to 18 |
| Deep Scanner | 1 | 2 | 0 | Extract up to three units per action |

Each recipe can be built once. Costs are checked before any ingredient is consumed. Gate components need their named relic; upgrades do not. The Forge panel shows each cost against the hold, the required relic and the effect.

The Choir permits a powered probe for **1 fuel**, a dive risking **18 hull** and recovering one bonus crystal if space permits, or a free withdrawal. The Sentinel permits a **2-circuit override**, a **30-credit toll**, or forced entry risking **34 hull**. Privateers offer **3 circuits for 1 alloy** (requiring two free hold spaces), an escape costing **2 fuel**, or a fight risking **22 hull** and awarding **40 credits**. Each completed landmark resolves only once.

Ship services are:

- Condense one crystal into up to three fuel.
- Spend one alloy and one circuit to repair up to 25 hull.
- At Loom or Verdant, buy up to five fuel for 15 credits.
- At Loom or Verdant, repair up to 45 hull for 20 credits.
- At Loom or Verdant, sell one alloy/crystal/circuit for 8/12/16 credits.
- Once per expedition, tow back to Loom, set fuel to five, take 15 damage before shielding, and spend six watches. Cargo is preserved. The tow cannot be used at Loom.

Full-tank and full-hull services do not consume resources. Partial fills consume the full advertised price, so plan ahead. There is no material-buying loop, infinite discovery reward or recurring deposit. If all fuel options and the tow have been exhausted, R begins a fresh expedition.

## Strategy and score

Begin Loom → Cinder → Wayfarer to recover the key and salvage alloy and circuits. Cinder connects to Hollow Choir's rich crystal deposits. Keep crystals for both fabrication and fuel; spend them deliberately. Forge components as materials arrive to release hold space. Shielding can pay for itself on a hazardous route, while a scanner exchanges early resources for later watches. The upgrades are optional, not prerequisites.

Loom and Verdant offer safe station services. Copper Reef provides a circuit-heavy alternate route. The optional Far Beacon rescue repairs the hull and earns credits, but requires travel and two actions if unscanned. Surveying every sector may improve discovery score while consuming time needed for the gate.

Scans award 75 points; each relic and the privateer resolution awards 250; each component awards 500; each upgrade awards 100. Cinder and Glass landmarks award 100 each, Verdant's archive awards 150, and rescuing the engineers awards 350. Resource extraction itself awards no score. A successful escape adds **1200 + 25 × remaining watches + 3 × remaining hull + min(credits, 500)**. The terminal result reports time, mapped sectors, hull, rescued or abandoned engineers and final score. A new run allows a different route or a better result.

## Run and validation

From the repository root:

```sh
cargo run -p starweaver --offline --locked -j 2
cargo run -p starweaver --offline --locked -j 2 -- --headless
cargo test -p starweaver --offline --locked -j 2
cargo clippy -p starweaver --all-targets --offline --locked -j 2 -- -D warnings
rustfmt --edition 2024 --check examples/starweaver/src/*.rs
```

The headless pilot emits only ordinary player inputs into the same rules. It finishes at **tick 2424, watch 37, 64 hull, 4817 points**, with all three mechanisms, all three relics, all nine engineers rescued and at least eight sectors visited. It surveys five sectors; its successful route does not claim to demonstrate every optional choice or upgrade.

Validation completed: **24 behavioral tests plus one executed public-API doctest**, strict scoped Clippy, owned-file formatting and the complete headless expedition. Regressions cover recipes and duplicate builds, finite stocks, capacity, upgrades, invalid routes, encounter affordability, loss/restart, deadline/idle behavior, transit input handling, service costs, trading, tow limits, mouse regions, the click-again jump, terminal immutability and reproducibility. Presentation tests cover header progress, short objectives, per-tick events, effects for surveys, extraction, relics, refusals and damage, lane routes, and that every action preview agrees with what the rules accept across the whole bot expedition. The complete-win test begins at `Game::new()` and never injects state.

The parent also inspected a native autoplay capture at frame 1100 and reported a readable chart, sidebar and transit/resource display with no overflow. For further parent-owned captures, `--autoplay --smoke-frames 1100 --screenshot PATH` gives a mid-expedition chart; 2500 reaches the completed ending. Headless validation is not a comprehensive manual playtest. Browser/Wasm packaging, audio behavior, persistence, controller/remapping integration and the remaining panel/branch screenshots are owned by the parent integration; this game has no standalone network or unfinished-campaign save claim.
