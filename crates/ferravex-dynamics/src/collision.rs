use crate::{Shape, math::*};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Manifold<const D: usize> {
    /// From the first shape toward the second.
    pub normal: [f32; D],
    pub penetration: f32,
}

pub(crate) fn contact<const D: usize>(
    a: Shape<D>,
    pa: [f32; D],
    b: Shape<D>,
    pb: [f32; D],
    margin: f32,
) -> Option<Manifold<D>> {
    match (a, b) {
        (Shape::Ball { radius: ra }, Shape::Ball { radius: rb }) => {
            let delta = sub(pb, pa);
            let distance = length(delta);
            if distance > ra + rb + margin {
                return None;
            }
            Some(Manifold {
                normal: normalize(delta).unwrap_or_else(|| unit(0, 1.)),
                penetration: (ra + rb - distance).max(0.),
            })
        }
        (Shape::Box { half_extents: ha }, Shape::Box { half_extents: hb }) => {
            let delta = sub(pb, pa);
            let mut penetration = f32::INFINITY;
            let mut axis = 0;
            for i in 0..D {
                let overlap = ha[i] + hb[i] - delta[i].abs();
                if overlap < -margin {
                    return None;
                }
                if overlap < penetration {
                    penetration = overlap;
                    axis = i;
                }
            }
            Some(Manifold {
                normal: unit(axis, if delta[axis] >= 0. { 1. } else { -1. }),
                penetration: penetration.max(0.),
            })
        }
        (Shape::Ball { radius }, Shape::Box { half_extents }) => {
            ball_box(pa, radius, pb, half_extents, margin)
        }
        (Shape::Box { half_extents }, Shape::Ball { radius }) => {
            ball_box(pb, radius, pa, half_extents, margin).map(|m| Manifold {
                normal: mul(m.normal, -1.),
                ..m
            })
        }
    }
}
fn ball_box<const D: usize>(
    p: [f32; D],
    radius: f32,
    center: [f32; D],
    half: [f32; D],
    margin: f32,
) -> Option<Manifold<D>> {
    let local = sub(p, center);
    let nearest = std::array::from_fn(|i| local[i].clamp(-half[i], half[i]));
    let toward_box = sub(nearest, local);
    let distance = length(toward_box);
    if distance > radius + margin {
        return None;
    }
    if distance > 1e-7 {
        return Some(Manifold {
            normal: mul(toward_box, distance.recip()),
            penetration: (radius - distance).max(0.),
        });
    }
    // A ball whose center is inside the box must exit through the closest face.
    let axis = (0..D)
        .min_by(|&a, &b| (half[a] - local[a].abs()).total_cmp(&(half[b] - local[b].abs())))
        .unwrap();
    Some(Manifold {
        normal: unit(axis, if local[axis] >= 0. { -1. } else { 1. }),
        penetration: radius + half[axis] - local[axis].abs(),
    })
}

/// Ray parameter measured in world units because callers normalize direction.
pub(crate) fn ray_ball<const D: usize>(
    origin: [f32; D],
    direction: [f32; D],
    center: [f32; D],
    radius: f32,
    max: f32,
) -> Option<(f32, [f32; D])> {
    let relative = sub(origin, center);
    let c = dot(relative, relative) - radius * radius;
    if c <= 0. {
        return Some((
            0.,
            normalize(relative).unwrap_or_else(|| mul(direction, -1.)),
        ));
    }
    let b = dot(relative, direction);
    if b >= 0. {
        return None;
    }
    let discriminant = b * b - c;
    if discriminant < 0. {
        return None;
    }
    // c / (-b + sqrt(d)) avoids subtracting nearly equal values for small hits.
    let t = c / (-b + discriminant.sqrt());
    if t > max {
        return None;
    }
    Some((
        t,
        normalize(add(relative, mul(direction, t))).unwrap_or_else(|| mul(direction, -1.)),
    ))
}

pub(crate) fn ray_box<const D: usize>(
    origin: [f32; D],
    direction: [f32; D],
    center: [f32; D],
    half: [f32; D],
    max: f32,
) -> Option<(f32, [f32; D])> {
    let p = sub(origin, center);
    let mut enter: f32 = 0.;
    let mut exit = max;
    let mut normal = mul(direction, -1.);
    for i in 0..D {
        if direction[i].abs() < 1e-8 {
            if p[i].abs() > half[i] {
                return None;
            }
            continue;
        }
        let t1 = (-half[i] - p[i]) / direction[i];
        let t2 = (half[i] - p[i]) / direction[i];
        let near = t1.min(t2);
        if near > enter {
            enter = near;
            normal = unit(i, -direction[i].signum());
        }
        exit = exit.min(t1.max(t2));
        if enter > exit {
            return None;
        }
    }
    Some((enter, normal))
}

/// Exact sphere/circle sweep against an AABB, including rounded corners/edges.
/// Each AABB feature contributes a face, edge-cylinder or corner-sphere root.
/// At most 26 features are examined in 3D, eight in 2D.
pub(crate) fn sweep_ball_box<const D: usize>(
    origin: [f32; D],
    radius: f32,
    direction: [f32; D],
    center: [f32; D],
    half: [f32; D],
    max: f32,
) -> Option<(f32, [f32; D])> {
    // Cheap conservative rejection; the actual result below does not treat the
    // rounded corners of a sphere cast as solid expanded-box corners.
    ray_box(origin, direction, center, half.map(|v| v + radius), max)?;
    if let Some(m) = ball_box(origin, radius, center, half, 0.) {
        let outward = mul(m.normal, -1.);
        if m.penetration > 1e-5 || dot(direction, outward) < -1e-6 {
            return Some((0., outward));
        }
    }
    let p = sub(origin, center);
    let mut best = max;
    let mut result = None;
    for axes in 1..(1_usize << D) {
        // Only the signs of clamped axes matter; avoid duplicate features.
        for signs in 0..(1_usize << D) {
            if signs & !axes != 0 {
                continue;
            }
            let mut a = 0.;
            let mut b = 0.;
            let mut c = -radius * radius;
            let mut feature = [0.; D];
            for i in 0..D {
                if axes & (1 << i) == 0 {
                    continue;
                }
                feature[i] = if signs & (1 << i) != 0 {
                    half[i]
                } else {
                    -half[i]
                };
                let q = p[i] - feature[i];
                a += direction[i] * direction[i];
                b += q * direction[i];
                c += q * q;
            }
            if a < 1e-12 {
                continue;
            }
            let disc = b * b - a * c;
            if disc < 0. {
                continue;
            }
            let t = (-b - disc.sqrt()) / a;
            if t < -1e-6 || t > best {
                continue;
            }
            let t = t.max(0.);
            let hit = add(p, mul(direction, t));
            let mut outward = [0.; D];
            let mut within = true;
            for i in 0..D {
                if axes & (1 << i) != 0 {
                    let sign = feature[i].signum();
                    if hit[i] * sign < half[i] - 1e-5 {
                        within = false;
                        break;
                    }
                    outward[i] = hit[i] - feature[i];
                } else if hit[i].abs() > half[i] + 1e-5 {
                    within = false;
                    break;
                }
            }
            if !within {
                continue;
            }
            let Some(normal) = normalize(outward) else {
                continue;
            };
            if dot(direction, normal) >= -1e-7 {
                continue;
            }
            best = t;
            result = Some((t, normal));
        }
    }
    result
}
