use ferravex_arcade::Outcome;
fn main() {
    if std::env::args().any(|arg| arg == "--headless") {
        let mut game = pantry_panic::Game::new();
        let mut shift = 1;
        for tick in 0..180_000 {
            let command = game.pilot(tick);
            game.step(&command);
            if game.shift() != shift {
                shift = game.shift();
                println!(
                    "tick {tick}: service {shift}, {} dishes, score {}",
                    game.served_total(),
                    game.score()
                );
            }
            if game.outcome() != Outcome::Playing {
                println!(
                    "tick {tick}: {:?}, {} dishes, score {}",
                    game.outcome(),
                    game.served_total(),
                    game.score()
                );
                println!("{}", game.summary());
                if game.outcome() != Outcome::Won {
                    std::process::exit(1);
                }
                return;
            }
        }
        eprintln!("The legal kitchen pilot did not finish within 180000 ticks.");
        std::process::exit(1);
    }
    pantry_panic::run();
}
