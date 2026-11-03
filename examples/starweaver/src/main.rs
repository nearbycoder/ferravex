use ferravex_arcade::{ArcadeGame, Outcome};
fn main() {
    if std::env::args().any(|arg| arg == "--headless") {
        let mut game = starweaver::Game::new();
        for tick in 0..60_000 {
            let input = game.pilot(tick);
            game.step(&input);
            if game.outcome() != Outcome::Playing {
                println!("Starweaver ended at tick {tick}: {}", game.summary());
                if game.outcome() == Outcome::Won {
                    return;
                }
                std::process::exit(1);
            }
        }
        eprintln!("Legal pilot did not finish: {}", game.summary());
        std::process::exit(1);
    }
    starweaver::run();
}
