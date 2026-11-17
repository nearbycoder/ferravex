use crate::{
    courses::{COURSES, Course},
    juice::{self, CORAL, GREEN, VIOLET},
    look::{self, GOLD, Glyph, INK, Item, Line, MUTED, TEAL, glyph, line},
    quips::{self, NextGoal, Rank},
    sim::Run,
    timing::Comparison,
};
use ferravex_engine::play::tween::{Appearance, Entrance, Motion, RollCounter};
use ferravex_render::{
    icons::{self, Icon},
    input::PlayerInput,
    prelude::*,
    shapes::{self, Fill, Shadow},
    text::text_width,
    ui::{
        Ui, UiRect, Viewport,
        kit::{self, ButtonKind, ButtonMotion, KitTheme},
    },
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
    /// The picker is the game's front door: no run has started yet.
    pub front: bool,
}

/// Eased menu state that outlives one frame: per-button hover/press/focus,
/// time since the current menu opened and the rolling finish time.
pub(crate) struct Menu {
    buttons: [ButtonMotion; 8],
    opened: f32,
    showing: u8,
    roll: RollCounter,
    /// Render seconds, for the title's idle motion.
    time: f32,
}

impl Menu {
    pub fn new() -> Self {
        Self {
            buttons: [ButtonMotion::default(); 8],
            opened: 0.,
            showing: 0,
            roll: RollCounter::new(0),
            time: 0.,
        }
    }
}

fn slot(id: u64) -> usize {
    match id {
        1 => 0,
        2 => 1,
        4 => 2,
        5 => 3,
        id => (4 + id.saturating_sub(10) as usize).min(7),
    }
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
    pub next: NextGoal,
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
fn screen(r: UiRect) -> Rect {
    Rect::new(r.min[0], r.min[1], r.size[0], r.size[1])
}
/// The course button; its right edge stays left of the fullscreen menu corner.
pub(crate) fn course_rect(w: f32) -> UiRect {
    rect(12., 8., (w - 88.).min(240.), 44.)
}
/// Touch Recover sits below the fullscreen menu corner (y >= 60).
pub(crate) fn recover_rect(w: f32) -> UiRect {
    rect(w - 102., 64., 88., 32.)
}
/// Input stays in `Ui` (focus, pointer capture, gamepad); the kit paints.
fn hit(ui: &mut Ui, id: u64, r: UiRect) {
    ui.button(id, r, "", true, kit::INVISIBLE_BUTTON).unwrap();
}

/// Each course's signature hazard as a pictogram: water lanes, wind-swept
/// rails, clockwork sweepers and ice.
pub(crate) fn hazard(index: usize) -> Icon {
    [Icon::Wave, Icon::Wind, Icon::Gear, Icon::Snowflake][index % 4]
}

/// Difficulty as 1-3 bolts, from the authored challenge's first word.
pub(crate) fn difficulty(course: &Course) -> usize {
    match course.challenge.split(" / ").next() {
        Some("Warm-up") => 1,
        Some("Hard") => 2,
        _ => 3,
    }
}

fn medal_of(score: u64) -> &'static str {
    match score {
        3 => "GOLD",
        2 => "SILVER",
        _ => "BRONZE",
    }
}

/// Per-course accent: sandstone, verdigris, rust and ice.
pub(crate) fn accent(index: usize) -> Color {
    [
        Color::new(0.80, 0.58, 0.40, 1.),
        Color::new(0.30, 0.64, 0.58, 1.),
        Color::new(0.76, 0.38, 0.25, 1.),
        Color::new(0.56, 0.78, 0.88, 1.),
    ][index % 4]
}

fn medal_name(medal: &str) -> &'static str {
    match medal {
        "GOLD" => "Gold",
        "SILVER" => "Silver",
        _ => "Bronze",
    }
}

