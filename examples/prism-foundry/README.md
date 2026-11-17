# Prism Foundry

Bring dawn back to a sleeping glass city. Twelve authored optical chambers form
one complete puzzle campaign: turn mirrors, split beams, filter white light and
combine colors at locks. Each cleared circuit restores another workshop; the
final Daybreak Engine earns a complete ending, engineering rank and recorded
score. There is no time limit. Local undo, reset and optional engineer hints make
an incorrect arrangement recoverable.

## Play

```sh
cargo run -p prism-foundry --offline --locked -j 2
```

The parent browser catalog registers `/prism-foundry/`. The shared Ferravex host
provides the title screen, paginated field guide, pause, control remapping, sound,
new campaign, ending and completed-run records. Desktop keyboard and pointer are
the intended controls. An unfinished campaign is not saved.

| Control | Action |
| --- | --- |
| Arrow keys / WASD | Move the gold cell cursor, one cell per press |
| Space / Enter | Rotate the selected device; continue after a cleared chamber |
| Left click a device | Select and rotate it immediately |
| X / E / click the undo button | Undo the last rotation |
| C / Q / click the reset button | Reset this chamber's arrangement |
| 4 / click the bulb button | Reveal the current engineer hint |
| 5 / click Seal | Check and seal the current circuit |
| H | Open or close the field guide |
| Esc / P | Pause or resume |
| R | Start a fresh twelve-chamber campaign |
| M | Toggle sound |
| F2 | Edit controls |

Each workbench button takes exactly the same rule path as its key.

## Reading the workbench

The play screen is drawn rather than written:

- **Header:** twelve pips track the campaign; the chamber's name sits beside
  them. Below, each lock appears as a token in its color, with a tick once it
  is lit. Mixed colors show their recipe once (`Y = R + G`).
- **Board:** beams glow and flow from their sources. A lock's ring is split
  into one arc per wanted channel; an arc lights when that channel arrives. A
  red ring and a cross mark a lock spoiled by an unwanted channel. Turned
  devices swing into place.
- **Previews:** hovering a device shows a turn arrow, its next setting and the
  light paths it would produce as marching dashed beams, with a tick or cross
  on every lock that would change. Hovering undo or reset previews their
  outcome; the device undo would restore is ringed.
- **Pace:** three stars dim as costs mount. The meter's gold half is the
  three-star budget (par, marked by a flag), its silver half the two-star
  budget. Hovering a command that would cost a star flashes that star red.
  Chips appear for a used hint, resets and rejected seals.
- **Locks list:** each lock's wanted channels are rings, arriving channels are
  filled, and an unwanted arriving channel is struck through.
- **Dial:** the selected device's settings in order, the current one ringed
  (filters cycle R, G, B).
- **Hint:** the bulb button reveals the engineer's note on request; the board
  cells it names gain numbered markers in the note's order.
- **The glass city:** one tower per chamber stands below the board. Sealed
  towers light in their locks' colors with one band of windows per star.

Accepted actions are felt where they happen: a ring on each turn, a burst and
sparkle when a lock lights, a pulse on the seal button when every lock
matches, red rings on unlit locks and a shake for a rejected seal, and a
"Sealed!" callout with the city tower lighting up. Refused commands (turning
stone, undo with no history) ring the thing that refused.

## How light works

