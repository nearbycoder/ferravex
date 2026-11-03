use ferravex_arcade::Outcome;
fn main() {
    if std::env::args().any(|a| a == "--headless") {
        let mut game = gravity_post::Game::new();
        let mut previous = 0;
        for tick in 0..180_000 {
            if game.contract() != previous {
                previous = game.contract();
                println!("tick {tick}: contract {previous}, score {}", game.score());
            }
            game.step(&game.pilot(tick));
            if game.outcome() != Outcome::Playing {
                println!("tick {tick}: {:?}: {}", game.outcome(), game.summary());
                if game.outcome() != Outcome::Won {
                    eprintln!("{game:?}");
                    std::process::exit(1);
                }
                return;
            }
        }
        eprintln!("Pilot timed out: {game:?}");
        std::process::exit(1);
    }
    gravity_post::run();
}
