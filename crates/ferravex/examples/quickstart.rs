//! A facade-only example: arrows move, Space pauses, N steps while paused,
//! and R restarts. `--headless` runs a fixed scenario without opening a window.
//!
//! Run on a supported native graphics host with:
//! `cargo run -p ferravex --example quickstart --offline --locked -j 2`.
//! Simulation uses a 60 Hz session; rendering interpolates its last two poses.

use ferravex::prelude::*;

const SIZE: Vec2 = vec2(640.0, 360.0);
const RADIUS: f32 = 14.0;
const SPEED: f32 = 180.0;

struct Ball {
    previous: Vec2,
    position: Vec2,
}

impl Ball {
    fn new() -> Self {
        Self {
            previous: SIZE * 0.5,
            position: SIZE * 0.5,
        }
    }

    // Simulation has no renderer or wall-clock access.
    fn fixed_update(&mut self, dt: f32, direction: Vec2) {
        self.previous = self.position;
        self.position = (self.position + direction.clamp_length_max(1.0) * SPEED * dt)
            .clamp(Vec2::splat(RADIUS), SIZE - Vec2::splat(RADIUS));
    }

    fn visible_position(&self, alpha: f32, playing: bool) -> Vec2 {
        if playing {
            self.previous.lerp(self.position, alpha)
        } else {
            // Inactive sessions return alpha=0; show the current pose when paused.
            self.position
        }
    }
}

fn new_session() -> GameSession {
    let config = SessionConfig::try_new(60.0, 8, true).expect("valid quickstart settings");
    GameSession::from_config(config)
}

// Both hosts and tests execute this same simulation path. The inspector command
// issues an actual tick only while paused; it never adds a tick while playing.
fn advance(
    ball: &mut Ball,
    session: &mut GameSession,
    elapsed: f64,
    direction: Vec2,
    single_step: bool,
) -> FrameStep {
    let frame = if single_step && session.is_paused() {
        session.step_paused().expect("session is paused")
    } else {
        session.advance(elapsed)
    };
    for _ in 0..frame.steps {
        // Consume any gameplay action edges here, once per actual tick.
        // This example has held movement only; N is a host inspector command.
        ball.fixed_update(frame.dt, direction);
    }
    frame
}

fn render_frame(ball: &Ball, alpha: f32, state: SessionState) {
    render::clear_background(Color::new(0.05, 0.07, 0.12, 1.0));
    render::set_camera(&Camera2D::from_display_rect(Rect::new(
        0.0, 0.0, SIZE.x, SIZE.y,
    )));
    let position = ball.visible_position(alpha, state == SessionState::Playing);
    render::draw_circle(position.x, position.y, RADIUS, render::ORANGE);
    render::draw_text(
        "Arrows move | Space pauses | N steps | R restarts",
        18.0,
        28.0,
        20.0,
        render::WHITE,
    );
    if state == SessionState::Paused {
        render::draw_text(
            "Paused — N steps once; Space resumes",
            18.0,
            56.0,
            20.0,
            render::YELLOW,
        );
    }
}

fn config() -> Conf {
    Conf {
        window_title: "Ferravex quickstart".into(),
        window_width: SIZE.x as i32,
        window_height: SIZE.y as i32,
        ..Conf::default()
    }
}

async fn game() {
    let mut session = new_session();
    let mut ball = Ball::new();

    loop {
        // Host controls run once per rendered frame, outside catch-up updates.
        // Restart resets both clock debt and interpolation history.
        if render::is_key_pressed(KeyCode::R) {
            session.restart();
            ball = Ball::new();
        } else if render::is_key_pressed(KeyCode::Space) {
            session.toggle_pause();
        }
        let direction = vec2(
            i32::from(render::is_key_down(KeyCode::Right)) as f32
                - i32::from(render::is_key_down(KeyCode::Left)) as f32,
            i32::from(render::is_key_down(KeyCode::Down)) as f32
                - i32::from(render::is_key_down(KeyCode::Up)) as f32,
        );
        let frame = advance(
            &mut ball,
            &mut session,
            f64::from(render::get_frame_time()),
            direction,
            render::is_key_pressed(KeyCode::N),
        );

        // Rendering only reads simulation snapshots, even on zero-step frames.
        render_frame(&ball, frame.alpha, session.state());
        render::next_frame().await;
    }
}

// This is the existing renderer macro, reached entirely through the facade.
ferravex::render::game!(config, game);

