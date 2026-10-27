use crate::{
    courses::{COURSES, Course},
    juice::{self, GREEN, VIOLET},
    look::{self, GOLD, INK, MUTED, TEAL, line},
    quips::{self, Rank},
    sim::Run,
};
use ferravex_engine::play::tween::{Appearance, Entrance, Motion, RollCounter};
use ferravex_render::{
    input::PlayerInput,
    prelude::*,
    shapes::{self, Fill, Shadow},
    text::text_width,
    ui::{
        Ui, UiRect, Viewport,
        kit::{self, ButtonKind, ButtonMotion},
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
        kit::button(
            screen(recover_rect(w)),
            "Recover",
            ButtonKind::Secondary,
            &recover_button,
            &look::theme(),
        );
    }
    ui.paint();
    if picker {
        let entrance = Entrance::fade_up(14.).with_stagger(0.05);
        let look_in = entrance.sample(menu.opened, 0, motion);
        if front {
            title(w, y, menu, motion);
        }
        let panel = kit::appear(Rect::new(x, y, pw, ph), look_in);
        let theme = look::theme().faded(look_in.alpha);
        kit::panel(panel, &theme);
        let t = look::fit(
            "Choose your course",
            pw - 24.,
            look::centered(look::body(24., look::fade(INK, look_in.alpha))),
        );
        look::draw(
            if resumable {
                "Courses · run paused"
            } else {
                "Choose your course"
            },
            panel.x + panel.w * 0.5,
            t.centered_baseline(panel.y + 19.),
            t,
        );
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
                entrance.sample(menu.opened, i + 1, motion).alpha,
            );
        }
        if front && h - (y + ph) >= 52. {
            let theme = look::theme().faded(entrance.sample(menu.opened, 6, motion).alpha);
            let keys = ["Enter"];
            let label = "Roll out";
            let width = look::prompt_width(&keys, label, &theme);
            let pulse = if motion.is_reduced() {
                1.
            } else {
                0.75 + 0.25 * (menu.time * 3.).sin()
            };
            kit::key_prompt(
                (w - width) * 0.5,
                y + ph + 16.,
                &keys,
                label,
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
fn title(w: f32, panel_y: f32, menu: &Menu, motion: Motion) {
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
    shapes::fill_circle(
        c,
        r,
        Fill::radial(
            c - vec2(r * 0.35, r * 0.35),
            r * 1.4,
            look::fade(Color::new(1., 0.98, 0.92, 1.), look_in.alpha),
            look::fade(Color::new(0.78, 0.72, 0.6, 1.), look_in.alpha),
        ),
    );
    let band = vec2(spin.cos(), spin.sin()) * r * 0.82;
    shapes::fill_capsule(
        c - band,
        c + band,
        r * 0.22,
        look::fade(GOLD, look_in.alpha),
    );
    shapes::stroke_circle(
        c,
        r,
        1.5,
        look::fade(Color::new(0.05, 0.1, 0.11, 0.8), look_in.alpha),
    );
    let sub = look::centered(look::body(16., look::fade(MUTED, look_in.alpha)));
    if panel_y - (cy + size * 0.5) > 26. {
        look::draw(
            "Four sky courses. One more try.",
            w * 0.5,
            sub.centered_baseline(cy + size * 0.5 + 16.),
            sub,
        );
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
    alpha: f32,
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
    if tall {
        let difficulty = c.challenge.split(" / ").next().unwrap_or("");
        let s = look::body(16., look::fade(shapes::mix(tint, INK, 0.55), alpha));
        look::draw(difficulty, r.x + 12., s.centered_baseline(r.y + 18.), s);
        if resume {
            let label = "Resume ▶";
            let rs = look::body(16., look::fade(GOLD, alpha));
            let lw = text_width(label, rs);
            look::draw(
                label,
                r.x + r.w - 12. - lw,
                rs.centered_baseline(r.y + 18.),
                rs,
            );
        }
    }
    if r.h >= 60. {
        let (label, dot) = match record {
            Some((score, Some(us))) => {
                let medal = match score {
                    3 => "GOLD",
                    2 => "SILVER",
                    _ => "BRONZE",
                };
                (
                    line!(
                        "{} · best {}",
                        medal_name(medal),
                        crate::hud::stopwatch_line(us as f32 / 1e6)
                    ),
                    Some(juice::medal_color(medal)),
                )
            }
            _ if tall => (line!("Gold target {:.0}s", c.gold_seconds), None),
            _ => (
                line!(
                    "{} · gold {:.0}s",
                    c.challenge.split(" / ").next().unwrap_or(""),
                    c.gold_seconds
                ),
                None,
            ),
        };
        let s = look::fit(
            label.as_str(),
            r.w - 36.,
            look::body(16., look::fade(INK, alpha * 0.92)),
        );
        let lw = text_width(label.as_str(), s) + if dot.is_some() { 14. } else { 0. };
        let y = r.y + r.h - 16.;
        let mut x = cx - lw * 0.5;
        if let Some(dot) = dot {
            shapes::fill_circle(vec2(x + 4., y), 4.5, look::fade(dot, alpha));
            x += 14.;
        }
        look::draw(label.as_str(), x, s.centered_baseline(y), s);
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
    let (comparison, best) = if records.error().is_some() {
        (line!("Session record · save unavailable"), false)
    } else {
        crate::hud::pretty_finish(&records.finish_label())
    };
    let mut stats = line!("All {} crystals · falls {}", r.gems, r.falls);
    if r.bonks > 0 {
        let _ = std::fmt::Write::write_fmt(&mut stats, format_args!(" · bonks {}", r.bonks));
    }
    if r.secret {
        let _ = std::fmt::Write::write_str(&mut stats, " · moonstone ♦");
    }
    let header = line!("{} complete", c.name);
    // (priority, height); lower priority numbers survive first.
    const ROWS: [(u8, f32); 7] = [
        (6, 22.),
        (0, 48.),
        (1, 24.),
        (4, 22.),
        (2, 30.),
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
    for (i, row) in ROWS.iter().enumerate() {
        if !keep[i] {
            continue;
        }
        let look_in = entrance.sample(opened, shown + 1, motion);
        shown += 1;
        let a = look_in.alpha * panel_look.alpha;
        let mid = cursor + row.1 * 0.5 + look_in.offset[1];
        let width = pw - 32.;
        match i {
            0 => {
                let s = look::fit(
                    header.as_str(),
                    width,
                    look::centered(look::body(16., look::fade(TEAL, a))),
                );
                look::draw(header.as_str(), cx, s.centered_baseline(mid), s);
            }
            1 => {
                let centis = shown_centis.max(0) as f32 / 100.;
                let time = crate::hud::stopwatch_line(centis);
                let ts = look::punch(32., look::fade(INK, a));
                let medal = line!("★ {}", medal_name(r.medal));
                let ms = look::body(16., look::fade(Color::new(0.12, 0.08, 0.03, 1.), a));
                let chip_w = text_width(medal.as_str(), ms) + 24.;
                let total = chip_w + 14. + text_width(time.as_str(), ts);
                let mut x0 = cx - total * 0.5;
                let chip = Rect::new(x0, mid - 14., chip_w, 28.);
                shapes::glow_disc(
                    chip.center(),
                    chip_w * 0.8,
                    look::fade(medal_color, 0.3 * a),
                );
                shapes::fill_rounded_rect(
                    chip,
                    14.,
                    Fill::vertical(
                        chip,
                        look::fade(
                            shapes::mix(medal_color, Color::new(1., 1., 1., 1.), 0.35),
                            a,
                        ),
                        look::fade(medal_color, a),
                    ),
                );
                look::draw(medal.as_str(), x0 + 12., ms.centered_baseline(mid), ms);
                x0 += chip_w + 14.;
                look::draw(time.as_str(), x0, ts.centered_baseline(mid), ts);
            }
            2 => {
                let s = look::fit(
                    comparison.as_str(),
                    width,
                    look::centered(look::body(
                        16.,
                        look::fade(if best { GREEN } else { INK }, a),
                    )),
                );
                look::draw(comparison.as_str(), cx, s.centered_baseline(mid), s);
            }
            3 => {
                let s = look::fit(
                    stats.as_str(),
                    width,
                    look::centered(look::body(
                        16.,
                        look::fade(if r.secret { VIOLET } else { MUTED }, a),
                    )),
                );
                look::draw(stats.as_str(), cx, s.centered_baseline(mid), s);
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
                let s = look::fit(
                    &r.next,
                    width,
                    look::centered(look::body(16., look::fade(GOLD, a))),
                );
                look::draw(&r.next, cx, s.centered_baseline(mid), s);
            }
        }
        cursor += row.1;
    }
    let next = if course + 1 < COURSES.len() {
        line!("Next: {} →", COURSES[course + 1].name)
    } else {
        line!("Choose a course")
    };
    let button_look = entrance.sample(opened, shown + 1, motion);
    let theme = look::theme().faded(button_look.alpha * panel_look.alpha);
    kit::button(
        kit::appear(next_rect, button_look),
        next.as_str(),
        ButtonKind::Primary,
        next_button,
        &theme,
    );
    kit::button(
        kit::appear(retry_rect, button_look),
        if r.medal == "GOLD" {
            "One more try"
        } else {
            "Retry for gold"
        },
        ButtonKind::Secondary,
        retry_button,
        &theme,
    );
    let _ = Appearance::SHOWN;
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
