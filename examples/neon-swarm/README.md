# Neon Swarm

A complete compact 2D twin-stick survival shooter built with Ferravex. Clear three escalating waves of drones in an industrial containment arena. Fast chasers and armored brutes flank four pieces of bullet-blocking cover. Defeated enemies drop health after every fifth kill. Dash through danger to survive; a completed run ends with a sector-secured screen.

```sh
cargo run --release -p neon-swarm
cargo test -p neon-swarm
cargo run --release -p neon-swarm -- --autoplay --smoke-frames 1800 --screenshot /tmp/neon-swarm.png
```

Enter/click starts. WASD or arrows move, mouse aims, left click fires, Space/left Shift dashes. Escape pauses, R restarts. Touch: the lower-left stick moves, right-side touch dashes, and weapons automatically aim/fire at the nearest drone. End screens also accept Enter/click/touch to restart.

Ferravex `FixedClock` runs simulation at 120 Hz independently of rendering. Ferravex's hecs world stores enemies, bullets, and pickups in separate component queries. Projectile sweeps select the nearest hit and compare cover occlusion, so fast shots cannot tunnel through enemies or scenery. Circle collision and tangent steering let drones flow around cover. The deterministic autoplay controls the same movement, dash, and weapon inputs used by a person; its headless test must win all three waves.

The game needs no downloaded textures, fonts, audio, or models. Flat geometry, restrained cyan/coral silhouettes, sparse floor joints, and minimal combat indicators keep targets readable. No sound is included in this slice.

Engine gaps exercised: swept segment/circle and segment/AABB queries were promoted to `ferravex_core::collision` and this game now uses them. Circle/rectangle character movement remains game-specific. This game now uses the shared `ferravex_render::canvas::Canvas2D` helper added during this game suite to unify aspect-preserving drawing and mouse/touch transforms. Visual testing caught an inverted Y axis and high-DPI viewport mismatch; the shared logical-pixel camera fixes both.
