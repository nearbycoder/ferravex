# The browser player guide

Every game on this site shares the same player: one menu for controls, settings,
sound and help, plus tools for screenshots, performance and accessibility. Browse
the [games](../examples/catalog.json), or start with the
[Copper Greens course guide](../examples/shared/mini-golf/README.md).

## Quick reference

| I want to... | Do this |
| --- | --- |
| Start playing | Click the game, or Tab to it, so it has keyboard focus. |
| Change a key or button | **Game menu → Controls** |
| Play fullscreen | **Game menu → Fullscreen**; **Game menu → Exit fullscreen** returns to the page. |
| Reduce motion or flashes | **Game menu → Settings → Player tools** |
| Use larger text or higher contrast | **Game menu → Settings → Player tools** |
| Save a screenshot or share a link | **Save screenshot** or **Copy game link** in Player tools |
| Fix a game that will not load | Press **Retry game**, then use **Copy support details** if it still fails. |

These tools live in the browser player. They never change a game's rules, and
native desktop builds keep their own documented behavior.

## The game menu

The small **Game menu** button opens Controls, Settings, Sound and How to play.
Help and diagnostics wait until you ask for them, so the playfield stays clear
apart from scores, objectives and any touch controls you need. Ready and pause
screens show only their main action and your results. Mechanics and display
notes live in **How to play**. The same single menu appears when a game is
embedded in a page, so Fullscreen is always in one place.

**Fullscreen** gives the game the entire browser viewport, leaving site navigation
and guides outside. The menu button stays in the top corner. If your browser
denies native fullscreen, the game expands to fill the window instead, and some
browser chrome may remain visible. Orbit Breaker's tall playfield stays centered
without stretching its physics or pointer coordinates.

While fullscreen, Shift+Tab from the game reaches the game menu rather than hidden
page controls. The Controls editor keeps its own Tab navigation. Opening the
menu stops local gameplay input; online matches and replays keep running by
their own rules. Closing the menu keeps any pause you chose yourself.

## Controls and focus

Every canvas has a keyboard focus target, accessible name and linked instructions. Click or Tab to the canvas to play. Use **Controls** to view and remap keyboard, mouse, touch or gamepad bindings. Long action names and bindings wrap onto readable pages; use the page arrows to see the full set. An Unbound label means that action has no binding. The Game guide in Details loads the catalog description and default controls on demand; current custom bindings are listed in Controls.

Ctrl, Command and Alt shortcuts remain available to your browser. IME composition does not trigger gameplay actions. Page navigation, capture cancellation and focus changes clear held input. Controllers stop sampling while unfocused and resynchronize on return. These changes apply to all three browser adapters, including Courtyard and Orbit Breaker.

## Screenshots and sharing

**Save screenshot** captures a PNG immediately after the next rendered frame. WebGL buffers are read before the browser discards them. The capture contains the game canvas, including its rendered overlays; browser dialogs and player controls outside the canvas are not included. Closing Details first is optional. Local games pause while settings are open; Strike replay and connected Strike/Courtyard sessions keep progressing with local gameplay input suppressed. Use the replay panel's pause control to pause a recording.

**Copy game link** includes the active palette and visual quality. It removes autoplay, development tools, URL fragments and multiplayer addresses. Resolution, player preferences and game progress are not embedded in links. If clipboard access is blocked, a selectable link appears.

## Performance and power

The **Performance overlay** shows rendered FPS and framebuffer size. GPU hosts also report draw counts and vertices when available. Measurements describe the current device and scene; they are not a cross-device benchmark.

**Frame rate limit** offers display refresh, 60 FPS and 30 FPS. It limits rendering and host submission; existing fixed clocks still determine simulation timing. The first frame after a tab becomes visible resets its delta clock. Hidden pages skip rendering and simulation submission. Courtyard peer updates are also suspended in hidden tabs, so a background browser tab is not an always-on multiplayer server.

**Quick performance preset** combines lighting, resolution and frame limits:

