use ferravex_arcade::Outcome;
fn main() {
    if std::env::args().any(|arg| arg == "--headless") {
        let mut game = tin_automata::Game::new();
        let mut last = 0;
        for tick in 0..100_000 {
            if game.room() != last {
                last = game.room();
                println!("tick {tick}: workbench {last}, {} points", game.score());
            }
            game.step(&game.pilot(tick));
            if game.outcome() != Outcome::Playing {
                println!("tick {tick}: {:?}; {}", game.outcome(), game.summary());
                return;
            }
        }
        eprintln!("Program pilot did not finish: {game:?}");
        std::process::exit(1);
    }
    tin_automata::run();
}
