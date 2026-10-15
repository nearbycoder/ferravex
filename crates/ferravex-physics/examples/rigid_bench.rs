#[cfg(feature = "dim3")]
fn main() {
    use ferravex_physics::{SolverSettings, d3::PhysicsWorld};
    use std::time::Instant;
    let mut settings = SolverSettings::default();
    let mut ticks = 1200usize;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--cold" => settings.warm_start = false,
            "--no-sleep" => settings.allow_sleep = false,
            "--iterations" => {
                settings.velocity_iterations = args
                    .next()
                    .expect("iteration count")
                    .parse()
                    .expect("integer iterations")
            }
            "--ticks" => {
                ticks = args
                    .next()
                    .expect("tick count")
                    .parse()
                    .expect("integer ticks")
            }
            _ => panic!("rigid_bench [--cold] [--no-sleep] [--iterations N] [--ticks N]"),
        }
    }
    assert!((120..=10000).contains(&ticks));
    let mut world = PhysicsWorld::new([0., -9.81, 0.]);
    world.set_solver_settings(settings).unwrap();
    world.add_box([0., -0.5, 0.], [10., 0.5, 10.], false);
    for y in 0..4 {
        for x in 0..8 {
            for z in 0..4 {
                world.add_box(
                    [
                        x as f32 * 1.02 - 3.5,
                        0.5 + y as f32 * 1.002,
                        z as f32 * 1.02 - 1.5,
                    ],
                    [0.5; 3],
                    true,
                );
            }
        }
    }
    let mut times = Vec::with_capacity(ticks);
    let mut visits = 0usize;
    let mut warmed = 0usize;
    let mut persisted = 0usize;
    for _ in 0..ticks {
        let start = Instant::now();
        let stats = world.step(1. / 120.);
        times.push(start.elapsed().as_secs_f64() * 1e6);
        visits += stats.velocity_constraints;
        warmed += stats.warm_started_points;
        persisted += stats.persisted_points;
    }
    let mean = |times: &[f64]| times.iter().sum::<f64>() / times.len() as f64;
    let all = mean(&times);
    let early = mean(&times[..120]);
    let tail = mean(&times[times.len() - 120..]);
    times.sort_unstable_by(f64::total_cmp);
    println!(
        "{{\"schema\":1,\"bodies\":{},\"ticks\":{ticks},\"warm_start\":{},\"allow_sleep\":{},\"iterations\":{},\"mean_us\":{all:.3},\"p95_us\":{:.3},\"first_120_mean_us\":{early:.3},\"last_120_mean_us\":{tail:.3},\"constraint_visits\":{visits},\"warm_started_points\":{warmed},\"persisted_points\":{persisted},\"sleeping_bodies\":{},\"islands\":{},\"sleeping_islands\":{},\"cache_dropped\":{}}}",
        world.bodies.len(),
        settings.warm_start,
        settings.allow_sleep,
        settings.velocity_iterations,
        times[(times.len() * 95).div_ceil(100) - 1],
        world.last_step.sleeping_bodies,
        world.last_step.islands,
        world.last_step.sleeping_islands,
        world.last_step.cache_dropped
    );
}
#[cfg(not(feature = "dim3"))]
fn main() {
    eprintln!("enable the dim3 feature to run this benchmark");
}
