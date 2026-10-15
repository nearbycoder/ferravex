# Strike / Dockyard

The restored low-poly shooter prototype uses the shared authoritative FPS simulation and original Ferravex immediate renderer. Procedural shipping containers, cranes, operators and the rifle are cached meshes with flat normals and restrained directional lighting/fog. No external models, textures, fonts or runtime libraries are loaded.

This recovery is local and unpublished. The native interactive host currently supports Linux X11/GLX; the browser uses the shared WebGL2 host. Native Windows/macOS/iOS game hosts are not implemented, and recovery-wide gameplay validation is pending.

## Play locally

```sh
cargo run --release -p strike
cargo run --release -p strike -- --play --smoke-frames 360 --screenshot /tmp/strike.png
```

WASD moves, mouse aims, left click fires, Space jumps and R reloads. Escape toggles the menu, Tab shows the scoreboard and F3 toggles diagnostics. The browser includes touch controls, but physical-device behavior needs testing. Offline practice includes opponents using the same movement, collision and weapons simulation as the player.

## Native network mode

Start the local authority and connect a Linux client:

```sh
cargo run --release -p ferravex-fps --bin ferravex-server -- --bind 127.0.0.1:4000
cargo run --release -p strike -- --connect 127.0.0.1:4000 --play
```

The 64 Hz authority owns movement, collision, shots, health, ammunition, reload and respawns. Clients predict and reconcile local movement and interpolate remote players. UDP is a trusted-development/LAN transport without encryption or account authentication. The browser Strike build provides offline practice; it does not bridge native UDP. Courtyard's separate browser WebRTC session is available in the other example.

This is a small FPS slice, not a finished tactical multiplayer game. Buy/plant/defuse rules exist separately in `ferravex-play` and are not integrated here. Production networking, matchmaking, native audio and broader platform hosts remain unfinished.

## Validation

```sh
cargo test -p ferravex-fps -p ferravex-net -p strike
cargo check -p strike --target wasm32-unknown-unknown
```

Rerun these against the recovered code and test menu/input, pointer capture, shooting/reload, death/respawn and network behavior separately from a short screenshot smoke run. Native `--capture-dir directory` writes frame images for local recording workflows; encoding a video is a separate step.
