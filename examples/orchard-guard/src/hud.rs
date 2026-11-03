//! Desktop HUD, banners and overlays in the 1200x760 design space.
//!
//! Button rectangles for Seed/Upgrade, Pause, Wave and the paused Resume keep
//! their established positions (browser tests locate them by geometry). Each
//! button's secondary line now sits below it instead of overlapping its title.
use super::*;
use crate::juice::{self, Juice};
use crate::quips::{self, RunSummary};
use crate::save::Best;
use crate::scene::{coin_icon, fit_center, heart_icon, text_width};

const SOFT: Color = Color::new(0.45, 0.50, 0.36, 1.);
const WARN: Color = Color::new(0.66, 0.27, 0.18, 1.);
pub(super) const SPEED_ID: u64 = 10;
/// Design-space speed toggle. Its right edge stays left of the browser
/// fullscreen menu corner (72 CSS px from the right) at every fitted size.
pub(super) const SPEED_RECT: Rect = Rect {
    x: 976.,
    y: 13.,
    w: 128.,
    h: 44.,
};
/// Leftmost design x of the reserved fullscreen menu corner at the given
/// window size, for layouts fitted with `Viewport::fit` to 1200x760.
#[cfg(test)]
fn corner_x(screen: Vec2) -> (f32, f32) {
    let scale = (screen.x / 1200.).min(screen.y / 760.);
    let left = (screen.x - 1200. * scale) * 0.5;
    let top = (screen.y - 760. * scale) * 0.5;
    ((screen.x - 72. - left) / scale, (60. - top) / scale)
}

pub(super) fn key_hint(input: &PlayerInput, action: &str) -> String {
    let mut buffer = [0; 256];
    input
        .format_hint(action, &mut buffer)
        .unwrap_or("See Controls")
        .to_owned()
}

/// Mixed-case modern caption centred under a control (key hint or status).
fn caption(_ui: &mut Ui, rect: Rect, text: &str, color: Color) {
    style::text_fit(
        text,
        rect.x + rect.w * 0.5,
        rect.y + rect.h + 16.,
        style::body(14., color),
        rect.w + 24.,
    );
}

pub(super) fn format_time(seconds: f32) -> String {
    let s = seconds.max(0.) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

pub(super) fn run_summary(g: &Game) -> RunSummary {
    RunSummary {
        won: g.outcome == Outcome::Won,
        wave: g.wave,
        waves: g.level.waves.len() as u8,
        lives: g.lives,
        max_lives: g.level.lives,
        kills: g.kills,
        perfect_waves: g.perfect_waves,
        royal: g.royal_stopped,
        frost_planted: g.towers.iter().flatten().any(|t| t.kind == Kind::Frost),
        towers: g.towers.iter().flatten().count(),
    }
}

fn top_bar(campaign: &Campaign, juice: &Juice) {
    let g = &campaign.game;
    ferravex_render::shapes::fill_rounded_rect(
        Rect::new(0., 0., 1200., 70.),
        0.,
        ferravex_render::shapes::Fill::vertical(
            Rect::new(0., 0., 1200., 70.),
            style::BAR_TOP,
            style::BAR_BOTTOM,
        ),
    );
    draw_rectangle(0., 68., 1200., 2., Color::new(0.23, 0.39, 0.24, 0.35));
    // Logo apple.
    draw_circle(38., 37., 14., juice::APPLE);
    draw_circle(33., 32., 4., Color::new(1., 0.8, 0.7, 0.6));
    draw_line(38., 24., 40., 16., 3., Color::from_hex(0x85704c));
    draw_ellipse(46., 18., 7., 3.5, -20., juice::LEAF);
    label("Orchard Guard", 64., 46., 24., INK);
    let waves = g.level.waves.len();
    let shown = if g.active || g.outcome != Outcome::Playing {
        g.wave
    } else {
        g.next_wave().map_or(g.wave, |next| next.number)
    };
    center(&format!("Wave {shown} / {waves}"), 600., 34., 24., INK);
    if g.active {
        let total = g.level.waves[usize::from(g.wave) - 1].count as f32;
        let left = (g.pests.len() + usize::from(g.remaining)) as f32;
        let done = ((total - left) / total).clamp(0., 1.);
        draw_rectangle(500., 44., 200., 10., Color::new(0.23, 0.39, 0.24, 0.2));
        draw_rectangle(500., 44., 200. * done, 10., GREEN);
    } else if g.outcome == Outcome::Playing {
        let text = if usize::from(g.wave) >= waves {
            "All waves sent".to_string()
        } else if g.wave == 0 {
            "Up next · plant, then send it".to_string()
        } else {
            "Up next · build and upgrade".to_string()
        };
        center(&text, 600., 59., 13., GREEN);
    }
    // Hearts and coins wobble when they change.
    let wobble = (juice.hurt * 30.).sin() * 4. * juice.hurt * juice.motion().max(0.4);
    let red = mix_color(juice::APPLE, Color::from_hex(0xff4d3a), juice.hurt);
    heart_icon(760. + wobble, 34., 14., red);
    label(&format!("{}", g.lives), 782., 46., 26., INK);
    let bump = 1. + juice.coin * 0.25 * juice.motion();
    coin_icon(860., 34., 13. * bump);
    label(&format!("{}", g.money), 880., 46., 26., INK);
}

/// Declared after Restart and before Wave so Wave stays last in focus order.
fn speed_button(ui: &mut Ui, campaign: &Campaign) {
    let fast = campaign.fast;
    // Painted by the shared control style: gold when fast-forward is on.
    style::toggle(
        ui,
        SPEED_ID,
        SPEED_RECT,
        if fast { "Speed 2×" } else { "Speed 1×" },
        fast,
        18.,
    );
}

fn mix_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0., 1.);
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        1.,
    )
}

