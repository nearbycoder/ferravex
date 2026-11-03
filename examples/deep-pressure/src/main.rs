fn main() {
    if std::env::args().any(|arg| arg == "--headless") {
        use ferravex_arcade::Outcome;
        let mut game = deep_pressure::Game::new();
        let mut previous = 0;
        for tick in 0..120_000 {
            let input = game.pilot(tick);
            game.tick(&input);
            if game.sortie() != previous {
                previous = game.sortie();
                println!(
                    "Sortie {} at tick {}: {} survivors home, {} points",
                    previous,
                    tick,
                    game.rescued(),
                    game.score()
                );
            }
            if game.outcome() != Outcome::Playing {
                println!(
                    "{:?} at tick {}: {} survivors, {} points",
                    game.outcome(),
                    tick,
                    game.rescued(),
                    game.score()
                );
                if game.outcome() != Outcome::Won {
                    std::process::exit(1);
                }
                return;
            }
        }
        eprintln!(
            "Pilot did not finish: sortie {}, position {:?}, air {}, survivors {}",
            game.sortie(),
            game.position(),
            game.oxygen(),
            game.rescued()
        );
        std::process::exit(1);
    }
    deep_pressure::run();
}
