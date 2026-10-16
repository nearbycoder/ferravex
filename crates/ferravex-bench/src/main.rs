//! A repeatable local baseline, not a claim about other machines or engines.
use ferravex_fps::{Arena, Match};
use ferravex_net::InputFrame;
use std::{hint::black_box, time::Instant};

fn measure(name: &str, ticks: usize, mut step: impl FnMut()) {
    for _ in 0..256 {
        step();
    }
    let mut samples = Vec::with_capacity(ticks);
    let start = Instant::now();
    for _ in 0..ticks {
        let t = Instant::now();
        step();
        samples.push(t.elapsed().as_secs_f64() * 1_000_000.);
    }
    let elapsed = start.elapsed();
    samples.sort_by(f64::total_cmp);
    let mean = samples.iter().sum::<f64>() / ticks as f64;
    println!(
        "{name}: {ticks} ticks | mean {mean:.2} us | p50 {:.2} us | p99 {:.2} us | total {:.3} s",
        samples[ticks / 2],
        samples[(ticks * 99 / 100).min(ticks - 1)],
        elapsed.as_secs_f64()
    );
}
fn main() {
    let ticks = std::env::args()
        .nth(1)
        .and_then(|x| x.parse::<usize>().ok())
        .unwrap_or(4096)
        .clamp(100, 1_000_000);
    println!(
        "Ferravex baseline / {} / {} / warmed 256 ticks / rendering excluded",
        std::env::consts::ARCH,
        std::env::consts::OS
    );
    let mut game = Match::new(Arena::default());
    for id in 0..16 {
        game.spawn_player(id);
    }
    let mut inputs = Vec::with_capacity(16);
    measure("16-player authoritative FPS", ticks, || {
        inputs.clear();
        for id in 0..16 {
            inputs.push((
                id,
                InputFrame {
                    sequence: game.tick,
                    move_x: 0.5,
                    move_z: 0.6,
                    yaw: game.tick as f32 * 0.01 + id as f32,
                    pitch: 0.,
                    buttons: if game.tick % 128 < 64 {
                        ferravex_net::BUTTON_FIRE
                    } else {
                        ferravex_net::BUTTON_RELOAD
                    },
                },
            ));
        }
        black_box(game.step(&inputs));
    });
    // Snapshot compression is a server hot path independent of rendering.
    let base = [0_u8; 1000];
    let mut sparse = base;
    for byte in sparse.iter_mut().step_by(97) {
        *byte = 1;
    }
    for (name, next) in [
        ("unchanged 1000-byte snapshot delta", &base),
        ("sparse 1000-byte snapshot delta", &sparse),
        ("dense 1000-byte snapshot delta", &[1_u8; 1000]),
    ] {
        measure(name, ticks, || {
            black_box(
                ferravex_net::delta::Delta::between(0, black_box(&base), 1, black_box(next))
                    .unwrap(),
            );
        });
    }
    use ferravex_dynamics::{BodyDesc, PhysicsWorld, Shape};
    let mut p2 = PhysicsWorld::<2>::new([0., -9.81]);
    p2.insert(BodyDesc::fixed(Shape::cuboid([60., 0.5]), [0., -0.5]))
        .unwrap();
    for y in 0..16 {
        for x in 0..16 {
            p2.insert(BodyDesc::dynamic(
                Shape::cuboid([0.45; 2]),
                [x as f32 * 1.1 - 8., y as f32 * 1.1 + 1.],
                1.,
            ))
            .unwrap();
        }
    }
    measure("256 first-party 2D bodies", ticks, || {
        p2.step(1. / 64.).unwrap();
    });
    let mut p3 = PhysicsWorld::<3>::new([0., -9.81, 0.]);
    p3.insert(BodyDesc::fixed(
        Shape::cuboid([60., 0.5, 60.]),
        [0., -0.5, 0.],
    ))
    .unwrap();
    for y in 0..4 {
        for z in 0..8 {
            for x in 0..8 {
                p3.insert(BodyDesc::dynamic(
                    Shape::cuboid([0.45; 3]),
                    [
                        x as f32 * 1.2 - 4.,
                        y as f32 * 1.2 + 1.,
                        z as f32 * 1.2 - 4.,
                    ],
                    1.,
                ))
                .unwrap();
            }
        }
    }
    measure("256 first-party 3D bodies", ticks, || {
        p3.step(1. / 64.).unwrap();
    });
}
