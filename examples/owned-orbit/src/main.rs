#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static ALLOCATOR: ferravex_core::telemetry::TrackingAllocator =
    ferravex_core::telemetry::TrackingAllocator;
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Ok(name) = std::env::var("FERRAVEX_LOOK") {
        ferravex_engine::render::set_look(
            ferravex_engine::graphics::Look::from_name(&name)
                .expect("FERRAVEX_LOOK must be authored, warm, cool or ink"),
        );
    }
    use ferravex_core::telemetry::{FrameSample, FrameToken, Session};
    let mut performance = Session::from_env();
    let mut frames = 1800_u64;
    let mut autoplay = false;
    let mut screenshot = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--frames" => {
                frames = args
                    .next()
                    .expect("--frames needs N")
                    .parse()
                    .expect("integer frames")
            }
            "--autoplay" => autoplay = true,
            "--screenshot" => screenshot = Some(args.next().expect("--screenshot needs path")),
            "--help" => {
                println!(
                    "Orbit Breaker headless native host: --autoplay --frames N --screenshot path.png\nInteractive browser: /owned-orbit/"
                );
                return;
            }
            _ => panic!("unknown argument: {arg}"),
        }
    }
    assert!(frames <= 1_000_000, "frame budget too large");
    let mut app = owned_orbit::Orbit::new();
    #[cfg(debug_assertions)]
    let capture = capture::Capture::from_env(&mut app);
    for _frame in 0..frames {
        let token = performance.as_ref().map(|_| FrameToken::begin());
        let input = if autoplay {
            app.game.autoplay()
        } else {
            owned_orbit::game::Input::default()
        };
        #[cfg(debug_assertions)]
        let input = capture.input(&mut app, _frame, input);
        app.advance_input(1.0 / 120.0, input);
        #[cfg(debug_assertions)]
        if capture.reached(&app) {
            break;
        }
        if let (Some(session), Some(token)) = (&mut performance, token) {
            let cpu_us = token.elapsed_us();
            let render_start = std::time::Instant::now();
            app.render();
            session.record(token.finish(FrameSample {
                cpu_us,
                software_render_us: Some(render_start.elapsed().as_secs_f64() * 1e6),
                ..Default::default()
            }));
        }
    }
    if let Some(session) = performance {
        session
            .write(
                "software/orbit",
                app.canvas.frame.width(),
                app.canvas.frame.height(),
            )
            .expect("write performance report");
    }
    app.render();
    if let Some(path) = screenshot {
        app.canvas.frame.save_png(path).expect("write PNG");
    }
    println!(
        "phase={:?} sector={} lives={} panels={} score={} time={:.2}s ccd_impacts={}",
        app.game.phase,
        app.game.sector,
        app.game.lives,
        app.game.remaining(),
        app.game.score,
        app.game.time,
        app.game.ccd_impacts
    );
}
#[cfg(target_arch = "wasm32")]
fn main() {}

/// Debug-build-only visual review hooks; not compiled into release or Wasm,
/// so shipped behavior and the public CLI are unchanged.
///
/// - `ORBIT_FAKE_BEST=N` supplies a stored personal best of `N` points.
/// - `ORBIT_PAUSE_AT=N` pauses on frame `N`.
/// - `ORBIT_SABOTAGE_AT=N` makes autoplay park the paddle from frame `N`.
/// - `ORBIT_REDUCED=1` enables reduced motion and reduced flash.
/// - `ORBIT_UNTIL=multi|held|last|satellite|volley|transition|lost` stops the
///   run `ORBIT_AFTER` frames (default 12) after that state first appears.
#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
mod capture {
    use owned_orbit::{Orbit, game::Input};

    fn number<T: std::str::FromStr>(name: &str) -> Option<T> {
        std::env::var(name).ok()?.parse().ok()
    }

    pub struct Capture {
        pause_at: Option<u64>,
        sabotage_at: Option<u64>,
        until: Option<String>,
        after: u32,
        seen: std::cell::Cell<Option<u32>>,
    }

    impl Capture {
        pub fn from_env(app: &mut Orbit) -> Self {
            if let Some(best) = number("ORBIT_FAKE_BEST") {
                app.set_personal_best(Some(best));
            }
            if number::<u8>("ORBIT_REDUCED") == Some(1) {
                ferravex_engine::render::preferences::set_feedback_policy(
                    ferravex_engine::play::feedback::FeedbackPolicy::new(true, true, 1.)
                        .expect("valid policy"),
                );
            }
            Self {
                pause_at: number("ORBIT_PAUSE_AT"),
                sabotage_at: number("ORBIT_SABOTAGE_AT"),
                until: std::env::var("ORBIT_UNTIL").ok(),
                after: number("ORBIT_AFTER").unwrap_or(12),
                seen: std::cell::Cell::new(None),
            }
        }

        pub fn reached(&self, app: &Orbit) -> bool {
            use owned_orbit::game::Phase;
            let g = &app.game;
            let hit = match self.until.as_deref() {
                Some("multi") => !g.extra_balls.is_empty(),
                Some("held") => g.held.is_some(),
                Some("last") => g.remaining() == 1 && g.phase == Phase::Playing,
                Some("satellite") => g.satellites > 0,
                Some("volley") => g.volley >= 3,
                Some("transition") => g.phase == Phase::Transition,
                Some("lost") => g.feedback() == Some(owned_orbit::game::Feedback::LifeLost),
                _ => return false,
            };
            let seen = match (self.seen.get(), hit) {
                (Some(n), _) => n + 1,
                (None, true) => 0,
                (None, false) => return false,
            };
            self.seen.set(Some(seen));
            seen >= self.after
        }

        pub fn input(&self, app: &mut Orbit, frame: u64, input: Input) -> Input {
            if self.pause_at == Some(frame) {
                app.set_paused(true);
            }
            if self.sabotage_at.is_some_and(|at| frame >= at) {
                return Input {
                    axis: 1.,
                    launch: input.launch,
                    restart: false,
                };
            }
            input
        }
    }
}
