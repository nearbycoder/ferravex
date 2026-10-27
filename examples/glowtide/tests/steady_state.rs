//! A late-game horde of 300+ foes ticks without a single heap allocation.
//!
//! Runs as its own test binary so no other test thread allocates concurrently.
//! Counting uses the engine's tracking allocator, which the `game!` macro
//! installs for native builds; recording is enabled through its perf session.
use ferravex_core::telemetry::{FrameSample, FrameToken, Session};
use glowtide::sim::{Event, Game, MAX_EVENTS, RunConfig, bot};

#[test]
fn steady_state_ticks_with_a_300_plus_horde_do_not_allocate() {
    let out = std::env::temp_dir().join("glowtide-steady-state.json");
    // SAFETY: this test binary is single-test and sets the variable before
    // any other thread could read the environment.
    unsafe { std::env::set_var("FERRAVEX_PERF_OUTPUT", &out) };
    let _session = Session::from_env().expect("enables allocation counting");
    let mut game = Game::new(RunConfig {
        seed: 42,
        ..RunConfig::default()
    });
    game.time = 420.;
    game.stress(420);
    let mut events: Vec<Event> = Vec::with_capacity(MAX_EVENTS);
    // Warm-up: let pools reach their working sizes.
    for _ in 0..120 {
        let input = bot::autoplay(&game);
        game.update(input);
        game.drain_events(&mut events);
        events.clear();
    }
    let token = FrameToken::begin();
    let mut fewest = usize::MAX;
    for _ in 0..600 {
        let input = bot::autoplay(&game);
        game.update(input);
        game.drain_events(&mut events);
        events.clear();
        fewest = fewest.min(game.foes.len());
    }
    let sample = token.finish(FrameSample::default());
    assert!(fewest >= 300, "horde stayed above 300 foes (min {fewest})");
    assert_eq!(
        sample.allocation_calls, 0,
        "{} allocations ({} bytes) over 600 ticks",
        sample.allocation_calls, sample.allocation_bytes
    );
}
