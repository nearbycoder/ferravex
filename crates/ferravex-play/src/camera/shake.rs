use super::{CameraError, valid_dt};
use ferravex_math::Vec3;
const CAPACITY: usize = 8;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShakeImpulse {
    /// Maximum local right/up/forward displacement.
    pub translation: Vec3,
    /// Maximum rotation about local right/up/forward, radians.
    pub rotation: Vec3,
    pub duration: f32,
    pub frequency: f32,
}
impl ShakeImpulse {
    fn valid(self) -> bool {
        self.translation.is_finite()
            && self.translation.min_element() >= 0.
            && self.translation.max_element() <= 10.
            && self.rotation.is_finite()
            && self.rotation.min_element() >= 0.
            && self.rotation.max_element() <= 0.5
            && self.duration.is_finite()
            && (0.001..=10.).contains(&self.duration)
            && self.frequency.is_finite()
            && (0.1..=100.).contains(&self.frequency)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShakeSample {
    pub translation: Vec3,
    pub rotation: Vec3,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Layer {
    impulse: ShakeImpulse,
    time: f64,
    seed: u64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraShake {
    layers: [Option<Layer>; CAPACITY],
    seed: u64,
    sequence: u64,
}
impl CameraShake {
    pub const fn new(seed: u64) -> Self {
        Self {
            layers: [None; CAPACITY],
            seed,
            sequence: 0,
        }
    }
    pub fn active(&self) -> usize {
        self.layers.iter().filter(|v| v.is_some()).count()
    }
    pub fn clear(&mut self) {
        self.layers.fill(None);
    }
    /// Capacity failures leave every active impulse and the seed sequence unchanged.
    pub fn push(&mut self, impulse: ShakeImpulse) -> Result<(), CameraError> {
        if !impulse.valid() {
            return Err(CameraError::InvalidInput);
        }
        let slot = self
            .layers
            .iter_mut()
            .find(|v| v.is_none())
            .ok_or(CameraError::ShakeCapacity)?;
        *slot = Some(Layer {
            impulse,
            time: 0.,
            seed: mix(self.seed.wrapping_add(self.sequence)),
        });
        self.sequence = self.sequence.wrapping_add(1);
        Ok(())
    }
    pub fn advance(&mut self, dt: f32) -> Result<ShakeSample, CameraError> {
        if !valid_dt(dt) {
            return Err(CameraError::InvalidInput);
        }
        let mut result = ShakeSample::default();
        for slot in &mut self.layers {
            let Some(layer) = slot else {
                continue;
            };
            layer.time += f64::from(dt);
            if layer.time >= f64::from(layer.impulse.duration) {
                *slot = None;
                continue;
            }
            let envelope = (1. - layer.time / layer.impulse.duration as f64).powi(2) as f32;
            let sample = |axis| {
                noise(
                    layer.seed.wrapping_add(axis),
                    layer.time * layer.impulse.frequency as f64,
                ) * envelope
            };
            result.translation +=
                layer.impulse.translation * Vec3::new(sample(0), sample(1), sample(2));
            result.rotation += layer.impulse.rotation * Vec3::new(sample(3), sample(4), sample(5));
        }
        Ok(result)
    }
}
fn mix(mut n: u64) -> u64 {
    n ^= n >> 30;
    n = n.wrapping_mul(0xbf58476d1ce4e5b9);
    n ^= n >> 27;
    n = n.wrapping_mul(0x94d049bb133111eb);
    n ^ (n >> 31)
}
fn noise(seed: u64, time: f64) -> f32 {
    let index = time.floor() as u64;
    let fraction = (time - index as f64) as f32;
    let t = fraction * fraction * (3. - 2. * fraction);
    let value = |i| ((mix(seed.wrapping_add(i)) >> 40) as f32 / (1_u32 << 24) as f32) * 2. - 1.;
    let a = value(index);
    a + (value(index + 1) - a) * t
}