#[cfg(not(target_arch = "wasm32"))]
fn headless() {
    let mut session = new_session();
    let mut ball = Ball::new();
    for _ in 0..60 {
        advance(&mut ball, &mut session, 1.0 / 60.0, Vec2::X, false);
    }
    assert_eq!(session.tick(), 60);
    assert!(
        ball.position
            .abs_diff_eq(SIZE * 0.5 + Vec2::X * SPEED, 0.001)
    );

    session.set_paused(true);
    let paused = advance(&mut ball, &mut session, 30.0, Vec2::X, false);
    assert_eq!(paused.steps, 0);
    let stepped = advance(&mut ball, &mut session, 30.0, Vec2::X, true);
    assert_eq!(stepped.steps, 1);
    assert_eq!(session.tick(), 61);
    assert!(session.is_paused());
    assert_eq!(ball.visible_position(stepped.alpha, false), ball.position);
    assert!(
        ball.position
            .abs_diff_eq(SIZE * 0.5 + Vec2::X * (SPEED + SPEED / 60.0), 0.001)
    );

    session.restart();
    ball = Ball::new();
    assert_eq!(
        advance(&mut ball, &mut session, 0.0, Vec2::X, false).steps,
        0
    );
    assert_eq!(session.tick(), 0);
    assert_eq!(ball.visible_position(0.0, true), SIZE * 0.5);
    println!("quickstart headless: 60 fixed ticks, paused step, and restart passed");
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    let mut args = std::env::args_os().skip(1);
    match (args.next(), args.next()) {
        (None, None) => run(),
        (Some(arg), None) if arg == "--headless" => headless(),
        (Some(arg), None) if arg == "--help" || arg == "-h" => {
            println!(
                "Usage: quickstart [--headless | --help]\n\
                 Default: interactive window; arrows move, Space pauses, N steps, R restarts.\n\
                 --headless: run fixed simulation/pause/step/restart assertions without a window."
            );
        }
        _ => {
            eprintln!("Usage: quickstart [--headless | --help]");
            return std::process::ExitCode::from(2);
        }
    }
    std::process::ExitCode::SUCCESS
}

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_is_equal_across_render_rates() {
        for fps in [30, 60, 144] {
            let mut ball = Ball::new();
            let mut session = GameSession::new(60.0, 8, true);
            for _ in 0..fps {
                let frame = session.advance(1.0 / f64::from(fps));
                for _ in 0..frame.steps {
                    ball.fixed_update(frame.dt, Vec2::X);
                }
            }
            assert_eq!(session.tick(), 60);
            assert!(
                ball.position
                    .abs_diff_eq(SIZE * 0.5 + Vec2::X * SPEED, 0.001)
            );
        }
    }

    #[test]
    fn interpolation_does_not_mutate_simulation_and_pause_shows_current_pose() {
        let mut ball = Ball::new();
        ball.fixed_update(1.0 / 60.0, Vec2::X);
        let position = ball.position;
        assert_eq!(ball.visible_position(0.0, true), ball.previous);
        assert_eq!(
            ball.visible_position(0.5, true),
            (ball.previous + position) * 0.5
        );
        assert_eq!(ball.visible_position(0.0, false), position);
        assert_eq!(ball.position, position);
    }

    #[test]
    fn restart_clears_clock_debt_and_old_interpolation_pose() {
        let mut session = GameSession::new(60.0, 8, true);
        let mut ball = Ball::new();
        let frame = session.advance(1.5 / 60.0);
        for _ in 0..frame.steps {
            ball.fixed_update(frame.dt, Vec2::X);
        }
        assert_ne!(ball.position, SIZE * 0.5);
        session.restart();
        ball = Ball::new();
        let frame = session.advance(0.0);
        assert_eq!(frame.steps, 0);
        assert_eq!(session.tick(), 0);
        assert_eq!(
            ball.visible_position(frame.alpha, session.is_playing()),
            SIZE * 0.5
        );
    }

    #[test]
    fn inspector_step_keeps_pause_and_fractional_debt_without_repeating() {
        let mut ball = Ball::new();
        let mut session = new_session();
        assert_eq!(
            advance(&mut ball, &mut session, 0.5 / 60.0, Vec2::X, false).steps,
            0
        );
        session.set_paused(true);
        let frame = advance(&mut ball, &mut session, 30.0, Vec2::X, true);
        assert_eq!(frame.steps, 1);
        assert_eq!(frame.alpha, 1.0);
        assert_eq!(session.tick(), 1);
        assert!(session.is_paused());
        let current = ball.position;
        assert_eq!(
            ball.visible_position(frame.alpha, session.is_playing()),
            current
        );
        assert_eq!(
            advance(&mut ball, &mut session, 30.0, Vec2::X, false).steps,
            0
        );
        assert_eq!(ball.position, current);

        session.set_paused(false);
        let resumed = advance(&mut ball, &mut session, 0.5 / 60.0, Vec2::X, false);
        assert_eq!(resumed.steps, 1);
        assert_eq!(session.tick(), 2);
        assert!(
            ball.position
                .abs_diff_eq(SIZE * 0.5 + Vec2::X * (2.0 * SPEED / 60.0), 0.001)
        );
    }

    #[test]
    fn inspector_command_does_not_add_ticks_when_playing_or_terminal() {
        let mut ball = Ball::new();
        let mut session = new_session();
        assert_eq!(
            advance(&mut ball, &mut session, 0.0, Vec2::X, true).steps,
            0
        );
        assert_eq!(
            advance(&mut ball, &mut session, 2.0 / 60.0, Vec2::X, true).steps,
            2
        );
        assert_eq!(session.tick(), 2);
        session.finish(true);
        let current = ball.position;
        assert_eq!(
            advance(&mut ball, &mut session, 30.0, Vec2::X, true).steps,
            0
        );
        assert_eq!(session.tick(), 2);
        assert_eq!(ball.position, current);
    }
}
