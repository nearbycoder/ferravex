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
80-coin oven reserve also loses at the festival. The final screen names the
rank and shows tables served, crops harvested, coins and tools (or the missed
deadline or the cold oven) as icon tiles, and offers a new campaign; R also
restarts the complete season.

## Controls

| Input | Action |
| --- | --- |
| Arrows / WASD | Select a field plot or menu row; use presses, not holds |
| Space / Enter | Work the selected plot, buy a seed/tool, or deliver an order |
| X / E | Cycle the seed packet on Field; sell one harvested crop at Stall |
| X / E on Tools | Compost the selected **empty** field plot |
| C / Q | Open the sleep confirmation; Space confirms, X/C cancels |
| 1 / 2 / 3 / 4 / 5 | Field / Seed Stall / Village / Tools / Almanac |
| Pointer | Select tabs, plots, seed packets and rows; click a selected plot again to work it; press any Buy, Sell, Deliver or Compost button; the ledger's Space and moon buttons act and sleep |
| H | Key caps and a four-line field guide; Left/Right turn its pages |
| Esc / P | Pause / resume |
| R | Start a fresh fourteen-day campaign |
| F2 | Inspect or remap controls |
| M | Toggle sound |

Clicking a plot selects it and a second click works it, exactly like Space.
Every row button routes through the same rule as its key: Buy is Space, Sell
and Compost are X, Deliver is Space on the Village page, and the ledger's moon
is C. On the sleep card, both confirm and cancel have pointer buttons.

## Reading the screen

The screen explains itself with icons, meters and motion rather than sentences:

- **Header:** fourteen pips count the mornings; the chip names the current
  village table (or the festival). Below them the order is drawn as crops with
  one pip per ingredient, filled from your basket, then an hourglass with the
  days left (a lock and its opening day before the window), the coin reward,
  and a pulsing `3 Deliver` key cap once it can be served.
- **Field:** a solid blue drop is watered soil; a pale drop in a breathing ring
  asks for water, turning red with stress dots after dry nights. Moons count a
  crop's growing nights (lit ones have passed), a green check marks a ripe,
  glowing crop and a red cross a wilted one. Two gold dots show soil richness;
  spent soil is pale and cracked. A pill over the selected (or hovered) plot
  previews Space: the action and its energy cost, the missing thing in red, or
  a moon when it only needs a night. With the Brass Can, the neighbors one
  watering will reach are outlined. Seed packets show nights to grow and seeds
  in the tin.
- **Ledger:** coins with a flame ring that fills toward the oven's 80; energy as
  one segment per point, blinking the cost of the next action; the basket with
  order pips; today's and tomorrow's weather with their effect (rain waters,
  heat doubles thirst, frost pauses growth without Cold Frames); and tonight's
  forecast of crops that will grow, ripen, freeze, stay dry or wilt. The moon
  button glows once no plot accepts more work.
- **Pages:** the stall shows nights, today's stock and your seed tin, with a
  green count of spare harvest on Sell or an amber lock when every crop is
  promised to an order. Village rows draw each table's window on a fourteen-day
  strip; hovering a row shows its letter. Tools draw what they do, and the
  compost card maps the field with the selected plot outlined. The almanac is a
  fourteen-day forecast colored by order window; hovering today or an earlier
  day shows that morning's letter.
- **Feedback:** accepted actions play where they happen (soil bursts, water
  rings, harvest sparkles, coin popups at the stall, confetti and a callout
  for a served table). Each morning brings a day callout and a toast with the
  first line of the new letter. Refused actions shake gently and ring the thing
  that refused, with its icon and a word ("Tired", "Short", "Sold out").

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
anything. Rain waters existing and newly planted crops for free. A blue drop marks
watered soil, moons show growing progress, a green check marks a crop you can
harvest and red stress dots warn of dry nights.

A dry night adds one stress; heat adds two. Three stress wilts a plant, losing
the seed and crop. Clear it with Space before replanting. On the day-ten frost,
watered crops remain safe but do not grow without Cold Frames. The Almanac
shows the complete authored forecast and a new village letter every morning.
There are no hidden weather rolls.

Each harvest uses one soil level, shown as two, one or no gold dots. Planting on
spent soil adds one growing night (an extra moon). Select an empty field plot, switch to Tools, and press X to
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
Ferravex's fixed-capacity `Inventory<3>` for produce. Its read-only checks
(`field_check`, `seed_check`, `deliver_check`, `tool_check`, `compost_check`
and the `night` forecast) decide every action and also drive the on-screen
previews, so a preview can never disagree with the rule. Each playing tick
clears a bounded list of presentation events (what was accepted or refused,
and where); the rules never read it. `view.rs` supplies the farm art, five
management pages, the effects for those events and the `ArcadeGame` adapter
(`progress`, `effects`, `draw_objective` and `draw_result`). Pointer targets
are shared geometry in `model.rs`, so drawing and hit-testing cannot drift.
The shared host owns tick timing, input-edge buffering, audio, menus, records
and browser status.

Twenty-one behavioral tests and one executable rustdoc example pass. They cover
an entirely legal fourteen-day campaign win, action costs and invalid repeats,
confirmation/cancel, rain, heat/wilting, frost, soil/harvest accounting,
transactional orders, finite market stock, one-time upgrades and watering
geometry, deadlines/oven failure, terminal invariance, clean restart, held
input and invalid pointers; plus the pointer paths (a second click works a
plot, row buttons buy/sell/compost/deliver, the moon opens sleep), the night
forecast against the real sleep, bounded per-tick events that never steer the
rules, header progress from day one to the festival, effects for harvests,
deliveries, mornings and refusals, and short header and summary copy. Strict
Clippy and owned-file formatting pass. The headless campaign completes all
three orders at day fourteen with 218 coins and score 3597.

These are model/build results, not a human playtest or physical-device
certification. The parent reviewed native autoplay frame 600 and reported readable field,
ledger, HUD and controls without overlap. Browser review and combined release
staging remain separate. Audio listening, gamepad hardware, touch-only play and
cross-platform runtime behavior have not been verified by this assignment.
