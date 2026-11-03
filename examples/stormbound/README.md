# Stormbound

An eight-island naval artillery campaign built with Ferravex. Captain the Free
Flag through a blockade, aim real cannonball trajectories, choose ammunition,
keep a damaged ship afloat, and spend liberated-harbor salvage on the next
crossing. The eighth victory ends the campaign with your captain rank.

## Play

```sh
cargo run -p stormbound --offline --locked -j 2
```

The shared arcade host supplies the title screen, paginated H guide, pause,
restart, controls editor, sound controls and local best-score record. The game
itself draws a side-on sea chart, ships with visibly broken sections, cannon
trails, wind, forecast, aiming history, damage feedback and a repair dock.

| Situation | Controls |
| --- | --- |
| Briefing | Space / Enter raises the flag |
| Aim | Left / Right changes angle; Up / Down changes powder |
| Fire | Space / Enter fires one selected round |
| Ammunition | 1 roundshot, 2 chainshot, 3 explosive shell |
| Navigator | X / E toggles the firing table; it never changes your settings |
| Retreat | C / Q during aiming enters a recoverable setback |
| Setback | C / Q spends a lifeboat to retry this island; Space retires |
| Dock | 1 repair, 2 guns, 3 plating, 4 magazine; Space sails |
| Host | H guide, Esc / P pause, R fresh campaign, M sound, F2 controls |

Held aim keys adjust at 20 units per second; an input edge adjusts one unit.
Angle is 15–70 degrees and powder is 25–90. The first briefing starts at a tested
40 degrees / 29 powder setting that physically contacts the opening enemy ship.

## The eight battles

| Island | Captain / ship | Tactical difference |
| --- | --- | --- |
| Lantern Reef | Midshipman Finch / The Little Gull | Calm tutorial; loose powder settings |
| Copper Shoals | Captain Flint / Copper Jack | East wind and more accurate low shots |
| Widow's Pass | Sable Vane / The Rigging Knife | Chainshot targets the mast |
| Iron Cove | Commodore Brack / The Iron Bell | Ten-point armor and high arcs |
| Skyglass Strait | The Long Watch / Far Horizon | Long crossing, headwind and precise plunging shots |
| Ashen Keys | Powder Rose / The Cinder Rose | Explosive shells; wind reverses after every exchange |
| Twinlight Sound | The Bell Brothers / Double Trouble | Two separately simulated cannonballs every third exchange |
| Storm Crown | Admiral Nocturne / The Black Meridian | Armored final hull; shells target your weakest intact section |

Each island has an authored briefing, tactical advice, range, wind, ship color,
armor and captain policy. Enemy hull integrity rises from 150 to 339; mast
integrity rises from 40 to 47. The Free Flag begins with 195 hull and 50 mast.

## Cannon and damage rules

Each ship has three independent hull sections and one mast. Destroy all three
hull sections to win a battle. A destroyed section becomes a hole: a later
cannonball passes through it. The mast and the hull rectangles are solid;
decorative sail cloth and distant islands are not additional collision targets.

Both captains use the same fixed 60 Hz ballistic integration. Powder controls
launch speed, gravity pulls downward, and wind accelerates the ball horizontally
throughout its flight. First-party swept sphere-versus-box queries find the
first actual section contact, including a shot returning to its own ship. A
miss never causes damage. Dotted prediction and live flight use the same
integration and contact path; the faint gold trace retains the last player shot.
The navigator searches reachable integer angle/powder controls without injecting
hits or moving your cannon.

The ship starts each island with **8 rounds, 2 chains and 2 shells**. Magazine
upgrades add up to two shells. Every accepted shot spends exactly one item.
Empty selected ammunition cannot fire. Running out of all ammunition without
winning creates a setback.

| Ammunition | Unupgraded player damage | Use |
| --- | --- | --- |
| Roundshot | 30 hull, 18 mast; armor reduces hull damage | Reliable hull damage after specials run low |
| Chainshot | 52 mast, 10 hull | Drop the enemy mast to disrupt return volleys |
| Shell | 42 hull or 24 mast; ignores armor; 14 splash to other hull sections within 57 world units | Break armor and clustered intact sections |

Each gun level adds 8 direct damage and 4 shell splash damage. Any hull impact
also strips three points of that section's armor. Enemy direct round damage is
19 plus the zero-based island index; enemy shells add 12. Enemy magazines also
have finite supplies, and their tactics change actual aim/powder/ammunition.
There are no damage rolls or automatic enemy hits.