/// Toasts take the banner slot; otherwise preparation shows the incoming card.
fn banner(campaign: &Campaign, juice: &Juice, input: &PlayerInput) {
    let g = &campaign.game;
    let (x, y, w, h) = (330., 80., 540., 70.);
    if let Some(toast) = &juice.toast {
        let a = toast.alpha();
        let drop = (1. - a) * 10. * juice.motion();
        draw_rectangle(
            x + 3.,
            y + 4. - drop,
            w,
            h,
            Color::new(0.14, 0.2, 0.13, 0.25 * a),
        );
        draw_rectangle(x, y - drop, w, h, Color::new(0.19, 0.25, 0.19, 0.93 * a));
        draw_rectangle(
            x,
            y - drop,
            6.,
            h,
            Color::new(toast.color.r, toast.color.g, toast.color.b, a),
        );
        fit_center(
            &toast.head,
            600.,
            y + 32. - drop,
            24.,
            w - 40.,
            Color::new(toast.color.r, toast.color.g, toast.color.b, a),
        );
        fit_center(
            &toast.sub,
            600.,
            y + 56. - drop,
            15.,
            w - 40.,
            Color::new(CREAM.r, CREAM.g, CREAM.b, a),
        );
        return;
    }
    if g.active || g.outcome != Outcome::Playing {
        return;
    }
    let Some(next) = g.next_wave() else { return };
    draw_rectangle(x + 3., y + 4., w, h, Color::new(0.14, 0.2, 0.13, 0.18));
    draw_rectangle(x, y, w, h, Color::new(0.98, 0.95, 0.84, 0.95));
    draw_rectangle(x, y, 6., h, GOLD);
    let zoomies = next.fast_count;
    let beetles = u32::from(next.spec.count) - zoomies;
    fit_center(
        &format!(
            "Incoming: {beetles} beetles + {zoomies} zoomies · {:.0} HP",
            next.spec.hp
        ),
        600.,
        y + 28.,
        20.,
        w - 40.,
        INK,
    );
    let towers = g.towers.iter().flatten().count();
    let line = if g.wave == 0 && towers == 0 {
        "Pick a round plot, plant a tower, then send the wave".to_string()
    } else if g.wave == 0 {
        format!(
            "Nice! Plant more, or press {} to send the wave",
            key_hint(input, "wave")
        )
    } else {
        quips::incoming(next.number).to_string()
    };
    fit_center(&line, 600., y + 54., 15., w - 40., GREEN);
}

