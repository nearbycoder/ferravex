# Neon Swarm

Restored locally with first-party Ferravex APIs. Native play currently uses Linux X11/GLX; browser play uses the shared WebGL2 host. This recovery has not been published, and prior backend test results must be rechecked.

A complete compact 2D twin-stick survival shooter built with Ferravex. Clear three escalating waves of drones in an industrial containment arena. Fast chasers and armored brutes flank four pieces of bullet-blocking cover. Defeated enemies drop health after every fifth kill. Dash through danger to survive; a completed run ends with a sector-secured screen.

```sh
cargo run --release -p neon-swarm
cargo test -p neon-swarm
cargo run --release -p neon-swarm -- --autoplay --smoke-frames 1800 --screenshot /tmp/neon-swarm.png
```

Enter/click starts. WASD or arrows move, mouse aims, left click fires, Space/left Shift dashes. Escape pauses, R restarts. Touch: the lower-left stick moves, right-side touch dashes, and weapons automatically aim/fire at the nearest drone. End screens also accept Enter/click/touch to restart.

Ferravex `FixedClock` runs simulation at 120 Hz independently of rendering. Ferravex's original `ferravex-ecs` world stores enemies, bullets, and pickups in separate component queries. Projectile sweeps select the nearest hit and compare cover occlusion, so fast shots cannot tunnel through enemies or scenery. Circle collision and tangent steering let drones flow around cover. The deterministic autoplay controls the same movement, dash, and weapon inputs used by a person; its headless test must win all three waves.

The game needs no downloaded textures, fonts, audio, or models. Flat geometry, restrained cyan/coral silhouettes, sparse floor joints, and minimal combat indicators keep targets readable. No sound is included in this slice.

Engine gaps exercised: swept segment/circle and segment/AABB queries were promoted to `ferravex_core::collision` and this game now uses them. Circle/rectangle character movement remains game-specific. This game now uses the shared `ferravex_render::canvas::Canvas2D` helper added during this game suite to unify aspect-preserving drawing and mouse/touch transforms. The helper maps drawing and pointer/touch coordinates through the same aspect-preserving canvas. Native Linux currently uses physical window pixels; automatic desktop DPI scaling is not implemented.

Kills now build a 3.5-second score chain, increasing the multiplier every three kills up to x5. Taking a hit breaks the chain. The HUD and best-run record show points; old rules records remain separate.

Each kill earns 10 XP. Levels 2–4 permanently improve the current run’s shot damage (24 → 28 → 32 → 36). The shared progression curve and capped upgrade bank own advancement; restart resets the build.

Health drops attract within 130 pixels when unobstructed; cover blocks attraction. Full-health players leave drops available for later instead of consuming them.

Spawns now select the farthest eligible perimeter location at least 260 pixels from the player and clear of live enemies. Unsafe locations defer admission before spending wave credits.
