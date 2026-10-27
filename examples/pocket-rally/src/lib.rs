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
#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
mod capture;
mod hud;
#[cfg(test)]
mod journey200_tests;
mod juice;
mod presentation;
mod quips;
mod save;
mod session;
mod sim;
mod style;
use ferravex_play::particles::{RibbonFacing, Trail, TrailConfig, TrailSample};
use ferravex_render::effects::{EffectStyle, ParticleRenderer, ParticleStyle, draw_ribbon};
use ferravex_render::{demo::Demo, prelude::*};
use hud::{Hints, View, clock, text_centered};
use juice::{Juice, Priority};
use presentation::lap_comparison;
use session::{Notice, RaceSession};
use sim::{Controls, LAPS, Pose, Race, Track};
use style::{Before, Event, Style};

const CREAM: Color = juice::CREAM;
const INK: Color = hud::INK;
#[allow(unused_mut)]
fn conf() -> Conf {
    let mut conf = ferravex_render::window_conf("Pocket Rally / Ferravex");
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    capture::window(&mut conf);
    conf
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
/// The car is drawn 15% larger than its point-collision footprint so it reads
/// at follow-camera zoom; `scale` adds the bump squash (length, width).
fn car(pose: Pose, ghost: bool, scale: Vec2) {
    let s = scale * 1.15;
    let q = |center: Vec2, half: Vec2, color: Color| local_quad(pose, center * s, half * s, color);
    if ghost {
        q(
            Vec2::ZERO,
            vec2(12., 6.),
            Color::new(0.95, 0.89, 0.60, 0.30),
        );
        q(
            vec2(3., 0.),
            vec2(2., 4.6),
            Color::new(0.95, 0.89, 0.60, 0.25),
        );
        return;
    }
    local_quad(
        Pose {
            position: pose.position + vec2(3., 4.),
            ..pose
        },
        Vec2::ZERO,
        vec2(13., 7.) * s,
        Color::new(0.08, 0.12, 0.10, 0.32),
    );
    for x in [-7., 7.] {
        for y in [-6., 6.] {
            q(vec2(x, y), vec2(3., 1.6), rgba(25, 30, 27));
        }
    }
    q(Vec2::ZERO, vec2(12., 5.8), rgba(184, 62, 39));
    q(vec2(1., 0.), vec2(10., 2.), CREAM);
    q(vec2(-1., 0.), vec2(4., 5.), rgba(224, 101, 58));
    q(vec2(3., 0.), vec2(2., 4.7), rgba(42, 66, 65));
    q(vec2(-5., 0.), vec2(1.5, 4.7), rgba(42, 66, 65));
    for y in [-3.7, 3.7] {
        q(vec2(11., y), vec2(1., 1.3), CREAM);
    }
    q(vec2(-11., 0.), vec2(1.2, 6.8), rgba(90, 46, 33));
}
/// Three pond ducks bob on the water and paddle away from a nearby car.
fn ducks(time: f32, motion: f32, car: Vec2) {
    for (i, home) in [vec2(990., 572.), vec2(1032., 590.), vec2(962., 598.)]
        .into_iter()
        .enumerate()
    {
        let bob = vec2(
            (time * 0.7 + i as f32).sin(),
            (time * 0.9 + i as f32 * 2.).cos(),
        ) * 3.;
        let away = home - car;
        let flee = away.normalize_or_zero() * (70. - away.length()).clamp(0., 26.);
        let p = home + bob * motion + flee;
        let facing = if flee.length() > 1. {
            flee.normalize()
        } else {
            vec2(if i % 2 == 0 { 1. } else { -1. }, 0.)
        };
        draw_circle(p.x + 1., p.y + 2., 5., Color::new(0.1, 0.2, 0.2, 0.25));
        draw_circle(p.x, p.y, 5., rgba(250, 246, 232));
        let head = p + facing * 5.;
        draw_circle(head.x, head.y, 3., rgba(62, 112, 74));
        let beak = head + facing * 3.;
        let side = vec2(-facing.y, facing.x) * 1.5;
        draw_triangle(
            beak + side,
            beak - side,
            beak + facing * 3.,
            rgba(238, 152, 56),
        );
    }
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
    // Generous base land: portrait follow views can extend past the fields.
    draw_rectangle(-700., -500., 2500., 1650., rgba(158, 161, 111));
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
    let pond = style::POND;
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
    // The follow camera, juice and style tracker are reset by the caller.
}

/// Countdown fly-in from the circuit overview to the follow camera. Reduced
/// motion starts on the follow camera instead of zooming.
fn camera_mix(session: &RaceSession, juice: &Juice) -> f32 {
    if juice.motion() == 0. || session.countdown() == 0 {
        return 1.;
    }
    let t = ((session.countdown_progress() - 0.3) / 0.6).clamp(0., 1.);
    t * t * (3. - 2. * t)
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
    race.derive_splits(&track);
    let mut record_error = false;
    // Race records are optional: a storage failure still leaves a playable game.
    let mut records = ferravex_render::storage::Records::new("pocket-rally").ok();
    let mut results = hud::Results::default();
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
    let mut style = Style::new();
    let mut juice = Juice::new(&track);
    let mut particles = ParticleRenderer::new(512).expect("bounded presentation particles");
    let mut attempt = 0_u32;
    let mut pause_seed = 0_u32;
    let mut was_paused = false;
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    let mut capture = capture::Capture::from_env();
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
        ferravex_render::ui::warm_up_font_sizes(&[
            11, 12, 13, 14, 15, 16, 17, 18, 20, 22, 28, 32, 36, 44,
        ]);
        let w = screen_width();
        let h = screen_height();
        let marks_enabled = presentation::sync_marks(&mut marks);
        juice.apply_policy(ferravex_render::preferences::feedback_policy());
        let touch_layout = presentation::touch_layout(w, h);
        player_input.set_touch_layout(&touch_layout).unwrap();
        player_input.sample_play();
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        {
            if capture.pause_now() {
                player_input.set_paused(true);
            }
            capture.pond(&mut race);
        }
        let touch_mode = player_input.touch_seen();
        let restarting = player_input.pressed("restart")
            || race.finished()
                && !player_input.controls_open()
                && (player_input.pressed("confirm") || is_mouse_button_pressed(MouseButton::Left));
        if restarting {
            restart(&mut race, &mut session, &track, &mut marks, &mut sound);
            style = Style::new();
            juice.reset(&track);
            results = hud::Results::default();
            attempt += 1;
            player_input.set_paused(false);
            player_input.sample(true);
        }
        let paused = player_input.paused();
        if paused && !was_paused {
            pause_seed += 1;
        }
        was_paused = paused;
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
                #[allow(unused_mut)]
                let mut auto = race.autopilot(&track);
                #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
                capture.steer(&mut auto);
                auto
            } else {
                controls
            };
            let laps = race.laps;
            let finished = race.finished();
            let previous_best = race.best_time;
            let countdown = session.countdown();
            let notice = session.notice;
            let before = Before::new(&race, session.off_track());
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
                    if session.countdown() == 0 {
                        juice.on_go();
                    }
                }
                if countdown > 0 {
                    juice.follow(&race, frame.dt);
                    juice.advance(frame.dt);
                }
                continue;
            }
            style.observe(before, &race, &track, input, session.off_track(), frame.dt);
            juice.on_tick(&race, &style, &track);
            juice.follow(&race, frame.dt);
            juice.advance(frame.dt);
            if race.best_time != previous_best
                && !demo.autoplay
                && let Some(best) = save::BestLap::from_race(&race, record_context)
            {
                record_error = saved_lap.commit(best).is_err();
            }
            if race.laps > laps {
                sound.cue(Cue::Pickup, 0.5);
                if !race.finished() {
                    let best = race.best_time != previous_best;
                    let clean = style
                        .events
                        .iter()
                        .any(|event| matches!(event, Event::Lap { clean: true, .. }));
                    juice.lap_toast(
                        race.laps,
                        race.last_lap.unwrap_or(0.),
                        best,
                        lap_comparison(race.last_comparison, best),
                        clean.then(|| quips::clean_lap(style.stats.clean_laps)),
                        race.laps + 1 == LAPS,
                    );
                }
            }
            if session.notice != notice && session.notice == Some(Notice::BackOnTrack) {
                juice.toast(
                    "BACK ON TRACK".to_owned(),
                    quips::back_on_track(style.stats.off_track_episodes).to_owned(),
                    (juice::GREEN, CREAM),
                    1.4,
                    Priority::Info,
                );
            }
            for event in style.events.iter() {
                match event {
                    Event::DriftBanked { rating, .. } => {
                        sound.cue(Cue::Pickup, 0.16 + 0.05 * rating as u8 as f32);
                    }
                    Event::DriftBotched { .. } => sound.cue(Cue::Failure, 0.3),
                    Event::Bump { strength } => {
                        sound.cue(Cue::Impact(Surface::Wood), 0.3 + 0.4 * strength);
                    }
                    Event::Splash { .. } => sound.cue(Cue::Impact(Surface::Water), 0.6),
                    Event::Donut => sound.cue(Cue::Success, 0.35),
                    Event::Split { delta, .. } if delta <= 0. => sound.cue(Cue::Click, 0.22),
                    _ => {}
                }
            }
            if race.finished() && !finished {
                ferravex_render::performance::context(Some(race.ticks), None);
                ferravex_render::performance::event(
                    ferravex_play::performance::DiagnosticKind::Completed,
                    race.total_time,
                );
                sound.cue(Cue::Success, 0.65);
                results = hud::Results {
                    demo: demo.autoplay,
                    ..Default::default()
                };
                if !demo.autoplay
                    && let Some(records) = &mut records
                {
                    let previous = records.get("race", record_context);
                    results.previous_best = previous
                        .and_then(|record| record.fastest_us)
                        .map(|us| us as f32 / 1_000_000.);
                    results.previous_style =
                        previous.map(|record| record.score.min(u64::from(u32::MAX)) as u32);
                    // A failed write keeps the in-memory best for this session.
                    let _ = records.attempt(
                        "race",
                        record_context,
                        u64::from(style.stats.style),
                        ferravex_play::persistence::run_micros(race.total_time),
                    );
                }
                #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
                capture.results(&mut results);
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
                            color: if race.car.offroad {
                                [0.36, 0.33, 0.20, 0.30]
                            } else {
                                [0.20, 0.22, 0.17, 0.34]
                            },
                            width: 1.6,
                        }),
                    )
                    .unwrap();
            }
        }
        if race.finished() && active {
            juice.advance_celebration(demo.dt());
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

        // World: follow camera with the countdown fly-in and juice shake.
        clear_background(rgba(137, 148, 103));
        let bar = hud::bar_height(w);
        // At the finish, ease the car left of the results card on wide screens.
        let framing = if race.finished() {
            let ease = if juice.motion() > 0. {
                let t = (juice.celebrate / 0.8).clamp(0., 1.);
                t * t * (3. - 2. * t)
            } else {
                1.
            };
            (vec2(w, h) / 2. - hud::finish_framing(w, h), ease)
        } else {
            (Vec2::ZERO, 0.)
        };
        let (center, scale) = juice.view(w, h, bar, camera_mix(&session, &juice), framing);
        let view = View {
            width: w,
            height: h,
            center,
            scale,
        };
        // A full-window projection keeps input in logical display pixels, avoiding raw
        // framebuffer viewport coordinates on Retina and fractional-scale displays.
        let mut camera = Camera2D::from_display_rect(Rect::new(
            center.x - w / (2. * scale),
            center.y - h / (2. * scale),
            w / scale,
            h / scale,
        ));
        camera.zoom.y = camera.zoom.y.abs();
        set_camera(&camera);
        draw_land(&track);
        ducks(juice.time, juice.motion().min(1.), race.car.pose.position);
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
        particles
            .draw_2d(
                &juice.particles,
                ParticleStyle {
                    stretch: 0.004,
                    max_stretch: 2.5,
                    ..Default::default()
                },
            )
            .expect("authored particle style");
        if !race.finished() {
            let (gate, tangent) = track.gate(race.next_gate);
            let normal = vec2(-tangent.y, tangent.x);
            let pulse = 1. + 0.18 * (juice.time * 6.).sin() * juice.motion().min(1.);
            for side in [-1., 1.] {
                let p = gate + normal * 44. * side;
                draw_triangle(
                    p - (tangent * 6. + normal * 3.) * pulse,
                    p + tangent * 5. * pulse,
                    p - (tangent * 6. - normal * 3.) * pulse,
                    CREAM,
                );
            }
            if let Some(ghost) = race.ghost() {
                car(ghost, true, Vec2::ONE);
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
        car(race.car.pose, false, juice.car_scale());
        set_default_camera();

        // HUD, in contract order: bar texts, then guidance, then cards.
        let hints = Hints::new(&player_input);
        hud::flash(&juice, w, h);
        hud::pops(&juice, &style, &race, &view);
        hud::top_bar(&race, &juice, w);
        let best_label = race.best_time.map(|best| {
            format!(
                "{} {}",
                if record_error {
                    "SESSION BEST"
                } else {
                    "BEST LAP"
                },
                clock(best)
            )
        });
        hud::minimap(
            &track,
            &race,
            race.ghost(),
            hud::minimap_rect(w),
            best_label,
        );
        hud::banner(
            &race,
            &track,
            &session,
            &style,
            &juice,
            demo.autoplay && race.driver_reversing(),
            w,
        );
        if !paused && let Some(tip) = hud::coach(&race, &session, &style, &hints, attempt) {
            hud::coach_chip(&tip, w, h, touch_mode);
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
        hud::go_stamp(&juice, w, h);
        if session.countdown() > 0 && !session.clock.is_paused() {
            let record = records
                .as_ref()
                .and_then(|records| records.get("race", record_context))
                .and_then(|record| record.fastest_us)
                .map(|us| {
                    format!(
                        "BEST RACE {}  ·  GOLD UNDER {:.0}S",
                        clock(us as f32 / 1_000_000.),
                        quips::MEDALS[0]
                    )
                });
            hud::countdown(&session, &juice, &hints, attempt, record, w, h);
        }
        if race.finished() {
            hud::results(
                &race,
                &style,
                &juice,
                &results,
                &presentation::retry_hint(&player_input)
                    .to_uppercase()
                    .replace("TO RETRY", "TO RACE AGAIN"),
                w,
                h,
            );
        }
        if paused && !player_input.controls_open() {
            hud::paused(&race, &hints, pause_seed, w, h);
        }
        ferravex_render::performance::context(Some(race.ticks), None);
        // The game paints its own pause card, so skip the shared PAUSED badge.
        player_input.draw_controls();
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        capture.end_frame();
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