The board has columns A–K and rows 1–7. Sources emit continuously. Mirrors
alternate between `/` and `\`. Splitters preserve the incoming direction **and**
create a reflected branch. Empty crossings do not steer beams; stone, sources
and locks absorb incoming light.

Diamond filters cycle **R → G → B → R**. They pass only the selected channel.
A blue filter blocks a red beam completely. Split white before filtering if two
branches need different colors.

Colors add at a lock:

| Arriving channels | Lock color |
| --- | --- |
| Red | R |
| Green | G |
| Blue | B |
| Red + green | Y, yellow |
| Red + blue | M, magenta |
| Green + blue | C, cyan |
| Red + green + blue | W, white |

A lock requires an **exact** match. Extra green spoils a magenta lock. Letters on
filters and locks, per-channel arcs and the workbench's channel slots supplement
the colored beams. All locks must match simultaneously before sealing. A wrong
seal leaves the arrangement intact and adds a rejected attempt; it never awards
points or advances the campaign.

## The twelve chambers

| # | Chamber | Main problem |
| --- | --- | --- |
| 1 | First Reflection | Two mirrors turn red light into its first lock |
| 2 | Folded Light | A folded path crosses itself without interference |
| 3 | The Long Way | Route around stone and ignore a decoy mirror |
| 4 | Twin Loom | Split one source into two independently routed branches |
| 5 | Cross Stitch | Chain splitters to serve three locks |
| 6 | Choose A Color | Filter separate white sources to red and blue |
| 7 | Chromatic Fork | Split white before filtering two different branches |
| 8 | Golden Junction | Recombine red and green at a yellow lock |
| 9 | Violet Crossing | Build a magenta lock without green contamination |
| 10 | Three-Color Press | Separate and recombine all three channels into white |
| 11 | Signal And Noise | Cross two circuits while keeping exact targets clean |
| 12 | The Daybreak Engine | Run a three-source circuit powering three final locks |

Each chamber has a contextual lesson (published to status readers) and a
separate optional hint. All layouts, reference solutions, color targets and
hints are authored locally. There is no random puzzle generation or external
content.

## Stars, undo and replay

Each sealed chamber awards 1,000 points plus 250 per star. The maximum campaign
score is 21,000 points and 36 stars. Score is awarded once per chamber.

- **Three stars:** no hints, resets or rejected seals, and rotations at or below
  the authored solution's par.
- **Two stars:** rotations at most twice par and at most two resets. Revealing a
  hint or rejecting a seal removes the three-star rating.
- **One star:** any other completed solution.

Par describes a verified reference solution, not a claim of mathematical
minimality. An alternative circuit is valid whenever every lock matches.

Undo restores the preceding dial setting and subtracts that turn. The latest
128 rotations are retained; undo cannot reach earlier discarded history. Reset
restores all starting dials and clears the current turn count and undo history.
Hints, reset counts and rejected seals survive a local reset. Cleared chambers
cannot be reset or scored again. R starts the whole campaign over, including
stars and score. The final rank is Master of Light for 36 stars, Prism Engineer
for 24–35, or Foundry Keeper below 24.

A useful strategy is to follow one beam from the source and fix each bend in
order. Solve separate colors before recombining them. A white lock may be dark
because one branch is missing; a colored lock may be wrong because an unwanted
channel is arriving. The channel slots distinguish these cases: an empty ring
is missing, a struck-through dot is spoiling. Hover previews and undo are
useful for testing a reflection without losing the prior arrangement.

## Implementation and validation

`src/content.rs` contains the twelve chamber layouts and reference dial settings.
`src/model.rs` contains pure input-driven rules, bounded tracing and the legal
pilot. It also records a bounded per-tick event list (turns, lit locks,
rejections, seals and refusals) that rules never read; `src/view.rs` turns
those events into effects and draws the brass-and-glass workbench, header
objective and results card with Ferravex primitives, icons and juice. Idle
motion uses the host's presentation clock; turn animations use the model's tick
counter. Previews trace a copy of the dials and never mutate the model. No
manifests, engine APIs, external crates or assets are required by this game.

Tracing deduplicates each directed color channel before queue admission. An
11×7 board has at most **924 directed color states**; a closed mirror loop or a
splitter network cannot grow the queue without bound. Dial state, receivers,
beam edges and undo history use fixed arrays. Re-tracing happens only when a dial
arrangement changes. Rendering does not mutate the model.

```sh
cargo test -p prism-foundry --offline --locked -j 2
cargo clippy -p prism-foundry --all-targets --offline --locked -j 2 -- -D warnings
cargo run -p prism-foundry --offline --locked -j 2 -- --headless
```

The package's **21 behavioral tests and one executable doctest pass**. Tests
cover each chamber's legal solution from the normal sequential campaign,
complete legal-input replay, loop termination, additive colors and contamination,
filters, reflection, splitter behavior, absorbers, undo capacity, reset costs,
invalid input, held-button edges, single-award scoring and terminal invariance,
plus header progress, effects for turns, lit locks, seals and refusals, button
clicks matching their keys, preview accuracy, hint-cell parsing and the help
word budget. The headless legal pilot wins at tick **4,170**, with
**21,000 points / 36 stars**. It rotates devices with ordinary pointer commands,
seals with key 5 and advances with Space; it never installs the authored
solution into model state. Strict Clippy and scoped formatting checks pass.

Native smoke captures of the title, chambers 1, 2, 3, 7, 10 and 12, hover
previews, the hint note and the results card were reviewed for overlap and
legibility. Broader native and browser integration belongs to the parent
collection check.
These automated results do not establish human puzzle difficulty, accessibility
for all players, mobile/controller play, cross-platform behavior or audible
sound-device output. See [The Twenty](../../docs/GAMES_20.md) for combined evidence.
