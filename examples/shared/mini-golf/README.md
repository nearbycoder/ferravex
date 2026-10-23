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

The compact HUD shows hole, par and shots; a thin meter and five shot controls
sit below the course. Play, Practice and Card are short on-demand menus. Use the
browser Game menu for help, current bindings and settings. Fullscreen fills the
viewport with the app, without the website toolbar or a navigation rail. The
course projection and pointer mapping use the same reserved scene region.

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
Practice can start at any hole and never replaces that record. Restart clears the
current card and retains the saved best. Opening Controls/Details or losing focus
suspends this local game; the paused ball and obstacles retain their state.

## Shared rules

`mini-golf` contains `course`, `game` and `presentation` modules. The two front
ends each own an ordinary `GameSession` loop: sample input once per display frame,
advance fixed time, consume gameplay edges on executed ticks, draw the selected
view. No wall clock or renderer enters the rules module. Moving obstacles use
simulation time and freeze with the ball. Four collision substeps per 120 Hz tick
bound travel at the maximum admitted speed and prevent thin-wall tunneling.

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

## Verify

```sh
cargo test -p mini-golf -p mini-golf-2d -p mini-golf-3d --offline --locked -j 2
cargo clippy -p mini-golf -p mini-golf-2d -p mini-golf-3d --all-targets --offline --locked -j 2 -- -D warnings
```

The legal-putt regression finishes every authored hole through the same collision,
surface and capture logic as a human. Projection tests round-trip 2D/3D pointer
coordinates inside the HUD reservations. Browser playtests remain distinct from
those rules tests; physical touch, audio listening and platform coverage should
only be claimed when actually exercised.
