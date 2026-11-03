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

- **1 / Locations:** Up/Down chooses one of three places. Space examines it and
  adds its verified record to the notebook.
- **2 / People:** Up/Down chooses a witness. Space records their statement.
- **3 / Notebook:** Up/Down chooses a record. Space pins or unpins it. Pin two
  directly contradictory records and press **C** to connect them. **X** clears
  the pins; it never deletes collected evidence.
- Return to **People**, select the witness whose statement you contradicted,
  and press **C** to confront them. Read the signed response there or as **C1**
  in the notebook; it explains the method and motive.
- **4 / Theory:** Up/Down selects WHO, HOW or WHY. Left/Right changes that answer.
  Space files the complete accusation. All three scene records, all three
  interviews and the signed confrontation are required before filing.
- A solved case opens an explanatory edition. Space continues to the next case.
  Finish the sixth edition to print the final newspaper.

Mouse clicks select tabs and rows. Clicking a theory answer cycles it. The large
bottom buttons perform their displayed actions. Away from Notebook, X opens it.
Held movement or held Space does not repeat deductions: actions use press edges.

The shared host provides **Enter** to begin, **Esc/P** to pause, **H** for the
paginated field guide, **R** for a new six-case campaign, **M** for sound and
**F2** for control remapping. Space/Enter, X/E and C/Q share their respective
primary, secondary and third-action bindings. On-screen hints show the defaults.

## Credibility, scoring and replay

You begin with four credibility points. A formal accusation with the wrong
suspect, method or motive spends one point. Four false reports end the campaign.
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

For parent-owned native graphical checks, useful frames are around 120 (scene
record), 400 (notebook), 600 (theory), and 4,200 (final case). The standard runner
accepts `--autoplay --smoke-frames N --screenshot PATH`. Demonstration runs use
transient personal records.

## Implementation and limits

`model.rs` contains renderer-independent rules with fixed arrays: seven current
records, two pins, three theory answers and four view selections. `content.rs`
contains the original case writing. `view.rs` paints the paper dossiers, original
procedural portraits, location emblems and red-string notebook. The shared
`ferravex-arcade` runner owns timing, input sampling, menus, storage and audio.
No external packages, assets, requests or services are needed.

Thirteen behavioral tests and one executed documentation example pass, including
a legal full-campaign win, credibility failure, immutable endings, one-shot
rewards, invalid actions, clue pinning, every verdict field and mouse button
semantics. Strict scoped Clippy and formatting pass. Browser/Wasm integration,
actual graphical layout, device audio, gamepad/touch play and platform portability
are parent-owned validation; these simulation checks do not establish them.
