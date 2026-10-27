//! Continuous-collision workload: every dynamic body has CCD enabled, as in the
//! physics labs, so each substep sweeps all bodies against all others.
//! Timings describe this machine only.
//!
//! `cargo run --release -p ferravex-physics --example ccd_bench [-- BODIES]`
#[cfg(feature = "dim3")]
fn main() {
    use ferravex_math::Vec3;
    use ferravex_physics::d3::{ColliderBuilder, PhysicsWorld, RigidBodyBuilder};
    use std::time::Instant;
    let bodies: usize = std::env::args()
        .nth(1)
        .map_or(96, |n| n.parse().expect("integer body count"));
    assert!((1..=2048).contains(&bodies));
    let mut world = PhysicsWorld::new([0., -9.81, 0.]);
    world.add_box([0., -0.5, 0.], [30., 0.5, 30.], false);
    for wall in [-1., 1.] {
        world.add_box([wall * 12., 3., 0.], [0.25, 3.5, 12.], false);
        world.add_box([0., 3., wall * 12.], [12., 3.5, 0.25], false);
    }
    for i in 0..bodies {
        let f = i as f32;
        let builder = RigidBodyBuilder::dynamic()
            .translation(Vec3::new(
                (i % 12) as f32 * 1.6 - 9.,
                1. + (i / 144) as f32 * 1.5,
                (i / 12 % 12) as f32 * 1.6 - 9.,
            ))
            .linvel(Vec3::new((f * 1.7).sin() * 30., 4., (f * 0.9).cos() * 30.))
            .ccd_enabled(true);
        let collider = if i % 2 == 0 {
            ColliderBuilder::ball(0.35)
        } else {
            ColliderBuilder::cuboid(0.3, 0.3, 0.3)
        };
        world.insert(builder, collider);
    }
    let ticks = 600;
    let mut samples = Vec::with_capacity(ticks);
    let mut impacts = 0;
    let mut substeps = 0;
    for _ in 0..ticks {
        let start = Instant::now();
        let stats = world.step(1. / 60.);
        samples.push(start.elapsed().as_secs_f64() * 1e6);
        impacts += stats.ccd_impacts;
        substeps += stats.substeps;
    }
    let mean = samples.iter().sum::<f64>() / ticks as f64;
    samples.sort_by(f64::total_cmp);
    println!(
        "{{\"bodies\":{bodies},\"ticks\":{ticks},\"mean_us\":{mean:.1},\"p50_us\":{:.1},\"p99_us\":{:.1},\"substeps\":{substeps},\"ccd_impacts\":{impacts}}}",
        samples[ticks / 2],
        samples[ticks * 99 / 100]
    );
}
#[cfg(not(feature = "dim3"))]
fn main() {}
