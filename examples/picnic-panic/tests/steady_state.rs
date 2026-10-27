//! A busy picnic ticks without a single heap allocation.
//!
//! Runs as its own test binary so no other test thread allocates concurrently.
//! Counting uses the engine's tracking allocator, which the `game!` macro
//! installs for native builds; recording is enabled through its perf session.
//! The window covers the rigid-body world (seesaw, bell, sign and bunting
//! joints), the raccoon's character motor, five picnickers' perception,
//! decisions and path planning, the to-do list and the demo raccoon's input.
use ferravex_core::telemetry::{FrameSample, FrameToken, Session};
use picnic_panic::sim::{Event, Game, MAX_EVENTS, RunConfig, bot::Bot};

#[test]
fn steady_state_ticks_do_not_allocate() {
    let out = std::env::temp_dir().join("picnic-panic-steady-state.json");
    // SAFETY: this test binary is single-test and sets the variable before
    // any other thread could read the environment.
    unsafe { std::env::set_var("FERRAVEX_PERF_OUTPUT", &out) };
    let _session = Session::from_env().expect("enables allocation counting");
    let mut game = Game::new(RunConfig {
        seed: 42,
        completed: 0,
    });
    let mut bot = Bot::new(&game);
    let mut events: Vec<Event> = Vec::with_capacity(MAX_EVENTS);
    // Warm-up: let solver caches, routes and pools reach working sizes.
    for _ in 0..60 * 45 {
        let input = bot.input(&game);
        game.update(input);
        game.drain_events(&mut events);
        events.clear();
    }
    let token = FrameToken::begin();
    let mut moving = 0_u32;
    for _ in 0..60 * 20 {
        let input = bot.input(&game);
        game.update(input);
        game.drain_events(&mut events);
        events.clear();
        moving += u32::from(game.folk.iter().any(|f| f.speed > 0.1));
    }
    let sample = token.finish(FrameSample::default());
    assert!(moving > 600, "picnickers were busy ({moving} moving ticks)");
    assert_eq!(
        sample.allocation_calls, 0,
        "{} allocations ({} bytes) over 1200 ticks",
        sample.allocation_calls, sample.allocation_bytes
    );
}
