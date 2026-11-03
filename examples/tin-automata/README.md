# Tin Automata

A robot-programming puzzle campaign about bringing the Daybreak factory back to
life. Write a short instruction tape, watch your tin courier obey it, and repair
the program when its route goes wrong. Deliver every numbered parcel to its
matching dock to restore a workbench; finish all twelve to revive the factory.

The campaign contains twelve distinct authored 9-by-7 workbenches, seventeen
parcels, seven instructions, two switch circuits, and thirty-six available stars.
Its mint, copper and cream workshop art is drawn procedurally. Parcel numbers,
gate letters, the robot's facing arrow, tape cursor and execution highlight make
the rules visible without relying only on color.

## Start playing

From the repository root:

```sh
cargo run -p tin-automata --offline --locked
```

Read the title screen's field guide with **H**, then start. In the first room,
write **Forward, Grab, Forward, Forward, Forward, Forward, Drop**. Choose each
instruction with Left/Right, press Space to write it, and press **1** to run.
The robot starts facing east. A room ends immediately after its last delivery;
Space then opens the next workbench with an empty tape.

## Controls

| Control | Action |
| --- | --- |
| Left/Right or A/D | Choose an instruction while editing |
| Up/Down or W/S | Move the tape editing cursor |
| Space or Enter | Write/replace the selected instruction and advance; after a fault return to editing; after a clear enter the next room |
| X or E | Delete the selected instruction; at the append cell delete the last instruction |
| 1 | Run from the restored board; during execution pause/resume |
| 2 | Start with one step, or perform one execution step and pause |
| C or Q | Stop and restore this workbench, retaining the program |
| 3 | Undo a tape edit, including deletion or clear; up to 32 edits |
| 4 | Clear the tape while editing; undo can recover it |
| 5 | Open/close the current room's hint; opening it pauses execution |
| Click a tape cell | Select that cell while editing |
| Click the instruction bar | Cycle through available instructions |
| Click a bottom button | Run, step, undo, clear or hint |
| H | Shared field guide |
| P or Escape | Shared campaign pause |
| R | Restart the entire campaign, including earned stars and current tape |
| M | Toggle shared audio |

Closing a hint does not resume execution automatically; press **1** to resume.
Editing commands are ignored while the robot runs or is paused. Use **C** to
restore the board before editing. Holding a direction does not repeatedly change
the tape: the editor consumes key presses.

## Instructions and factory rules

| Instruction | Effect |
| --- | --- |
| Forward | Move one tile in the robot's current facing direction |
| Left | Turn ninety degrees left without changing position |
| Right | Turn ninety degrees right without changing position |
| Grab | Pick up a parcel on the robot's current tile; capacity is one |
| Drop | Place the carried parcel on the current tile |
| Repeat 2 | Execute the most recent simple instruction two additional times; unlocked in room 5 |
| Repeat 3 | Execute the most recent simple instruction three additional times; unlocked in room 6 |

Forward followed by Repeat 3 moves four tiles in total. A repeat instruction
needs an earlier simple instruction. Consecutive repeats use that same simple
instruction rather than recursively expanding each other. The repeat command
itself and each expanded copy are separate visible execution steps; automatic
execution performs one step every 18 fixed ticks, or 0.3 seconds at 60 Hz.

The robot can carry a parcel across other parcels. It can set one down on empty
floor and pick it up again later. A numbered dock permanently accepts only its
matching parcel. Dropping on the wrong dock, dropping onto another parcel,
grabbing with full arms, or using empty arms incorrectly causes a fault without
destroying parcels. Walls, pits and closed gates also stop an attempted move.

Walking onto switch **a** latches gate **A** open for this attempt; **b** opens
**B**. The switch stays active after the robot leaves. A stop or retry restores
the robot, parcels, deliveries and both switches together while preserving the
edited tape. A fault freezes execution and records one failed attempt; press
**C** or **Space** to edit and try again. There is no campaign game-over limit.

Each room restricts the number of occupied tape cells. The editor cannot exceed
that budget, and no tape can exceed 32 instructions. An attempt also stops at
160 execution steps, so an invalid program cannot run indefinitely. Undo affects
the tape, not accumulated hint or fault penalties.

## Campaign and medals

