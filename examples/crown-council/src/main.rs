fn main() {
    if std::env::args().any(|arg| arg == "--headless") {
        let mut game = crown_council::Game::new();
        for tick in 0..10_000 {
            let input = game.pilot(tick);
            game.step(&input);
            if game.outcome() != ferravex_arcade::Outcome::Playing {
                println!("{} | finished at tick {tick}", game.summary());
                if game.outcome() != ferravex_arcade::Outcome::Won {
                    std::process::exit(1);
                }
                return;
            }
        }
        eprintln!("Reign did not finish: {}", game.summary());
        std::process::exit(1);
    }
    crown_council::run();
}