pub fn draw(
    ui: &mut Ui,
    player: &mut PlayerInput,
    run: &Run,
    view: View,
    records: &crate::timing::TrialRecords,
    menu: &mut Menu,
    dt: f32,
) -> Option<Command> {
    let View {
        course,
        picker,
        resumable,
        can_recover,
        context,
        front,
        ..
    } = view;
    let w = screen_width();
    let h = screen_height();
    // A hidden/collapsed player may briefly report a zero-sized viewport.
    if w < 160. || h < 144. {
        return None;
    }
    let motion = look::motion();
    let dt = if dt.is_finite() {
        dt.clamp(0., 0.1)
    } else {
        0.
    };
    menu.time += dt;
    let results = view.results.filter(|_| run.finished);
    let showing = if picker {
        1
    } else if results.is_some() {
        2
    } else {
        0
    };
    if showing != menu.showing {
        menu.showing = showing;
        menu.opened = 0.;
        if let Some(r) = results.filter(|_| showing == 2) {
            menu.roll = RollCounter::new(0);
            menu.roll.duration = 0.9;
            menu.roll.set_target((r.time * 100.).round() as i64);
        }
    } else {
        menu.opened += dt;
    }

    ui.begin(Viewport::fit([w, h], [w, h], [0.; 4]).unwrap());
    ui.set_auto_focus(picker || results.is_some());
    hit(ui, 1, course_rect(w));
    let recover = !picker && can_recover && player.touch_seen();
    if recover {
        hit(ui, 2, recover_rect(w));
    }
    let pw = (w - 24.).min(560.);
    let ph = if picker {
        (h - 16.).min(268.)
    } else {
        (h - 16.).min(304.)
    };
    let x = (w - pw) * 0.5;
    let y = (h - ph) * 0.5;
    let cw = (pw - 36.) * 0.5;
    let ch = (ph - 48.) * 0.5;
    let card_rect = |i: usize| {
        rect(
            x + 12. + (i % 2) as f32 * (cw + 12.),
            y + 36. + (i / 2) as f32 * (ch + 8.),
            cw,
            ch,
        )
    };
    let stacked = ph >= 188.;
    let next_rect = if stacked {
        rect(x + 14., y + ph - 104., pw - 28., 44.)
    } else {
        rect(x + 12., y + ph - 50., (pw - 36.) * 0.5, 44.)
    };
    let retry_rect = if stacked {
        rect(x + 14., y + ph - 52., pw - 28., 44.)
    } else {
        rect(
            x + 24. + (pw - 36.) * 0.5,
            y + ph - 50.,
            (pw - 36.) * 0.5,
            44.,
        )
    };
    if picker || results.is_some() {
        ui.modal(100).unwrap();
        // The browser contract detects open menus by this exact overlay color.
        ui.panel(rect(0., 0., w, h), Color::new(0.07, 0.13, 0.14, 0.7))
            .unwrap();
        if picker {
            for i in 0..COURSES.len() {
                hit(ui, 10 + i as u64, card_rect(i));
            }
        } else {
            hit(ui, 4, next_rect);
            hit(ui, 5, retry_rect);
        }
    }
    ui.finish_player(player, !player.controls_open());
    let mut state = |ui: &Ui, id: u64| {
        let b = &mut menu.buttons[slot(id)];
        b.update(kit::ui_button_state(ui, id, true), dt, motion);
        *b
    };
    let course_button = state(ui, 1);
    let recover_button = state(ui, 2);
    let card_buttons: [ButtonMotion; 4] = std::array::from_fn(|i| state(ui, 10 + i as u64));
    let next_button = state(ui, 4);
    let retry_button = state(ui, 5);

    // HUD-level buttons sit under the menu overlay, so paint them first.
    course_chip(
        screen(course_rect(w)),
        COURSES[course].name,
        accent(course),
        picker,
        &course_button,
        player,
    );
    if recover {
        // Back to the flag: an icon button for touch players.
        let r = screen(recover_rect(w));
        let theme = look::theme();
        kit::button(r, "", ButtonKind::Secondary, &recover_button, &theme);
        look::draw_row_centered(
            &[
                Item::Icon(Glyph::Icon(glyph::RECOVER), INK),
                Item::Icon(Glyph::Icon(glyph::CHECKPOINT), TEAL),
            ],
            r.x + r.w * 0.5,
            r.y + r.h * 0.5 + 2. * recover_button.press(),
            18.,
            &theme,
        );
    }
    ui.paint();
    if picker {
        let entrance = Entrance::fade_up(14.).with_stagger(0.05);
        let look_in = entrance.sample(menu.opened, 0, motion);
        // Earned medals per course (current course content for the current one).
        let medals: [Option<&'static str>; 4] = std::array::from_fn(|i| {
            let c = &COURSES[i];
            records
                .book
                .get(c.id, if i == course { context } else { c.context() })
                .filter(|r| r.fastest_us.is_some())
                .map(|r| medal_of(r.score))
        });
        if front {
            title(w, y, menu, motion, medals);
        }
        let panel = kit::appear(Rect::new(x, y, pw, ph), look_in);
        let theme = look::theme().faded(look_in.alpha);
        kit::panel(panel, &theme);
        // A map headline; a paused run adds the pause mark.
        let head = look::body(24., INK);
        let header: &[Item] = if resumable {
            &[
                Item::Icon(Glyph::Icon(glyph::COURSES), GOLD),
                Item::Text("Courses", head),
                Item::Space(8.),
                Item::Small(Glyph::Icon(glyph::PAUSED), MUTED, 16.),
            ]
        } else {
            &[
                Item::Icon(Glyph::Icon(glyph::COURSES), GOLD),
                Item::Text("Courses", head),
            ]
        };
        look::draw_row_centered(header, panel.x + panel.w * 0.5, panel.y + 19., 22., &theme);
        for (i, c) in COURSES.iter().enumerate() {
            let record = records
                .book
                .get(c.id, if i == course { context } else { c.context() });
            course_card(
                kit::appear(
                    screen(card_rect(i)),
                    entrance.sample(menu.opened, i + 1, motion),
                ),
                i,
                c,
                record.map(|r| (r.score, r.fastest_us)),
                (i == course, i == course && resumable),
                &card_buttons[i],
                (
                    entrance.sample(menu.opened, i + 1, motion).alpha,
                    menu.time,
                    motion,
                ),
            );
        }
        if front && h - (y + ph) >= 52. {
            let theme = look::theme().faded(entrance.sample(menu.opened, 6, motion).alpha);
            let pulse = if motion.is_reduced() {
                1.
            } else {
                0.75 + 0.25 * (menu.time * 3.).sin()
            };
            look::draw_row_centered(
                &[
                    Item::Key("Enter"),
                    Item::Icon(Glyph::Icon(glyph::RESUME), GOLD),
                ],
                w * 0.5,
                y + ph + 28.,
                22.,
                &theme.faded(pulse),
            );
        }
    } else if let Some(results) = results {
        let shown = menu.roll.advance(dt, motion);
        finish_card(
            view.course,
            records,
            results,
            (x, y, pw, ph),
            (
                screen(next_rect),
                &next_button,
                screen(retry_rect),
                &retry_button,
            ),
            (menu.opened, motion, shown),
        );
    }
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

