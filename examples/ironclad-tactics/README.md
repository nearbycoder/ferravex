# Ironclad Tactics

Three borrowed machines liberate the Copper Coast across six authored squad-tactics operations. Plan an entire squad turn, move out of telegraphed attacks, spend action points on fire or specialist skills, and choose five permanent upgrades before the final Warden battle.

This is a complete campaign with mission briefings, distinct objectives, bounded enemy turns, reserve waves, limited mission retries, score/medals, and a liberation ending. The game uses the first-party Ferravex arcade host, renderer, math and four-neighbor navigation grid; it has no added dependencies or external assets.

## Play

```sh
cargo run -p ironclad-tactics --offline --locked -j 2
```

The shared host supplies title/start, pause, help, remapping, sound, whole-campaign restart, records and native/browser presentation. Game-specific rules are renderer-independent. Rendering is procedural: riveted bulkheads, striped cover, tracked squad machines, enemy chassis, route dots, reserve markers and fixed enemy aim lines.

| Control | Action |
| --- | --- |
| Space/Enter | Deploy from briefing; execute the cursor's contextual order |
| 1 / 2 / 3 | Select BRASS / KESTREL / MICA; choose upgrade at debrief |
| Arrows or WASD, tapped | Move targeting cursor; does not spend AP |
| Click board | Select friendly machine, shoot an enemy, interact with objective, or move |
| Click squad card | Select that machine |
| X/E | Selected machine's specialist skill |
| C/Q or 4 | End the entire squad turn |
| Space/X after setback | Spend one retry and restart the current operation |
| C after setback | Concede the campaign |
| Esc/P / H / M / R / F2 | Host pause / help / sound / whole-campaign restart / remap |

Aim at an ally **with the cursor** and press X to repair it while MICA remains selected. Clicking an ally selects it. If no living ally occupies the cursor, MICA repairs herself. The sidebar also provides click targets for executing the cursor order, using the skill and ending the turn.

## Squad and rules

| Machine | Hull | AP each turn | Cannon damage / range | X skill |
| --- | ---: | ---: | --- | --- |
| BRASS, Vanguard | 18 | 4 | 4 / 4 tiles | Brace: 1 AP; incoming hits reduced by 2 until next turn; moving cancels brace |
| KESTREL, Scout | 14 | 5 | 3 / 6 tiles | Focus: 1 AP; next shot gains 2 damage; cannot stack |
| MICA, Engineer | 15 | 4 | 3 / 4 tiles | Patch: 2 AP + one kit; repair up to 6 hull within Manhattan distance 2 |

- Movement costs 1 AP per tile along a clear four-way path. Walls and living actors block paths. The cursor previews the path; a destination exceeding available AP is rejected in full.
- A cannon shot costs 2 AP. It requires Manhattan weapon range and symmetric, conservative grid line of sight. Solid walls and closed diagonal corners block sight. Low cover reduces incoming damage by 1. Minimum damage is 1.
- Enemies plan one action each at the start of the squad turn. Red lines/crossed target tiles mark fixed shots. Amber lines mark one-tile movement. Ending the squad turn resolves each surviving enemy's announced action once; attacks do not retarget a machine that moved. AP refills afterward. Idle ticks and held movement keys never advance turns.
- Patrols have 6 hull, range 5 and damage 3. Rammers have 8 hull, range 1 and damage 4. Immobile sentries have 7 hull, range 6 and damage 3. The Warden has 18 hull, range 5 and damage 4.
- An objective interaction costs 2 AP from its tile or an adjacent tile. Clicking a distant objective routes toward it; issue another order when in range to interact. Capture/hold requires a living ally within one tile and no enemy within one tile for consecutive enemy phases. An unguarded or contested phase resets its progress.
- The rescue operator attaches to the interacting machine. Only that carrier must reach E, but its destruction immediately fails the operation. Objectives and defeated enemies cannot yield repeated rewards.
- All three machines are repaired at each new operation; MICA receives two kits. Choose one squad-wide permanent upgrade after each of the first five victories: +3 maximum hull, +1 cannon damage or +1 maximum AP. Each upgrade has two ranks.
- The campaign has three retries in total. A retry reloads the same operation, keeps previously earned upgrades, and rolls score back to the last completed operation. A fourth failure ends the campaign. Missing a mission's turn deadline or losing the entire squad also fails the operation.

