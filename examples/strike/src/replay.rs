//! Opt-in practice recording. Live and playback authority never share state.

#[cfg(test)]
mod polish_tests {
    use super::*;
    use ferravex_render::{
        input::{ActionKind, Binding, InputCode},
        prelude::*,
    };

    #[test]
    fn replay_footer_keeps_status_tick_messages_and_remapped_prompts_readable() {
        let mut input = PlayerInput::transient("journey200-196-replay");
        for (name, key) in [("replay_pause", KeyCode::P), ("replay", KeyCode::F5)] {
            input
                .global_action(
                    name,
                    name,
                    ActionKind::Button,
                    vec![Binding::digital(InputCode::Key(key as u16))],
                )
                .unwrap();
        }
        for (width, height) in [(320., 480.), (390., 608.), (1280., 576.)] {
            let (panel, top) = footer_bounds(width, height);
            let baseline = crate::hud::vitals_baseline(height, false, false);
            for (x, align) in [
                (50., ferravex_render::ui::Align::Left),
                (width - 45., ferravex_render::ui::Align::Right),
            ] {
                let (vitals, style) = crate::hud::formatted_layout(
                    width,
                    x,
                    baseline,
                    crate::hud::VITALS_SIZE,
                    WHITE,
                    align,
                );
                assert!(style.shadow);
                // Frozen Ui bitmap glyphs have seven rows of size/8 pixels;
                // their shadow is displaced by half a cell in both axes.
                let cell = style.size / 8.;
                let glyph_and_shadow_bottom = vitals.min[1] + 7.5 * cell;
                assert!(glyph_and_shadow_bottom <= vitals.min[1] + vitals.size[1]);
                assert!(panel.min[1] + panel.size[1] + 4. <= vitals.min[1]);
                let prompt_shadow_bottom = top + 56. + 14. + 7.5 * (14. / 8.);
                assert!(prompt_shadow_bottom < vitals.min[1]);
            }
            assert!(top + 56. + 28. <= panel.min[1] + panel.size[1]);
            assert_eq!(panel.min[1], height - 150.);
            assert!(panel.min[0] >= 12. && panel.min[0] + panel.size[0] <= width - 12.);
        }
        for width in [288., 358., 1248.] {
            for mode in [
                "REC",
                "REPLAY",
                "PAUSED",
                "SEEKING",
                "FINISHED",
                "MISMATCH / ERROR",
                "LIVE",
            ] {
                for message in [
                    "Recording practice - stop recording to save",
                    "Replay position updated",
                    "No recording yet - record a practice run first",
                    "Replay loaded",
                    "Live practice",
                    "Fresh practice - recording tools ready",
                ] {
                    let (first, second) = message_lines(message, width);
                    assert!(
                        !second.truncated,
                        "ordinary instructions must remain complete: {message}"
                    );
                    let reconstructed = if second.prefix.is_empty() {
                        first.to_owned()
                    } else {
                        format!("{first} {}", second.prefix)
                    };
                    assert_eq!(reconstructed, message);
                    let mut offsets = Vec::new();
                    overlay_text(mode, u64::MAX, message, &input, width, |offset, args| {
                        offsets.push(offset);
                        let text = args.to_string();
                        assert!(text.lines().count() <= 2);
                        for line in text.lines() {
                            assert!(measure_text(line, None, 14, 1.).width <= width, "{line}");
                        }
                    });
                    assert_eq!(offsets, [0., 28., 56.]);
                }
            }
        }
        input.rebind("replay_pause", vec![]).unwrap();
        input
            .rebind(
                "replay",
                [
                    KeyCode::LeftShift,
                    KeyCode::LeftControl,
                    KeyCode::RightShift,
                    KeyCode::RightControl,
                ]
                .map(|key| Binding::digital(InputCode::Key(key as u16)))
                .to_vec(),
            )
            .unwrap();
        let mut prompts = String::new();
        overlay_text(
            "PAUSED",
            700,
            "Replay loaded",
            &input,
            288.,
            |offset, args| {
                if offset == 56. {
                    prompts = args.to_string();
                }
            },
        );
        assert_eq!(prompts, "Pause: Unbound\nReplay/live: Controls (F2)");
        let long = "Replay rejected: ".to_owned() + &"é".repeat(1000);
        let (first, second) = message_lines(&long, 288.);
        assert!(second.truncated && second.suffix == "...");
        assert!(measure_text(first, None, 14, 1.).width <= 288.);
        assert!(second.width <= 288.);
        assert!(long.starts_with(first)); // No UTF-8 split, invented error or status change.
    }
}
use ferravex_fps::replay::{Capture, World};
use ferravex_fps::{GameEvent, InputFrame, Match, Player, TICK_RATE};
use ferravex_play::persistence::{Document, SaveData, SaveError, Value};
use ferravex_play::replay::{
    Limits, Playback, PlaybackStatus, Recording,
    ghost::{GhostTrack, Sample},
};
use ferravex_render::{
    input::PlayerInput,
    storage::{PlatformStore, Slot},
};
use std::sync::Arc;
#[derive(Default, Clone)]
struct Clip(Option<Arc<Recording>>);
impl SaveData for Clip {
    const SCHEMA: u64 = 0x4658434c49503031;
    const VERSION: u32 = 1;
    fn write(&self, d: &mut Document) -> Result<(), SaveError> {
        d.set("present", Value::Bool(self.0.is_some()))?;
        if let Some(t) = &self.0 {
            d.set("tape", Value::Bytes(t.encode()))?;
        }
        Ok(())
    }
    fn read(d: &Document) -> Result<Self, SaveError> {
        Ok(Self(if d.boolean("present")? {
            Some(Arc::new(
                Recording::decode(d.bytes("tape")?).map_err(|_| SaveError::InvalidData)?,
            ))
        } else {
            None
        }))
    }
}
#[derive(Default)]
pub struct Session {
    capture: Option<Capture>,
    clip: Option<Arc<Recording>>,
    slot: Option<Slot<Clip, PlatformStore>>,
    pub playback: Option<Playback>,
    pub world: Option<World>,
    pub paused: bool,
    pub message: String,
    ghost: Option<GhostTrack>,
    ghost_job: Option<(Playback, World, GhostTrack)>,
    pub show_ghost: bool,
    ghost_age: u64,
    pub diagnostic_id: Option<u64>,
}
impl Session {
    fn load(&mut self) {
        if self.slot.is_some() {
            return;
        }
        match Slot::with_backend(
            "strike",
            "last-replay",
            Clip::default(),
            PlatformStore::new(),
        ) {
            Ok(slot) => {
                self.clip = slot.value().0.clone();
                self.slot = Some(slot);
            }
            Err(e) => self.message = format!("Replay storage: {e:?}"),
        }
    }
    pub fn recording(&self) -> bool {
        self.capture.is_some()
    }
    pub fn playing(&self) -> bool {
        self.playback.is_some()
    }
    /// Verified completion belongs to the active driver, independently of pause.
    fn finished(&self) -> bool {
        self.playback.as_ref().is_some_and(Playback::finished)
    }
    fn mode_label(&self) -> &'static str {
        if self.recording() {
            return "REC";
        }
        match self.playback.as_ref().map(Playback::status) {
            Some(PlaybackStatus::Faulted) => "MISMATCH / ERROR",
            Some(PlaybackStatus::Seeking { .. }) => "SEEKING",
            Some(PlaybackStatus::Finished) => "FINISHED",
            Some(_) if self.paused => "PAUSED",
            Some(_) => "REPLAY",
            None => "LIVE",
        }
    }
    /// End the old capture before replacing its authoritative world. Keep the
    /// saved clip available, but do not carry ghost time/seek state into retry.
    pub(super) fn reset_practice(&mut self) {
        self.stop_record();
        self.playback = None;
        self.world = None;
        self.paused = false;
        self.ghost = None;
        self.ghost_job = None;
        self.show_ghost = false;
        self.ghost_age = 0;
        self.diagnostic_id = None;
        self.message = "Fresh practice - recording tools ready".into();
    }
    pub fn start_record(&mut self, game: &Match) {
        if self.playing() {
            self.message = "Return to live practice before recording".into();
            return;
        }
        self.load();
        self.ghost_job = None;
        match Capture::new(game, 0, Limits::default()) {
            Ok(c) => {
                self.capture = Some(c);
                self.ghost = GhostTrack::new(
                    game.context_hash().unwrap_or(0),
                    TICK_RATE as u16,
                    Limits::default().ticks + 1,
                )
                .ok();
                if let (Some(g), Some(p)) = (&mut self.ghost, game.player(0)) {
                    let _ = g.push(sample(u64::from(game.tick), p, false));
                }
                self.message = "Recording practice - stop recording to save".into();
            }
            Err(e) => self.message = format!("Recording failed: {e:?}"),
        }
    }
    pub fn stop_record(&mut self) {
        if let Some(c) = self.capture.take() {
            let tape = Arc::new(c.finish());
            self.message = format!("Saved {} replay ticks", tape.tick_count());
            if let Some(slot) = &mut self.slot
                && let Err(e) = slot.commit(Clip(Some(tape.clone())))
            {
                self.message = format!("Replay kept in memory; save failed: {e:?}");
            }
            self.clip = Some(tape);
        }
    }
    pub fn capture(&mut self, game: &Match, commands: &[(u8, InputFrame)], events: &[GameEvent]) {
        if let Some(c) = &mut self.capture {
            if let Err(e) = c.push(game, commands, &[], events) {
                self.message = format!("Recording stopped: {e:?}");
                let why = self.message.clone();
                self.stop_record();
                self.message = why;
                return;
            }
            if let (Some(g), Some(p)) = (&mut self.ghost, game.player(0)) {
                let teleport = g
                    .samples()
                    .last()
                    .is_some_and(|s| s.position.distance_squared(p.position) > 4.);
                let tick = g.samples().last().map_or(0, |s| s.tick + 1);
                let _ = g.push(sample(tick, p, teleport));
            }
        }
        self.ghost_age = self.ghost_age.saturating_add(1);
    }
    pub fn toggle_playback(&mut self, game: &Match) {
        if self.playing() {
            self.playback = None;
            self.world = None;
            self.paused = false;
            self.diagnostic_id = None;
            self.message = "Live practice".into();
            return;
        }
        self.stop_record();
        self.load();
        let Some(tape) = self.clip.clone() else {
            self.message = "No recording yet - record a practice run first".into();
            return;
        };
        self.diagnostic_id = Some(ferravex_play::persistence::checksum(&tape.encode()));
        let mut w = World::new(Match::new(game.arena.clone()));
        let mut p = Playback::new(tape);
        match p.start(&mut w) {
            Ok(()) => {
                self.playback = Some(p);
                self.world = Some(w);
                self.paused = false;
                self.message = "Replay loaded".into();
            }
            Err(e) => self.message = format!("Replay rejected: {e:?}"),
        }
    }
    pub fn advance(&mut self) -> bool {
        if self.paused || self.finished() {
            return false;
        }
        if let (Some(p), Some(w)) = (&mut self.playback, &mut self.world) {
            if p.seeking() {
                return false;
            }
            if let Err(e) = p.step(w) {
                self.message = format!("Replay stopped at tick {}: {e:?}", p.position());
                self.paused = true;
                return false;
            }
            return true;
        }
        false
    }
    fn seek(&mut self, delta: i64) {
        if let (Some(p), Some(w)) = (&mut self.playback, &mut self.world) {
            let target = p
                .position()
                .saturating_add_signed(delta)
                .clamp(p.recording().start_tick(), p.recording().end_tick());
            if let Err(e) = p.seek(w, target) {
                self.message = format!("Seek failed: {e:?}");
                self.paused = true;
            } else {
                self.message = if p.seeking() {
                    "Seeking replay"
                } else {
                    "Replay position updated"
                }
                .into();
            }
        }
    }
    /// Returns true when presentation must cut and suppress seek-time effects.
    pub fn controls(&mut self, input: &mut PlayerInput, game: &Match, network: bool) -> bool {
        let record = input.pressed("record");
        let play = input.pressed("replay");
        let back = input.pressed("replay_back");
        let forward = input.pressed("replay_forward");
        if network {
            if record || play {
                self.message = "Replay tools currently support offline practice".into();
            }
            return false;
        }
        if record {
            if self.recording() {
                self.stop_record();
            } else {
                self.start_record(game);
            }
        }
        let was_playing = self.playing();
        if play {
            self.toggle_playback(game);
        }
        if input.pressed("replay_pause") && self.playing() {
            self.paused = !self.paused;
        }
        let seek_requested = self.playing() && (back || forward);
        if back && self.playing() {
            self.seek(-64);
        }
        if forward && self.playing() {
            self.seek(64);
        }
        if input.pressed("ghost") {
            self.toggle_ghost(game);
        }
        was_playing != self.playing() || seek_requested
    }
    fn toggle_ghost(&mut self, game: &Match) {
        self.show_ghost = !self.show_ghost;
        self.ghost_age = 0;
        if self.show_ghost && self.ghost.is_none() {
            self.load();
            if let Some(t) = self.clip.clone() {
                let mut w = World::new(Match::new(game.arena.clone()));
                let mut p = Playback::new(t);
                if p.start(&mut w).is_ok() {
                    let Ok(mut g) = GhostTrack::new(
                        p.recording().header().context,
                        64,
                        p.recording().tick_count() + 1,
                    ) else {
                        self.message = "Replay is too long for a ghost track".into();
                        return;
                    };
                    if let Some(local) = w.game.player(0) {
                        let _ = g.push(sample(p.position(), local, false));
                    }
                    self.ghost_job = Some((p, w, g));
                }
            }
        }
    }
    pub fn pump(&mut self) -> bool {
        let mut cut = false;
        if let (Some(p), Some(w)) = (&mut self.playback, &mut self.world)
            && p.seeking()
        {
            cut = true;
            if let Err(e) = p.pump_seek(w, 64) {
                self.message = format!("Seek stopped: {e:?}");
                self.paused = true;
            } else if !p.seeking() {
                self.message = "Replay position updated".into();
            }
        }
        if let Some((p, w, g)) = &mut self.ghost_job {
            let mut failed = false;
            for _ in 0..64 {
                if p.finished() {
                    break;
                }
                if p.step(w).is_err() {
                    failed = true;
                    break;
                }
                if let Some(local) = w.game.player(0) {
                    let teleport = g
                        .samples()
                        .last()
                        .is_some_and(|s| s.position.distance_squared(local.position) > 4.);
                    let _ = g.push(sample(p.position(), local, teleport));
                }
            }
            if failed {
                self.ghost_job = None;
                self.message = "Ghost rejected: recording desynchronized".into();
            } else if p.finished() {
                self.ghost = self.ghost_job.take().map(|(_, _, g)| g);
                self.ghost_age = 0;
            }
        }
        cut
    }
    pub fn ghost_sample(&self, alpha: f32) -> Option<Sample> {
        if !self.show_ghost || self.playing() {
            return None;
        }
        let g = self.ghost.as_ref()?;
        let start = g.samples().first()?.tick;
        g.sample(start.saturating_add(self.ghost_age), alpha)
    }
    pub fn overlay(
        &self,
        ui: &mut ferravex_render::ui::Ui,
        diagnostics: bool,
        input: &PlayerInput,
    ) {
        use ferravex_render::{
            prelude::*,
            ui::{TextStyle, UiRect},
        };
        if self.message.is_empty() && !diagnostics {
            return;
        }
        let mode = self.mode_label();
        let tick = self.playback.as_ref().map_or_else(
            || self.capture.as_ref().map_or(0, |c| c.tick_count() as u64),
            |p| p.position(),
        );
        let width = (screen_width() - 32.).max(1.);
        let (panel, top) = footer_bounds(screen_width(), screen_height());
        let _ = ui.panel(panel, Color::new(0.04, 0.065, 0.055, 0.88));
        overlay_text(mode, tick, &self.message, input, width, |offset, text| {
            let _ = ui.text_fmt(
                UiRect {
                    min: [16., top + offset],
                    size: [width, 28.],
                },
                TextStyle {
                    size: 14.,
                    color: Color::new(0.9, 0.85, 0.65, 1.),
                    shadow: true,
                    wrap: true,
                    ..Default::default()
                },
                text,
            );
        });
    }
    pub fn import(&mut self, bytes: &[u8], game: &Match) -> Result<(), String> {
        let t = Recording::decode(bytes).map_err(|e| format!("{e:?}"))?;
        let tape = Arc::new(t);
        let mut p = Playback::new(tape.clone());
        let mut w = World::new(Match::new(game.arena.clone()));
        p.start(&mut w).map_err(|e| format!("{e:?}"))?;
        if w.game.player(0).is_none() {
            return Err("Practice recording has no local player".into());
        }
        self.diagnostic_id = Some(ferravex_play::persistence::checksum(bytes));
        self.capture = None;
        self.clip = Some(tape);
        self.playback = Some(p);
        self.world = Some(w);
        self.paused = false;
        self.ghost = None;
        self.ghost_job = None;
        self.message = "Imported replay".into();
        Ok(())
    }
    pub fn export(&mut self) -> Option<Vec<u8>> {
        self.stop_record();
        self.clip.as_ref().map(|t| t.encode())
    }
}
// A bounded two-line notification. Long diagnostics keep an explicit ellipsis;
// their full, unchanged message is still published to the optional replay panel.
// Mode/tick and both current binding prompts always have their own complete rows.
fn message_lines(message: &str, width: f32) -> (&str, ferravex_render::ui::FittedLabel<'_>) {
    let columns = (width / 10.5).floor().max(1.) as usize;
    let boundary = message
        .char_indices()
        .nth(columns)
        .map_or(message.len(), |(i, _)| i);
    let split = if boundary < message.len() {
        message[..boundary]
            .rfind(char::is_whitespace)
            .filter(|&i| i > 0)
            .unwrap_or(boundary)
    } else {
        boundary
    };
    let (first, rest) = message.split_at(split);
    (
        first.trim_end(),
        ferravex_render::ui::fit_label(rest.trim_start(), width, 10.5).unwrap(),
    )
}

