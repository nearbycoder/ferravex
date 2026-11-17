//! The realm at a glance: the right page (four pillar meters, policy
//! leanings, royal works), the header's council track and the results card.
//!
//! Everything here is read-only. Geometry helpers are shared with
//! `feedback.rs` so effects rise exactly where the change is drawn.
use crate::model::Ending;
use crate::{
    Game,
    model::{PROJECTS, ProjectState, Screen, WORK_TILES},
    view::{
        BROWN, CREAM, FADED, GAIN, GOLD, LOSS, PILLARS, POLICIES, TEAL, VELLUM_SIDE, WORK_ICONS,
        centered, line, num, plain, promise_badge, seal,
    },
};
use ferravex_arcade::shapes::{self, Fill};
use ferravex_arcade::typography::{TextAlign, draw_text_styled};
use ferravex_arcade::*;

/// Pillar meter centres, left to right: treasury, commons, guard, chapel.
pub(crate) const GAUGE_X: [f32; 4] = [692., 760., 828., 896.];
pub(crate) const BADGE_Y: f32 = 152.;
pub(crate) const GAUGE_TOP: f32 = 182.;
pub(crate) const GAUGE_H: f32 = 148.;
const GAUGE_W: f32 = 26.;
/// A pillar at or below this value is in danger.
pub(crate) const DANGER: i16 = 20;
/// Executed ticks a meter takes to settle after a change.
const EASE_TICKS: f32 = 28.;
/// Policy leaning rows.
pub(crate) const LEAN_Y: f32 = 384.;
pub(crate) const LEAN_PITCH: f32 = 19.;
const SEAL_COLUMN: f32 = 675.;

/// Height on a pillar meter for `value` in `0..=100`.
pub(crate) fn level_y(value: f32) -> f32 {
    GAUGE_TOP + GAUGE_H * (1. - value.clamp(0., 100.) / 100.)
}
/// Where the `n`th (zero-based) pip of a leaning row sits.
pub(crate) fn pip(policy: usize, n: usize) -> Vec2 {
    vec2(
        692. + n as f32 * 6.4 + (n / 5) as f32 * 3.5,
        LEAN_Y + policy as f32 * LEAN_PITCH,
    )
}
/// A royal-work tile on the right page.
pub(crate) fn tile(j: usize) -> Rect {
    Rect::new(
        WORK_TILES.x + 2. + j as f32 * 65.,
        WORK_TILES.y + 4.,
        58.,
        78.,
    )
}
/// The displayed value of pillar `k`, easing from its previous value.
fn shown(game: &Game, k: usize) -> f32 {
    let to = f32::from(game.resources[k]);
    let since = f32::from(game.since);
    if game.outcome() != Outcome::Playing || since >= EASE_TICKS {
        return to;
    }
    let t = since / EASE_TICKS;
    let from = f32::from(game.eased_from[k]);
    from + (to - from) * (1. - (1. - t).powi(3))
}