## Authored operations

Each board is 11 by 8 tiles, with three controllable roles and at most eight enemy slots.

| Operation | Enemies | Objective | Deadline |
| --- | ---: | --- | ---: |
| Breakwater | 4 | Disable the coastal patrol | 20 turns |
| The Captive Signal | 4 | Rescue the operator at O; escort the carrier to E | 24 turns |
| Relay Nine | 5 | Activate O, then guard it for 2 enemy phases | 24 turns |
| Bridge of Sparks | 5 total | Activate O; guard for 4 phases and survive through turn 6 | 24 turns |
| Foundry Warden | 4 | Disable O and P, then clear the patrol | 26 turns |
| The Last Engine | 6 | Defeat the Warden and guard, then activate O | 28 turns |

Bridge reserves arrive on turns 3 and 5. An occupied marked entry uses the nearest empty floor, with deterministic tie-breaking; newly arrived reserves telegraph before their next action. Rescue, capture and hold do not require eliminating every enemy. The final objective and guard may be completed in either order.

Score is 100 per disabled enemy, plus 1,000 per operation, 150 per surviving machine and 25 per remaining deadline turn. Earn three medals for all three survivors within half the deadline; two medals for at least two survivors otherwise; one for completing with one survivor. Failed-mission points are discarded on retry. The shared host retains completed-run records; there is no mid-campaign save.

## Strategy

Disable a marked attacker before ending the turn, or move off its target square. A full squad has enough AP to combine advancing, concentrated fire and protection. Use KESTREL's range to remove sentries while BRASS crosses gaps. Reserve enough AP for a shot after moving; spend leftover AP on focus or brace. Keep MICA close enough to repair a damaged escort carrier. Leave corridors open for extraction, and keep a guard beside an activated relay instead of chasing every remaining enemy. Cannon upgrades shorten patrol battles; armor and drive upgrades support riskier captures and faster escorts.

## Verification and replay

```sh
cargo test -p ironclad-tactics --offline --locked -j 2
cargo clippy -p ironclad-tactics --all-targets --offline --locked -j 2 -- -D warnings
cargo run -p ironclad-tactics --offline --locked -j 2 -- --headless
```

Executed: **14 behavioral tests and 1 runnable rustdoc example passed**, strict all-target Clippy passed, and owned Rust files passed formatting. Tests cover authored content, AP/path costs, idle/invalid inputs, symmetric LOS, cover, shot rewards, focus/repair/brace resources, fixed enemy fire, enemy movement, occupied reserve entries, loss/retry/score rollback, objective contests/deadlines, upgrade caps, controller edge cases, full campaign completion, deterministic command replay and terminal-state invariance.

The headless controller submits ordinary selection, cursor, click, skill and end-turn inputs to `Game::step`; it neither modifies game state nor grants invulnerability. Its complete run reaches all six operations, keeps all three retries, earns 18 medals, and wins at model tick **2420** with **14,175** points. Mission starts occur at ticks 0, 311, 821, 1171, 1631 and 1991.

Parent native smoke commands can use:

```sh
cargo run -p ironclad-tactics --offline --locked -j 2 -- --autoplay --smoke-frames 240 --screenshot target/games20/ironclad-tactics-240.png
```

Useful model-tick capture windows are 180 (first board), 500 (rescue), 1000 (relay), 1400 (bridge), 2100 (final battle) and 2500 (ending). Native frame counts and model ticks can differ. The parent inspected an actual native frame-240 screenshot and reported readable board, AP panels, intentions, labels and footer.

Limits: automated legal-input completion demonstrates one successful build and route, not exhaustive human balance testing. Individual mechanic tests use isolated fixtures; the complete-campaign test starts from `Game::new()` and uses only legal commands. Browser/Wasm, touch devices, native audio, remapping and shared persistence are integration checks owned by the parent and were not separately exercised by this implementation agent. No workspace build or shared staging was run.
