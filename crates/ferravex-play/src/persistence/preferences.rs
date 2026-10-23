use super::{Document, SaveData, SaveError, Value};
/// Device-local presentation preferences, separate from gameplay control bindings.
///
/// Use [`Self::diff`] to extract choices relative to defaults and
/// [`Self::compose`] to resolve defaults, user choices and temporary overrides.
/// Only the resulting complete preferences use the existing version-5 save
/// format. Unknown document fields are still ignored on read and omitted when
/// encoding a new save; composition does not retain them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preferences {
    pub master: f32,
    pub buses: [f32; 4],
    pub muted: bool,
    pub quality: Option<u8>,
    pub look: u8,
    /// Browser framebuffer density: device, 0.75x, 1x, 1.5x, 2x, 3x.
    pub resolution: u8,
    /// Preset, off, 512 hard, 1024 soft.
    pub shadows: u8,
    /// Preset, off, 2, 4, 8 local lights.
    pub lights: u8,
    pub adaptive: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            master: 0.7,
            buses: [1.; 4],
            muted: false,
            quality: None,
            look: 0,
            resolution: 0,
            shadows: 0,
            lights: 0,
            adaptive: false,
        }
    }
}
impl Preferences {
    pub fn valid(self) -> bool {
        std::iter::once(self.master)
            .chain(self.buses)
            .all(|v| v.is_finite() && (0. ..=1.).contains(&v))
            && self.quality.is_none_or(|q| q <= 2)
            && self.look <= 3
            && self.resolution <= 5
            && self.shadows <= 3
            && self.lights <= 4
    }

    /// Resolve layers in order; the last supplied value for each field wins.
    ///
    /// Validates the base and each layer, including values hidden by later
    /// layers. Returns [`SaveError::InvalidData`] for any invalid input. Nothing
    /// is mutated on success or failure. Uses constant storage, with no heap
    /// allocation, and work proportional to the supplied slice length.
    ///
    /// Keep user choices separate from temporary layers when saving: the
    /// returned value contains all effective overrides, without provenance.
    ///
    /// ```
    /// use ferravex_play::persistence::{Preferences, SaveData};
    ///
    /// let defaults = Preferences::default();
    /// let user = defaults.diff(&Preferences { master: 0.4, ..defaults }).unwrap();
    /// let temporary = defaults.diff(&Preferences { muted: true, ..defaults }).unwrap();
    /// let effective = defaults.compose(&[user, temporary]).unwrap();
    /// assert_eq!(effective.master, 0.4);
    /// assert!(effective.muted);
    /// // Save only the user layer; the temporary mute is session-local.
    /// let saved = defaults.compose(&[user]).unwrap().encode(1).unwrap();
    /// assert!(!Preferences::decode(&saved).unwrap().0.muted);
    /// assert_eq!(defaults.compose(&[]).unwrap(), defaults);
    /// ```
    pub fn compose(self, layers: &[PreferenceOverrides]) -> Result<Self, SaveError> {
        if !self.valid() {
            return Err(SaveError::InvalidData);
        }
        let mut result = self;
        for layer in layers {
            if !layer.valid() {
                return Err(SaveError::InvalidData);
            }
            result = layer.apply_unchecked(result);
        }
        Ok(result)
    }

