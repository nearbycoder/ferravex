# Accessible gameplay integration

This guide describes the current Journey200 source for all fifteen demos and
three browser adapters. It documents implemented controls and presentation
choices, runnable integration recipes, and remaining verification work. It is
not an accessibility certification or a claim that every game is playable with
assistive technology. Fresh combined Wasm/browser, physical-device and
screen-reader validation remains the integration owner's responsibility.

Read [unified input](INPUT.md) for profile semantics and the
[coding-agent guide](AGENT_GUIDE.md) for fixed ticks and the
[browser player guide](PLAYER_GUIDE.md) for settings and backups.
The behavior below follows the actual
[player](../web/player.js), [input bridge](../web/input.js),
[visual synchronizer](../web/visuals.js) and game implementations.

## Keyboard access and remapping

The browser pages provide a focusable named canvas, linked text instructions,
visible page controls and loading/status text. Those DOM descriptions expose
instructions, not a semantic representation of the game world. The shared
Controls editor and most game HUDs are drawn inside the canvas; keyboard
navigation there does not create DOM buttons or screen-reader announcements.

Focus the canvas before game actions. F2 or the Controls button opens the shared
editor; controller Home is also reserved for it. Escape cancels a pending
binding capture before closing Controls. Up/Down and Enter navigate the editor;
Tab behavior depends on the host below. Browser Ctrl/Command/Alt shortcuts and
IME composition are excluded from gameplay by the shared input bridge, except
for the explicit page-focus shortcuts authored by individual hosts. F2/Home are
runtime-reserved, and page-reserved keys can still appear in an imported profile:
choose a different gameplay binding where the host intercepts one.

Pause before reading help. Moving focus within the document is not a universal
pause signal. Releasing pointer lock, leaving the browser window, opening
Controls/Details, and moving to another DOM element are different events.
Fullscreen may handle the first Escape; the fullscreen handler leaves Escape to
an open Controls editor for capture cancellation. Boot focus logic preserves
intervening user navigation during loading; its tests use event/DOM doubles.

| Browser host | Route out of the canvas in current HTML |
| --- | --- |
| [Emberline](../web/emberline/index.html), [Sky Courier](../web/sky-courier/index.html), [Siege Foundry](../web/siege-foundry/index.html), [2D Physics](../web/physics-2d/index.html), [3D Physics](../web/physics-3d/index.html) | Tab/Shift+Tab leaves when Controls is closed. Close the editor first to use page traversal. |
| [Pocket Rally](../web/pocket-rally/index.html) | Tab leaves with Controls closed; Shift+Tab leaves even with it open. |
| [Neon Swarm](../web/neon-swarm/index.html), [Marble Run](../web/marble-run/index.html) | Shift+Tab leaves; ordinary Tab remains available to game/menu navigation. Use Up for backwards editor navigation. |
| [Salvage Pinball](../web/salvage-pinball/index.html), [Orbit Breaker](../web/owned-orbit/index.html) | Shift+Tab focuses How to play, including while Controls is open; it leaves panel state unchanged. |
| [Crystal Cavern](../web/crystal-cavern/index.html), [Courtyard](../web/owned/index.html) | Tab is reserved for the page, including with Controls open; use arrows/Enter in the editor. Release Courtyard's mouse capture first. |
| [Orchard Guard](../web/orchard-guard/index.html) | Shift+Escape opens/focuses How to play without pausing; exit fullscreen first. Tab remains in-canvas navigation. |
| [Dune Skimmer](../web/dune-skimmer/index.html) | Ctrl+F6 focuses All Ferravex games. The Return to game link restores canvas focus. |
| [Strike](../web/strike/index.html) | Release mouse capture, then Alt+M (Option+M) focuses the all-games link. Tab remains the scoreboard/menu key. |

Keyboard access to menus does not establish complete keyboard play. In
[Orchard Guard](../examples/orchard-guard/README.md), plot numbers identify places,
not selection shortcuts: keyboard plot selection requires remapping **Point /
select**, moving the virtual cursor with WASD, and activating that binding. The
capture replaces its mouse binding. If a HUD button retains keyboard focus,
opening and closing Controls clears it. This is a current ergonomic limit, not
an automatic keyboard-selection path.

