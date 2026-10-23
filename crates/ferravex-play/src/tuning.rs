//! Validated, versioned gameplay profiles. Loading is separate from applying;
//! hosts adopt a complete profile at a restart/checkpoint, never half an edit.
//! Use [`TuningProfile::diff`] to inspect a candidate before replacing a profile.
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

/// A tunable scalar, in the order reported by [`TuningDiff::changes`].
/// Rules identity is not tunable: a diff rejects mismatched rulesets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TuningField {
    Acceleration,
    Braking,
    MaxSpeed,
    Response,
    Traction,
    Difficulty,
    Steering,
    Aim,
    JumpBufferSeconds,
}

impl TuningField {
    /// Profile field path suitable for a tool's change report.
    ///
    /// ```
    /// use ferravex_play::tuning::TuningField;
    /// assert_eq!(TuningField::Steering.name(), "assists.steering");
    /// ```
    pub const fn name(self) -> &'static str {
        match self {
            Self::Acceleration => "motion.acceleration",
            Self::Braking => "motion.braking",
            Self::MaxSpeed => "motion.max_speed",
            Self::Response => "motion.response",
            Self::Traction => "motion.traction",
            Self::Difficulty => "difficulty",
            Self::Steering => "assists.steering",
            Self::Aim => "assists.aim",
            Self::JumpBufferSeconds => "assists.jump_buffer_seconds",
        }
    }

    fn value(self, profile: &TuningProfile) -> f32 {
        match self {
            Self::Acceleration => profile.motion.acceleration,
            Self::Braking => profile.motion.braking,
            Self::MaxSpeed => profile.motion.max_speed,
            Self::Response => profile.motion.response,
            Self::Traction => profile.motion.traction,
            Self::Difficulty => profile.difficulty,
            Self::Steering => profile.assists.steering,
            Self::Aim => profile.assists.aim,
            Self::JumpBufferSeconds => profile.assists.jump_buffer_seconds,
        }
    }
}

/// A changed scalar from a validated [`TuningProfile::diff`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TuningChange {
    pub field: TuningField,
    /// Value in the current profile, preserving its exact float bits.
    pub before: f32,
    /// Value in the candidate profile, preserving its exact float bits.
    pub after: f32,
}

/// An immutable, validated preview owning snapshots of both profiles.
///
/// Created by [`TuningProfile::diff`]. Storage and iteration are bounded to two
/// profiles and nine scalar comparisons, with no heap allocation. Later edits
/// to the original profiles do not change this preview; diff again to review
/// those edits before calling [`TuningProfile::replace`].
#[derive(Clone, Copy, Debug)]
pub struct TuningDiff {
    before: TuningProfile,
    after: TuningProfile,
}

impl TuningDiff {
    /// Changed values in [`TuningField`] declaration order: motion, difficulty,
    /// then assists. Unchanged values are omitted.
    ///
    /// Comparison uses float bits, without a tolerance. In particular, changing
    /// an assist from `0.0` to `-0.0` is reported because its saved bytes and
    /// fingerprint change. No values are normalized. See [`TuningProfile::diff`]
    /// for a complete preview-and-replace example.
    pub fn changes(&self) -> impl Iterator<Item = TuningChange> + '_ {
        [
            TuningField::Acceleration,
            TuningField::Braking,
            TuningField::MaxSpeed,
            TuningField::Response,
            TuningField::Traction,
            TuningField::Difficulty,
            TuningField::Steering,
            TuningField::Aim,
            TuningField::JumpBufferSeconds,
        ]
        .into_iter()
        .filter_map(|field| {
            let before = field.value(&self.before);
            let after = field.value(&self.after);
            (before.to_bits() != after.to_bits()).then_some(TuningChange {
                field,
                before,
                after,
            })
        })
    }

    /// Whether both validated profiles have identical saved profile bytes.
    /// This compares values directly, rather than relying on checksum equality.
    pub fn is_empty(&self) -> bool {
        self.changes().next().is_none()
    }
}

/// Why a [`TuningProfile::diff`] could not produce a replacement preview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TuningDiffError {
    /// The current profile fails [`TuningProfile::validated`].
    InvalidCurrent,
    /// The candidate profile fails [`TuningProfile::validated`].
    InvalidCandidate,
    /// Both profiles are valid, but belong to different games/rulesets.
    RulesMismatch { current: u32, candidate: u32 },
}

