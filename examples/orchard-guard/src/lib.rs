mod game;
mod level;
mod save;
use ferravex_core::FixedClock;
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{PlayerInput, Preset};
use ferravex_render::ui::{Align, ButtonStyle, TextStyle, Ui, UiRect, Viewport};
use ferravex_render::{canvas::Canvas2D, demo::Demo, prelude::*};
use game::*;
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
    label(
        s,
        x - measure_text(s, None, size as u16, 1.).width * 0.5,
        y,
        size,
        color,
    );
}
fn button(ui: &mut Ui, id: u64, rect: Rect, title: &str, subtitle: &str, enabled: bool) {
    ui.button(
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
            disabled: Color::from_hex(0xb1b49c),
            size: 17.,
        },
    )
    .unwrap();
    ui.text(
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
fn field(g: &Game) {
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
            let range = g.towers[i].map_or(158., Tower::range);
            draw_circle(p.x, p.y, range, Color::new(0.96, 0.95, 0.72, 0.10));
            draw_circle_lines(p.x, p.y, range, 1., Color::new(0.96, 0.95, 0.72, 0.4));
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
        let wiggle = (g.time * 24. + pest.id as f32).sin() * 3.;
        for n in [-1., 1.] {
            for t in [-6., 0., 6.] {
                let leg = p + side * n * 14. + dir * (t + wiggle * n);
                let root = p + dir * t;
                draw_line(root.x, root.y, leg.x, leg.y, 2., INK);
            }
        }
        let color = if pest.slow > 0. {
            Color::from_hex(0x75bfca)
        } else if pest.fast {
            Color::from_hex(0x9a7451)
        } else {
            Color::from_hex(0xa65e4d)
        };
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
fn hud(ui: &mut Ui, g: &Game) {
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
    center(
        if g.active {
            "Pests on the path"
        } else {
            "Build, then send the next wave"
        },
        600.,
        53.,
        14.,
        Color::from_hex(0x727b59),
    );
    label(&format!("{}  HEARTS", g.lives), 880., 39., 19., INK);
    draw_circle(1060., 32., 9., GOLD);
    label(&format!("{}", g.money), 1078., 39., 24., INK);
    draw_rectangle(0., 660., 1200., 100., CREAM);
    if g.outcome != Outcome::Playing {
        return;
    }
    if let Some(tower) = g.towers[g.selected] {
        label(tower.kind.name(), 28., 692., 20., INK);
        label(
            &format!("Level {} / 3", tower.level + 1),
            28.,
            716.,
            17.,
            GREEN,
        );
        label(
            if tower.kind == Kind::Seed {
                "Fast, focused damage"
            } else {
                "Chills pests to 45% speed"
            },
            28.,
            740.,
            14.,
            Color::from_hex(0x727b59),
        );
        let cost = tower.upgrade_cost();
        let title = if tower.level == 2 {
            "FULLY GROWN".into()
        } else {
            format!("UPGRADE · {cost}")
        };
        button(
            ui,
            3,
            Rect::new(260., 683., 190., 55.),
            &title,
            "[U]  More power + range",
            tower.level < 2 && g.money >= cost,
        );
    } else {
        label("EMPTY PLOT", 28., 696., 20., INK);
        label("Choose a tower", 28., 721., 15., Color::from_hex(0x727b59));
        button(
            ui,
            1,
            Rect::new(240., 683., 190., 55.),
            "SEED SLING · 60",
            "[1]  Fast damage",
            g.money >= 60,
        );
        button(
            ui,
            2,
            Rect::new(443., 683., 190., 55.),
            "FROST HIVE · 80",
            "[2]  Slow the swarm",
            g.money >= 80,
        );
    }
    label("Select a plot", 670., 701., 16., GREEN);
    label("R restart", 670., 727., 14., Color::from_hex(0x727b59));
    button(
        ui,
        4,
        Rect::new(957., 683., 215., 55.),
        if g.active {
            "WAVE IN PROGRESS"
        } else {
            "SEND NEXT WAVE"
        },
        &g.level.wave_hint,
        !g.active,
    );
}
enum Action {
    Build(Kind),
    Upgrade,
    Wave,
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
    let mut clock = FixedClock::new(60., 8);
    let mut sound = GameAudio::new();
    let mut player_input = PlayerInput::preset("orchard-guard", Preset::Strategy);
    let mut hud_ui = Ui::new();
    hud_ui.set_auto_focus(false);
    loop {
        if let Some(next) = content.poll(demo.dt(), |text| {
            Ok(Game::with_level(Arc::new(level::Level::parse(text)?)))
        }) {
            g = next;
            progress.content_changed(
                &g,
                ferravex_play::persistence::checksum(content.current().as_bytes()),
            );
            hud_ui.cancel();
            clock.reset();
            sound = GameAudio::new();
        }
        ferravex_render::ui::warm_up_font_sizes(&[14, 15, 16, 17, 18, 19, 20, 24, 28, 30]);
        let canvas = Canvas2D::fit(SIZE, vec2(screen_width().max(1.), screen_height().max(1.)));
        player_input.sample_play();
        let cursor = player_input.cursor(demo.dt(), 600.);
        let touch = ferravex_render::input::logical_touches()
            .into_iter()
            .find(|t| t.phase == TouchPhase::Started);
        let pointer = canvas.pointer(touch.as_ref().map_or_else(|| cursor, |t| t.position));
        let click = (player_input.pressed("primary") || touch.is_some())
            && player_input.gameplay_active()
            && !(player_input.cursor_is_virtual() && hud_ui.focused().is_some());
        if player_input.pressed("restart") {
            g = Game::with_level(g.level.clone());
            progress.restart(&g);
            hud_ui.cancel();
            clock.reset();
        }
        if g.outcome == Outcome::Playing {
            if click && let Some(p) = pointer {
                for (i, plot) in g.level.plots.iter().enumerate() {
                    if p.distance(*plot) < 30. {
                        g.selected = i;
                    }
                }
            }
            if player_input.pressed("select_one") {
                g.build(g.selected, Kind::Seed);
            }
            if player_input.pressed("select_two") {
                g.build(g.selected, Kind::Frost);
            }
            if player_input.pressed("upgrade") {
                g.upgrade(g.selected);
            }
            if player_input.pressed("wave") {
                g.launch();
            }
        }
        let dt = if demo.autoplay {
            demo.dt()
        } else {
            player_input.simulation_dt(demo.dt())
        };
        let step = clock.advance(dt as f64);
        for _ in 0..step.steps {
            if demo.autoplay {
                g.autoplay();
            }
            let shots = g.fired;
            let kills = g.kills;
            let lives = g.lives;
            let outcome = g.outcome;
            g.step(step.dt);
            if g.fired > shots {
                sound.cue(Cue::Click, 0.5);
            }
            if g.kills > kills {
                sound.cue(Cue::Impact(Surface::Grass), 0.35);
            }
            if g.lives < lives {
                sound.cue(Cue::Failure, 0.3);
            }
            if g.outcome != outcome {
                sound.cue(
                    if g.outcome == Outcome::Won {
                        Cue::Success
                    } else {
                        Cue::Failure
                    },
                    0.65,
                );
            }
        }
        clear_background(Color::from_hex(0x27372a));
        set_camera(&canvas.camera());
        field(&g);
        hud_ui.begin(
            Viewport::fit([screen_width(), screen_height()], [1200., 760.], [0.; 4]).unwrap(),
        );
        hud(&mut hud_ui, &g);
        if g.outcome != Outcome::Playing {
            draw_rectangle(0., 70., 1200., 590., Color::new(0.14, 0.20, 0.13, 0.45));
            draw_rectangle(375., 235., 450., 250., CREAM);
            center(
                if g.outcome == Outcome::Won {
                    "HARVEST SAVED"
                } else {
                    "THE PESTS GOT THROUGH"
                },
                600.,
                294.,
                30.,
                INK,
            );
            center(
                &format!("{} pests stopped · {} hearts remaining", g.kills, g.lives),
                600.,
                334.,
                18.,
                GREEN,
            );
            center("Four waves. One little orchard.", 600., 366., 17., GREEN);
            hud_ui.modal(100).unwrap();
            button(
                &mut hud_ui,
                5,
                Rect::new(490., 398., 220., 55.),
                "PLANT AGAIN",
                "[R] A fresh start",
                true,
            );
        }
        hud_ui.set_auto_focus(g.outcome != Outcome::Playing);
        let ui_enabled = player_input.gameplay_active();
        hud_ui.finish_player(&mut player_input, ui_enabled);
        hud_ui.paint();
        let action = if hud_ui.activated(1) {
            Some(Action::Build(Kind::Seed))
        } else if hud_ui.activated(2) {
            Some(Action::Build(Kind::Frost))
        } else if hud_ui.activated(3) {
            Some(Action::Upgrade)
        } else if hud_ui.activated(4) {
            Some(Action::Wave)
        } else {
            None
        };
        if g.outcome == Outcome::Playing {
            match action {
                Some(Action::Build(kind)) => {
                    g.build(g.selected, kind);
                }
                Some(Action::Upgrade) => {
                    g.upgrade(g.selected);
                }
                Some(Action::Wave) => {
                    g.launch();
                }
                None => {}
            }
        } else if hud_ui.activated(5) {
            g = Game::with_level(g.level.clone());
            progress.restart(&g);
            hud_ui.cancel();
            clock.reset();
        }

        progress.sync(&g);
        if progress.error().is_some() {
            label("Session progress", 670., 747., 12., GREEN);
        }
        if player_input.cursor_is_virtual() {
            let point = canvas.unproject(cursor);
            draw_circle_lines(point.x, point.y, 6., 1.5, CREAM);
        }
        set_default_camera();
        player_input.draw_settings();
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}
