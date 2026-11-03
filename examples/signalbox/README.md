# Signalbox

Take the brass key to Bramble Junction and bring every train home. Signalbox is a
complete railway dispatch campaign: **eight authored shifts, 61 scheduled
services, four destination platforms, and one shared bridge**. The train rules
run in Rust on Ferravex's fixed-tick host; the same campaign builds for native
windows and the browser.

Study both point settings, let a train through a signal, and keep the next
service safely waiting. Fast mail, slow freight, opposing traffic, rain, snow,
bridge inspections, and tighter due times gradually complicate the railway.
There is no reflex penalty for thinking: planning mode stops the railway clock.

## Playing

```sh
cargo run -p signalbox --offline --locked -j 2
```

A supported native display/OpenGL host is required. For the shared browser
catalog, use the repository's parent-owned web build and open the Signalbox
entry. No downloadable art, fonts, or external libraries are required.

The title screen and each shift have instructions. Enter the signalbox with
**Space/Enter**, deliver every service before the closing bell, then advance to
the next timetable. Complete the eighth shift to receive the brass key and a
career rank. Losing all three shift retries and then suffering another incident
ends the career; **R** immediately begins a fresh career through the shared host.

## Controls

| Control | Action |
| --- | --- |
| Left / Right, or A / D | Select one of the six switchboard controls |
| Space / Enter | Operate selected control; accept a briefing or completed shift |
| Up / Down, or W / S | Cycle selected control forward/backward |
| 1–5 | Select the first five switchboard controls; Right reaches the sixth |
| Click a bottom control | Operate that signal or set of points |
| X / E, or click the planning button | Stop the railway clock to plan; resume it |
| C / Q | Spend one of three career retries and reopen the current shift |
| Escape / P | Shared pause menu |
| H | Shared help |
| M | Shared sound toggle |
| R | Restart the entire career |

Input acts on **presses**, not held keys. Space does not repeatedly cycle a switch
while held. Mouse clicks and keyboard controls operate the same rules.

## Routes and signals

Every train carries a destination letter on its roof. The timetable repeats its
letter, due time, and current state. A/B are the west-bank stations and C/D are
the east-bank stations. The point controls cycle **STRAIGHT → NORTH → SOUTH**.
Points choose a route in either direction; they do not know a train's destination.

| Origin → destination | West points | East points | Signals to clear |
| --- | --- | --- | --- |
| West → A (Abbey) | North | — | West gate |
| West → B (Orchard) | South | — | West gate |
| West → C (Quarry) | Straight | North | West gate, west bridge |
| West → D (Harbour) | Straight | South | West gate, west bridge |
| East → C (Quarry) | — | North | East gate |
| East → D (Harbour) | — | South | East gate |
| East → A (Abbey) | North | Straight | East gate, east bridge |
| East → B (Orchard) | South | Straight | East gate, east bridge |

A green signal releases **one** train and automatically returns to red. The
origin yards queue waiting services safely, but the open railway does not provide
automatic routing or collision protection. Two opposing trains on the single
bridge can collide. Fast mail can also catch a slow train ahead. Points lock when
a train is within their protected area; trying to change locked points is
rejected without a sound or scoring event.

Wrong platforms, leaving the network without a delivery, collisions, and an
end-of-shift bell with services outstanding are safety incidents. An incident
holds the railway until you spend a retry. Existing commissions survive; the
failed shift's provisional earnings are discarded. Retrying an unfinished shift
with C spends the same retry resource.

## The eight shifts

| Shift | Services | New challenge | Closing bell |
| --- | ---: | --- | ---: |
| The apprentice's bell | 4 | Learn near-bank and cross-bridge routes | 125 s |
| Two banks, one bridge | 5 | Opposing traffic and fast mail | 150 s |
| Harvest wagons | 6 | Slow, heavy freight and mixed traffic | 180 s |
| The bridge inspector | 7 | Bridge entrances closed at 18–34 s | 200 s |
| The seaside special | 8 | Tight due times and overlapping services | 220 s |
| Rain on the wires | 9 | 85% speed; closure at 26–43 s | 260 s |
| Winter through service | 10 | Snow reduces speed to 78% | 320 s |
| The royal timetable | 12 | Final inspection at 40–57 s; dense timetable | 340 s |

Each shift has its own approach geometry and authored service order, origins,
destinations, speeds, release times, and due times. The six-control network stays
recognizable. Bridge inspections block new entries; an already-crossing train
can finish. Pause/planning time never advances these windows.

## Commission, medals, and replay

A delivery earns 200 provisional commission plus up to 180 for punctuality.
The punctuality bonus decreases by one per 0.2 seconds late, down to zero. Safe
late deliveries still count. A completed shift banks its provisional earnings
plus 300 and 150 per medal star; provisional earnings cannot be farmed through
retries.

- **Gold / three stars:** every train on time.
- **Silver / two stars:** at most 45 seconds of combined lateness.
- **Bronze / one star:** everyone safely delivered before closing.

The full campaign awards up to 24 stars. Final ranks range from Keeper of the
Brass Key to Master of the Line. The shared host retains the best
career record on the local device. There is no mid-shift resume save.

A safe first strategy is to release one train at a time. For higher ranks, route
near-bank services while another train crosses the bridge, and prioritize mail
before it catches freight. Use X to examine the whole board before opening a
signal. A slow but safe dispatcher can finish every shift.

## Source and checks

- `src/content.rs`: eight authored schedules and district conditions.
- `src/model.rs`: bounded train state, fixed ticks, routing, signals, collision
  checks, scoring, retries, legal dispatcher, and behavioral tests.
- `src/view.rs`: read-only countryside, railway, trains, timetable, switchboard,
  briefings, and shift results.
- `src/lib.rs`: shared host integration and player metadata.

```sh
cargo test -p signalbox --offline --locked -j 2
cargo clippy -p signalbox --all-targets --offline --locked -j 2 -- -D warnings
```

The ten unit tests include a complete eight-shift win through ordinary player
inputs, both terminal outcomes, geometric collisions, wrong routes, red signal
holds, protected points, bridge inspection refusal, planning, deadline failure,
bounded retries, and one-time scoring. A rustdoc example executes the pure model.
The bot uses actual switch and signal controls; it has no invulnerability,
teleportation, direct deliveries, or privileged routes.

For parent-owned native visual smoke, an active first-shift capture is:

```sh
cargo run -p signalbox --offline --locked -j 2 -- \
  --autoplay --smoke-frames 1400 --screenshot target/games20/signalbox.png
```

Tests establish behavior on the tested host; they are not a human playtest or
proof of browser, audio-device, mobile, or cross-platform behavior. Native visual
smoke, browser staging, catalog integration, and broader release validation are
owned by the parent integration task.
