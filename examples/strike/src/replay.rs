//! Opt-in practice recording. Live and playback authority never share state.
use ferravex_fps::replay::{Capture, World};
use ferravex_fps::{GameEvent, InputFrame, Match, Player, TICK_RATE};
use ferravex_play::persistence::{Document, SaveData, SaveError, Value};
use ferravex_play::replay::{
    Limits, Playback, Recording,
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
                self.message = "Recording practice - F4 stops and saves".into();
            }
            Err(e) => self.message = format!("Recording failed: {e:?}"),
        }
    }
    pub fn stop_record(&mut self) {
        if let Some(c) = self.capture.take() {
            let tape = Arc::new(c.finish());
            self.message = format!("Saved {} replay ticks - F5 watches", tape.tick_count());
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
            self.message = "Live practice".into();
            return;
        }
        self.stop_record();
        self.load();
        let Some(tape) = self.clip.clone() else {
            self.message = "No recording yet - F4 starts one".into();
            return;
        };
        let mut w = World::new(Match::new(game.arena.clone()));
        let mut p = Playback::new(tape);
        match p.start(&mut w) {
            Ok(()) => {
                self.playback = Some(p);
                self.world = Some(w);
                self.paused = false;
                self.message = "Replay - P pauses, arrows seek, F5 returns live".into();
            }
            Err(e) => self.message = format!("Replay rejected: {e:?}"),
        }
    }
    pub fn advance(&mut self) -> bool {
        if self.paused {
            return false;
        }
        if let (Some(p), Some(w)) = (&mut self.playback, &mut self.world) {
            if p.seeking() || p.finished() {
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
        if play {
            self.toggle_playback(game);
        }
        if input.pressed("replay_pause") && self.playing() {
            self.paused = !self.paused;
        }
        if back {
            self.seek(-64);
        }
        if forward {
            self.seek(64);
        }
        if input.pressed("ghost") {
            self.toggle_ghost(game);
        }
        play || back || forward
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
    pub fn overlay(&self, ui: &mut ferravex_render::ui::Ui, diagnostics: bool) {
        use ferravex_render::{
            prelude::*,
            ui::{TextStyle, UiRect},
        };
        if self.message.is_empty() && !diagnostics {
            return;
        }
        let mode = if self.recording() {
            "REC"
        } else if self.playing() {
            if self.paused { "PAUSED" } else { "REPLAY" }
        } else {
            "LIVE"
        };
        let tick = self.playback.as_ref().map_or_else(
            || self.capture.as_ref().map_or(0, |c| c.tick_count() as u64),
            |p| p.position(),
        );
        let message = if self.message.is_empty() {
            "F4 record / F5 watch / F6 ghost / P pause / arrows seek"
        } else {
            &self.message
        };
        let _ = ui.text_fmt(
            UiRect {
                min: [16., screen_height() - 78.],
                size: [(screen_width() - 32.).max(0.), 34.],
            },
            TextStyle {
                size: 14.,
                color: Color::new(0.9, 0.85, 0.65, 1.),
                shadow: true,
                ..Default::default()
            },
            format_args!("{mode} {tick}  {message}"),
        );
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
        self.capture = None;
        self.clip = Some(tape);
        self.playback = Some(p);
        self.world = Some(w);
        self.paused = false;
        self.ghost = None;
        self.ghost_job = None;
        self.message = "Imported replay - F5 returns live".into();
        Ok(())
    }
    pub fn export(&mut self) -> Option<Vec<u8>> {
        self.stop_record();
        self.clip.as_ref().map(|t| t.encode())
    }
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
#[cfg(target_arch = "wasm32")]
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
                s.toggle_playback(game);
                cut = true;
            }
            if bits & 4 != 0 && s.playing() {
                s.paused = !s.paused;
            }
            if bits & 8 != 0 {
                s.seek(-64);
                cut = true;
            }
            if bits & 16 != 0 {
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
        #[cfg(target_arch = "wasm32")]
        return browser::poll(self, game, network);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (game, network);
            false
        }
    }
    pub fn browser_sync(&self) {
        #[cfg(target_arch = "wasm32")]
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
}
