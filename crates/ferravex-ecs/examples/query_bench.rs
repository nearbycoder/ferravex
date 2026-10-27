//! Reproducible ECS query workload; timings describe this machine only.
//!
//! `cargo run --release -p ferravex-ecs --example query_bench`
use ferravex_ecs::{Entity, With, World};
use std::{hint::black_box, time::Instant};

struct Position([f32; 2]);
struct Velocity([f32; 2]);
struct Enemy(u32);
struct Pickup;

fn measure(name: &str, iterations: usize, mut run: impl FnMut()) {
    for _ in 0..16 {
        run();
    }
    let mut samples: Vec<f64> = (0..iterations)
        .map(|_| {
            let start = Instant::now();
            run();
            start.elapsed().as_secs_f64() * 1e6
        })
        .collect();
    samples.sort_by(f64::total_cmp);
    let mean = samples.iter().sum::<f64>() / iterations as f64;
    println!(
        "{name}: {iterations} passes | mean {mean:.2} us | p50 {:.2} us | p99 {:.2} us",
        samples[iterations / 2],
        samples[(iterations * 99 / 100).min(iterations - 1)]
    );
}

fn main() {
    let mut world = World::new();
    for i in 0..4096_u32 {
        let p = Position([i as f32, 0.]);
        let v = Velocity([1., 0.5]);
        match i % 4 {
            0 => drop(world.spawn((p, v, Enemy(i)))),
            1 => drop(world.spawn((p, Pickup))),
            _ => drop(world.spawn((p, v))),
        }
    }
    println!("ferravex-ecs query workload: 4096 entities, 4 component types");
    measure("(&mut Position, &Velocity)", 2000, || {
        for (mut p, v) in &mut world.query::<(&mut Position, &Velocity)>() {
            p.0[0] += v.0[0];
            p.0[1] += v.0[1];
        }
    });
    measure("(Entity, &Position, &Enemy)", 2000, || {
        let mut sum = 0_u64;
        for (e, p, enemy) in &mut world.query::<(Entity, &Position, &Enemy)>() {
            sum += u64::from(enemy.0) + p.0[0] as u64 + u64::from(e.id());
        }
        black_box(sum);
    });
    measure("&Position filtered With<Pickup>", 2000, || {
        black_box(
            world
                .query_filtered::<&Position, With<Pickup>>()
                .iter()
                .count(),
        );
    });
}
