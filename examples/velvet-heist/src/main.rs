use ferravex_arcade::Outcome;
fn main() {
    if std::env::args().any(|a| a == "--headless") {
        let mut g = velvet_heist::Game::new();
        let mut old = 0;
        for tick in 0..120_000 {
            if old != g.heist() {
                old = g.heist();
                println!("tick {tick}: heist {old}, score {}", g.score());
            }
            g.step(&g.pilot(tick));
            if g.outcome() != Outcome::Playing {
                println!("tick {tick}: {:?}: {}", g.outcome(), g.summary());
                if g.outcome() != Outcome::Won {
                    std::process::exit(1);
                }
                return;
            }
        }
        eprintln!("Legal pilot timeout: {g:?}");
        std::process::exit(1);
    }
    velvet_heist::run();
}
