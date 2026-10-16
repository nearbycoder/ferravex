mod ai;
mod game;

use ferravex_core::FixedClock;
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{
    PlayerInput, Preset,
    touch::{Control, Output, Region},
};
use ferravex_render::ui::{Align, ButtonStyle, TextStyle, Ui, UiRect, Viewport};
use ferravex_render::{canvas::Canvas2D, demo::Demo, prelude::*};
use game::{ARENA, Bullet, COVER, Enemy, Game, Input, Phase, Pickup, Position};

const INK: Color = Color::from_rgba(9, 16, 22, 255);
const FLOOR: Color = Color::from_rgba(17, 29, 35, 255);
const LINE: Color = Color::from_rgba(40, 62, 68, 255);
const MUTED: Color = Color::from_rgba(125, 150, 154, 255);
const PAPER: Color = Color::from_rgba(221, 237, 227, 255);
const CYAN: Color = Color::from_rgba(98, 236, 207, 255);
const CORAL: Color = Color::from_rgba(255, 132, 93, 255);

fn conf() -> Conf {
    ferravex_render::window_conf("Neon Swarm · Ferravex")
}

ferravex_render::game!(conf, game);
async fn game() {
    let mut demo = Demo::new();
    let mut sound = GameAudio::new();
    let mut particles = ferravex_render::effects::ParticleRenderer::new(512).unwrap();
    let mut game = Game::new();
    let mut records = ferravex_render::storage::Records::new("neon-swarm").unwrap();
    let mut clock = FixedClock::new(120.0, 12);
    let mut started = demo.autoplay;
    let mut paused = false;
    let mut player_input = PlayerInput::preset("neon-swarm", Preset::TopDown);
    let mut hud_ui = Ui::new();
    loop {
        ferravex_render::ui::warm_up_font_sizes(&[14, 15, 18, 22, 30, 32, 56]);
        let canvas = Canvas2D::fit(
            vec2(1280.0, 800.0),
            vec2(screen_width().max(1.0), screen_height().max(1.0)),
        );
        let mouse = canvas.unproject(Vec2::from(mouse_position()));
        let scene_active = started && game.phase == Phase::Playing;
        let control = |rect: Rect, output| Control {
            region: Region::Rect(Rect::new(
                canvas.project(vec2(rect.x, rect.y)).x,
                canvas.project(vec2(rect.x, rect.y)).y,
                rect.w * canvas.scale,
                rect.h * canvas.scale,
            )),
            output,
        };
        let layout = if scene_active {
            [
                Control {
                    region: Region::Circle {
                        center: canvas.project(vec2(202., 600.)),
                        radius: 210. * canvas.scale,
                    },
                    output: Output::Stick {
                        x: 0,
                        y: 1,
                        radius: 66. * canvas.scale,
                        floating: false,
                    },
                },
                control(Rect::new(640., 0., 640., 800.), Output::Button(7)),
            ]
        } else {
            [
                control(Rect::new(0., 0., 1280., 800.), Output::Button(10)),
                control(Rect::new(0., 0., 1280., 800.), Output::Button(10)),
            ]
        };
        player_input
            .set_touch_layout(&layout[..if scene_active { 2 } else { 0 }])
            .unwrap();
        player_input.sample(scene_active && !paused);
        let touch_mode = player_input.touch_seen();
        if player_input.pressed("pause") && started && game.phase == Phase::Playing {
            paused = !paused;
        }
        if player_input.pressed("restart") {
            game = Game::new();
            clock.reset();
            started = true;
            paused = false;
        }
        player_input.sample(started && !paused && game.phase == Phase::Playing);
        let movement = player_input.movement();
        let mut input = Input {
            movement: vec2(movement.x, -movement.y),
            aim: mouse,
            ..Default::default()
        };
        let aim = vec2(player_input.value("aim_x"), player_input.value("aim_y"));
        if aim.length_squared() > 0.01 {
            input.aim = game.player + aim * 200.;
        }
        let touch_aim = player_input.touch_active();
        if touch_aim {
            input.aim = game.nearest_enemy().unwrap_or(game.player + game.facing);
        }
        let previous_phase = game.phase;
        if started && !paused {
            let dt = if demo.autoplay {
                demo.dt()
            } else {
                player_input.simulation_dt(demo.dt())
            };
            let step = clock.advance(dt as f64);
            for _ in 0..step.steps {
                let fire = player_input.take("fire");
                let dash = player_input.take("boost");
                input.fire =
                    fire.pressed || fire.value > 0.5 || touch_aim && game.nearest_enemy().is_some();
                input.dash = dash.pressed || dash.value > 0.5;
                let shots = game.shots;
                let health = game.health;
                let phase = game.phase;
                let dashing = game.dash_time;
                game.update(
                    if demo.autoplay {
                        game.autoplay()
                    } else {
                        input
                    },
                    step.dt,
                );
                if game.shots > shots {
                    sound.cue(Cue::Fire, 0.55);
                }
                if game.health < health {
                    sound.cue(Cue::Impact(Surface::Metal), 0.4);
                }
                if game.health > health {
                    sound.cue(Cue::Pickup, 0.5);
                }
                if game.dash_time > dashing {
                    sound.cue(Cue::Jump, 0.3);
                }
                if game.phase != phase {
                    sound.cue(
                        if game.phase == Phase::Won {
                            Cue::Success
                        } else {
                            Cue::Failure
                        },
                        0.65,
                    );
                }
            }
        }
        sound.walk(
            vec3(game.player.x, 0., game.player.y),
            true,
            started && !paused && (demo.autoplay || player_input.gameplay_active()),
            Surface::Metal,
            80.,
        );
        clear_background(INK);
        set_camera(&canvas.camera());
        draw_arena();
        draw_entities(&game);
        particles
            .draw_2d(
                &game.particles,
                ferravex_render::effects::ParticleStyle {
                    draw: ferravex_render::effects::EffectStyle {
                        blend: ferravex_render::effects::EffectBlend::Additive,
                        ..Default::default()
                    },
                    stretch: 0.006,
                    max_stretch: 3.,
                    ..Default::default()
                },
            )
            .unwrap();
        hud_ui.begin(
            Viewport::fit([screen_width(), screen_height()], [1280., 800.], [0.; 4]).unwrap(),
        );
        if previous_phase == Phase::Playing && game.phase != Phase::Playing {
            let time = if game.phase == Phase::Won {
                ferravex_render::storage::run_micros(game.time)
            } else {
                None
            };
            let _ = records.attempt("sector-07", 1, game.kills as u64, time);
        }
        draw_hud(&mut hud_ui, &game, started, touch_mode);
        if started && !paused && game.phase == Phase::Playing && !touch_mode && !demo.autoplay {
            draw_circle_lines(mouse.x, mouse.y, 7.0, 1.2, CYAN);
            draw_line(mouse.x - 11.0, mouse.y, mouse.x - 5.0, mouse.y, 1.0, CYAN);
            draw_line(mouse.x + 5.0, mouse.y, mouse.x + 11.0, mouse.y, 1.0, CYAN);
        }
        if touch_mode && started && game.phase == Phase::Playing {
            draw_circle_lines(202.0, 600.0, 66.0, 1.4, Color::new(0.4, 0.8, 0.75, 0.22));
            draw_circle(
                202.0 + input.movement.x * 34.0,
                600.0 + input.movement.y * 34.0,
                18.0,
                Color::new(0.4, 0.8, 0.75, 0.25),
            );
            draw_circle_lines(1100.0, 610.0, 40.0, 1.4, Color::new(0.4, 0.8, 0.75, 0.4));
            label("DASH", 1078.0, 616.0, 18, CYAN);
        }
        if !started {
            overlay(
                &mut hud_ui,
                "NEON SWARM",
                "Contain three waves. Keep moving.",
                "START",
            );
        } else if paused {
            overlay(
                &mut hud_ui,
                "PAUSED",
                "Take a breath. The swarm can wait.",
                "RESUME",
            );
        } else if game.phase != Phase::Playing {
            let won = game.phase == Phase::Won;
            overlay(
                &mut hud_ui,
                if won { "SECTOR SECURED" } else { "SIGNAL LOST" },
                &format!(
                    "{} cleared / {:.0}s / best {}",
                    game.kills,
                    game.time,
                    records.get("sector-07", 1).map_or(0, |r| r.score)
                ),
                "RESTART",
            );
        } else if game.wave_banner > 0.0 {
            let alpha = game.wave_banner.min(1.0);
            let text = format!("WAVE 0{}", game.wave);
            let width = measure_text(&text, None, 22, 1.0).width;
            label(
                &text,
                640.0 - width * 0.5,
                154.0,
                22,
                Color::new(CYAN.r, CYAN.g, CYAN.b, alpha),
            );
        }
        hud_ui.finish_player(
            &mut player_input,
            !started || paused || game.phase != Phase::Playing,
        );
        hud_ui.paint();
        if hud_ui.activated(1) {
            if game.phase != Phase::Playing {
                game = Game::new();
                clock.reset();
            }
            started = true;
            paused = false;
        }
        set_default_camera();
        player_input.draw_settings();
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}

fn label(text: &str, x: f32, y: f32, size: u16, color: Color) {
    draw_text_ex(
        text,
        x,
        y,
        TextParams {
            font_size: size,
            color,
            ..Default::default()
        },
    );
}

fn draw_arena() {
    draw_rectangle(
        ARENA.x - 9.0,
        ARENA.y - 9.0,
        ARENA.w + 18.0,
        ARENA.h + 18.0,
        Color::from_rgba(24, 40, 46, 255),
    );
    draw_rectangle(ARENA.x, ARENA.y, ARENA.w, ARENA.h, FLOOR);
    // Sparse floor joints establish scale while leaving combat silhouettes quiet.
    for x in (96..1208).step_by(64) {
        draw_line(
            x as f32,
            ARENA.y,
            x as f32,
            ARENA.bottom(),
            1.0,
            Color::from_rgba(24, 39, 45, 255),
        );
    }
    for y in (136..716).step_by(64) {
        draw_line(
            ARENA.x,
            y as f32,
            ARENA.right(),
            y as f32,
            1.0,
            Color::from_rgba(24, 39, 45, 255),
        );
    }
    draw_circle_lines(640.0, 414.0, 92.0, 1.0, LINE);
    draw_circle_lines(640.0, 414.0, 100.0, 1.0, LINE);
    draw_line(620.0, 414.0, 660.0, 414.0, 1.0, LINE);
    draw_line(640.0, 394.0, 640.0, 434.0, 1.0, LINE);
    label("07", 596.0, 435.0, 56, Color::from_rgba(32, 51, 56, 255));
    for (i, cover) in COVER.iter().enumerate() {
        draw_rectangle(cover.x + 5.0, cover.y + 8.0, cover.w, cover.h, INK);
        draw_rectangle(
            cover.x,
            cover.y,
            cover.w,
            cover.h,
            Color::from_rgba(35, 51, 57, 255),
        );
        draw_rectangle_lines(
            cover.x,
            cover.y,
            cover.w,
            cover.h,
            1.4,
            Color::from_rgba(69, 91, 94, 255),
        );
        draw_line(
            cover.x + 9.0,
            cover.y + 12.0,
            cover.right() - 9.0,
            cover.y + 12.0,
            2.0,
            LINE,
        );
        for x in 0..4 {
            draw_line(
                cover.x + 12.0 + x as f32 * 10.0,
                cover.bottom() - 8.0,
                cover.x + 17.0 + x as f32 * 10.0,
                cover.bottom() - 13.0,
                3.0,
                Color::from_rgba(139, 116, 68, 255),
            );
        }
        label(
            &format!("C{}", i + 1),
            cover.right() - 27.0,
            cover.bottom() - 10.0,
            14,
            MUTED,
        );
    }
    for p in [
        vec2(72.0, 414.0),
        vec2(1208.0, 414.0),
        vec2(640.0, 112.0),
        vec2(640.0, 716.0),
    ] {
        draw_circle(p.x, p.y, 4.0, CORAL);
        draw_circle_lines(
            p.x,
            p.y,
            9.0,
            1.0,
            Color::new(CORAL.r, CORAL.g, CORAL.b, 0.35),
        );
    }
    for (x, y, sx, sy) in [
        (72.0, 112.0, 1.0, 1.0),
        (1208.0, 112.0, -1.0, 1.0),
        (72.0, 716.0, 1.0, -1.0),
        (1208.0, 716.0, -1.0, -1.0),
    ] {
        draw_line(x, y, x + sx * 25.0, y, 2.0, CYAN);
        draw_line(x, y, x, y + sy * 25.0, 2.0, CYAN);
    }
}

fn draw_entities(game: &Game) {
    for (p, pickup) in &mut game.world.query::<(&Position, &Pickup)>() {
        let pulse = 1.0 + (game.time * 5.0).sin() * 0.1;
        draw_poly(
            p.0.x,
            p.0.y,
            4,
            15.0 * pulse,
            45.0,
            Color::from_rgba(26, 69, 63, 255),
        );
        let color = if pickup.remaining < 3.0 && (game.time * 8.0).sin() < 0.0 {
            MUTED
        } else {
            CYAN
        };
        draw_line(p.0.x - 5.0, p.0.y, p.0.x + 5.0, p.0.y, 2.0, color);
        draw_line(p.0.x, p.0.y - 5.0, p.0.x, p.0.y + 5.0, 2.0, color);
    }
    for (p, enemy) in &mut game.world.query::<(&Position, &Enemy)>() {
        let color = if enemy.flash > 0.0 {
            PAPER
        } else if enemy.heavy {
            Color::from_rgba(208, 109, 153, 255)
        } else {
            CORAL
        };
        let heading = (game.player - p.0).to_angle().to_degrees();
        let sides = if enemy.heavy { 6 } else { 4 };
        draw_poly(
            p.0.x + 2.0,
            p.0.y + 5.0,
            sides,
            enemy.radius + 2.0,
            heading,
            INK,
        );
        draw_poly(p.0.x, p.0.y, sides, enemy.radius, heading, color);
        draw_poly(p.0.x, p.0.y, sides, enemy.radius - 4.0, heading, FLOOR);
        let nose = Vec2::from_angle(heading.to_radians()) * (enemy.radius - 2.0);
        draw_circle(p.0.x + nose.x, p.0.y + nose.y, 3.0, color);
        if enemy.heavy {
            draw_circle(p.0.x, p.0.y, 5.0, color);
            for pip in 0..(enemy.health / 24.0).ceil() as u32 {
                draw_rectangle(
                    p.0.x - 10.0 + pip as f32 * 8.0,
                    p.0.y - 32.0,
                    5.0,
                    2.0,
                    color,
                );
            }
        }
    }
    for (p, bullet) in &mut game.world.query::<(&Position, &Bullet)>() {
        let tail = p.0 - bullet.velocity.normalize() * 16.0;
        draw_line(tail.x, tail.y, p.0.x, p.0.y, 2.5, CYAN);
        draw_circle(p.0.x, p.0.y, 2.0, PAPER);
    }
    let p = game.player;
    let angle = game.facing.to_angle().to_degrees();
    if game.dash_time > 0.0 {
        for index in 1..5 {
            let ghost = p - game.facing * index as f32 * 12.0;
            draw_poly(
                ghost.x,
                ghost.y,
                6,
                14.0,
                angle,
                Color::new(CYAN.r, CYAN.g, CYAN.b, 0.15 / index as f32),
            );
        }
    }
    draw_circle(p.x + 2.0, p.y + 5.0, 17.0, INK);
    let player_color = if game.health == 0 {
        MUTED
    } else if game.hurt_time > 0.6 {
        CORAL
    } else {
        CYAN
    };
    draw_poly(p.x, p.y, 6, 15.0, angle, player_color);
    draw_poly(p.x, p.y, 6, 10.0, angle, FLOOR);
    let nose = p + game.facing * 24.0;
    draw_line(p.x, p.y, nose.x, nose.y, 6.0, player_color);
    if game.muzzle_time > 0.0 {
        draw_circle(nose.x, nose.y, 7.0, PAPER);
    }
    if game.hurt_time > 0.0 {
        draw_circle_lines(
            p.x,
            p.y,
            23.0,
            1.0,
            Color::new(CORAL.r, CORAL.g, CORAL.b, game.hurt_time),
        );
    }
}

fn hud_label(
    ui: &mut Ui,
    position: [f32; 2],
    width: f32,
    size: f32,
    color: Color,
    align: Align,
    args: std::fmt::Arguments<'_>,
) {
    let [x, y] = position;
    ui.text_fmt(
        UiRect {
            min: [x, y - size * 0.875],
            size: [width, size],
        },
        TextStyle {
            size,
            color,
            align,
            ..Default::default()
        },
        args,
    )
    .unwrap();
}
fn draw_hud(ui: &mut Ui, game: &Game, started: bool, touch: bool) {
    hud_label(
        ui,
        [72., 57.],
        380.,
        30.,
        PAPER,
        Align::Left,
        format_args!("NEON SWARM"),
    );
    hud_label(
        ui,
        [73., 80.],
        400.,
        14.,
        MUTED,
        Align::Left,
        format_args!("SECTOR 07 / CONTAINMENT"),
    );
    for wave in 1..=3 {
        ui.panel(
            UiRect {
                min: [595. + (wave - 1) as f32 * 40., 49.],
                size: [10.; 2],
            },
            if wave <= game.wave { CYAN } else { LINE },
        )
        .unwrap();
    }
    if started {
        hud_label(
            ui,
            [596., 80.],
            260.,
            14.,
            MUTED,
            Align::Left,
            format_args!("{} REMAIN", game.remaining_enemies()),
        );
    }
    let hp = if game.health <= 30 { CORAL } else { CYAN };
    ui.panel(
        UiRect {
            min: [1088., 49.5],
            size: [16., 3.],
        },
        hp,
    )
    .unwrap();
    ui.panel(
        UiRect {
            min: [1094.5, 43.],
            size: [3., 16.],
        },
        hp,
    )
    .unwrap();
    hud_label(
        ui,
        [1120., 61.],
        110.,
        32.,
        PAPER,
        Align::Left,
        format_args!("{:03}", game.health),
    );
    for pip in 0..10 {
        ui.panel(
            UiRect {
                min: [1088. + pip as f32 * 12., 76.],
                size: [8., 3.],
            },
            if game.health > pip * 10 { hp } else { LINE },
        )
        .unwrap();
    }
    let ready = game.dash_cooldown <= 0.;
    hud_label(
        ui,
        [73., 756.],
        180.,
        15.,
        if ready { CYAN } else { MUTED },
        Align::Left,
        format_args!("{}", if ready { "DASH READY" } else { "DASH" }),
    );
    ui.panel(
        UiRect {
            min: [73., 768.],
            size: [108., 2.],
        },
        LINE,
    )
    .unwrap();
    ui.panel(
        UiRect {
            min: [73., 768.],
            size: [108. * (1. - game.dash_cooldown / 1.7).clamp(0., 1.), 2.],
        },
        CYAN,
    )
    .unwrap();
    hud_label(
        ui,
        [265., 759.],
        750.,
        14.,
        MUTED,
        Align::Center,
        format_args!(
            "{}",
            if touch {
                "LEFT: MOVE     RIGHT: DASH     AUTO FIRE"
            } else {
                "WASD  MOVE     MOUSE  FIRE     SPACE  DASH     ESC  PAUSE"
            }
        ),
    );
    hud_label(
        ui,
        [1097., 757.],
        160.,
        15.,
        MUTED,
        Align::Left,
        format_args!("{:02} CLEARED", game.kills),
    );
}
fn overlay(ui: &mut Ui, title: &str, detail: &str, action: &str) {
    ui.panel(
        UiRect {
            min: [0., 100.],
            size: [1280., 626.],
        },
        Color::new(0.025, 0.055, 0.07, 0.82),
    )
    .unwrap();
    ui.panel(
        UiRect {
            min: [602., 323.],
            size: [76., 3.],
        },
        CYAN,
    )
    .unwrap();
    hud_label(
        ui,
        [80., 386.],
        1120.,
        52.,
        PAPER,
        Align::Center,
        format_args!("{title}"),
    );
    hud_label(
        ui,
        [80., 430.],
        1120.,
        21.,
        MUTED,
        Align::Center,
        format_args!("{detail}"),
    );
    ui.button(
        1,
        UiRect {
            min: [470., 468.],
            size: [340., 52.],
        },
        action,
        true,
        ButtonStyle {
            fill: FLOOR,
            ink: CYAN,
            focus: CYAN,
            ..Default::default()
        },
    )
    .unwrap();
}