Static HTML and catalog controls describe defaults. After the first input sample,
use `PlayerInput::format_hint` for the current device or `format_hint_for` for a
chosen family. These read current mappings without consuming edges. `Unbound`
means no nonzero binding in that selection; it does not mean the action is
unavailable in every family. Pad/touch numbers denote channels, not verified
hardware glyphs or installed touch regions. A bounded hint buffer can fail with
`HintError::BufferTooSmall { required }`; show an explicit “See Controls” fallback
instead of silently displaying an old/default binding.

Profiles use the existing FXIN v1 format, separately from player-settings
backups. Browser input storage uses `ferravex.controls.v1.<owner>`; unavailable or
invalid storage keeps usable controls and reports a panel error. For a custom
import UI, `ActionMap::validate_config(&bytes)` previews unknown names and bounded
active-context conflicts; `apply_config_checked(&bytes)` rejects those conflicts
before changing mappings. The existing permissive import remains available for
intentional sharing. Diagnostics examine currently active contexts, not every
future context combination. See the [configuration implementation](../crates/ferravex-play/src/input/config.rs).

### Recipe: current hints and one input snapshot per tick

This complete headless example exercises the public renderer adapter using a
synthetic controller sample. It checks remapping, unbound hints, a press/release
between ticks, catch-up and local pause. It does not open a window, save a profile
or establish physical-controller support.

```rust,check,crate-type=bin
use ferravex_core::session::GameSession;
use ferravex_render::input::{
    self, ActionKind, Binding, HintDevice, InputCode, PadState, PlayerInput,
};

fn main() {
    let mut player = PlayerInput::transient("accessibility-recipe");
    player.action("jump", "Jump", ActionKind::Button, vec![
        Binding::digital(InputCode::Key(32)),
        Binding::digital(InputCode::PadButton(0)),
    ]).unwrap();
    let mut hint = [0; 96];
    assert_eq!(player.format_hint_for(
        "jump", Some(HintDevice::KeyboardMouse), &mut hint,
    ).unwrap(), "Space");
    player.rebind("jump", vec![Binding::digital(InputCode::PadButton(1))]).unwrap();
    assert_eq!(player.format_hint_for(
        "jump", Some(HintDevice::KeyboardMouse), &mut hint,
    ).unwrap(), "Unbound");
    assert_eq!(player.format_hint_for("jump", None, &mut hint).unwrap(), "Pad 1");

    let mut session = GameSession::try_new(120., 12, true).unwrap();
    let mut pad = PadState { connected: true, ..Default::default() };
    let mut frame = |elapsed: f64, level: f32| {
        pad.buttons[1] = level;
        assert!(input::gamepad_sample(0, pad));
        input::host_frame(elapsed as f32, 640, 360, || {
            player.sample(true); // Real games reconcile menu/focus gates here.
            let fixed = session.advance(elapsed);
            let mut presses = 0;
            for _ in 0..fixed.steps {
                let [jump] = player.take_tick(["jump"]);
                presses += u32::from(jump.pressed);
                // Pass this same snapshot to gameplay/animation for this tick.
            }
            (fixed.steps, presses)
        })
    };
    assert_eq!(frame(0., 0.), (0, 0)); // Connect neutral before accepting presses.
    assert_eq!(frame(0., 1.), (0, 0));
    assert_eq!(frame(0., 0.), (0, 0)); // A complete tap waits for a real tick.
    assert_eq!(frame(1. / 60., 0.), (2, 1));
    assert_eq!(frame(1. / 60., 0.), (2, 0));
    assert!(session.set_paused(true));
    assert_eq!(session.advance(30.).steps, 0);
    assert!(session.set_paused(false));
    assert_eq!(session.advance(0.).steps, 0); // No accumulated pause-time debt.
    assert!(input::gamepad_sample(0, PadState::default()));
}
```

