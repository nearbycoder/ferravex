#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static ALLOCATOR: ferravex_core::telemetry::TrackingAllocator =
    ferravex_core::telemetry::TrackingAllocator;
#[cfg(not(target_arch = "wasm32"))]
fn main() {
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
    for _ in 0..frames {
        let token = performance.as_ref().map(|_| FrameToken::begin());
        let input = if autoplay {
            app.game.autoplay()
        } else {
            owned_orbit::game::Input::default()
        };
        app.game.tick(input, 1.0 / 120.0);
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
