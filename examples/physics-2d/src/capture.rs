//! Debug-build-only visual review hooks. Not compiled into release or Wasm
//! builds, so shipped behavior and the public CLI are unchanged.
//!
//! - `PHYSICS_LAB_WINDOW=390x700` sets the native window's logical size.
//! - `PHYSICS_LAB_SCRIPT="60:chaos,200:pulse@0;1,300:domino"` injects lab
//!   commands on render frames. Actions: `chaos`, `domino`, `pause`, `step`,
//!   `reset`, `stats`, `ball`, `crate`, `drop@x;y`, `crate@x;y`, `pulse@x;y`,
//!   `select@x;y` (world meters).
//! - `PHYSICS_LAB_GOALS=N` marks the first N goals done (card/hook review).
//! - `PHYSICS_LAB_FOCUS=1` keeps simulating while the review window is unfocused.
use super::hud::Hud;
use super::toys::Goal;
use super::{Conf, Lab, LabInput, ToyInput};
use ferravex_render::prelude::*;

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

fn point(text: &str) -> Option<Vec2> {
    let (x, y) = text.split_once(';')?;
    Some(vec2(x.parse().ok()?, y.parse().ok()?))
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

    pub fn inject(
        &mut self,
        lab: &mut Lab,
        input: &mut LabInput,
        toys: &mut ToyInput,
        hud: &mut Hud,
    ) {
        if self.frame == 1 && self.goals > 0 {
            for goal in Goal::ALL.iter().take(self.goals.min(3)) {
                lab.fun.goals[goal.index()] = true;
            }
            lab.fun.tower = 7.9;
            lab.fun.altitude = 13.4;
            lab.fun.bonk_peak = 64;
        }
        for (_, action) in self.script.iter().filter(|(at, _)| *at == self.frame) {
            let (name, arg) = action.split_once('@').unwrap_or((action, ""));
            match name {
                "chaos" => toys.chaos = true,
                "domino" => toys.scene = true,
                "pause" => input.toggle_pause = true,
                "step" => input.single_step = true,
                "reset" => input.reset = true,
                "stats" => hud.stats = !hud.stats,
                "ball" => input.ball = Some(true),
                "crate" if arg.is_empty() => input.ball = Some(false),
                "crate" => input.drop = point(arg).map(|p| (p, true)),
                "drop" => input.drop = point(arg).map(|p| (p, false)),
                "pulse" => input.pulse = point(arg),
                "select" => {
                    let query = point(arg).and_then(|p| lab.sandbox.inspect(p).ok());
                    lab.select_body(query.as_ref());
                }
                "finish" => {
                    // Complete the last goal through the real event path.
                    lab.fun.goals = [true, true, true, false];
                    lab.fun.scenario = super::Scenario::Dominoes;
                    toys.scene = false;
                }
                _ => eprintln!("unknown PHYSICS_LAB_SCRIPT action {action}"),
            }
        }
    }

    pub fn end_frame(&mut self) {
        self.frame += 1;
    }
}
