# Sky Courier

Restored locally with first-party Ferravex APIs. Native play currently uses Linux X11/GLX; browser play uses the shared WebGL2 host. This recovery has not been published, and prior backend test results must be rechecked.

A small 3D coastal flight game built with Ferravex. Pilot a low-poly floatplane through five ordered amber navigation rings, then slow down and fly low over the marked island landing pad to deliver the north coast mail before the 90-second window closes. All geometry is procedural and cached at startup; no external assets are required.

```sh
cargo run -p sky-courier --release
```

| Input | Action |
| --- | --- |
| A / D or left / right | Turn and bank |
| W / S or up / down | Climb / dive; release to level out |
| Space | Boost |
| Shift | Slow flight; required for delivery |
| R | Restart |
| Touch left stick | Turn and climb / dive |
| Hold touch on right side | Slow flight |

Follow the amber ring. Cleared rings disappear; future rings are muted. On the delivery approach, hold Shift (or the right side on touch) and fly through the airspace above the coral cross at less than 18 metres/second. The speed display uses knots. The final low pass releases the parcel automatically.

Ocean, island, ceiling, and map-boundary impacts cost one of three hull points and five seconds. Surviving impacts recover you at the previous beacon with temporary collision immunity. Three impacts or running out of time ends the flight. The same recovery rules apply to human and automated play.

## Validation

```sh
cargo test -p sky-courier
cargo clippy -p sky-courier --all-targets -- -D warnings
cargo check -p sky-courier --target wasm32-unknown-unknown
cargo run -p sky-courier -- --autoplay --smoke-frames 3000 --screenshot /tmp/sky-courier.png
```

Autoplay steers through the normal turn/climb/slow inputs; it does not teleport, grant immunity, or change the timer. The fixed simulation runs at 120 Hz with bounded catch-up. Tests cover ordered progression, a complete delivery, crash recovery and failure, timeout and restart, slow-flight delivery, and swept beacon capture.

This is an arcade flight slice, not an aerodynamic simulator. Collision volumes conservatively approximate island terrain; decorative houses and trees do not have separate colliders. The delivery is a low flyover rather than a wheel-contact landing. There is no audio, networking, or saved progress. The restored runtime implements Linux and WebAssembly hosts; rerun rendering and input checks against the current build. Native Windows/macOS/iOS hosts are not implemented.
