fn main() {
    if std::env::args().any(|a| a == "--headless") {
        let mut game = prism_foundry::Game::new();
        for tick in 0..20_000 {
            game.tick(&game.pilot(tick));
            if game.outcome() == ferravex_arcade::Outcome::Won {
                println!(
                    "All twelve chambers sealed at tick {tick}; score {}",
                    game.score()
                );
                return;
            }
        }
        eprintln!("Legal campaign did not complete within 20000 ticks");
        std::process::exit(1);
    }
    prism_foundry::run();
}
