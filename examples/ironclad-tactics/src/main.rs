use ferravex_arcade::{Input, Outcome};
fn main() {
    if std::env::args().any(|a| a == "--headless") {
        let mut game = ironclad_tactics::Game::new();
        let mut previous = 0;
        for tick in 0..100_000 {
            if game.mission() != previous {
                previous = game.mission();
                println!("tick {tick}: operation {previous}, score {}", game.score());
            }
            let command: Input = game.autoplay(tick);
            game.step(&command);
            if game.outcome() != Outcome::Playing {
                println!("tick {tick}: {:?}, score {}", game.outcome(), game.score());
                if game.outcome() != Outcome::Won {
                    std::process::exit(1);
                }
                return;
            }
        }
        eprintln!("Campaign did not finish within 100000 legal-input ticks.");
        std::process::exit(1);
    }
    ironclad_tactics::run();
}
