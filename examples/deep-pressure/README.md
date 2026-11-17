# Deep Pressure

A complete submarine rescue campaign: six authored cavern sorties, 27 stranded
people, one little yellow submarine. Bring every survivor to the upper-left
home dock to win. The final station has a complete ending and campaign score.
The rules run at fixed 60 Hz independently of drawing, platform input and clocks.

| Sortie | Station | Survivors | New pressure |
| --- | --- | ---: | --- |
| 1 | The Kelp Nursery | 3 | Inertia, reefs, two rescue berths, drifting jellyfish |
| 2 | Copper Cable | 4 | Thermal vent timing and cross currents |
| 3 | The Glass Cathedral | 4 | Permanently armed mines and narrow passages |
| 4 | Night Train | 5 | Strong currents and separated eastern chambers |
| 5 | The Violet Fault | 5 | Deep-water hull stress and more air consumption |
| 6 | Last Light Station | 6 | A final rescue circuit across three tall columns |

The collection contains 18 reef formations, 15 authored hazard placements
(three types), 27 named survivors, three two-level refit tracks and six medals.
Visuals are original procedural art: layered water, drifting particles, kelp,
current streaks, submarine lights, air-bell silhouettes and pulsing beacons.

## Reading the screen

The play screen explains itself with icons, dials and motion instead of text:

- **Header:** six sortie pips, the station name, and every survivor of the
  campaign as a person icon grouped by sortie: gold once home, aqua aboard,
  pulsing white while waiting. The home icon counts people landed.
- **Strip above the water:** dive clock beside the medal this pace earns (its
  gold ring drains toward the 150-second gold limit; a coral dot marks each
  hit), then the current (wave and one to three arrows), the violet pressure
  warning on the deep charts (a shield once armor is fitted) and the depth.
- **Chart:** waiting air bells glow under a bobbing chevron; a dashed reach ring
  appears on approach, green when a tap will winch, amber when the boat must
  brake first, with a Space key cap when it will work. The dock's dashed ring
  is its true reach and its home badge bobs while people are aboard. One
  porthole per berth lights up for each passenger. When the cabin is full,
  everyone is collected or air is down to the trip home, marching dots and a
  bow chevron show the route to the dock (coral when air is short).
- **Instruments:** an air dial (bubbles) whose white mark is the air the trip
  home needs, a hull dial (shield), a cabin cutaway with one seat per berth, the
  manifest (pin waiting, person aboard, check home), clickable ballast (arrow
  rising, anchor holding) and sonar (recharge ring, air cost) buttons, a speed
  bar whose green zone is slow enough to winch or dock, credits and tugs.
  Hovering a manifest name rings that diver's bell, and hovering a bell
  highlights the name.
- **Feedback:** winches burst bubbles and raise a person at the bell and a
  sparkle in the seat; docking bursts gold with landed people and pay at the
  purse; hits throw debris where they land with a hull popup and a shake (mines
  flash); refusals show "Slow down" or "Full" at the boat and a red ring on the
  thing that refused; low air pulses the dial and the water's edges red.
- **Between dives:** the briefing shows the job (people to home), the hazards,
  current and pressure ahead, and refit cards with their gain, level pips and
  coin price; the debrief shows the medal and four stat tiles; a wreck shows the
  cause and the recovery tugs left.

## Play

```sh
cargo run -p deep-pressure --offline --locked -j 2
```

| Input | Action |
| --- | --- |
| Arrows / WASD | Thrust; releasing controls leaves inertia |
| Space / Enter | Launch, winch a nearby diver, dock or continue |
| Hold Space | Brake; release and tap again to winch |
| X / E | Toggle rising light ballast / neutral ballast |
| C / Q | Sonar: highlights hazard danger circles for three seconds |
| Click ballast / sonar button | Same command as X / C while diving |
| 1 / 2 / 3 | Buy air / armor / cabin refit during briefing |
| Left / Right, then X | Select and buy a briefing refit |
| Click refit card / launch button | Same briefing action as keyboard |
| R | Restart the entire career |
| H | Key caps and a four-line guide |
| Esc / P | Pause |
| M | Sound toggle |

Winching requires speed below 35 and distance below 25 chart units. Docking
requires the same slow approach within 31 units of the dock. A diver aboard has not
yet been saved: return to the dock and press Space to unload, refill air and repair
hull. Dock service is free, but no reward is paid without a newly delivered person.

