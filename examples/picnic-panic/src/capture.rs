//! Debug-build-only visual review hooks (not compiled into release or Wasm).
//!
//! - `PICNIC_WINDOW=1600x1000` native window logical size.
//! - `PICNIC_SCREEN=title|wardrobe` start on a menu instead of autoplay.
//! - `PICNIC_SKIP=SECONDS` fast-forward the autoplay raccoon headlessly.
//! - `PICNIC_DONE=MASK` start with pranks crossed off (bit 0 = pie … bit 6 = melon).
//! - `PICNIC_PAUSE_AT=N` pause on render frame N.
//! - `PICNIC_END_AT=N` finish the day on frame N (results screen).
//! - `PICNIC_CAM=YAW,PITCH,DIST` hold a camera angle (radians, metres).
//! - `PICNIC_AT=X,Z` drop the raccoon at a spot before the first frame.
//! - `PICNIC_HAT=N` wear hat N; `PICNIC_TOUCH=1` show touch controls.
//! - `PICNIC_NOTES=0|1` force the notepad closed/open.
//! - `PICNIC_FOCUS=1` keep simulating when the review window loses focus.
use crate::Conf;
use crate::run::Run;
use crate::sim::bot::Bot;
use crate::sim::{Event, MAX_EVENTS};
use ferravex_render::prelude::{Vec3, vec3};

fn number<T: std::str::FromStr>(name: &str) -> Option<T> {
    std::env::var(name).ok()?.parse().ok()
}

fn triple(name: &str) -> Option<[f32; 3]> {
    let v = std::env::var(name).ok()?;
    let mut it = v.split(',').map(|s| s.trim().parse::<f32>());
    Some([it.next()?.ok()?, it.next()?.ok()?, it.next()?.ok()?])
}

pub fn window(conf: &mut Conf) {
    if let Some((w, h)) = std::env::var("PICNIC_WINDOW").ok().and_then(|size| {
        let (w, h) = size.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    }) {
        conf.window_width = w;
        conf.window_height = h;
    }
}

pub struct Capture {
    frame: u32,
    pub screen: Option<String>,
    skip: Option<f32>,
    pub done: Option<u8>,
    pause_at: Option<u32>,
    end_at: Option<u32>,
    pub cam: Option<[f32; 3]>,
    at: Option<[f32; 2]>,
    pub hat: Option<u8>,
    pub touch: bool,
    pub notes: Option<bool>,
    pub focus: bool,
}

impl Capture {
    pub fn from_env() -> Self {
        Self {
            frame: 0,
            screen: std::env::var("PICNIC_SCREEN").ok(),
            skip: number("PICNIC_SKIP"),
            done: number("PICNIC_DONE"),
            pause_at: number("PICNIC_PAUSE_AT"),
            end_at: number("PICNIC_END_AT"),
            cam: triple("PICNIC_CAM"),
            at: std::env::var("PICNIC_AT").ok().and_then(|v| {
                let (x, z) = v.split_once(',')?;
                Some([x.parse().ok()?, z.parse().ok()?])
            }),
            hat: number("PICNIC_HAT"),
            touch: number::<u8>("PICNIC_TOUCH").is_some_and(|v| v > 0),
            notes: number::<u8>("PICNIC_NOTES").map(|v| v > 0),
            focus: number::<u8>("PICNIC_FOCUS").is_some_and(|v| v > 0),
        }
    }

    pub fn pause_now(&self) -> bool {
        self.pause_at == Some(self.frame)
    }

    pub fn end_now(&self) -> bool {
        self.end_at == Some(self.frame)
    }

    /// Fast-forward once, headlessly, with the demo raccoon.
    pub fn prepare(&mut self, run: &mut Run, bot: &mut Bot) {
        if let Some([x, z]) = self.at.take() {
            let p = vec3(x, 0.1, z);
            let yaw = run.game.coon.yaw;
            crate::sim::raccoon::set_down(&mut run.game, p, yaw);
            run.game.coon.prev_pos = run.game.coon.motor.position;
        }
        if let Some(seconds) = self.skip.take() {
            let mut events = Vec::with_capacity(MAX_EVENTS);
            for _ in 0..(seconds * 60.) as u32 {
                let input = bot.input(&run.game);
                run.game.update(input);
                run.game.drain_events(&mut events);
                events.clear();
                if run.game.is_over() {
                    break;
                }
            }
        }
    }

    pub fn end_frame(&mut self) {
        self.frame += 1;
    }

    #[allow(dead_code)]
    pub fn spot(&self) -> Option<Vec3> {
        self.at.map(|[x, z]| vec3(x, 0., z))
    }

    #[allow(dead_code)]
    pub fn events_hint(_: &[Event]) {}
}
