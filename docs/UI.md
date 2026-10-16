# Shared HUD and menus

`ferravex_render::ui::Ui` renders compact screen-space overlays on the same
native OpenGL and browser WebGL2 stream as the world. It queues solid/gradient panels, bitmap
text and buttons, applies nested rectangular clips, and preserves paint order.
It does not change the caller's camera, lighting, material, depth or shader state.
Strike uses it for labels, diagnostics, roster and Play/Resume; Neon Swarm uses
it for its HUD and Start/Resume/Restart menus; Orchard Guard uses it for build,
upgrade, wave and restart buttons. Existing immediate helpers remain compatible.

```rust
use ferravex_render::ui::{Ui, UiRect, Viewport, TextStyle, ButtonStyle};
let mut ui = Ui::new(); // Keep this instance across frames.
let viewport = Viewport::fit([1280., 720.], [1280., 720.], [0.; 4]).unwrap();
ui.begin(viewport);
ui.text_fmt(UiRect { min: [24., 20.], size: [220., 24.] },
    TextStyle::default(), format_args!("Health {}", 100)).unwrap();
ui.button(1, UiRect { min: [24., 60.], size: [180., 44.] },
    "Resume", true, ButtonStyle::default()).unwrap();
ui.finish(true); // Layout first, then process the current input journal once.
ui.paint();
if ui.activated(1) { /* resume */ }
```

Use logical display pixels, not framebuffer/device pixels. `Viewport::fit`
letterboxes a design size inside the available screen, with left/top/right/bottom
safe-area insets. The caller supplies OS safe areas; automatic mobile insets
remain a native-host concern. `unproject` rejects letterbox space and half-open
outer edges. Renderer-independent `Anchors` and `slots` support anchored/stretch
rectangles and allocation-free equal row/column layouts. Neon and Orchard share
their existing design-space viewport with world rendering; Strike keeps its
responsive screen-space HUD.

Stable numeric IDs preserve capture and focus across frame rebuilds. Tab/right/
down advance focus; Shift+Tab/left/up go backward; Enter activates. Selected-pad
D-pad/A uses `finish_with_gamepad(enabled, player.settings().gamepad)`.
`finish_player(&mut player, enabled)` also accepts the profile’s remapped global
confirm action; the three adopting games use this adapter. Gameplay overlays can
`set_auto_focus(false)` to require an explicit focus choice before accepting, so
Orchard’s A button can still select a plot until its HUD receives focus. Navigation
is edge based, without held-key repeat. Hotplugged held A is suppressed for the
connection frame. Mouse and touch require press and release on the same enabled,
visible widget. One touch owns capture; other fingers cannot release it. Removed,
disabled, fully clipped, cancelled, unfocused or overflowed input cancels capture.
Disabled widgets occlude clicks underneath. The F2 controls editor blocks menu
input. Navigation and Enter have fixed host bindings in addition to a profile’s
remapped confirm action. Gameplay action rebinding remains in PlayerInput. Space remains available to games such as Orchard's wave shortcut.

Call `modal(blocker_id)` before adding modal buttons to remove lower controls
from interaction. Add its covering panel after lower draw commands. IDs must
be unique within the current interaction scope, including the blocker. A
nonfocusable blocker absorbs outside presses without activating another button.
Rebuild/finish/paint a UI once per render frame. Keep game actions behind an
appropriate input context; UI is not a simulation scheduler.

The original 5×7 font is drawn as horizontal bit runs, reducing geometry compared
with one rectangle per lit pixel. Text supports alignment, shrinking to fit,
explicit newlines, character wrapping, clipping and an optional shadow. UTF-8
boundaries are preserved; unsupported characters render `?`. It does not provide
complex-script shaping, rich text, editable fields, localization or a screen-reader
bridge. The DOM F2 editor remains the browser's accessible controls interface.

Each Ui preallocates 1024 commands/widgets and a 32 KiB text arena. A frame admits
at most 196608 estimated vertices and 16 clip levels; oversized or invalid commands
return UiError without partially registering a button or retaining formatted text.
`text_fmt` writes directly into the arena. Actual glyph geometry is clipped before
submission. Renderer stream buffers may grow on initial/worst-case use; reused
UI buffers do not grow during a frame. Each Ui owns 213504 bytes (about 209 KiB) of command,
text and widget capacity on this 64-bit host, in addition to the renderer stream.
Use fewer instances rather than one instance per entity. No textures, external
fonts, layout libraries or third-party packages are required.
