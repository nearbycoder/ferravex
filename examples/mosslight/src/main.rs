fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    if std::env::args().any(|arg| arg == "--headless") {
        if std::env::args().count() != 2 {
            eprintln!(
                "Usage: mosslight --headless (or native --autoplay --smoke-frames N --screenshot PATH)"
            );
            std::process::exit(2);
        }
        let mut game = mosslight::Game::new();
        let mut floor = game.floor();
        for tick in 0..54_000 {
            let input = game.autoplay(tick);
            game.step(&input);
            if game.floor() != floor {
                println!(
                    "Floor {floor} cleared at tick {tick}, turn {}",
                    game.turns()
                );
                floor = game.floor();
            }
            if game.outcome() != ferravex_arcade::Outcome::Playing {
                println!(
                    "Finished at tick {tick}: {:?}, score {}, health {}, turns {}",
                    game.outcome(),
                    game.score(),
                    game.health(),
                    game.turns()
                );
                println!("{}", ferravex_arcade::ArcadeGame::summary(&game));
                break;
            }
        }
        if game.outcome() != ferravex_arcade::Outcome::Won {
            eprintln!("Headless expedition did not reach the campaign ending.");
            std::process::exit(1);
        }
        return;
    }
    mosslight::run();
}
