# The browser player guide

All seventeen games and labs share browser tools for controls, presentation and recovery. Open **Game menu → Settings**, then scroll to **Player tools**. These tools run in the browser adapter; native hosts and simulation rules retain their documented behavior. See the [game catalog](../examples/catalog.json) and [Copper Greens guide](../examples/shared/mini-golf/README.md).

## A game-first screen

The small **Game menu** button opens Controls, Settings, Sound and How to play.
Instructions and diagnostics are available on demand instead of filling the
playfield. Games retain their scores, objectives and actionable touch controls.

**Fullscreen** gives the app the entire browser viewport: website navigation,
guides and scroll rails stay outside it. The menu button remains in the top
corner; **Game menu → Exit fullscreen** returns to the page. Browsers that deny
native fullscreen use the same app layout in an expanded player; browser chrome
may remain visible. Orbit Breaker's portrait playfield is centered without
stretching its physics or pointer coordinates.

While fullscreen, Shift+Tab from the canvas reaches the game menu instead of hidden
page controls. The Controls editor retains its own internal Tab navigation.
Opening the game menu suppresses local gameplay input; connected matches and
replays retain their existing progression rules. Closing it preserves an
independent manual pause.

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

## Implementation and validation

The shared adapter is [player.js](../web/player.js), initialized by [details.js](../web/details.js). Input fixes live in [input.js](../web/input.js); startup recovery lives in [loader.js](../web/loader.js). The renderer, Courtyard and Orbit Breaker call the same player frame hooks after drawing.

The [player release catalog](../website/releases/player30.json) records the earlier implementation series. The [current engine workflow](ENGINE_WORKFLOWS.md) covers the shipped API additions; [release status](STOPPING_POINT.md) separates completed work from deferred plans. See the [validation guide](GAME_TESTS.md) for native, Wasm and browser checks. Platform support claims remain governed by [Platforms](PLATFORMS.md).
