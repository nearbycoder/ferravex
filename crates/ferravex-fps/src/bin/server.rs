//! Native headless authority; optional first-party browser gateway, no renderer.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::{
        env, thread,
        time::{Duration, Instant},
    };
    let mut bind = "127.0.0.1:4000".to_owned();
    let mut ticks = None;
    let mut websocket: Option<String> = None;
    let mut origins = Vec::new();
    let mut tactical = false;
    let mut quick = false;
    let mut players = 2usize;
    let mut rounds = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bind" => bind = args.next().ok_or("--bind requires an address")?,
            "--websocket" => {
                websocket = Some(args.next().ok_or("--websocket requires an address")?)
            }
            "--origin" => origins.push(args.next().ok_or("--origin requires an exact origin")?),
            "--tactical" => tactical = true,
            "--quick" => {
                tactical = true;
                quick = true;
            }
            "--players" => players = args.next().ok_or("--players requires 2-16")?.parse()?,
            "--rounds" => {
                rounds = Some(
                    args.next()
                        .ok_or("--rounds requires a count")?
                        .parse::<u16>()?,
                )
            }
            "--ticks" => {
                ticks = Some(
                    args.next()
                        .ok_or("--ticks requires a count")?
                        .parse::<u64>()?,
                )
            }
            "--help" | "-h" => {
                println!(
                    "ferravex-server [--bind 127.0.0.1:4000] [--websocket 127.0.0.1:4001] [--origin http://127.0.0.1:8087] [--tactical | --quick] [--players 2] [--rounds 3] [--ticks N]\n64 Hz authority; native UDP plus optional first-party browser gateway. --quick selects shorter tactical rounds with 4000 starting credits. --ticks bounds a local test run."
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    if !tactical && (players != 2 || rounds.is_some()) {
        return Err("--players/--rounds require --tactical or --quick".into());
    }
    if websocket.is_none() && !origins.is_empty() {
        return Err("--origin requires --websocket".into());
    }
    let mut server = ferravex_fps::GameServer::bind(&bind)?;
    if tactical {
        let mut config = if quick {
            ferravex_play::rounds::Config {
                buy_seconds: 1.,
                round_seconds: 30.,
                bomb_seconds: 10.,
                plant_seconds: 1.,
                defuse_seconds: 2.,
                kit_defuse_seconds: 1.,
                intermission_seconds: 1.,
                rounds_to_win: 3,
                starting_money: 4000,
                ..Default::default()
            }
        } else {
            Default::default()
        };
        if let Some(rounds) = rounds {
            config.rounds_to_win = rounds;
        }
        server.enable_tactical(ferravex_fps::TacticalOptions { players, config })?;
    }
    let mut gateway = if let Some(bind) = websocket {
        let mut config = ferravex_net::websocket::Config::new(bind.parse()?, server.local_addr()?);
        config.allowed_origins.extend(origins);
        let gateway = ferravex_net::websocket::Gateway::bind(config)?;
        println!(
            "Browser WebSocket gateway ws://{}/ferravex",
            gateway.local_addr()?
        );
        Some(gateway)
    } else {
        None
    };
    println!(
        "Ferravex dedicated server on {} | {} Hz | {} slots",
        server.local_addr()?,
        ferravex_fps::TICK_RATE,
        if tactical {
            players
        } else {
            ferravex_fps::MAX_PLAYERS
        }
    );
    let step = Duration::from_secs_f64(1. / ferravex_fps::TICK_RATE as f64);
    let started = Instant::now();
    let mut next = started;
    let mut count = 0u64;
    while ticks.is_none_or(|limit| count < limit) {
        let now = Instant::now();
        if now < next {
            thread::sleep(next - now);
        }
        if let Some(gateway) = &mut gateway {
            gateway.poll()?;
        }
        server.tick()?;
        if let Some(gateway) = &mut gateway {
            gateway.poll()?;
        }
        count += 1;
        next += step;
        if Instant::now().saturating_duration_since(next) > Duration::from_millis(250) {
            next = Instant::now();
        }
        if count.is_multiple_of(640) {
            println!(
                "tick={} players={} rejected={} rejected_shots={} tactical={:?}",
                server.game.tick,
                server.peer_count(),
                server.rejected_packets,
                server.rejected_shots,
                server.tactical_view().map(|v| (v.phase, v.round, v.score))
            );
        }
    }
    println!(
        "Stopped after {count} ticks in {:.3}s",
        started.elapsed().as_secs_f64()
    );
    if let Some(gateway) = gateway {
        println!("Gateway {:?}", gateway.stats());
    }
    Ok(())
}
#[cfg(target_arch = "wasm32")]
fn main() {
    eprintln!(
        "ferravex-server is a native authority; browsers use the shared client and WebSocket gateway."
    );
}
