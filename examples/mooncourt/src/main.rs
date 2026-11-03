use ferravex_arcade::Outcome;
fn main() {
    if std::env::args().any(|arg| arg == "--headless") {
        let mut game = mooncourt::Game::new();
        let mut last = 0;
        for tick in 0..300_000 {
            if game.match_number() != last {
                last = game.match_number();
                println!("tick {tick}: match {last}, score {}", game.score());
            }
            game.step(&game.pilot(tick));
            if game.outcome() != Outcome::Playing {
                println!("tick {tick}: {:?}: {}", game.outcome(), game.summary());
                if game.outcome() != Outcome::Won {
                    std::process::exit(1);
                }
                return;
            }
        }
        eprintln!(
            "Legal pilot timeout: phase {:?}, games {:?}, points {:?}",
            game.phase(),
            game.games(),
            game.points()
        );
        eprintln!("{game:?}");
        std::process::exit(1);
    }
    mooncourt::run();
}
