# Fossil Frontier

A complete six-permit excavation and museum management campaign built with
Ferravex. Survey buried strata, select tools, protect fragile specimens, identify
anatomical evidence and fund the next expedition. Assemble the fictional Dawn
Giant skeleton to open the basin museum. The field notebook, lab, camp, museum
and identification atlas are all playable pages of the same career.

## Run

From the repository root:

```sh
cargo run -p fossil-frontier --offline --locked -j 2
cargo run -p fossil-frontier --offline --locked -j 2 -- --headless
```

The native host provides a title menu, pause, paginated help, sound, control
remapping, whole-career restart and locally stored best records. The game
requires no external assets, services or dependencies. Browser staging and Wasm
integration belong to the collection's shared host.

## Campaign

Each authored dig is an 8 by 6 grid with four specimens. The six sites contain
24 specimens total: six Giant parts and eighteen other finds from five fossil
families. Fossil positions are authored; ordinary stone depths are deterministic.
There is no random seed or hidden deadline based on elapsed real time.

| Permit | Geology / hardness | Light per day | Signature part | Other finds |
| --- | --- | --- | --- | --- |
| Riverbend | River silt / 0 | 54 | Skull | Ammonite, fern, trilobite |
| Amber Hollow | Amber shale / 1 | 52 | Neck | Fern, tooth, ammonite |
| Salt Cliff | Limestone / 2 | 50 | Rib cage | Trilobite, ammonite, wing |
| Basalt Step | Basalt and ash / 3 | 48 | Pelvis | Tooth, trilobite, fern |
| Windglass | Sandstone / 2 | 46 | Legs | Wing, ammonite, tooth |
| Dawn Quarry | Granite and marl / 3 | 48 | Tail | Wing, trilobite, fern |

Fossil depths grow from two to nine layers. To finish a permit, correctly
identify its Giant part and at least one other specimen. Finish all six permits
to win. Optional specimens, condition and remaining daylight determine your
score and medals. You may leave two optional finds behind if your budget is
running short. Camp's final permit action opens the museum and ends the career.

## Controls

| Control | Action |
| --- | --- |
| 1, 2, 3, 4, 5 | Dig, Lab, Camp, Museum, Atlas |
| Arrows / WASD in Dig | Move the selected tile |
| Space / Enter in Dig | Use selected tool; jacket an exposed fossil |
| X / E in Dig | Survey this tile and four orthogonal neighbors |
| C / Q in Dig | Cycle Brush, Chisel, Drill |
| Up / Down in Lab | Select one of four tray positions |
| Left / Right in Lab | Select an identification |
| Space / Enter in Lab | Submit identification |
| Up / Down in Camp | Select an action |
| Space / Enter in Camp | Accept selected action |
| Left / Right in Museum | Highlight a permit's exhibit card |
| Arrows in Atlas | Highlight a species reference |
| Mouse | Select tabs, field tiles, tools, samples; cycle the Lab candidate; submit research; accept Camp cards |
| Space / click on permit briefing | Begin fieldwork |
| Space after a setback | Spend a recovery grant, or close the expedition when none remain |
| Esc / P, H, M, F2 | Shared pause, help, mute, control remapping |
| R | Restart the entire career |

The model consumes fresh input edges on actual fixed ticks. Holding a key does
not repeat purchases or digs. Moving the cursor, reading pages and planning are
free. Clicking a field tile only selects it; a separate Space action works it.

## Field rules

Start with 65 coins, 30 supply units and three recovery grants. Each permit
allows three working days and five supply purchases. X surveys an orthogonal
cross for one supply and two daylight; repeating a completely surveyed cross
costs nothing. Survey reveals depth and visible fossil silhouettes. All cells
show their geological hardness. Unsurveyed tiles can still be dug blindly.

| Tool | Layers per stroke | Supplies | Daylight | Effect at fossil contact |
| --- | --- | --- | --- | --- |
| Brush | 1 | 1 | 3 minus precision level | Always preserves condition |
| Chisel | max(1, 2 minus floor(hardness / 2)) | 2 | 2 | Damages fossil |
| Drill | max(1, 4 + 2 times motor level minus hardness) | 3 | 1 | More severe fossil damage |

A rough tool damages a fossil when its cut **reaches** the fossil, including an
exact-depth cut. Damage is the specimen's authored fragility, plus ten per layer
of overshoot, plus twenty for a drill, minus eight per precision level. Condition
starts at 100 and cannot fall below zero. Brush strokes never damage fossils.
Rough cuts that stop above a fossil do no damage.

At depth zero, another Space action jackets the fossil for one supply and two
daylight. Empty bedrock grants nothing; jacketed tiles cannot be harvested
again. A sample's condition determines its museum funding and research score.
Destroying the mandatory Giant part triggers a recoverable permit setback.

