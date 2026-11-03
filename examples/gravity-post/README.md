# Gravity Post

An eight-world precision flying campaign about delivering the things that keep
isolated communities connected. A letter, a medical valve, seeds and a peace
treaty all have to survive a real landing. Collect each parcel at its yellow pad,
then bring it to its green recipient to earn your courier certificate.

This is one of [The Twenty](../../docs/GAMES_20.md). The
[shared campaign host](../shared/arcade/README.md) supplies the title, field guide,
pause, control remapping, new-career retry, ending and local best records. The
game owns its physics, maps, cargo, resources, progression and procedural art.

## The route

Each authored world has a blue departure depot, yellow pickup, green destination,
purple supply outpost and one optional airborne survey beacon. Their roles move
between four landing sites, so later jobs cross the chart in different directions.
The rocky terrain and pad elevations also change. All eight deliveries are
required; beacons and outpost detours are optional.

| Contract | Parcel and recipient | Gravity | Wind | Storm deadline |
| --- | --- | --- | --- | --- |
| Postmark Moon | A child's first letter for South Dome School | 14 | Calm | 180 s |
| Copper Mesa | A replacement heart valve for the mesa clinic | 18 | +3, gust 1 | 180 s |
| Glass Tundra | Orchard seeds for the warmhouse growers | 16 | -4, gust 2 | 190 s |
| Verdant Rift | A recording of spring rain for deep-rift listeners | 21 | +1, gust 4 | 200 s |
| Ashen Harbour | A focusing lens for the lighthouse keepers | 24 | -3, gust 4 | 200 s |
| Ringshadow | A navigation clock for the ring observatory | 11 | +3.5, gust 2.5 | 210 s |
| Thunder Crown | A peace treaty for the western council | 29 | +4, gust 3.5 | 210 s |
| Dawn Archive | The collected letters of eight worlds for the archivist | 26 | -2, gust 5 | 220 s |

Gravity and wind are accelerations in metres per second squared. Positive wind
pushes right. Gusts vary with simulation ticks; there is no wall-clock randomness.
The deadline starts when you open the route, continues while landed, and pauses
with the shared pause/help/settings screens. Briefings and refit decisions do not
consume flight time.

## Controls

- **Left/right** or **A/D** tilt the ship, limited to 65 degrees from upright.
- **Up/W** or held **Space/Enter** fires the main engine.
- **Down/S** holds the retro-brake, overriding the engine. It levels the ship and
  approaches an eight-metres-per-second downward drift while gravity still acts.
- **X/E** services the pad where you have landed: load, deliver, refuel or repair.
  It does nothing in midair.
- **C/Q** requests recovery during a flight. This abandons that contract's current
  attempt and opens the recovery screen.
- **1/2/3** buy engine, tank or landing-gear refits between successful deliveries.
  The three refit cards can also be clicked.
- **Space/Enter** opens a briefing, advances after a delivery or spends a recovery
  permit on the wreck screen. The briefing and next-contract buttons accept clicks.
- **Esc/P** pauses, **H** opens the paginated guide, **F2** remaps controls,
  **M** toggles sound and **R** starts a completely new eight-world career.

Desktop keyboard play is the intended control scheme. Pointer input covers menus
and refits; it does not steer the ship.

## Flying and landing

The model integrates velocity and position at 60 fixed ticks per second. Thrust
points along the tilted ship's axis. Coasting preserves inertia while wind,
gravity and each world's light atmospheric drag act on the ship. There is no
automatic pad snapping before a valid touchdown.

Climb above a ridge before beginning a long traverse. Tilt toward your destination
to accelerate, then tilt back early to reduce sideways speed. Watch the ground
shadow and instruments rather than judging speed from the flame alone. The
retro-brake is useful for the last approach, but holding it for an entire trip
spends fuel while making little horizontal progress.

The ship uses a 30-metre-wide collision footprint and a ten-metre foot offset.
Every pad is 64 metres wide. Both sides of the footprint must fit. Stock landing
limits are at most **10 m/s sideways**, **18 m/s downward**, and about **14 degrees
of tilt**. Successful touchdown stops the ship and levels it. A downward speed
above the limit, excessive sideways speed, poor attitude or touching rough ground
causes an impact instead. Collision damage depends on actual impact speed.

Pad and terrain collision tests run every tick; motion is bounded to at most
three metres per axis per tick. These are deliberately approachable arcade contact rules,
not a rotating rigid-body simulation. The ship can bounce after a survivable
impact. A short 45-tick impact cooldown prevents one contact from dealing damage
on every tick.

