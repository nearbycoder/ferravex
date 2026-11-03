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

Start with the charter shown in the ledger. Press **2** for Market, buy **three
grain** with Space, then **1** for Chart. Brinewatch is already selected: Space
sets sail. On arrival, **3** opens Charters and Space delivers. The next charter
appears immediately. Repeat the voyage around the six islands, then select
**Settle ship debt** at Haven's Shipyard.

| Control | Action |
| --- | --- |
| 1 / 2 / 3 / 4 / 5 | Chart / Market / Charters / Shipyard / Log |
| Arrows or WASD edges | Select destination, good, or shipyard service |
| Space / Enter | Sail, buy, deliver, refit, settle debt, or wait, according to the visible tab |
| X / E | Sell one selected good in Market; switch sheltered/open sea on Chart; open Market from Charters; return to Chart from Shipyard/Log |
| C / Q | Return to Chart while in port |
| Click | Select an island, tab or row; activate the painted action buttons |
| Up / Down held at sea | Steer across the passage to avoid storm rings |
| Space / Enter at sea | Reef or unfurl sails; the action button also works with the mouse |
| H | Help |
| P / Escape | Pause |
| R | Restart the entire voyage |
| M | Sound setting, handled by the shared host |

The native and browser builds use the shared Ferravex arcade lifecycle for
start, pause, help, full retry, accepted-event cues and best-score records.
Keyboard and pointer controls are authored here. Actual touch/gamepad device
coverage is not claimed.

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
ship and a parchment trading ledger; all art is drawn through Ferravex.

## Verification

```sh
cargo test -p saltwind --offline --locked -j 2
cargo clippy -p saltwind --all-targets --offline --locked -j 2 -- -D warnings
cargo run -p saltwind --offline --locked -j 2 -- --headless
```

The headless command runs the same model using ordinary player inputs. It buys
cargo, travels, resupplies, delivers all six charters and pays the debt. It grants
no special money, invulnerability, movement or objective completion.

The implementation pass ran 13 behavioral tests and one executable rustdoc
example. Coverage includes the complete legal voyage, trade accounting and
anti-farming, capacity rejection, delivery duplication/wrong ports, forecasted
food/day consumption, reefing, mouse hit regions, real storm damage/armor,
starvation, deadline loss, restart construction, terminal invariance and
same-host deterministic input replay. The headless pilot won at tick 4633, day
24, with 184 crowns after repaying the debt, 100% hull and score 4783.

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
