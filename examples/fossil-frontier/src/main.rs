use ferravex_arcade::Outcome;
fn main() {
    if std::env::args().any(|s| s == "--headless") {
        let mut g = fossil_frontier::Game::new();
        for tick in 0..100_000 {
            g.step(&g.pilot(tick));
            if g.outcome() != Outcome::Playing {
                println!("tick {tick}: {:?}: {}", g.outcome(), g.summary());
                if g.outcome() != Outcome::Won {
                    std::process::exit(1)
                }
                return;
            }
        }
        eprintln!("Legal pilot timed out: {g:?}");
        std::process::exit(1)
    }
    fossil_frontier::run();
}
