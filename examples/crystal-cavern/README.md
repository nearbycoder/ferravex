# Crystal Cavern

A compact first-person exploration puzzle in a warm, faceted cave. Recover three mint crystals from the side chambers, awaken altar **I**, then altar **II**, and walk through the opened seal into daylight. The altar markings are carved bars on their tops. Press **E** nearby to interact; aiming directly at the object is unnecessary.

```sh
cargo run -p crystal-cavern
```

| Control | Action |
| --- | --- |
| WASD | Move and strafe |
| Arrow keys or left-button drag | Look around |
| E | Recover a crystal / activate an altar |
| R | Restart the expedition |
| H | Toggle the controls hint |

The browser build uses the same controls and does not require pointer lock. Keyboard/mouse is currently required; touch controls and gamepad support are not implemented.

The example exercises Ferravex's fixed simulation clock, shared AABB segment queries, flat-normal meshes, portable lighting/fog shader, and smoke-run harness. Walls and the sealed gate use swept collision, interactions require line of sight, and one-shot interaction presses remain latched until a simulation tick. Cached procedural meshes need no downloaded textures or models. This is a single handcrafted level with cosmetic crystal glow (full ambient shading), no dynamic point lights or audio.

```sh
cargo test -p crystal-cavern
cargo clippy -p crystal-cavern --all-targets -- -D warnings
cargo check -p crystal-cavern --target wasm32-unknown-unknown
cargo run -p crystal-cavern -- --autoplay --smoke-frames 6000 --screenshot /tmp/crystal-cavern.png
```

Autoplay navigates the map with movement, turning and interaction inputs through the same simulation as a player. The headless completion test verifies the whole expedition, alongside collision, gate prerequisites, interaction visibility and restart tests.
