# Mosslight

Carry the village's last ember beneath a forest that stole the sun. Mosslight is
a complete, turn-based dungeon expedition: six authored floors, three ordinary
enemy species, a telegraphed final boss, permanent relic choices, limited healing
and lantern charges, optional treasures, defeat, and a scored ending.

The dungeon waits for you. There is no real-time pressure and no need to hold
movement keys. A careful expedition takes roughly a few hundred decisions;
the reproducible smoke player reaches the ending in 155 accepted turns.

## Play

```sh
cargo run -p mosslight --offline --locked -j 2
```

The shared Ferravex host supplies the title/tutorial, pause, help, restart,
accepted-event audio and local best-record storage. Browser publication and
catalog registration are owned by the collection integrator.

| Control | Action |
| --- | --- |
| Tap arrows or WASD | Move one tile; bump a creature to strike |
| Space / Enter | Guard; descend when standing on stairs |
| X / E | Drink one flask, restoring up to 12 health |
| C / Q | Spend one lantern charge to burst every foe within two tiles |
| 1 / 2 / 3 at camp | Choose blade, heart or lantern relic |
| Left click on the board | Move or strike an adjacent tile; mark a distant creature |
| Left click on a sidebar button | Same as its key: guard/descend (Space), flask (X), burst (C) |
| Left click on a relic card | Choose that relic |
| Hover | Preview a step, strike, flask, burst or relic before committing |
| Esc / P | Pause |
| H | Field manual |
| R | Begin a fresh expedition |
| M | Toggle sound |

Holding a direction does not repeatedly spend turns. An idle fixed tick leaves
all dungeon rules untouched. Blocked movement, an empty burst or an unnecessary
flask spend no turn or resource. Input priority accepts at most one action in a
tick: flask, burst, guard, then movement. A click on a sidebar button presses
exactly that button's key, so both share one rule path.

## Reading the screen

The interface shows state instead of describing it:

- **Header:** six floor pips and the floor's name. Under them, a quest chain
  shows the floor's goal: the key, the Root King's crown on the last floor, then
  the stairs. Done steps turn green and the current one glows, next to a short
  objective such as "Find the key".
- **Vitals:** a heart with current and maximum health over a segmented bar
  (a notch every five), and a sword with the blade's damage. When a telegraphed
  attack covers the hero's tile, the part of the bar it would take blinks red and
  a heart chip over the hero shows the blow.
- **Action buttons:** a shield with Space (each blow lands three weaker), a potion
  with five flask vials and X, and a flame with lantern charge pips and C.
  Unusable actions dim. The flask button glows when health is at half or less,
  and the lantern glows when two or more foes are in reach. On open stairs the
  shield becomes a gold descend arrow, and a Space prompt rises over the hero.
- **Run stats:** an hourglass with turns taken, a gem for each amber jar on this
  floor (lit once taken) and a skull with foes defeated.
- **Foes:** one row per living creature with its portrait, name, health, the
  damage it deals and its intent: `z` resting, a gold arrow moving, a red `!`
  about to strike. Hovering a row or a creature links the two and brightens that
  creature's attack cells.
- **Previews:** hovering a neighbouring tile, a sidebar button or a relic card
  steps that exact command on a copy of the model. The board shows the outcome as
  icon chips (a heart for health gained or lost, a sword for damage dealt, a skull
  for a kill, the key, amber, a flask). Foes' health bars ghost the blow and the
  sidebar ghosts health, flasks and charges. Hovering the lantern also lights the
  burst's two-tile reach. A refusal shows its reason instead: a lock, a crown,
  "Full" or "Too far".
- **Effects:** each accepted event is felt where it happens. Hits pop their damage
  and spark, kills burst in the creature's colours, and landed blows shake the
  screen and pop a red heart. Taking the key rings every lock it opens, the lantern
  burst rings out to its reach, and a new floor's name rises over the board.
  Refusals give a small shake and a red ring on whatever refused.

## The expedition

| Floor | Place | Main lesson |
| --- | --- | --- |
| 1 | The Lantern Stair | Read rootling attacks, find the key and unlock the stairs |
| 2 | The Glasshouse | Break line of sight to spore beams; introduce bramble knights |
| 3 | The Thorn Library | Use recovery turns and choose routes through winding shelves |
| 4 | The Flooded Choir | Manage flasks and avoid intersecting attack lanes |
| 5 | The Amber Vault | Decide how much optional daylight is worth the detour |
| 6 | The Root King's Hall | Defeat the King, retrieve the key and escape with the dawn |

