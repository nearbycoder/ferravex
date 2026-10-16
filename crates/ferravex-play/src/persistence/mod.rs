//! Versioned, bounded first-party save data. Checksums detect corruption, not tampering.
//! No filesystem, browser or game-specific dependencies.
const MAGIC: &[u8; 8] = b"FXSV0001";
pub const MAX_SAVE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_FIELDS: usize = 128;
const HEADER: usize = 36;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveError {
    InvalidData,
    UnsupportedVersion,
    WrongSchema,
    Capacity,
    DuplicateField,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    U64(u64),
    I64(i64),
    F64(f64),
    Text(String),
    Bytes(Vec<u8>),
}
impl Value {
    fn len(&self) -> usize {
        match self {
            Self::Bool(_) => 1,
            Self::U64(_) | Self::I64(_) | Self::F64(_) => 8,
            Self::Text(v) => v.len(),
            Self::Bytes(v) => v.len(),
        }
    }
    fn valid(&self) -> bool {
        match self {
            Self::F64(v) => v.is_finite(),
            Self::Text(v) => v.len() <= 65536,
            Self::Bytes(v) => v.len() <= MAX_SAVE_BYTES - 44,
            _ => true,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    schema: u64,
    version: u32,
    generation: u64,
    fields: Vec<(String, Value)>,
    payload_len: usize,
}
impl Document {
    pub fn new(schema: u64, version: u32, generation: u64) -> Result<Self, SaveError> {
        if schema == 0 || version == 0 {
            return Err(SaveError::InvalidData);
        }
        Ok(Self {
            schema,
            version,
            generation,
            fields: Vec::new(),
            payload_len: 0,
        })
    }
    pub fn schema(&self) -> u64 {
        self.schema
    }
    pub fn version(&self) -> u32 {
        self.version
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn fields(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.fields.iter().map(|(k, v)| (k.as_str(), v))
    }
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.fields
            .binary_search_by(|(name, _)| name.as_str().cmp(key))
            .ok()
            .map(|i| &self.fields[i].1)
    }
    /// Replacements validate before mutation. Keys sort canonically on disk.
    pub fn set(&mut self, key: &str, value: Value) -> Result<(), SaveError> {
        if !key_valid(key) || !value.valid() {
            return Err(SaveError::InvalidData);
        }
        let found = self
            .fields
            .binary_search_by(|(name, _)| name.as_str().cmp(key));
        if found.is_err() && self.fields.len() == MAX_FIELDS {
            return Err(SaveError::Capacity);
        }
        let old = found.as_ref().ok().map(|&index| &self.fields[index].1);
        let length = self.payload_len - old.map_or(0, |v| 6 + key.len() + v.len())
            + 6
            + key.len()
            + value.len();
        if length > MAX_SAVE_BYTES - HEADER - 8 {
            return Err(SaveError::Capacity);
        }
        match found {
            Ok(index) => self.fields[index].1 = value,
            Err(index) => self.fields.insert(index, (key.into(), value)),
        }
        self.payload_len = length;
        Ok(())
    }
    pub fn u64(&self, key: &str) -> Result<u64, SaveError> {
        match self.get(key) {
            Some(Value::U64(v)) => Ok(*v),
            _ => Err(SaveError::InvalidData),
        }
    }
    pub fn i64(&self, key: &str) -> Result<i64, SaveError> {
        match self.get(key) {
            Some(Value::I64(v)) => Ok(*v),
            _ => Err(SaveError::InvalidData),
        }
    }
    pub fn f64(&self, key: &str) -> Result<f64, SaveError> {
        match self.get(key) {
            Some(Value::F64(v)) => Ok(*v),
            _ => Err(SaveError::InvalidData),
        }
    }
    pub fn boolean(&self, key: &str) -> Result<bool, SaveError> {
        match self.get(key) {
            Some(Value::Bool(v)) => Ok(*v),
            _ => Err(SaveError::InvalidData),
        }
    }
    pub fn text(&self, key: &str) -> Result<&str, SaveError> {
        match self.get(key) {
            Some(Value::Text(v)) => Ok(v),
            _ => Err(SaveError::InvalidData),
        }
    }
    pub fn bytes(&self, key: &str) -> Result<&[u8], SaveError> {
        match self.get(key) {
            Some(Value::Bytes(v)) => Ok(v),
            _ => Err(SaveError::InvalidData),
        }
    }
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER + self.payload_len + 8);
        out.extend_from_slice(MAGIC);
        out.extend(self.schema.to_le_bytes());
        out.extend(self.version.to_le_bytes());
        out.extend(self.generation.to_le_bytes());
        out.extend((self.payload_len as u32).to_le_bytes());
        out.extend((self.fields.len() as u16).to_le_bytes());
        out.extend(0u16.to_le_bytes());
        for (key, value) in &self.fields {
            out.push(key.len() as u8);
            out.extend(key.as_bytes());
            out.push(match value {
                Value::Bool(_) => 0,
                Value::U64(_) => 1,
                Value::I64(_) => 2,
                Value::F64(_) => 3,
                Value::Text(_) => 4,
                Value::Bytes(_) => 5,
            });
            out.extend((value.len() as u32).to_le_bytes());
            match value {
                Value::Bool(v) => out.push(u8::from(*v)),
                Value::U64(v) => out.extend(v.to_le_bytes()),
                Value::I64(v) => out.extend(v.to_le_bytes()),
                Value::F64(v) => out.extend(v.to_le_bytes()),
                Value::Text(v) => out.extend(v.as_bytes()),
                Value::Bytes(v) => out.extend(v),
            }
        }
        out.extend(checksum(&out).to_le_bytes());
        out
    }
    /// Check the bounded envelope/checksum and every field before allocating values.
    pub fn decode(data: &[u8]) -> Result<Self, SaveError> {
        if data.len() < HEADER + 8 || data.len() > MAX_SAVE_BYTES {
            return Err(SaveError::Capacity);
        }
        if &data[..8] != MAGIC {
            return Err(SaveError::UnsupportedVersion);
        }
        let schema = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let version = u32::from_le_bytes(data[16..20].try_into().unwrap());
        let generation = u64::from_le_bytes(data[20..28].try_into().unwrap());
        let length = u32::from_le_bytes(data[28..32].try_into().unwrap()) as usize;
        let count = u16::from_le_bytes(data[32..34].try_into().unwrap()) as usize;
        if schema == 0
            || version == 0
            || count > MAX_FIELDS
            || data[34..36] != [0, 0]
            || length != data.len() - HEADER - 8
        {
            return Err(SaveError::InvalidData);
        }
        let end = data.len() - 8;
        if checksum(&data[..end]) != u64::from_le_bytes(data[end..].try_into().unwrap()) {
            return Err(SaveError::InvalidData);
        }
        validate_fields(&data[HEADER..end], count)?;
        let mut doc = Self::new(schema, version, generation)?;
        doc.fields.reserve_exact(count);
        let mut reader = Reader(&data[HEADER..end]);
        for _ in 0..count {
            let (key, tag, bytes) = reader.field()?;
            let value = match tag {
                0 => Value::Bool(bytes[0] != 0),
                1 => Value::U64(u64::from_le_bytes(bytes.try_into().unwrap())),
                2 => Value::I64(i64::from_le_bytes(bytes.try_into().unwrap())),
                3 => Value::F64(f64::from_le_bytes(bytes.try_into().unwrap())),
                4 => Value::Text(std::str::from_utf8(bytes).unwrap().into()),
                5 => Value::Bytes(bytes.into()),
                _ => unreachable!(),
            };
            doc.set(key, value)?;
        }
        Ok(doc)
    }
}
fn key_valid(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 64
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b))
}
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], SaveError> {
        if n > self.0.len() {
            return Err(SaveError::InvalidData);
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn field(&mut self) -> Result<(&'a str, u8, &'a [u8]), SaveError> {
        let n = self.take(1)?[0] as usize;
        let key = std::str::from_utf8(self.take(n)?).map_err(|_| SaveError::InvalidData)?;
        let tag = self.take(1)?[0];
        let n = u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as usize;
        Ok((key, tag, self.take(n)?))
    }
}
fn validate_fields(data: &[u8], count: usize) -> Result<(), SaveError> {
    let mut r = Reader(data);
    let mut previous = None;
    for _ in 0..count {
        let (key, tag, value) = r.field()?;
        if !key_valid(key) || previous.is_some_and(|p| p >= key) {
            return Err(SaveError::DuplicateField);
        }
        previous = Some(key);
        let valid = match tag {
            0 => value.len() == 1 && value[0] <= 1,
            1 | 2 => value.len() == 8,
            3 => value.len() == 8 && f64::from_le_bytes(value.try_into().unwrap()).is_finite(),
            4 => value.len() <= 65536 && std::str::from_utf8(value).is_ok(),
            5 => true,
            _ => false,
        };
        if !valid {
            return Err(SaveError::InvalidData);
        }
    }
    if !r.0.is_empty() {
        return Err(SaveError::InvalidData);
    }
    Ok(())
}
pub fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
/// Implement explicit older-version migration in read(); reject newer versions.
/// Decode into a temporary value and validate it before returning to the host.
pub trait SaveData: Sized {
    const SCHEMA: u64;
    const VERSION: u32;
    const MAX_BYTES: usize = MAX_SAVE_BYTES;
    fn write(&self, document: &mut Document) -> Result<(), SaveError>;
    fn read(document: &Document) -> Result<Self, SaveError>;
    fn encode(&self, generation: u64) -> Result<Vec<u8>, SaveError> {
        let mut d = Document::new(Self::SCHEMA, Self::VERSION, generation)?;
        self.write(&mut d)?;
        let bytes = d.encode();
        if bytes.len() > Self::MAX_BYTES {
            return Err(SaveError::Capacity);
        }
        Ok(bytes)
    }
    fn decode(data: &[u8]) -> Result<(Self, u64), SaveError> {
        if data.len() > Self::MAX_BYTES {
            return Err(SaveError::Capacity);
        }
        let d = Document::decode(data)?;
        if d.schema() != Self::SCHEMA {
            return Err(SaveError::WrongSchema);
        }
        if d.version() > Self::VERSION {
            return Err(SaveError::UnsupportedVersion);
        }
        Ok((Self::read(&d)?, d.generation()))
    }
}
#[cfg(test)]
mod tests;

mod preferences;
pub use preferences::Preferences;

mod records;
pub use records::{MAX_RECORDS, MAX_RUN_US, Record, RecordBook, run_micros};