In a real loop, handle global menu actions in the presentation frame, then
reconcile local lifecycle before `advance`. Use either `sample_play()` for the
legacy single-player input pause owner or `sample(enabled)` with your game's
session owner. Do not advance a local simulation merely because `sample` was
called. Consume gameplay `take_tick` only inside scheduled ticks; several taps
before one tick coalesce into press/release flags, not a count. Read relative
`look(dt)` once per presentation frame and pass absolute aim to simulation.
Context/focus changes clear stale edges and suppress held buttons until release.
Restart must also clear game-owned pending commands, effects and camera history.

## Touch layouts and cancellation

The shared [touch core](../crates/ferravex-play/src/input/touch.rs) supports at most
16 controls and 16 captured touches. A finger acquires its role on start and keeps
it until release/cancel; moving across a button boundary does not transfer it to
another control. Layout changes release captures; unchanged layouts retain them.
The browser bridge handles pointercancel/lost capture, viewport changes, page
lifecycle and mouse-button chords. After cancellation, a move alone cannot
reacquire a held finger. These are implemented contracts with synthetic tests,
not physical touchscreen verification.

Touch interaction differs by game. These are authored defaults; remapping and
custom layouts can change the bindings. The source-backed READMEs below explain
the corresponding rules and controls.

| Demo | Authored touch interaction |
| --- | --- |
| [Emberline](../examples/emberline/README.md) | Left/right/jump/pause regions; tap start/completion/pause cards. |
| [Neon Swarm](../examples/neon-swarm/README.md) | Lower-left movement stick; right-side dash; automatic target/fire while gameplay touch is active. |
| [Pocket Rally](../examples/pocket-rally/README.md) | Four bottom steering/gas/brake regions, fitted to narrow viewports. |
| [Salvage Pinball](../examples/salvage-pinball/README.md) | Tap to launch; hold lower-left/right flipper regions. |
| [Orchard Guard](../examples/orchard-guard/README.md) | Tap plots and build/upgrade/wave/restart buttons. |
| [Marble Run](../examples/marble-run/README.md) | Stick/brake; Courses, Recover and finish-panel buttons. |
| [Dune Skimmer](../examples/dune-skimmer/README.md) | Left/right steering, GO and BOOST; controls appear after first touch. |
| [Crystal Cavern](../examples/crystal-cavern/README.md) | Left movement stick, right look region and USE; tap overlay buttons. No pointer lock required. |
| [Siege Foundry](../examples/siege-foundry/README.md) | Scene drag aims; visible Fire button shoots. |
| [Sky Courier](../examples/sky-courier/README.md) | Left stick turns/climbs; holding the right side slows flight. No default touch boost region. |
| [Strike](../examples/strike/README.md) | Left movement stick, right swipe aim, FIRE/UP/LOAD buttons and tactical USE. No mouse capture needed for touch. |
| [2D Physics](../examples/physics-2d/README.md) | Tap the scene to place the selected shape; inspector buttons drop/pulse/pause/step/reset. |
| [3D Physics](../examples/physics-3d/README.md) | Drag the scene to orbit; inspector buttons select/drop/pulse/pause/step/view/reset. |
| [Courtyard](../examples/owned-lab/src/interactive.rs) | Left movement stick, right look region, FIRE/UP/R regions. |
| [Orbit Breaker](../examples/owned-orbit/README.md) | Drag to steer; tap to launch/retry. |

A `Touch N` hint alone is not evidence that a corresponding region exists.

`TouchLayoutSettings` retains mirroring, size, opacity and per-slot edits.
`apply_in_safe_area` and `diagnose_layout` provide checked geometry for a custom
host: finite viewport dimensions in 64..=1,000,000, a contained safe rectangle at
least 64 pixels on each axis, bounded slots, and full-shape overlap diagnostics.
Button fitting uses a 44 logical-pixel minimum and 8-pixel margin where the
validated area permits it. This geometric rule is not a measured touch-usability
or standards claim. Overlap can be intentional (a background look region);
diagnostics do not reorder controls, and boundary contact counts as overlap.

### Recipe: fit, inspect and install a custom surface

