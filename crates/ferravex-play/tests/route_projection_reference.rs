//! `Route3::project` builds its result once for the winning segment. This pins
//! it bit-for-bit to the original evaluate-on-every-improvement formulation.
use ferravex_math::Vec3;
use ferravex_play::navigation::{Route3, RouteProjection};

fn reference(points: &[Vec3], closed: bool, length: f64, query: Vec3) -> RouteProjection {
    let lengths: Vec<f64> = std::iter::once(0.)
        .chain(points.windows(2).scan(0., |total, w| {
            *total += distance(w[0], w[1]);
            Some(*total)
        }))
        .collect();
    let canonical = |travel: f64| {
        if closed && length > 0. {
            travel % length
        } else {
            travel.min(length)
        }
    };
    let q = query.to_array().map(f64::from);
    let mut best = None;
    let mut best_squared = f64::INFINITY;
    for segment in 0..points.len() - 1 + usize::from(closed) {
        let a = points[segment].to_array().map(f64::from);
        let b = points[(segment + 1) % points.len()]
            .to_array()
            .map(f64::from);
        let delta: [f64; 3] = std::array::from_fn(|i| b[i] - a[i]);
        let squared_length = delta.iter().map(|v| v * v).sum::<f64>();
        if squared_length == 0. {
            continue;
        }
        let fraction =
            ((0..3).map(|i| (q[i] - a[i]) * delta[i]).sum::<f64>() / squared_length).clamp(0., 1.);
        let projected: [f64; 3] = std::array::from_fn(|i| a[i] * (1. - fraction) + b[i] * fraction);
        let squared = (0..3).map(|i| (q[i] - projected[i]).powi(2)).sum::<f64>();
        if squared < best_squared {
            let segment_length = squared_length.sqrt();
            let along = canonical(lengths[segment] + segment_length * fraction);
            best = Some(RouteProjection {
                position: Vec3::from_array(projected.map(|v| v as f32)),
                direction: Vec3::from_array(delta.map(|v| (v / segment_length) as f32)),
                segment,
                segment_fraction: fraction,
                distance: along,
                progress: along / length,
                distance_to_route: squared.sqrt(),
            });
            best_squared = squared;
        }
    }
    best.unwrap_or(RouteProjection {
        position: points[0],
        direction: Vec3::ZERO,
        segment: 0,
        segment_fraction: 0.,
        distance: 0.,
        progress: 0.,
        distance_to_route: distance(query, points[0]),
    })
}

fn distance(a: Vec3, b: Vec3) -> f64 {
    (0..3)
        .map(|i| (f64::from(b.to_array()[i]) - f64::from(a.to_array()[i])).powi(2))
        .sum::<f64>()
        .sqrt()
}

#[test]
fn deferred_projection_matches_reference_for_open_closed_and_degenerate_routes() {
    let mut seed = 0x2545_f491_u32;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        (seed % 2000) as f32 / 10. - 100.
    };
    let mut route = Route3::new(64).unwrap();
    for case in 0..96 {
        let count = 1 + case % 23;
        let mut points: Vec<Vec3> = (0..count).map(|_| Vec3::new(next(), next(), 0.)).collect();
        if case % 5 == 0 && count > 2 {
            points[1] = points[0]; // zero-length segment
        }
        if case % 7 == 0 {
            points.iter_mut().for_each(|p| p.z = p.x * 0.5);
        }
        let closed = case % 2 == 1;
        if closed {
            route.set_closed(&points).unwrap();
        } else {
            route.set(&points).unwrap();
        }
        for _ in 0..24 {
            let query = Vec3::new(next(), next(), next() * 0.1);
            let actual = route.project(query).unwrap().unwrap();
            let expected = reference(&points, closed, route.length(), query);
            assert_eq!(actual, expected, "case {case}, query {query:?}");
            assert_eq!(actual.distance.to_bits(), expected.distance.to_bits());
            assert_eq!(
                actual.distance_to_route.to_bits(),
                expected.distance_to_route.to_bits()
            );
        }
    }
}
