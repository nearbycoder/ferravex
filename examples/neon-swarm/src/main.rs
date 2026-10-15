mod game;

use ferravex_core::FixedClock;
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

#[macroquad::main(conf)]
async fn main() {
    let mut demo = Demo::new();
    let mut game = Game::new();
    let mut clock = FixedClock::new(120.0, 12);
    let mut started = demo.autoplay;
    let mut paused = false;
    let mut touch_mode = false;
    loop {
        ferravex_render::ui::warm_up_font_sizes(&[14, 15, 18, 22, 30, 32, 56]);
        let canvas = Canvas2D::fit(
            vec2(1280.0, 800.0),
            vec2(screen_width().max(1.0), screen_height().max(1.0)),
        );
        let mouse = canvas.unproject(Vec2::from(mouse_position()));
        let touch_points = ferravex_render::input::logical_touches();
        if !touch_points.is_empty() {
            touch_mode = true;
        }
        if !started
            && (is_key_pressed(KeyCode::Enter)
                || is_mouse_button_pressed(MouseButton::Left)
                || !touch_points.is_empty())
        {
            started = true;
        }
        if is_key_pressed(KeyCode::Escape) && started && game.phase == Phase::Playing {
            paused = !paused;
        }
        if is_key_pressed(KeyCode::R)
            || (game.phase != Phase::Playing
                && (is_key_pressed(KeyCode::Enter)
                    || is_mouse_button_pressed(MouseButton::Left)
                    || !touch_points.is_empty()))
        {
            game = Game::new();
            clock.reset();
            started = true;
            paused = false;
        }
        let mut input = Input {
            movement: vec2(
                axis(KeyCode::D, KeyCode::Right) - axis(KeyCode::A, KeyCode::Left),
                axis(KeyCode::S, KeyCode::Down) - axis(KeyCode::W, KeyCode::Up),
            ),
            aim: mouse,
            fire: is_mouse_button_down(MouseButton::Left),
            dash: is_key_down(KeyCode::Space) || is_key_down(KeyCode::LeftShift),
        };
        if touch_mode {
            for touch in &touch_points {
                let p = canvas.unproject(touch.position);
                if p.x < 640.0 {
                    input.movement = ((p - vec2(202.0, 600.0)) / 66.0).clamp_length_max(1.0);
                } else {
                    input.dash = true;
                }
            }
            input.aim = game.nearest_enemy().unwrap_or(game.player + game.facing);
            input.fire = game.nearest_enemy().is_some();
        }
        if started && !paused {
            let step = clock.advance(f64::from(demo.dt()));
            for _ in 0..step.steps {
                game.update(
                    if demo.autoplay {
                        game.autoplay()
                    } else {
                        input
                    },
                    step.dt,
                );
            }
        }
        clear_background(INK);
        set_camera(&canvas.camera());
        draw_arena();
        draw_entities(&game);
        draw_hud(&game, started, touch_mode);
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
                "NEON SWARM",
                "Contain three waves. Keep moving.",
                if touch_mode {
                    "TAP TO START"
                } else {
                    "ENTER TO START"
                },
            );
        } else if paused {
            overlay(
                "PAUSED",
                "Take a breath. The swarm can wait.",
                "ESC TO RESUME",
            );
        } else if game.phase != Phase::Playing {
            let won = game.phase == Phase::Won;
            overlay(
                if won { "SECTOR SECURED" } else { "SIGNAL LOST" },
                &format!(
                    "{} hostiles cleared  /  {:.0} seconds",
                    game.kills, game.time
                ),
                if touch_mode {
                    "TAP TO RESTART"
                } else {
                    "R TO RESTART"
                },
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
        set_default_camera();
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}

fn axis(primary: KeyCode, alternate: KeyCode) -> f32 {
    if is_key_down(primary) || is_key_down(alternate) {
        1.0
    } else {
        0.0
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
    for particle in &game.particles {
        let color = Color::new(
            particle.color.r,
            particle.color.g,
            particle.color.b,
            (particle.life * 2.0).min(1.0),
        );
        draw_rectangle(particle.pos.x, particle.pos.y, 3.0, 3.0, color);
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

fn draw_hud(game: &Game, started: bool, touch: bool) {
    label("NEON SWARM", 72.0, 57.0, 30, PAPER);
    label("SECTOR 07 / CONTAINMENT", 73.0, 80.0, 14, MUTED);
    for wave in 1..=3 {
        let x = 600.0 + (wave - 1) as f32 * 40.0;
        draw_circle(x, 54.0, 5.0, if wave <= game.wave { CYAN } else { LINE });
    }
    if started {
        label(
            &format!("{} REMAIN", game.remaining_enemies()),
            596.0,
            80.0,
            14,
            MUTED,
        );
    }
    let hp_color = if game.health <= 30 { CORAL } else { CYAN };
    draw_line(1088.0, 51.0, 1104.0, 51.0, 3.0, hp_color);
    draw_line(1096.0, 43.0, 1096.0, 59.0, 3.0, hp_color);
    label(&format!("{:03}", game.health), 1120.0, 61.0, 32, PAPER);
    for pip in 0..10 {
        draw_rectangle(
            1088.0 + pip as f32 * 12.0,
            76.0,
            8.0,
            3.0,
            if game.health > pip * 10 {
                hp_color
            } else {
                LINE
            },
        );
    }
    let ready = game.dash_cooldown <= 0.0;
    label(
        if ready { "DASH READY" } else { "DASH" },
        73.0,
        756.0,
        15,
        if ready { CYAN } else { MUTED },
    );
    draw_rectangle(73.0, 768.0, 108.0, 2.0, LINE);
    draw_rectangle(
        73.0,
        768.0,
        108.0 * (1.0 - game.dash_cooldown / 1.7),
        2.0,
        CYAN,
    );
    let controls = if touch {
        "LEFT: MOVE     RIGHT: DASH     AUTO FIRE"
    } else {
        "WASD  MOVE       MOUSE  FIRE       SPACE  DASH       ESC  PAUSE"
    };
    let width = measure_text(controls, None, 14, 1.0).width;
    label(controls, 640.0 - width / 2.0, 759.0, 14, MUTED);
    label(
        &format!("{:02} CLEARED", game.kills),
        1097.0,
        757.0,
        15,
        MUTED,
    );
}

fn overlay(title: &str, detail: &str, action: &str) {
    draw_rectangle(
        0.0,
        100.0,
        1280.0,
        626.0,
        Color::new(0.025, 0.055, 0.07, 0.82),
    );
    draw_line(602.0, 323.0, 678.0, 323.0, 3.0, CYAN);
    for (text, y, size, color) in [
        (title, 386.0, 52, PAPER),
        (detail, 430.0, 21, MUTED),
        (action, 495.0, 18, CYAN),
    ] {
        let width = measure_text(text, None, size, 1.0).width;
        label(text, 640.0 - width * 0.5, y, size, color);
    }
}
