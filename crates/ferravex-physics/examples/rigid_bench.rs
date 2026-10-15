#[cfg(feature = "dim3")]
fn main() {
    use ferravex_physics::d3::PhysicsWorld;
    use std::time::Instant;
    let mut world = PhysicsWorld::new([0., -9.81, 0.]);
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
    let start = Instant::now();
    let mut contacts = 0;
    for _ in 0..1200 {
        contacts = world.step(1. / 120.).contact_points;
    }
    println!(
        "original rigid solver: {} bodies; {} contact points; 1200 ticks; {:.3} ms/tick; {} sleeping",
        world.bodies.len(),
        contacts,
        start.elapsed().as_secs_f64() * 1000. / 1200.,
        world.last_step.sleeping_bodies
    );
}
#[cfg(not(feature = "dim3"))]
fn main() {
    eprintln!("enable the dim3 feature to run this benchmark");
}