Every floor contains a key (the seed sigil), a moonwell, a flask and optional
amber. Stepping on the key unlocks the gold seals, whose lock badges disappear,
and the stairs, which start to glow. Reach the stair tile and press Space to
finish a floor. The final stairs also require the Root King's defeat; until then
they show a red crown badge.

After each of the first five floors, a camp offers one lasting relic as a card
with its icon and gain. Hovering or selecting a card previews the result in the
sidebar and lists the next floor's creatures:

- **Sunsteel:** +1 melee damage, which also strengthens lantern bursts.
- **Rootheart:** +5 maximum health and an immediate five-point heal.
- **Long Wick:** +1 charge capacity and two immediate charges.

All camps also restore seven health, one flask and one charge, capped by your
current capacities. Flasks are limited to five. Moonwells restore eight health,
one flask and one charge **once per floor**. A full flask belt leaves an unopened
flask in place so you can return later.

## Read the enemy, then act

Red bracketed cells show attacks that resolve after your next accepted action.
They breathe in and out and deepen with the damage they carry. A knight striking
from two tiles away is linked to its cell by a dashed lane. The target cells
remain fixed during that action: step away, kill the attacker, or press Space to
reduce each incoming hit by three. Creatures recover for one turn after attacking.
Gold arrows indicate movement, `z` recovery and a red `!` a pending strike.

- **Rootlings** chase nearby and bite for three damage.
- **Bramble knights** strike a chosen cell up to two tiles away for five damage.
- **Spore bulbs** fire six-tile straight beams for four damage. Walls and gold
  seals block beams.
- **The Root King** alternates a four-tile cross with a close, two-tile ring.
  Each hit deals seven damage; each attack is followed by a recovery turn.

Bumping a creature attacks without moving into its tile. A slain creature's
pending attack is cancelled. Lantern bursts deal blade damage plus three to all
foes within Manhattan distance two, including through thin walls. Thorns deal
two damage when stepped on. Drinking a flask also gives surviving enemies a turn.

## Scores and replay goals

A seed is worth 100, each defeated ordinary foe 80, each amber jar 140, each
cleared floor 350 and the King 700. Victory adds 2,000, twenty points per remaining
health, and an efficiency bonus of `max(0, 600 - turns)`. Unique rewards cannot be
collected twice. Ending ranks are Lantern Bearer, Dawnbringer at 6,500 and
Sunkeeper at 7,800. Try a different relic build or retrieve amber that the shortest
route leaves behind.

## Implementation and validation

`src/content.rs` holds the authored maps. `src/model.rs` owns pure bounded rules
and records a bounded list of presentation events for each executed tick (what
was accepted or refused, and where). No rule reads that list. `src/view.rs` draws
the board, original procedural characters, attack marks, sidebar and camp cards.
It also implements the arcade hooks: header pips (`progress`), the quest chain
(`draw_objective`), location effects (`effects`) and the results card
(`draw_result`). Hover previews step a copy of the model. Drawing never mutates the
game, and no game rule polls a renderer, wall clock or I/O. The shared
`ferravex-arcade` host consumes fixed-tick action edges and owns saves,
window/browser integration, audio and pause/restart.

```sh
cargo test -p mosslight --offline --locked -j 2
cargo clippy -p mosslight --all-targets --offline --locked -j 2 -- -D warnings
cargo run -p mosslight --offline --locked -j 2 -- --headless
```

The package check executes twenty behavioral tests and one rustdoc example.
The complete-campaign regression starts from a fresh game and submits only legal
player commands. It visits all six floors, chooses five relics, defeats the King
and reaches victory. The `--headless` executable independently prints each floor
milestone and fails if the same legal player does not finish. Further tests cover
the sidebar buttons (each click matches its key, including refusals and
descending), relic card clicks, event bookkeeping, header progress at the start
and end, effects for the key and a landed blow, and side-effect-free previews.

For the integrator's graphical smoke capture, suitable points are 420 frames
(the Glasshouse) and 1,350 frames (the King's hall):

```sh
cargo run -p mosslight --offline --locked -j 2 -- --autoplay --smoke-frames 420 --screenshot target/mosslight-glasshouse.png
```

The visual pass ran the tests, the headless expedition, strict Clippy and native
smoke screenshots (play, hover previews, camp, both endings). It did not run a
hands-on playtest, Wasm/browser build, audio-device test, or cross-platform/device
validation. Those remain collection integration checks.
