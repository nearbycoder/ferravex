//! Debug-build-only visual review hooks. This module is not compiled into
//! release or Wasm builds, so shipped behavior and the public CLI are unchanged.
//!
//! - `CAVERN_WINDOW=390x700` sets the native window's logical size.
//! - `CAVERN_PAUSE_AT=N` pauses on render frame `N` (and stays paused).
//! - `CAVERN_STALL_AT=N` stops the autoplay pilot from frame `N`, optionally
//!   turning to `CAVERN_YAW` radians and adding `CAVERN_STUCK` seconds of
//!   no-progress time so the whispered hints can be reviewed.
//! - `CAVERN_SECRETS_AT=N` marks both secrets found on frame `N`.
//! - `CAVERN_FAKE_BEST=SECONDS` pretends a stored best existed at the finish.
//! - `CAVERN_STAGE_PICKUP=N` stops the pilot on frame `N`, stands it facing
//!   crystal `CAVERN_STAGE_CRYSTAL` (default 0) and presses use 20 frames later,
//!   so a real accepted pickup can be framed for gallery captures.
use super::{Conf, Expedition, Progress, vec2};

fn number<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.parse().ok()
}

pub(super) fn window(conf: &mut Conf) {
    if let Some((w, h)) = std::env::var("CAVERN_WINDOW").ok().and_then(|size| {
        let (w, h) = size.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    }) {
        conf.window_width = w;
        conf.window_height = h;
    }
}

pub(super) struct Capture {
    frame: u32,
    pause_at: Option<u32>,
    stall_at: Option<u32>,
    yaw: Option<f32>,
    stuck: Option<f32>,
    secrets_at: Option<u32>,
    fake_best: Option<f32>,
    stage_pickup: Option<u32>,
    stage_crystal: usize,
}

impl Capture {
    pub fn from_env() -> Self {
        Self {
            frame: 0,
            pause_at: number("CAVERN_PAUSE_AT"),
            stall_at: number("CAVERN_STALL_AT"),
            yaw: number("CAVERN_YAW"),
            stuck: number("CAVERN_STUCK"),
            secrets_at: number("CAVERN_SECRETS_AT"),
            fake_best: number("CAVERN_FAKE_BEST"),
            stage_pickup: number("CAVERN_STAGE_PICKUP"),
            stage_crystal: number::<usize>("CAVERN_STAGE_CRYSTAL").unwrap_or(0).min(2),
        }
    }

    pub fn paused(&self) -> bool {
        self.pause_at.is_some_and(|at| self.frame >= at)
    }

    /// Applies one-shot state hooks before this frame's simulation. Returns
    /// synthetic presentation events for the secrets hook.
    pub fn before_frame(&mut self, expedition: &mut Expedition) -> Option<Progress> {
        if self.stall_at == Some(self.frame) {
            expedition.autoplay = false;
            if let Some(yaw) = self.yaw {
                expedition.game.yaw = yaw;
            }
            if let Some(stuck) = self.stuck {
                expedition.game.stuck += stuck;
            }
        }
        if self.stage_pickup == Some(self.frame) {
            let game = &mut expedition.game;
            expedition.autoplay = false;
            let crystal = game.crystals[self.stage_crystal];
            game.position = crystal + vec2(1.35, 0.25);
            game.velocity = vec2(0., 0.);
            // A staged view is mid-run: the movement onboarding has long faded.
            game.walked = game.walked.max(10.);
            let delta = crystal - game.position;
            game.yaw = delta.x.atan2(-delta.y);
        }
        if self.secrets_at == Some(self.frame) {
            expedition.game.shy_found = true;
            expedition.game.note_reads = expedition.game.note_reads.max(1);
            return Some(Progress {
                shy: true,
                ..Default::default()
            });
        }
        None
    }

    /// True on the staged pickup's use frame.
    pub fn use_now(&self) -> bool {
        self.stage_pickup.is_some_and(|at| self.frame == at + 20)
    }

    pub fn fake_best(&self) -> Option<f32> {
        self.fake_best
    }

    pub fn end_frame(&mut self) {
        self.frame += 1;
    }
}
