# Wildfire Watch

A complete, turn-based incident-command campaign about getting a valley home
before dawn. Coordinate an engine, a ranger crew and a rescue truck across six
authored emergencies. Fire has an exact forecast; saving everyone still requires
choosing where to spend action points, water and air support.

## The campaign

| Incident | Residents | Turn limit | Situation |
| --- | ---: | ---: | --- |
| Pine Creek | 6 | 18 | Two hamlets and a road through the pine belt |
| Orchard Wind | 8 | 18 | Two orchard settlements under shifting wind |
| Ridge Road | 9 | 20 | Three villages between rock and dry brush |
| Ember Ferry | 12 | 22 | Three settlements, a river and one bridge |
| Red Sky | 12 | 22 | School, mill and depot threatened by four ignitions |
| The Dawn Line | 12 | 24 | Four final evacuations under changing winds |

There are **59 residents, 17 villages and 18 starting ignitions** across the six
11-by-8 maps. Each map has its own terrain, village placement, wind sequence,
briefing and tactical advice. Finish every evacuation and contain every active
fire to reach the dawn ending. A cleared incident unlocks the next one; there is
no level-select bypass.

## Controls

| Control | Action |
| --- | --- |
| 1 / 2 / 3 | Select Engine / Ranger / Rescue |
| Arrows / WASD | Move the targeting cursor, one tile per fresh press |
| Space / Enter | Perform the context action at the cursor |
| Click map | Select a tile and attempt the same context action |
| X / E | Cut an adjacent firebreak |
| C / Q | End the turn and advance fire and wind |
| 4 | Buy an air drop centered on the cursor |
| 5 | Show or hide the next-ignition forecast |
| H | Open the full paginated field guide |
| Esc / P | Pause or resume |
| R | Start a fresh six-incident career |
| M / F2 | Toggle sound / inspect or remap controls |

The crew cards and action labels in the right panel also accept pointer input.
At a briefing, Space deploys the crews. At a debrief, 1–3 purchase the shown
upgrades and Space proceeds; the upgrade cards also accept clicks. At a setback,
Space spends a retry. Planning, cursor movement, help and ordinary idle ticks do
not advance the incident.

## Crews, actions and priorities

Every crew begins with four action points per turn. Move to any reachable tile
whose complete path fits the remaining points: living forest costs two per tile,
other walkable terrain costs one. Water, rock and fire block movement. Crews can
share a tile; each keeps independent action points, water and passengers.

| Crew | Starting water | Hose range | Heat removed | Passenger capacity | Firebreak cost |
| --- | ---: | ---: | ---: | ---: | ---: |
| Engine | 6 | 2 tiles | 3 | 2 | 2 AP |
| Ranger | 3 | 1 tile | 2 | 2 | 1 AP |
| Rescue | 3 | 1 tile | 2 | 6 | 2 AP |

Ranges use orthogonal tile distance. A hose action costs **two AP and one water**.
Completely extinguishing a tile wets it for three spreads. A partial application
reduces its heat; another crew can finish the job. Stand beside base or water and
target it to refill a partly empty tank for one AP. Repeated attempts at a full
tank cost nothing.

Stand on or beside a populated village and target it to load residents for two
AP, up to the crew's passenger capacity. Aboard passengers are **not yet rescued**.
Return to base or an adjacent tile and target base to unload them for one AP.
There is no passenger transfer between crews. An empty village may still burn,
but its already evacuated residents remain safe.

Context actions take precedence over movement: targeting an adjacent populated
village attempts loading, and targeting nearby base with passengers attempts
unloading. If the relevant action is unaffordable, it is rejected rather than
silently moving the crew. The right panel names the currently valid action.

Cutting an unburned grass, brush or forest tile removes its remaining fuel
permanently. Cut breaks cannot reignite and cost only one AP for Rangers. Burning
tiles and villages cannot be cut. A repeated cut does not consume points.

## Fire, forecasts and failure

Pressing C performs one simultaneous spread using the currently displayed wind:

1. The old fire pattern determines every next ignition. Neighboring heat adds;
   the downwind contribution gains two. Required heat is four for forest and
   villages, three for grass and two for brush.
2. Existing fires consume one fuel and grow one heat, up to three. A fire whose
   fuel runs out goes out, though its final embers can still ignite a neighbor in
   that same spread. A newly ignited tile starts at one heat and cannot spread
   again until the next turn.
3. Wetness decreases after blocking that spread. A value of three therefore
   protects exactly three spreads. Roads, water, rock and cut breaks have no fuel.
4. Crews regain their action points and the forecast advances to the next wind.

