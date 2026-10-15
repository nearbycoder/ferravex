//! Small internal array arithmetic; no third-party math dependency.
pub(crate) fn add<const D: usize>(a: [f32; D], b: [f32; D]) -> [f32; D] {
    std::array::from_fn(|i| a[i] + b[i])
}
pub(crate) fn sub<const D: usize>(a: [f32; D], b: [f32; D]) -> [f32; D] {
    std::array::from_fn(|i| a[i] - b[i])
}
pub(crate) fn mul<const D: usize>(a: [f32; D], n: f32) -> [f32; D] {
    a.map(|v| v * n)
}
pub(crate) fn dot<const D: usize>(a: [f32; D], b: [f32; D]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
pub(crate) fn length<const D: usize>(a: [f32; D]) -> f32 {
    dot(a, a).sqrt()
}
pub(crate) fn unit<const D: usize>(axis: usize, sign: f32) -> [f32; D] {
    let mut result = [0.; D];
    result[axis] = sign;
    result
}
pub(crate) fn normalize<const D: usize>(a: [f32; D]) -> Option<[f32; D]> {
    let len = length(a);
    (len.is_finite() && len > 1e-8).then(|| mul(a, len.recip()))
}
pub(crate) fn valid<const D: usize>(v: [f32; D]) -> bool {
    v.iter().all(|v| v.is_finite() && v.abs() <= 1_000_000.)
}