The carried parcel adds 22% effective mass, reducing main-engine acceleration.
It also takes damage in impacts. Zero hull or zero parcel integrity causes a
wreck, as does reaching the storm deadline. An empty tank disables both powered
controls; it does not suspend gravity. If stranded without usable pad reserves,
request recovery with C.

## Fuel, surveys and refits

The stock tank holds 120 fuel units. Full main thrust uses 3.8 units per second;
the retro-brake uses 1.9. The depot holds 75 reserve units, the pickup 28 and the
purple outpost 65. X transfers only what fits in your tank, subtracting that
amount from the pad's reserve. The outpost has one repair kit worth up to 30 hull.
Supplies are local to the contract and cannot be regenerated by repeated service.

The diamond survey beacon is visible above the terrain. Passing within 18 metres
logs it once and awards 250 points, 20 credits and up to 15 fuel. Its bonus cannot
be collected twice during an attempt. A direct route often passes near a beacon,
but spending the last fuel chasing one can lose the parcel.

Each completed contract awards 120 credits plus 20 per earned stamp. Refits are
permanent for the current career, with two levels in each track:

| Refit | First / second price | Effect per level |
| --- | --- | --- |
| Engine | 110 / 170 credits | +14 unloaded thrust acceleration and -0.3 main-engine fuel burn per second |
| Tank | 90 / 150 credits | +30 fuel capacity |
| Landing gear | 80 / 140 credits | +3 m/s to both landing speed limits, +0.06 radians tilt allowance, and 15% less hull impact damage |

Cargo damage is not reduced by the gear's hull protection. New contracts begin
at their depot with a full tank and repaired ship; purchased refits persist.
The refit desk rejects unaffordable or already-maxed purchases without spending
anything. You may skip purchases and continue with Space.

## Stamps, recovery and the ending

Deliver with at least 90% parcel integrity and 80% hull for three stamps. At least
60% integrity and 40% hull earns two; any other surviving delivery earns one.
Each stamp gives 500 points. Delivery also awards eight points per cargo-integrity
percent, four per hull percent, four per remaining fuel unit and three per whole
second remaining. Loading a parcel earns 100 points once. Beacon bonuses are
additional. The combined resource bonus is truncated to whole points.

There are **three recovery permits for the whole career**. A wreck offers a
deliberate retry while permits remain. Retrying restores the beginning of that
contract, including its cash and score checkpoint, fresh supplies and an unlogged
beacon. Earlier completed deliveries and refits remain. Failed attempts therefore
cannot farm cash, score or free beacon fuel. A fourth wreck ends the career.

Deliver the Dawn Archive capsule to complete the campaign. The certificate reports
all eight deliveries, the 24 possible stamps, survey coverage, credits, permits and
career score. You can immediately begin another career to improve the result.
The host stores completed-run records and personal bests when storage is available;
unfinished careers are not saved.

## Run and verification

From the repository root:

```sh
cargo run -p gravity-post --offline --locked -j 2
cargo run -p gravity-post --offline --locked -j 2 -- --headless
cargo test -p gravity-post --offline --locked -j 2
cargo clippy -p gravity-post --all-targets --offline --locked -j 2 -- -D warnings
```

The headless pilot reads the chart and ship instruments and generates ordinary
tilt, thrust, brake, service and refit inputs. It never changes position, fuel,
cargo, collision rules or progress directly. Its complete legal run
finishes at tick **27007**, roughly **7 minutes 30 seconds** of simulated ticks:
eight parcels, 23/24 stamps, all eight beacons, 900 credits, all three recovery
permits and **31,678 points**. The replay test runs a second fresh game with the
same command stream and compares their entire states after each tick.

Sixteen behavioral tests cover that campaign, menu inactivity, thrust and cargo
mass, gravity/wind/inertia, retro-braking, empty fuel, valid/invalid landings,
gear effects, rough-ground/border impacts, finite service/repair, cargo-gated
delivery, one-shot beacon rewards, rollback, deadline and bounded retries,
paid/capped refits, pointer card bounds, authored pad geometry and frozen terminal
states. A public Rust API doctest executes the pure model without opening a window.

Native visual capture requires a graphics display. Useful windows include frame
600 for ascent, 1400 for the first pickup approach, 2500 for loaded flight and
7000 for a later world. Parent review found the frame-600 chart, terrain, pad labels,
beacon and instruments legible. The parent owns final native/browser builds,
captures and combined collection checks. Compilation and a headless winning route
do not establish human difficulty balance, physical gamepad/mobile support,
audio-device quality, cross-platform floating-point determinism or browser behavior.

```sh
cargo run -p gravity-post --offline --locked -j 2 -- --autoplay --smoke-frames 600 --screenshot target/gravity-post.png
```
