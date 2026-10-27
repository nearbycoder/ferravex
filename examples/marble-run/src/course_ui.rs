use crate::{
    CREAM, GOLD, INK, TEAL,
    courses::{COURSES, Course},
    juice::{self, GREEN, VIOLET},
    quips::{self, Rank},
    sim::Run,
};
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
pub(crate) struct View<'a> {
    pub course: usize,
    pub picker: bool,
    pub resumable: bool,
    pub can_recover: bool,
    pub context: u64,
    /// The finished run's card; `None` until the celebration delay ends.
    pub results: Option<&'a Results>,
}

/// A finished run, summarised once at the accepted finish.
#[derive(Clone, Debug)]
pub(crate) struct Results {
    pub medal: &'static str,
    pub time: f32,
    pub gems: usize,
    pub falls: u32,
    pub bonks: u32,
    pub secret: bool,
    pub rank: Rank,
    pub next: String,
}

impl Results {
    pub fn new(run: &Run, course: &Course, best_us: Option<u64>) -> Self {
        let medal = course.medal(run.elapsed, run.falls);
        let summary = quips::Summary {
            medal,
            elapsed: run.elapsed,
            gold: course.gold_seconds,
            falls: run.falls,
            bonks: run.stats.bonks,
            clutches: run.stats.clutches,
            best_air: run.stats.best_air,
            secret: run.secret_found,
        };
        Self {
            medal,
            time: run.elapsed,
            gems: run.gems(),
            falls: run.falls,
            bonks: run.stats.bonks,
            secret: run.secret_found,
            rank: quips::rank(summary),
            next: quips::next_goal(
                course,
                medal,
                best_us.map(|us| us as f32 / 1e6),
                run.secret_found,
            ),
        }
    }
}

