use super::*;
/// Device-local presentation preferences, separate from gameplay control bindings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preferences {
    pub master: f32,
    pub buses: [f32; 4],
    pub muted: bool,
    pub quality: Option<u8>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            master: 0.7,
            buses: [1.; 4],
            muted: false,
            quality: None,
        }
    }
}
impl Preferences {
    pub fn valid(self) -> bool {
        std::iter::once(self.master)
            .chain(self.buses)
            .all(|v| v.is_finite() && (0. ..=1.).contains(&v))
            && self.quality.is_none_or(|q| q <= 2)
    }
}
impl SaveData for Preferences {
    const SCHEMA: u64 = 0x4658505245465331;
    const VERSION: u32 = 2;
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