/// The right page. `preview` is the change under consideration (ghosted on
/// the meters) and `policy` the leaning it would add a pip to.
pub(crate) fn realm(game: &Game, preview: Option<[i16; 4]>, policy: Option<usize>) {
    let t = clock();
    for k in 0..4 {
        gauge(game, k, preview.map(|d| d[k]), t);
    }
    divider(668., 370., 250., with_alpha(GOLD, 0.55));
    leanings(game, policy, t);
    divider(668., 458., 250., with_alpha(GOLD, 0.55));
    works(game, t);
}
/// One pillar as a column: its icon on the capital, a meter that drains
/// toward a red zero zone, the value on the base.
fn gauge(game: &Game, k: usize, delta: Option<i16>, t: f32) {
    let (glyph, color) = PILLARS[k];
    let cx = GAUGE_X[k];
    let value = game.resources[k];
    let danger = value <= DANGER && game.outcome() == Outcome::Playing;
    let throb = pulse(t, 1.2);
    let x = cx - GAUGE_W * 0.5;
    let bottom = GAUGE_TOP + GAUGE_H;
    let stone = darken(VELLUM_SIDE, 0.2);
    rounded(cx - 19., GAUGE_TOP - 8., 38., 6., 2., stone);
    rounded(cx - 19., bottom + 2., 38., 6., 2., stone);
    let track = Rect::new(x, GAUGE_TOP, GAUGE_W, GAUGE_H);
    shapes::fill_rounded_rect(track, 7., darken(VELLUM_SIDE, 0.13));
    let zone_top = level_y(f32::from(DANGER));
    let zone = Rect::new(x, zone_top, GAUGE_W, bottom - zone_top);
    let zone_alpha = if danger { 0.18 + 0.24 * throb } else { 0.13 };
    shapes::fill_rounded_rect(zone, 7., with_alpha(LOSS, zone_alpha));
    let level = shown(game, k);
    let top = level_y(level);
    if level > 0.3 {
        let fill = Rect::new(x, top, GAUGE_W, bottom - top);
        shapes::fill_rounded_rect(
            fill,
            7_f32.min(fill.h * 0.5),
            Fill::vertical(fill, lighten(color, 0.3), color),
        );
        if fill.h > 10. {
            let shine = Rect::new(x + 5., top + 4., 4., fill.h - 8.);
            shapes::fill_rounded_rect(shine, 2., Color::new(1., 1., 1., 0.2));
        }
    }
    // The latest change: a draining red trail or a bright crest.
    let since = f32::from(game.since);
    if game.outcome() == Outcome::Playing && since < EASE_TICKS * 2. {
        let fade = 1. - since / (EASE_TICKS * 2.);
        let from = game.eased_from[k];
        if from > value {
            let a = level_y(f32::from(from));
            if top - a > 0.5 {
                let trail = Rect::new(x, a, GAUGE_W, top - a);
                shapes::fill_rounded_rect(trail, 4., with_alpha(LOSS, 0.6 * fade));
            }
        } else if from < value {
            shapes::glow_disc(
                vec2(cx, top),
                26.,
                with_alpha(lighten(color, 0.55), 0.55 * fade),
            );
        }
    }
    // The decree under consideration, ghosted onto the meter.
    if let Some(d) = delta {
        let target = (value + d).clamp(0, 100);
        let (now_y, then_y) = (level_y(f32::from(value)), level_y(f32::from(target)));
        if target > value {
            let ghost = Rect::new(x, then_y, GAUGE_W, now_y - then_y);
            shapes::fill_rounded_rect(
                ghost,
                4.,
                with_alpha(lighten(color, 0.4), 0.5 + 0.35 * throb),
            );
            shapes::stroke_rounded_rect(ghost, 4., 1.5, with_alpha(GAIN, 0.55 + 0.45 * throb));
        } else if target < value {
            let ghost = Rect::new(x, now_y, GAUGE_W, then_y - now_y);
            shapes::fill_rounded_rect(ghost, 4., with_alpha(LOSS, 0.3 + 0.3 * throb));
            shapes::stroke_rounded_rect(ghost, 4., 1., with_alpha(LOSS, 0.7));
        }
        if target != value {
            crate::view::arrow(cx + 20., then_y, d, if d > 0 { GAIN } else { LOSS });
        }
        if target == 0 {
            let at = vec2(cx, bottom - 15.);
            shapes::glow_disc(at, 24., with_alpha(LOSS, 0.35 + 0.35 * throb));
            icons::draw(Icon::Skull, at, 20., CREAM);
        }
    }
    shapes::stroke_rounded_rect(track, 7., 1., with_alpha(BROWN, 0.22));
    if danger {
        let rim = Rect::new(x - 2., GAUGE_TOP - 2., GAUGE_W + 4., GAUGE_H + 4.);
        shapes::stroke_rounded_rect(rim, 9., 2., with_alpha(LOSS, 0.35 + 0.55 * throb));
    }
    // The capital: the pillar's icon on a medallion.
    let c = vec2(cx, BADGE_Y);
    if danger {
        shapes::glow_disc(c, 32., with_alpha(LOSS, 0.3 * throb + 0.1));
    }
    shapes::fill_circle(
        c,
        19.,
        Fill::radial(
            c - vec2(5., 6.),
            26.,
            lighten(color, 0.3),
            darken(color, 0.1),
        ),
    );
    shapes::stroke_circle(c, 19., 1.5, if danger { LOSS } else { GOLD });
    icons::draw(glyph, c, 22., CREAM);
    let ink = if danger { LOSS } else { BROWN };
    centered(num(i32::from(value)), cx, 358., 21., ink);
}
/// Each policy's seal and one pip per decree sealed under it; the leading
/// policy (the legacy so far) wears a crown. The considered decree's policy
/// shows its next pip as a breathing ring.
fn leanings(game: &Game, preview: Option<usize>, t: f32) {
    let lead = (0..4)
        .max_by_key(|&i| (game.legacy[i], std::cmp::Reverse(i)))
        .filter(|&i| game.legacy[i] > 0);
    for (p, (_, color)) in POLICIES.iter().enumerate() {
        let y = LEAN_Y + p as f32 * LEAN_PITCH;
        let count = usize::from(game.legacy[p]).min(30);
        if lead == Some(p) {
            shapes::glow_disc(vec2(SEAL_COLUMN, y), 15., with_alpha(GOLD, 0.35));
        }
        seal(p, SEAL_COLUMN, y, 8.);
        // A faint rule for the row the pips will fill.
        let rule = Rect::new(
            pip(p, 0).x - 3.,
            y - 0.5,
            pip(p, 29).x - pip(p, 0).x + 6.,
            1.,
        );
        shapes::fill_rounded_rect(rule, 0.5, with_alpha(*color, 0.18));
        for n in 0..count {
            shapes::fill_circle(pip(p, n), 2.7, *color);
        }
        if preview == Some(p) && count < 30 {
            shapes::stroke_circle(
                pip(p, count),
                3.3,
                1.3,
                with_alpha(*color, 0.45 + 0.55 * pulse(t, 1.1)),
            );
        }
        if lead == Some(p) {
            let end = pip(p, count.saturating_sub(1)).x + 13.;
            icons::draw(Icon::Crown, vec2(end, y - 1. + bob(t, 0.6, 1.)), 14., GOLD);
        }
    }
}
/// The four royal works as tiles: cost when open, a countdown ring while
/// building, a check badge once complete. The row opens the works page.
fn works(game: &Game, t: f32) {
    let live = game.outcome() == Outcome::Playing && game.screen != Screen::Briefing;
    let row = live && WORK_TILES.contains(pointer());
    let open = game.screen == Screen::Projects;
    for (j, project) in PROJECTS.iter().enumerate() {
        let base = tile(j);
        let lifted = row && base.contains(pointer());
        let r = Rect::new(
            base.x,
            base.y - if lifted { 2. } else { 0. },
            base.w,
            base.h,
        );
        let icon_at = vec2(r.x + r.w * 0.5, r.y + 28.);
        let foot = r.y + 64.;
        match game.projects[j] {
            ProjectState::Complete => {
                shapes::fill_rounded_rect(
                    r,
                    8.,
                    Fill::vertical(
                        r,
                        with_alpha(lighten(GOLD, 0.3), 0.5),
                        with_alpha(GOLD, 0.3),
                    ),
                );
                let mid = vec2(icon_at.x, r.y + r.h * 0.5);
                shapes::glow_disc(mid, 30., with_alpha(GOLD, 0.15 + 0.12 * pulse(t, 0.4)));
                icons::draw(WORK_ICONS[j], mid, 32., darken(GOLD, 0.35));
                icon_badge(Icon::Check, r.x + r.w - 9., r.y + 9., 18., GAIN, CREAM);
            }
            ProjectState::Building(left) => {
                shapes::fill_rounded_rect(r, 8., with_alpha(TEAL, 0.14));
                let total = f32::from(project.duration.max(1));
                let done = (total - f32::from(left)) / total;
                ring_meter(icon_at.x, icon_at.y, 20., 3.5, done, TEAL);
                // A spark at the ring's leading edge: work in progress.
                let a = -std::f32::consts::FRAC_PI_2 + done * std::f32::consts::TAU;
                let spark = icon_at + vec2(a.cos(), a.sin()) * 20.;
                shapes::glow_disc(spark, 8., with_alpha(GOLD, 0.35 + 0.4 * pulse(t, 1.3)));
                shapes::fill_circle(spark, 2.6, lighten(GOLD, 0.4));
                icons::draw(WORK_ICONS[j], icon_at, 22., TEAL);
                icon_value(
                    Icon::Hourglass,
                    num(i32::from(left)),
                    icon_at.x - 15.,
                    foot,
                    13.,
                    TEAL,
                );
            }
            ProjectState::Unbuilt => {
                let late = game.count + usize::from(project.duration) > 30;
                let short = game.resources[0] <= project.cost;
                let a = if late { 0.45 } else { 1. };
                shapes::fill_rounded_rect(r, 8., with_alpha(BROWN, 0.06));
                shapes::stroke_rounded_rect(r, 8., 1., with_alpha(FADED, 0.25 * a));
                icons::draw(WORK_ICONS[j], icon_at, 26., with_alpha(FADED, 0.8 * a));
                let ink = if short || late { LOSS } else { BROWN };
                icon_value(
                    Icon::Coin,
                    num(i32::from(project.cost)),
                    icon_at.x - 16.,
                    foot,
                    13.,
                    with_alpha(ink, a),
                );
            }
        }
        if open || lifted {
            shapes::stroke_rounded_rect(
                r,
                8.,
                1.5,
                with_alpha(if open { TEAL } else { GOLD }, 0.85),
            );
        } else if row {
            shapes::stroke_rounded_rect(r, 8., 1., with_alpha(GOLD, 0.5));
        }
    }
}

