use ferravex_arcade::Outcome;
fn main() {
    if std::env::args().any(|arg| arg == "--headless") {
        let mut game = gilded_deck::Game::new();
        let mut floor = 1;
        for tick in 0..100_000 {
            let input = game.pilot(tick);
            game.step(&input);
            if game.encounter() != floor {
                floor = game.encounter();
                println!(
                    "tick {tick}: encounter {floor}, {} HP, {} points",
                    game.health(),
                    game.score()
                );
            }
            if game.outcome() != Outcome::Playing {
                println!("tick {tick}: {:?}. {}", game.outcome(), game.summary());
                if game.outcome() != Outcome::Won {
                    std::process::exit(1);
                }
                return;
            }
        }
        eprintln!(
            "Legal pilot exceeded its 100000-tick bound. {}",
            game.summary()
        );
        std::process::exit(1);
    }
    gilded_deck::run();
}
