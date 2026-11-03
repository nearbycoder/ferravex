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
| X / E | Undo the last rotation |
| C / Q | Reset this chamber's arrangement |
| 4 | Reveal the current engineer hint |
| 5 / click Seal Chamber | Check and seal the current circuit |
| H | Open or close the field guide |
| Esc / P | Pause or resume |
| R | Start a fresh twelve-chamber campaign |
| M | Toggle sound |
| F2 | Edit controls |

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
filters and locks, receiver readouts and three small channel lights supplement
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

Each chamber has a contextual lesson and a separate optional hint. All layouts,
reference solutions, color targets and hints are authored locally. There is no
random puzzle generation or external content.

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
channel is arriving. The NEED/HAVE readout distinguishes these cases. Undo is
useful for testing a reflection without losing the prior arrangement.

## Implementation and validation

`src/content.rs` contains the twelve chamber layouts and reference dial settings.
`src/model.rs` contains pure input-driven rules, bounded tracing and the legal
pilot. `src/view.rs` draws the brass-and-glass workbench using Ferravex primitives.
No manifests, engine APIs, external crates or assets are required by this game.

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

The package's **14 behavioral tests and one executable doctest pass**. Tests
cover each chamber's legal solution from the normal sequential campaign,
complete legal-input replay, loop termination, additive colors and contamination,
filters, reflection, splitter behavior, absorbers, undo capacity, reset costs,
invalid input, held-button edges, single-award scoring and terminal invariance.
The headless legal pilot wins at tick **4,170**, with **21,000 points / 36 stars**.
It rotates devices with ordinary pointer commands, seals with key 5 and advances
with Space; it never installs the authored solution into model state. Strict
Clippy and scoped formatting checks pass.

The parent inspected the actual native frame 2,700 capture: the magenta circuit,
board labels, sidebar result and footer were readable with no overlap found.
Broader native and browser integration belongs to the parent collection check.
These automated results do not establish human puzzle difficulty, accessibility
for all players, mobile/controller play, cross-platform behavior or audible
sound-device output. See [The Twenty](../../docs/GAMES_20.md) for combined evidence.
