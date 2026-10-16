//! A complete small rally time trial; all art is procedural and all motion is simulated.
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{
    PlayerInput, Preset,
    touch::{Control, Output, Region},
};
mod save;
mod sim;
use ferravex_core::FixedClock;
use ferravex_play::particles::{RibbonFacing, Trail, TrailConfig, TrailSample};
use ferravex_render::effects::{EffectStyle, draw_ribbon};
use ferravex_render::{demo::Demo, prelude::*};
use sim::{Controls, LAPS, Pose, Race, Track};

const CREAM: Color = Color::new(0.96, 0.92, 0.81, 1.);
const INK: Color = Color::new(0.12, 0.19, 0.17, 1.);
fn conf() -> Conf {
    ferravex_render::window_conf("Pocket Rally / Ferravex")
}
fn rgba(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba(r, g, b, 255)
}
fn line(a: Vec2, b: Vec2, width: f32, color: Color) {
    draw_line(a.x, a.y, b.x, b.y, width, color);
}
fn quad(points: [Vec2; 4], color: Color) {
    draw_triangle(points[0], points[1], points[2], color);
    draw_triangle(points[0], points[2], points[3], color);
}
fn local_quad(pose: Pose, center: Vec2, half: Vec2, color: Color) {
    let rotation = Vec2::from_angle(pose.angle);
    quad(
        [
            vec2(-half.x, -half.y),
            vec2(half.x, -half.y),
            vec2(half.x, half.y),
            vec2(-half.x, half.y),
        ]
        .map(|p| pose.position + rotation.rotate(p + center)),
        color,
    );
}
fn car(pose: Pose, ghost: bool) {
    if ghost {
        local_quad(
            pose,
            Vec2::ZERO,
            vec2(12., 6.),
            Color::new(0.95, 0.89, 0.60, 0.28),
        );
        return;
    }
    local_quad(
        Pose {
            position: pose.position + vec2(3., 4.),
            ..pose
        },
        Vec2::ZERO,
        vec2(13., 7.),
        Color::new(0.08, 0.12, 0.10, 0.32),
    );
    for x in [-7., 7.] {
        for y in [-6., 6.] {
            local_quad(pose, vec2(x, y), vec2(3., 1.6), rgba(25, 30, 27));
        }
    }
    local_quad(pose, Vec2::ZERO, vec2(12., 5.8), rgba(184, 62, 39));
    local_quad(pose, vec2(1., 0.), vec2(10., 2.), CREAM);
    local_quad(pose, vec2(-1., 0.), vec2(4., 5.), rgba(224, 101, 58));
    local_quad(pose, vec2(3., 0.), vec2(2., 4.7), rgba(42, 66, 65));
    local_quad(pose, vec2(-5., 0.), vec2(1.5, 4.7), rgba(42, 66, 65));
    for y in [-3.7, 3.7] {
        local_quad(pose, vec2(11., y), vec2(1., 1.3), CREAM);
    }
    local_quad(pose, vec2(-11., 0.), vec2(1.2, 6.8), rgba(90, 46, 33));
}
fn tree(p: Vec2, size: f32, variant: u32) {
    let shadow = Color::new(0.12, 0.20, 0.13, 0.18);
    draw_poly(p.x + 5., p.y + 6., 6, size * 0.9, 15., shadow);
    let light = if variant.is_multiple_of(2) {
        rgba(80, 108, 61)
    } else {
        rgba(64, 95, 59)
    };
    draw_poly(p.x, p.y, 5, size, 18., light);
    draw_triangle(
        p + vec2(-size * 0.8, -size * 0.45),
        p + vec2(0., -size),
        p + vec2(0., size * 0.65),
        rgba(111, 128, 70),
    );
    draw_triangle(
        p + vec2(0., -size),
        p + vec2(size * 0.9, size * 0.3),
        p + vec2(0., size * 0.65),
        rgba(48, 78, 50),
    );
}
fn barn(p: Vec2) {
    draw_rectangle(
        p.x + 5.,
        p.y + 6.,
        62.,
        33.,
        Color::new(0.1, 0.15, 0.1, 0.25),
    );
    draw_rectangle(p.x, p.y, 58., 32., rgba(223, 204, 159));
    quad(
        [
            p + vec2(-3., -3.),
            p + vec2(61., -3.),
            p + vec2(61., 14.),
            p + vec2(-3., 14.),
        ],
        rgba(154, 82, 52),
    );
    quad(
        [
            p + vec2(-3., 14.),
            p + vec2(61., 14.),
            p + vec2(61., 31.),
            p + vec2(-3., 31.),
        ],
        rgba(114, 66, 46),
    );
    line(
        p + vec2(-3., 14.),
        p + vec2(61., 14.),
        2.,
        rgba(190, 119, 72),
    );
}
fn draw_land(track: &Track) {
    draw_rectangle(-100., -100., 1300., 850., rgba(158, 161, 111));
    let fields = [
        rgba(164, 164, 111),
        rgba(172, 166, 112),
        rgba(147, 155, 103),
        rgba(180, 169, 120),
        rgba(145, 154, 102),
    ];
    for y in 0..5 {
        for x in 0..8 {
            let p = vec2(x as f32 * 160. - 60., y as f32 * 160. - 60.);
            let k = ((x * 13 + y * 7) % fields.len() as i32) as usize;
            draw_triangle(p, p + vec2(160., 0.), p + vec2(160., 160.), fields[k]);
            draw_triangle(
                p,
                p + vec2(160., 160.),
                p + vec2(0., 160.),
                fields[(k + 1) % fields.len()],
            );
        }
    }
    for y in 0..13 {
        line(
            vec2(370., 235. + y as f32 * 8.),
            vec2(470., 210. + y as f32 * 8.),
            2.,
            rgba(133, 145, 88),
        );
    }
    for y in 0..14 {
        line(
            vec2(710., 240. + y as f32 * 8.),
            vec2(810., 230. + y as f32 * 8.),
            2.,
            rgba(151, 144, 91),
        );
    }
    let pond = [
        vec2(930., 560.),
        vec2(985., 534.),
        vec2(1060., 555.),
        vec2(1080., 612.),
        vec2(980., 625.),
    ];
    for i in 1..pond.len() - 1 {
        draw_triangle(pond[0], pond[i], pond[i + 1], rgba(115, 145, 132));
    }
    draw_triangle(pond[1], pond[2], vec2(975., 603.), rgba(128, 157, 140));
    barn(vec2(620., 260.));
    barn(vec2(275., 280.));
    for i in 0..150_u32 {
        let x = ((i * 197 + 43) % 1090) as f32 + 5.;
        let y = ((i * 157 + i * i * 3 + 31) % 640) as f32;
        let p = vec2(x, y);
        if track.nearest(p).1 > 67.
            && !Rect::new(270., 245., 430., 90.).contains(p)
            && !(x > 920. && y > 525.)
        {
            tree(p, 8. + (i % 8) as f32, i);
        }
    }
    for (width, color) in [
        (84., rgba(185, 169, 128)),
        (73., rgba(206, 189, 146)),
        (66., rgba(80, 86, 77)),
    ] {
        for i in 0..track.points.len() {
            let a = track.points[i];
            let b = track.points[(i + 1) % track.points.len()];
            line(a, b, width, color);
            draw_circle(a.x, a.y, width * 0.5, color);
        }
    }
    for i in 0..track.points.len() {
        if i % 4 == 0 {
            line(
                track.points[i],
                track.points[(i + 1) % track.points.len()],
                1.4,
                rgba(180, 178, 148),
            );
        }
        if i % 2 == 0 {
            let normal = vec2(-track.tangent(i).y, track.tangent(i).x);
            let a = track.points[i] + normal * 35.;
            let b = track.points[(i + 1) % track.points.len()] + normal * 35.;
            if i % 48 < 15 {
                line(a, b, 4., if i % 4 == 0 { rgba(180, 93, 69) } else { CREAM });
            }
        }
    }
    let (start, tangent) = track.gate(0);
    let normal = vec2(-tangent.y, tangent.x);
    for x in 0..2 {
        for y in 0..10 {
            let center = start + tangent * (x as f32 * 5. - 2.5) + normal * (y as f32 * 6. - 27.);
            local_quad(
                Pose {
                    position: center,
                    angle: tangent.y.atan2(tangent.x),
                },
                Vec2::ZERO,
                vec2(2.5, 3.),
                if (x + y) % 2 == 0 { CREAM } else { INK },
            );
        }
    }
    for side in [-1., 1.] {
        let post = start + normal * 46. * side;
        draw_circle(
            post.x + 2.,
            post.y + 3.,
            5.,
            Color::new(0.1, 0.15, 0.1, 0.2),
        );
        draw_circle(post.x, post.y, 3., CREAM);
        line(post, post - tangent * 16., 3., CREAM);
    }
}
fn text(s: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text(s, x + 1., y + 1., size, Color::new(0.05, 0.1, 0.08, 0.35));
    draw_text(s, x, y, size, color);
}
fn time(t: f32) -> String {
    format!("{:02}:{:05.2}", (t / 60.) as u32, t % 60.)
}
ferravex_render::game!(conf, game);
async fn game() {
    let track = Track::new();
    let mut race = Race::new(&track);
    let mut clock = FixedClock::new(120., 8);
    let mut demo = Demo::new();
    let record_context = save::context(&track);
    let mut saved_lap =
        ferravex_render::storage::Slot::open("pocket-rally", "best-lap", save::BestLap::default())
            .unwrap();
    saved_lap.value().restore(&mut race, record_context);
    let mut record_error = false;
    let mut sound = GameAudio::new();
    let mut marks: [Trail; 2] = std::array::from_fn(|_| {
        Trail::new(TrailConfig {
            capacity: 256,
            lifetime: 7.,
            spacing: 3.5,
            teleport_distance: 80.,
            fade_width: false,
            ..Default::default()
        })
        .unwrap()
    });
    let mut player_input = PlayerInput::preset("pocket-rally", Preset::Racer);
    player_input
        .add_default_binding(
            "move_back",
            ferravex_render::input::Binding {
                code: ferravex_render::input::InputCode::Touch(6),
                scale: 0.3,
                deadzone: 0.,
            },
        )
        .unwrap();
    loop {
        ferravex_render::ui::warm_up_font_sizes(&[12, 13, 16, 17, 18, 20, 21, 23, 25, 26, 32]);
        let w = screen_width();
        let h = screen_height();
        player_input
            .set_touch_layout(&[
                Control {
                    region: Region::Rect(Rect::new(0., h - 130., 100., 130.)),
                    output: Output::Button(12),
                },
                Control {
                    region: Region::Rect(Rect::new(100., h - 130., 100., 130.)),
                    output: Output::Button(13),
                },
                Control {
                    region: Region::Rect(Rect::new(w - 100., h - 130., 100., 130.)),
                    output: Output::Button(14),
                },
                Control {
                    region: Region::Rect(Rect::new(w - 200., h - 130., 100., 130.)),
                    output: Output::Button(6),
                },
            ])
            .unwrap();
        player_input.sample_play();
        let touch_mode = player_input.touch_seen();
        if player_input.pressed("restart")
            || race.finished()
                && (player_input.pressed("confirm") || is_mouse_button_pressed(MouseButton::Left))
        {
            race.reset(&track);
            clock.reset();
            for trail in &mut marks {
                trail.clear();
            }
        }
        let movement = player_input.movement();
        let brake = player_input.held("brake");
        let controls = Controls {
            throttle: movement.y,
            steer: movement.x,
            handbrake: brake,
        };
        let dt = if demo.autoplay {
            demo.dt()
        } else {
            player_input.simulation_dt(demo.dt())
        };
        let frame = clock.advance(dt as f64);
        for _ in 0..frame.steps {
            let input = if demo.autoplay {
                race.autopilot(&track)
            } else {
                controls
            };
            let laps = race.laps;
            let finished = race.finished();
            let previous_best = race.best_time;
            race.step(&track, input, frame.dt);
            if race.best_time != previous_best
                && !demo.autoplay
                && let Some(best) = save::BestLap::from_race(&race, record_context)
            {
                record_error = saved_lap.commit(best).is_err();
            }
            if race.laps > laps {
                sound.cue(Cue::Pickup, 0.5);
            }
            if race.finished() && !finished {
                ferravex_render::performance::context(Some(race.ticks), None);
                ferravex_render::performance::event(
                    ferravex_play::performance::DiagnosticKind::Completed,
                    race.total_time,
                );
                sound.cue(Cue::Success, 0.65);
            }
            for (side, trail) in [-1., 1.].into_iter().zip(&mut marks) {
                let pose = race.car.pose;
                let point =
                    pose.position + Vec2::from_angle(pose.angle).rotate(vec2(-7., side * 6.));
                let emitting = !race.finished()
                    && (race.car.sliding || race.car.offroad)
                    && race.car.velocity.length() > 15.;
                trail
                    .update(
                        frame.dt,
                        emitting.then_some(TrailSample {
                            position: [point.x, point.y, 0.],
                            color: [0.20, 0.22, 0.17, 0.34],
                            width: 1.6,
                        }),
                    )
                    .unwrap();
            }
        }
        let pos = race.car.pose.position;
        sound.walk(
            vec3(pos.x, 0., pos.y),
            race.car.offroad || race.car.sliding,
            demo.autoplay || player_input.gameplay_active(),
            if race.car.offroad {
                Surface::Sand
            } else {
                Surface::Stone
            },
            42.,
        );
        sound.motor(
            race.car.velocity.length(),
            250.,
            !race.finished() && (demo.autoplay || player_input.gameplay_active()),
        );
        clear_background(rgba(137, 148, 103));
        let scale = (screen_width() / 1140.).min((screen_height() - 135.).max(150.) / 650.);
        // A full-window projection keeps input in logical display pixels, avoiding raw
        // framebuffer viewport coordinates on Retina and fractional-scale displays.
        let mut camera = Camera2D::from_display_rect(Rect::new(
            550. - screen_width() / (2. * scale),
            325. - screen_height() / (2. * scale),
            screen_width() / scale,
            screen_height() / scale,
        ));
        camera.zoom.y = camera.zoom.y.abs();
        set_camera(&camera);
        draw_land(&track);
        for trail in &marks {
            draw_ribbon(
                trail,
                RibbonFacing::Plane {
                    normal: [0., 0., 1.],
                },
                2.,
                EffectStyle::default(),
            )
            .unwrap();
        }
        if !race.finished() {
            let (gate, tangent) = track.gate(race.next_gate);
            let normal = vec2(-tangent.y, tangent.x);
            for side in [-1., 1.] {
                let p = gate + normal * 44. * side;
                draw_triangle(
                    p - tangent * 6. - normal * 3.,
                    p + tangent * 5.,
                    p - tangent * 6. + normal * 3.,
                    CREAM,
                );
            }
            if let Some(ghost) = race.ghost() {
                car(ghost, true);
            }
        }
        car(race.car.pose, false);
        set_default_camera();
        let width = screen_width();
        text("POCKET RALLY", 26., 36., 25., CREAM);
        if width > 660. {
            text(
                "VALLE VERDE",
                27.,
                57.,
                13.,
                Color::new(0.96, 0.92, 0.81, 0.68),
            );
        }
        let lap_x = if width > 660. {
            width * 0.45
        } else {
            width * 0.51
        };
        text(
            &format!("LAP {} / {LAPS}", (race.laps + 1).min(LAPS)),
            lap_x,
            36.,
            20.,
            CREAM,
        );
        if width > 440. {
            text(
                &time(if race.finished() {
                    race.last_lap.unwrap_or(0.)
                } else {
                    race.lap_time
                }),
                width - 220.,
                36.,
                23.,
                CREAM,
            );
        }
        text(
            &format!(
                "{:03}",
                if race.finished() {
                    0
                } else {
                    (race.car.velocity.length() * 0.48) as u32
                }
            ),
            width - 86.,
            36.,
            25.,
            CREAM,
        );
        text("KM/H", width - 85., 54., 12., CREAM);
        if let Some(best) = race.best_time {
            text(
                &format!(
                    "{} {}",
                    if record_error { "SESSION BEST" } else { "BEST" },
                    time(best)
                ),
                26.,
                screen_height() - 24.,
                17.,
                CREAM,
            );
        }
        if touch_mode && !player_input.touch_customized() {
            for (x, label) in [
                (46., "<"),
                (144., ">"),
                (width - 150., "BRAKE"),
                (width - 52., "GAS"),
            ] {
                draw_circle(
                    x,
                    screen_height() - 65.,
                    36.,
                    Color::new(0.1, 0.2, 0.14, 0.35),
                );
                let measure = measure_text(label, None, 16, 1.);
                text(
                    label,
                    x - measure.width / 2.,
                    screen_height() - 59.,
                    16.,
                    CREAM,
                );
            }
        } else if demo.time() < 9. || race.ticks == 0 {
            let hint = if width > 700. {
                "WASD / ARROWS  drive     SPACE  handbrake     R  restart"
            } else {
                "Arrows drive / Space brake / R restart"
            };
            let measure = measure_text(hint, None, 17, 1.);
            text(
                hint,
                (width - measure.width) / 2.,
                screen_height() - 25.,
                17.,
                CREAM,
            );
        }
        if race.finished() {
            let cx = width / 2.;
            let cy = screen_height() / 2.;
            draw_rectangle(
                cx - 190.,
                cy - 105.,
                380.,
                210.,
                Color::new(0.12, 0.19, 0.17, 0.94),
            );
            text("FINISH", cx - 54., cy - 54., 32., CREAM);
            text(
                &format!("3 laps    {}", time(race.total_time)),
                cx - 107.,
                cy - 12.,
                26.,
                CREAM,
            );
            text(
                &format!("Best lap    {}", time(race.best_time.unwrap_or(0.))),
                cx - 104.,
                cy + 22.,
                21.,
                CREAM,
            );
            text(
                "R or tap to race your ghost",
                cx - 109.,
                cy + 70.,
                18.,
                CREAM,
            );
        }
        ferravex_render::performance::context(Some(race.ticks), None);
        player_input.draw_settings();
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}
