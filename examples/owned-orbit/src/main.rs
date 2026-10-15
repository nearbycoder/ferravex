#[cfg(not(target_arch = "wasm32"))]
fn main() {
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
        let input = if autoplay {
            app.game.autoplay()
        } else {
            owned_orbit::game::Input::default()
        };
        app.game.tick(input, 1.0 / 120.0);
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
