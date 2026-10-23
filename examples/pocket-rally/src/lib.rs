//! A complete small rally time trial; all art is procedural and all motion is simulated.
//!
//! The host uses [`ferravex_core::session::GameSession`] for fixed timing. Countdown
//! ticks precede race timing; pause/settings/focus loss never advance the race.
//!
//! ```
//! use ferravex_core::session::GameSession;
//! let mut session = GameSession::new(120., 8, true);
//! assert_eq!(session.advance(1. / 60.).steps, 2);
//! session.set_paused(true);
//! assert_eq!(session.advance(30.).steps, 0);
//! session.restart();
//! assert_eq!(session.tick(), 0);
//! ```
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{PlayerInput, Preset, touch::Region};
#[cfg(test)]
mod journey200_tests;
mod presentation;
mod save;
mod session;
mod sim;
use ferravex_play::particles::{RibbonFacing, Trail, TrailConfig, TrailSample};
use ferravex_render::effects::{EffectStyle, draw_ribbon};
use ferravex_render::{demo::Demo, prelude::*};
use presentation::{hint, lap_comparison, text_width};
use session::{Notice, RaceSession};
use sim::{Controls, LAPS, Pose, Race, RoadCue, Track};

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
fn text_right(s: &str, right: f32, y: f32, size: f32, color: Color) {
    let width = text_width(s, size);
    text(s, right - width, y, size, color);
}
fn text_centered(s: &str, cx: f32, y: f32, size: f32, max_width: f32, color: Color) {
    let width = text_width(s, size);
    let fit = (max_width / width.max(1.)).min(1.);
    text(s, cx - width * fit / 2., y, size * fit, color);
}
fn time(t: f32) -> String {
    format!("{:02}:{:05.2}", (t / 60.) as u32, t % 60.)
}
fn restart(
    race: &mut Race,
    session: &mut RaceSession,
    track: &Track,
    marks: &mut [Trail; 2],
    sound: &mut GameAudio,
) {
    session.restart(race, track);
    for trail in marks {
        trail.clear();
    }
    sound.reset();
    // The overview projection is rebuilt every frame; no retained camera offset
    // or smoothing velocity survives a restart.
}
ferravex_render::game!(conf, game);
async fn game() {
    let track = Track::new();
    let mut race = Race::new(&track);
    let mut session = RaceSession::new();
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
        let marks_enabled = presentation::sync_marks(&mut marks);
        let touch_layout = presentation::touch_layout(w, h);
        player_input.set_touch_layout(&touch_layout).unwrap();
        player_input.sample_play();
        let touch_mode = player_input.touch_seen();
        let restarting = player_input.pressed("restart")
            || race.finished()
                && !player_input.controls_open()
                && (player_input.pressed("confirm") || is_mouse_button_pressed(MouseButton::Left));
        if restarting {
            restart(&mut race, &mut session, &track, &mut marks, &mut sound);
            player_input.set_paused(false);
            player_input.sample(true);
        }
        let movement = player_input.movement();
        let brake = player_input.held("brake");
        let controls = Controls {
            throttle: movement.y,
            steer: movement.x,
            handbrake: brake,
        };
        let active = !player_input.paused() && (demo.autoplay || player_input.gameplay_active());
        if active {
            sound.resume();
        } else {
            sound.pause();
        }
        let frame = session.advance(if restarting { 0. } else { demo.dt() as f64 }, active);
        for _ in 0..frame.steps {
            let input = if demo.autoplay && session.countdown() == 0 && !race.finished() {
                race.autopilot(&track)
            } else {
                controls
            };
            let laps = race.laps;
            let finished = race.finished();
            let previous_best = race.best_time;
            let countdown = session.countdown();
            if !session.step(&mut race, &track, input, frame.dt) {
                if countdown != session.countdown() {
                    sound.cue(
                        if session.countdown() == 0 {
                            Cue::Pickup
                        } else {
                            Cue::Click
                        },
                        0.4,
                    );
                }
                continue;
            }
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
                let emitting = marks_enabled
                    && !race.finished()
                    && (race.car.sliding || race.car.offroad)
                    && race.car.velocity.length() > 15.;
                if !marks_enabled {
                    continue;
                }
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
        let driving = session.clock.is_playing() && session.countdown() == 0;
        sound.walk(
            vec3(pos.x, 0., pos.y),
            race.car.offroad || race.car.sliding,
            driving,
            if race.car.offroad {
                Surface::Sand
            } else {
                Surface::Stone
            },
            42.,
        );
        sound.motor(race.car.velocity.length(), 250., driving);
        clear_background(rgba(137, 148, 103));
        let scale = (screen_width() / 1140.).min((screen_height() - 190.).max(150.) / 650.);
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
            if let Some(target) = session.recovery_target(&race, &track) {
                let from = race.car.pose.position;
                let direction = (target - from).normalize_or_zero();
                let end = from + direction * from.distance(target).min(85.);
                let normal = vec2(-direction.y, direction.x);
                let amber = rgba(255, 215, 124);
                line(from + direction * 18., end, 2., amber);
                draw_triangle(
                    end,
                    end - direction * 9. + normal * 4.,
                    end - direction * 9. - normal * 4.,
                    amber,
                );
                draw_circle_lines(target.x, target.y, 7., 2., amber);
            }
        }
        car(race.car.pose, false);
        set_default_camera();
        let width = screen_width();
        let margin = 16.;
        let right = width - margin;
        draw_rectangle(0., 0., width, 80., Color::new(0.12, 0.19, 0.17, 0.78));
        text(
            &format!("LAP {} / {LAPS}", (race.laps + 1).min(LAPS)),
            margin,
            28.,
            16.,
            CREAM,
        );
        text_right(
            &format!(
                "{:03} KM/H",
                if race.finished() {
                    0
                } else {
                    (race.car.velocity.length() * 0.48) as u32
                }
            ),
            right - 64.,
            28.,
            16.,
            CREAM,
        );
        text(
            &format!("LAP {}", time(race.lap_time)),
            margin,
            51.,
            14.,
            CREAM,
        );
        text(
            &format!("GATES {} / 16", race.checkpoints_passed()),
            margin,
            72.,
            13.,
            CREAM,
        );
        text_right(
            &format!("RACE {}", time(race.total_time)),
            right,
            72.,
            14.,
            CREAM,
        );
        if !race.finished() {
            let mut detail = None;
            let road_cue = track.road_cue(&race.car);
            let message = if session.off_track() {
                "OFF TRACK - FOLLOW AMBER ARROW".to_owned()
            } else if let Some(notice) = session.notice {
                match notice {
                    Notice::Go => "GO!".to_owned(),
                    Notice::Checkpoint(gate) => format!("GATE {gate} CLEARED"),
                    Notice::Lap {
                        number,
                        time: lap_time,
                        best,
                        comparison,
                    } => {
                        detail = Some(lap_comparison(comparison, best));
                        format!("LAP {number}  {}", time(lap_time))
                    }
                    Notice::BackOnTrack => "BACK ON TRACK".to_owned(),
                }
            } else if demo.autoplay && race.driver_reversing() {
                "AUTO RECOVERY - REVERSING".to_owned()
            } else if session.countdown() > 0 {
                "".to_owned()
            } else if road_cue == RoadCue::WrongWay {
                "WRONG WAY - FOLLOW GATE ARROWS".to_owned()
            } else if race.next_gate == 0 {
                "GATES CLEAR - CROSS THE FINISH".to_owned()
            } else {
                let turn = match road_cue {
                    RoadCue::Left => "LEFT BEND",
                    RoadCue::Right => "RIGHT BEND",
                    _ => "FOLLOW THE ROAD",
                };
                format!("{turn}  /  GATE {}", race.next_gate)
            };
            let font = if width < 540. { 13 } else { 17 };
            let message_width = text_width(&message, font as f32)
                .max(
                    detail
                        .as_ref()
                        .map_or(0., |label| text_width(label, font as f32)),
                )
                .min((width - 40.).max(1.));
            if !message.is_empty() {
                draw_rectangle(
                    (width - message_width) / 2. - 10.,
                    99.,
                    message_width + 20.,
                    if detail.is_some() { 46. } else { 25. },
                    INK,
                );
                text_centered(
                    &message,
                    width / 2.,
                    117.,
                    font as f32,
                    message_width,
                    CREAM,
                );
                if let Some(detail) = detail {
                    text_centered(&detail, width / 2., 138., font as f32, message_width, CREAM);
                }
            }
        }
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
            for (control, (_, label)) in touch_layout.iter().zip(presentation::TOUCH_BUTTONS) {
                let Region::Rect(rect) = control.region else {
                    continue;
                };
                let x = rect.x + rect.w / 2.;
                draw_circle(
                    x,
                    h - 65.,
                    (rect.w * 0.36).min(36.),
                    Color::new(0.1, 0.2, 0.14, 0.35),
                );
                text_centered(label, x, h - 59., 16., rect.w - 6., CREAM);
            }
        }
        if session.countdown() > 0 && !session.clock.is_paused() {
            let cx = width / 2.;
            let cy = screen_height() / 2.;
            let panel_width = (width - 32.).clamp(1., 340.);
            let content_width = (panel_width - 40.).max(1.);
            draw_rectangle(
                cx - panel_width / 2.,
                cy - 65.,
                panel_width,
                130.,
                Color::new(0.12, 0.19, 0.17, 0.94),
            );
            text_centered("GET READY", cx, cy - 24., 23., content_width, CREAM);
            text_centered(
                &session.countdown().to_string(),
                cx,
                cy + 15.,
                32.,
                content_width,
                CREAM,
            );
            text_centered(
                &format!("Hold {} for gas", hint(&player_input, "move_forward")),
                cx,
                cy + 47.,
                17.,
                content_width,
                CREAM,
            );
        }
        if race.finished() {
            let cx = width / 2.;
            let cy = screen_height() / 2.;
            let panel_width = (width - 32.).clamp(1., 420.);
            let content_width = (panel_width - 40.).max(1.);
            draw_rectangle(
                cx - panel_width / 2.,
                cy - 105.,
                panel_width,
                210.,
                Color::new(0.12, 0.19, 0.17, 0.94),
            );
            text_centered("FINISH", cx, cy - 54., 32., content_width, CREAM);
            text_centered(
                &format!("3 laps    {}", time(race.total_time)),
                cx,
                cy - 12.,
                26.,
                content_width,
                CREAM,
            );
            text_centered(
                &format!("Best lap    {}", time(race.best_time.unwrap_or(0.))),
                cx,
                cy + 22.,
                21.,
                content_width,
                CREAM,
            );
            text_centered(
                &format!("FINAL LAP {}", lap_comparison(race.last_comparison, false)),
                cx,
                cy + 47.,
                13.,
                content_width,
                CREAM,
            );
            text_centered(
                &presentation::retry_hint(&player_input),
                cx,
                cy + 78.,
                18.,
                content_width,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn race_restart_stops_only_its_owned_gameplay_audio() {
        let track = Track::new();
        let mut race = Race::new(&track);
        let mut session = RaceSession::new();
        let mut sound = GameAudio::new();
        let mut other = GameAudio::new();
        let mut marks = std::array::from_fn(|_| Trail::new(TrailConfig::default()).unwrap());
        sound.motor(100., 250., true);
        other.motor(150., 250., true);
        restart(&mut race, &mut session, &track, &mut marks, &mut sound);
        assert!(ferravex_render::audio::stats().active > 0);
        other.reset();
        assert_eq!(ferravex_render::audio::stats().active, 0);
    }

    #[test]
    fn restart_clears_skids_and_active_audio_before_another_countdown() {
        let track = Track::new();
        let mut race = Race::new(&track);
        let mut session = RaceSession::new();
        let mut sound = GameAudio::new();
        let mut marks = std::array::from_fn(|_| Trail::new(TrailConfig::default()).unwrap());
        for trail in &mut marks {
            trail
                .update(
                    1. / 120.,
                    Some(TrailSample {
                        position: [100., 100., 0.],
                        color: [0.2; 4],
                        width: 1.,
                    }),
                )
                .unwrap();
            assert!(!trail.is_empty());
        }
        sound.motor(100., 250., true);
        sound.cue(Cue::Success, 0.5);
        assert!(ferravex_render::audio::stats().active > 0);
        let _ = session.advance(1. / 240., true);
        restart(&mut race, &mut session, &track, &mut marks, &mut sound);
        assert!(marks.iter().all(Trail::is_empty));
        assert_eq!(ferravex_render::audio::stats().active, 0);
        assert_eq!(session.advance(1. / 240., true).steps, 0);
        assert_eq!(session.countdown(), 3);
    }
}
