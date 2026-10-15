use super::{MAX_EFFECT_LIFETIME, RibbonPoint};
pub const MAX_BEAMS: usize = 4096;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeamDesc {
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub start_color: [f32; 4],
    pub end_color: [f32; 4],
    pub width: f32,
    pub lifetime: f32,
}
impl Default for BeamDesc {
    fn default() -> Self {
        Self {
            start: [0.; 3],
            end: [0., 0., 1.],
            start_color: [1.; 4],
            end_color: [1.; 4],
            width: 0.02,
            lifetime: 0.1,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeamError {
    InvalidBeam,
    InvalidCapacity,
    InvalidDelta,
    Full,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverflowPolicy {
    DropNewest,
    ReplaceOldest,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeamView {
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub start_color: [f32; 4],
    pub end_color: [f32; 4],
    pub width: f32,
    pub normalized_age: f32,
}
impl BeamView {
    pub fn points(self) -> [RibbonPoint; 2] {
        [
            RibbonPoint {
                position: self.start,
                color: self.start_color,
                width: self.width,
                break_before: true,
            },
            RibbonPoint {
                position: self.end,
                color: self.end_color,
                width: self.width,
                break_before: false,
            },
        ]
    }
}
struct Beam {
    desc: BeamDesc,
    age: f64,
    serial: u64,
}
pub struct BeamPool {
    beams: Vec<Beam>,
    limit: usize,
    policy: OverflowPolicy,
    serial: u64,
    dropped: u64,
    replaced: u64,
}
impl BeamPool {
    pub fn new(capacity: usize, policy: OverflowPolicy) -> Self {
        Self::try_new(capacity, policy).expect("beam capacity <= 4096")
    }
    pub fn try_new(capacity: usize, policy: OverflowPolicy) -> Result<Self, BeamError> {
        if capacity > MAX_BEAMS {
            return Err(BeamError::InvalidCapacity);
        }
        Ok(Self {
            beams: Vec::with_capacity(capacity),
            limit: capacity,
            policy,
            serial: 0,
            dropped: 0,
            replaced: 0,
        })
    }
    pub fn len(&self) -> usize {
        self.beams.len()
    }
    pub fn is_empty(&self) -> bool {
        self.beams.is_empty()
    }
    pub fn capacity(&self) -> usize {
        self.limit
    }
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
    pub fn replaced(&self) -> u64 {
        self.replaced
    }
    pub fn clear(&mut self) {
        self.beams.clear();
    }
    pub fn spawn(&mut self, desc: BeamDesc) -> Result<(), BeamError> {
        if !desc
            .start
            .into_iter()
            .chain(desc.end)
            .chain(desc.start_color)
            .chain(desc.end_color)
            .chain([desc.width, desc.lifetime])
            .all(f32::is_finite)
            || !(0.0..=1e6).contains(&desc.width)
            || !(0.0..=MAX_EFFECT_LIFETIME).contains(&desc.lifetime)
            || desc.lifetime == 0.
        {
            return Err(BeamError::InvalidBeam);
        }
        if self.limit == 0
            || self.beams.len() == self.limit && self.policy == OverflowPolicy::DropNewest
        {
            self.dropped = self.dropped.saturating_add(1);
            return Err(BeamError::Full);
        }
        let beam = Beam {
            desc,
            age: 0.,
            serial: self.serial,
        };
        self.serial = self.serial.saturating_add(1);
        if self.beams.len() == self.limit {
            let oldest = self
                .beams
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| {
                    a.age
                        .total_cmp(&b.age)
                        .then_with(|| b.serial.cmp(&a.serial))
                })
                .map(|(i, _)| i)
                .expect("nonempty full beam pool");
            self.beams[oldest] = beam;
            self.replaced = self.replaced.saturating_add(1);
        } else {
            self.beams.push(beam);
        }
        Ok(())
    }
    pub fn update(&mut self, dt: f32) -> Result<(), BeamError> {
        if !dt.is_finite() || dt < 0. {
            return Err(BeamError::InvalidDelta);
        }
        self.beams.retain_mut(|b| {
            b.age += f64::from(dt);
            b.age < f64::from(b.desc.lifetime)
        });
        Ok(())
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = BeamView> + '_ {
        self.beams.iter().map(|b| {
            let t = (b.age / f64::from(b.desc.lifetime)) as f32;
            let fade = |mut color: [f32; 4]| {
                color[3] *= 1. - t;
                color
            };
            BeamView {
                start: b.desc.start,
                end: b.desc.end,
                start_color: fade(b.desc.start_color),
                end_color: fade(b.desc.end_color),
                width: b.desc.width,
                normalized_age: t,
            }
        })
    }
}
