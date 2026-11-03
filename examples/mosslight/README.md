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
| Left click | Inspect a tile; move or strike if it is adjacent |
| Esc / P | Pause |
| H | Field manual |
| R | Begin a fresh expedition |
| M | Toggle sound |

Holding a direction does not repeatedly spend turns. An idle fixed tick leaves
all dungeon rules untouched. Blocked movement, an empty burst or an unnecessary
flask spend no turn or resource. Input priority accepts at most one action in a
tick: flask, burst, guard, then movement.

## The expedition

| Floor | Place | Main lesson |
| --- | --- | --- |
| 1 | The Lantern Stair | Read rootling attacks, find the leaf sigil and unlock the stairs |
| 2 | The Glasshouse | Break line of sight to spore beams; introduce bramble knights |
| 3 | The Thorn Library | Use recovery turns and choose routes through winding shelves |
| 4 | The Flooded Choir | Manage flasks and avoid intersecting attack lanes |
| 5 | The Amber Vault | Decide how much optional daylight is worth the detour |
| 6 | The Root King's Hall | Defeat the King, retrieve the sigil and escape with the dawn |

Every floor contains a seed sigil, a moonwell, a flask and optional amber.
Stepping on the sigil unlocks gold seals. Reach the stair tile and press Space to
finish a floor. The final stairs also require the Root King's defeat.

After each of the first five floors, a camp offers one lasting relic:

- **Sunsteel:** +1 melee damage, which also strengthens lantern bursts.
- **Rootheart:** +5 maximum health and an immediate five-point heal.
- **Long Wick:** +1 charge capacity and two immediate charges.

All camps also restore seven health, one flask and one charge, capped by your
current capacities. Flasks are limited to five. Moonwells restore eight health,
one flask and one charge **once per floor**. A full flask belt leaves an unopened
flask in place so you can return later.

## Read the enemy, then act

Red outlined cells show attacks that resolve after your next accepted action.
The target cells remain fixed during that action: step away, kill the attacker,
or press Space to reduce each incoming hit by three. Creatures recover for one
turn after attacking. Gold arrows indicate movement; a `z` indicates recovery.

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

`src/content.rs` holds the authored maps. `src/model.rs` owns pure bounded rules;
`src/view.rs` draws the board, original procedural characters, attack marks,
status panel and camp cards. No game rule polls a renderer, wall clock or I/O.
The shared `ferravex-arcade` host consumes fixed-tick action edges and owns saves,
window/browser integration, audio and pause/restart.

```sh
cargo test -p mosslight --offline --locked -j 2
cargo clippy -p mosslight --all-targets --offline --locked -j 2 -- -D warnings
cargo run -p mosslight --offline --locked -j 2 -- --headless
```

The package check executes thirteen behavioral tests and one rustdoc example.
The complete-campaign regression starts from a fresh game and submits only legal
player commands. It visits all six floors, chooses five relics, defeats the King
and reaches victory. The `--headless` executable independently prints each floor
milestone and fails if the same legal player does not finish.

For the integrator's graphical smoke capture, suitable points are 420 frames
(the Glasshouse) and 1,350 frames (the King's hall):

```sh
cargo run -p mosslight --offline --locked -j 2 -- --autoplay --smoke-frames 420 --screenshot target/mosslight-glasshouse.png
```

This implementation agent ran model tests, the headless expedition and strict
Clippy. It did not run a graphical playtest, Wasm/browser build, audio-device test,
or cross-platform/device validation. Those remain collection integration checks.