// Keep the whole status panel above the 30px health/ammo glyph blocks whose
// baseline is height - 26. The menu reserves the same 150px bottom band.
fn footer_bounds(width: f32, height: f32) -> (ferravex_render::ui::UiRect, f32) {
    let top = height - 146.;
    (
        ferravex_render::ui::UiRect {
            min: [12., top - 4.],
            size: [(width - 24.).max(1.), 92.],
        },
        top,
    )
}

fn overlay_text(
    mode: &str,
    tick: u64,
    message: &str,
    input: &PlayerInput,
    width: f32,
    mut emit: impl FnMut(f32, std::fmt::Arguments<'_>),
) {
    let (first, second) = message_lines(message, width);
    emit(0., format_args!("{mode}\nTick {tick}"));
    emit(
        28.,
        format_args!("{first}\n{}{}", second.prefix, second.suffix),
    );
    let mut pause = [0; 64];
    let mut play = [0; 64];
    let pause = crate::hud::menu_binding(input, "replay_pause", "Pause", width, &mut pause);
    let play = crate::hud::menu_binding(input, "replay", "Replay/live", width, &mut play);
    emit(56., format_args!("Pause: {pause}\nReplay/live: {play}"));
}
fn sample(tick: u64, p: &Player, teleport: bool) -> Sample {
    Sample {
        tick,
        position: p.position,
        rotation: ferravex_render::prelude::Quat::from_rotation_y(-p.yaw),
        teleport,
    }
}

pub fn draw_ghost(g: Sample) {
    use ferravex_render::prelude::*;
    let points: [Vec3; 8] = std::array::from_fn(|i| {
        g.position
            + g.rotation
                * vec3(
                    if i & 1 == 0 { -0.3 } else { 0.3 },
                    if i & 2 == 0 { 0. } else { 1.8 },
                    if i & 4 == 0 { -0.3 } else { 0.3 },
                )
    });
    for i in 0..8 {
        for axis in [1, 2, 4] {
            let j = i ^ axis;
            if i < j {
                draw_line_3d(points[i], points[j], Color::new(0.3, 0.9, 0.8, 0.65));
            }
        }
    }
}
#[cfg(any(target_arch = "wasm32", test))]
mod browser {
    use super::*;
    use std::cell::RefCell;
    const MAX_BYTES: usize = 8 * 1024 * 1024;
    #[derive(Default)]
    struct Bridge {
        commands: u32,
        edit: Vec<u8>,
        pending: Option<Vec<u8>>,
        out: Vec<u8>,
        revision: u32,
        status: u32,
        finished: u32,
        tick: u64,
        message: String,
    }
    thread_local! {static BRIDGE:RefCell<Bridge>=RefCell::new(Bridge::default());}
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_replay_command(bits: u32) {
        BRIDGE.with(|b| b.borrow_mut().commands |= bits & 127);
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_replay_buffer(len: usize) -> usize {
        if len == 0 || len > MAX_BYTES {
            return 0;
        }
        BRIDGE.with(|b| {
            let mut b = b.borrow_mut();
            if b.pending.is_some() {
                return 0;
            }
            b.edit.resize(len, 0);
            b.edit.as_mut_ptr() as usize
        })
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_replay_submit(len: usize) -> u32 {
        BRIDGE.with(|b| {
            let mut b = b.borrow_mut();
            if len == 0 || len != b.edit.len() || b.pending.is_some() {
                return 0;
            }
            b.pending = Some(std::mem::take(&mut b.edit));
            1
        })
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_replay_export() -> usize {
        BRIDGE.with(|b| b.borrow().out.as_ptr() as usize)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_replay_export_len() -> usize {
        BRIDGE.with(|b| b.borrow().out.len())
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_replay_revision() -> u32 {
        BRIDGE.with(|b| b.borrow().revision)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_replay_status() -> u32 {
        BRIDGE.with(|b| b.borrow().status)
    }
    /// One only when the active playback driver has verified through its end.
    /// Read after the normal frame, just like status/tick. Pause, lack of ticks,
    /// an in-progress seek or a verification fault never imply completion.
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_replay_finished() -> u32 {
        BRIDGE.with(|b| b.borrow().finished)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_replay_tick() -> u64 {
        BRIDGE.with(|b| b.borrow().tick)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_replay_message() -> usize {
        BRIDGE.with(|b| b.borrow().message.as_ptr() as usize)
    }
    #[unsafe(no_mangle)]
    pub extern "C" fn ferravex_replay_message_len() -> usize {
        BRIDGE.with(|b| b.borrow().message.len())
    }
    pub fn poll(s: &mut Session, game: &Match, network: bool) -> bool {
        BRIDGE.with(|b| {
            let mut b = b.borrow_mut();
            let bits = std::mem::take(&mut b.commands);
            let incoming = b.pending.take();
            if network {
                return false;
            }
            let mut cut = false;
            if let Some(bytes) = incoming {
                match s.import(&bytes, game) {
                    Ok(()) => cut = true,
                    Err(e) => s.message = format!("Import rejected: {e}"),
                }
            }
            if bits & 1 != 0 {
                if s.recording() {
                    s.stop_record();
                } else {
                    s.start_record(game);
                }
            }
            if bits & 2 != 0 {
                let was_playing = s.playing();
                s.toggle_playback(game);
                cut |= was_playing != s.playing();
            }
            if bits & 4 != 0 && s.playing() {
                s.paused = !s.paused;
            }
            if bits & 8 != 0 && s.playing() {
                s.seek(-64);
                cut = true;
            }
            if bits & 16 != 0 && s.playing() {
                s.seek(64);
                cut = true;
            }
            if bits & 32 != 0 {
                s.toggle_ghost(game);
            }
            if bits & 64 != 0 {
                if let Some(bytes) = s.export() {
                    b.out = bytes;
                    b.revision = b.revision.wrapping_add(1);
                } else {
                    s.message = "No recording to export".into();
                }
            }
            cut
        })
    }
    pub fn sync(s: &Session) {
        BRIDGE.with(|b| {
            let mut b = b.borrow_mut();
            b.status = u32::from(s.recording())
                | u32::from(s.playing()) << 1
                | u32::from(s.paused) << 2
                | u32::from(s.playback.as_ref().is_some_and(|p| p.seeking())) << 3
                | u32::from(s.playback.as_ref().is_some_and(|p| p.fault().is_some())) << 4;
            b.finished = u32::from(s.finished());
            b.tick = s.playback.as_ref().map_or_else(
                || s.capture.as_ref().map_or(0, |c| c.tick_count() as u64),
                |p| p.position(),
            );
            if b.message != s.message {
                b.message.clone_from(&s.message);
            }
        })
    }
}
impl Session {
    pub fn browser_poll(&mut self, game: &Match, network: bool) -> bool {
        #[cfg(any(target_arch = "wasm32", test))]
        return browser::poll(self, game, network);
        #[cfg(not(any(target_arch = "wasm32", test)))]
        {
            let _ = (game, network);
            false
        }
    }
    pub fn browser_sync(&self) {
        #[cfg(any(target_arch = "wasm32", test))]
        browser::sync(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session() -> Session {
        Session {
            slot: Some(
                Slot::with_backend(
                    "strike",
                    "test-replay",
                    Clip::default(),
                    PlatformStore::transient(),
                )
                .unwrap(),
            ),
            ..Session::default()
        }
    }
    fn recorded(ticks: usize) -> (Match, Vec<u8>) {
        let mut game = crate::practice();
        let mut s = session();
        s.start_record(&game);
        for _ in 0..ticks {
            let events = game.step(&[]);
            s.capture(&game, &[], &events);
        }
        (game, s.export().unwrap())
    }

    fn assert_bridge(s: &Session, finished: bool, flags: u32, tick: u64) {
        s.browser_sync();
        assert_eq!(browser::ferravex_replay_finished(), u32::from(finished));
        assert_eq!(browser::ferravex_replay_status(), flags);
        assert_eq!(browser::ferravex_replay_tick(), tick);
    }

    #[test]
    fn inactive_replay_commands_do_not_cut_practice_input_or_invent_completion() {
        use ferravex_render::input::touch::{Control, Event, Output, Phase, Region};
        use ferravex_render::input::{ActionKind, Binding, InputCode, host_frame, touch_event};
        let game = crate::practice();
        let mut s = session();
        let mut input = PlayerInput::transient("strike-inactive-replay");
        input
            .global_action(
                "replay_back",
                "Seek back",
                ActionKind::Button,
                vec![Binding::digital(InputCode::Touch(2))],
            )
            .unwrap();
        input
            .set_touch_layout(&[Control {
                region: Region::Circle {
                    center: ferravex_render::vec2(40., 40.),
                    radius: 20.,
                },
                output: Output::Button(2),
            }])
            .unwrap();
        host_frame(0., 800, 600, || input.sample(true));
        for phase in [Phase::Start, Phase::End] {
            touch_event(Event {
                id: 141,
                phase,
                position: ferravex_render::vec2(40., 40.),
            });
        }
        host_frame(0., 800, 600, || {
            input.sample(true);
            assert!(!s.controls(&mut input, &game, false));
        });
        for command in [8, 16, 2] {
            browser::ferravex_replay_command(command);
            assert!(!s.browser_poll(&game, false));
            assert_bridge(&s, false, 0, 0);
        }
    }

    #[test]
    fn completion_export_tracks_terminal_tick_pause_seek_and_return_live() {
        let (game, bytes) = recorded(3);
        let mut s = session();
        s.import(&bytes, &game).unwrap();
        assert_bridge(&s, false, 2, 0);
        browser::ferravex_replay_command(4);
        assert!(!s.browser_poll(&game, false));
        assert!(!s.advance());
        assert_bridge(&s, false, 6, 0);
        browser::ferravex_replay_command(4);
        s.browser_poll(&game, false);
        for tick in 1..=3 {
            assert!(s.advance());
            assert_bridge(&s, tick == 3, 2, tick);
        }
        assert_eq!(s.mode_label(), "FINISHED");
        assert!(!s.advance());
        s.paused = true;
        assert_bridge(&s, true, 6, 3);
        s.seek(-1);
        assert_bridge(&s, false, 14, 0);
        assert!(s.pump());
        assert_bridge(&s, false, 6, 2);
        assert_eq!(s.mode_label(), "PAUSED");
        s.seek(1);
        assert_bridge(&s, false, 14, 0);
        assert!(s.pump());
        assert_bridge(&s, true, 6, 3);
        s.toggle_playback(&game);
        assert_bridge(&s, false, 0, 0);
        s.toggle_playback(&game);
        assert_bridge(&s, false, 2, 0);
    }

    #[test]
    fn completion_handles_empty_import_driver_reset_and_practice_restart() {
        let (game, empty) = recorded(0);
        let (_, nonempty) = recorded(1);
        let mut s = session();
        s.import(&empty, &game).unwrap();
        assert_bridge(&s, true, 2, 0);
        assert!(!s.advance());
        assert!(s.import(b"invalid", &game).is_err());
        assert_bridge(&s, true, 2, 0);
        s.playback.as_mut().unwrap().reset();
        assert_bridge(&s, false, 2, 0);
        s.playback
            .as_mut()
            .unwrap()
            .start(s.world.as_mut().unwrap())
            .unwrap();
        assert_bridge(&s, true, 2, 0);
        s.import(&nonempty, &game).unwrap();
        assert_bridge(&s, false, 2, 0);
        assert!(s.advance());
        assert_bridge(&s, true, 2, 1);
        s.reset_practice();
        assert_bridge(&s, false, 0, 0);
        s.start_record(&game);
        assert_bridge(&s, false, 1, 0);
    }

    #[test]
    fn completion_waits_for_final_seek_pump_and_browser_import_resynchronizes() {
        let (game, bytes) = recorded(127);
        let mut s = session();
        // Exercise the actual shared bridge/import path, not a synthetic status.
        let ptr = browser::ferravex_replay_buffer(bytes.len());
        assert_ne!(ptr, 0);
        // SAFETY: buffer just reserved bytes.len() writable bytes in this thread.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
        }
        assert_eq!(browser::ferravex_replay_submit(bytes.len()), 1);
        assert!(s.browser_poll(&game, false));
        assert_bridge(&s, false, 2, 0);
        s.seek(127);
        assert_bridge(&s, false, 10, 0);
        assert!(s.pump());
        assert_bridge(&s, false, 10, 64);
        s.paused = true;
        assert!(s.pump());
        assert_bridge(&s, true, 6, 127);
        assert!(!s.pump());
        assert_bridge(&s, true, 6, 127);
        // Seeking past the end clamps to a real endpoint and must verify again.
        s.seek(i64::MAX);
        assert_bridge(&s, false, 14, 0);
        assert!(s.pump());
        assert!(s.pump());
        assert_bridge(&s, true, 6, 127);
        s.import(&bytes, &game).unwrap();
        assert_bridge(&s, false, 2, 0);
    }

    #[test]
    fn mismatch_on_final_tick_is_not_finished_and_seek_can_recover() {
        let (game, bytes) = recorded(1);
        let mut s = session();
        s.import(&bytes, &game).unwrap();
        s.world.as_mut().unwrap().game.players[0].health -= 1;
        assert!(!s.advance());
        assert_eq!(
            s.playback.as_ref().unwrap().status(),
            PlaybackStatus::Faulted
        );
        assert_bridge(&s, false, 22, 1);
        assert_eq!(s.mode_label(), "MISMATCH / ERROR");
        assert!(!s.advance());
        assert_bridge(&s, false, 22, 1);
        s.seek(-1);
        assert_bridge(&s, false, 6, 0);
        s.paused = false;
        assert!(s.advance());
        assert_bridge(&s, true, 2, 1);
    }

    #[test]
    fn local_menu_and_settings_allow_real_replay_completion_without_live_ticks() {
        let (game, bytes) = recorded(5);
        let hash = game.state_hash();
        let mut s = session();
        s.import(&bytes, &game).unwrap();
        let mut host = crate::host::Host::new(true);
        host.open_menu();
        for tick in 1..=5 {
            let frame = host.advance(
                crate::host::Mode::Replay,
                1. / TICK_RATE as f64,
                false,
                !s.paused,
            );
            assert_eq!(frame.steps, 1);
            for _ in 0..frame.steps {
                assert!(s.advance());
            }
            assert_bridge(&s, tick == 5, 2, tick);
        }
        assert_eq!(game.state_hash(), hash);
        assert!(!host.active());
    }

    #[test]
    fn recording_export_import_seek_and_live_are_independent() {
        let mut game = crate::practice();
        let mut s = session();
        s.start_record(&game);
        for _ in 0..150 {
            let inputs = [(
                0,
                InputFrame {
                    move_z: 1.,
                    buttons: ferravex_fps::BUTTON_FIRE,
                    ..Default::default()
                },
            )];
            let events = game.step(&inputs);
            s.capture(&game, &inputs, &events);
        }
        let bytes = s.export().unwrap();
        assert!(!s.recording());
        let live_hash = game.state_hash();
        s.import(&bytes, &game).unwrap();
        for _ in 0..150 {
            assert!(s.advance());
        }
        assert_eq!(s.world.as_ref().unwrap().game.state_hash(), live_hash);
        s.seek(-64);
        while s.pump() {}
        assert_eq!(s.playback.as_ref().unwrap().position(), 86);
        assert_eq!(game.state_hash(), live_hash);
        let before = s.playback.as_ref().unwrap().position();
        assert!(s.import(b"bad", &game).is_err());
        assert_eq!(s.playback.as_ref().unwrap().position(), before);
        s.toggle_playback(&game);
        assert!(!s.playing());
        assert_eq!(game.state_hash(), live_hash);
        let encoded = Clip(s.clip.clone()).encode(1).unwrap();
        let loaded = Clip::decode(&encoded).unwrap().0;
        assert_eq!(loaded.0.unwrap().encode(), bytes);
    }

    #[test]
    fn retry_starts_a_fresh_recording_without_a_world_discontinuity() {
        let mut game = crate::practice();
        let initial_hash = game.state_hash();
        let mut bots = crate::bots::PracticeBots::new(&game.arena);
        let mut s = session();
        s.start_record(&game);
        for _ in 0..3 {
            let events = game.step(&[]);
            s.capture(&game, &[], &events);
        }
        crate::reset_practice(&mut game, &mut bots, &mut s);
        assert_eq!(game.state_hash(), initial_hash);
        assert!(s.recording());
        assert_eq!(s.clip.as_ref().unwrap().tick_count(), 3);
        let events = game.step(&[]);
        s.capture(&game, &[], &events);
        let expected = game.state_hash();
        let bytes = s.export().unwrap();
        s.import(&bytes, &game).unwrap();
        assert!(s.advance());
        assert_eq!(s.world.as_ref().unwrap().game.state_hash(), expected);
        assert_eq!(s.playback.as_ref().unwrap().recording().tick_count(), 1);
    }

    #[test]
    fn menu_pause_does_not_record_extra_authoritative_ticks() {
        let mut host = crate::host::Host::new(true);
        let mut game = crate::practice();
        let mut s = session();
        s.start_record(&game);
        let dt = 1. / TICK_RATE as f64;
        for paused in [false, true, false] {
            if paused {
                host.open_menu();
            } else {
                host.resume();
            }
            let frame = host.advance(
                crate::host::Mode::Practice,
                if paused { 60. } else { dt },
                true,
                true,
            );
            for _ in 0..frame.steps {
                let events = game.step(&[]);
                s.capture(&game, &[], &events);
            }
        }
        let expected = game.state_hash();
        let bytes = s.export().unwrap();
        s.import(&bytes, &game).unwrap();
        assert_eq!(s.playback.as_ref().unwrap().recording().tick_count(), 2);
        assert!(s.advance());
        assert!(s.advance());
        assert_eq!(s.world.as_ref().unwrap().game.state_hash(), expected);
    }
}
