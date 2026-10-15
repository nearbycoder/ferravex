# 2D Physics Lab

An interactive laboratory for Ferravex's original rotational 2D solver. A procedural stack of boxes and balls sits inside fixed platforms and boundaries. The simulation uses a 120 Hz fixed clock, interpolated body poses, restitution and bounded ball CCD, with a maximum of 240 dynamic bodies.

The recovery is local and unpublished. Native play requires Linux X11/GLX; browser play uses the original shared WebGL2 host. Other native interactive hosts are not implemented. Compilation and visible behavior need fresh validation after solver changes.

```sh
cargo run --release -p physics-2d
cargo run --release -p physics-2d -- --smoke-frames 360 --screenshot /tmp/physics-2d.png
cargo check -p physics-2d --target wasm32-unknown-unknown
```

Click inside the scene to add the selected shape. Press 1 to select balls and 2 to select boxes; right click adds a box. Space pauses and R rebuilds the original scene. The game uses the same first-party renderer and built-in glyphs as the other restored examples, without external assets.

This sandbox has no win condition. Validate contacts, visible rotations, stable resting bodies, spawn limits and pause/reset. It does not establish general physics accuracy: dynamic capsules, joints, mesh collisions and swept box time-of-impact are unsupported. See the `ferravex-physics` README for the solver's precise scope.
