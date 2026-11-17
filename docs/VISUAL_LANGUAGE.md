# Visual language: show, don't tell

Ferravex games explain themselves through what the player sees and feels, not
through sentences. A new player should understand what to do from shapes,
color, motion and a handful of words. This guide is the standard for every
game in the catalog and for new games built from the templates.

## Rules

1. **Icons, not labels.** Every resource, stat and count is an icon plus a
   number: a heart and `12`, a coin and `158`, a clock and `1:41`. Drop words
   such as "Health", "Coins", "Energy", "Time left", "Score".
2. **Meters, pips and dials, not fractions in prose.** Progress through a
   campaign uses header pips (`ArcadeGame::progress`), resources use bars or
   ring meters, steps use rows of dots. "Day 7 of 14" becomes seven lit pips.
3. **State is color, shape and motion.** Selected things lift and glow;
   available things gently pulse or bob; disabled things dim; danger flashes
   red; success sparkles. If a state needs a sentence to be noticed, give it a
   stronger visual.
4. **Feedback is felt where it happens.** Every accepted action produces an
   effect at its location: a burst where the crop was harvested, a coin popup
   rising from the shop, a ring where the signal switched, shake when the hull
   is hit. Refused actions get a short shake, a red flash on the thing that
   refused, and at most a one- or two-word popup ("No fuel") or the missing
   resource's icon with a cross.
5. **No permanent logs.** Status lines that narrate the last action ("Planted
   bean. Water it before bed.") become transient popups or a toast. If the
   information matters later, it belongs in the scene as visible state (a wet
   tile, a growth ring), not in a sentence.
6. **Teach by affordance and preview.** Show what can be clicked (hover lift,
   cursor ring, glow), and preview consequences before committing: ghosted
   outcomes, `+3`/`-2` chips with resource icons, highlighted target tiles and
   predicted paths. A preview replaces an explanation.
7. **A strict word budget during play.** Allowed: proper names (a place, a
   character, a card), numbers, one-to-three-word headlines and popups, and a
   key cap next to the control it activates. Not allowed: instructions,
   tutorials, rule reminders and flavor paragraphs on the play screen.
   Story and flavor survive only where they are the game (a detective's
   evidence, a petition's question) and even there are cut to the essential
   line.
8. **Short help.** The guide (H) is for reference, not required reading. The
   host already draws the controls as key caps on page one; the remaining
   pages are at most four short bullets in total, each under fifteen words.
9. **Idle life.** Use the presentation clock for gentle motion: water ripples,
   flickering flames, bobbing markers, breathing selection rings. A still
   screen reads as a menu; a living one reads as a game.
10. **Consistent iconography.** Use `ferravex_render::icons` (92 pictograms)
    before drawing a bespoke symbol, and use the same icon for the same idea
    everywhere in a game.

## Toolkit

Engine (`ferravex_render`, also reexported by `ferravex_arcade`):

- `icons::{draw, draw_styled, badge, Icon, IconStyle}`: two-tone vector
  pictograms (heart, coin, clock, bolt, shield, gem, crate, ship, train,
  sprout, drop, sun, rain, flame, key, lock, eye, gear, trophy, …). See the
  specimen with `cargo run -p ferravex-render --example icon_sheet`.
- `juice::Juice`: bounded presentation effects advanced by frame time:
  `burst`, `sparkle`, `confetti`, `ring`, `popup`, `popup_icon`, `callout`,
  `toast`, `shake`, `flash`, `vignette`. `juice::{pulse, bob}` give idle
  motion curves.
- `shapes::{fill_polygon, fill_triangle, fill_sector}` join the existing
  anti-aliased rounded rectangles, circles, capsules, arcs and glows.

Arcade host (`ferravex_arcade`, used by the twenty-game collection):

- Paint helpers: `icon`, `icon_value` (icon + number), `icon_badge`, `pips`,
  `ring_meter`, plus the existing `card`, `chip`, `meter`, `glow`, `keycap`.
- `clock()` is the presentation clock for idle motion; `pointer()` is the
  current pointer for hover highlights in `draw`. Neither may decide rules.
- Optional `ArcadeGame` hooks:
  - `progress() -> Option<(current, total)>` draws header pips.
  - `effects(cue, &mut Juice)` requests effects after each executed tick.
  - `draw_objective(area) -> bool` draws the objective visually in the header.
  - `draw_result(area) -> bool` draws the ending's key results as icons.
- The host already provides: a live bot demo behind the title, icon buttons
  for pause/help/controls/sound, a rolling star score with rising `+N`
  gains, sparkles on good cues, shake and a red vignette on bad cues,
  confetti on victory and a count-up results card.

## Review commands

```sh
cargo run -p mosslight -- --smoke-frames 240 --screenshot title.png          # title + demo
cargo run -p mosslight -- --autoplay --smoke-frames 600 --screenshot play.png
FERRAVEX_ARCADE_VIEW=help cargo run -p mosslight -- --autoplay --smoke-frames 120 --screenshot help.png
FERRAVEX_ARCADE_FAST=40 cargo run -p mosslight -- --autoplay --smoke-frames 900 --screenshot end.png
```

`FERRAVEX_ARCADE_VIEW` accepts `help`, `help:N` and `paused`;
`FERRAVEX_ARCADE_FAST` multiplies autoplay simulation speed, and
`FERRAVEX_ARCADE_POINTER=x,y` pins the pointer in logical units to capture
hover states (all native only).
Screenshots are presentation checks, not playtests.
