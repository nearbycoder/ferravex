# Shared runtime audio

The first-party audio core lives in `ferravex_play::audio`; the engine re-exports
it. `ferravex_render::audio` supplies runtime playback and browser/native output.
There are no external Cargo audio packages or copied sound assets. The original
procedural cue bank contains footsteps and impacts for stone, metal, wood, sand,
grass and water, plus weapon, reload, jump, pickup, outcome, click and motor cues.
At 24 kHz the generated bank occupies 616,896 decoded sample bytes; those samples
are generated once, rather than shipped in every game module.

## Runtime API

```rust,no_run
use ferravex_render::audio::{self, Cue, GameAudio, Surface};
let mut sound = GameAudio::new(); // preloads built-in sounds outside playback
sound.cue(Cue::Pickup, 0.5);
sound.walk([0.0, 0.0, 2.0].into(), true, true, Surface::Stone, 1.65);
sound.motor(20.0, 40.0, true);
audio::set_master_gain(0.7);
audio::set_muted(false);
```

Games decide which simulation events deserve sound. `GameAudio::walk` uses
actual grounded travel, rather than raw held movement, and discards teleports
and air movement. `motor` updates a single loop's pitch/gain and releases it when
inactive. `cue` is local; `at` uses positional attenuation. Set the listener pose
with `audio::listener(position, right)` for world emitters. Surface selection is
an authored game decision, not an inferred collision material.

`Sound::wav` decodes PCM16 mono/stereo WAV once. `audio::play_sound` accepts a
validated sound and `VoiceParams`, returning a generational voice handle for
`set_params`, `playing` and `stop`. Custom sounds do not require generating the
built-in bank. Keep source owners alive during playback if the final sample Arc
must not be freed while mixing. The lower-level `Mixer` can render into a host's
own sample buffer without using the renderer runtime.

Voices have pitch, loop state, position, gain, priority, gain smoothing and an
Effects/Ambience/Music/UI bus. Equal-priority overload drops the new sound;
higher-priority sounds can replace the oldest lowest-priority voice. Stale
handles cannot control the replacement. Nonpositional sounds preserve stereo;
positional sources downmix to mono before equal-power panning and inverse-square
falloff. Rate changes preserve fractional source positions. Explicit stop fades,
initial/changed gain ramps and one-shot tails avoid abrupt cuts. Final output is
finite and clamped. Resampling is linear; HRTF, acoustic occlusion, streaming
compressed formats and high-quality pitch conversion are outside this iteration.

The core supports 1–128 voices and 8–96 kHz output. The runtime uses 64 voices
and limits each host pull to 4,096 frames. Registration, decoding and bank
creation allocate; validated voice insertion/control and mixing reuse bounded
storage. The allocation regression covers repeated starts, parameter changes,
stealing/retirement, render and silent advancement.

## Browser output

`web/audio.js` connects the shared Rust PCM pull API to an original AudioWorklet
and bounded stereo ring. Output begins after a game gesture or the Sound button.
Worklet requests carry reset epochs, so late PCM cannot cross mute, settings or
focus changes. The Rust mixer also drops transient voices at these boundaries
and restarts surviving loops with a gain ramp, preventing old one-shot tails
from resuming after a suspended page. The normal 1,536-frame target queue at 48 kHz is 32 ms of queued PCM, not a
measurement of total hardware latency. The queue adapts to at least two output quanta within its 4,096-frame capacity. Missing
PCM produces silence with an underrun count; producer overflow is bounded.

Rust mixing and buffer transfer run on the browser main thread; playback runs
in the worklet. Long main-thread stalls can underrun this queue. Device-rate
conversion uses the actual requested output rate. Worklet processing uses the
supplied output-array length instead of assuming a permanent render quantum.
Browser AudioWorklet requires a secure context, and Web Audio activation follows
browser gesture policy. See [AudioWorklet](https://developer.mozilla.org/en-US/docs/Web/API/AudioWorklet)
and [Web Audio practices](https://developer.mozilla.org/en-US/docs/Web/API/Web_Audio_API/Best_practices).

Canvas diagnostic attributes expose state, played frames, underruns, overflows,
queued frames and recent output peak. They show delivery through Web Audio;
they do not prove a physical speaker is unmuted. Current live Emberline checks
observed nonzero output and zero underruns/overflows, plus mute and settings
suspension/resumption. All fifteen browser examples use the bridge and emit game-specific cues. The
thirteen shared-renderer examples also use Linux output. Owned Orbit remains a
headless native reference; Courtyard retains a headless PCM buffer for callers.

## Linux output and capture

Linux dynamically loads the system ALSA library through original Rust FFI.
Device access stays on a worker thread with a three-block PCM channel. The game
thread reserves space before mixing and never waits for a device write.
Nonblocking writes handle partial progress, bounded recovery attempts and
shutdown checks. Missing devices leave gameplay running with silent advancement.
The requested ALSA buffer latency is 10 ms; actual latency depends on the system
and device. Failed native sinks retry after a bounded two-second delay.
Windows uses an original WinMM PCM16 sink and Apple platforms use AudioQueue
float stereo, each with three device buffers and the same three-block application
queue. Callbacks only return buffer credit; mixing stays in Rust. These platform
sinks pass Rust cross-checks on Windows/macOS but await runtime verification;
iOS SDK/device checks remain excluded. Native suspension releases device queues
and resume opens a fresh sink while preserving capture/mixer ownership. A Linux
null-device check verifies resumed capture appends rather than truncates.

Native interactive games use the default ALSA PCM. `FERRAVEX_AUDIO_DEVICE`
selects another system device and `FERRAVEX_AUDIO=off` disables device output.
Fixed smoke runs mix offline using simulation time. For a bounded PCM16 capture:

```sh
FERRAVEX_AUDIO_CAPTURE=target/emberline.wav target/release/emberline --autoplay --smoke-frames 360
cargo run -p ferravex-play --example audio_bank --offline --locked -- target/audio-bank.wav
cargo test -p ferravex-render audio::native::tests::system_null --offline --locked -- --ignored
node web/tests/audio.mjs
```

The native ALSA test exercises both null and file PCMs and verifies bytes passed
to the system output path. A six-second Emberline capture contains 288,000 stereo
frames at 48 kHz with nonzero output. WAV captures are capped at 8,388,608 frames.
Capture files and measured reports stay under ignored `target`; they are not
part of game download sizes.
