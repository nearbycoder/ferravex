//! Reproducible workload; timings describe this machine, not a performance promise.
use ferravex_dynamics::{BodyDesc, PhysicsWorld, Shape};
use std::time::Instant;
fn main() {
    let mut world = PhysicsWorld::<3>::new([0., -9.81, 0.]);
    world
        .insert(BodyDesc::fixed(
            Shape::cuboid([30., 0.5, 30.]),
            [0., -0.5, 0.],
        ))
        .unwrap();
    for i in 0..512 {
        world
            .insert(BodyDesc::dynamic(
                Shape::ball(0.45),
                [
                    (i % 32) as f32 * 1.2 - 18.,
                    1. + (i % 7) as f32 * 0.2,
                    (i / 32) as f32 * 1.2 - 9.,
                ],
                1.,
            ))
            .unwrap();
    }
    // Warm up allocations and settle the contacts before measuring.
    for _ in 0..240 {
        world.step(1. / 120.).unwrap();
    }
    let start = Instant::now();
    let mut pairs = 0;
    let mut contacts = 0;
    for _ in 0..1200 {
        let stats = world.step(1. / 120.).unwrap();
        pairs += stats.candidate_pairs;
        contacts += stats.contacts;
    }
    println!(
        "ferravex-dynamics: {} bodies, 1200 steps, {:.3} ms/step, {} pairs/step, {} contacts/step",
        world.len(),
        start.elapsed().as_secs_f64() * 1000. / 1200.,
        pairs / 1200,
        contacts / 1200
    );
}