The safe rectangle below is supplied by the host; Rust does not read CSS safe
area insets. Resolve from the original authored controls on every resize, then
install into a directly owned `TouchSurface`. The current
[`PlayerInput::set_touch_layout`](../crates/ferravex-render/src/input/player.rs)
uses legacy `TouchLayoutSettings::apply` itself: it is not a safe-area-aware
adapter. Passing already customized results through it can apply saved edits
twice. Existing demo toolbar safe-area CSS does not prove in-canvas adaptation.

```rust,check,crate-type=bin
use ferravex_math::{Rect, Vec2};
use ferravex_play::input::{ActionMap, InputCode};
use ferravex_play::input::touch::{
    Control, Event, Output, Phase, Region, TouchLayoutSettings, TouchSurface,
};

fn main() {
    let viewport = Vec2::new(400., 200.);
    let safe = Rect::new(20., 10., 360., 180.);
    let authored = Control {
        region: Region::Circle { center: Vec2::new(100., 100.), radius: 24. },
        output: Output::Button(0),
    };
    let settings = TouchLayoutSettings::default();
    let resolved = settings.apply_in_safe_area(authored, 0, viewport, safe).unwrap();
    assert!(TouchLayoutSettings::diagnose_layout(&[resolved], viewport, safe)
        .unwrap().is_clear());
    assert!(TouchLayoutSettings::diagnose_layout(&[resolved; 2], viewport, safe)
        .unwrap().overlaps[0][1]);
    assert!(settings.apply_in_safe_area(authored, 0, viewport,
        Rect::new(0., 0., 63., 200.)).is_err());

    let mut map = ActionMap::default();
    let mut surface = TouchSurface::default();
    surface.set_layout(&[resolved], &mut map).unwrap();
    let center = Vec2::new(110., 100.);
    surface.handle(Event { id: 1, position: center, phase: Phase::Start }, &mut map)
        .unwrap();
    assert_eq!(map.physical_value(InputCode::Touch(0)), 1.);
    surface.handle(Event { id: 1, position: center, phase: Phase::Cancel }, &mut map)
        .unwrap();
    assert_eq!(map.physical_value(InputCode::Touch(0)), 0.);
}
```

## Contrast, text, motion and flash

Open **Details → Player tools** in a browser host. The settings have different
scopes; none replaces gameplay collision, scoring, timers or authoritative ticks.

| Setting | Implemented scope and limit |
| --- | --- |
| Player contrast | High-contrast backgrounds, borders, text and focus styles on targeted DOM player controls. Does not recolor the game canvas or guarantee authored game contrast. |
| Player text size | Enlarges the Details dialog and its controls. Does not scale in-canvas HUD/editor glyphs or expose them to assistive technology. |
| Game and player UI motion | System preference by default, or explicit Reduced/Standard. The shared visual helper resolves `prefers-reduced-motion` live and publishes the resulting boolean; the player also disables its UI transitions when reduced. |
| Reduce game flashes | Independent Off/On choice, default Off. Not inferred from motion or a speculative media query. Suppresses the channels each game implements below; it is not a flash-frequency or photosensitivity-safety assessment. |

Current exports use player-settings backup **version 2**, strictly importing
versions 1 and 2. The local key remains `ferravex.player.v1.<game-id>`. Fields are
`hud`, `cap`, `contrast`, `text`, `motion`, `flash`; values are the implemented
string enums. A v1 backup cannot contain flash; a sparse import retains missing
current choices. Preview and Cancel leave settings untouched; Apply persists the
validated choices. Reset restores all six defaults. Storage denial leaves
session-only settings with an explanation. Backups exclude remaps, touch-layout
profiles, Rust graphics/audio preferences and game progress.

### Implemented per-demo presentation channels

These rows summarize inspected source, not fresh visual acceptance. Source links
identify consumers; their game READMEs provide controls and targeted-test context.
Every listed consumer reads shared preferences during presentation, including
local menus and zero-tick frames. Essential motion can remain: reducing decoration
does not stop balls, vehicles, enemies, pickups, moving platforms or required
camera tracking. Re-enabling cleared transient effects must require fresh events;
ambient animations can resume without catching up suppressed time.

