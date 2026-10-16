# Saves, settings and game records

`ferravex_play::persistence` (also `ferravex_engine::persistence`) owns a bounded,
versioned binary save container, typed save contracts, presentation preferences
and personal records. `ferravex_render::storage` supplies the same Slot API over
native files and browser localStorage. All code is first-party; Rust std, OS file
APIs and browser storage are the platform boundary.

```rust
use ferravex_play::persistence::RecordBook;
use ferravex_render::storage::Slot;
let mut slot = Slot::open("my-game", "records", RecordBook::default()).unwrap();
let mut next = slot.value().clone();
next.record("level-one", 1, 1200, Some(45_000_000)).unwrap();
slot.commit(next).unwrap(); // Explicit event boundary; no background per-frame I/O.
```

Keep the Slot across frames. `value`, `generation` and `status` inspect memory.
Loading occurs once at construction; commits perform validation, serialization
and one storage write. A failed write keeps the last accepted value. The Records
convenience type additionally retains an unsaved session best and exposes its
error. It saves only completed attempts; ordinary frames do not poll storage.
Storage operations are synchronous and can stall a save/load frame. They are not
included in warmed gameplay timing, and this is not an asynchronous save service.

## Typed data and migrations

Implement SaveData with a stable nonzero SCHEMA ID, current VERSION, MAX_BYTES,
write and read methods. write must validate the application value; read must
validate decoded fields and explicitly migrate supported older versions. Use
Document's typed integer, boolean, finite double, UTF-8 text and byte fields.
U64/I64 values never pass through floating point. Version-1 presentation settings
(master/mute/quality) migrate to version 2 by supplying unity audio-bus gains.
Version 3 adds visual style; version 4 adds independent resolution, shadows and
local-light details. Versions 1–3 migrate with device resolution and preset
shadow/light defaults, preserving the existing fields.
Newer application versions and wrong schema IDs are rejected.

Slot construction validates its default, then stages a decoded replacement.
Rejected, oversized, unavailable or future save data leaves defaults usable and
protects the original storage from automatic overwrite. Inspect LoadStatus and
handle commit errors; explicit reset replaces the rejected save. For validation
that requires a current level or ruleset, use validate_loaded: it can construct a
new game transactionally and protects the file when that validation fails.

The FXSV0001 little-endian envelope contains a schema u64, application version
u32, generation u64, payload length u32, field count u16 and zero reserved u16.
The 36-byte header is followed by canonically sorted fields and an 8-byte FNV-1a
checksum. Each field is key-length u8, key bytes, tag u8, value-length u32 and
value bytes. Tags 0..5 are bool, u64, i64, f64, UTF-8 text and bytes. Decoder checks
bounds, checksum, ordering, duplicate keys, tags, exact lengths and finite values
before allocating decoded values. Checksums detect corruption; they do not
authenticate a player or establish an authoritative multiplayer score.

An archive is at most 8 MiB including framing, with at most 128 fields, 64-byte
ASCII field keys and 64 KiB per text value. Typed codecs can choose smaller limits:
preferences 1 KiB, records 16 KiB, Orchard progress 4 KiB, Rally ghost 80 KiB.
Browser transport applies the requested typed limit before base64 decoding.
Native reads validate file length and bound growth even if a file changes while
being read. MAX_SAVE_BYTES bounds the archive, not total process memory during
encoding/decoding. Large blob values and encoded buffers coexist temporarily.

## Platform storage

Owner/slot names are 1..64 lowercase ASCII letters, digits, underscores or hyphens;
path separators, dots and Windows reserved device names are rejected. Native
files live in `<root>/<owner>/<slot>.fxsave`:

- Linux: `$XDG_DATA_HOME/ferravex/saves`, or `~/.local/share/ferravex/saves`.
- macOS/iOS: `~/Library/Application Support/ferravex/saves` inside the host sandbox.
- Windows: `%LOCALAPPDATA%/ferravex/saves`, falling back to APPDATA.
- `FERRAVEX_SAVE_DIR` selects an explicit absolute root. FileStore::new also
  supports an application-owned absolute root for tools and tests.

Writes create an exclusive temporary file in the same directory, write and sync
it, then replace the destination. Unix also syncs the directory. Windows uses
our direct MoveFileExW binding with replace-existing/write-through flags; its
runtime behavior is untested on this Linux host. DurabilityUncertain means the
complete new file replaced the old one but directory syncing failed; Slot then
accepts the committed value while returning that error. Interrupted or failed
writes before replacement do not replace the last complete destination.