impl std::fmt::Display for TuningDiffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCurrent => f.write_str("current tuning profile is invalid"),
            Self::InvalidCandidate => f.write_str("candidate tuning profile is invalid"),
            Self::RulesMismatch { current, candidate } => write!(
                f,
                "tuning rules mismatch: current {current}, candidate {candidate}"
            ),
        }
    }
}

impl std::error::Error for TuningDiffError {}

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
    /// Preview changes from this profile to `candidate`, without mutating either.
    ///
    /// Validates the current profile, then the candidate, then checks matching
    /// rules IDs. Errors identify the failed step in that order. Unlike
    /// [`Self::replace`], which may repair an invalid current profile, this method
    /// requires both sides to be valid so every reported value is in range.
    ///
    /// The preview owns copies; it neither applies changes nor schedules them.
    /// Hosts still choose an appropriate restart/checkpoint for replacement.
    ///
    /// ```
    /// use ferravex_play::tuning::{TuningChange, TuningField, TuningProfile};
    ///
    /// let mut current = TuningProfile::rally();
    /// let mut candidate = current;
    /// candidate.assists.steering = 0.5;
    /// let preview = current.diff(&candidate).unwrap();
    /// assert!(!preview.is_empty());
    /// assert_eq!(preview.changes().collect::<Vec<_>>(), vec![TuningChange {
    ///     field: TuningField::Steering,
    ///     before: 0.0,
    ///     after: 0.5,
    /// }]);
    /// assert_eq!(current.assists.steering, 0.0);
    /// // At a host-selected restart/checkpoint, after reviewing the changes:
    /// current.replace(candidate).unwrap();
    /// assert!(current.diff(&candidate).unwrap().is_empty());
    /// ```
    ///
    /// ```
    /// use ferravex_play::tuning::{TuningDiffError, TuningProfile};
    /// let current = TuningProfile::rally();
    /// assert!(matches!(current.diff(&TuningProfile::arena()),
    ///     Err(TuningDiffError::RulesMismatch { current: 1, candidate: 2 })));
    /// let mut invalid = current;
    /// invalid.assists.aim = f32::NAN;
    /// assert!(matches!(current.diff(&invalid), Err(TuningDiffError::InvalidCandidate)));
    /// ```
    pub fn diff(&self, candidate: &Self) -> Result<TuningDiff, TuningDiffError> {
        self.validated()
            .map_err(|_| TuningDiffError::InvalidCurrent)?;
        candidate
            .validated()
            .map_err(|_| TuningDiffError::InvalidCandidate)?;
        if self.rules != candidate.rules {
            return Err(TuningDiffError::RulesMismatch {
                current: self.rules,
                candidate: candidate.rules,
            });
        }
        Ok(TuningDiff {
            before: *self,
            after: *candidate,
        })
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

    // Field, tool label, rally value, inclusive lower bound, inclusive upper bound.
    const FIELDS: [(TuningField, &str, f32, f32, f32); 9] = [
        (
            TuningField::Acceleration,
            "motion.acceleration",
            155.,
            0.0001,
            10000.,
        ),
        (TuningField::Braking, "motion.braking", 260., 0.0001, 10000.),
        (
            TuningField::MaxSpeed,
            "motion.max_speed",
            265.,
            0.0001,
            10000.,
        ),
        (
            TuningField::Response,
            "motion.response",
            14.,
            0.0001,
            10000.,
        ),
        (TuningField::Traction, "motion.traction", 9., 0.0001, 10000.),
        (TuningField::Difficulty, "difficulty", 1., 0.25, 4.),
        (TuningField::Steering, "assists.steering", 0., 0., 1.),
        (TuningField::Aim, "assists.aim", 0., 0., 1.),
        (
            TuningField::JumpBufferSeconds,
            "assists.jump_buffer_seconds",
            0.,
            0.,
            0.5,
        ),
    ];

    fn field_mut(profile: &mut TuningProfile, field: TuningField) -> &mut f32 {
        match field {
            TuningField::Acceleration => &mut profile.motion.acceleration,
            TuningField::Braking => &mut profile.motion.braking,
            TuningField::MaxSpeed => &mut profile.motion.max_speed,
            TuningField::Response => &mut profile.motion.response,
            TuningField::Traction => &mut profile.motion.traction,
            TuningField::Difficulty => &mut profile.difficulty,
            TuningField::Steering => &mut profile.assists.steering,
            TuningField::Aim => &mut profile.assists.aim,
            TuningField::JumpBufferSeconds => &mut profile.assists.jump_buffer_seconds,
        }
    }

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

    #[test]
    fn identical_profiles_have_empty_previews() {
        for profile in [TuningProfile::rally(), TuningProfile::arena()] {
            let diff = profile.diff(&profile).unwrap();
            assert!(diff.is_empty());
            assert_eq!(diff.changes().count(), 0);
        }
    }

    #[test]
    fn every_field_has_an_inspectable_directional_change() {
        let current = TuningProfile::rally();
        for (field, label, before, _, after) in FIELDS {
            let mut candidate = current;
            *field_mut(&mut candidate, field) = after;
            let diff = current.diff(&candidate).unwrap();
            assert!(!diff.is_empty());
            assert_eq!(field.name(), label);
            assert_eq!(
                diff.changes().collect::<Vec<_>>(),
                [TuningChange {
                    field,
                    before,
                    after,
                }]
            );
            assert_eq!(
                candidate
                    .diff(&current)
                    .unwrap()
                    .changes()
                    .collect::<Vec<_>>(),
                [TuningChange {
                    field,
                    before: after,
                    after: before,
                }]
            );
        }
    }

    #[test]
    fn all_nine_changes_have_stable_order_and_iteration_can_repeat() {
        let current = TuningProfile::rally();
        let mut candidate = current;
        for (field, _, _, _, upper) in FIELDS {
            *field_mut(&mut candidate, field) = upper;
        }
        let diff = current.diff(&candidate).unwrap();
        let expected = FIELDS.map(|(field, _, before, _, after)| TuningChange {
            field,
            before,
            after,
        });
        assert_eq!(diff.changes().collect::<Vec<_>>(), expected);
        assert_eq!(diff.changes().collect::<Vec<_>>(), expected);
    }

    #[test]
    fn sparse_preview_does_not_mutate_inputs_and_survives_later_edits() {
        let mut current = TuningProfile::rally();
        let mut candidate = current;
        candidate.motion.response = 12.;
        candidate.difficulty = 2.;
        candidate.assists.aim = 0.25;
        let current_save = current.encode(7).unwrap();
        let candidate_save = candidate.encode(7).unwrap();
        let diff = current.diff(&candidate).unwrap();
        assert_eq!(current.encode(7).unwrap(), current_save);
        assert_eq!(candidate.encode(7).unwrap(), candidate_save);
        let expected = [
            TuningChange {
                field: TuningField::Response,
                before: 14.,
                after: 12.,
            },
            TuningChange {
                field: TuningField::Difficulty,
                before: 1.,
                after: 2.,
            },
            TuningChange {
                field: TuningField::Aim,
                before: 0.,
                after: 0.25,
            },
        ];
        assert_eq!(diff.changes().collect::<Vec<_>>(), expected);
        current.replace(candidate).unwrap();
        assert!(current.diff(&candidate).unwrap().is_empty());
        candidate.assists.aim = 0.75;
        assert_eq!(diff.changes().collect::<Vec<_>>(), expected);
        assert_eq!(current.diff(&candidate).unwrap().changes().count(), 1);
    }

    #[test]
    fn diff_validates_both_sides_at_every_scalar_boundary() {
        let current = TuningProfile::rally();
        for (field, _, _, lower, upper) in FIELDS {
            for value in [lower, upper] {
                let mut candidate = current;
                *field_mut(&mut candidate, field) = value;
                assert!(current.diff(&candidate).is_ok(), "{field:?} = {value}");
                assert!(candidate.diff(&current).is_ok(), "{field:?} = {value}");
            }
            for value in [
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
                lower.next_down(),
                upper.next_up(),
            ] {
                let mut invalid = current;
                *field_mut(&mut invalid, field) = value;
                let before = invalid.bytes();
                assert_eq!(
                    current.diff(&invalid).unwrap_err(),
                    TuningDiffError::InvalidCandidate,
                    "{field:?} = {value}"
                );
                assert_eq!(
                    invalid.diff(&current).unwrap_err(),
                    TuningDiffError::InvalidCurrent,
                    "{field:?} = {value}"
                );
                assert_eq!(invalid.bytes(), before);
            }
        }
    }

    #[test]
    fn rules_rejection_and_error_precedence_are_explicit() {
        let current = TuningProfile::rally();
        let candidate = TuningProfile::arena();
        let error = current.diff(&candidate).unwrap_err();
        assert_eq!(
            error,
            TuningDiffError::RulesMismatch {
                current: 1,
                candidate: 2
            }
        );
        assert_eq!(
            error.to_string(),
            "tuning rules mismatch: current 1, candidate 2"
        );
        let invalid = TuningProfile {
            rules: 0,
            ..current
        };
        assert_eq!(
            current.diff(&invalid).unwrap_err(),
            TuningDiffError::InvalidCandidate
        );
        assert_eq!(
            invalid.diff(&current).unwrap_err(),
            TuningDiffError::InvalidCurrent
        );
        assert_eq!(
            invalid.diff(&invalid).unwrap_err(),
            TuningDiffError::InvalidCurrent
        );
        let invalid_candidate = TuningProfile {
            difficulty: f32::NAN,
            ..candidate
        };
        assert_eq!(
            current.diff(&invalid_candidate).unwrap_err(),
            TuningDiffError::InvalidCandidate
        );
        assert_eq!(
            invalid.diff(&candidate).unwrap_err(),
            TuningDiffError::InvalidCurrent
        );
        assert_eq!(current, TuningProfile::rally());
        assert_eq!(candidate, TuningProfile::arena());
    }

    #[test]
    fn signed_zero_assists_are_reported_without_normalization() {
        let current = TuningProfile::rally();
        for field in [
            TuningField::Steering,
            TuningField::Aim,
            TuningField::JumpBufferSeconds,
        ] {
            let mut candidate = current;
            *field_mut(&mut candidate, field) = -0.;
            assert_eq!(current, candidate); // Numeric equality is insufficient here.
            assert_ne!(
                current.fingerprint().unwrap(),
                candidate.fingerprint().unwrap()
            );
            let diff = current.diff(&candidate).unwrap();
            assert!(!diff.is_empty());
            let changes: Vec<_> = diff.changes().collect();
            assert_eq!(changes.len(), 1);
            assert_eq!(changes[0].field, field);
            assert_eq!(changes[0].before.to_bits(), 0.0_f32.to_bits());
            assert_eq!(changes[0].after.to_bits(), (-0.0_f32).to_bits());
            let (decoded, _) = TuningProfile::decode(&candidate.encode(1).unwrap()).unwrap();
            assert_eq!(decoded.bytes(), candidate.bytes());
            assert!(decoded.diff(&candidate).unwrap().is_empty());
        }
    }

    #[test]
    fn adjacent_float_changes_are_not_hidden_by_a_tolerance() {
        let current = TuningProfile::rally();
        let mut candidate = current;
        candidate.motion.max_speed = current.motion.max_speed.next_up();
        let diff = current.diff(&candidate).unwrap();
        assert_eq!(diff.changes().count(), 1);
        let change = diff.changes().next().unwrap();
        assert_eq!(change.field, TuningField::MaxSpeed);
        assert_eq!(change.after.to_bits(), change.before.to_bits() + 1);
    }

    #[test]
    fn replace_can_still_repair_an_invalid_current_profile() {
        let candidate = TuningProfile::rally();
        let mut current = candidate;
        current.motion.acceleration = f32::NAN;
        assert_eq!(
            current.diff(&candidate).unwrap_err(),
            TuningDiffError::InvalidCurrent
        );
        current.replace(candidate).unwrap();
        assert_eq!(current, candidate);
    }

    #[test]
    fn version_one_save_and_fingerprints_remain_compatible() {
        // Legacy rally profile, save version 1, generation 4; fixed independently
        // of the production encoder to detect representation/fingerprint drift.
        let saved = [
            0x46, 0x58, 0x53, 0x56, 0x30, 0x30, 0x30, 0x31, 0x47, 0x4e, 0x49, 0x4e, 0x55, 0x54,
            0x58, 0x46, 0x01, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x35, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x07, 0x70, 0x72, 0x6f, 0x66, 0x69,
            0x6c, 0x65, 0x05, 0x28, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1b,
            0x43, 0x00, 0x00, 0x82, 0x43, 0x00, 0x80, 0x84, 0x43, 0x00, 0x00, 0x60, 0x41, 0x00,
            0x00, 0x10, 0x41, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x36, 0x35, 0xe0, 0x48, 0x8c, 0x3e, 0xf2, 0xdf,
        ];
        let profile = TuningProfile::rally();
        assert_eq!(TuningProfile::VERSION, 1);
        assert_eq!(TuningProfile::SCHEMA, 0x465854554e494e47);
        assert_eq!(TuningProfile::MAX_BYTES, 256);
        assert_eq!(profile.encode(4).unwrap(), saved);
        assert_eq!(TuningProfile::decode(&saved).unwrap(), (profile, 4));
        assert_eq!(profile.fingerprint().unwrap(), 0x8f2a5be84b60f391);
        assert_eq!(
            TuningProfile::arena().fingerprint().unwrap(),
            0x6c4660d1d0b6d881
        );
    }
}