| Demo / source | Reduced motion | Reduced flashes |
| --- | --- | --- |
| [Emberline](../examples/emberline/src/presentation.rs) | Clears dust/jump accents; removes decorative drift, bob/squash, parallax and velocity look-ahead. | Independently clears bright jump accents; steady checkpoint text remains. |
| [Neon Swarm](../examples/neon-swarm/src/presentation.rs) | Clears dash emphasis/trails and moving sparks; stops pickup pulsing. | Clears muzzle, damage and arena flashes, additive sparks and pickup blinking; static attack markers remain. |
| [Pocket Rally](../examples/pocket-rally/src/presentation.rs) | Clears/suppresses skid trails. Car, gameplay ghost and guidance continue. | No authored flash channel, shake or hit-stop to disable. |
| [Salvage Pinball](../examples/salvage-pinball/src/app/presentation.rs) | Clears bumper rings and ball trails. | Clears bumper illumination; textual scoring remains. |
| [Orchard Guard](../examples/orchard-guard/src/presentation.rs) | Disables decorative leg movement. | Clears presentation hit flashes and remembers suppressed impacts. |
| [Marble Run](../examples/marble-run/src/presentation.rs) | Removes gem bob/spin, camera anticipation and recovery settling. | No additional flash channel; checkpoint notices remain steady. |
| [Dune Skimmer](../examples/dune-skimmer/src/presentation.rs) | Clears dust/shake; removes cosmetic banking, hover/gate bob, exhaust deformation and speed/boost camera widening. | Freezes exhaust color pulse independently. |
| [Crystal Cavern](../examples/crystal-cavern/src/lib.rs) | Freezes crystal bobbing and crystal/altar rotation. | Bypasses the animated crystal shader; freezing shader time alone would not remove brightness changes from bobbing. |
| [Siege Foundry](../examples/siege-foundry/src/lib.rs) | Clears barrel recoil. | No screen-flash channel; shot/target text and colors remain. |
| [Sky Courier](../examples/sky-courier/src/presentation.rs) | Clears wing trails/exhaust/camera impulses; stops propeller decoration. | Clears the damage screen-edge cue. |
| [Strike](../examples/strike/src/effects.rs) and [weapon animation](../examples/strike/src/animation.rs) | Clears weapon motion, crosshair expansion, menu sway and cosmetic particles. | Clears damage tint, muzzle light and additive shot particles/traces. Accepted hit confirmation remains. |
| [2D Physics](../examples/physics-2d/src/lib.rs) | Clears expanding pulse rings. | Also clears pulse rings; the same cosmetic combines motion and brightness. |
| [3D Physics](../examples/physics-3d/src/lib.rs) | Clears pulse rings and cuts camera smoothing; deliberate orbit/zoom remains. | Also clears pulse rings, independently of the motion choice. |
| [Courtyard](../examples/owned-lab/src/lib.rs) | Clears sparks, weapon bob/operator sway; freezes decorative orbit camera. | Clears the hit marker. Training text, audio and peer progression remain. |
| [Orbit Breaker](../examples/owned-orbit/src/game.rs) | Clears particles, trails and paddle emphasis; stops decorative capsule pulses. | Clears panel flashes; cracks, charge meters and falling pickups remain. |

An effect can belong to both channels: moving additive sparks and expanding bright
rings stop under either opt-out. Independent preferences mean each choice is
stored and applied separately, not that every effect has only one classification.

### Recipe: apply a policy to a real feedback consumer

The standard [runtime](../web/runtime.js) and the custom
[Courtyard](../web/owned/platform.js) / [Orbit](../web/owned-orbit/platform.js)
hosts read `player.presentationPreferences` and refresh `bindVisualPreferences`
before game/control frames, after queued quality changes and after renderer
restoration. Their common Wasm export is
`ferravex_feedback_preferences(motion: u32, flash: u32) -> u32`: only 0/1 flags
are valid; 1 means accepted (including unchanged), 0 means rejected with state
preserved. Intensity stays game-owned. Both custom games reuse the common renderer
policy/export; a second policy store or export is unnecessary.