fn notice(campaign: &Campaign, save_error: bool) {
    let text = if save_error {
        "Save unavailable".to_string()
    } else if let Some(message) = campaign.notice() {
        notice_text(message).to_owned()
    } else {
        return;
    };
    let size = 16.;
    let w = (text_width(&text, size) + 36.).min(1100.);
    let (x, y) = (600. - w * 0.5, 622.);
    style::dark_chip(Rect::new(x, y, w, 30.), 1.);
    fit_center(&text, 600., y + 22., size, w - 24., CREAM);
}

pub(super) fn hud(
    ui: &mut Ui,
    campaign: &Campaign,
    juice: &Juice,
    save_error: bool,
    input: &PlayerInput,
) {
    let g = &campaign.game;
    if campaign.session.is_paused() || g.outcome != Outcome::Playing {
        // Keep the frame recognisable under overlays; no controls are live.
        top_bar(campaign, juice);
        draw_rectangle(0., 660., 1200., 100., CREAM);
        return;
    }
    banner(campaign, juice, input);
    notice(campaign, save_error);
    top_bar(campaign, juice);
    draw_rectangle(0., 660., 1200., 100., CREAM);
    draw_rectangle(0., 660., 1200., 2., Color::new(0.23, 0.39, 0.24, 0.35));
    if let Some(tower) = g.towers[g.selected] {
        label(tower.kind.name(), 24., 684., 22., INK);
        label(
            &format!("Plot {} · Level {}/3", g.selected + 1, tower.level + 1),
            24.,
            706.,
            14.,
            GREEN,
        );
        label(
            &format!("{:.0} dmg · {:.2}s", tower.damage(), tower.reload()),
            24.,
            727.,
            14.,
            SOFT,
        );
        label(&format!("Range {:.0}", tower.range()), 24., 748., 14., SOFT);
        let status = g.upgrade_status(g.selected);
        let next = Tower {
            level: (tower.level + 1).min(2),
            ..tower
        };
        let rect = Rect::new(240., 683., 415., 55.);
        button(
            ui,
            3,
            rect,
            &if tower.level == 2 {
                "Fully grown".into()
            } else {
                format!("Upgrade · {} coins", tower.upgrade_cost())
            },
            status.is_ok(),
            20.,
        );
        let (text, color) = match status {
            Ok(()) => (
                format!(
                    "Next → {:.0} dmg · {:.0} range · {:.2}s",
                    next.damage(),
                    next.range(),
                    next.reload()
                ),
                GREEN,
            ),
            Err(error) => (availability(Err(error), ""), WARN),
        };
        caption(
            ui,
            rect,
            &format!("{text}   [{}]", key_hint(input, "upgrade")),
            color,
        );
    } else {
        label(
            &format!("Empty plot {}", g.selected + 1),
            24.,
            684.,
            22.,
            INK,
        );
        label("Pick a tower  →", 24., 708., 14., GREEN);
        label("Sling: big bonks", 24., 728., 14., SOFT);
        label("Frost: slows pests", 24., 748., 14., SOFT);
        for (id, kind, x, action, blurb) in [
            (1, Kind::Seed, 240., "select_one", "Hits hard"),
            (2, Kind::Frost, 443., "select_two", "Slows + splash"),
        ] {
            let status = g.build_status(g.selected, kind);
            let rect = Rect::new(x, 683., 190., 55.);
            let name = if kind == Kind::Seed { "Sling" } else { "Frost" };
            button(
                ui,
                id,
                rect,
                &format!("{name} · {}", kind.price()),
                status.is_ok(),
                20.,
            );
            let (text, color) = match status {
                Ok(()) => (format!("{blurb} [{}]", key_hint(input, action)), GREEN),
                Err(error) => (availability(Err(error), ""), WARN),
            };
            caption(ui, rect, &text, color);
        }
    }
    let pause = Rect::new(695., 683., 110., 55.);
    button(ui, 6, pause, "Pause", campaign.session.is_started(), 20.);
    caption(ui, pause, &key_hint(input, "pause"), SOFT);
    let restart = Rect::new(817., 683., 128., 55.);
    button(ui, 5, restart, "Restart", true, 20.);
    caption(ui, restart, &key_hint(input, "restart"), SOFT);
    speed_button(ui, campaign);
    let next = g.next_wave();
    let title = if g.active {
        format!("Wave {} active", g.wave)
    } else if let Some(wave) = &next {
        format!("Send wave {}", wave.number)
    } else {
        "Harvest time".into()
    };
    let wave = Rect::new(957., 683., 215., 55.);
    button(ui, 4, wave, &title, g.wave_status().is_ok(), 20.);
    let text = if g.active {
        format!("{} pests to go", g.pests.len() + usize::from(g.remaining))
    } else if let Some(wave) = next {
        format!("{} pests · [{}]", wave.spec.count, key_hint(input, "wave"))
    } else {
        String::new()
    };
    caption(ui, wave, &text, if g.active { WARN } else { GREEN });
}