The initial boat has two berths, 42 seconds of air and 100 hull. Passengers use
extra air and reduce top speed. Water below the lowest shelf consumes additional
air. In sorties five and six, pressure below the purple line slowly damages a
boat without armor. Neutral ballast prevents passive rise; it does not cancel
currents. Hard reef collisions, jellyfish, mines and active thermal vents damage
the hull. Thermal vents are quiet for three seconds and active for two.

Sonar costs two seconds of air and takes four seconds to recharge. It shows the
actual hazard collision radii for three seconds. All survivor beacons remain
visible without sonar, so no blind exploration is required.

Every landed survivor pays 12 credits and 250 points. Air costs 30 then 50 credits
and adds 12 seconds each time. Armor costs 25 then 45, adding 20 hull per level
and reducing impact damage. Cabin space costs 35 then 55 and adds one berth per
level. Purchases are available only between sorties. Sortie medals reward low
impact counts; gold also requires a finish within 150 seconds. Completion adds
medal and time bonuses, and the host records the campaign best score.

A wreck loses this sortie's rescues and earnings. Press Space to spend one of
three recovery tugs and retry it. Earlier completed sorties and installed refits
remain. The fourth wreck ends the career. Invalid commands and repeated docking
cannot create money or score; recovery restores the sortie's starting ledger.

## Strategy

Start by setting neutral ballast. Get familiar with holding Space to slow down
before a rescue. Bring the nearest people home before attempting a long crossing.
Watch air on the outward leg; turning back early costs no money. The dock repairs
hull, so taking a shorter rescue circuit can be better than filling every berth.
An early air tank adds useful margin; a larger cabin helps efficient circuits;
armor protects against both collisions and late-campaign pressure. Observe vents
or pass outside their danger radius. Use sonar near unfamiliar hazards, not on
cooldown: its air cost matters.

## Validation

```sh
cargo test -p deep-pressure --offline --locked -j 2
cargo clippy -p deep-pressure --all-targets --offline --locked -j 2 -- -D warnings
rustfmt --edition 2024 --check examples/deep-pressure/src/{lib,main,content,model,view}.rs
cargo run -p deep-pressure --offline --locked -j 2 -- --headless
```

There are 21 behavioral tests and one executable public API doctest. Tests cover
inertia and ballast, reef collisions, cabin limits, winch range/speed, paid refit
limits, sonar consumption/cooldown, docking without reward duplication, hazard
timing/armor, oxygen loss, bounded recovery and ledger rollback, pointer actions
(including the instrument buttons mirroring X and C), all authored rescue
routes, terminal invariance, the per-tick presentation events, the guidance
trail, medal pace, header progress and copy, and the effects requested for
rescues, docking, hits and refits. A complete legal pilot
starts with `Game::new()` and rescues all 27 people using only normal `Input`;
it never teleports, mutates resources, advances stages directly or skips hazards.
Focused mechanic tests use local fixtures to isolate their conditions.

The verified headless run wins at tick 16,879 with 12,184 points. Sorties begin
at ticks 0, 1,923, 4,551, 7,167, 10,313 and 13,981. The pilot is deterministic,
read-only and uses grid navigation followed by the same physical thrust and
braking rules as a human. It is a legal smoke driver, not a claim of ideal play.

Presentation events are recorded per tick for effects only and are never read
by the rules; the guidance trail and the air mark reuse the pilot's own route
search and air rule of thumb without changing state. Native captures of the
title, help, briefing, a dive, a bell approach, a winch, docking, sonar, a reef
strike, low air, a wreck, sortie five and the results card were reviewed.
Parent integration owns additional native captures, browser/Wasm builds and shared
staging. Suggested native captures: autoplay tick/frame 600 for the nursery,
3,000 for the cable station and 11,000 for the violet pressure layer. Example:

```sh
cargo run -p deep-pressure --offline --locked -j 2 -- \
  --autoplay --smoke-frames 600 --screenshot /tmp/deep-pressure-600.png
```

Headless validation proves campaign reachability and model semantics; it is not
manual visual, audio, touchscreen, browser or human difficulty testing. No
external engine/game dependencies, assets, network services or save protocols
were added. Persistent best scores and lifecycle controls belong to the shared
arcade host; individual unfinished sorties are not saved across app restarts.
