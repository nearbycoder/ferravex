# Saltwind

A merchant voyage across the Six Isles. Your mother's ship carries a 900-crown
lien. Complete six delivery charters, trade your way around the islands, and
come home with enough money to make Saltwind yours before day 43.

This is a finite single-player campaign with six named ports, six authored
story charters, three trade goods, limited cargo and market stock, six weekly
market/wind forecasts, two passage types, food and hull management, three ship
upgrades, and three captain ranks. Planning at port is turn based; sailing is a
fixed-tick journey with steering and reefing. A conservative complete voyage
uses about 24 game days; optional trades, upgrades and faster routes change its
profit, schedule and final score.

## Play

```sh
cargo run -p saltwind --offline --locked -j 2
```

Start with the charter shown in the header: a grain sack with `0/3`, an arrow,
Brinewatch's pin and the crowns it pays. A pulsing dot marks the ledger tab that
holds the next step. Press **2** for Market, buy **three grain** with Space,
then **1** for Chart. Brinewatch is already selected: Space sets sail. On
arrival, **3** opens Charters and Space delivers. The next charter appears
immediately. Repeat the voyage around the six islands, then select **Settle
debt** at Haven's Shipyard.

| Control | Action |
| --- | --- |
| 1 / 2 / 3 / 4 / 5 | Chart / Market / Charters / Shipyard / Log |
| Arrows or WASD edges | Select destination, good, or shipyard service |
| Space / Enter | Sail, buy, deliver, refit, settle debt, or wait, according to the visible tab |
| X / E | Sell one selected good in Market; switch sheltered/open sea on Chart; open Market from Charters; return to Chart from Shipyard/Log |
| C / Q | Return to Chart while in port |
| Click | Select an island, tab or row; activate the painted action buttons; pick the other side of the Chart's route switch (the side already chosen does nothing) |
| Up / Down held at sea | Steer across the passage to avoid storm rings |
| Space / Enter at sea | Reef or unfurl sails; the action button and the other sail-plan tile also work with the mouse |
| H | Help |
| P / Escape | Pause |
| R | Restart the entire voyage |
| M | Sound setting, handled by the shared host |

The native and browser builds use the shared Ferravex arcade lifecycle for
start, pause, help, full retry, accepted-event cues and best-score records.
Keyboard and pointer controls are authored here. Actual touch/gamepad device
coverage is not claimed.

## Reading the screen

Saltwind explains itself with icons, meters, pips and previews rather than
sentences; words are kept to names, numbers and one-word verbs beside key caps.

- **Header:** seven pips (six charters, then the homecoming) and the current
  charter's name. The objective row shows the charter's good and crates aboard,
  its destination and pay (or, after the sixth seal, crowns against the
  900-crown lien), then this week's forecast icon, the season meter split into
  six weeks, and the days left.
- **Ship band (top of the chart):** purse, hull meter, one pantry pip per day of
  food and one hold slot per crate, colored by good (gold grain sack, blue cloth
  bolt, red spice jar). While a course is chosen or sailed, the pips it will eat
  blink, and missing days show as red rings.
- **Chart:** the chosen course is a marching dashed line with a chip for its
  days and a shield (sheltered) or squall cloud (storms on the open course).
  Hovering an island previews the same for it. Storm rings show a squall cloud;
  they flash red when the ship is caught on open sea.
- **Ledger tabs:** map, bag, scroll, wrench and book icons with their key
  numbers. Market rows show crates held and in stock, the coin cost to buy and
  the coins a sale returns, an arrow when the price is above or below the port's
  usual rate, a star for goods this week's news lifts, and how many crates the
  charter still needs. Buttons preview their coin, day or food change and dim
  when the action would be refused. The Log is a six-week calendar.
- **At sea:** a day-by-day passage track, the two sail plans with their speed
  and per-squall hull damage, and a dial for the next sunrise with the food it
  eats and the sea's danger.
- **Feedback:** coin popups rise from market rows, crates burst into hold
  slots, deliveries burst gold at the port with the reward and a seal, squalls
  spray, shake and pop the hull loss at the ship, a new week shows a forecast
  toast, and refused actions shake with a red ring and the missing resource.

## Rules and strategy

- **Cargo:** eight crates initially, twelve with a cargo expansion. Grain, cloth
  and spice occupy one slot per crate. The model reuses Ferravex's bounded
  `Inventory<3>` for atomic addition/removal.
