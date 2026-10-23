# Compose a renderer HUD

The [runnable presentation lab](../crates/ferravex-render/examples/journey200_hud.rs)
combines the existing renderer, current control bindings, measured button layout,
live preferences and retained-render diagnostics. It has a native window and a
headless assertion mode. It adds no library API, dependency, save format or host
export. All commands below run from the repository root.

## Run the lab

```sh
cargo run -p ferravex-render --example journey200_hud --offline --locked -j 2 -- --help
cargo run -p ferravex-render --example journey200_hud --offline --locked -j 2 -- --headless
cargo test -p ferravex-render --example journey200_hud --offline --locked -j 2
```

To open the native window, omit the example arguments. A display and the native
host's system libraries are required; see [platform prerequisites](PLATFORMS.md).
For a bounded launch, use the lab's own `--frames N` option:

```sh
cargo run -p ferravex-render --example journey200_hud --offline --locked -j 2
cargo run -p ferravex-render --example journey200_hud --offline --locked -j 2 -- --frames 2
```

`N` must be a positive `u32`. Extra/unknown arguments fail with a diagnostic and
exit 1. This example does not use the games' `Demo` parser: `--autoplay`,
`--smoke-frames` and `--screenshot` are not its flags. It has no browser host or
catalog entry; a successful native launch does not establish Wasm support.

In the window:

- **Preview** accepts one presentation event, grows a retained cyan square
  briefly, and overlays a bounded amber flash inside the preview area. The
  initial keyboard shortcut is Space; the registered gamepad channel is button
  2. These are configuration labels, not a claim that a device is connected.
- **Reduce motion** clears and suppresses the square's pulse independently of
  **Reduce flash**, which clears and suppresses the amber overlay. Selected
  buttons show which reductions are active.
- **Rebind preview** switches the keyboard shortcut between Space and J while
  retaining the gamepad binding. The displayed hint reads the updated action.
- Buttons use the shared UI's pointer release, Tab/arrow navigation and
  Enter/gamepad acceptance. Initial automatic focus is disabled. Close the
  window to exit. This lab installs no touch action region.