| Preset | Lighting | Resolution | Frame limit |
| --- | --- | --- | --- |
| Battery saver | Low | 0.75× | 30 FPS |
| Balanced | Medium | 1× | 60 FPS |
| Sharp | High | Device density, capped by framebuffer limits | Display refresh |

Only settings supported by a game affect its rendering. 2D artwork retains authored lighting. Orbit Breaker keeps its fixed CPU framebuffer resolution. Per-setting controls above Player tools allow further tuning. Existing quality/look link overrides remain session overrides and may supersede saved presentation choices.

**Keep screen awake** requests the browser screen wake lock. It requires browser support and an allowed secure context; the browser may deny or revoke it. Hidden pages release it and visible pages attempt to restore the request. It is never enabled automatically or saved as a preference.

## Accessibility options

**Player contrast** adds high contrast backgrounds, borders, text and focus rings to browser controls. **Player text size** enlarges the Details panel and its controls. The current engine also exposes opt-in game-UI preferences and a theme adapter. A demo must adopt those APIs to change custom in-canvas text; stored preferences alone do not resize arbitrary game art.

**Game and player UI motion** follows the system preference by default and offers reduced or standard motion. Reduced motion suppresses supported cosmetic camera and effect motion as well as browser control transitions. **Reduce game flashes** independently reduces supported flashes. Both settings apply live, including while paused. Essential movement, game rules, audio and haptics retain their behavior. The [accessible gameplay guide](JOURNEY_200_ACCESSIBILITY.md) describes each demo's consumers and remaining verification limits.

## Session and connectivity

**Active session** counts time while the page is focused and visible with settings closed. It is a session timer, not a speedrun or authoritative game timer. Reset session timer starts it over. It does not persist across reloads.

The connection status explains when the browser reports offline. A loaded single-player game can keep running. Loading a new game, fetching its guide and multiplayer can require a connection. This release does not install a service worker or promise offline reloads.

## Saved preferences and backups

Overlay, frame cap, contrast, text, game/UI motion and reduced-flash preferences are stored per game under `ferravex.player.v1.<game-id>`. Existing Rust presentation preferences, custom input profiles and game saves use separate storage. Storage denial applies choices to the current session and reports the failure.

**Export player settings** downloads version 2 JSON. **Import player settings** accepts version 1 or 2 backups of at most 4 KiB, validates their fields, and previews changes before **Apply imported settings** or **Cancel import**. Missing settings retain their current values; version 1 does not contain the reduced-flash setting. Applying saves the complete preferences in one write. The backup can be transferred between games or browsers; it excludes saved progress, multiplayer credentials and input remaps.

**Reset player settings** restores the six host preferences. Use Reset details for lighting/resolution/style. Neither action erases game progress or custom bindings. Screen wake lock and session timing are session controls and are not part of these backups.

## Loading and troubleshooting

All games compile downloaded Wasm as a stream when supported. If MIME policy prevents that, the loader uses bytes from the same response. It does not download the game twice. Loading waits are bounded to 60 seconds, including stalled response bodies. Startup failures display **Retry game**, which reloads the game page.

**Copy support details** copies game ID, viewport, framebuffer, frame counters and player settings. It excludes full URLs, peer addresses and saved progress. **Download diagnostics JSON** provides a bounded runtime report without clipboard access; **Export playtest CSV** provides engine diagnostics. Share the artifact that is relevant to the problem.

## For developers

The shared player is [player.js](../web/player.js), started by
[details.js](../web/details.js). Input handling lives in [input.js](../web/input.js)
and startup recovery in [loader.js](../web/loader.js). The shared renderer,
Courtyard and Orbit Breaker all call the same player frame hooks after drawing.

The [player release catalog](../website/releases/player30.json) lists how these
tools were introduced. [Tour the engine](ENGINE_WORKFLOWS.md) covers the matching
engine APIs, and the [release scope](STOPPING_POINT.md) separates shipped work
from plans. See the [validation guide](GAME_TESTS.md) for native, WebAssembly and
browser checks, and [Platforms](PLATFORMS.md) for platform support.
