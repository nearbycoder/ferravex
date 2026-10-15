# Strike gameplay

[Watch the 15-second recording](https://ferravex.com/recordings/strike-gameplay.mp4).

The recording shows the native Ferravex Strike example running the shared FPS simulation with scripted player movement, aiming and firing. It is a silent 60 fps capture at 1440 × 900.

To capture another run:

```sh
cargo run --release -p strike -- --play --smoke-frames 900 --capture-dir target/capture
ffmpeg -framerate 60 -i target/capture/%06d.png -c:v libx264 -crf 23 -pix_fmt yuv420p -movflags +faststart target/strike-gameplay.mp4
```