| Room | Workbench | New challenge | Tape limit | Three-star par |
| --- | --- | --- | ---: | ---: |
| 1 | First Parcel | Pick up and deliver | 10 | 7 |
| 2 | The Right Angle | A corridor with a right turn | 12 | 9 |
| 3 | Upstairs Mail | Left turns and northbound delivery | 12 | 9 |
| 4 | The Missing Floor | Route around a broken floor | 16 | 14 |
| 5 | Repeat Department | Compress a straight route with Repeat 2 | 8 | 7 |
| 6 | Long Delivery | Combine Repeat 3 with a corner | 11 | 9 |
| 7 | Wake the Switch | Open a gate along the delivery route | 8 | 6 |
| 8 | Switchback | Detour to a remote switch | 18 | 15 |
| 9 | Two Addresses | Deliver two numbered parcels in one tape | 21 | 17 |
| 10 | Twin Relays | Activate two circuits in sequence | 12 | 10 |
| 11 | Assembly Line | Three deliveries and two return journeys | 32 | 29 |
| 12 | The Daybreak Engine | Three unequal delivery routes and linked gates | 32 | 29 |

Three stars require a program at or below par, no faults, and no hint use.
Two stars allow at most four extra instructions and two faults. Other successful
programs earn one star. Viewing a hint caps that room at two stars, even after
undoing or clearing the tape. Stopping an experiment before it faults is free.

Each completed room banks `500 + 500 * stars + max(100 - executed_steps, 0)`
points once. Editing, failed actions and replaying a solved screen cannot farm
score. The final screen reports restored workbenches, stars and total points.
The shared host can retain completed-run records locally when available; an
unfinished campaign, tape, and individual level selections are not saved.

## Planning a good program

Trace the robot's facing through each corner before adding movement commands.
Count travel after the parcel tile, not from the starting tile. Step through the
first turn or switch to verify it, then stop, edit and rerun from the restored
board. Repeats are most useful after Forward, but repeating a turn can express a
return journey too. With several parcels, finish one delivery before returning
for the next, and remember that return turns reverse which way Left points.

The displayed par is a tested reference-program length, not a claim of a proven
global minimum. Shorter legal solutions are welcome. Replaying the full campaign
can improve stars and the execution-step bonus.

## Implementation and validation

`model.rs` contains the pure fixed-tick rules and bounded state, `content.rs`
contains the authored rooms, and `view.rs` contains read-only rendering. Position
cells reuse `ferravex_play::navigation::Cell`. No external dependencies or assets
are added. The shared arcade host owns timing, input edge retention, menus,
letterboxing, audio cues and completed-run records.

The public API exports `Game`, `Phase`, `META`, and the generated `run()` entry.
`Game::new`, `step(&Input)`, `outcome`, `room`, `phase`, `program_len`,
`executed_steps`, `score`, `summary`, and the read-only `pilot(tick)` support
headless callers. The model module includes an executable rustdoc example.

```sh
cargo test -p tin-automata --offline --locked -j 2
cargo clippy -p tin-automata --all-targets --offline --locked -j 2 -- -D warnings
cargo run -p tin-automata --offline --locked -- --headless
rustfmt --edition 2024 --check examples/tin-automata/src/lib.rs examples/tin-automata/src/model.rs examples/tin-automata/src/content.rs examples/tin-automata/src/view.rs examples/tin-automata/src/main.rs
```

Sixteen behavioral tests cover editing and undo, hard budgets, repeat expansion,
turning, execution cadence, pause/step/stop, fault atomicity, parcel handling,
switch reset, hazards, hints and medals, pointer controls, full campaign wins,
and exact replay. The complete-campaign tests begin with `Game::new()` and enter
every solution through ordinary editor inputs; they never inject tapes, room
progress or victory state. Focused rule tests set up individual edge cases.
The headless pilot likewise enters its reference programs through the same
player controls. It completes all twelve rooms at tick 8988 with 36 stars and
24,974 points. This is simulation validation, not a human balance playtest.

The parent integration review inspected an actual native gameplay capture.
Browser/Wasm integration is parent-owned. This package does not independently
claim browser, mobile, cross-platform or audio-output validation. There is no
level editor, cloud save, multiplayer, condition instruction or nested loop;
the bounded repeat instructions provide the campaign's control-flow mechanic.