- **Markets:** each port has its own specialty prices. Scarcity adjusts prices
  as stock moves, and weekly news changes demand. Buy/sell prices have a spread;
  repeatedly buying and reselling in one port loses money and never gives
  points. Weekly caravans add two crates per good up to twelve. Warehouses will
  not buy beyond sixteen crates.
- **Charters:** deliver the specified quantity at the specified port. Delivery
  consumes those goods and pays once. The next charter's goods are reasonably
  priced at your current port; the charter reward includes your trading margin.
- **Sailing:** distance, weekly wind and canvas upgrades set the forecast. A
  sheltered route adds one day and avoids squall damage. Open sea is quicker,
  but storm rings and periodic rough weather can damage your ship.
- **Food:** a sea day uses one provision. An empty pantry costs 22 hull each day.
  Buy up to six days of food at a time, limited by the twelve-day pantry and
  your money. A day is 180 simulation ticks, or three seconds at the nominal
  60 Hz rate. Port planning does not use calendar time. Waiting in Log does.
- **Reefing:** reefed sails advance at 70% speed and limit a direct storm hit to
  three hull instead of eleven. The extra time still consumes food; even a
  final partial sailing day counts. Do not reef on a safe passage without
  checking your supplies.
- **Upgrades:** swift canvas costs 85 crowns and removes one passage day,
  minimum two. Cargo expansion costs 70. Brass plating costs 70 and prevents
  three hull damage per storm event. Each can be bought once.
- **Repair:** up to thirty hull per visit to the repair service, at one crown
  per two hull, rounded up. Healthy hull and a full pantry reject extra purchases.
- **Ending:** the banker requires all six sealed charters and 900 crowns at
  Haven. Hull reaching zero, the season expiring, or total destitution loses the
  voyage. R creates a fresh campaign with all markets, cargo and resources reset.
- **Score:** first visits and accepted charters award points. The final score
  includes remaining money, hull and days. Captain ranks reward a profitable,
  early, well-preserved return. Extra trading and route/refit choices give
  replay goals without allowing repeated invalid actions to farm points.

The six ports are Haven, Brinewatch, Glass Quay, Cedar Reach, Saffron Key and
Gullspire. Their charter stories progress from bread for lighthouse keepers to
the homeward cargo. The visual chart contains authored island silhouettes,
houses, piers, lighthouses, storm rings, a wind compass, passage marks, a sailing
ship and a parchment trading ledger; all art is drawn through Ferravex, with the
engine's shared pictograms for every resource and action.

## Verification

```sh
cargo test -p saltwind --offline --locked -j 2
cargo clippy -p saltwind --all-targets --offline --locked -j 2 -- -D warnings
cargo run -p saltwind --offline --locked -j 2 -- --headless
```

The headless command runs the same model using ordinary player inputs. It buys
cargo, travels, resupplies, delivers all six charters and pays the debt. It grants
no special money, invulnerability, movement or objective completion.

The suite has 22 behavioral tests and one executable rustdoc example. Coverage
includes the complete legal voyage, trade accounting and anti-farming, capacity
rejection, delivery duplication/wrong ports, forecasted food/day consumption,
reefing, mouse hit regions (including the route switch's sides and the
sail-plan tiles), real storm damage/armor, starvation, deadline loss, restart
construction, terminal invariance, same-host deterministic input replay, header
progress at the start and end of the campaign, and the presentation effects for
purchases, refusals, deliveries and squalls. The headless pilot won at tick
4633, day 24, with 184 crowns after repaying the debt, 100% hull and score 4783.

For the integration owner's native visual smoke:

```sh
cargo run -p saltwind --offline --locked -j 2 -- --autoplay --smoke-frames 240 --screenshot target/saltwind-sea.png
```

The default pilot begins sailing after its initial grain purchase; around tick
240 is a good sea view. Port shopping lasts only a few dozen ticks because the
pilot issues an action every eight ticks; interactive input is better for
inspecting all ledger tabs. The complete run ends around tick 4633, so later
captures show the result overlay.

Compilation, unit tests and headless completion are not a visual or audio
playtest. Native/browser rendering, sound output, resizing, actual device
input, and browser persistence are left to parent integration checks. There is
no networking and no cross-platform bitwise-determinism claim.
