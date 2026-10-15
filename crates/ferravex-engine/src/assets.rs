//! Deterministic cooked bundles with bounded parsing and transactional hot reload.
//! The original RLE codec is intended for flat-color assets, not general-purpose compression.
//! Each bundle permits 4096 paths (240 bytes each) and 64 MiB of decoded payload.
//! Retained Arc readers keep old generations alive; the limit is per bundle, not global.
//! Checksums detect corruption and provide no authentication or encryption.
use crate::replay::checksum;
use std::{collections::BTreeMap, sync::Arc};
const MAGIC: &[u8; 8] = b"FXPK0001";
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_FILES: usize = 4096;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetError {
    Path,
    Limit,
    Format,
    Checksum,
}
#[derive(Clone, Default)]
pub struct Bundle {
    files: BTreeMap<String, Arc<[u8]>>,
    bytes: usize,
}
impl Bundle {
    pub fn insert(&mut self, path: &str, bytes: &[u8]) -> Result<(), AssetError> {
        if !valid_path(path) {
            return Err(AssetError::Path);
        }
        let old = self.files.get(path).map_or(0, |v| v.len());
        let total = (self.bytes - old)
            .checked_add(bytes.len())
            .ok_or(AssetError::Limit)?;
        if total > MAX_BYTES
            || self.files.len() >= MAX_FILES && old == 0 && !self.files.contains_key(path)
        {
            return Err(AssetError::Limit);
        }
        self.files.insert(path.into(), Arc::from(bytes));
        self.bytes = total;
        Ok(())
    }
    pub fn get(&self, path: &str) -> Option<Arc<[u8]>> {
        self.files.get(path).cloned()
    }
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
    pub fn encode(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend((self.files.len() as u32).to_le_bytes());
        for (name, raw) in &self.files {
            let mut stored_len = 0;
            let mut offset = 0;
            while offset < raw.len() {
                let mut count = 1;
                while count < 255
                    && offset + count < raw.len()
                    && raw[offset + count] == raw[offset]
                {
                    count += 1;
                }
                stored_len += 2;
                if stored_len >= raw.len() {
                    break;
                }
                offset += count;
            }
            let compressed = offset == raw.len() && stored_len < raw.len();
            let mut rle = Vec::new();
            if compressed {
                rle.reserve_exact(stored_len);
                let mut offset = 0;
                while offset < raw.len() {
                    let mut count = 1;
                    while count < 255
                        && offset + count < raw.len()
                        && raw[offset + count] == raw[offset]
                    {
                        count += 1;
                    }
                    rle.extend([count as u8, raw[offset]]);
                    offset += count;
                }
            }
            let stored: &[u8] = if compressed { &rle } else { raw };
            out.extend((name.len() as u16).to_le_bytes());
            out.extend(name.as_bytes());
            out.push(u8::from(compressed));
            out.extend((raw.len() as u32).to_le_bytes());
            out.extend((stored.len() as u32).to_le_bytes());
            out.extend(checksum(raw).to_le_bytes());
            out.extend(stored);
        }
        out.extend(checksum(&out).to_le_bytes());
        out
    }
    pub fn decode(data: &[u8]) -> Result<Self, AssetError> {
        if data.len() < 20 || data.len() > MAX_BYTES + MAX_FILES * 280 || &data[..8] != MAGIC {
            return Err(AssetError::Format);
        }
        let end = data.len() - 8;
        if checksum(&data[..end]) != u64::from_le_bytes(data[end..].try_into().unwrap()) {
            return Err(AssetError::Checksum);
        }
        let mut reader = Reader {
            bytes: &data[8..end],
            at: 0,
        };
        let count = reader.u32()? as usize;
        if count > MAX_FILES {
            return Err(AssetError::Limit);
        }
        let mut result = Self::default();
        for _ in 0..count {
            let len = reader.u16()? as usize;
            let name = std::str::from_utf8(reader.take(len)?).map_err(|_| AssetError::Path)?;
            if !valid_path(name) || result.files.contains_key(name) {
                return Err(AssetError::Path);
            }
            let mode = reader.take(1)?[0];
            let raw_len = reader.u32()? as usize;
            let stored_len = reader.u32()? as usize;
            let hash = reader.u64()?;
            if raw_len > MAX_BYTES - result.bytes || stored_len > MAX_BYTES {
                return Err(AssetError::Limit);
            }
            let stored = reader.take(stored_len)?;
            let raw: Arc<[u8]> = match mode {
                0 if raw_len == stored_len => {
                    if checksum(stored) != hash {
                        return Err(AssetError::Checksum);
                    }
                    Arc::from(stored)
                }
                1 if stored_len.is_multiple_of(2) => {
                    // Validate expansion completely before allocating the claimed output.
                    let mut expanded = 0_usize;
                    for pair in stored.chunks_exact(2) {
                        let run = usize::from(pair[0]);
                        if run == 0 {
                            return Err(AssetError::Format);
                        }
                        expanded = expanded.checked_add(run).ok_or(AssetError::Limit)?;
                        if expanded > raw_len {
                            return Err(AssetError::Format);
                        }
                    }
                    if expanded != raw_len {
                        return Err(AssetError::Format);
                    }
                    let mut raw = Vec::with_capacity(raw_len);
                    for pair in stored.chunks_exact(2) {
                        raw.resize(raw.len() + usize::from(pair[0]), pair[1]);
                    }
                    if checksum(&raw) != hash {
                        return Err(AssetError::Checksum);
                    }
                    Arc::from(raw)
                }
                _ => return Err(AssetError::Format),
            };
            result.bytes += raw.len();
            result.files.insert(name.into(), raw);
        }
        if reader.at != reader.bytes.len() {
            return Err(AssetError::Format);
        }
        Ok(result)
    }
}
fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 240
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        })
}
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], AssetError> {
        let end = self.at.checked_add(len).ok_or(AssetError::Format)?;
        let bytes = self.bytes.get(self.at..end).ok_or(AssetError::Format)?;
        self.at = end;
        Ok(bytes)
    }
    fn u16(&mut self) -> Result<u16, AssetError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, AssetError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, AssetError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
}
/// Existing readers retain their Arc until the next successful reload is adopted.
#[derive(Default)]
pub struct LiveBundle {
    current: Arc<Bundle>,
    generation: u64,
}
impl LiveBundle {
    pub fn current(&self) -> Arc<Bundle> {
        self.current.clone()
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn reload(&mut self, bytes: &[u8]) -> Result<(), AssetError> {
        let generation = self.generation.checked_add(1).ok_or(AssetError::Limit)?;
        let next = Bundle::decode(bytes)?;
        self.current = Arc::new(next);
        self.generation = generation;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deterministic_pack_roundtrip_compression_and_failed_reload_preserves_readers() {
        let mut a = Bundle::default();
        a.insert("textures/flat.rgba", &[37; 10000]).unwrap();
        a.insert("meta/version", b"v1").unwrap();
        let bytes = a.encode();
        assert!(bytes.len() < 300);
        let mut live = LiveBundle::default();
        live.reload(&bytes).unwrap();
        let old = live.current();
        assert_eq!(&*old.get("meta/version").unwrap(), b"v1");
        assert_eq!(Bundle::decode(&bytes).unwrap().encode(), bytes);
        let mut bad = bytes.clone();
        bad[20] ^= 1;
        assert!(live.reload(&bad).is_err());
        assert_eq!(live.generation(), 1);
        a.insert("meta/version", b"v2").unwrap();
        live.reload(&a.encode()).unwrap();
        assert_eq!(&*old.get("meta/version").unwrap(), b"v1");
        assert_eq!(&*live.current().get("meta/version").unwrap(), b"v2");
        for path in ["../escape", "/root", "a//b", "a/./b", "a\\b", ""] {
            assert_eq!(a.insert(path, b""), Err(AssetError::Path));
        }
        for end in 0..bytes.len() {
            assert!(Bundle::decode(&bytes[..end]).is_err());
        }
    }
}

#[cfg(test)]
mod malformed_tests {
    use super::*;
    fn pack(records: &[(&str, u8, u32, &[u8], u64)]) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend((records.len() as u32).to_le_bytes());
        for &(name, mode, raw_len, stored, hash) in records {
            bytes.extend((name.len() as u16).to_le_bytes());
            bytes.extend(name.as_bytes());
            bytes.push(mode);
            bytes.extend(raw_len.to_le_bytes());
            bytes.extend((stored.len() as u32).to_le_bytes());
            bytes.extend(hash.to_le_bytes());
            bytes.extend(stored);
        }
        bytes.extend(checksum(&bytes).to_le_bytes());
        bytes
    }
    #[test]
    fn checksum_valid_malformed_runs_are_rejected_before_expansion() {
        for (raw_len, stored) in [
            (2, &[3, 7][..]),
            (3, &[0, 7][..]),
            (3, &[2, 7][..]),
            (3, &[3][..]),
            (MAX_BYTES as u32, &[1, 7][..]),
        ] {
            let bytes = pack(&[("a", 1, raw_len, stored, 0)]);
            assert!(matches!(Bundle::decode(&bytes), Err(AssetError::Format)));
        }
        assert!(matches!(
            Bundle::decode(&pack(&[("a", 9, 0, &[], 0)])),
            Err(AssetError::Format)
        ));
    }
    #[test]
    fn rejects_duplicate_paths_trailing_bytes_and_inner_checksum() {
        let hash = checksum(b"a");
        assert!(matches!(
            Bundle::decode(&pack(&[("a", 0, 1, b"a", hash), ("a", 0, 1, b"a", hash)])),
            Err(AssetError::Path)
        ));
        assert!(matches!(
            Bundle::decode(&pack(&[("../a", 0, 1, b"a", hash)])),
            Err(AssetError::Path)
        ));
        assert!(matches!(
            Bundle::decode(&pack(&[("a", 0, 1, b"a", hash ^ 1)])),
            Err(AssetError::Checksum)
        ));
        let mut bytes = pack(&[]);
        bytes.truncate(bytes.len() - 8);
        bytes.push(0);
        bytes.extend(checksum(&bytes).to_le_bytes());
        assert!(matches!(Bundle::decode(&bytes), Err(AssetError::Format)));
    }
    #[test]
    fn raw_and_compressed_selection_roundtrip_and_file_cap() {
        let mut bundle = Bundle::default();
        bundle.insert("empty", b"").unwrap();
        bundle.insert("raw", b"abcdef").unwrap();
        bundle.insert("short", b"aaa").unwrap();
        bundle.insert("runs", &[4; 256]).unwrap();
        let bytes = bundle.encode();
        let decoded = Bundle::decode(&bytes).unwrap();
        assert_eq!(decoded.encode(), bytes);
        for i in bundle.files.len()..MAX_FILES {
            bundle.insert(&format!("file{i}"), b"").unwrap();
        }
        assert_eq!(bundle.insert("one-too-many", b""), Err(AssetError::Limit));
        assert!(bundle.insert("empty", b"replacement").is_ok());
    }
}