Browser keys are `ferravex.save.v1.<game>.<slot>` with an fxsv1 base64 prefix.
The first-party bridge scopes calls to the current game, checks memory ranges,
bounds payloads, snapshots a read across Wasm memory growth and reports quota or
unavailable-storage errors. localStorage setItem supplies atomic replacement;
no external storage service is used. Private browsing/eviction can still remove
local data. Slots assume one writer; simultaneous processes/tabs use last-writer
wins rather than a cross-process transaction or cloud synchronization.

Native --autoplay, --frames and --smoke-frames sessions and browser autoplay use
transient save/presentation storage so gameplay validation cannot replace a user's
records or presentation preferences. Applications/tools can explicitly select PlatformStore::transient.
The legacy FXIN control-profile format and existing control storage keys/paths
remain compatible; their native writes now share the atomic file implementation.
FXIN profiles retain their separate, explicit-edit saving behavior.

## Settings in the examples

All fifteen examples expose audio/graphics settings in the existing F2
Devices tab (after the device fields; page 2 at desktop size): master, effects, ambience, music and UI volume, mute, and graphics
quality (game default/low/medium/high). PlayerInput loads them once and applies
changes immediately. The graphics preference overrides authored game quality;
an explicit FERRAVEX_QUALITY or browser ?quality= override takes precedence and
is not saved as a new preference. Existing bindings, sensitivity, inversion and
controller selection remain in the FXIN profile.

All browser hosts also expose the shared Details panel for lighting preset,
resolution, shadows, local lights and visual style. Its validated changes use
the same per-game Rust preference slot and commit only on edits. Opening it
pauses gameplay through the shared controls context; closing clears held input.
Disabled controls describe features the current renderer or scene does not use.
Reset details leaves audio and bindings intact. A failed write retains the
session value and displays “Session only · save failed.”

PlayerInput::preferences returns a copy; set_preferences applies/commits a
validated replacement, save_preferences retries an unsaved value, and
reset_preferences explicitly restores defaults. Failed persistence leaves
session changes working and the settings panel reports that they were not saved.
Devices Reset also replaces a rejected preference file. Changes from an external
sound button are detected and saved at the next PlayerInput sample. Restored mute
is respected by implicit browser gestures/focus; the explicit Sound button can
enable it again. Browser audio still needs a user gesture.

## Game adoption

Pocket Rally saves its best-lap time and original 20 Hz ghost, up to 6000 poses
(the existing five-minute recording cap). Ghost poses/angles must be finite and
bounded and are tied to a course/rules fingerprint. Restart preserves the best;
a matching future session restores both the best-time HUD and the actual ghost.

Marble Run stores best completion time and crystal score by scene-source hash;
Neon Swarm stores sector best score and successful completion time. Orchard
stores attempts by scene-source hash and resumes safe checkpoints between waves,
including towers/upgrades, money, lives, completed waves, counters and elapsed
time. Restore validates tower shapes, wave counts, lives and earned/spent economy
against that exact level before creating a new game. It rebuilds routes/IDs and
captures ready tower cooldowns as zero; it is not a mid-combat physics snapshot.
Build/upgrade/selection changes and cleared waves checkpoint once. Victory clears
the checkpoint; restart explicitly starts fresh while retaining records. An
interrupted/lost wave can resume the last safe checkpoint. Source editing starts
a fresh game; mismatched source hashes are not applied. Whitespace changes also
change that conservative hash. Only one Orchard checkpoint is retained.

RecordBook holds at most 24 distinct level/rules contexts, exact u64 best scores,
fastest successful times up to seven days and completed-attempt counts. It rejects
capacity and integer overflow without changing an existing record. No per-frame
storage queries, filesystem watchers or automatic save retries run in gameplay.

Local coverage includes malformed/checksum-valid save rejection, version
migration, file replacement/failure, contextual protection, actual Rally lap ghost
roundtrip and Orchard wave/campaign restore. Real release Wasm tests exercise all
fifteen examples' preference editing/reload/mute and idle I/O, rejected-file reset,
quality precedence and actual Orchard build/checkpoint/reload/restart. Available
Linux and browser checks are recorded in PERFORMANCE.md; unavailable Apple/Windows
runtime tests are not claimed.
