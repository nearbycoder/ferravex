//! Native headless dedicated server. No renderer or windowing dependency.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{
        env, thread,
        time::{Duration, Instant},
    };
    let mut bind = "127.0.0.1:4000".to_owned();
    let mut ticks = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bind" => bind = args.next().ok_or("--bind requires an address")?,
            "--ticks" => {
                ticks = Some(
                    args.next()
                        .ok_or("--ticks requires a count")?
                        .parse::<u64>()?,
                )
            }
            "--help" | "-h" => {
                println!(
                    "ferravex-server [--bind 127.0.0.1:4000] [--ticks N]\n64 Hz authoritative UDP arena. --ticks exits after N ticks for smoke tests."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    let mut server = ferravex_fps::GameServer::bind(&bind)?;
    println!(
        "Ferravex dedicated server on {} | {} Hz | {} slots",
        server.local_addr()?,
        ferravex_fps::TICK_RATE,
        ferravex_fps::MAX_PLAYERS
    );
    let step = Duration::from_secs_f64(1.0 / ferravex_fps::TICK_RATE as f64);
    let started = Instant::now();
    let mut next = started;
    let mut count = 0u64;
    while ticks.is_none_or(|limit| count < limit) {
        let now = Instant::now();
        if now < next {
            thread::sleep(next - now);
        }
        server.tick()?;
        count += 1;
        next += step;
        // Avoid an unbounded catch-up spiral if the process was suspended.
        if Instant::now().saturating_duration_since(next) > Duration::from_millis(250) {
            next = Instant::now();
        }
        if count.is_multiple_of(640) {
            println!(
                "tick={} players={} rejected={}",
                server.game.tick,
                server.peer_count(),
                server.rejected_packets
            );
        }
    }
    println!(
        "Stopped after {count} ticks in {:.3}s",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}
#[cfg(target_arch = "wasm32")]
fn main() {
    eprintln!("ferravex-server requires native UDP; build the strike example for browser play.");
}
