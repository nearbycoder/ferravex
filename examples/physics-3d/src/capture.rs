//! Debug-build-only visual review hooks. Not compiled into release or Wasm
//! builds, so shipped behavior and the public CLI are unchanged.
//!
//! - `PHYSICS_LAB_WINDOW=390x700` sets the native window's logical size.
//! - `PHYSICS_LAB_SCRIPT="60:bowl,200:chaos"` injects yard commands on render
//!   frames: `bowl`, `chaos`, `pulse`, `pause`, `step`, `reset`, `view`,
//!   `crate`, `ball`, `drop`, `stats`, `yaw@RADIANS`, `zoom@METERS`.
//! - `PHYSICS_LAB_GOALS=N` marks the first N goals done (card/hook review).
//! - `PHYSICS_LAB_FOCUS=1` keeps simulating while the review window is unfocused.
use super::hud::Hud;
use super::toys::Goal;
use super::{Commands, Conf, Lab};

pub(super) fn window(conf: &mut Conf) {
    if let Some((w, h)) = std::env::var("PHYSICS_LAB_WINDOW").ok().and_then(|size| {
        let (w, h) = size.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    }) {
        conf.window_width = w;
        conf.window_height = h;
    }
}

pub(super) struct Capture {
    frame: u32,
    script: Vec<(u32, String)>,
    goals: usize,
    focus: bool,
}

impl Capture {
    pub fn from_env() -> Self {
        let script = std::env::var("PHYSICS_LAB_SCRIPT")
            .unwrap_or_default()
            .split(',')
            .filter_map(|step| {
                let (frame, action) = step.split_once(':')?;
                Some((frame.trim().parse().ok()?, action.trim().to_owned()))
            })
            .collect();
        Self {
            frame: 0,
            script,
            goals: std::env::var("PHYSICS_LAB_GOALS")
                .ok()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0),
            focus: std::env::var("PHYSICS_LAB_FOCUS").is_ok_and(|v| v == "1"),
        }
    }

    pub fn forced(&self) -> bool {
        self.focus
    }

    pub fn inject(&mut self, lab: &mut Lab, commands: &mut Commands, hud: &mut Hud) {
        if self.frame == 1 && self.goals > 0 {
            for goal in Goal::ALL.iter().take(self.goals.min(3)) {
                lab.fun.goals[goal.index()] = true;
            }
            lab.fun.airtime = 7.6;
            lab.fun.bonk_peak = 58;
        }
        for (_, action) in self.script.iter().filter(|(at, _)| *at == self.frame) {
            let (name, arg) = action.split_once('@').unwrap_or((action, ""));
            match name {
                "bowl" => commands.bowl = true,
                "chaos" => commands.chaos = true,
                "pulse" => commands.pulse = true,
                "pause" => commands.pause = true,
                "step" => commands.step = true,
                "reset" => commands.reset = true,
                "view" => commands.view = true,
                "crate" => commands.crate_drop = true,
                "ball" => commands.ball_drop = true,
                "drop" => commands.drop = true,
                "stats" => hud.stats = !hud.stats,
                "yaw" => lab.orbit.yaw = arg.parse().unwrap_or(lab.orbit.yaw),
                "zoom" => lab.orbit.radius = arg.parse().unwrap_or(lab.orbit.radius),
                _ => eprintln!("unknown PHYSICS_LAB_SCRIPT action {action}"),
            }
        }
    }

    pub fn end_frame(&mut self) {
        self.frame += 1;
    }
}
