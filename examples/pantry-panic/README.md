# Pantry Panic

A neighborhood restaurant grows from its first lunch to a lantern banquet. Move
one cook through a compact kitchen, prepare ingredients, juggle two burners and
feed guests before their patience runs out. Seven authored services contain 42
orders and six recipes; the final service brings all four ingredients together.

```sh
cargo run -p pantry-panic --offline --locked -j 2
```

The parent-owned browser catalog also registers `pantry-panic`. This game uses
Ferravex's first-party renderer and shared [arcade host](../shared/arcade/README.md)
for fixed ticks, input, sound cues, menus, field guide, results and local records.

## Open the doors

Enter starts the shared host, then Space opens the first service. The in-game
briefing shows the day's menu and quota. Every service allows one missed guest;
a second missed guest fails that service. Complete all seven services to finish
the restaurant's story.

| Service | Guests / quota | New challenge |
| --- | --- | --- |
| The First Lunch | 4 / 3 | Fresh salads and the kitchen route |
| Rain on the Windows | 5 / 4 | Hot soup and burner timing |
| Market Day | 5 / 4 | Tomato rice and mixed orders |
| The Garden Club | 6 / 5 | Herb pilaf and exact ingredients |
| After the Theater | 7 / 6 | Slow risotto and a busier queue |
| The Critic's Table | 7 / 6 | All five regular dishes, arriving every 17 seconds |
| The Lantern Banquet | 8 / 7 | Six recipes, including the four-ingredient House Feast |

Arrivals follow a fixed menu and schedule. Three tickets can be visible at once;
if the window is full, the next scheduled arrival waits for a free ticket. Each
guest's patience begins when that ticket actually appears. Tickets show dish,
base payment and remaining patience; the line below previews the next arrival.
The kitchen also has an overall closing deadline.

## Controls

| Control | Action |
| --- | --- |
| Arrows / WASD | Move the cook |
| Click a station or floor tile | Walk to that station's marked access tile or the chosen floor tile |
| Space / Enter | Use the adjacent station; open or continue a service |
| Hold Space / Enter at CHOP | Prepare the ingredient in your hands |
| X / E | Discard held food; with empty hands at MIX, clear its bowl; beside a burned pot, clean the burner |
| C / Q | Cycle the six recipe cards |
| Click an order ticket | Show its recipe |
| 1 / 2 / 3 between services | Buy knife / shoes / burner upgrade |
| H | Open the paginated field guide, which pauses the game |
| Esc / P | Pause |
| R | Start a whole new restaurant career |
| M | Toggle sound |
| F2 | Shared control editor |

The marked floor squares identify each station's interaction position. Finish
moving onto the square before tapping Space. Clicking a station moves the cook;
it does not perform an interaction or bypass preparation time.

## From pantry to table

1. **Collect:** stand below a pantry and press Space with empty hands. Each
   ingredient costs two coins. Holding food prevents another purchase.
2. **Prepare:** take it to CHOP and hold Space. Vegetables are chopped; rice is
   rinsed and portioned at the same bench. Initial preparation takes 1.5 seconds.
   Moving away discards partial preparation progress.
3. **Mix:** press Space at MIX to add one prepared ingredient. Repeat for every
   ingredient in the recipe. Raw and duplicate ingredients are rejected.
4. **Lift:** with empty hands, press Space at MIX to lift an exact, complete
   recipe. A bowl with an invalid combination stays on the counter until corrected
   by adding ingredients or cleared with X.
5. **Cook:** put a mixed hot recipe on either empty burner. A green timer and
   sound cue announce readiness. Return with empty hands and press Space to take
   it. Salad skips this step.
6. **Plate and serve:** press Space at PLATE, then at SERVE. A matching waiting
   guest pays once, and your hands become empty. The oldest matching ticket is
   served first. Wrong dishes remain in your hands and award nothing.

