fn main() -> std::process::ExitCode {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments.as_slice() == ["--headless"] {
        use ferravex_arcade::{ArcadeGame, Outcome};
        let mut game = saltwind::Game::new();
        for tick in 0..40_000 {
            let input = game.pilot(tick);
            game.tick(&input);
            if game.result() != Outcome::Playing {
                println!(
                    "Saltwind: {:?} after {} fixed ticks. {} Score: {}.",
                    game.result(),
                    tick + 1,
                    game.summary(),
                    game.hud().score
                );
                return if game.result() == Outcome::Won {
                    std::process::ExitCode::SUCCESS
                } else {
                    std::process::ExitCode::FAILURE
                };
            }
        }
        eprintln!("Saltwind pilot did not complete the campaign within 40,000 ticks.");
        return std::process::ExitCode::FAILURE;
    }
    saltwind::run();
    std::process::ExitCode::SUCCESS
}