fn stat(x: f32, y: f32, name: &str, value: &str, badge: Option<&str>) {
    label(name, x, y, 13., SOFT);
    label(value, x, y + 28., 26., INK);
    if let Some(badge) = badge {
        let bx = x + text_width(value, 26.) + 12.;
        let w = text_width(badge, 12.) + 12.;
        draw_rectangle(bx, y + 9., w, 20., GOLD);
        label(badge, bx + 6., y + 24., 12., INK);
    }
}

pub(super) fn overlay(
    ui: &mut Ui,
    campaign: &Campaign,
    best: Option<Best>,
    input: &PlayerInput,
    festival: impl FnOnce(),
) {
    let g = &campaign.game;
    let paused = campaign.session.is_paused();
    if !paused && g.outcome == Outcome::Playing {
        return;
    }
    draw_rectangle(0., 0., 1200., 760., Color::new(0.14, 0.20, 0.13, 0.55));
    if paused {
        style::card(Rect::new(345., 215., 510., 300.), 18., 1.);
        draw_rectangle(345., 215., 510., 6., GOLD);
        center("Orchard paused", 600., 268., 30., INK);
        fit_center(
            quips::pause(g.kills + u32::from(g.wave)),
            600.,
            298.,
            15.,
            470.,
            GREEN,
        );
        center(
            &format!(
                "Wave {}/{} · {} hearts · {} stopped",
                g.wave,
                g.level.waves.len(),
                g.lives,
                g.kills
            ),
            600.,
            342.,
            17.,
            INK,
        );
        ui.modal(100).unwrap();
        let resume = Rect::new(375., 404., 215., 55.);
        button(ui, 7, resume, "Resume", true, 20.);
        caption(ui, resume, &key_hint(input, "pause"), SOFT);
        let again = Rect::new(610., 404., 215., 55.);
        button(ui, 8, again, "Plant again", true, 20.);
        caption(ui, again, &key_hint(input, "restart"), SOFT);
        return;
    }
    festival();
    let won = g.outcome == Outcome::Won;
    let run = run_summary(g);
    let rank = quips::rank(run);
    let (x, y, w, h) = (280., 88., 640., 562.);
    draw_rectangle(x + 5., y + 6., w, h, Color::new(0.1, 0.14, 0.09, 0.3));
    draw_rectangle(x, y, w, h, CREAM);
    // Harvest-festival bunting (muted after a loss).
    let colors = if won {
        [juice::APPLE, GOLD, juice::LEAF, juice::ICE]
    } else {
        [
            SOFT,
            Color::from_hex(0x9b8a6a),
            SOFT,
            Color::from_hex(0x8a8f78),
        ]
    };
    let sag = |t: f32| (t * std::f32::consts::PI).sin() * 10.;
    for i in 0..16 {
        let t0 = i as f32 / 16.;
        let t1 = (i as f32 + 1.) / 16.;
        let a = vec2(x + w * t0, y + sag(t0));
        let b = vec2(x + w * t1, y + sag(t1));
        let mid = (a + b) * 0.5 + vec2(0., 20.);
        draw_triangle(a, b, mid, colors[i % 4]);
    }
    let title = if won {
        "Harvest saved!"
    } else {
        "The pests got through"
    };
    center(title, 600., y + 74., 36., INK);
    let sub = if won {
        format!("All {} waves defended", g.level.waves.len())
    } else {
        format!("The beetles reached the pies in wave {}", g.wave)
    };
    center(&sub, 600., y + 100., 15., GREEN);
    let ribbon = if won { GOLD } else { Color::from_hex(0xd8c49a) };
    draw_rectangle(400., y + 116., 400., 66., ribbon);
    draw_triangle(
        vec2(400., y + 116.),
        vec2(380., y + 149.),
        vec2(400., y + 182.),
        ribbon,
    );
    draw_triangle(
        vec2(800., y + 116.),
        vec2(820., y + 149.),
        vec2(800., y + 182.),
        ribbon,
    );
    center("Your gardener rank", 600., y + 136., 12., INK);
    fit_center(rank.title, 600., y + 168., 28., 370., INK);
    fit_center(rank.quip, 600., y + 206., 15., 590., GREEN);
    let best_score = best.and_then(|b| b.previous_score);
    let new_score = best.is_some_and(|b| b.new_score && b.previous_score.is_some());
    let (c1, c2, top) = (330., 630., y + 236.);
    stat(
        c1,
        top,
        "Pests stopped",
        &g.kills.to_string(),
        new_score.then_some("New best!"),
    );
    stat(
        c2,
        top,
        "Hearts left",
        &format!("{} / {}", g.lives, g.level.lives),
        (won && g.lives == g.level.lives).then_some("Flawless"),
    );
    stat(
        c1,
        top + 62.,
        "Perfect waves",
        &format!("{} / {}", g.perfect_waves, g.level.waves.len()),
        None,
    );
    stat(
        c2,
        top + 62.,
        "Lucky bonks",
        &g.lucky_hits.to_string(),
        None,
    );
    if won {
        let new_time = best.is_some_and(|b| b.new_fastest && b.previous_fastest_us.is_some());
        stat(
            c1,
            top + 124.,
            "Harvest time",
            &format_time(g.time),
            new_time.then_some("Fastest!"),
        );
    } else {
        stat(
            c1,
            top + 124.,
            "Waves survived",
            &format!("{} / {}", g.wave.saturating_sub(1), g.level.waves.len()),
            None,
        );
    }
    let previous = match (best_score, best.and_then(|b| b.previous_fastest_us)) {
        (Some(score), Some(us)) => format!("{score} · {}", format_time(us as f32 / 1e6)),
        (Some(score), None) => score.to_string(),
        _ => "First run!".into(),
    };
    stat(c2, top + 124., "Previous best", &previous, None);
    if g.royal_stopped {
        fit_center(
            "★ Secret found: Sir Crunchalot dethroned ★",
            600.,
            y + 432.,
            15.,
            590.,
            Color::from_hex(0x5d3a78),
        );
    }
    let goal = quips::next_goal(run, best_score);
    draw_rectangle(
        310.,
        y + 446.,
        580.,
        32.,
        Color::new(0.23, 0.39, 0.24, 0.12),
    );
    fit_center(&goal, 600., y + 468., 16., 560., GREEN);
    ui.modal(100).unwrap();
    let again = Rect::new(490., y + 488., 220., 55.);
    button(ui, 8, again, "Plant again", true, 22.);
    caption(ui, again, &key_hint(input, "restart"), SOFT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fullscreen_menu_corner_stays_clear_of_desktop_hud() {
        for screen in [
            vec2(1280., 800.),
            vec2(1600., 1000.),
            vec2(900., 600.),
            vec2(2560., 1440.),
        ] {
            let (x, y) = corner_x(screen);
            // The speed toggle is the rightmost top-bar element; four-digit
            // coin counts end at 880 + 4 glyphs of 26px.
            let coins_right = 880. + text_width("9999", 26.);
            for right in [SPEED_RECT.x + SPEED_RECT.w, coins_right] {
                assert!(right <= x || SPEED_RECT.y >= y, "{screen:?}: {right} > {x}");
            }
            assert!(
                coins_right + 12. <= SPEED_RECT.x,
                "coins and speed stay apart"
            );
        }
    }
}