The helper reads effective quality for invalidation; it does not persist a
URL-masked quality value. Missing/rejected exports leave the last successful
snapshot unchanged, expose `canvas.dataset.visualPreferencesError`, and retry
on later refreshes. Refresh failure must not gate network/replay progression.
Successful publication alone does not prove the game consumes the policy.

Call `preferences::sync_feedback` before accepted-event feedback and presentation
sampling, even with zero ticks or an open menu. It updates existing controller
envelopes immediately. Use `feedback_policy()` separately for already-emitted
particles, trails, shader animation and camera state outside that controller.
Clear those objects when disabled; hiding their draws alone can revive old effects.

```rust,check,crate-type=bin
use ferravex_math::Vec3;
use ferravex_play::feedback::{FeedbackController, FeedbackEvent, FeedbackPolicy};
use ferravex_render::preferences;

fn main() {
    let previous = preferences::feedback_policy();
    preferences::set_feedback_policy(FeedbackPolicy::default());
    let mut feedback = FeedbackController::<8>::new(Default::default(), 7).unwrap();
    preferences::sync_feedback(&mut feedback);
    let event = FeedbackEvent::hurt(Vec3::ZERO, 1.).unwrap();
    // Gameplay has already accepted damage. A duplicate ID cannot replay it.
    let original = feedback.emit(1, event).unwrap().unwrap();
    assert!(feedback.emit(1, event).unwrap().is_none());
    assert!(feedback.impact().flash > 0.);

    preferences::set_feedback_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
    assert!(preferences::sync_feedback(&mut feedback)); // No simulation tick needed.
    assert_eq!(feedback.impact().emphasis, 0.);
    assert!(feedback.impact().flash > 0.); // Motion did not change flash preference.
    preferences::set_feedback_policy(FeedbackPolicy::new(true, true, 1.).unwrap());
    preferences::sync_feedback(&mut feedback);
    assert_eq!(feedback.impact().flash, 0.);
    let reduced = feedback.emit(2, event).unwrap().unwrap();
    assert_eq!((reduced.cue, reduced.gain), (original.cue, original.gain));
    assert_eq!(feedback.impact().flash, 0.);

    preferences::set_feedback_policy(FeedbackPolicy::default());
    preferences::sync_feedback(&mut feedback);
    assert_eq!(feedback.impact().flash, 0.); // Cleared flashes cannot revive.
    feedback.clear(); // Restart also clears the bounded event-ID history.
    preferences::set_feedback_policy(previous);
}
```

`FeedbackPolicy::new` checks finite intensity in 0..=1. Policy intensity scales
visuals, whereas event intensity can also scale sound: do not change accepted
event intensity to implement a visual preference. Handle feedback capacity/errors;
only an `Ok(Some(response))` should produce its corresponding new cue/effects.
A bounded deduplication history is not a replacement for network acknowledgments.
The policy, renderer and `GameAudio` belong to the calling thread; publish and
consume on that thread. The current browser controls persist motion/flash;
there is no equivalent newly implemented native settings UI claim.

## Pause is a gameplay contract

For local play, `GameSession` ignores inactive elapsed time. Reconcile focus,
settings and manual-pause state before advancing. Preserve a manual pause when a
panel closes, neutralize stale gameplay input, and stop subsequent budgeted ticks
if the game reaches a terminal outcome. Pausing may let cosmetic envelopes settle;
it must not spend ammunition, advance cooldowns, lose race time or score points.

- Emberline, Pocket Rally, Salvage Pinball, Orchard Guard, Marble Run, Dune Skimmer,
  Crystal Cavern, Siege Foundry, Sky Courier and both physics labs gate local
  simulation for Controls/focus interruptions while retaining their own manual
  pause rules. Closing a temporary gate is not a command to undo manual pause.
- Neon Swarm explicitly leaves its Resume screen up after Controls/focus return.
  Courtyard offline training and Orbit Breaker likewise require explicit resume
  after leaving the browser; returning focus alone is insufficient.