/// The header objective: thirty councils in three acts toward the crown,
/// each sealed council coloured by its policy, then the rule: all four
/// pillars above zero.
pub(crate) fn objective(game: &Game, area: Rect) -> bool {
    let cy = area.y + area.h * 0.5;
    let t = clock();
    let x0 = area.x + 14.;
    let at = |i: usize| vec2(x0 + i as f32 * 12. + (i / 10) as f32 * 12., cy);
    let lost = game.outcome() == Outcome::Lost;
    for i in 0..30 {
        let c = at(i);
        if i < game.count {
            let policy = game.history[i].map_or(0, |r| r.petition.choices[r.choice].legacy);
            shapes::fill_circle(c, 4.3, lighten(POLICIES[policy].1, 0.12));
            if lost && i + 1 == game.count {
                shapes::glow_disc(c, 12., Color::new(0.95, 0.3, 0.25, 0.5));
                icons::draw(Icon::Skull, c, 13., Color::new(0.98, 0.45, 0.4, 1.));
            }
        } else if i == game.count && game.outcome() == Outcome::Playing {
            shapes::glow_disc(c, 12., with_alpha(GOLD, 0.25 + 0.3 * pulse(t, 1.)));
            shapes::fill_circle(c, 5., lighten(GOLD, 0.35));
        } else {
            shapes::stroke_circle(c, 3.4, 1.4, with_alpha(MUTED, 0.5));
        }
    }
    let goal = at(29) + vec2(24., 0.);
    if game.outcome() == Outcome::Won {
        shapes::glow_disc(goal, 22., with_alpha(GOLD, 0.5));
    }
    icons::draw_styled(
        Icon::Crown,
        goal,
        22.,
        IconStyle::new(lighten(GOLD, 0.3)).shadow(Color::new(0., 0., 0., 0.4)),
    );
    let mut x = goal.x + 44.;
    for (k, (glyph, color)) in PILLARS.iter().enumerate() {
        let danger = game.resources[k] <= DANGER && game.outcome() == Outcome::Playing;
        let ink = if danger {
            with_alpha(Color::new(0.98, 0.42, 0.38, 1.), 0.6 + 0.4 * pulse(t, 1.2))
        } else {
            lighten(*color, 0.35)
        };
        icons::draw(*glyph, vec2(x, cy), 18., ink);
        x += 24.;
    }
    text(
        "> 0",
        x - 4.,
        cy + 6.,
        17.,
        Color::new(0.84, 0.88, 0.92, 1.),
    );
    true
}