/// The front door: an idling logo with a rolling marble above the picker.
fn title(w: f32, panel_y: f32, menu: &Menu, motion: Motion, medals: [Option<&'static str>; 4]) {
    if panel_y < 104. {
        return;
    }
    let look_in = Entrance::fade_up(18.).sample(menu.opened, 0, motion);
    let bob = if motion.is_reduced() {
        0.
    } else {
        (menu.time * 1.6).sin() * 3.
    };
    let size = if w >= 560. { 48. } else { 32. };
    let logo = look::centered(
        look::punch(size, look::fade(INK, look_in.alpha))
            .with_outline(look::fade(Color::new(0.05, 0.1, 0.11, 0.9), look_in.alpha))
            .with_letter_spacing(1.),
    );
    let cy = (panel_y * 0.5 - 6.).max(48.) + look_in.offset[1] + bob;
    let text_w = text_width("MARBLE RUN", logo);
    look::draw(
        "MARBLE RUN",
        w * 0.5 + 22.,
        logo.centered_baseline(cy),
        logo,
    );
    // A little marble that rolls in place beside the logo.
    let r = size * 0.42;
    let c = vec2(w * 0.5 - text_w * 0.5 - r - 4. + 22., cy);
    let spin = if motion.is_reduced() {
        0.4
    } else {
        menu.time * 2.4
    };
    shapes::glow_disc(c, r * 2.4, look::fade(GOLD, 0.28 * look_in.alpha));
    look::marble(c, r, spin, look_in.alpha);
    // The medal shelf: one trophy per course, lit in the medal earned.
    if panel_y - (cy + size * 0.5) > 30. {
        let y = cy + size * 0.5 + 18.;
        for (i, medal) in medals.iter().enumerate() {
            let x = w * 0.5 + (i as f32 - 1.5) * 30.;
            let color = match medal {
                Some(medal) => juice::medal_color(medal),
                None => look::fade(MUTED, 0.35),
            };
            if medal.is_some() {
                shapes::glow_disc(vec2(x, y), 16., look::fade(color, 0.3 * look_in.alpha));
            }
            look::icon(
                glyph::MEDAL,
                vec2(x, y),
                20.,
                look::fade(color, look_in.alpha),
            );
        }
    }
}

/// The top-left course chip doubles as the picker button.
fn course_chip(
    r: Rect,
    name: &str,
    tint: Color,
    selected: bool,
    m: &ButtonMotion,
    player: &PlayerInput,
) {
    let r = Rect::new(r.x, r.y + 2. * m.press(), r.w, r.h);
    look::glass(r, 12., 1.);
    if m.hover() > 0.01 {
        shapes::fill_rounded_rect(r, 12., Color::new(1., 1., 1., 0.06 * m.hover()));
    }
    let edge = if selected {
        GOLD
    } else {
        look::fade(look::theme().focus, m.focus())
    };
    if selected || m.focus() > 0.01 {
        shapes::stroke_rounded_rect(r, 12., 2., edge);
    }
    let dot = vec2(r.x + 18., r.y + r.h * 0.5);
    shapes::glow_disc(dot, 12., look::fade(tint, 0.4));
    shapes::fill_circle(dot, 6., tint);
    let key = crate::hud::hint_line(player, "courses");
    let key_style = look::body(16., MUTED);
    let key_w = text_width(key.as_str(), key_style) + 14.;
    let style = look::fit(name, r.w - 46. - key_w, look::body(16., INK));
    look::draw(
        name,
        r.x + 32.,
        style.centered_baseline(r.y + r.h * 0.5),
        style,
    );
    let cap = Rect::new(r.x + r.w - key_w - 8., r.y + 10., key_w, r.h - 20.);
    shapes::fill_rounded_rect(cap, 6., Color::new(1., 1., 1., 0.08));
    look::draw(
        key.as_str(),
        cap.x + 7.,
        key_style.centered_baseline(cap.y + cap.h * 0.5),
        key_style,
    );
}

fn course_card(
    r: Rect,
    index: usize,
    c: &Course,
    record: Option<(u64, Option<u64>)>,
    (selected, resume): (bool, bool),
    m: &ButtonMotion,
    (alpha, time, motion): (f32, f32, Motion),
) {
    if alpha <= 0. {
        return;
    }
    let lift = 2. * m.hover() - 2. * m.press();
    let r = Rect::new(r.x, r.y - lift, r.w, r.h);
    let tint = accent(index);
    let dark = Color::new(0.05, 0.09, 0.1, 1.);
    shapes::drop_shadow(
        r,
        12.,
        Shadow::soft(Color::new(0., 0., 0., 0.4 * alpha))
            .offset(vec2(0., 4. + lift))
            .blur(14.),
    );
    let k = 0.42 - 0.1 * m.hover();
    shapes::fill_rounded_rect(
        r,
        12.,
        Fill::vertical(
            r,
            look::fade(shapes::mix(tint, dark, k), alpha),
            look::fade(shapes::mix(tint, dark, k + 0.28), alpha),
        ),
    );
    // A bright sky band across the top: each card reads as a little postcard.
    let band = Rect::new(r.x + 1., r.y + 1., r.w - 2., (r.h * 0.18).min(18.));
    shapes::fill_rounded_rect(
        band,
        11.,
        Fill::vertical(
            band,
            look::fade(Color::new(1., 1., 1., 0.14), alpha),
            look::fade(Color::new(1., 1., 1., 0.), alpha),
        ),
    );
    shapes::stroke_rounded_rect(
        r,
        12.,
        if selected { 2. } else { 1. },
        look::fade(
            if selected {
                GOLD
            } else {
                Color::new(1., 1., 1., 0.12 + 0.18 * m.hover())
            },
            alpha,
        ),
    );
    if m.focus() > 0.01 {
        let g = 3. + (1. - m.focus());
        shapes::stroke_rounded_rect(
            Rect::new(r.x - g, r.y - g, r.w + 2. * g, r.h + 2. * g),
            12. + g,
            2.,
            look::fade(look::theme().focus, m.focus() * alpha),
        );
    }
    let cx = r.x + r.w * 0.5;
    let name = look::fit(
        c.name,
        r.w - 16.,
        look::centered(look::body(24., look::fade(INK, alpha))),
    );
    let tall = r.h >= 92.;
    let mid = if tall {
        r.y + r.h * 0.48
    } else if r.h >= 60. {
        r.y + r.h * 0.38
    } else {
        r.y + r.h * 0.5
    };
    look::draw(c.name, cx, name.centered_baseline(mid), name);
    let ink = shapes::mix(tint, INK, 0.55);
    let bolts = difficulty(c);
    let theme = look::theme().faded(alpha);
    if tall {
        // Difficulty as bolts at the top left; the course's hazard (or a
        // pulsing resume mark for the paused run) at the top right.
        for i in 0..3 {
            let lit = i < bolts;
            look::icon(
                glyph::DIFFICULTY,
                vec2(r.x + 18. + i as f32 * 15., r.y + 18.),
                14.,
                look::fade(
                    if lit { GOLD } else { ink },
                    alpha * if lit { 1. } else { 0.4 },
                ),
            );
        }
        if resume {
            let pulse = if motion.is_reduced() {
                1.
            } else {
                1. + 0.08 * (time * 4.).sin()
            };
            icons::badge(
                glyph::RESUME,
                vec2(r.x + r.w - 22., r.y + 20.),
                26. * pulse,
                look::fade(GOLD, alpha),
                look::fade(Color::new(0.16, 0.10, 0.03, 1.), alpha),
            );
        } else {
            look::icon(
                hazard(index),
                vec2(r.x + r.w - 22., r.y + 19.),
                20.,
                look::fade(ink, alpha),
            );
        }
    }
    if r.h >= 60. {
        // The earned medal and best time, or the gold target to chase.
        let (time, medal_color, earned) = match record {
            Some((score, Some(us))) => (
                crate::hud::stopwatch_line(us as f32 / 1e6),
                juice::medal_color(medal_of(score)),
                true,
            ),
            _ => (crate::hud::target_time(c.gold_seconds), GOLD, false),
        };
        let text = look::body(16., look::fade(INK, 0.92));
        let trophy = Item::Icon(
            Glyph::Icon(glyph::MEDAL),
            if earned {
                medal_color
            } else {
                look::fade(GOLD, 0.6)
            },
        );
        let bolt = |lit: bool| {
            Item::Small(
                Glyph::Icon(glyph::DIFFICULTY),
                if lit { GOLD } else { look::fade(ink, 0.4) },
                13.,
            )
        };
        let full = [
            bolt(bolts > 0),
            bolt(bolts > 1),
            bolt(bolts > 2),
            Item::Space(12.),
            trophy,
            Item::Text(time.as_str(), text),
        ];
        // Short cards carry the difficulty down here with the medal.
        let items = if tall { &full[4..] } else { &full[..] };
        let size = 18.;
        let items = if look::row_width(items, size, &theme) <= r.w - 16. {
            items
        } else {
            &full[4..]
        };
        if earned {
            shapes::glow_disc(
                vec2(
                    cx - look::row_width(items, size, &theme) * 0.5 + 9.,
                    r.y + r.h - 16.,
                ),
                16.,
                look::fade(medal_color, 0.3 * alpha),
            );
        }
        look::draw_row_centered(items, cx, r.y + r.h - 16., size, &theme);
    }
}

/// The results card: medal and rolling time, the comparison with the prior
/// best, a silly rank and the next goal, fitted by priority to the height.
fn finish_card(
    course: usize,
    records: &crate::timing::TrialRecords,
    r: &Results,
    (x, y, pw, ph): (f32, f32, f32, f32),
    (next_rect, next_button, retry_rect, retry_button): (Rect, &ButtonMotion, Rect, &ButtonMotion),
    (opened, motion, shown_centis): (f32, Motion, i64),
) {
    let c = &COURSES[course];
    let entrance = Entrance::fade_up(12.).with_stagger(0.06);
    let panel_look = Entrance::pop(0.94).sample(opened, 0, motion);
    let theme = look::theme().faded(panel_look.alpha);
    let panel = kit::appear(Rect::new(x, y, pw, ph), panel_look);
    kit::panel(panel, &theme);
    let medal_color = juice::medal_color(r.medal);
    let ribbon = Rect::new(panel.x, panel.y, panel.w, 4.);
    shapes::fill_rounded_rect(ribbon, 2., look::fade(medal_color, panel_look.alpha));
    let buttons_top = next_rect.y;
    // (priority, height); lower priority numbers survive first.
    const ROWS: [(u8, f32); 7] = [
        (6, 22.),
        (0, 44.),
        (1, 24.),
        (4, 22.),
        (2, 28.),
        (5, 22.),
        (3, 22.),
    ];
    let available = buttons_top - y - 14.;
    let mut keep = [false; 7];
    let mut used = 0.;
    for priority in 0..7 {
        for (i, row) in ROWS.iter().enumerate() {
            if row.0 == priority && used + row.1 <= available {
                keep[i] = true;
                used += row.1;
            }
        }
    }
    let cx = panel.x + panel.w * 0.5;
    let mut cursor = y + 12. + ((available - used) * 0.5).clamp(0., 12.);
    let mut shown = 0;
    let body = |color: Color| look::body(16., color);
    for (i, row) in ROWS.iter().enumerate() {
        if !keep[i] {
            continue;
        }
        let look_in = entrance.sample(opened, shown + 1, motion);
        shown += 1;
        let a = look_in.alpha * panel_look.alpha;
        let mid = cursor + row.1 * 0.5 + look_in.offset[1];
        let width = pw - 32.;
        let row_theme = look::theme().faded(a);
        let fit_row = |items: &[Item]| {
            if look::row_width(items, 18., &row_theme) <= width {
                look::draw_row_centered(items, cx, mid, 18., &row_theme);
            }
        };
        match i {
            0 => fit_row(&[
                Item::Icon(Glyph::Icon(glyph::CHECKPOINT), TEAL),
                Item::Text(c.name, body(TEAL)),
            ]),
            1 => medal_and_time(r, cx, mid, a, shown_centis),
            2 => {
                let delta = match records.comparison() {
                    Comparison::NewBest(d) | Comparison::VsBest(d) => crate::hud::signed_seconds(d),
                    _ => Line::new(),
                };
                let crown = |color| Item::Icon(Glyph::Icon(glyph::BEST), color);
                let items: &[Item] = if records.error().is_some() {
                    // The session keeps the record even when saving failed.
                    &[
                        Item::Icon(Glyph::Icon(glyph::WARNING), CORAL),
                        Item::Text("NOT SAVED", body(CORAL)),
                    ]
                } else {
                    match records.comparison() {
                        Comparison::First => &[crown(GOLD), Item::Text("FIRST CLEAR", body(GOLD))],
                        Comparison::Tie => &[crown(INK), Item::Text("TIED", body(INK))],
                        Comparison::NewBest(_) => &[
                            crown(GREEN),
                            Item::Text("NEW BEST", body(GREEN)),
                            Item::Text(delta.as_str(), body(GREEN)),
                        ],
                        Comparison::VsBest(_) => {
                            &[crown(MUTED), Item::Text(delta.as_str(), body(CORAL))]
                        }
                        Comparison::Unavailable => &[],
                    }
                };
                fit_row(items);
            }
            3 => {
                let gems = line!("{}", r.gems);
                let falls = line!("{}", r.falls);
                let bonks = line!("{}", r.bonks);
                let mut items = [Item::Space(0.); 11];
                let mut n = 0;
                for item in [
                    Item::Icon(Glyph::Icon(glyph::CRYSTAL), GOLD),
                    Item::Text(gems.as_str(), body(INK)),
                    Item::Space(18.),
                    Item::Icon(
                        Glyph::Icon(glyph::FALL),
                        if r.falls == 0 { MUTED } else { CORAL },
                    ),
                    Item::Text(falls.as_str(), body(INK)),
                ] {
                    items[n] = item;
                    n += 1;
                }
                if r.bonks > 0 {
                    for item in [
                        Item::Space(18.),
                        Item::Icon(Glyph::Icon(glyph::BONK), CORAL),
                        Item::Text(bonks.as_str(), body(INK)),
                    ] {
                        items[n] = item;
                        n += 1;
                    }
                }
                if r.secret {
                    items[n] = Item::Space(18.);
                    items[n + 1] = Item::Icon(Glyph::Icon(glyph::CRYSTAL), VIOLET);
                    n += 2;
                }
                fit_row(&items[..n]);
            }
            4 => {
                let s = look::fit(
                    r.rank.title,
                    width,
                    look::centered(look::body(
                        24.,
                        look::fade(Color::new(1., 0.88, 0.62, 1.), a),
                    )),
                );
                look::draw(r.rank.title, cx, s.centered_baseline(mid), s);
            }
            5 => {
                let s = look::fit(
                    r.rank.quip,
                    width,
                    look::centered(look::body(16., look::fade(MUTED, a))),
                );
                look::draw(r.rank.quip, cx, s.centered_baseline(mid), s);
            }
            _ => {
                // The next goal, as a target followed by what it takes.
                let aim = Item::Icon(Glyph::Icon(glyph::AIM), MUTED);
                match r.next {
                    NextGoal::Medal {
                        medal,
                        seconds,
                        falls,
                    } => {
                        let time = crate::hud::target_time(seconds);
                        let falls = if falls == 0 {
                            line!("0")
                        } else {
                            line!("≤{falls}")
                        };
                        fit_row(&[
                            aim,
                            Item::Space(6.),
                            Item::Icon(Glyph::Icon(glyph::MEDAL), juice::medal_color(medal)),
                            Item::Text(time.as_str(), body(INK)),
                            Item::Space(12.),
                            Item::Icon(Glyph::Icon(glyph::FALL), MUTED),
                            Item::Text(falls.as_str(), body(INK)),
                        ]);
                    }
                    NextGoal::Moonstone => fit_row(&[
                        aim,
                        Item::Space(6.),
                        Item::Icon(Glyph::Icon(glyph::CRYSTAL), VIOLET),
                        Item::Icon(Glyph::Icon(glyph::SECRET), VIOLET),
                    ]),
                    NextGoal::Best(best) => {
                        let best = crate::hud::stopwatch_line(best);
                        fit_row(&[
                            aim,
                            Item::Space(6.),
                            Item::Icon(Glyph::Icon(glyph::BEST), GOLD),
                            Item::Text(best.as_str(), body(GOLD)),
                        ]);
                    }
                    NextGoal::Faster => fit_row(&[
                        aim,
                        Item::Space(6.),
                        Item::Icon(Glyph::Icon(glyph::SPEED), GOLD),
                    ]),
                }
            }
        }
        cursor += row.1;
    }
    let button_look = entrance.sample(opened, shown + 1, motion);
    let theme = look::theme().faded(button_look.alpha * panel_look.alpha);
    let next = kit::appear(next_rect, button_look);
    kit::button(next, "", ButtonKind::Primary, next_button, &theme);
    let ink = theme.accent_ink;
    let label = look::body(16., ink).with_shadow(Color::new(0., 0., 0., 0.));
    let next_items = if course + 1 < COURSES.len() {
        [
            Item::Text(COURSES[course + 1].name, label),
            Item::Icon(Glyph::Icon(glyph::NEXT), ink),
        ]
    } else {
        [
            Item::Icon(Glyph::Icon(glyph::COURSES), ink),
            Item::Text("Courses", label),
        ]
    };
    button_row(&next_items, next, next_button, &theme);
    let retry = kit::appear(retry_rect, button_look);
    kit::button(retry, "", ButtonKind::Secondary, retry_button, &theme);
    let again = look::body(16., INK);
    let retry_items: &[Item] = if r.medal == "GOLD" {
        &[
            Item::Icon(Glyph::Icon(glyph::ATTEMPT), INK),
            Item::Text("One more try", again),
        ]
    } else {
        &[
            Item::Icon(Glyph::Icon(glyph::ATTEMPT), INK),
            Item::Text("Retry", again),
            Item::Space(6.),
            Item::Icon(Glyph::Icon(glyph::MEDAL), GOLD),
        ]
    };
    button_row(retry_items, retry, retry_button, &theme);
    let _ = Appearance::SHOWN;
}

/// Icons and a short label centred on a kit button, following its press.
fn button_row(items: &[Item], r: Rect, m: &ButtonMotion, theme: &KitTheme) {
    look::draw_row_centered(
        items,
        r.x + r.w * 0.5,
        r.y + r.h * 0.5 + 2. * m.press(),
        18.,
        theme,
    );
}

/// The medal chip (trophy + medal name) beside the rolling finish time.
fn medal_and_time(r: &Results, cx: f32, mid: f32, a: f32, shown_centis: i64) {
    let medal_color = juice::medal_color(r.medal);
    let centis = shown_centis.max(0) as f32 / 100.;
    let time = crate::hud::stopwatch_line(centis);
    let ts = look::punch(32., look::fade(INK, a));
    let dark = Color::new(0.12, 0.08, 0.03, 1.);
    let ms = look::body(16., look::fade(dark, a)).with_shadow(Color::new(0., 0., 0., 0.));
    let name = medal_name(r.medal);
    let chip_w = 18. + 6. + text_width(name, ms) + 24.;
    let total = chip_w + 14. + text_width(time.as_str(), ts);
    let mut x0 = cx - total * 0.5;
    let chip = Rect::new(x0, mid - 15., chip_w, 30.);
    shapes::glow_disc(
        chip.center(),
        chip_w * 0.8,
        look::fade(medal_color, 0.3 * a),
    );
    shapes::fill_rounded_rect(
        chip,
        15.,
        Fill::vertical(
            chip,
            look::fade(
                shapes::mix(medal_color, Color::new(1., 1., 1., 1.), 0.35),
                a,
            ),
            look::fade(medal_color, a),
        ),
    );
    look::icon(
        glyph::MEDAL,
        vec2(x0 + 12. + 9., mid),
        18.,
        look::fade(dark, a),
    );
    look::draw(name, x0 + 12. + 18. + 6., ms.centered_baseline(mid), ms);
    x0 += chip_w + 14.;
    look::draw(time.as_str(), x0, ts.centered_baseline(mid), ts);
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
            front: false,
        };
        assert_eq!(view.selection(2), Command::Select(2));
        view.resumable = true;
        assert_eq!(view.selection(2), Command::Toggle);
        assert_eq!(view.selection(1), Command::Select(1));
        view.resumable = false; // Completion offers a fresh trial.
        assert_eq!(view.selection(2), Command::Select(2));
    }
}