A fallen enemy mast cancels odd-numbered return volleys and worsens its future
powder settings. This can counter the brothers' third-exchange broadside. A
fallen player mast limits powder to 76. Hull and mast damage carry between
islands; there is no automatic free health restoration.

## Docks, failure and rank

A liberated island awards 85 salvage plus eight per zero-based island index.
The ship starts with 40 salvage. Spend deliberately before sailing:

| Purchase | Cost | Effect |
| --- | --- | --- |
| Patch | 30 | Repair up to 80 hull, weakest sections first, plus 25 mast |
| Long guns | 65, then 95 | Two permanent levels, +8 direct damage each |
| Plating | 55, then 80 | Two levels; +5 armor per hull section per level |
| Magazine | 45, then 65 | Two levels; +1 shell at each island per level |

Sailing refills ammunition and purchased armor. Repairs and upgrades are only
available at the seven between-island docks. Purchases that are full, capped,
invalid or unaffordable spend nothing.

Sinking, an empty magazine or a voluntary retreat enters a setback. The two
lifeboats restore the current island's approach checkpoint, including the ship,
ammunition and salvage. A retry spends a lifeboat and awards nothing. Failed
shots remain in campaign accuracy. Retiring, or trying to retry with no
lifeboats, ends the voyage. R starts a completely new campaign.

Each victory awards 800 points, twice your remaining hull, and 15 for each
unused ammunition item. No score is awarded for individual hits, invalid input,
shopping or retreating. Finish all eight islands with both lifeboats and at
least 75% accuracy to earn **Storm Admiral**; other full victories earn **Island
Liberator**. Incomplete voyages earn Fleet Captain or Free Sailor by progress.

## Strategy

Use the preview to understand the relationship between powder and range before
spending ammunition. Recheck it when the wind changes. High arcs amplify wind's
influence because the projectile stays airborne longer. X can expose a firing
table when you need help; it gives a setting, not a guaranteed abstract hit.

Chainshot is particularly valuable before a long exchange: removing a mast can
save several enemy returns. Shells work best while adjacent hull sections remain
intact. Once a section is gone, aim at a surviving one instead of firing through
the same hole. Preserve enough salvage to repair before buying another upgrade.
The final captain deliberately attacks weakened sections.

## Model API and verification

`stormbound::Game` exposes `new`, `step`, `phase`, `battle_number`, `ammunition`,
`hull`, `score`, `outcome`, `summary`, and the read-only legal-control `pilot`.
`Phase` identifies briefings, aiming, flight, enemy preparation, docks,
recoverable setbacks and both endings. `ArcadeGame` delegates to these pure
rules; drawing never advances the simulation. The executable Rust API example
is in `src/model.rs` and runs as a doctest.

```sh
cargo test -p stormbound --offline --locked -j 2
cargo clippy -p stormbound --all-targets --offline --locked -j 2 -- -D warnings
rustfmt --edition 2024 --check examples/stormbound/src/*.rs
cargo run -p stormbound --offline --locked -j 2 -- --headless
```

The implementation's validation passed 12 behavioral tests and one executed
doctest. Tests cover wind/gravity, swept contact and destroyed-section holes,
armor and ammunition effects, prediction/live collision agreement, accepted
fire edges, invalid input, paid/capped upgrades and repairs, bounded retries,
empty-magazine and sinking setbacks, mast consequences, gust reversals, two
physical broadside flights, and immutable terminal states. The full-campaign
test starts at `Game::new`, supplies only ordinary `Input`, takes actual enemy
damage, uses every ammunition type and wins all eight islands.

The headless executable completed in **7,096 ticks**, with **9,882 points**,
**38 hits from 38 shots**, **two lifeboats** and **Storm Admiral** rank. This is
a deterministic automation result on the tested native host, not a human
balance or graphics playtest. The navigator computes accurate settings; human
play involves aiming and choosing your own dock purchases.

Storage is bounded: two four-section ships, one active cannonball, fixed
148-point traces, eight authored battles and finite magazines. Flights expire
within 420 ticks. There are no external crates, fetched assets, wall clocks,
network calls or platform IO in the model. This is a single-player stationary
ship artillery campaign, without ship steering, multiplayer or mid-campaign
saving. Browser builds, final native visual checks, audio/device behavior and
human balance testing belong to integration and are not established by the
headless results above.