    /// Extract the minimal overrides that transform this base into `choices`.
    ///
    /// Validates both values and returns [`SaveError::InvalidData`] if either is
    /// invalid. Each differing field (including each individual bus) contains
    /// the chosen value; equal fields inherit. Gain comparison is exact numeric
    /// equality, so positive and negative zero compare equal. No allocation.
    ///
    /// A snapshot cannot reveal whether a value equal to the base was explicitly
    /// chosen. To pin such a value across future default changes, set its override
    /// to `Some(value)` explicitly. In particular, `Some(None)` resets quality to
    /// the game's default; an outer `None` inherits the lower layer's quality.
    ///
    /// ```
    /// use ferravex_play::persistence::Preferences;
    ///
    /// let defaults = Preferences { quality: Some(2), ..Preferences::default() };
    /// let choices = Preferences { quality: None, buses: [1., 1., 0.25, 1.], ..defaults };
    /// let diff = defaults.diff(&choices).unwrap();
    /// assert_eq!(diff.quality, Some(None));
    /// assert_eq!(diff.buses, [None, None, Some(0.25), None]);
    /// assert_eq!(diff.master, None);
    /// assert_eq!(defaults.compose(&[diff]).unwrap(), choices);
    /// ```
    pub fn diff(&self, choices: &Self) -> Result<PreferenceOverrides, SaveError> {
        if !self.valid() || !choices.valid() {
            return Err(SaveError::InvalidData);
        }
        Ok(PreferenceOverrides {
            master: (self.master != choices.master).then_some(choices.master),
            buses: std::array::from_fn(|i| {
                (self.buses[i] != choices.buses[i]).then_some(choices.buses[i])
            }),
            muted: (self.muted != choices.muted).then_some(choices.muted),
            quality: (self.quality != choices.quality).then_some(choices.quality),
            look: (self.look != choices.look).then_some(choices.look),
            resolution: (self.resolution != choices.resolution).then_some(choices.resolution),
            shadows: (self.shadows != choices.shadows).then_some(choices.shadows),
            lights: (self.lights != choices.lights).then_some(choices.lights),
            adaptive: (self.adaptive != choices.adaptive).then_some(choices.adaptive),
        })
    }
}

/// Sparse, device-local overrides of [`Preferences`], with one slot per field.
///
/// `None` inherits; `Some(value)` overrides even when the value is false, zero,
/// or equal to the defaults. [`Default`] creates an empty layer. Bus slots are
/// effects, ambience, music and UI, in that order. The quality field has three
/// states: `None` inherits, `Some(None)` clears a quality preference, and
/// `Some(Some(0..=2))` selects low/medium/high.
///
/// Public fields allow editing a draft; [`Self::valid`], [`Self::then`] and
/// [`Preferences::compose`] check the same ranges as [`Preferences::valid`].
/// This fixed-size type has no save codec; persist resolved [`Preferences`] to
/// preserve the existing schema. An equal-to-default explicit choice cannot be
/// recovered by diffing a saved snapshot.
///
/// ```
/// use ferravex_play::persistence::Preferences;
///
/// let defaults = Preferences::default();
/// let mut user = defaults.diff(&defaults).unwrap();
/// user.master = Some(defaults.master); // Explicitly pin this gain.
/// user.quality = Some(None); // Explicitly use the game's quality default.
/// let new_defaults = Preferences { master: 0.5, quality: Some(2), ..defaults };
/// let effective = new_defaults.compose(&[user]).unwrap();
/// assert_eq!(effective.master, defaults.master);
/// assert_eq!(effective.quality, None);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PreferenceOverrides {
    /// Finite gain in `0..=1`, or inherit.
    pub master: Option<f32>,
    /// Independent finite gains in `0..=1`: effects, ambience, music, UI.
    pub buses: [Option<f32>; 4],
    pub muted: Option<bool>,
    /// Inherit, clear, or choose a quality index in `0..=2`.
    pub quality: Option<Option<u8>>,
    /// Visual style index in `0..=3`, or inherit.
    pub look: Option<u8>,
    /// Framebuffer density index in `0..=5`, or inherit.
    pub resolution: Option<u8>,
    /// Shadow setting index in `0..=3`, or inherit.
    pub shadows: Option<u8>,
    /// Local-light setting index in `0..=4`, or inherit.
    pub lights: Option<u8>,
    pub adaptive: Option<bool>,
}

