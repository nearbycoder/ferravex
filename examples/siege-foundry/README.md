# Siege Foundry

A low-poly 3D demolition range built with Ferravex's fixed clock, Rapier 3D, cached meshes, and portable lit material. Bring down all six rust-red blocks with eight iron cannonballs. Supporting sandstone blocks are physical bodies too: removing a low support can collapse an entire tower.

```sh
cargo run --release -p siege-foundry
cargo test -p siege-foundry
cargo run --release -p siege-foundry -- --autoplay --smoke-frames 480 --screenshot siege-foundry.png
```

- **Left / Right**: change cannon heading.
- **Up / Down**: raise / lower the barrel.
- **Space** or **Fire**: shoot. The dotted line previews the gravity-driven arc.
- **Drag in the scene**: aim with a mouse or touch; use the Fire button to shoot.
- **R** or **Rebuild**: restore the range and ammunition.

Rust-red blocks count when they fall, rotate far enough, or move out of their original stack. They turn muted green once counted. Wait for the final shot to settle before an out-of-ammunition verdict. The physical simulation runs at 120 Hz; transforms interpolate between ticks, and cannonballs use continuous collision detection. A bounded eight-projectile allocation avoids unbounded debris growth.

Tests exercise an untouched stable stack, shot cooldown and ammunition exhaustion, CCD configuration, the final-shot grace period, zero-tick input buffering, and an autoplay run that actually knocks down every target through normal ballistic contacts. Autoplay only aims and fires; it never moves targets or awards points directly.

The range is a small single-player example. Blocks topple as rigid pieces rather than fracturing, and the arc preview ignores contact deflections. No audio, persistent high scores, or network mode is included. Rendering and controls support native and WebAssembly builds; touch uses Macroquad's touch-to-mouse compatibility.