The profile is `PlayerInput::transient`; all choices stay in memory. The Preview
action is deliberately registered with `global_action`, because it is a UI
demonstration with no game rules, clock, network or replay. A game must emit
feedback only after its rules accept an event. It must sample gameplay input
before advancing time and consume `take_tick` edges inside actual fixed ticks,
including catch-up batches. See the [agent guide](AGENT_GUIDE.md#fixed-ticks-input-and-feedback)
and [input guide](INPUT.md). Do not copy the lab's frame-level global action
handling for shooting, jumping or other gameplay actions.

## Reuse the actual layout and hint APIs

The example's private `layout` function returns geometry without touching the
renderer. `Viewport::responsive` uses host logical pixels at scale 1 and applies
left/top/right/bottom safe insets. The example uses a 12-pixel margin and a
16-pixel font. `Font::try_metrics` and `BitmapFontMetrics::measure_line` measure
each ASCII button label; its widest advance plus 16 pixels of button padding
sets `hud_slots`' minimum column width. Rows keep their 44-pixel height while
space permits. Short viewports clip later rows and collapse the preview instead
of making overlapping controls or negative sizes.

`UiRect` is the play/UI rectangle (`min`, `size`); it differs from the immediate
renderer math `Rect` (`x`, `y`, `w`, `h`). UI coordinates start at the safe area's
origin. The immediate preview adds `viewport.pixels().min` when positioning its
retained mesh, so it aligns with the UI after the safe inset.

This complete, display-free example shows a rebinding-aware hint and a measured
button. All `unwrap` calls below use fixed, valid demonstration inputs; the
runnable lab propagates checked API failures as visible errors.

```rust,check crate-type=bin
use ferravex_render::{get_default_font, input::{ActionKind, Binding, InputCode, PlayerInput}};
use ferravex_render::ui::{ButtonStyle, Ui, Viewport, hud_slots};

fn main() {
    let mut input = PlayerInput::transient("presentation-guide");
    input.global_action("preview", "Preview", ActionKind::Button,
        vec![Binding::digital(InputCode::Key(32))]).unwrap();
    let mut bytes = [0; 128];
    assert_eq!(input.format_hint("preview", &mut bytes).unwrap(), "Space");
    input.rebind("preview", vec![Binding::digital(InputCode::Key(106))]).unwrap();
    let hint = input.format_hint("preview", &mut bytes).unwrap();
    assert_eq!(hint, "J");

    let metrics = get_default_font().try_metrics(16, 1.).unwrap();
    let width = metrics.measure_line(hint).unwrap().width + 16.;
    let viewport = Viewport::responsive([320., 240.], [12.; 4]).unwrap();
    let rect = hud_slots(viewport.bounds(), 1, width, 44., 8.).unwrap().next().unwrap();
    let mut ui = Ui::new(); // Retain this instance across real frames.
    ui.begin(viewport);
    ui.hud_button(1, rect, hint, true, ButtonStyle {
        size: 16., ..Default::default()
    }).unwrap();
    assert_eq!(rect.size[1], 44.);
}
```

`format_hint(name, &mut bytes)` selects `current_device()`; it reads the current
bindings without consuming input edges. `format_hint_for(name, Some(device),
&mut bytes)` queries a particular family, while `None` includes all bindings.
An absent family binding yields `Unbound`; it does not fall back to keyboard.
`HintError::BufferTooSmall { required }` reports the byte count and leaves the
buffer intact. The lab's two bounded bindings fit its 128-byte buffer. A larger
editor should handle the capacity error explicitly. See the
[hint implementation](../crates/ferravex-render/src/input/player/editor.rs).

Font measurements describe the immediate bitmap renderer's logical advance and
ink bounds. Unsupported characters are blank in that renderer; shared UI uses
`?` fallback and its own wrapping. The lab measures ASCII labels, for which the
advances agree. `Ui::label` and `hud_button` ellipsize at fixed size; generic
`Ui::text_fmt` can shrink a nonwrapping diagnostic line to fit. The lab uses that
behavior for its hint and numeric diagnostics. Do not claim matching multiline
wrapping or framebuffer coverage from `BitmapTextMetrics`. See the
[font source](../crates/ferravex-render/src/font.rs) and
[shared UI](../crates/ferravex-render/src/ui/shared.rs).

## Apply preferences to a real consumer

`preferences::current()` reads actual audio and underlying graphics choices;
`try_apply(next)` validates a complete snapshot before changing anything.
`try_apply_with_feedback(next, policy)` also publishes the session's independent
motion/flash policy. `ApplyOutcome` separates audio, graphics and feedback
changes and reports effective quality/look. A URL or environment override may
mask a changed user choice: do not save the effective result over that choice.
These calls do not persist preferences or advance any clock.

Publishing policy alone does not update an existing effect owner. Call
`preferences::sync_feedback(&mut feedback)` before emitting accepted events and
before sampling presentation, even on zero-tick frames and while local menus
are open. Disabling a channel clears its pending effects, so switching back on
does not resurrect them. Independently owned particles or camera impulses need
their own policy handling. The lab has only the controller-owned pulse/flash.

This complete program exercises an actual consumer and invalid-snapshot rollback:

```rust,check crate-type=bin
use ferravex_play::feedback::{FeedbackController, FeedbackEvent, FeedbackPolicy};
use ferravex_render::{Vec3, preferences};

fn main() {
    let mut feedback = FeedbackController::<8>::new(Default::default(), 180).unwrap();
    preferences::try_apply_with_feedback(preferences::current(), FeedbackPolicy::default()).unwrap();
    preferences::sync_feedback(&mut feedback);
    assert!(feedback.emit(1, FeedbackEvent::hurt(Vec3::ZERO, 1.).unwrap()).unwrap().is_some());
    assert!(feedback.impact().emphasis > 0.);
    assert!(feedback.impact().flash > 0.);

    let reduced = FeedbackPolicy::new(true, false, 1.).unwrap();
    preferences::try_apply_with_feedback(preferences::current(), reduced).unwrap();
    assert!(preferences::sync_feedback(&mut feedback));
    assert_eq!(feedback.impact().emphasis, 0.);
    assert!(feedback.impact().flash > 0.);

    let before = preferences::current();
    let invalid = preferences::Preferences { master: f32::NAN, ..before };
    assert!(preferences::try_apply_with_feedback(invalid, FeedbackPolicy::default()).is_err());
    assert_eq!(preferences::current(), before);
    assert_eq!(preferences::feedback_policy(), reduced);
    preferences::try_apply_with_feedback(before, FeedbackPolicy::default()).unwrap();
    preferences::sync_feedback(&mut feedback);
    assert_eq!(feedback.impact().emphasis, 0.); // A cleared pulse stays cleared.
}
```

The lab calls `advance_with_impact` once per presentation frame with a delta in
`0..=0.25`. Its returned impact drives square scale and the bounded flash once;
it ignores hit-stop and does not use feedback to gate a clock. A game with
network authority or replay must keep that progression running under local UI.
The [feedback source](../crates/ferravex-play/src/feedback.rs) defines event
deduplication, capacity and envelope behavior; the
[preference source](../crates/ferravex-render/src/preferences.rs) defines application.

Existing browser games publish the common
`ferravex_feedback_preferences(motion: u32, flash: u32) -> u32` ABI. Both flags
must be 0 or 1; accepted calls return 1 even when unchanged, and invalid flags
return 0 without changing policy. That publication preserves feedback intensity
and does not prove a game's consumers obey it. This lab adds no export. The
browser host owns system-motion resolution and persistence: its storage key
stays at v1, while backups export v2 and import validated v1/v2 data. Those host
contracts are separate from this transient example; see
[visual preferences](../web/visuals.js) and [player settings](../web/player.js).

## Inspect diagnostics at the correct boundary

The lab creates one `ResidentMesh::try_new` square and reuses it. Each visible
preview uses `retained::draw_resident_checked`, which reports invalid transforms
or capacity errors rather than silently skipping them. Collapsed preview regions
submit nothing. `retained::telemetry_snapshot()` then reports mesh usage,
recorded instances/batches and cumulative draw-validation rejections. The HUD
labels these as CPU counters. They are neither GPU timings nor actual driver
draw calls; current-frame batches may change on finalization.

`Ui::reserved_bytes()` measures the UI's reserved CPU storage, excluding the
renderer stream. Headless resizing asserts that this value stays unchanged.
That does not establish zero allocations across the entire renderer. The lab
reuses the UI arena and mesh, bounds hint storage, and does not append an event
history. It leaves diagnostics intact: `clear_background` clears frame work but
does not reset cumulative retained rejection counters. See
[retained telemetry](../crates/ferravex-render/src/retained.rs).

The interactive frame samples global input, queues buttons, finishes interaction
once, applies accepted UI actions, and paints the preview plus labels. Policy
and hint changes affect that frame; queued button selection styling updates on
the next frame. Preference syncing also runs when there is no accepted action.
Other input/focus/lifecycle owners remain the renderer's job.

## Validation and integration scope

The headless run asserts six logical viewport cases: 960x640, 320x568, 568x320,
120x120, 1x1 and 0x0. It verifies contained, nonoverlapping regions, visible versus
collapsed retained submissions, unchanged UI reservation, rebinding, unbound
touch hints, independent policy channels, cleared-effect behavior and invalid
preference rollback. Four inline tests add row reflow, invalid dimensions,
unchanged hint buffers on capacity errors, preserved gamepad binding and CLI
argument bounds. Headless mode records CPU geometry without a window or GPU;
it does not inspect rasterized pixels or prove physical input behavior.

The two complete Rust fences are marked for the
[documentation checker](../scripts/check-docs.py). Link checking is:

```sh
python3 -B scripts/check-docs.py docs/JOURNEY_200_PRESENTATION.md --json
```

For `--check-rust`, supply matching first-party `.rlib` artifacts using
`--extern ferravex_render=PATH` and `--extern ferravex_play=PATH`. The checker
automatically adds their directories to the dependency search path. Its check
is metadata-only: it never executes assertions. The task receipt records
separate compilation and execution of both snippets, the example tests, linting
and the bounded native launch. No fresh browser, Wasm, screen-reader, controller,
touch, performance benchmark or visual playtest is established here.

Cargo discovers the example automatically; no manifest/reexport change is
needed. This guide is listed in the current shared validator and site builder.
The validator's `python.docs` command checks links; compiling its marked snippets
manually requires both externs above and `--check-rust`. The separate full
`rust.guide-examples` step compiles and executes them with
[the trusted-example runner](../scripts/check-guide-examples.py). Parent owns shared build
output, combined validation and any final integration claim.