| Recipe | Prepared ingredients | Cooking | Base payment |
| --- | --- | --- | --- |
| Garden Salad | Tomato + greens | None | 18 coins |
| Forest Soup | Tomato + mushroom | 3 seconds | 22 coins |
| Tomato Rice | Tomato + rice | 4 seconds | 24 coins |
| Herb Pilaf | Greens + rice | 3.5 seconds | 24 coins |
| Fungi Risotto | Mushroom + rice | 5 seconds | 26 coins |
| House Feast | All four | 6 seconds | 36 coins |

Food initially burns five seconds after becoming ready. A burned pot cannot be
served; stand beside it with empty hands and press X to clear it. Burning counts
as waste once. Discards and mistaken mixing bowls never refund ingredient costs.
The two burners run independently, so preparing the next bowl while one cooks
can help during busy services.

## Money, upgrades and retries

Begin with 48 coins. Each served dish earns its base payment plus a patience tip;
faster service pays more. A completed service adds 20 coins. Buy equipment only
between services, retaining enough cash to prepare the next dish.

| Upgrade | First / second price | Effect |
| --- | --- | --- |
| Sharp Knife | 45 / 75 coins | Preparation falls to 1.0, then 0.7 seconds |
| Quick Shoes | 50 / 80 coins | A floor step takes 8, then 6 ticks instead of 10 |
| Safe Burners | 35 / 65 coins | Burn grace rises to 8, then 11 seconds |

Every upgrade has exactly two levels. Invalid, unaffordable or maxed purchases
change no money or equipment. Three service retries are shared across the career.
A retry restores money, score and dishes served to the start of the failed shift,
while retaining completed earlier services and equipment. Failed-service earnings
cannot be farmed. After the fourth failed service, the career ends.

Serving every guest with no waste earns three stars for a service. Serving every
guest with waste earns two; meeting quota earns one. Service stars and dish tips
contribute to the score. The ending awards Neighborhood Legend, Beloved Bistro or
Open for Business based on total stars. Replay to improve route efficiency,
reduce waste and finish with more money.

A sharper knife is a useful early investment. Prepare ingredients in pantry order
to avoid forgetting one, inspect the ticket before lifting a bowl, and clear a
burner before carrying another dish to it. The recipe panel stays on the recipe
you chose; changing it never alters food already in the kitchen.

## Implementation and checks

`content.rs` contains the seven menus, six recipes and station locations.
`model.rs` is an integer-tick, renderer-independent state machine with fixed arrays
for three orders and two pots, a four-bit mixing bowl and bounded campaign
counters. `view.rs` draws the kitchen, cook, food, tickets, recipe book and service
menus with procedural shapes and measured text. No external assets or libraries
are used.

```sh
cargo test -p pantry-panic --offline --locked -j 2
cargo clippy -p pantry-panic --all-targets --offline --locked -j 2 -- -D warnings
cargo run -p pantry-panic --offline --locked -j 2 -- --headless
```

The implementation check passed 16 behavioral tests and one executed API doctest,
strict package Clippy and scoped formatting. Tests cover paid collection, chopping,
recipe validation, both burners, cooking and burning, plating and serving,
patience, quotas, retries, upgrade caps, pointer positions, terminal stability and
an exact replay of a complete legal campaign. The full playthrough begins with
`Game::new()` and changes state only through ordinary `Input` passed to `step`.

The legal pilot completes all seven services at tick 46,003: 42 dishes, 21 stars,
37,650 score, 974 coins, all upgrades and no spent retries. This is about 12 minutes
47 seconds of simulated play, not a measured human completion time. The pilot
uses the same rules and paid ingredients as a player, and is intentionally simple:
it works one dish at a time rather than optimizing both burners.

The parent performs native/browser integration and graphical captures separately.
Automated rules checks do not establish human difficulty balance, physical mobile
or gamepad support, audible-device quality or behavior on other operating systems.
Mid-career saves are not implemented; the shared host retains completed records.
