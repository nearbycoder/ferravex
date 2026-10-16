use super::*;
/// Device-local presentation preferences, separate from gameplay control bindings.
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
}
impl SaveData for Preferences {
    const SCHEMA: u64 = 0x4658505245465331;
    const VERSION: u32 = 4;
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
        let detailed = Preferences {
            resolution: 5,
            shadows: 3,
            lights: 4,
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