Red borders and exclamation marks show the exact next ignitions. Blue outlines
show wet ground. Existing flames include a heat number. The map uses no smoke
overlay that hides targets. Cursor inspection reports the target's fuel and heat;
villages show their remaining population.

An occupied village catching fire fails the incident. A crew caught by new fire
with passengers also fails it. An empty crew retreats to base with no water;
this affects the medal but can be recovered. A burning, empty village is damaged
and limits the incident to bronze. Missing the turn deadline also fails it.

Each incident supplies two air-drop sorties, plus any air-support upgrades.
One drop costs **12 credits** and one sortie. It removes three heat from the
cursor tile and its four orthogonal neighbors and wets those five tiles. A drop
without any fire in that footprint is rejected without spending either resource.

There are **three retries across the whole career**. A retry restores the failed
incident's map, starting credits and uncompleted rescues. Earlier completed
incidents and purchased equipment remain intact. The fourth failure ends the
career. Retrying cannot farm cash, rescues, medals or score.

## Equipment, medals and scoring

Every cleared incident pays 25 credits plus three per resident. The career starts
with 50 credits. Unspent credits carry forward, so a drop now competes with an
equipment investment later.

| Upgrade | First / second cost | Effect per level |
| --- | ---: | --- |
| Tanks | 30 / 45 | Every crew carries two more water |
| Training | 35 / 50 | Every crew receives one extra AP each turn |
| Air base | 40 / 55 | One extra air-drop sortie per incident |

All three upgrades have a hard cap of two levels and take effect on the next
dispatch. Buying an unaffordable or capped upgrade changes nothing.

Gold requires all villages intact, no crew retreats and at most one air drop.
Silver requires intact villages and no more than one retreat. Other successful
incidents earn bronze. The campaign has 18 possible medals.

Each cleared incident banks this score once:

```text
1000 + 100 × residents rescued + 5 × surviving forest tiles
     + 20 × unused turns + 200 × medal count
```

No score is awarded for repeated orders or failed attempts. The shared host saves
personal best scores and completed-run records when storage is available. An
unfinished career is not saved. Autoplay leaves personal records unchanged.

## Strategy

Use Rescue for the large evacuations while Engine keeps the nearest fire front
quiet. Let Rangers create breaks or finish weak flames. A loaded rescue crew is
vulnerable: inspect its return route before ending the turn. Roads provide fast,
nonflammable paths, and the river becomes a refill source as well as a barrier.

Plan enough AP to load or unload after arriving. Do not spend an entire tank on
isolated flames that will burn out safely. A well-placed drop can stop several
fronts at once; a poorly placed one costs the same twelve credits. Training is
especially useful for the river crossing and final four-village evacuation.

## Run and validation

From the repository root:

```sh
cargo run -p wildfire-watch --offline --locked -j 2
cargo run -p wildfire-watch --offline --locked -j 2 -- --headless
cargo test -p wildfire-watch --offline --locked -j 2
cargo clippy -p wildfire-watch --all-targets --offline --locked -j 2 -- -D warnings
```

The legal planner wins at tick **3630**, saving **59 people**, earning **15 medals**
and **17,565 points**, with all three retries intact. It submits the same select,
target, action and end-turn inputs as a person. It never writes progression,
cash, fire, passengers or success state. The tests repeat that full command stream
and compare the resulting state at every tick.

The 17 behavioral tests cover complete victory, authored-map reachability,
forecast simultaneity, wet lifetime, movement/AP costs, hose roles, refill
admission, permanent breaks, passenger capacity and delivery, air-drop footprints,
village/crew hazards, deadlines and real failed attempts, retry rollback, capped
upgrades, pointer bounds and terminal invariance. The public model's doctest
executes without a display. Strict Clippy and scoped formatting also pass.

The implementation uses the existing `ferravex-play::navigation::Grid` for
weighted paths and the parent-owned `ferravex-arcade` host for fixed-tick input,
menus, audio cues, records and rendering. Model storage is an 88-tile array,
three crews, six medal slots and bounded counters; temporary paths and planning
candidates are limited by that same board.

Parent native review at frame 350 found the tactical board and resource display
readable. Its requested sidebar shortening was applied and the rebuilt frame350
capture passed the parent's follow-up inspection. Useful additional native
capture windows are 180 or 350 for Pine Creek, 1300 for Ridge Road, 2000 for Ember
Ferry, 3100 for the finale and 3700 for the ending. Parent owns final browser/Wasm
integration and visual checks. Automated solutions establish rule completion,
not human difficulty balance, physical input/audio device behavior, mobile
support or cross-platform determinism.