impl PreferenceOverrides {
    /// Whether every supplied value is valid; an empty layer is valid.
    ///
    /// ```
    /// use ferravex_play::persistence::Preferences;
    /// let defaults = Preferences::default();
    /// let mut layer = defaults.diff(&defaults).unwrap();
    /// assert!(layer.valid());
    /// layer.buses[2] = Some(f32::NAN);
    /// assert!(!layer.valid());
    /// ```
    pub fn valid(self) -> bool {
        self.apply_unchecked(Preferences::default()).valid()
    }

    /// Whether all fields inherit. Explicit false/zero/quality-clear values
    /// count as overrides, even if applying them would leave a base unchanged.
    ///
    /// ```
    /// use ferravex_play::persistence::Preferences;
    /// let defaults = Preferences::default();
    /// let mut layer = defaults.diff(&defaults).unwrap();
    /// assert!(layer.is_empty());
    /// layer.muted = Some(false);
    /// assert!(!layer.is_empty());
    /// ```
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }

    /// Combine two sparse layers, giving supplied values in `higher` priority.
    ///
    /// Inherited fields remain unset until applied to a base. Returns
    /// [`SaveError::InvalidData`] if either layer is invalid, even for values
    /// shadowed by `higher`. Neither input is mutated and no storage is allocated.
    ///
    /// ```
    /// use ferravex_play::persistence::Preferences;
    /// let defaults = Preferences::default();
    /// let user = defaults.diff(&Preferences { master: 0.2, muted: true, ..defaults }).unwrap();
    /// let mut temporary = defaults.diff(&defaults).unwrap();
    /// temporary.muted = Some(false); // Override an explicit mute.
    /// let merged = user.then(temporary).unwrap();
    /// assert_eq!(merged.master, Some(0.2));
    /// assert_eq!(merged.muted, Some(false));
    /// assert_eq!(merged.look, None);
    /// assert_eq!(defaults.compose(&[merged]), defaults.compose(&[user, temporary]));
    /// ```
    pub fn then(self, higher: Self) -> Result<Self, SaveError> {
        if !self.valid() || !higher.valid() {
            return Err(SaveError::InvalidData);
        }
        Ok(Self {
            master: higher.master.or(self.master),
            buses: std::array::from_fn(|i| higher.buses[i].or(self.buses[i])),
            muted: higher.muted.or(self.muted),
            quality: higher.quality.or(self.quality),
            look: higher.look.or(self.look),
            resolution: higher.resolution.or(self.resolution),
            shadows: higher.shadows.or(self.shadows),
            lights: higher.lights.or(self.lights),
            adaptive: higher.adaptive.or(self.adaptive),
        })
    }

    fn apply_unchecked(self, base: Preferences) -> Preferences {
        Preferences {
            master: self.master.unwrap_or(base.master),
            buses: std::array::from_fn(|i| self.buses[i].unwrap_or(base.buses[i])),
            muted: self.muted.unwrap_or(base.muted),
            quality: self.quality.unwrap_or(base.quality),
            look: self.look.unwrap_or(base.look),
            resolution: self.resolution.unwrap_or(base.resolution),
            shadows: self.shadows.unwrap_or(base.shadows),
            lights: self.lights.unwrap_or(base.lights),
            adaptive: self.adaptive.unwrap_or(base.adaptive),
        }
    }
}

