use crate::{CREAM, GOLD, INK, TEAL, courses::COURSES, sim::Run};
use ferravex_render::{
    input::PlayerInput,
    prelude::*,
    ui::{Align, ButtonOptions, ButtonStyle, TextStyle, Ui, UiRect, Viewport},
};

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Select(usize),
    Toggle,
    Retry,
    Next,
    Recover,
}
pub(crate) struct View {
    pub course: usize,
    pub picker: bool,
    pub resumable: bool,
    pub can_recover: bool,
    pub context: u64,
}

impl View {
    fn selection(&self, index: usize) -> Command {
        if index == self.course && self.resumable {
            Command::Toggle
        } else {
            Command::Select(index)
        }
    }
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
fn button(ui: &mut Ui, id: u64, r: UiRect, text: &str, selected: bool) {
    let width = measure_text(text, None, 15, 1.).width.max(1.);
    ui.button_with_options(
        id,
        r,
        text,
        ButtonOptions {
            enabled: true,
            selected,
        },
        ButtonStyle {
            fill: TEAL,
            ink: CREAM,
            focus: GOLD,
            disabled: INK,
            size: (15. * ((r.size[0] - 12.) / width).min(1.)).max(8.),
        },
    )
    .unwrap();
}
pub fn draw(
    ui: &mut Ui,
    player: &mut PlayerInput,
    run: &Run,
    view: View,
    records: &crate::timing::TrialRecords,
) -> Option<Command> {
    let View {
        course,
        picker,
        resumable,
        can_recover,
        context,
    } = view;
    let w = screen_width();
    let h = screen_height();
    // A hidden/collapsed player may briefly report a zero-sized viewport.
    if w < 160. || h < 144. {
        return None;
    }
    ui.begin(Viewport::fit([w, h], [w, h], [0.; 4]).unwrap());
    ui.set_auto_focus(picker || run.finished);
    button(ui, 1, rect(w - 102., 12., 88., 34.), "COURSES", picker);
    if !picker && can_recover && player.touch_seen() {
        button(ui, 2, rect(w - 102., 54., 88., 32.), "RECOVER", false);
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
                if resumable {
                    "COURSES / RUN PAUSED"
                } else {
                    "CHOOSE YOUR COURSE"
                },
                20.,
                CREAM,
            );
            let cw = (pw - 36.) * 0.5;
            let ch = (ph - 48.) * 0.5;
            for (i, c) in COURSES.iter().enumerate() {
                let cx = x + 12. + (i % 2) as f32 * (cw + 12.);
                let cy = y + 36. + (i / 2) as f32 * (ch + 8.);
                let title = if i == course && resumable {
                    format!("RESUME {}", c.name)
                } else {
                    c.name.into()
                };
                button(ui, 10 + i as u64, rect(cx, cy, cw, ch), &title, i == course);
                let best = records
                    .book
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
                        if i == course { TEAL } else { CREAM },
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
                .book
                .get(c.id, context)
                .and_then(|r| r.fastest_us)
                .map_or(run.elapsed, |us| us as f32 / 1e6);
            if ph >= 240. {
                label(
                    ui,
                    rect(x + 12., y + 67., pw - 24., 16.),
                    &format!("BEST {:.1}s / GOLD {:.0}s + NO FALLS", best, c.gold_seconds),
                    13.,
                    CREAM,
                );
            }
            label(
                ui,
                rect(
                    x + 12.,
                    y + if ph >= 240. { 96. } else { 64. },
                    pw - 24.,
                    14.,
                ),
                &records.finish_label(),
                if ph >= 240. { 12. } else { 10. },
                CREAM,
            );
            if ph >= 240. {
                label(
                    ui,
                    rect(x + 12., y + 120., pw - 24., 16.),
                    if records.error().is_some() {
                        "SESSION RECORD / SAVE UNAVAILABLE"
                    } else {
                        "ALL CRYSTALS COLLECTED"
                    },
                    10.,
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
                false,
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
                false,
            );
        }
    }
    ui.finish_player(player, !player.controls_open());
    ui.paint();
    for i in 0..COURSES.len() {
        if ui.activated(10 + i as u64) {
            return Some(view.selection(i));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choosing_the_current_course_resumes_even_before_the_timer_starts() {
        let mut view = View {
            course: 2,
            picker: true,
            resumable: false,
            can_recover: false,
            context: 0,
        };
        assert_eq!(view.selection(2), Command::Select(2));
        view.resumable = true;
        assert_eq!(view.selection(2), Command::Toggle);
        assert_eq!(view.selection(1), Command::Select(1));
        view.resumable = false; // Completion offers a fresh trial.
        assert_eq!(view.selection(2), Command::Select(2));
    }
}
