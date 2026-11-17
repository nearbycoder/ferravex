# Voltage Choir

A complete four-lane rhythm concert about bringing a silent city back to life.
Play six original authored arrangements, restore six districts and reach the
citywide finale. A clean full concert takes about four minutes, plus the time
you spend on song cards and results. The melodies, charts, rules and procedural
concert art are first-party source; there are no downloaded songs or assets.

## Your concert

| Song | District | Tempo | Bars | Notes | Accuracy target |
| --- | --- | ---: | ---: | ---: | ---: |
| First Spark | Rooftop Workshop | 100 BPM | 12 | 44 | 60% |
| Copper Steps | Midnight Tram | 120 BPM | 16 | 80 | 62% |
| Glass Rain | Winter Garden | 120 BPM | 20 | 96 | 65% |
| Night Circuit | Electric Arcade | 150 BPM | 20 | 146 | 65% |
| Aurora Current | Northlight Tower | 150 BPM | 24 | 162 | 68% |
| Voltage Choir | The Whole City | 150 BPM | 32 | 232 | 70% |

The **760 notes** progress from steady single beats through offbeats, rests,
staircases and simultaneous chords. Every song has its own authored motifs,
arrangement, melody, bass progression, introduction and lesson. The soundtrack
uses tick-synchronous lead, bass and harmony notes through the shared host's
bounded procedural music voices. It continues independently of whether you hit
or miss the chart.

## Controls

- **Enter** starts the host campaign. **Space/Enter** or the card's button starts
  each song's four-beat count-in and advances song results.
- The lanes, from left to right, are **Left, Down, Up, Right**. Use those arrow
  keys or **A, S, W, D**. Tap when a note's center reaches the bright line.
- Linked notes are a chord: press both direction keys together. Held keys do not
  trigger later notes. Keys **1–4** or clicking a lane (or its key) can play
  single notes; direction keys are the recommended controls because they accept
  simultaneous chords. The shared numeric-slot input represents one slot per tick.
- **X/E**, or clicking the full Encore meter, activates Encore. On the
  failed-song card, **X/E** or the **End** button ends the concert.
- **C/Q** spends one retry ticket to restart the current song. On the failed-song
  card, **Space/Enter**, **C/Q**, or the **Retry** button starts another take.
- **Esc/P** pauses, **H** opens the four-line field guide, **F2** opens control
  remapping, **M** toggles sound, and **R** starts a fresh six-song concert.

The chart stops during the shared pause, help and controls screens. The game is
intended for desktop keyboard play; pointer input is supplementary. Pointer clicks
use the same rule paths as their keys, and hovering a lane, the full Encore meter
or a card button highlights it.

## Reading the stage

The play screen speaks in icons, meters and motion rather than sentences.

- **Header:** six pips track the concert's songs beside the current song's name.
  The objective row shows the gold accuracy goal and a map of the whole chart in
  four lane rows: upcoming notes glow faintly, hits light up, misses turn red,
  Encore leaves a gold band and a playhead marks the song position.
- **Left panel:** the crowd (people icon and energy) with a vertical meter, its
  red danger line and twelve fans who light up, bounce on the beat, raise their
  arms when roaring and slump red when fading. Below it, a flame and the combo
  count, which pops on each hit, the score multiplier chip and its progress to
  the next step, then six district bulbs that light as songs are cleared.
- **Right panel:** the song and district names, a progress bar, four beat lamps,
  tempo and time left; an accuracy ring with a gold goal tick, judgment tallies
  (gem Perfect, check Good, dash Okay, cross Miss, hand empty tap) and an
  early/late gauge; the Encore bolt meter with its X key cap (or a shield while
  active); and the three retry tickets beside the C key cap.
- **Highway:** each lane key shows its arrow and letter, sinks when struck and
  lifts under the pointer. Judgments pop as one word (PERFECT, GOOD, OK, MISS)
  with a beam, ring and burst in the lane color. A miss flashes its lane red, an
  empty tap crosses its socket, and the crowd's loss rises from the crowd panel.
  Combo steps, a full meter, Encore, retries and restored districts each get
  their own burst, ring or callout.
