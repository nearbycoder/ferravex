# Replays, ghosts and desync debugging

`ferravex_play::replay` is the first-party timeline implementation, also exposed
through `ferravex_engine::replay`. The old `Replay` / `Frame` API and FXRP0001
input-log format remain available; new sessions use FXRT0001.

## Watch a Strike practice run

Open `/play/strike/?replay-tools` for an optional accessible DOM panel with
record/stop, replay/live, pause, one-second seek, ghost, import and export controls.
Start practice using its normal Play button. The tools do not start or resume the
live match automatically. Keyboard shortcuts are rebindable through F2:

| Default | Action |
|---|---|
| F4 | Start recording; stop and save |
| F5 | Watch the last recording; return to the frozen live match |
| F6 | Show a presentation-only wireframe ghost from the clip's beginning |
| P | Pause/resume playback |
| Left / Right | Seek backward/forward one second |
| F3 | Show diagnostics, including replay help |

The last completed recording persists in the typed `strike/last-replay` save slot.
A failed save keeps the clip in memory and displays the error. Rejected files are
protected by the save layer; import does not overwrite the saved clip. Export
stops an active recording and downloads the complete prefix as `.fxrt`. Each
session is bounded to 19,200 ticks (five minutes at 64 Hz), 32,768 events, 152
checkpoints and a 4 MiB payload arena. Whichever limit is reached first stops
recording cleanly. File import is capped at 8 MiB.

Native CLI examples (available Linux host):

```sh
cargo run --release -p strike -- --play --record /tmp/practice.fxrt --smoke-frames 600
cargo run --release -p strike -- --play --replay /tmp/practice.fxrt --smoke-frames 600
```

The CLI exports after the bounded run finishes. For interactive sessions, stop
with F4 to save the clip before closing. Smoke/autoplay sessions use transient
save slots; explicitly requested CLI export still writes the selected file.

## Timeline and checkpoint contract

Create a `Recorder` with a schema/version, level/rules context hash, tick rate,
seed, capacity limits, initial checkpoint and state hash. Push exactly one row
after each simulation tick. Supply accepted authority events and periodic full
checkpoints. All recorder metadata and payload capacity are reserved at creation;
validated pushes and ghost sampling allocate nothing. Finalizing shrinks buffers;
encoding, decoding, checkpoint restoration, save I/O and import/export can allocate.
These operations are not part of the zero-allocation promise.

Implement `ReplayWorld` for a game. Restore must validate/stage before replacing
state. Step consumes recorded input once, simulates once and compares expected
events; it must never apply their damage a second time. Hash all state that can
influence later simulation, including RNG state and queues. `derive_seed` supplies
stateless tick/channel seeds; games still own the choice of random algorithm and
what their checkpoint contains.

`Playback` checks schema, version, tick rate and context before starting. Seeking
restores the nearest earlier checkpoint, then `pump_seek` advances at most the
caller's tick budget. Every tick checks its expected hash. Hash mismatches report
tick/expected/actual and halt; game-step/state errors poison the cursor until a
successful seek/restart. Presentation should cut cameras, animation and transient
effects on seeks; fast-forward must not play audio or award achievements.

Strike uses a separate playback `Match`; its live practice authority and AI freeze.
It records the sanitized commands of every player rather than rerunning bot
perception during playback. Complete FXMC0001 checkpoints include arena, players,
weapon/jump/respawn timers, rewind poses and life generations. The state hash uses
the same canonical body without allocating. Accepted shot/hit/kill/respawn events
are compared against newly simulated output. Replay camera, effects, and HUD use
the playback match. Seeks advance at most 64 ticks per rendered frame and suppress
seek-time sounds/effects. Saved clips can play after a fresh launch.

## Shared 2D and 3D ghosts

`ghost::GhostTrack` stores bounded tick/position/quaternion samples, with context
and tick rate. Sampling interpolates sparse positions and shortest-arc rotations;
teleport samples prevent blending across discontinuities. Integer ticks avoid
long-session precision loss. Ghosts disappear outside their recorded interval and
never participate in collision, damage, scoring or network authority.

Rally converts its persisted 20 Hz best-lap poses to the shared track, starts each
lap with an initial pose, and interpolates ghost motion between samples. Its
existing FXSV best-lap format remains compatible. Strike records a 64 Hz 3D track;
loaded clips build a ghost incrementally, at most 64 replay ticks per frame. Ghost
tracks are limited to 120,000 samples; a longer imported clip can play but does not
build a ghost. F6 restarts ghost time alongside the current practice run.

## Bounds and practical limits

FXRT0001 stores fixed metadata tables followed by canonically packed byte spans
and an FNV-1a checksum. Decode checks file/count limits, exact length, reserved
fields, contiguous ticks, ordered checkpoints, event ownership, span packing and
checksum before allocating decoded output. No trailing, aliased or unused bytes
are accepted. Payload meaning remains the game adapter's responsibility. Files
are data, never executable code. Checksums detect corruption, not authenticity.

This is offline practice replay, not live multiplayer recording or video capture.
It does not promise bitwise simulation equivalence across CPU architectures or
future rule versions; a difference stops playback visibly. Changing simulation
rules requires a schema/version/context change. Platform audio phase, renderer
resources, bot internals (replaced by recorded commands), and UI preferences are
not checkpointed. Recording reserves several MiB up front; synchronous save and
export can stall. The default performance harness measures ordinary warm frames,
not recording overhead, replay restoration or durable I/O latency.
