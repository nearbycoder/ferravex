//! Authored world-space composition zones. Presentation offsets never alter the
//! tracked body or shooter aim; first-person input is passed through unchanged.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraZone {
    pub min: Vec3,
    pub max: Vec3,
    pub feather: f32,
    pub priority: i16,
    pub focus_offset: Vec3,
    pub boom_scale: f32,
    pub fov_scale: f32,
}
impl CameraZone {
    fn valid(self) -> bool {
        bounded(self.min)
            && bounded(self.max)
            && (self.max - self.min).min_element() > 0.
            && nonnegative(self.feather, 10000.)
            && bounded(self.focus_offset)
            && self.boom_scale.is_finite()
            && (0.5..=2.).contains(&self.boom_scale)
            && self.fov_scale.is_finite()
            && (0.5..=1.5).contains(&self.fov_scale)
    }
    fn weight(self, p: Vec3) -> f32 {
        let margin = (p - self.min).min(self.max - p).min_element();
        if margin < 0. {
            0.
        } else if self.feather == 0. {
            1.
        } else {
            (margin / self.feather).clamp(0., 1.)
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraComposer<const N: usize> {
    zones: [CameraZone; N],
    anticipation: f32,
    max_lead: f32,
    half_life: f32,
    offset: Vec3,
    boom: f32,
    fov: f32,
}
impl<const N: usize> CameraComposer<N> {
    pub fn new(
        zones: [CameraZone; N],
        anticipation: f32,
        max_lead: f32,
        half_life: f32,
    ) -> Result<Self, CameraError> {
        if N > 32
            || zones.iter().any(|z| !z.valid())
            || !nonnegative(anticipation, 1.)
            || !nonnegative(max_lead, 100.)
            || !nonnegative(half_life, 10.)
        {
            return Err(CameraError::InvalidSettings);
        }
        Ok(Self {
            zones,
            anticipation,
            max_lead,
            half_life,
            offset: Vec3::ZERO,
            boom: 1.,
            fov: 1.,
        })
    }
    pub fn clear(&mut self) {
        self.offset = Vec3::ZERO;
        self.boom = 1.;
        self.fov = 1.;
    }
    pub fn update(
        &mut self,
        mut input: Input3,
        acceleration: Vec3,
        dt: f32,
    ) -> Result<Input3, CameraError> {
        if !valid_dt(dt)
            || !bounded(input.target.position)
            || !bounded(input.target.velocity)
            || !bounded(acceleration)
            || !input.target.rotation.is_finite()
            || (input.target.rotation.length_squared() - 1.).abs() > 0.001
            || !input.fov_y.is_finite()
            || !(0.05..=3.1).contains(&input.fov_y)
        {
            return Err(CameraError::InvalidInput);
        }
        input.mode.desired(input.target)?;
        if matches!(input.mode, Mode3::FirstPerson { .. }) {
            return Ok(input);
        }
        let mut selected = None;
        let mut priority = i16::MIN;
        for zone in self.zones {
            let weight = zone.weight(input.target.position);
            if weight > 0. && (selected.is_none() || zone.priority > priority) {
                selected = Some((zone, weight));
                priority = zone.priority;
            }
        }
        let (focus, boom, fov) = selected.map_or((Vec3::ZERO, 1., 1.), |(z, w)| {
            (
                z.focus_offset * w,
                1. + (z.boom_scale - 1.) * w,
                1. + (z.fov_scale - 1.) * w,
            )
        });
        let lead = (input.target.velocity * self.anticipation
            + acceleration * (0.5 * self.anticipation * self.anticipation))
            .clamp_length_max(self.max_lead);
        let alpha = if self.half_life == 0. {
            1.
        } else {
            1. - (-std::f32::consts::LN_2 * dt / self.half_life).exp()
        };
        let mut next = *self;
        next.offset = next.offset.lerp(focus + lead, alpha);
        next.boom += (boom - next.boom) * alpha;
        next.fov += (fov - next.fov) * alpha;
        input.target.position += next.offset;
        input.fov_y = (input.fov_y * next.fov).clamp(0.05, 3.1);
        match &mut input.mode {
            Mode3::Follow {
                eye_offset,
                look_offset,
            } => *eye_offset = *look_offset + (*eye_offset - *look_offset) * next.boom,
            Mode3::Orbit { distance, .. } => *distance *= next.boom,
            Mode3::Chase {
                distance, height, ..
            } => {
                *distance *= next.boom;
                *height *= next.boom;
            }
            Mode3::FirstPerson { .. } => {}
        }
        input.mode.desired(input.target)?;
        if !bounded(input.target.position) {
            return Err(CameraError::InvalidInput);
        }
        *self = next;
        Ok(input)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> Input3 {
        Input3 {
            target: Target3 {
                velocity: Vec3::X * 20.,
                ..Target3::at(Vec3::ZERO)
            },
            mode: Mode3::Follow {
                eye_offset: Vec3::Y * 10.,
                look_offset: Vec3::ZERO,
            },
            fov_y: 1.,
        }
    }
    fn zone() -> CameraZone {
        CameraZone {
            min: Vec3::splat(-10.),
            max: Vec3::splat(10.),
            feather: 2.,
            priority: 1,
            focus_offset: Vec3::Z,
            boom_scale: 1.,
            fov_scale: 1.,
        }
    }
    #[test]
    fn anticipation_is_bounded_and_does_not_zoom() {
        let mut c = CameraComposer::new([zone()], 0.5, 2., 0.).unwrap();
        let i = c.update(input(), Vec3::ZERO, 0.1).unwrap();
        assert_eq!(i.target.position, Vec3::X * 2. + Vec3::Z);
        if let Mode3::Follow {
            eye_offset,
            look_offset,
        } = i.mode
        {
            assert_eq!(eye_offset.distance(look_offset), 10.);
        }
        let old = c;
        assert!(c.update(input(), Vec3::splat(f32::NAN), 0.1).is_err());
        assert_eq!(c, old);
    }
    #[test]
    fn priority_feather_and_first_person_are_explicit() {
        let mut high = zone();
        high.priority = 2;
        high.focus_offset = -Vec3::Z;
        let mut c = CameraComposer::new([zone(), high], 0., 0., 0.).unwrap();
        assert_eq!(
            c.update(input(), Vec3::ZERO, 0.1).unwrap().target.position,
            -Vec3::Z
        );
        let mut fp = input();
        fp.mode = Mode3::FirstPerson {
            eye_offset: Vec3::Y,
            yaw: 0.,
            pitch: 0.,
        };
        assert_eq!(c.update(fp, Vec3::ZERO, 0.1).unwrap(), fp);
    }
}
