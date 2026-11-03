use ferravex_arcade::Outcome;
fn main() {
    if std::env::args().any(|a| a == "--headless") {
        let mut game = wildfire_watch::Game::new();
        let mut mission = 1;
        for tick in 0..100_000 {
            let input = game.pilot(tick);
            game.step(&input);
            if game.mission() != mission {
                mission = game.mission();
                println!("tick {tick}: incident {mission}, {} points", game.score());
            }
            if game.outcome() != Outcome::Playing {
                println!("tick {tick}: {:?}. {}", game.outcome(), game.summary());
                if game.outcome() != Outcome::Won {
                    std::process::exit(1);
                }
                return;
            }
        }
        eprintln!("Legal planner did not finish: {}", game.summary());
        std::process::exit(1);
    }
    wildfire_watch::run();
}
