fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    if std::env::args().any(|arg| arg == "--headless") {
        let mut game = voltage_choir::Game::new();
        let mut completed = 0;
        for tick in 0..60_000 {
            let command = game.autoplay(tick);
            game.step(&command);
            if game.completed_tracks() != completed {
                completed = game.completed_tracks();
                println!(
                    "Track {completed} cleared at tick {tick}: {} points",
                    game.score()
                );
            }
            if game.outcome() != ferravex_arcade::Outcome::Playing {
                println!(
                    "Voltage Choir: {:?}, {} songs, score {}, {} retry tickets, tick {}",
                    game.outcome(),
                    game.completed_tracks(),
                    game.score(),
                    game.retries(),
                    tick
                );
                if game.outcome() != ferravex_arcade::Outcome::Won {
                    std::process::exit(1);
                }
                return;
            }
        }
        eprintln!("Voltage Choir headless concert did not complete");
        std::process::exit(1);
    }
    voltage_choir::run();
}
