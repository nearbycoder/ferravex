# Hearthharvest

Twelve plots, fourteen mornings, and a village to feed. Grow three kinds of
produce, manage a small seed budget, improve your tools, and bring everyone to
the Lantern Feast. This is a complete, finite farming campaign with a festival
ending, three success ranks, two explicit failure conditions, and local best
scores through the shared Ferravex host.

## Win the season

Deliver all three village orders and keep **80 coins** for the village oven.
Sleep on day fourteen to begin the festival. Each order must be delivered
**before sleeping on its deadline day**:

| Village order | Opens | Deadline | Ingredients | Reward |
| --- | --- | --- | --- | --- |
| Soup Night | Day 1 | Day 5 | 4 turnips | 45 coins |
| Founders' Table | Day 6 | Day 9 | 2 turnips, 3 beans | 80 coins |
| Lantern Feast | Day 10 | Day 14 | 2 turnips, 2 beans, 3 pumpkins | 120 coins |

Missing a table's deadline ends the season. Completing the orders without the
80-coin oven reserve also loses at the festival. The final screen explains the
result and offers a new campaign; R also restarts the complete season.

## Controls

| Input | Action |
| --- | --- |
| Arrows / WASD | Select a field plot or menu row; use presses, not holds |
| Space / Enter | Work the selected plot, buy a seed/tool, or deliver an order |
| X / E | Cycle selected seed on Field; sell one harvested crop at Stall |
| X / E on Tools | Compost the selected **empty** field plot |
| C / Q | Open the sleep confirmation; Space confirms, X/C cancels |
| 1 / 2 / 3 / 4 / 5 | Field / Seed Stall / Village / Tools / Almanac |
| Pointer | Select tabs, plots, seed types and menu rows; click the ledger action button |
| H | Full in-game field guide; Left/Right turn its pages |
| Esc / P | Pause / resume |
| R | Start a fresh fourteen-day campaign |
| F2 | Inspect or remap controls |
| M | Toggle sound |

Clicking a plot selects it; the ledger's action button performs the current
Space action. The keyboard remains necessary for selling and composting. On a
sleep card, both confirm and cancel have pointer buttons.

## Learn the field

You start with 35 coins, five turnip seeds, three bean seeds, three pumpkin seeds,
18 daily energy and twelve plots of rich soil. Planting spends two energy;
watering, harvesting and clearing a wilted crop each spend one. Space chooses
the valid action for the selected plot. Ready crops stay safe in the ground
until you can harvest them.

| Crop | Watered growing nights | Seed price | Sale price |
| --- | --- | --- | --- |
| Turnip | 2 | 4 coins | 10 coins |
| Bean | 3 | 6 coins | 18 coins |
| Pumpkin | 4 | 9 coins | 30 coins |

A crop grows only when you confirm sleep after watering it. Merely waiting,
rendering more frames, holding Space, or opening menus cannot grow or harvest
anything. Rain waters existing and newly planted crops for free. The blue dot
marks watered soil; `age / required nights` shows growing progress. READY
marks a crop you can harvest. A `!1` or `!2` mark warns of dry stress.

A dry night adds one stress; heat adds two. Three stress wilts a plant, losing
the seed and crop. Clear it with Space before replanting. On the day-ten frost,
watered crops remain safe but do not grow without Cold Frames. The Almanac
shows the complete authored forecast and a new village letter every morning.
There are no hidden weather rolls.

Each harvest uses one soil level, shown as S2, S1 or S0. Planting on S0 adds one
growing night. Select an empty field plot, switch to Tools, and press X to
restore rich soil for two coins and one energy. Composting already rich or
occupied soil is rejected without spending resources.

## Markets and tools

The seed stall restocks four turnip, three bean and two pumpkin seeds each
morning. A seed tin holds at most 24 of each kind. The produce basket holds at
most 99 of each crop. Seeds cannot be resold as harvested produce. Village
orders preflight every ingredient before taking any crop from the basket.

| Tool | Cost | Effect |
| --- | --- | --- |
| Brass Can | 45 coins | Water the selected plot and its right/lower neighbors for one energy |
| Work Boots | 55 coins | Add six daily energy, including the purchase day |
| Cold Frames | 70 coins | Let watered crops grow through frost |

Tools are permanent for the current season and cannot be bought twice. The
can does not wrap from the right edge of one row into the next row.

Start by planting and watering four turnips for Soup Night. Use the rainy
second morning to sow beans or pumpkins. Save all future order ingredients
before selling the surplus. Buy the Brass Can after your first delivery to
make a larger field manageable. Late pumpkins need four nights, so plant them
well before the final week ends.

Every accepted harvest scores 20 points plus five per soil level at planting;
each delivered order adds 400. Festival scoring rewards remaining coins, tool
ownership and avoiding wilt. A successful season earns Village Gardener;
Golden Harvest needs at least 24 crops and 160 coins; Hearthkeeper needs all
three tools, at least 120 coins, and no wilted crops. These give a reason to
replay beyond merely completing the tables.

## Run

From the repository root:

```sh
cargo run -p hearthharvest --offline --locked -j 2
cargo run -p hearthharvest --offline --locked -j 2 -- --headless
cargo test -p hearthharvest --offline --locked -j 2
cargo clippy -p hearthharvest --all-targets --offline --locked -j 2 -- -D warnings
```

The normal command needs a supported native display. `--headless` runs the
same legal-input campaign without opening a graphics window. The browser host
is registered by the integration parent at `/hearthharvest/` in the local
catalog after the shared web build.

The shared native smoke interface supports:

```sh
cargo run -p hearthharvest --offline --locked -j 2 -- --autoplay --smoke-frames 600 --screenshot target/hearthharvest.png
```

Autoplay chooses ordinary page, pointer and action commands; it cannot add
resources or skip order windows. It uses transient personal records. Suggested
capture frames are 600, 1600 and 3000, before the full demonstration ends at
tick 4270. Best scores and completed runs are stored locally by the shared
host; unfinished seasons are not saved.

## Implementation and verification

`content.rs` authors the three crops, three orders, three tools and fourteen
weather/letter entries. `model.rs` owns all rules and bounded storage, reusing
Ferravex's fixed-capacity `Inventory<3>` for produce. `view.rs` supplies the
farm art, five management pages and the `ArcadeGame` adapter. The shared host
owns tick timing, input-edge buffering, audio, menus, records and browser status.

Thirteen behavioral tests and one executable rustdoc example pass. They cover
an entirely legal fourteen-day campaign win, action costs and invalid repeats,
confirmation/cancel, rain, heat/wilting, frost, soil/harvest accounting,
transactional orders, finite market stock, one-time upgrades and watering
geometry, deadlines/oven failure, terminal invariance, clean restart, held
input and invalid pointers. Strict Clippy and owned-file formatting pass.
The headless campaign completes all three orders at day fourteen with 218
coins and score 3597.

These are model/build results, not a human playtest or physical-device
certification. The parent reviewed native autoplay frame 600 and reported readable field,
ledger, HUD and controls without overlap. Browser review and combined release
staging remain separate. Audio listening, gamepad hardware, touch-only play and
cross-platform runtime behavior have not been verified by this assignment.
