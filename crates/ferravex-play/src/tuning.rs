//! Validated, versioned gameplay profiles. Loading is separate from applying;
//! hosts adopt a complete profile at a restart/checkpoint, never half an edit.
use crate::persistence::{Document, SaveData, SaveError, Value, checksum};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionTuning {
    pub acceleration: f32,
    pub braking: f32,
    pub max_speed: f32,
    /// Exponential input response, in inverse seconds.
    pub response: f32,
    /// Lateral velocity decay, in inverse seconds.
    pub traction: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Assists {
    pub steering: f32,
    pub aim: f32,
    pub jump_buffer_seconds: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TuningProfile {
    /// Host-authored rules identity. Different games must use different IDs.
    pub rules: u32,
    pub motion: MotionTuning,
    /// Host difficulty multiplier; never a graphics-quality setting.
    pub difficulty: f32,
    pub assists: Assists,
}
impl TuningProfile {
    pub const fn rally() -> Self {
        Self {
            rules: 1,
            motion: MotionTuning {
                acceleration: 155.,
                braking: 260.,
                max_speed: 265.,
                response: 14.,
                traction: 9.,
            },
            difficulty: 1.,
            assists: Assists {
                steering: 0.,
                aim: 0.,
                jump_buffer_seconds: 0.,
            },
        }
    }
    pub const fn arena() -> Self {
        Self {
            rules: 2,
            motion: MotionTuning {
                acceleration: 2300.,
                braking: 3000.,
                max_speed: 230.,
                response: 24.,
                traction: 15.,
            },
            ..Self::rally()
        }
    }
    pub fn validated(self) -> Result<Self, SaveError> {
        let m = self.motion;
        if self.rules == 0
            || ![
                m.acceleration,
                m.braking,
                m.max_speed,
                m.response,
                m.traction,
            ]
            .iter()
            .all(|v| v.is_finite() && (0.0001..=10000.).contains(v))
            || !self.difficulty.is_finite()
            || !(0.25..=4.).contains(&self.difficulty)
            || ![self.assists.steering, self.assists.aim]
                .iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(v))
            || !self.assists.jump_buffer_seconds.is_finite()
            || !(0. ..=0.5).contains(&self.assists.jump_buffer_seconds)
        {
            return Err(SaveError::InvalidData);
        }
        Ok(self)
    }
    /// Transactional replacement; reject profiles belonging to another ruleset.
    pub fn replace(&mut self, candidate: Self) -> Result<(), SaveError> {
        candidate.validated()?;
        if candidate.rules != self.rules {
            return Err(SaveError::InvalidData);
        }
        *self = candidate;
        Ok(())
    }
    fn bytes(self) -> [u8; 40] {
        let m = self.motion;
        let mut bytes = [0; 40];
        bytes[..4].copy_from_slice(&self.rules.to_le_bytes());
        for (out, value) in bytes[4..].as_chunks_mut::<4>().0.iter_mut().zip([
            m.acceleration,
            m.braking,
            m.max_speed,
            m.response,
            m.traction,
            self.difficulty,
            self.assists.steering,
            self.assists.aim,
            self.assists.jump_buffer_seconds,
        ]) {
            out.copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }
    /// Include in record/replay contexts so assisted or differently tuned runs
    /// cannot silently replace records made under different handling rules.
    pub fn fingerprint(self) -> Result<u64, SaveError> {
        self.validated()?;
        Ok(checksum(&self.bytes()))
    }
}
impl SaveData for TuningProfile {
    const SCHEMA: u64 = 0x465854554e494e47;
    const VERSION: u32 = 1;
    const MAX_BYTES: usize = 256;
    fn write(&self, d: &mut Document) -> Result<(), SaveError> {
        self.validated()?;
        d.set("profile", Value::Bytes(self.bytes().to_vec()))
    }
    fn read(d: &Document) -> Result<Self, SaveError> {
        let bytes = d.bytes("profile")?;
        if bytes.len() != 40 {
            return Err(SaveError::InvalidData);
        }
        let rules = u32::from_le_bytes(bytes[..4].try_into().unwrap());
        let mut values = [0.; 9];
        for (out, word) in values.iter_mut().zip(bytes[4..].as_chunks::<4>().0) {
            *out = f32::from_le_bytes(*word);
        }
        let [
            acceleration,
            braking,
            max_speed,
            response,
            traction,
            difficulty,
            steering,
            aim,
            jump_buffer_seconds,
        ] = values;
        Self {
            rules,
            motion: MotionTuning {
                acceleration,
                braking,
                max_speed,
                response,
                traction,
            },
            difficulty,
            assists: Assists {
                steering,
                aim,
                jump_buffer_seconds,
            },
        }
        .validated()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_roundtrip_rules_and_transactional_rejection() {
        let mut profile = TuningProfile::rally();
        let bytes = profile.encode(4).unwrap();
        assert_eq!(TuningProfile::decode(&bytes).unwrap(), (profile, 4));
        let original = profile;
        let mut invalid = profile;
        invalid.motion.max_speed = f32::NAN;
        assert!(profile.replace(invalid).is_err());
        assert_eq!(profile, original);
        assert!(profile.replace(TuningProfile::arena()).is_err());
        let mut assisted = profile;
        assisted.assists.steering = 0.5;
        assert_ne!(profile.fingerprint(), assisted.fingerprint());
    }
}
