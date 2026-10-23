# Design for different screens

A smaller screen should change the composition, not make every label smaller.
Keep the world, HUD, menus and page navigation in their own coordinate spaces.
The browser demo hosts demonstrate several approaches; each game's README
explains its authored layout and controls.

## Fit the world, reserve the interface

Use `Viewport::responsive` for logical-pixel HUD geometry. Reserve top/bottom
strips with `HudRegions`, then fit the game world into the remaining rectangle.
`Canvas2D::try_fit` and the checked content-rectangle fitting helpers preserve
camera/input offsets. See [the engine workflows](ENGINE_WORKFLOWS.md#keep-the-hud-readable)
and [HUD and menus](UI.md) for current composition APIs.

Use the same accepted rectangle for painting, picking and touch hit testing.
Reject letterbox presses rather than transforming them into world input. In 3D,
prepare projection/picking rays with explicit viewport, screen-Y and depth
conventions. Changing projection for a narrow view must preserve physics and
camera sensitivity unless a game deliberately changes them.

## Wrap content before reducing type

- Keep essential body text at a readable authored logical-pixel size.
- Size actions for their full labels and at least a 44-pixel interaction target.
- Use `ui::flow` for wrapping actions and `ui::wrapped_text` for measured blocks.
- Use stat lists for aligned values and stacked fallback on narrow screens.
- Use content-driven cards for ready, pause and results screens.
- Inspect omissions/overflow and provide an explicit next page or scroll path.
- Keep current bindings honest: show **Unbound** or **See Controls** when needed.

Long remaps, zero values, multi-digit prices and large counters are layout inputs,
not exceptional user mistakes. Test them before fixing a menu's dimensions.
A visual card does not become accessible just because its text fits: canvas text
still needs a deliberate semantic/DOM integration for assistive technology.

## Make a short viewport navigable

The labs and some flight/3D examples retain a taller play surface so their HUD,
scene and Controls editor stay readable. Their hosts provide native page scroll
surfaces or Scene/Controls/Guide links outside the canvas. Keep these links
focusable, visible and separate from gameplay input.

The shared Controls editor needs a 300-pixel panel plus 24 pixels of vertical
insets. Reserve at least 324 logical pixels of canvas height; if the page is
shorter, provide scrolling rather than clipping the title or Done button.

A canvas may own touch gestures for steering or aiming. Leave a native browser
pan surface outside it; a tall canvas with `touch-action: none` can otherwise
trap a small page. Verify the top and bottom of the player are reachable at short
heights, including after closing a menu and returning from fullscreen.

The documentation site's **Open full player** link avoids an extra embedded
scroll surface on phones. Embedded play remains available when you choose it.
Native fullscreen and the expanded fallback have different browser behavior;
check both paths rather than inferring one from the other.

## Keep preferences and focus predictable

Player tools provide text/contrast preferences, motion, reduced flash and frame
caps. UI theme adapters and the semantic status bridge are opt-in APIs: custom
HUDs need to consume them. The browser player reports a missing/rejected bridge
rather than treating it as successfully applied.

Keep focus outlines visible. Opening Controls must block the world beneath it;
closing it must preserve an independent manual pause. Escape can cancel a
binding capture, close a panel, pause a game or leave fullscreen, depending on the
current owner. Explain the active behavior and preserve held-key releases.

[The player guide](PLAYER_GUIDE.md) covers settings, remapping, diagnostics and
storage recovery. Device input should use current action descriptors and the
visible touch-layout slots rather than assuming every possible action is shown.

## Play a useful screen matrix

| Viewport | What to check |
| --- | --- |
| 1280 × 800 | Main flow, mouse/keyboard, complete results and help |
| 768 × 1024 | Intermediate layout, wrapping, embedded play and navigation |
| 390 × 844 | Phone portrait, readable HUD, touch-control spacing |
| 320 × 568 | Minimum width, long bindings, menus and page scroll |
| 320 × 480 | Short portrait, Controls/footer access and fullscreen exit |
| 844 × 390 | Landscape, short-height content and safe-area clearance |

A browser resize changes CSS layout. It does not emulate iOS/Android, a physical
controller, native touch dispatch or mobile GPU performance. Label synthetic DOM
inputs and scripted/autoplay scenarios separately from manual browser input.

For each game, start it, perform its core action, pause/resume, open/close
Controls, retry/reset, and inspect a real result where practical. Labs have no
win condition: drop/select a body, pause, step and reset. Multiplayer menus must
keep authority progressing with neutral local commands. Replay completion needs
an explicit terminal signal.

Save the exact artifact/source identity, viewport, actions, screenshots, console
errors and failed requests. A headless test or framebuffer read is useful
supporting evidence; it is not proof of the pixels visibly presented by the
browser. Known capture-path limitations must remain visible in the report.

## Follow a working consumer

| Pattern | Example |
| --- | --- |
| Screen-space arena HUD and large actions | [Neon Swarm](../examples/neon-swarm/README.md) |
| Plot controls and full purchase explanations | [Orchard Guard](../examples/orchard-guard/README.md) |
| Narrow menus and complete interaction notices | [Crystal Cavern](../examples/crystal-cavern/README.md) |
| Tall panel with native page navigation | [Sky Courier](../examples/sky-courier/README.md) |
| Inspector plus scene/action navigation | [2D lab](../examples/physics-2d/README.md), [3D lab](../examples/physics-3d/README.md) |
| Small framebuffer with readable DOM detail | [Orbit Breaker](../examples/owned-orbit/README.md) |

Preserve each example's authored game rules and input transforms when borrowing
its presentation pattern. A reusable layout API is a starting point, not evidence
that every consumer has migrated to it.
