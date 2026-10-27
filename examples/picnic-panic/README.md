# Picnic Panic

A small, very determined raccoon crashes the annual village picnic on
Thistledown Green. Work through a hand-written to-do list of pranks while
Nana Hollis, Mr. Pumble, Maestro Trumble, little Pip and Biscuit the dog go
about their afternoon, notice you, chase you and carry you back to your burrow.

## Today's to-do list

1. **Steal a whole pie.** Nana's pie is cooling on the red gingham table. Your
   den, by the south-west hedge, is very hungry.
2. **Swap the marrow for a pinecone.** Pinecones grow under the pines; the prize
   marrow sits on the judging table. Mr. Pumble will judge what he finds.
3. **Make the band play off-key.** Borrow the sheet music from the bandstand and
   keep walking.
4. **Send a blanket sledding.** The tea tray is on the hilltop table. Put it near
   the crest, load the folded blanket on it, and give it a shove.
5. **Ring the village bell.** Hold the grab button at the rope by the gate.
6. **Whoopee-cushion a picnicker.** Somebody's toy box. Somebody's favourite bench.
7. **Launch a melon off the seesaw.** Melon on the low end. Raccoon from a great
   height (the hay bales help; a mid-air belly-flop helps more).
8. **???** Finish the list and something grand appears. Then make your escape.

Picnickers have readable vision cones: white while calm, yellow with a "?"
when something caught their eye, red with a "!" when they give chase. Sneak
(half a stick press, or a gentle key tap) to shrink their view. Hide in bushes,
under tablecloths or in the picnic basket; a searcher who pokes your bush for a
moment will find you. Chitter to lure someone away. Owners return displaced
belongings, so finish what you start. Adults scruff you and carry you to your
newest burrow (wriggle free with six jumps); Pip hugs and Biscuit licks, which
only costs a moment and your dignity. Shiny bottle caps and six hidden bonus
tasks reward curiosity; hats unlock in the Wardrobe and your best day leaves a
speedrun ghost.

## Controls

| Action | Keyboard / mouse | Gamepad | Touch |
| --- | --- | --- | --- |
| Move (half press to sneak) | WASD or arrows | Left stick | Drag the left side |
| Camera | Drag with the mouse, Z / X, wheel to zoom | Right stick | Drag the right side |
| Jump | Space | A | Jump |
| Grab, drop, place, use (hold) | E | X | Grab |
| Throw | F or right mouse | B | Throw |
| Chitter | C | Y | Chitter |
| Belly-slide (mid-air: belly-flop) | Shift | RB | Slide |
| To-do list | N | LB | Tap the list |
| Pause | Escape | Start | Pause button |

Open **Controls** (F2) to inspect or remap bindings. Reduced motion freezes
sway, shake and set-piece camera swoops; reduced flash removes flashes and caps
bloom. Vision cones and the best-run ghost can be toggled in the pause menu.

## Run and inspect

```sh
cargo run -p picnic-panic --release --offline --locked -j 2
cargo test -p picnic-panic --offline --locked -j 2
cargo run -p picnic-panic --release --offline --locked -j 2 -- --autoplay --smoke-frames 600 --screenshot /tmp/picnic.png
```

Debug builds add review hooks (`PICNIC_WINDOW`, `PICNIC_SKIP`, `PICNIC_CAM`,
`PICNIC_DONE`, `PICNIC_END_AT`, …); see `src/capture.rs`.

## Implementation

- `sim/` owns the seeded 60 Hz rules with no platform I/O: the
  [authored park](content/park.fxscene) is loaded through the engine's
  schema-validated scene format; `props.rs` builds the rigid-body world (hinged
  seesaw, bell and sign, distance-joint bunting); `raccoon.rs` drives the shared
  character motor; `folk.rs` runs sight cones, hearing, expiring memory, a shared
  decision graph and a budgeted grid path planner; `pranks.rs` feeds an engine
  objective graph; `bot.rs` is the autoplay demo raccoon.
- `run.rs` connects the rules to `GameSession`: input edges are consumed only on
  executed ticks, the winning tick ends the batch and pause discards time.
- `scene.rs` and `models.rs` draw the AO-baked, chunked low-poly park with sun
  shadows, lanterns, LOD trees and custom sway, glow, water, jelly, ghost and
  vision-cone shader hooks; `rig.rs` animates skinned characters with an
  animation graph, cross-fades and additive layers.
- `fx.rs`, `sfx.rs`, `hud.rs` and `menus.rs` consume accepted events only:
  particles, trails, set-piece framing, positional sound, an adaptive score with
  a spatial (eventually off-key) brass band, the notepad, toasts and the menus.
- `meta.rs` stores lifetime progress, hats, the in-progress to-do list (the
  engine's objective snapshot) and the best-run ghost in versioned save slots.

Tests cover each prank's completion conditions, perception, chases, catches,
wriggling free, hiding and search give-up, chitter lures, throwing, owners
fetching belongings, the finale gate and escape, save round-trips, determinism
across render cadences and pauses, and a zero-allocation steady state. They do
not establish physical touch/gamepad behavior, audible sound or frame rates on
other devices. See [platform limits](../../docs/PLATFORMS.md).