impl SaveData for Preferences {
    const SCHEMA: u64 = 0x4658505245465331;
    const VERSION: u32 = 5;
    const MAX_BYTES: usize = 1024;
    fn write(&self, d: &mut Document) -> Result<(), SaveError> {
        if !self.valid() {
            return Err(SaveError::InvalidData);
        }
        d.set("master", Value::F64(self.master as f64))?;
        d.set("muted", Value::Bool(self.muted))?;
        d.set(
            "quality",
            Value::U64(self.quality.map_or(0, |v| v as u64 + 1)),
        )?;
        d.set("look", Value::U64(self.look as u64))?;
        d.set("resolution", Value::U64(self.resolution as u64))?;
        d.set("shadows", Value::U64(self.shadows as u64))?;
        d.set("lights", Value::U64(self.lights as u64))?;
        d.set("adaptive", Value::Bool(self.adaptive))?;
        for (key, v) in ["effects", "ambience", "music", "ui"]
            .into_iter()
            .zip(self.buses)
        {
            d.set(key, Value::F64(v as f64))?;
        }
        Ok(())
    }
    fn read(d: &Document) -> Result<Self, SaveError> {
        let quality = d.u64("quality")?;
        if quality > 3 {
            return Err(SaveError::InvalidData);
        }
        let mut p = Self {
            master: read_gain(d, "master")?,
            muted: d.boolean("muted")?,
            quality: quality.checked_sub(1).map(|q| q as u8),
            ..Default::default()
        };
        // Version 1 had master/mute/quality only; bus gains default to unity.
        if d.version() >= 2 {
            for (key, gain) in ["effects", "ambience", "music", "ui"]
                .into_iter()
                .zip(p.buses.iter_mut())
            {
                *gain = read_gain(d, key)?;
            }
        }
        if d.version() >= 3 {
            let look = d.u64("look")?;
            if look > 3 {
                return Err(SaveError::InvalidData);
            }
            p.look = look as u8;
        }
        if d.version() >= 4 {
            let resolution = d.u64("resolution")?;
            let shadows = d.u64("shadows")?;
            let lights = d.u64("lights")?;
            if resolution > 5 || shadows > 3 || lights > 4 {
                return Err(SaveError::InvalidData);
            }
            p.resolution = resolution as u8;
            p.shadows = shadows as u8;
            p.lights = lights as u8;
        }
        if d.version() >= 5 {
            p.adaptive = d.boolean("adaptive")?;
        }
        if !p.valid() {
            return Err(SaveError::InvalidData);
        }
        Ok(p)
    }
}
fn read_gain(d: &Document, key: &str) -> Result<f32, SaveError> {
    let v = d.f64(key)?;
    if !(0. ..=1.).contains(&v) {
        return Err(SaveError::InvalidData);
    }
    Ok(v as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn customized() -> Preferences {
        Preferences {
            master: 1.,
            buses: [0., 0.25, 0.5, 0.75],
            muted: true,
            quality: Some(2),
            look: 3,
            resolution: 5,
            shadows: 3,
            lights: 4,
            adaptive: true,
        }
    }

    #[test]
    fn layers_inherit_and_override_each_field_in_order() {
        let base = customized();
        let lower = PreferenceOverrides {
            master: Some(0.25),
            buses: [Some(1.), None, Some(0.25), None],
            quality: Some(Some(0)),
            look: Some(1),
            ..Default::default()
        };
        let higher = PreferenceOverrides {
            master: Some(0.),
            buses: [None, Some(1.), Some(0.), None],
            muted: Some(false),
            quality: Some(None),
            look: Some(0),
            resolution: Some(0),
            shadows: Some(0),
            lights: Some(0),
            adaptive: Some(false),
        };
        let empty = PreferenceOverrides::default();
        assert!(empty.valid());
        assert!(empty.is_empty());
        assert_eq!(base.compose(&[]), Ok(base));
        assert_eq!(base.compose(&[empty]), Ok(base));
        let layers = [empty, lower, empty, higher, empty];
        let before = layers;
        assert_eq!(
            base.compose(&layers),
            Ok(Preferences {
                master: 0.,
                buses: [1., 1., 0., 0.75],
                ..Default::default()
            })
        );
        assert_eq!(base, customized());
        assert_eq!(layers, before);
        let reversed = base.compose(&[higher, lower]).unwrap();
        assert_eq!(reversed.master, 0.25);
        assert_eq!(reversed.buses, [1., 1., 0.25, 0.75]);
        assert_eq!(reversed.quality, Some(0));
    }

    #[test]
    fn sparse_composition_preserves_explicit_choices_and_is_associative() {
        let base = Preferences::default();
        let a = base.diff(&customized()).unwrap();
        let b = PreferenceOverrides {
            quality: Some(None),
            muted: Some(false),
            buses: [None, Some(0.), None, None],
            ..Default::default()
        };
        let c = PreferenceOverrides {
            quality: Some(Some(1)),
            buses: [None, None, None, Some(1.)],
            adaptive: Some(false),
            ..Default::default()
        };
        let empty = PreferenceOverrides::default();
        assert_eq!(a.then(empty), Ok(a));
        assert_eq!(empty.then(a), Ok(a));
        assert_eq!(b.then(empty).unwrap().quality, Some(None));
        assert_eq!(b.then(empty).unwrap().muted, Some(false));
        assert_eq!(a.then(b).unwrap().then(c), a.then(b.then(c).unwrap()));
        assert_eq!(
            base.compose(&[a, b, c]),
            base.compose(&[a.then(b).unwrap().then(c).unwrap()])
        );
        // Equal-to-default explicit choices still pin a future base value.
        let pinned = PreferenceOverrides {
            master: Some(base.master),
            ..Default::default()
        };
        assert!(!pinned.is_empty());
        assert_eq!(customized().compose(&[pinned]).unwrap().master, base.master);
        assert!(
            base.diff(&base.compose(&[pinned]).unwrap())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn diffs_are_sparse_directional_and_reconstruct_every_field() {
        let defaults = Preferences::default();
        let chosen = customized();
        let changes = defaults.diff(&chosen).unwrap();
        assert_eq!(
            changes,
            PreferenceOverrides {
                master: Some(1.),
                buses: [Some(0.), Some(0.25), Some(0.5), Some(0.75)],
                muted: Some(true),
                quality: Some(Some(2)),
                look: Some(3),
                resolution: Some(5),
                shadows: Some(3),
                lights: Some(4),
                adaptive: Some(true),
            }
        );
        assert_eq!(defaults.compose(&[changes]), Ok(chosen));
        let reset = chosen.diff(&defaults).unwrap();
        assert_eq!(reset.quality, Some(None));
        assert_eq!(reset.muted, Some(false));
        assert_eq!(reset.look, Some(0));
        assert_eq!(reset.resolution, Some(0));
        assert_eq!(reset.shadows, Some(0));
        assert_eq!(reset.lights, Some(0));
        assert_eq!(reset.adaptive, Some(false));
        assert_eq!(chosen.compose(&[reset]), Ok(defaults));
        assert!(chosen.diff(&chosen).unwrap().is_empty());
        let mut one_bus = defaults;
        one_bus.buses[2] = 0.;
        assert_eq!(
            defaults.diff(&one_bus).unwrap(),
            PreferenceOverrides {
                buses: [None, None, Some(0.), None],
                ..Default::default()
            }
        );
    }

    #[test]
    fn gain_diff_uses_exact_numeric_equality() {
        let base = Preferences {
            master: 0.,
            buses: [0.5; 4],
            ..Default::default()
        };
        let mut chosen = base;
        chosen.master = -0.;
        assert!(base.diff(&chosen).unwrap().is_empty());
        chosen.buses[3] = f32::from_bits(0.5f32.to_bits() + 1);
        let diff = base.diff(&chosen).unwrap();
        assert_eq!(diff.buses, [None, None, None, Some(chosen.buses[3])]);
        assert_eq!(base.compose(&[diff]), Ok(chosen));
    }

    #[test]
    fn invalid_gains_are_rejected_even_when_overwritten() {
        let base = Preferences::default();
        let empty = PreferenceOverrides::default();
        let repair = base.diff(&customized()).unwrap();
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.01, 1.01] {
            for slot in 0..5 {
                let mut invalid_base = base;
                let mut invalid_layer = empty;
                if slot == 0 {
                    invalid_base.master = bad;
                    invalid_layer.master = Some(bad);
                } else {
                    invalid_base.buses[slot - 1] = bad;
                    invalid_layer.buses[slot - 1] = Some(bad);
                }
                assert!(!invalid_layer.valid());
                assert_eq!(invalid_base.compose(&[]), Err(SaveError::InvalidData));
                assert_eq!(invalid_base.compose(&[repair]), Err(SaveError::InvalidData));
                assert_eq!(base.diff(&invalid_base), Err(SaveError::InvalidData));
                assert_eq!(invalid_base.diff(&base), Err(SaveError::InvalidData));
                assert_eq!(
                    base.compose(&[repair, invalid_layer]),
                    Err(SaveError::InvalidData)
                );
                assert_eq!(
                    base.compose(&[invalid_layer, repair]),
                    Err(SaveError::InvalidData)
                );
                assert_eq!(invalid_layer.then(repair), Err(SaveError::InvalidData));
                assert_eq!(repair.then(invalid_layer), Err(SaveError::InvalidData));
            }
        }
    }

    #[test]
    fn index_limits_match_existing_validation_for_bases_and_layers() {
        let defaults = Preferences::default();
        let repair = defaults.diff(&customized()).unwrap();
        for (slot, limit) in [(0, 2), (1, 3), (2, 5), (3, 3), (4, 4)] {
            for value in 0..=u8::MAX {
                let mut base = defaults;
                let mut layer = PreferenceOverrides::default();
                match slot {
                    0 => {
                        base.quality = Some(value);
                        layer.quality = Some(Some(value));
                    }
                    1 => {
                        base.look = value;
                        layer.look = Some(value);
                    }
                    2 => {
                        base.resolution = value;
                        layer.resolution = Some(value);
                    }
                    3 => {
                        base.shadows = value;
                        layer.shadows = Some(value);
                    }
                    _ => {
                        base.lights = value;
                        layer.lights = Some(value);
                    }
                }
                let valid = value <= limit;
                assert_eq!(base.valid(), valid);
                assert_eq!(layer.valid(), valid);
                assert_eq!(defaults.compose(&[layer]).is_ok(), valid);
                assert_eq!(defaults.diff(&base).is_ok(), valid);
                if !valid {
                    assert_eq!(base.compose(&[repair]), Err(SaveError::InvalidData));
                    assert_eq!(base.diff(&defaults), Err(SaveError::InvalidData));
                    assert_eq!(
                        defaults.compose(&[layer, repair]),
                        Err(SaveError::InvalidData)
                    );
                    assert_eq!(layer.then(repair), Err(SaveError::InvalidData));
                    assert_eq!(repair.then(layer), Err(SaveError::InvalidData));
                }
            }
        }
    }

    #[test]
    fn composition_keeps_v5_encoding_and_unknown_field_policy() {
        assert_eq!(Preferences::SCHEMA, 0x4658505245465331);
        assert_eq!(Preferences::VERSION, 5);
        assert_eq!(Preferences::MAX_BYTES, 1024);
        let defaults = Preferences::default();
        let composed = defaults
            .compose(&[defaults.diff(&customized()).unwrap()])
            .unwrap();
        let mut document = Document::new(0x4658505245465331, 5, 9).unwrap();
        for (key, value) in [
            ("master", Value::F64(1.)),
            ("effects", Value::F64(0.)),
            ("ambience", Value::F64(0.25)),
            ("music", Value::F64(0.5)),
            ("ui", Value::F64(0.75)),
            ("muted", Value::Bool(true)),
            ("quality", Value::U64(3)),
            ("look", Value::U64(3)),
            ("resolution", Value::U64(5)),
            ("shadows", Value::U64(3)),
            ("lights", Value::U64(4)),
            ("adaptive", Value::Bool(true)),
        ] {
            document.set(key, value).unwrap();
        }
        assert_eq!(composed.encode(9).unwrap(), document.encode());
        document
            .set("extension", Value::Text("ignored".into()))
            .unwrap();
        let (loaded, generation) = Preferences::decode(&document.encode()).unwrap();
        assert_eq!((loaded, generation), (composed, 9));
        // Writing into an existing document leaves unrelated fields intact.
        loaded.write(&mut document).unwrap();
        assert_eq!(document.text("extension").unwrap(), "ignored");
        // Encoding a complete preference snapshot starts a fresh document.
        let saved = Document::decode(&loaded.encode(generation).unwrap()).unwrap();
        assert!(saved.get("extension").is_none());
        assert_eq!(saved.fields().count(), 12);
        let mut future = Document::new(Preferences::SCHEMA, 6, 0).unwrap();
        composed.write(&mut future).unwrap();
        assert_eq!(
            Preferences::decode(&future.encode()),
            Err(SaveError::UnsupportedVersion)
        );
    }

    #[test]
    fn preferences_migrate_and_reject_out_of_range_values() {
        let mut old = Document::new(Preferences::SCHEMA, 1, 7).unwrap();
        old.set("master", Value::F64(0.25)).unwrap();
        old.set("muted", Value::Bool(true)).unwrap();
        old.set("quality", Value::U64(2)).unwrap();
        let (p, generation) = Preferences::decode(&old.encode()).unwrap();
        assert_eq!(generation, 7);
        assert_eq!(p.buses, [1.; 4]);
        assert_eq!(p.quality, Some(1));
        assert_eq!(p.look, 0);
        assert_eq!((p.resolution, p.shadows, p.lights), (0, 0, 0));
        let mut v3 = Document::new(Preferences::SCHEMA, 3, 8).unwrap();
        v3.set("master", Value::F64(p.master as f64)).unwrap();
        v3.set("muted", Value::Bool(p.muted)).unwrap();
        v3.set("quality", Value::U64(2)).unwrap();
        v3.set("look", Value::U64(2)).unwrap();
        for key in ["effects", "ambience", "music", "ui"] {
            v3.set(key, Value::F64(1.)).unwrap();
        }
        let (migrated, _) = Preferences::decode(&v3.encode()).unwrap();
        assert_eq!(migrated, Preferences { look: 2, ..p });
        let mut v4 = v3;
        v4.version = 4;
        v4.set("resolution", Value::U64(5)).unwrap();
        v4.set("shadows", Value::U64(3)).unwrap();
        v4.set("lights", Value::U64(4)).unwrap();
        let (migrated, _) = Preferences::decode(&v4.encode()).unwrap();
        assert!(!migrated.adaptive);
        let detailed = Preferences {
            resolution: 5,
            shadows: 3,
            lights: 4,
            adaptive: true,
            ..p
        };
        assert_eq!(
            Preferences::decode(&detailed.encode(9).unwrap()).unwrap(),
            (detailed, 9)
        );
        for bad in [
            Preferences { resolution: 6, ..p },
            Preferences { shadows: 4, ..p },
            Preferences { lights: 5, ..p },
        ] {
            assert!(bad.encode(0).is_err());
        }
        let styled = Preferences { look: 3, ..p };
        assert_eq!(
            Preferences::decode(&styled.encode(9).unwrap()).unwrap(),
            (styled, 9)
        );
        assert!(Preferences { look: 4, ..p }.encode(0).is_err());
        assert!(p.muted);
        let bytes = p.encode(8).unwrap();
        assert_eq!(Preferences::decode(&bytes).unwrap(), (p, 8));
        old.set("master", Value::F64(1.5)).unwrap();
        assert!(Preferences::decode(&old.encode()).is_err());
        assert!(
            Preferences {
                master: f32::NAN,
                ..Default::default()
            }
            .encode(0)
            .is_err()
        );
    }
}