## Research and money

Lab displays two anatomical traits and a specimen silhouette. Compare these with
the six atlas entries, choose a species and submit it for one supply and two
daylight. Correct research grants funding once, awards three renown for condition
80 or better (otherwise one), and places the specimen in the museum. An incorrect
submission loses up to six available coins and one available renown; it does not
destroy the sample. Identified samples cannot be researched repeatedly for money.

| Identification | Evidence | Base grant |
| --- | --- | --- |
| Ammonite | Coiled chambers, ribbed shell | 35 |
| Fern frond | Paired leaflets, central stem | 30 |
| Trilobite | Three lobes, segmented armor | 45 |
| Predator tooth | Serrated edge, hollow root | 50 |
| Pterosaur wing | Hollow bone, elongated fourth finger | 55 |
| Dawn Giant | Spongy interior, huge supporting joint | 65 |

Actual funding is floor(base grant times condition / 100). Camp sells twelve
supply units for ten coins, with five packs per site and a storage cap of 120.
Rest costs twelve coins and restores that site's full daylight allowance. It
cannot be purchased on an untouched morning or after day three. An action that
cannot afford its supplies or daylight spends neither resource.

Precision levels cost 35 then 70 coins. Motor levels cost 40 then 75 coins. Each
track has two levels, requiring three renown for level one and six for level two;
renown is a requirement rather than a spent currency. Precision speeds up brushing
and reduces rough-tool damage. Motors deepen drilling cuts. Successful permit
completion retains improvements, adds 30 coins and supplies the next site with
sixteen additional supply units, capped at 120.

## Recovery, loss and scoring

Camp's Recover Permit action spends one of three career-wide grants. It restores
that permit's starting money, supplies, equipment, renown and score; clears its
specimens; and regenerates its original field. Previous completed permits,
medals and museum exhibits remain intact. Local purchases and earnings roll back
together, so recovery cannot duplicate rewards. Recovery is available voluntarily
and after resource insolvency or destruction of the signature specimen. With no
grants left, the next recovery request ends the career. R always starts a fresh
career immediately through the shared host.

A permit earns three medals for all four identified finds averaging at least 90
condition, two for at least three averaging at least 60, and one otherwise.
Research earns 12 times the awarded funding plus twice the specimen condition.
Permit completion adds 1,000 points, five per leftover daylight and 200 per
medal. The completed skeleton adds 3,000 plus twenty per renown. Recovering a
permit intentionally rolls back its score. The shared host preserves the best
finished-career record; it does not save an expedition mid-permit.

## Strategy

Survey before spending tools on empty rock. Use drills only while their cut will
stop above the fossil; change to a brush for the final layer. In hard stone a
chisel loses much of its advantage. Research a clean early find to finance
supplies and unlock precision. Keep at least twelve coins available for another
morning. Improving your brush early makes later deep sites much easier to finish.
Check the museum requirement before abandoning a permit; an intact Giant part
and one other identified specimen are sufficient. Optional finds support the
collection and score, but are not worth losing an entire permit.

## Implementation and verification

`content.rs` owns six authored sites and the species atlas. `model.rs` owns pure,
deterministic rules and fixed-capacity storage (48 field cells, four tray slots,
24 museum records, six medals). `view.rs` only reads model state, measures text
and draws original geology, fossil silhouettes and the six-part skeleton. The
shared Ferravex arcade host owns fixed ticks, input collection, audio cues,
persistence and renderer lifecycle.

```sh
cargo test -p fossil-frontier --offline --locked -j 2
cargo clippy -p fossil-frontier --all-targets --offline --locked -j 2 -- -D warnings
rustfmt --edition 2024 --check examples/fossil-frontier/src/content.rs examples/fossil-frontier/src/model.rs examples/fossil-frontier/src/view.rs examples/fossil-frontier/src/lib.rs examples/fossil-frontier/src/main.rs
```

Eighteen behavior tests cover survey coverage, tool costs and damage, safe
extraction, hardness and upgrades, atomic affordability, identification and
reward duplication, input edges and pointer bounds, camp limits, progression
requirements, recovery rollback, insolvency, terminal invariance and content
integrity. A fresh-game legal-input controller completes all six sites and its
recorded inputs produce identical state in a second fresh game. It earns all
24 perfect specimens and eighteen medals at tick 7,752 with 33,255 points and
938 coins. One public API rustdoc example executes without graphics. The
`--headless` mode independently runs the same ordinary input path.

These checks are model verification, not human usability or native/browser
visual playtesting. Native screenshots, interactive audio checks, browser/Wasm
verification and shared registration remain the parent integration owner's work.
The controller proves one all-collection solution; it does not guarantee
recovery from arbitrary player mistakes or solve every possible campaign state.
