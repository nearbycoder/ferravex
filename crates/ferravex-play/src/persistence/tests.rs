use super::*;
fn seal(bytes: &mut [u8]) {
    let end = bytes.len() - 8;
    let sum = checksum(&bytes[..end]);
    bytes[end..].copy_from_slice(&sum.to_le_bytes());
}
fn sample() -> Document {
    let mut d = Document::new(91, 1, 12).unwrap();
    d.set("count", Value::U64(u64::MAX)).unwrap();
    d.set("gain", Value::F64(0.5)).unwrap();
    d.set("title", Value::Text("Café 日".into())).unwrap();
    d.set("raw", Value::Bytes(vec![0, 255, 128])).unwrap();
    d.set("enabled", Value::Bool(true)).unwrap();
    d.set("offset", Value::I64(i64::MIN)).unwrap();
    d
}
#[test]
fn canonical_roundtrip_preserves_precision_and_rejects_truncation() {
    let d = sample();
    let bytes = d.encode();
    assert_eq!(Document::decode(&bytes).unwrap(), d);
    assert_eq!(Document::decode(&bytes).unwrap().u64("count"), Ok(u64::MAX));
    for end in 0..bytes.len() {
        assert!(Document::decode(&bytes[..end]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(Document::decode(&trailing).is_err());
    let mut reverse = Document::new(91, 1, 12).unwrap();
    for (k, v) in d.fields().collect::<Vec<_>>().into_iter().rev() {
        reverse.set(k, v.clone()).unwrap();
    }
    assert_eq!(reverse.encode(), bytes);
}
#[test]
fn bounded_mutations_are_transactional() {
    let mut d = Document::new(1, 1, 0).unwrap();
    d.set("gain", Value::F64(0.5)).unwrap();
    let old = d.clone();
    assert!(d.set("gain", Value::F64(f64::NAN)).is_err());
    assert!(d.set("../bad", Value::Bool(true)).is_err());
    assert_eq!(d, old);
    for i in 0..127 {
        d.set(&format!("x{i}"), Value::U64(0)).unwrap();
    }
    assert_eq!(
        d.set("overflow", Value::Bool(true)),
        Err(SaveError::Capacity)
    );
    d.set("gain", Value::Bool(false)).unwrap();
    let mut large = Document::new(1, 1, 0).unwrap();
    large
        .set("blob", Value::Bytes(vec![0; MAX_SAVE_BYTES - 54]))
        .unwrap();
    let previous = large.clone();
    assert_eq!(
        large.set("another", Value::Bool(true)),
        Err(SaveError::Capacity)
    );
    assert_eq!(large, previous);
}
#[test]
fn checksum_valid_forgery_still_requires_semantic_validation() {
    let mut d = Document::new(1, 1, 0).unwrap();
    d.set("a", Value::Bool(true)).unwrap();
    d.set("b", Value::F64(1.)).unwrap();
    let bytes = d.encode();
    for offset in [34, 38, 43] {
        let mut bad = bytes.clone();
        bad[offset] = 255;
        seal(&mut bad);
        assert!(Document::decode(&bad).is_err(), "{offset}");
    }
    let mut duplicate = bytes.clone();
    duplicate[45] = b'a';
    seal(&mut duplicate);
    assert!(Document::decode(&duplicate).is_err());
    let mut nan = bytes;
    let at = nan.len() - 16;
    nan[at..at + 8].copy_from_slice(&f64::NAN.to_le_bytes());
    seal(&mut nan);
    assert!(Document::decode(&nan).is_err());
}
#[derive(Debug, PartialEq)]
struct Settings {
    gain: f64,
    enabled: bool,
}
impl SaveData for Settings {
    const SCHEMA: u64 = 91;
    const VERSION: u32 = 2;
    fn write(&self, d: &mut Document) -> Result<(), SaveError> {
        if !(0. ..=1.).contains(&self.gain) {
            return Err(SaveError::InvalidData);
        }
        d.set("gain", Value::F64(self.gain))?;
        d.set("enabled", Value::Bool(self.enabled))
    }
    fn read(d: &Document) -> Result<Self, SaveError> {
        let value = Self {
            gain: d.f64("gain")?,
            enabled: if d.version() == 1 {
                true
            } else {
                d.boolean("enabled")?
            },
        };
        if !(0. ..=1.).contains(&value.gain) {
            return Err(SaveError::InvalidData);
        }
        Ok(value)
    }
}
#[test]
fn explicit_migration_future_versions_and_application_validation() {
    let mut old = Document::new(91, 1, 4).unwrap();
    old.set("gain", Value::F64(0.25)).unwrap();
    assert_eq!(
        Settings::decode(&old.encode()).unwrap(),
        (
            Settings {
                gain: 0.25,
                enabled: true
            },
            4
        )
    );
    assert_eq!(
        Settings::decode(&Document::new(91, 3, 4).unwrap().encode()),
        Err(SaveError::UnsupportedVersion)
    );
    assert_eq!(
        Settings::decode(&Document::new(92, 1, 4).unwrap().encode()),
        Err(SaveError::WrongSchema)
    );
    old.set("gain", Value::F64(2.)).unwrap();
    assert_eq!(Settings::decode(&old.encode()), Err(SaveError::InvalidData));
    let new = Settings {
        gain: 0.7,
        enabled: false,
    };
    assert_eq!(Settings::decode(&new.encode(5).unwrap()).unwrap(), (new, 5));
}
