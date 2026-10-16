use crate::{CREAM, GOLD, INK, TEAL, courses::COURSES, sim::Run};
use ferravex_render::{
    input::PlayerInput,
    prelude::*,
    storage::Records,
    ui::{Align, ButtonStyle, TextStyle, Ui, UiRect, Viewport},
};

pub enum Command {
    Select(usize),
    Toggle,
    Retry,
    Next,
    Recover,
}
fn rect(x: f32, y: f32, w: f32, h: f32) -> UiRect {
    UiRect {
        min: [x, y],
        size: [w, h],
    }
}
fn label(ui: &mut Ui, r: UiRect, text: &str, size: f32, color: Color) {
    ui.text(
        r,
        text,
        TextStyle {
            size,
            color,
            align: Align::Center,
            ..Default::default()
        },
    )
    .unwrap();
}
fn button(ui: &mut Ui, id: u64, r: UiRect, text: &str) {
    ui.button(
        id,
        r,
        text,
        true,
        ButtonStyle {
            fill: TEAL,
            ink: CREAM,
            focus: GOLD,
            disabled: INK,
            size: 15.,
        },
    )
    .unwrap();
}
pub fn draw(
    ui: &mut Ui,
    player: &mut PlayerInput,
    run: &Run,
    course: usize,
    picker: bool,
    records: &Records,
    context: u64,
) -> Option<Command> {
    let w = screen_width();
    let h = screen_height();
    // A hidden/collapsed player may briefly report a zero-sized viewport.
    if w < 160. || h < 144. {
        return None;
    }
    ui.begin(Viewport::fit([w, h], [w, h], [0.; 4]).unwrap());
    ui.set_auto_focus(picker || run.finished);
    button(ui, 1, rect(w - 102., 12., 88., 34.), "COURSES");
    if !picker && !run.finished && player.touch_seen() {
        button(ui, 2, rect(w - 102., 54., 88., 32.), "RECOVER");
    }
    if picker || run.finished {
        ui.modal(100).unwrap();
        ui.panel(rect(0., 0., w, h), Color::new(0.07, 0.13, 0.14, 0.7))
            .unwrap();
        let pw = (w - 24.).min(560.);
        let ph = (h - 16.).min(268.);
        let x = (w - pw) * 0.5;
        let y = (h - ph) * 0.5;
        ui.panel(rect(x, y, pw, ph), INK).unwrap();
        if picker {
            label(
                ui,
                rect(x + 12., y + 9., pw - 24., 24.),
                "CHOOSE YOUR COURSE",
                20.,
                CREAM,
            );
            let cw = (pw - 36.) * 0.5;
            let ch = (ph - 48.) * 0.5;
            for (i, c) in COURSES.iter().enumerate() {
                let cx = x + 12. + (i % 2) as f32 * (cw + 12.);
                let cy = y + 36. + (i / 2) as f32 * (ch + 8.);
                let title = if i == course && run.elapsed > 0. && !run.finished {
                    format!("RESUME {}", c.name)
                } else {
                    c.name.into()
                };
                button(ui, 10 + i as u64, rect(cx, cy, cw, ch), &title);
                let best = records
                    .get(c.id, if i == course { context } else { c.context() })
                    .and_then(|r| r.fastest_us);
                let summary = best.map_or_else(
                    || {
                        format!(
                            "GOLD {:.0}s / {}",
                            c.gold_seconds,
                            if i == 0 {
                                "WARM-UP"
                            } else if i == 1 {
                                "HARD"
                            } else {
                                "EXPERT"
                            }
                        )
                    },
                    |us| format!("BEST {:.1}s / GOLD {:.0}s", us as f32 / 1e6, c.gold_seconds),
                );
                if ch >= 60. {
                    label(
                        ui,
                        rect(cx + 5., cy + ch - 19., cw - 10., 15.),
                        &summary,
                        10.,
                        CREAM,
                    );
                }
            }
        } else {
            let c = &COURSES[course];
            label(
                ui,
                rect(x + 12., y + 10., pw - 24., 24.),
                &format!("{} / {}", c.name, c.medal(run.elapsed, run.falls)),
                22.,
                GOLD,
            );
            label(
                ui,
                rect(x + 12., y + 40., pw - 24., 20.),
                &format!("{:.1}s / {} falls", run.elapsed, run.falls),
                18.,
                CREAM,
            );
            let best = records
                .get(c.id, context)
                .and_then(|r| r.fastest_us)
                .map_or(run.elapsed, |us| us as f32 / 1e6);
            if ph >= 188. {
                label(
                    ui,
                    rect(x + 12., y + 67., pw - 24., 16.),
                    &format!("BEST {:.1}s / GOLD {:.0}s + NO FALLS", best, c.gold_seconds),
                    13.,
                    CREAM,
                );
            }
            if ph >= 240. {
                label(
                    ui,
                    rect(x + 12., y + 96., pw - 24., 16.),
                    if records.error().is_some() {
                        "SESSION RECORD / SAVE UNAVAILABLE"
                    } else {
                        "ALL CRYSTALS COLLECTED"
                    },
                    12.,
                    CREAM,
                );
            }
            button(
                ui,
                4,
                if ph >= 188. {
                    rect(x + 14., y + ph - 100., pw - 28., 44.)
                } else {
                    rect(x + 12., y + ph - 50., (pw - 36.) * 0.5, 44.)
                },
                if course + 1 < COURSES.len() {
                    "NEXT COURSE"
                } else {
                    "CHOOSE A COURSE"
                },
            );
            button(
                ui,
                5,
                if ph >= 188. {
                    rect(x + 14., y + ph - 50., pw - 28., 44.)
                } else {
                    rect(
                        x + 24. + (pw - 36.) * 0.5,
                        y + ph - 50.,
                        (pw - 36.) * 0.5,
                        44.,
                    )
                },
                "RETRY FOR GOLD",
            );
        }
    }
    ui.finish_player(player, !player.controls_open());
    ui.paint();
    for i in 0..COURSES.len() {
        if ui.activated(10 + i as u64) {
            if i == course && run.elapsed > 0. && !run.finished {
                return Some(Command::Toggle);
            }
            return Some(Command::Select(i));
        }
    }
    if ui.activated(1) {
        Some(Command::Toggle)
    } else if ui.activated(2) {
        Some(Command::Recover)
    } else if ui.activated(4) {
        Some(Command::Next)
    } else if ui.activated(5) {
        Some(Command::Retry)
    } else {
        None
    }
}
