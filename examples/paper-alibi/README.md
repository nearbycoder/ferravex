# Paper Alibi

A complete six-case detective adventure about a local newspaper following a
paper trail through its city. Examine places, interview people, compare records,
and publish conclusions that explain **who**, **how**, and **why**.

The investigations are authored rather than randomly assembled. They have
18 suspects, 18 locations, 18 physical records, 18 witness statements, six signed
confrontations, six explanatory debriefs and a final newspaper ending. Independent
scene records establish facts; a suspect's testimony can contradict them. Every
case provides two independently supported innocent alibis and one contradiction
that identifies the remaining suspect. Later cases use physical traces and
cross-referencing in addition to recordings.

## The six dossiers

1. **The Last Carbon** — a stolen payroll ledger in a locked newspaper safe.
2. **The Silent Bell** — a museum theft behind unbroken glass.
3. **The Blue Lantern** — lighthouse sabotage and a suspicious sea passage.
4. **The Borrowed Voice** — a forged radio announcement assembled from wax.
5. **The Winter Orchard** — substituted seeds and an unjust land valuation.
6. **The Glass Mayor** — a final evidence-case swap before a public hearing.

Each case can be read at your own pace. No real-time timer punishes reading or
planning. The cases form a connected story about diverted public funds and a
council registrar's grey seal, while retaining their own suspects and solutions.

## How to investigate

The screen explains itself with icons rather than instructions. Four icon tabs
(map, people, notebook, target) show their progress as dots and checks, and the
tab that holds your next useful step pings softly.

- **1 / Places (map):** Up/Down chooses one of three places. Space examines it;
  the blacked-out lines on the dossier become its verified record and a green
  **VERIFIED** stamp lands. The case premise sits on a clipping beside the list.
- **2 / People:** Up/Down chooses a witness. Space records their statement,
  stamped **CLAIM**: a witness may lie.
- **3 / Notebook (book):** Up/Down chooses a record. A magnifier marks verified
  scene evidence, a speech bubble a witness claim and a key the signed response;
  missing records stay as dim silhouettes. Space pins or unpins a record onto
  the pinboard. Pin a claim beside the fact it breaks and red string joins the
  two cards; the Connect button breathes until you press **C**. A real
  contradiction bursts into a bolt; anything else shakes. **X** (or the Clear
  button under the list) removes the pins, never the evidence.
- The contradicted witness now carries a red bolt in **People**. Select them and
  press **C** to confront: their denial is struck through and the signed
  response explains the method and motive.
- **4 / Accuse (target):** a rail of icons shows whether every scene, interview,
  the contradiction and the signature are in. Up/Down selects WHO, HOW or WHY;
  Left/Right changes that answer. Space files the accusation once every record
  and the confrontation are collected.
- A solved case opens its newspaper edition: the who, how and why, and the
  verified fact beside the struck-out denial. Space continues to the next case.
  Finish the sixth edition to print the final newspaper.

The header tracks the case: six progress pips, the case name, three WHO / HOW /
WHY slots that light as you earn them (the liar's face appears once the
contradiction is connected) and four hearts of credibility. Pointing at a ready
Accuse button makes the heart at risk flash.

Mouse clicks select tabs and rows; hovering a tab names it. Clicking a theory
answer cycles it. The large bottom buttons perform their displayed actions.
Away from Notebook, X opens it. Held movement or held Space does not repeat
deductions: actions use press edges.

The shared host provides **Enter** to begin, **Esc/P** to pause, **H** for the
four-bullet field guide, **R** for a new six-case campaign, **M** for sound and
**F2** for control remapping. Space/Enter, X/E and C/Q share their respective
primary, secondary and third-action bindings. Key caps on each button show the
defaults.

## Credibility, scoring and replay

You begin with four credibility points, shown as hearts. A formal accusation
with the wrong suspect, method or motive breaks one heart. Four false reports
end the campaign.
An unsupported accusation cannot be filed and does not spend credibility.
Exploring an unproductive clue connection is safe; keep investigating.

New physical evidence and interviews award 25 points each, a valid contradiction
100, and its signed confrontation 75. A closed case awards 500 plus 50 for each
remaining credibility point. Printing the final edition awards 500. A spotless
campaign scores **6,650** and earns **Editor of Record**. Repeated examinations,
links, confrontations or terminal commands cannot award extra points.

Try to solve the cases without false accusations. Notebook pinning is a reasoning
aid: compare a concrete denial with an independently verified fact. The innocent
alibis matter as much as an incriminating trace. Best scores and completed-run
records stay on the current device through the shared host; unfinished
investigations are not saved.

## Run and verify

```sh
cargo run -p paper-alibi --offline --locked -j 2
cargo run -p paper-alibi --offline --locked -j 2 -- --headless
cargo test -p paper-alibi --offline --locked -j 2
cargo clippy -p paper-alibi --all-targets --offline --locked -j 2 -- -D warnings
```

The headless command completes the full campaign with ordinary player commands,
without a display. Its scripted investigator gathers all records, makes the
connection, confronts the suspect and selects the authored solution through the
same input path as a human. It wins at tick 4,248 with four credibility points
and 6,650 score. It is a reproducible solution, not an independent human test of
mystery difficulty.

For parent-owned native graphical checks, useful autoplay frames in the first
case are around 100 (places), 250 (people), 445 (two pins on the board), 470
(contradiction), 515 (signed response), 710 (accusation) and 732 (edition). The
standard runner accepts `--autoplay --smoke-frames N --screenshot PATH`;
`FERRAVEX_ARCADE_FAST=40` with about 220 frames reaches the victory card.
Demonstration runs use transient personal records.

## Implementation and limits

`model.rs` contains renderer-independent rules with fixed arrays: seven current
records, two pins, three theory answers and four view selections. Each executed
tick also keeps a bounded list of the commands it accepted or refused (a new
record, a pin, a contradiction, a wrong witness, a retracted accusation). The
list is cleared at the start of every update, ignored by equality and never read
by the rules. `content.rs` contains the original case writing. `view.rs` paints
the paper dossiers, procedural portraits, engine pictograms for every location,
the red-string pinboard, the header's WHO / HOW / WHY slots and hearts, and the
icon results card. It turns the tick's events into effects where they happened:
bursts on collected records, a stamp slam, a "Contradiction!" callout over the
pinboard, a heart breaking in the header, and small shakes with one-word popups
for refusals. The shared `ferravex-arcade` runner owns timing, input sampling,
menus, storage, audio and the effect pool. No external packages, assets,
requests or services are needed.

Twenty-three behavioral tests and one executed documentation example pass. They
cover a legal full-campaign win, credibility failure, immutable endings, one-shot
rewards, invalid actions, clue pinning, every verdict field, mouse button
semantics including the Clear button, campaign progress pips, per-tick events,
effect requests for a contradiction, a retraction and a refusal, layout fits for
every authored line, and the help word budget. Strict scoped Clippy and
formatting pass. Browser/Wasm integration, actual graphical layout, device
audio, gamepad/touch play and platform portability are parent-owned validation;
these simulation checks do not establish them.
