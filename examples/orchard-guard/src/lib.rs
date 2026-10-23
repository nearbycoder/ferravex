//! A scene-authored tower defense game using the shared session lifecycle.
//!
//! Preparation and safe checkpoint loading happen before the first wave starts.
//! The same lifecycle is available to hosts independently of rendering:
//! ```
//! use ferravex_core::session::{GameSession, SessionState};
//! let mut session = GameSession::new(60., 8, false);
//! assert_eq!(session.advance(10.).steps, 0); // Build before sending the wave.
//! assert!(session.start());
//! assert!(session.set_paused(true));
//! assert_eq!(session.advance(30.).steps, 0); // No paused catch-up.
//! session.restart();
//! assert_eq!(session.state(), SessionState::Playing);
//! assert_eq!(session.tick(), 0);
//! ```
mod compact;
mod game;
mod host;
#[cfg(test)]
mod journey200_tests;
mod level;
mod presentation;
mod save;
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{PlayerInput, Preset};
use ferravex_render::ui::{Align, ButtonStyle, TextStyle, Ui, UiRect, Viewport};
use ferravex_render::{canvas::Canvas2D, demo::Demo, prelude::*};
use game::*;
use host::{Action, Campaign};
use presentation::Presentation;
use std::sync::Arc;

const SIZE: Vec2 = vec2(1200., 760.);
const INK: Color = Color::new(0.19, 0.25, 0.19, 1.);
const CREAM: Color = Color::new(0.98, 0.95, 0.84, 1.);
const GREEN: Color = Color::new(0.23, 0.39, 0.24, 1.);
const GOLD: Color = Color::new(0.88, 0.62, 0.22, 1.);
fn conf() -> Conf {
    let mut c = ferravex_render::window_conf("Orchard Guard · Ferravex");
    c.window_width = 1200;
    c.window_height = 760;
    c
}
fn label(s: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text(s, x, y, size, color);
}
fn center(s: &str, x: f32, y: f32, size: f32, color: Color) {
    let width = get_default_font()
        .try_metrics(size as u16, 1.)
        .and_then(|font| font.measure_line(s))
        .expect("bounded HUD text")
        .width;
    label(s, x - width * 0.5, y, size, color);
}
fn button(ui: &mut Ui, id: u64, rect: Rect, title: &str, subtitle: &str, enabled: bool) {
    ui.hud_button(
        id,
        UiRect {
            min: [rect.x, rect.y],
            size: [rect.w, rect.h],
        },
        title,
        enabled,
        ButtonStyle {
            fill: GREEN,
            ink: CREAM,
            focus: GOLD,
            disabled: Color::from_hex(0x616956),
            size: 17.,
        },
    )
    .unwrap();
    ui.label(
        UiRect {
            min: [rect.x + 10., rect.y + 37.],
            size: [rect.w - 20., 14.],
        },
        subtitle,
        TextStyle {
            size: 12.,
            color: CREAM,
            align: Align::Center,
            ..Default::default()
        },
    )
    .unwrap();
}
fn tree(p: Vec2, variant: usize) {
    draw_ellipse(
        p.x + 7.,
        p.y + 18.,
        25.,
        9.,
        0.,
        Color::new(0.22, 0.32, 0.19, 0.13),
    );
    draw_rectangle(p.x - 3., p.y, 6., 25., Color::from_hex(0x85704c));
    draw_poly(p.x, p.y - 8., 7, 25., 15., Color::from_hex(0x69894f));
    draw_poly(p.x - 8., p.y - 14., 7, 20., 0., Color::from_hex(0x78975b));
    for i in 0..3 {
        let a = (i as f32 * 2.1 + variant as f32) * 1.4;
        draw_circle(
            p.x + a.cos() * 14.,
            p.y - 10. + a.sin() * 13.,
            3.4,
            Color::from_hex(0xc67545),
        );
    }
}
fn field(g: &Game, presentation: &Presentation, compact: bool) {
    draw_rectangle(0., 70., 1200., 590., Color::from_hex(0xa7b87a));
    for i in 0..130 {
        let x = ((i * 97 + 31) % 1200) as f32;
        let y = 85. + ((i * 67 + 21) % 560) as f32;
        draw_line(x, y, x + 3., y - 5., 1., Color::new(0.35, 0.46, 0.25, 0.20));
    }
    for pair in g.level.path.windows(2) {
        draw_line(
            pair[0].x + 3.,
            pair[0].y + 6.,
            pair[1].x + 3.,
            pair[1].y + 6.,
            51.,
            Color::new(0.38, 0.43, 0.24, 0.18),
        );
    }
    for pair in g.level.path.windows(2) {
        draw_line(
            pair[0].x,
            pair[0].y,
            pair[1].x,
            pair[1].y,
            45.,
            Color::from_hex(0xd5bf8a),
        );
    }
    for p in &g.level.path {
        draw_circle(p.x, p.y, 22.5, Color::from_hex(0xd5bf8a));
    }
    for pair in g.level.path.windows(2) {
        let len = pair[0].distance(pair[1]);
        let d = (pair[1] - pair[0]) / len;
        for i in 0..(len / 23.) as usize {
            let p = pair[0] + d * (i as f32 * 23.);
            draw_circle(p.x, p.y, 1., Color::new(0.52, 0.42, 0.26, 0.25));
        }
    }
    for (i, p) in [
        vec2(65., 365.),
        vec2(100., 455.),
        vec2(180., 495.),
        vec2(80., 565.),
        vec2(180., 595.),
        vec2(335., 590.),
        vec2(540., 570.),
        vec2(620., 570.),
        vec2(870., 590.),
        vec2(1090., 590.),
        vec2(1150., 540.),
        vec2(1140., 180.),
        vec2(1035., 180.),
        vec2(930., 175.),
        vec2(670., 125.),
        vec2(570., 125.),
        vec2(375., 130.),
        vec2(80., 115.),
    ]
    .into_iter()
    .enumerate()
    {
        tree(p, i);
    }
    // Orchard gate and farmhouse establish the path's destination.
    draw_rectangle(1120., 385., 69., 43., Color::from_hex(0xf0e5bc));
    draw_triangle(
        vec2(1111., 386.),
        vec2(1154., 351.),
        vec2(1197., 386.),
        Color::from_hex(0xac6848),
    );
    draw_rectangle(1147., 404., 15., 24., Color::from_hex(0x7d6848));
    for y in [302., 353.] {
        for x in [1135., 1156., 1177.] {
            draw_rectangle(x, y, 5., 23., CREAM);
        }
        draw_rectangle(1135., y + 7., 47., 4., CREAM);
    }
    label("ORCHARD", 1106., 452., 15., GREEN);
    for (i, p) in g.level.plots.iter().enumerate() {
        if g.selected == i && g.outcome == Outcome::Playing {
            let range = g.towers[i]
                .unwrap_or_else(|| Tower::planted(Kind::Seed))
                .range();
            draw_circle(p.x, p.y, range, Color::new(0.96, 0.95, 0.72, 0.10));
            draw_circle_lines(p.x, p.y, range, 1., Color::new(0.96, 0.95, 0.72, 0.4));
            if g.towers[i].is_none() {
                draw_circle_lines(
                    p.x,
                    p.y,
                    Tower::planted(Kind::Frost).range(),
                    1.,
                    Color::new(0.40, 0.70, 0.75, 0.55),
                );
            }
        }
        draw_ellipse(
            p.x + 4.,
            p.y + 15.,
            25.,
            11.,
            0.,
            Color::new(0.24, 0.31, 0.18, 0.22),
        );
        draw_circle(p.x, p.y, 25., Color::from_hex(0xc9bd8a));
        draw_circle_lines(
            p.x,
            p.y,
            25.,
            2.,
            if g.selected == i {
                CREAM
            } else {
                Color::from_hex(0x8c9866)
            },
        );
        if let Some(tower) = g.towers[i] {
            match tower.kind {
                Kind::Seed => {
                    draw_rectangle(p.x - 12., p.y - 8., 24., 19., Color::from_hex(0x84623e));
                    draw_rectangle(p.x - 17., p.y - 16., 34., 8., Color::from_hex(0xbaa477));
                    let muzzle = *p + tower.aim * 25.;
                    draw_line(p.x, p.y - 5., muzzle.x, muzzle.y - 5., 10., GREEN);
                    draw_circle(p.x, p.y - 5., 12., Color::from_hex(0x496c3b));
                }
                Kind::Frost => {
                    draw_rectangle(p.x - 14., p.y - 12., 28., 22., Color::from_hex(0x679b99));
                    for y in [-12., -4., 4.] {
                        draw_line(
                            p.x - 15.,
                            p.y + y,
                            p.x + 15.,
                            p.y + y,
                            3.,
                            Color::from_hex(0x9ec6b5),
                        );
                    }
                    draw_triangle(
                        vec2(p.x - 20., p.y - 14.),
                        vec2(p.x, p.y - 28.),
                        vec2(p.x + 20., p.y - 14.),
                        Color::from_hex(0x447d80),
                    );
                    draw_circle(p.x, p.y, 5., INK);
                }
            }
            for n in 0..=tower.level {
                draw_circle(p.x - 6. + n as f32 * 6., p.y + 20., 2., CREAM);
            }
        } else {
            draw_line(p.x - 7., p.y, p.x + 7., p.y, 2., Color::from_hex(0x7d8a5f));
            draw_line(p.x, p.y - 7., p.x, p.y + 7., 2., Color::from_hex(0x7d8a5f));
        }
        if !compact {
            center(&format!("{}", i + 1), p.x, p.y + 42., 14., INK);
        }
    }
    for pest in &g.pests {
        let p = pest.pos;
        draw_ellipse(
            p.x + 3.,
            p.y + 7.,
            13.,
            6.,
            0.,
            Color::new(0.2, 0.2, 0.15, 0.2),
        );
        let dir =
            (g.level.path[(pest.segment + 1).min(g.level.path.len() - 1)] - p).normalize_or_zero();
        let side = vec2(-dir.y, dir.x);
        let wiggle = presentation.wiggle(pest.id);
        for n in [-1., 1.] {
            for t in [-6., 0., 6.] {
                let leg = p + side * n * 14. + dir * (t + wiggle * n);
                let root = p + dir * t;
                draw_line(root.x, root.y, leg.x, leg.y, 2., INK);
            }
        }
        let color = if pest.slow().is_some() {
            Color::from_hex(0x75bfca)
        } else if pest.fast {
            Color::from_hex(0x9a7451)
        } else {
            Color::from_hex(0xa65e4d)
        };
        let flash = presentation.flash(pest.id);
        let color = Color::new(
            color.r + (CREAM.r - color.r) * flash,
            color.g + (CREAM.g - color.g) * flash,
            color.b + (CREAM.b - color.b) * flash,
            1.,
        );
        draw_circle(p.x, p.y, if pest.fast { 8. } else { 11. }, color);
        draw_line(
            p.x - dir.x * 8.,
            p.y - dir.y * 8.,
            p.x + dir.x * 8.,
            p.y + dir.y * 8.,
            1.5,
            INK,
        );
        let head = p + dir * 10.;
        draw_circle(head.x, head.y, 5., INK);
        if let Some(slow) = pest.slow() {
            // Shape and countdown remain legible with flashes/motion disabled.
            draw_circle_lines(p.x, p.y, 17., 1.5, GREEN);
            center(
                &format!("SLOW {:.1}s", slow.remaining_seconds),
                p.x,
                p.y + 33.,
                10.,
                INK,
            );
        }
        if pest.hp < pest.max_hp {
            draw_rectangle(
                p.x - 12.,
                p.y - 20.,
                24.,
                3.,
                Color::new(0.2, 0.25, 0.18, 0.4),
            );
            draw_rectangle(
                p.x - 12.,
                p.y - 20.,
                24. * (pest.hp / pest.max_hp).max(0.),
                3.,
                CREAM,
            );
        }
    }
    if let Some(target) = g.selected_target() {
        draw_circle_lines(target.pos.x, target.pos.y, 21., 2., INK);
        center("TARGET", target.pos.x, target.pos.y - 26., 10., INK);
    }
    for s in &g.shots {
        draw_circle(
            s.pos.x,
            s.pos.y,
            4.,
            if s.kind == Kind::Seed {
                Color::from_hex(0xf3d080)
            } else {
                Color::from_hex(0xd1f1e8)
            },
        );
    }
}
fn availability(status: Result<(), ActionError>, available: &str) -> String {
    match status {
        Ok(()) => available.into(),
        Err(ActionError::Funds { missing }) => format!("Need {missing} more coins"),
        Err(ActionError::FullyGrown) => "Maximum level reached".into(),
        Err(ActionError::Occupied) => "Select an empty plot".into(),
        Err(ActionError::Empty) => "Build a tower first".into(),
        Err(_) => "Unavailable right now".into(),
    }
}
fn hint(input: &PlayerInput, action: &str) -> String {
    let mut buffer = [0; 256];
    input
        .format_hint(action, &mut buffer)
        .unwrap_or("See Controls")
        .into()
}

