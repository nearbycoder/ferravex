fn main() {
    if std::env::args().any(|arg| arg == "--headless") {
        let mut game = hearthharvest::Game::new();
        let mut day = 0;
        for tick in 0..100_000 {
            let input = game.autoplay(tick);
            game.tick(&input);
            if game.day() != day {
                day = game.day();
                println!(
                    "Day {day}: {} coins, {} orders delivered",
                    game.coins(),
                    game.orders_completed()
                );
            }
            if game.outcome() != ferravex_arcade::Outcome::Playing {
                if game.outcome() != ferravex_arcade::Outcome::Won {
                    eprintln!(
                        "Campaign lost: {}",
                        ferravex_arcade::ArcadeGame::summary(&game)
                    );
                    std::process::exit(1);
                }
                println!(
                    "Hearthharvest legal season won at tick {tick}: score {}, {} harvested orders, {} coins",
                    game.score(),
                    game.orders_completed(),
                    game.coins()
                );
                return;
            }
        }
        eprintln!("Legal season did not finish within the bounded run.");
        std::process::exit(1);
    }
    hearthharvest::run();
}