- **Song cards** show the tempo, length and goal as chips, a short lesson
  headline and a live demo of one bar of the song's own chart at its real tempo,
  with lesson hints that light in time with it: the arrow keys, a chord's two
  keys, no taps in the rests, the Encore meter and X, or the city's six bulbs.
  The results card shows the grade, accuracy, best combo, song score, judgment
  tallies and the newly lit bulb; the failed-take card turns the reason red
  (crowd or accuracy) and previews the ticket and points a retry costs. The
  concert's ending card shows the overall grade, districts lit, average
  accuracy, best combo and tickets left.

## Judgments, scoring and retries

The model runs at 60 fixed ticks per second. A note is **Perfect** within two
ticks of its target (about 33 ms), **Good** within four (67 ms), and **Okay**
within seven (117 ms). The boundaries include both early and late input. A note
more than seven ticks late is missed. If two same-lane note windows overlap,
the closest pending note wins, with earlier notes winning a tie. One edge can
judge at most one note in its lane.

Perfect/Good/Okay notes give 100/75/45 base points. The combo raises the multiplier
at every 20 consecutive hits, up to x5. A completed song also awards twice its
accuracy in thousandths. Accuracy weights Perfect/Good/Okay/Miss as
1000/850/600/0; empty taps do not earn notes or points, cost two crowd energy and
break the combo. A missed note costs ten crowd energy and breaks the combo.
Perfect and Good hits restore one energy, capped at 100. Each song begins at 80.

Hits fill **Encore** (4 charge for Perfect, 3 for Good, 1 for Okay). At 100, X
spends the charge for eight beats of protected miss/empty-tap damage and 25
additional points per hit. Misses still lower accuracy and break the combo.
Encore is a tactical safety net, not a way to skip the chart's accuracy target.

You must finish each song above zero crowd energy and meet its accuracy target.
There are **three retry tickets for the whole concert**. A retry clears that
song's judgments, energy, combo, charge and attempt points; completed-song points
remain banked. Failing without a ticket ends the concert. Complete the final
track and take the final bow for the citywide ending, aggregate accuracy and
personal-record score. Song results show S+/S/A/B/C/D grades and timing counts.

Watch the lane ahead of the line, especially before a chord. Keep your hands
quiet during Glass Rain's rests. Save an Encore for a dense passage instead of
spending it immediately. If your audio device adds latency, the visible judgment
line remains the authoritative timing reference; no device latency calibration
or audible synchronization quality is claimed.

## Run and check

From the repository root:

```sh
cargo run -p voltage-choir --offline --locked -j 2
cargo run -p voltage-choir --offline --locked -j 2 -- --headless
cargo test -p voltage-choir --offline --locked -j 2
cargo clippy -p voltage-choir --all-targets --offline --locked -j 2 -- -D warnings
```

The native renderer needs a graphics display. Parent-owned catalog and browser
build integration use the same model through `ferravex-arcade`. Best scores and
completed-run records use its existing local storage; an unfinished concert is
not saved. Autoplay calls the same public `step(Input)` path and does not modify
health, timing windows, judgments or score behind the rules.

`--headless` legally completes all six songs at tick 14880, scoring 287675 with
all three retry tickets remaining. Track clear ticks are 1945, 4141, 6841, 9025,
11569 and 14857. Suggested graphical smoke captures are frame 420 or 900 for
First Spark, frame 2600 for Copper Steps, and frame 13200 for the finale:

```sh
cargo run -p voltage-choir --offline --locked -j 2 -- --autoplay --smoke-frames 900 --screenshot target/voltage-choir.png
```

The bounded model uses at most 768 note slots, six result slots, four musical
note-ons and sixteen presentation events per tick. The events record what each
tick accepted (hits, misses, empty taps, combo steps, Encore, retries, clears)
so effects appear where they happened; rules never read them. Tests cover
complete legal concert victory, exact timing boundaries, chords, nearest-note
selection, input aliases/holds, missed-note failure, finite retries, Encore
costs/expiry, accuracy failure, result accounting, pointer bounds, the Encore
meter and End button clicks, header progress, effect emission, note-on lifetime,
terminal invariance and repeatability. The rustdoc example executes the pure
model without a window. Compilation and
headless solutions are not graphical playtests or evidence of physical keyboard,
audio-device, mobile, gamepad or cross-platform behavior; parent records those
integration checks separately.