/// The results card: the ending's seal (or the fallen pillar) and name, the
/// grade, then reign length, final pillars, completed works and badges.
pub(crate) fn result(game: &Game, area: Rect) -> bool {
    let Some(ending) = game.ending() else {
        return false;
    };
    let won = game.outcome() == Outcome::Won;
    let (x, y) = (area.x, area.y);
    let ink = Color::new(0.93, 0.95, 0.96, 1.);
    let dim = Color::new(0.6, 0.67, 0.74, 1.);
    let red = Color::new(0.96, 0.42, 0.38, 1.);
    let mark = ending_mark(ending);
    if won {
        seal(mark, x + 24., y + 24., 22.);
    } else {
        let (glyph, color) = PILLARS[mark];
        icon_badge(glyph, x + 24., y + 24., 46., darken(color, 0.1), CREAM);
        icon_badge(
            Icon::Skull,
            x + 42.,
            y + 42.,
            22.,
            Color::new(0.7, 0.16, 0.14, 1.),
            CREAM,
        );
    }
    line(ending.name(), x + 60., y + 33., area.w - 140., 26., ink);
    // The grade on a gilt medallion.
    let g = vec2(x + area.w - 28., y + 24.);
    shapes::fill_circle(
        g,
        24.,
        Fill::radial(
            g - vec2(6., 7.),
            32.,
            lighten(GOLD, 0.35),
            darken(GOLD, 0.2),
        ),
    );
    shapes::stroke_circle(g, 24., 2., lighten(GOLD, 0.5));
    let s = plain(26., Color::new(0.16, 0.1, 0.05, 1.)).with_align(TextAlign::Center);
    draw_text_styled(game.grade(), g.x, s.centered_baseline(g.y), s);
    // Reign length.
    let row = y + 100.;
    icons::draw(
        Icon::Crown,
        vec2(x + 16., row),
        30.,
        if won { lighten(GOLD, 0.3) } else { dim },
    );
    let big = style(30., ink);
    let w = draw_text_styled(
        num(game.count as i32),
        x + 38.,
        big.centered_baseline(row),
        big,
    )
    .width;
    text("/30", x + 42. + w, row + 7., 16., dim);
    // Final pillars as four small meters.
    let px = x + 150.;
    for (k, (glyph, color)) in PILLARS.iter().enumerate() {
        let cx = px + k as f32 * 30.;
        let v = game.resources[k];
        let track = Rect::new(cx - 6., row - 36., 12., 44.);
        shapes::fill_rounded_rect(track, 5., Color::new(1., 1., 1., 0.1));
        let h = 44. * f32::from(v.clamp(0, 100)) / 100.;
        if h > 0. {
            let fill = Rect::new(track.x, track.y + 44. - h, 12., h);
            shapes::fill_rounded_rect(fill, 5_f32.min(h * 0.5), lighten(*color, 0.15));
        }
        icons::draw(
            *glyph,
            vec2(cx, row + 20.),
            15.,
            if v == 0 { red } else { lighten(*color, 0.35) },
        );
        let label = plain(12., if v <= DANGER { red } else { dim }).with_align(TextAlign::Center);
        draw_text_styled(num(i32::from(v)), cx, row - 42., label);
    }
    // Completed works.
    let wx = x + 300.;
    for (j, glyph) in WORK_ICONS.iter().enumerate() {
        let c = vec2(wx + j as f32 * 34., row);
        if game.projects[j] == ProjectState::Complete {
            icon_badge(*glyph, c.x, c.y, 30., darken(GOLD, 0.05), CREAM);
            icon_badge(Icon::Check, c.x + 11., c.y - 11., 14., GAIN, CREAM);
        } else {
            shapes::stroke_circle(c, 14., 1.5, with_alpha(dim, 0.5));
            icons::draw(*glyph, c, 16., with_alpha(dim, 0.5));
        }
    }
    // Earned badges (successful reigns only).
    let mut by = row - 30.;
    for badge in game.badges() {
        icon_chip(Icon::Star, badge, x + 448., by, 14., lighten(GOLD, 0.3));
        by += 26.;
    }
    if !won {
        // The promises that were sealed still show what the reign stood for.
        let mut fx = x + 460.;
        for (flag, _, _) in crate::view::PROMISES {
            if game.flags & flag != 0 {
                promise_badge(flag, fx, row, 24., true);
                fx += 28.;
            }
        }
    }
    true
}
/// The seal (successful reigns) or fallen pillar (collapses) of an ending.
pub(crate) fn ending_mark(ending: Ending) -> usize {
    match ending {
        Ending::Charter | Ending::Bankruptcy => 0,
        Ending::Iron | Ending::Revolt => 1,
        Ending::Commerce | Ending::Mutiny => 2,
        Ending::Concordat | Ending::Schism => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn meters_ease_from_the_previous_values_then_settle() {
        let mut game = Game::new();
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        game.step(&Input {
            slot: Some(1),
            ..Input::default()
        });
        let record = game.history[0].unwrap();
        for k in 0..4 {
            assert_eq!(shown(&game, k), f32::from(record.before[k]));
        }
        for _ in 0..EASE_TICKS as usize {
            game.step(&Input::default());
        }
        for k in 0..4 {
            assert_eq!(shown(&game, k), f32::from(record.after[k]));
        }
        // Geometry: full meters reach the top, empty ones the base.
        assert_eq!(level_y(100.), GAUGE_TOP);
        assert_eq!(level_y(0.), GAUGE_TOP + GAUGE_H);
        assert!(
            pip(0, 29).x + 14. < 924.,
            "the leader's crown stays on the page"
        );
        assert!(WORK_TILES.contains(vec2(tile(3).x + tile(3).w, tile(3).y + tile(3).h)));
    }
}