fn take_command(input: &mut PlayerInput) -> Option<Action> {
    let edges = input.take_tick(["select_one", "select_two", "upgrade", "wave"]);
    edges
        .into_iter()
        .zip([
            Action::Build(Kind::Seed),
            Action::Build(Kind::Frost),
            Action::Upgrade,
            Action::Wave,
        ])
        .find_map(|(edge, action)| edge.pressed.then_some(action))
}

fn hud_action(ui: &Ui, plots: usize) -> Option<Action> {
    [
        (5, Action::Restart),
        (8, Action::Restart),
        (6, Action::Pause),
        (7, Action::Pause),
        (1, Action::Build(Kind::Seed)),
        (2, Action::Build(Kind::Frost)),
        (3, Action::Upgrade),
        (4, Action::Wave),
    ]
    .into_iter()
    .find_map(|(id, action)| ui.activated(id).then_some(action))
    .or_else(|| {
        (0..plots).find_map(|i| {
            ui.activated(compact::PLOT_ID + i as u64)
                .then_some(Action::Select(Some(i)))
        })
    })
}

fn hud(ui: &mut Ui, campaign: &Campaign, save_error: bool, input: &PlayerInput) {
    let g = &campaign.game;
    draw_rectangle(0., 0., 1200., 70., CREAM);
    label("ORCHARD GUARD", 28., 33., 28., INK);
    label(
        "Keep the harvest safe",
        29.,
        53.,
        15.,
        Color::from_hex(0x727b59),
    );
    center(
        &format!("WAVE  {} / {}", g.wave, g.level.waves.len()),
        600.,
        33.,
        20.,
        INK,
    );
    let wave_status = if campaign.session.is_paused() {
        "Paused - the orchard is safe".into()
    } else if g.outcome == Outcome::Won {
        "Every wave cleared. Harvest saved!".into()
    } else if g.outcome == Outcome::Lost {
        "Orchard lost - try a new planting plan".into()
    } else if g.active {
        format!("{} on path / {} still arriving", g.pests.len(), g.remaining)
    } else if g.wave == 0 {
        "Prepare your defenses, then send wave 1".into()
    } else {
        format!("Wave {} cleared - prepare for wave {}", g.wave, g.wave + 1)
    };
    center(&wave_status, 600., 53., 14., Color::from_hex(0x727b59));
    if let Some(next) = g.next_wave() {
        draw_rectangle(280., 83., 640., 58., CREAM);
        center(
            &format!(
                "NEXT {}: {} PESTS / {} RUNNERS / +{} COINS",
                next.number, next.spec.count, next.fast_count, next.spec.reward
            ),
            600.,
            105.,
            16.,
            INK,
        );
        center(
            &format!(
                "Health {:.0}/{:.0} / speed {:.0}/{:.0} / arrive every {:.2}s",
                next.spec.hp,
                next.spec.hp * next.spec.fast_factor,
                next.spec.speed,
                next.spec.fast_speed,
                next.spec.interval
            ),
            600.,
            127.,
            13.,
            GREEN,
        );
    }
    label(&format!("{}  HEARTS", g.lives), 880., 39., 19., INK);
    draw_circle(1060., 32., 9., GOLD);
    label(&format!("{}", g.money), 1078., 39., 24., INK);
    draw_rectangle(0., 660., 1200., 100., CREAM);
    if g.outcome != Outcome::Playing {
        return;
    }
    if let Some(tower) = g.towers[g.selected] {
        label(tower.kind.name(), 28., 684., 18., INK);
        label(
            &format!(
                "Plot {} / L{} / range {:.0}",
                g.selected + 1,
                tower.level + 1,
                tower.range()
            ),
            28.,
            703.,
            12.,
            GREEN,
        );
        label(
            &format!(
                "{:.0} damage / {:.2}s reload",
                tower.damage(),
                tower.reload()
            ),
            28.,
            721.,
            12.,
            Color::from_hex(0x727b59),
        );
        label(
            if tower.kind == Kind::Frost {
                "45% speed / nearby slow"
            } else {
                "Targets closest to exit"
            },
            28.,
            739.,
            12.,
            GREEN,
        );
        let status = g.upgrade_status(g.selected);
        let next = Tower {
            level: (tower.level + 1).min(2),
            ..tower
        };
        button(
            ui,
            3,
            Rect::new(240., 683., 415., 55.),
            &if tower.level == 2 {
                "FULLY GROWN".into()
            } else {
                format!("UPGRADE / {} coins", tower.upgrade_cost())
            },
            &availability(
                status,
                &format!(
                    "[{}] {:.0} damage / {:.0} range / {:.2}s",
                    hint(input, "upgrade"),
                    next.damage(),
                    next.range(),
                    next.reload()
                ),
            ),
            status.is_ok(),
        );
    } else {
        label(
            &format!("EMPTY PLOT {}", g.selected + 1),
            28.,
            684.,
            18.,
            INK,
        );
        let seed_tower = Tower::planted(Kind::Seed);
        let frost_tower = Tower::planted(Kind::Frost);
        label(
            &format!(
                "Seed: {:.0} damage / {:.2}s",
                seed_tower.damage(),
                seed_tower.reload()
            ),
            28.,
            703.,
            12.,
            GREEN,
        );
        label(
            &format!(
                "Frost: {:.0} damage / {:.2}s",
                frost_tower.damage(),
                frost_tower.reload()
            ),
            28.,
            721.,
            12.,
            GREEN,
        );
        label(
            &format!(
                "Ranges {:.0} / {:.0}",
                seed_tower.range(),
                frost_tower.range()
            ),
            28.,
            739.,
            12.,
            GREEN,
        );
        let seed = g.build_status(g.selected, Kind::Seed);
        let frost = g.build_status(g.selected, Kind::Frost);
        button(
            ui,
            1,
            Rect::new(240., 683., 190., 55.),
            &format!("SEED / {}", Kind::Seed.price()),
            &availability(
                seed,
                &format!("[{}] Direct damage", hint(input, "select_one")),
            ),
            seed.is_ok(),
        );
        button(
            ui,
            2,
            Rect::new(443., 683., 190., 55.),
            &format!("FROST / {}", Kind::Frost.price()),
            &availability(frost, &format!("[{}] 45% speed", hint(input, "select_two"))),
            frost.is_ok(),
        );
    }
    button(
        ui,
        6,
        Rect::new(695., 683., 110., 55.),
        "PAUSE",
        &hint(input, "pause"),
        campaign.session.is_started(),
    );
    button(
        ui,
        5,
        Rect::new(820., 683., 110., 55.),
        "RESTART",
        &hint(input, "restart"),
        true,
    );
    let next = g.level.waves.get(usize::from(g.wave));
    let title = if g.active {
        "WAVE ACTIVE".into()
    } else {
        format!("SEND WAVE {}", g.wave + 1)
    };
    let subtitle = if g.active {
        format!("{} pests left", g.pests.len() + usize::from(g.remaining))
    } else {
        next.map_or_else(String::new, |w| {
            format!(
                "[{}] {} pests / +{} coins",
                hint(input, "wave"),
                w.count,
                w.reward
            )
        })
    };
    button(
        ui,
        4,
        Rect::new(957., 683., 215., 55.),
        &title,
        &subtitle,
        g.wave_status().is_ok(),
    );
    let message = if save_error {
        "Save unavailable: keep this window open to continue your current harvest."
    } else {
        campaign.notice().unwrap_or_else(|| {
        if g.towers.iter().all(Option::is_none) {
            "No defenses yet. Select a numbered plot and plant a tower before sending a wave."
        } else if g.active {
            "Build and upgrade during the wave. Each stopped pest earns coins; every escape costs a heart."
        } else {
            &g.level.wave_hint
        }
    })
    };
    label(message, 28., 757., 14., INK);
}
fn overlay(ui: &mut Ui, campaign: &Campaign, input: &PlayerInput) {
    let g = &campaign.game;
    if !campaign.session.is_paused() && g.outcome == Outcome::Playing {
        return;
    }
    draw_rectangle(0., 70., 1200., 590., Color::new(0.14, 0.20, 0.13, 0.55));
    draw_rectangle(345., 235., 510., 270., CREAM);
    let paused = campaign.session.is_paused();
    center(
        if paused {
            "ORCHARD PAUSED"
        } else if g.outcome == Outcome::Won {
            "HARVEST SAVED"
        } else {
            "THE PESTS GOT THROUGH"
        },
        600.,
        292.,
        28.,
        INK,
    );
    center(
        &format!("{} stopped / {} hearts", g.kills, g.lives),
        600.,
        334.,
        18.,
        GREEN,
    );
    let detail = if paused {
        "Pests, reloads and the harvest clock are frozen.".into()
    } else if g.outcome == Outcome::Won {
        format!(
            "All {} waves cleared. Plant again for a fresh run.",
            g.level.waves.len()
        )
    } else {
        format!(
            "Reached wave {} of {}. Cover bends and mix both towers.",
            g.wave,
            g.level.waves.len()
        )
    };
    ui.text(
        UiRect {
            min: [375., 350.],
            size: [450., 44.],
        },
        &detail,
        TextStyle {
            size: 16.,
            color: GREEN,
            align: Align::Center,
            wrap: true,
            ..Default::default()
        },
    )
    .unwrap();
    ui.modal(100).unwrap();
    if paused {
        button(
            ui,
            7,
            Rect::new(375., 404., 215., 55.),
            "RESUME",
            &format!("[{}] Back to the orchard", hint(input, "pause")),
            true,
        );
        button(
            ui,
            8,
            Rect::new(610., 404., 215., 55.),
            "PLANT AGAIN",
            &format!("[{}] Reset this harvest", hint(input, "restart")),
            true,
        );
    } else {
        button(
            ui,
            8,
            Rect::new(490., 404., 220., 55.),
            "PLANT AGAIN",
            &format!("[{}] A fresh start", hint(input, "restart")),
            true,
        );
    }
}
ferravex_render::game!(conf, game);
async fn game() {
    let mut demo = Demo::with_content();
    let mut content =
        ferravex_render::content::ContentSource::new(level::SOURCE, demo.content.as_deref())
            .unwrap();
    let mut g = Game::with_level(Arc::new(level::Level::parse(content.current()).unwrap()));
    let mut progress = save::Progress::new(
        &mut g,
        ferravex_play::persistence::checksum(content.current().as_bytes()),
    );
    let mut campaign = Campaign::new(g, demo.autoplay);
    let mut sound = GameAudio::new();
    let mut presentation = Presentation::default();
    let mut player_input = PlayerInput::preset("orchard-guard", Preset::Strategy);
    let mut hud_ui = Ui::new();
    hud_ui.set_auto_focus(false);
    loop {
        let reloaded = if let Some(next) = content.poll(demo.dt(), |text| {
            Ok(Game::with_level(Arc::new(level::Level::parse(text)?)))
        }) {
            campaign = Campaign::new(next, demo.autoplay);
            progress.content_changed(
                &campaign.game,
                ferravex_play::persistence::checksum(content.current().as_bytes()),
            );
            hud_ui.cancel();
            sound = GameAudio::new();
            presentation = Presentation::default();
            true
        } else {
            false
        };
        let screen = vec2(screen_width().max(1.), screen_height().max(1.));
        let compact = (280. ..900.)
            .contains(&screen.x)
            .then(|| compact::Layout::new(screen));
        let canvas = Canvas2D::fit(SIZE, screen);
        // Keep resume/restart UI interactive while the session owns the pause.
        player_input.sample(true);
        let blocked = !player_input.gameplay_active();
        campaign.suspend(blocked && !demo.autoplay);
        let cursor = player_input.cursor(demo.dt(), 600.);
        let touch = ferravex_render::input::logical_touches()
            .into_iter()
            .find(|t| t.phase == TouchPhase::Started);
        let point = touch.as_ref().map_or_else(|| cursor, |t| t.position);
        let pointer = compact.map_or_else(|| canvas.pointer(point), |layout| layout.pointer(point));
        let click = (player_input.pressed("primary") || touch.is_some())
            && !blocked
            && !(player_input.cursor_is_virtual() && hud_ui.focused().is_some());
        // Lifecycle edges are render-frame commands. Combat shortcuts remain
        // pending in PlayerInput until Campaign calls take_command on a tick.
        let shortcuts = [
            (player_input.pressed("restart"), Action::Restart),
            (player_input.pressed("pause"), Action::Pause),
        ];
        let keyboard = shortcuts
            .into_iter()
            .find_map(|(pressed, action)| pressed.then_some(action));
        let plot = if click
            && campaign.game.outcome == Outcome::Playing
            && !campaign.session.is_paused()
        {
            pointer.filter(|p| (70. ..660.).contains(&p.y)).map(|p| {
                Action::Select(
                    campaign
                        .game
                        .level
                        .plots
                        .iter()
                        .position(|plot| p.distance(*plot) < 30.),
                )
            })
        } else {
            None
        };
        clear_background(Color::from_hex(0x27372a));
        set_camera(&compact.map_or_else(|| canvas.camera(), |layout| layout.camera()));
        presentation.frame(&campaign.game, demo.dt());
        field(&campaign.game, &presentation, compact.is_some());
        ferravex_render::set_world_colors(false);
        if let Some(layout) = compact {
            hud_ui.begin(Viewport::responsive([screen.x, screen.y], [0.; 4]).unwrap());
            compact::hud(
                &mut hud_ui,
                layout,
                &campaign,
                progress.error().is_some(),
                &player_input,
            );
            compact::overlay(&mut hud_ui, layout, &campaign, &player_input);
        } else {
            hud_ui.begin(Viewport::fit([screen.x, screen.y], [1200., 760.], [0.; 4]).unwrap());
            hud(
                &mut hud_ui,
                &campaign,
                progress.error().is_some(),
                &player_input,
            );
            overlay(&mut hud_ui, &campaign, &player_input);
        }
        hud_ui.set_auto_focus(
            campaign.session.is_paused() || campaign.game.outcome != Outcome::Playing,
        );
        hud_ui.finish_player(&mut player_input, !blocked && !reloaded);
        hud_ui.paint();
        let ui_action = hud_action(&hud_ui, campaign.game.level.plots.len());
        let previous_state = campaign.session.state();
        let events = campaign.frame_with_input(
            if reloaded { 0. } else { demo.dt() as f64 },
            blocked || reloaded,
            demo.autoplay,
            [keyboard, ui_action, plot],
            || take_command(&mut player_input),
        );
        if events.restarted {
            progress.restart(&campaign.game);
            hud_ui.cancel();
            sound = GameAudio::new();
            presentation = Presentation::default();
        } else {
            if campaign.session.state() != previous_state {
                hud_ui.cancel();
            }
            if events.fired {
                sound.cue(Cue::Click, 0.5);
            }
            if events.killed {
                sound.cue(Cue::Impact(Surface::Grass), 0.35);
            }
            if events.escaped && events.finished != Some(false) {
                sound.cue(Cue::Failure, 0.3);
            }
            if let Some(won) = events.finished {
                sound.cue(if won { Cue::Success } else { Cue::Failure }, 0.65);
            }
        }
        progress.sync(&campaign.game);
        if player_input.cursor_is_virtual() {
            if compact.is_some() {
                set_default_camera();
                draw_circle_lines(cursor.x, cursor.y, 6., 1.5, CREAM);
            } else {
                let point = canvas.unproject(cursor);
                draw_circle_lines(point.x, point.y, 6., 1.5, CREAM);
            }
        }
        set_default_camera();
        player_input.draw_controls();
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}

#[cfg(test)]
mod migration_tests {
    use super::*;
    use ferravex_render::input::{self, ActionKind, Binding, InputCode, PadState};

    #[test]
    fn hints_follow_remapping_reset_and_unbound_actions_without_stale_defaults() {
        let mut input = PlayerInput::transient("orchard-hints-test");
        input
            .action(
                "upgrade",
                "Upgrade",
                ActionKind::Button,
                vec![Binding::digital(InputCode::Key(KeyCode::U as u16))],
            )
            .unwrap();
        assert_eq!(hint(&input, "upgrade"), "U");
        input
            .rebind(
                "upgrade",
                vec![Binding::digital(InputCode::Key(KeyCode::J as u16))],
            )
            .unwrap();
        assert_eq!(hint(&input, "upgrade"), "J");
        input.rebind("upgrade", vec![]).unwrap();
        assert_eq!(hint(&input, "upgrade"), "Unbound");
        input.reset_defaults();
        assert_eq!(hint(&input, "upgrade"), "U");
    }

    #[test]
    fn real_player_tap_survives_zero_tick_frames_and_is_spent_once_during_catch_up() {
        let mut input = PlayerInput::transient("orchard-tick-test");
        input
            .action(
                "upgrade",
                "Upgrade",
                ActionKind::Button,
                vec![Binding::digital(InputCode::PadButton(5))],
            )
            .unwrap();
        let mut pad = PadState {
            connected: true,
            ..Default::default()
        };
        input::gamepad_sample(0, pad);
        input::host_frame(0., 1200, 760, || input.sample(true));
        let mut campaign = Campaign::new(Game::new(), false);
        campaign.game.money = 1000;
        campaign.game.build(0, Kind::Seed);
        campaign.game.launch();
        campaign.session.start();
        pad.buttons[5] = 1.;
        input::gamepad_sample(0, pad);
        pad.buttons[5] = 0.;
        input::gamepad_sample(0, pad);
        for elapsed in [0., 0., 0.1] {
            input::host_frame(elapsed, 1200, 760, || {
                input.sample(true);
                campaign.frame_with_input(f64::from(elapsed), false, false, [None; 3], || {
                    take_command(&mut input)
                });
            });
            assert_eq!(
                campaign.game.towers[0].unwrap().level,
                u8::from(elapsed > 0.)
            );
        }
        assert_eq!(campaign.game.money, 1000 - 60 - 65);
        assert_eq!(hint(&input, "upgrade"), "Pad 5");
        input::gamepad_sample(0, PadState::default());
    }
}