- Marble Run's course picker pauses its countdown, timer and moving obstacles
  while retaining a separate manual pause. Physics labs permit immediate shape
  edits while paused and a deliberate N/STEP single tick; pulse requires running.
  Both physics labs default to **Space** for pause, not Escape. Physics 2D reset preserves
  pause; Physics 3D reset resumes and restores its view.
- [Strike's host](../examples/strike/src/host.rs) pauses offline practice under
  its menu. Network mode keeps authoritative progression and sends neutral local
  input; the player remains vulnerable. Replay continues under local menus:
  use the dedicated replay pause control. `ferravex_replay_finished() -> u32`
  reports actual driver completion; paused or stationary playback is not proof
  of completion. The parent's browser-panel wiring/fresh verification is separate.
- Courtyard authority and guest prediction likewise continue beneath local menus;
  reset is disabled while connected. Page hiding, browser throttling, graphics
  loss and lifecycle suspension can still stop host submissions. This does not
  make a background browser tab a continuously running server.

## Evidence and remaining platform work

The three marked Rust programs compiled and ran successfully for this guide.
Targeted Rust checks passed 7 preference tests, 2 tick-snapshot tests and 6
safe-area-filter tests. Shared Node suites passed 24 player, 17 bridge, 14 focus,
15 visual-source and 16 Details checks, plus the input contract script. All
fifteen per-game host source suites also passed. These results concern the source
snapshot inspected during task 183; the receipt records exact commands/artifacts.

`scripts/check-docs.py --check-rust` alone emits metadata and **does not run
examples**. The shared validator's `DOC_CHECK_PATHS` now registers this guide for
link/anchor checks. Its separate full `rust.guide-examples` step compiles and
executes them using [the trusted-example runner](../scripts/check-guide-examples.py).
Manual compiler checks need the Rust externs `ferravex_core`, `ferravex_math`,
`ferravex_play`, `ferravex_render`.

Run from the repository root. These are focused source/headless checks, with no
shared dist/site generation:

```sh
python3 -B scripts/check-docs.py docs/JOURNEY_200_ACCESSIBILITY.md --json
cargo test -p ferravex-render --lib preferences::tests --offline --locked -j 2
cargo test -p ferravex-render --lib input::player::tests::tick_snapshot --offline --locked -j 2
cargo test -p ferravex-play --lib safe_area --offline --locked -j 2
node web/tests/player.mjs
node web/tests/input.mjs
node web/tests/journey200-visual-bridge.mjs
node web/tests/journey200-focus-navigation.mjs
node web/tests/visuals.mjs --source-only
node web/tests/details.mjs --source-only
```

The Node tests use source inspection and DOM/event/Wasm doubles. They can verify
focus routing, cancellation, import transactions and preference publication
ordering; they cannot verify actual canvas contrast, accessible names as spoken,
GPU flashes, pointer-lock permissions or tactile control placement. Game unit
checks and receipts establish their stated native/headless scope only. Earlier
platform results are not fresh Journey200 acceptance.

For a fresh parent-owned browser pass, exercise each host's listed keyboard exit
and return path, capture-cancel/close ordering, remapped and unbound hints, window
blur, tab hide/return, viewport rotation, and graphics restoration. Toggle all
four motion/flash combinations while an effect is active, while paused and on a
zero-tick frame; re-enable and check that old effects do not return. Check text,
focus visibility and canvas usability at narrow widths and browser zoom; exercise
real touch cancellation, multiple fingers and controller reconnection. Verify
that local menus do not halt network/replay authority, and that replay completion
comes from the actual driver. Record browser, OS, device, build and observations.

Browser hosts use standard APIs directly: most games require WebGL2, while Orbit
uses its software framebuffer through Canvas2D. Audio and pointer capture can
require a user gesture; feature availability depends on the browser. Automatic
native controller discovery is implemented for Linux's joystick provider, not
Windows/Apple. Windows/macOS/iOS host source or cross-target checks do not prove
runtime or assistive-technology behavior. Consult [Platforms](PLATFORMS.md) for
host prerequisites, while keeping its historical checks distinct from this task.
No screen-reader playthrough, physical touch/gamepad test, audio listening,
flash measurement or WCAG conformance audit is claimed here.
