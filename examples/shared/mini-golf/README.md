# Copper Greens

Twelve miniature-golf holes in two playable variants: **Copper Greens 2D** and
**Copper Greens 3D**. Both use the same course, ball simulation and scorecard.
The 3D version renders real extruded course geometry with an isometric camera;
the rules use a planar rolling-ball model with a shared raised-green heightfield.

## Play

```sh
cargo run --release -p mini-golf-2d
cargo run --release -p mini-golf-3d
```

Drag from the ball toward the target and release to putt. Distance controls power;
a tap changes aim without shooting. You can instead use the on-screen angle and
power buttons followed by PUTT. Keyboard defaults are arrows or WASD to aim and
change power, Space to putt, P/Escape to pause, C for the scorecard, and R to return
to the previous safe lie for one penalty stroke. Game menu → Controls (or F2) shows saved bindings and supports keyboard/controller
remapping. In fullscreen, Shift+Tab reaches the game menu.

The score chip shows the hole, par, strokes and a colored round-to-par badge;
tap it for the full card. Each hole opens with a short title card (name, par,
the hole's read and your best score there) that fades as soon as you putt. The
aim line grows and warms from mint to copper with power and ends in a target
ring; the putter pulls back further for harder shots. A labelled power meter
and five shot controls sit below the course, with a control hint that fades
after your first three putts. Fullscreen fills the viewport with the app,
without the website toolbar or a navigation rail. The course projection and
pointer mapping use the same reserved scene region.

## Feel and personality

Every celebration comes from an accepted rules event:

- **Every hole gets a name and a quip**: hole in one, eagle, birdie, par,
  bogey, double, triple, then a cheeky "+4 AND COUNTING". Quips rotate and
  never repeat back to back.
- **Hole in one**: confetti fountains from the cup, a ring shockwave, a short
  screen kick, a soft gold flash and a big headline.
- **The cup drop**: the ball sinks into the cup and the flag waves; birdies
  and better throw confetti, pars a polite puff.
- **So close**: a putt that rattles over the cup too fast gets a lip-out
  callout; a ball that stops within a putter's length hears about it too.
- **Bank and trick shots**: a holed putt that kissed one or two walls is a bank
  shot, three or more is a trick shot. Most holes hide a trick-shot ace — try
  putting *away* from the first flag.
- **Secret**: let a moving copper barrier help a putt drop. The first time per
  round it is announced; the results card counts how many you found.
- Splashes, sand, hard rail hits ("CLANG!" on copper), streaks of par or
  better ("ON A ROLL ×2") and returning the ball each have their own callout.

Between holes, a sheet slides up while the course re-fits above it, so the cup
drop stays visible. It shows the score name and quip, trick/assist/streak tags,
whether you set a new hole best, a twelve-cell scorecard strip colored by result
and NEXT HOLE. The round ends on a results card with a rank title (from
"Duck Pond Patron" to "Copper Deity"), your total and to-par, a personal-best
comparison, the strokes to the next title and round stats.

Reduced motion (shared player preference) removes particles, rings, the
rolling trail, shake, squash/stretch, the flag wave and every bounce or slide;
text callouts stay readable. Reduced flash removes the ace flash. Effects live
in course coordinates, so both views draw the same moments.

## Twelve greens

| Hole | Name | Par | Read the course |
| --- | --- | --- | --- |
| 1 | First Light | 2 | Learn aim, distance and power on an open green. |
| 2 | The Bank | 3 | Pass around the lower end of a stone wall. |
| 3 | Sand & Stone | 3 | Bunkers slow a ball much faster than grass. |
| 4 | Canal Crossing | 3 | Cross between two pools on a narrow bridge. |
| 5 | Split Decision | 3 | Choose a route around a central stone island. |
| 6 | The Windmill | 3 | Time a rotating barrier or go around it. |
| 7 | Switchback | 4 | Follow alternating openings with controlled shots. |
| 8 | High Road | 3 | Carry a raised green with slope acceleration. |
| 9 | Clockwork | 4 | Read the moving copper gates. |
| 10 | Island Links | 4 | Follow the dry ribbon between water hazards. |
| 11 | Garden Spiral | 4 | Enter the walled inner green through its opening. |
| 12 | Copper Cup | 4 | Combine bridge placement, sand and a moving gate. |

Water adds one penalty and returns to the last safe lie. The cup captures a ball
only within its radius and below its speed limit. The card and pause menu offer
**Take 12**, conceding the hole at its stroke limit. At twelve strokes, a hole is
automatically picked up and marked on the card. Completing all twelve holes records the full-round best locally when storage is available.
Each holed (not picked-up) hole also records a per-hole best, shown on its title
card; the demonstration never records anything.
Practice can start at any hole and never replaces that record. Restart clears the
current card and retains the saved best. Opening Controls/Details or losing focus
suspends this local game; the paused ball and obstacles retain their state.

## Shared rules

`mini-golf` contains `course`, `game`, `card`, `quips`, `juice` and
`presentation` modules. The two front ends each own an ordinary `GameSession`
loop: sample input once per display frame, advance fixed time, consume gameplay
edges on executed ticks, draw the selected view. No wall clock or renderer
enters the rules module. Moving obstacles use simulation time and freeze with
the ball. Four collision substeps per 120 Hz tick bound travel at the maximum
admitted speed and prevent thin-wall tunneling.

Rules ticks report observational events (rail/wall impact strength, copper
contact, lip-out, sand entry) and per-stroke bank/assist counters; they never
change the ball. `card` turns finished holes into score names, streaks, round
stats, rank titles and personal-best lines; `quips` picks copy as a pure
function of counters. While the ball rolls, effects age with accepted ticks;
after a hole ends (rules frozen) the celebration ages with clamped render time.

```rust,check
use mini_golf::game::{Round, Phase};
use ferravex_math::Vec2;
let mut round = Round::new();
assert!(round.shoot(Vec2::X, 0.56));
assert!(!round.shoot(Vec2::X, 0.8)); // Already rolling; no second stroke.
for _ in 0..2000 {
    round.tick();
    if round.phase != Phase::Rolling { break; }
}
assert!(matches!(round.phase, Phase::Aiming | Phase::Holed));
```

The optional `?autoplay=1` browser demonstration (native `--autoplay`) predicts
bounded candidate putts toward authored route points and then executes a legal
shot through the real rules. It does not teleport the ball or assign scores.
The read-only `golf_status_v1` / `golf_status_v1_len` exports expose versioned
UTF-8 JSON for diagnostics: hole, phase, strokes, total, power, position and card.
Read the pointer and length together, then copy before advancing another frame.
There are no exported game-state mutation or test-win functions.

Native debug builds read optional screenshot hooks (`GOLF_WINDOW=390x700`,
`GOLF_HOLE=9`, `GOLF_SHOT=DEG,POWER`, `GOLF_SKIP_WELCOME`, `GOLF_PAUSE_AT`,
`GOLF_CARD_AT`, `GOLF_TURBO`, `GOLF_FOCUS`, `GOLF_FAKE_BEST`,
`GOLF_PLAYER_CARD`); see `src/capture.rs`. Release and Wasm builds ignore them.

## Verify

```sh
cargo test -p mini-golf -p mini-golf-2d -p mini-golf-3d --offline --locked -j 2
cargo clippy -p mini-golf -p mini-golf-2d -p mini-golf-3d --all-targets --offline --locked -j 2 -- -D warnings
```

The legal-putt regression finishes every authored hole through the same collision,
surface and capture logic as a human. Further tests cover lip-outs, bank
counting, the copper assist, sand entry, score names, streaks, ranks, quip
rotation, reduced-motion clearing and the browser demonstration frame budget. Projection tests round-trip 2D/3D pointer
coordinates inside the HUD reservations. Browser playtests remain distinct from
those rules tests; physical touch, audio listening and platform coverage should
only be claimed when actually exercised.