impl View<'_> {
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
/// The course button; its right edge stays left of the fullscreen menu corner.
pub(crate) fn course_rect(w: f32) -> UiRect {
    rect(12., 8., (w - 88.).min(240.), 44.)
}
/// Touch Recover sits below the fullscreen menu corner (y >= 60).
pub(crate) fn recover_rect(w: f32) -> UiRect {
    rect(w - 102., 64., 88., 32.)
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
    // Selection reads as a gold card with dark ink; keyboard focus stays cream.
    let (fill, ink, focus) = if selected {
        (GOLD, INK, CREAM)
    } else {
        (TEAL, CREAM, GOLD)
    };
    ui.button_with_options(
        id,
        r,
        text,
        ButtonOptions::default(),
        ButtonStyle {
            fill,
            ink,
            focus,
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
        ..
    } = view;
    let w = screen_width();
    let h = screen_height();
    // A hidden/collapsed player may briefly report a zero-sized viewport.
    if w < 160. || h < 144. {
        return None;
    }
    ui.begin(Viewport::fit([w, h], [w, h], [0.; 4]).unwrap());
    let results = view.results.filter(|_| run.finished);
    ui.set_auto_focus(picker || results.is_some());
    button(ui, 1, course_rect(w), COURSES[course].name, picker);
    if !picker && can_recover && player.touch_seen() {
        button(ui, 2, recover_rect(w), "RECOVER", false);
    }
    if picker || results.is_some() {
        ui.modal(100).unwrap();
        ui.panel(rect(0., 0., w, h), Color::new(0.07, 0.13, 0.14, 0.7))
            .unwrap();
        let pw = (w - 24.).min(560.);
        let ph = if picker {
            (h - 16.).min(268.)
        } else {
            (h - 16.).min(304.)
        };
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
                let record = records
                    .book
                    .get(c.id, if i == course { context } else { c.context() });
                let detailed = ch >= 92.;
                let summary = record.and_then(|r| r.fastest_us).map_or_else(
                    || {
                        if detailed {
                            format!("GOLD TARGET {:.0} SEC", c.gold_seconds)
                        } else {
                            format!(
                                "{} / GOLD {:.0} SEC",
                                c.challenge.split(" / ").next().unwrap_or("").to_uppercase(),
                                c.gold_seconds,
                            )
                        }
                    },
                    |us| {
                        let medal = match record.map_or(0, |r| r.score) {
                            3 => "GOLD",
                            2 => "SILVER",
                            _ => "BRONZE",
                        };
                        format!("{medal} / BEST {}", crate::hud::stopwatch(us as f32 / 1e6))
                    },
                );
                if ch >= 60. {
                    label(
                        ui,
                        rect(cx + 5., cy + ch - 21., cw - 10., 15.),
                        &summary,
                        11.,
                        if i == course { INK } else { CREAM },
                    );
                }
                if detailed {
                    label(
                        ui,
                        rect(cx + 5., cy + 8., cw - 10., 13.),
                        &c.challenge.to_uppercase(),
                        10.,
                        if i == course { INK } else { CREAM },
                    );
                }
            }
        } else if let Some(results) = results {
            finish_card(ui, view.course, records, results, (x, y, pw, ph));
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

/// The results card: medal and time, the comparison with the prior best, a
/// silly rank and the next goal, fitted by priority to the available height.
fn finish_card(
    ui: &mut Ui,
    course: usize,
    records: &crate::timing::TrialRecords,
    r: &Results,
    (x, y, pw, ph): (f32, f32, f32, f32),
) {
    let c = &COURSES[course];
    let stacked = ph >= 188.;
    let buttons_top = if stacked { y + ph - 104. } else { y + ph - 50. };
    let comparison = if records.error().is_some() {
        ("SESSION RECORD / SAVE UNAVAILABLE".to_owned(), CREAM)
    } else {
        let label = crate::hud::spell_seconds(&records.finish_label());
        if let Some(delta) = label.strip_prefix("NEW BEST / ") {
            (format!("NEW PERSONAL BEST! {delta}"), GREEN)
        } else {
            (label, CREAM)
        }
    };
    let mut stats = format!("ALL {} CRYSTALS · FALLS {}", r.gems, r.falls);
    if r.bonks > 0 {
        stats.push_str(&format!(" · BONKS {}", r.bonks));
    }
    if r.secret {
        stats.push_str(" · MOONSTONE FOUND");
    }
    let headline = format!("{}  {}", r.medal, crate::hud::stopwatch(r.time));
    let header = format!("{} COMPLETE", c.name.to_uppercase());
    // (priority, height, text, size, color); lower priority numbers survive first.
    let rows: [(u8, f32, &str, f32, Color); 7] = [
        (6, 20., &header, 13., TEAL),
        (0, 36., &headline, 30., juice::medal_color(r.medal)),
        (1, 22., &comparison.0, 14., comparison.1),
        (4, 20., &stats, 12., if r.secret { VIOLET } else { CREAM }),
        (2, 26., r.rank.title, 21., CREAM),
        (5, 18., r.rank.quip, 12., CREAM),
        (3, 20., &r.next, 12., GOLD),
    ];
    let available = buttons_top - y - 12.;
    let mut keep = [false; 7];
    let mut used = 0.;
    for priority in 0..7 {
        for (i, row) in rows.iter().enumerate() {
            if row.0 == priority && used + row.1 <= available {
                keep[i] = true;
                used += row.1;
            }
        }
    }
    let mut cursor = y + 10. + ((available - used) * 0.5).clamp(0., 12.);
    for (row, _) in rows.iter().zip(keep).filter(|(_, k)| *k) {
        label(
            ui,
            rect(x + 12., cursor, pw - 24., row.1 - 2.),
            row.2,
            row.3,
            row.4,
        );
        cursor += row.1;
    }
    let next = if course + 1 < COURSES.len() {
        format!("NEXT: {}", COURSES[course + 1].name.to_uppercase())
    } else {
        "CHOOSE A COURSE".into()
    };
    button(
        ui,
        4,
        if stacked {
            rect(x + 14., y + ph - 104., pw - 28., 44.)
        } else {
            rect(x + 12., y + ph - 50., (pw - 36.) * 0.5, 44.)
        },
        &next,
        false,
    );
    button(
        ui,
        5,
        if stacked {
            rect(x + 14., y + ph - 52., pw - 28., 44.)
        } else {
            rect(
                x + 24. + (pw - 36.) * 0.5,
                y + ph - 50.,
                (pw - 36.) * 0.5,
                44.,
            )
        },
        if r.medal == "GOLD" {
            "ONE MORE TRY"
        } else {
            "RETRY FOR GOLD"
        },
        false,
    );
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
            results: None,
        };
        assert_eq!(view.selection(2), Command::Select(2));
        view.resumable = true;
        assert_eq!(view.selection(2), Command::Toggle);
        assert_eq!(view.selection(1), Command::Select(1));
        view.resumable = false; // Completion offers a fresh trial.
        assert_eq!(view.selection(2), Command::Select(2));
    }
}
