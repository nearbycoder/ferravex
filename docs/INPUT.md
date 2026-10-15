# Unified player input

Ferravex owns action mapping, contexts, event buffering, touch capture, controller
response and rebinding. Hosts provide physical events through browser standards
or the Linux joystick interface; there are no third-party input packages.

`ferravex_play::input` is the platform-independent core. The engine's `input`
module re-exports that API. `ferravex_render::input::PlayerInput` connects it to
ordered host events, storage and a compact controls panel. All fifteen browser
examples use this shared core and panel, including the two custom renderer hosts.
The two native owned reference programs remain headless simulation runners.

## Using a profile

```rust,no_run
use ferravex_render::input::{PlayerInput, Preset};
let mut input = PlayerInput::preset("my-platformer", Preset::Platformer);
// Each render frame, before game menus or legacy polling:
input.sample_play();
let dt = input.simulation_dt(1.0 / 60.0);
let movement = input.movement(); // right and forward/up are positive
// Inside each actual fixed simulation tick:
let jump = input.pressed("jump");
// Draw after the game's HUD:
input.draw_settings();
# let _ = (dt, movement, jump);
```

Use `sample(enabled)` when the game owns its menu/pause state. Network clients
must keep their clocks running and send neutral commands while gameplay is
inactive. `sample_play` provides single-player pause handling through the global
`pause` action. F2, the browser Controls button or controller Home opens settings;
Escape cancels a capture or closes settings. Tab/arrows/Enter and D-pad/A navigate
the panel. F2 and Home are reserved by the runtime so settings remain reachable.

Register custom actions with `action` or `global_action`; stable ASCII names
identify saved mappings independently of registration order. `ActionKind::Axis`
combines held values, `Button` buffers press/release edges and `Delta` accumulates
relative motion. `set_default_bindings` and `add_default_binding` customize an
authored preset before its first sample. `rebind` changes the current mapping;
`save` persists it. `transient` leaves persistence to the embedding application.

The action core supports exclusive or pass-through context stacks. Global
actions stay available beneath them. Gameplay buttons already held when a
context activates require release before another press. Axes can resume at their
current level. Context changes discard old motion and queued gameplay edges.
Quick press/release pairs survive render frames with no fixed tick. Consume
gameplay edges inside fixed ticks; global menu edges expire each render frame.

## Motion, controllers and touch

`look(dt)` returns local right/down radians: accumulated mouse and touch motion
plus the right-stick rate multiplied by dt. Consume it once per render frame and
send absolute aim to fixed simulation/network ticks. Mouse motion uses logical
viewport fractions; touch look uses logical pixels. Settings include independent
mouse/touch sensitivity, stick look rates, radial deadzone, response curve,
vertical inversion and a selected controller slot. `set_mouse_look_button`
restricts mouse look to dragging without restricting touch or controller look.
`cursor(dt, pixels_per_second)` adds movement-stick pointing to mouse/touch games.

Browser hosts poll at most four controllers, sixteen axes and thirty-two buttons;
unchanged values do not cross the Wasm boundary again. Standard browser mapping
is preferred. Linux reads `/dev/input/js0` through `js3` with nonblocking original
OS bindings, maps common axis/button codes, handles partial reads and disconnects,
and bounds work per poll. Other device layouts can be rebound. No physical
controller was available for this iteration's automated tests; hardware behavior
requires confirmation with actual devices. Other native hosts are iteration 20.

`touch::TouchSurface` captures each finger's role at start. Moving out of its
button, stick or look region does not transfer that finger to another role.
Multiple fingers can hold one button independently. Identical layouts preserve
capture; changed layouts, focus loss and cancellation release it. Configure
logical-pixel `Control` regions with `set_touch_layout`. Example-specific touch
art stays in each game's HUD. Touch-generated legacy mouse emulation does not
enter real mouse action mappings, so mixed devices stay independent.

## Saving and host boundaries

FXIN version 1 stores settings and named bindings with strict counts, finite-value
validation and an FNV checksum for accidental corruption. The maximum profile is
256 KiB. Imports validate before applying changes; unknown retired action names
are ignored and newly added actions retain defaults. The checksum is not an
authentication mechanism. Invalid/unavailable storage leaves usable defaults and
shows an error in the controls panel.

Browsers save under `ferravex.controls.v1.<owner>` in local storage. Native hosts
save `controls/<owner>.fxin` beneath their Ferravex configuration directory:
`$XDG_CONFIG_HOME/ferravex` (or `~/.config/ferravex`) on Linux,
`~/Library/Application Support/ferravex` on Apple systems, and
`%APPDATA%/ferravex` on Windows. Native writes use a synced temporary file and
rename. Changing bindings, resetting defaults or editing settings saves locally.

Custom renderer hosts can call `input::host_frame(dt, width, height, callback)`
once per render frame, outside a `start` future. The callback samples input,
advances the game and draws settings. The normal immediate vertex ABI exposes
the overlay afterwards. Owned Orbit draws those triangles through Canvas 2D;
Courtyard draws them through its WebGL host. Both reuse `web/input.js` for browser
input/storage and `web/input-overlay.js` for the controls overlay.

The platform journal holds 256 ordered events. Overflow discards that packet and
neutralizes input instead of retaining a partial press. Focus changes discard
pending edges, relative motion and touch capture. Reconnected or newly selected
controllers must return held channels to neutral before gameplay resumes.
Registration and rebinding allocate; bounded core feeds, context changes and
steady touch layouts reuse their storage. The open controls panel can allocate
for text/layout; retained UI work is iteration 16.
