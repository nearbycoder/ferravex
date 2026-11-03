fn main() {
    if std::env::args().any(|arg| arg == "--headless") {
        let mut game = stormbound::Game::new();
        for tick in 0..100_000 {
            let input = game.pilot(tick);
            game.step(&input);
            if game.outcome() != ferravex_arcade::Outcome::Playing {
                println!("{} ticks: {}", tick + 1, game.summary());
                if game.outcome() != ferravex_arcade::Outcome::Won {
                    std::process::exit(1);
                }
                return;
            }
        }
        eprintln!("Campaign timeout: {}", game.summary());
        std::process::exit(1);
    }
    stormbound::run();
}
