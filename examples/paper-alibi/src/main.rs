fn main() {
    if std::env::args().any(|arg| arg == "--headless") {
        use ferravex_arcade::Outcome;
        let mut game = paper_alibi::Game::new();
        for tick in 0..30_000 {
            let input = game.autoplay(tick);
            game.step(&input);
            if game.outcome() != Outcome::Playing {
                assert_eq!(game.outcome(), Outcome::Won, "scripted investigator failed");
                println!(
                    "Paper Alibi: all {} cases solved legally at tick {}; credibility {}/4; score {}.",
                    game.cases_solved(),
                    tick,
                    game.credibility(),
                    game.score()
                );
                return;
            }
        }
        panic!("campaign did not finish within the headless bound");
    }
    paper_alibi::run();
}
