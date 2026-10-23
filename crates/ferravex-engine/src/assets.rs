//! Deterministic cooked bundles with bounded parsing and transactional hot reload.
//! The original RLE codec is intended for flat-color assets, not general-purpose compression.
//! Each bundle permits 4096 paths (240 bytes each) and 64 MiB of decoded payload.
//! Retained Arc readers keep old generations alive; the limit is per bundle, not global.
//! Checksums detect corruption and provide no authentication or encryption.
//! Checked lookups normalize authoring paths; legacy operations and the wire format
//! continue to require canonical paths. Checked live updates validate before publication.
//! [`Bundle::manifest`] exposes sorted, borrowed metadata for tooling without
//! encoding a bundle or copying its payloads.
use crate::replay::checksum;
use std::{borrow::Cow, collections::BTreeMap, fmt, sync::Arc};
const MAGIC: &[u8; 8] = b"FXPK0001";
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_FILES: usize = 4096;
const MAX_PATH_BYTES: usize = 240;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetError {
    Path,
    Limit,
    Format,
    Checksum,
}
impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Path => "asset path must be relative, at most 240 ASCII bytes, with only letters, digits, '.', '_', '-' and '/' (no parent components)",
            Self::Limit => "asset bundle limit exceeded (4096 files, 64 MiB decoded payload, or generation counter exhausted)",
            Self::Format => "invalid or truncated FXPK0001 asset bundle",
            Self::Checksum => "asset bundle checksum mismatch",
        })
    }
}
impl std::error::Error for AssetError {}

/// Normalize a relative authoring path without consulting the filesystem.
///
/// Backslashes become slashes, empty and `.` components are removed, and case is
/// preserved. Absolute paths, drive prefixes, `..` components (even within the
/// root), non-ASCII names and normalized paths over 240 bytes are rejected.
/// Already canonical paths are borrowed; normalization allocates at most 240 bytes.
/// This is opt-in: [`Bundle::insert`] and [`Bundle::decode`] remain strict.
///
/// ```
/// use ferravex_engine::assets::{normalize_path, Bundle};
/// let mut bundle = Bundle::default();
/// let path = normalize_path(r".\textures\\flat.rgba")?;
/// assert_eq!(path, "textures/flat.rgba");
/// bundle.insert(&path, &[255, 0, 0, 255])?;
/// assert!(normalize_path("textures/../secret").is_err());
/// # Ok::<(), ferravex_engine::assets::AssetError>(())
/// ```
pub fn normalize_path(path: &str) -> Result<Cow<'_, str>, AssetError> {
    if valid_path(path) {
        return Ok(Cow::Borrowed(path));
    }
    if path.starts_with(['/', '\\']) {
        return Err(AssetError::Path);
    }
    let mut normalized = String::with_capacity(path.len().min(MAX_PATH_BYTES));
    for part in path.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        let separator = usize::from(!normalized.is_empty());
        if part == ".."
            || normalized.len() + separator > MAX_PATH_BYTES
            || part.len() > MAX_PATH_BYTES - normalized.len() - separator
            || !part
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            return Err(AssetError::Path);
        }
        if separator != 0 {
            normalized.push('/');
        }
        normalized.push_str(part);
    }
    if normalized.is_empty() {
        Err(AssetError::Path)
    } else {
        Ok(Cow::Owned(normalized))
    }
}

/// A checked lookup failure, with the requested path or canonical asset name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetLookupError {
    InvalidPath {
        path: String,
    },
    Missing {
        path: String,
    },
    InvalidUtf8 {
        path: String,
        error: std::str::Utf8Error,
    },
    WrongLength {
        path: String,
        expected: usize,
        actual: usize,
    },
}
impl fmt::Display for AssetLookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath { path } => {
                write!(f, "invalid asset path {path:?}: {}", AssetError::Path)
            }
            Self::Missing { path } => write!(f, "required asset {path:?} is missing"),
            Self::InvalidUtf8 { path, error } => {
                write!(f, "asset {path:?} is not UTF-8 text: {error}")
            }
            Self::WrongLength {
                path,
                expected,
                actual,
            } => write!(
                f,
                "asset {path:?} requires {expected} bytes, found {actual}"
            ),
        }
    }
}
impl std::error::Error for AssetLookupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidPath { .. } => Some(&AssetError::Path),
            Self::InvalidUtf8 { error, .. } => Some(error),
            _ => None,
        }
    }
}

/// The first failed edit. Earlier edits in the transaction were rolled back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetEditError {
    /// Zero-based position in the edit iterator.
    pub index: usize,
    /// Original authoring path, before normalization.
    pub path: String,
    pub error: AssetError,
}
impl fmt::Display for AssetEditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "asset edit {} at {:?} failed: {}",
            self.index, self.path, self.error
        )
    }
}
impl std::error::Error for AssetEditError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// A rejected live update. The active bundle and generation remain unchanged.
/// `E` is the application's validation error, often [`AssetLookupError`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleUpdateError<E> {
    Decode(AssetError),
    Edit(AssetEditError),
    Validation(E),
    GenerationExhausted,
}
impl<E: fmt::Display> fmt::Display for BundleUpdateError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(error) => write!(f, "asset reload could not decode bundle: {error}"),
            Self::Edit(error) => fmt::Display::fmt(error, f),
            Self::Validation(error) => write!(f, "asset update rejected by validation: {error}"),
            Self::GenerationExhausted => {
                f.write_str("asset update would overflow the generation counter")
            }
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for BundleUpdateError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(error) => Some(error),
            Self::Edit(error) => Some(error),
            Self::Validation(error) => Some(error),
            Self::GenerationExhausted => None,
        }
    }
}
/// Sorted content changes from one bundle generation to the next.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct BundleDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
}

/// Read-only metadata for one asset, produced by [`Bundle::manifest`].
///
/// The path borrows the bundle; the other fields are values. No payload is copied
/// or retained. Changing a returned entry cannot change the bundle.
///
/// ```
/// use ferravex_engine::assets::{AssetManifestEntry, Bundle};
/// let mut bundle = Bundle::default();
/// bundle.insert("meta/title", b"hello")?;
/// let entry: AssetManifestEntry<'_> = bundle.manifest().next().unwrap();
/// assert_eq!(entry.path, "meta/title");
/// assert_eq!(entry.byte_len, 5);
/// assert_eq!(entry.fingerprint, 0xa430d84680aabd0b);
/// # Ok::<(), ferravex_engine::assets::AssetError>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetManifestEntry<'a> {
    /// Canonical relative ASCII path with `/` separators and case preserved.
    pub path: &'a str,
    /// Decoded content length in bytes, including zero for an empty asset.
    pub byte_len: usize,
    /// Stable 64-bit FNV-1a of decoded bytes only, matching the FXPK0001 content
    /// checksum: start at `0xcbf29ce484222325`, then XOR each byte and multiply
    /// by `0x100000001b3` modulo 2^64. Independent of path, compression, insertion
    /// order, platform byte order and reload generation. This is a diagnostic
    /// fingerprint, not authentication or proof of equality; collisions are
    /// possible. Bundle diffs and no-op detection still compare exact content.
    pub fingerprint: u64,
}

#[derive(Clone, Default)]
pub struct Bundle {
    files: BTreeMap<String, Arc<[u8]>>,
    bytes: usize,
}
impl Bundle {
    /// Insert or replace content at a canonical path. Identical content keeps
    /// the existing payload Arc without allocating a replacement.
    ///
    /// ```
    /// use ferravex_engine::assets::Bundle;
    /// let mut bundle = Bundle::default();
    /// bundle.insert("meta/version", b"v1")?;
    /// let reader = bundle.get("meta/version").unwrap();
    /// bundle.insert("meta/version", b"v1")?;
    /// assert!(std::sync::Arc::ptr_eq(&reader, &bundle.get("meta/version").unwrap()));
    /// # Ok::<(), ferravex_engine::assets::AssetError>(())
    /// ```
    pub fn insert(&mut self, path: &str, bytes: &[u8]) -> Result<(), AssetError> {
        if !valid_path(path) {
            return Err(AssetError::Path);
        }
        if self.files.get(path).is_some_and(|old| &**old == bytes) {
            return Ok(());
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
    /// Number of assets, including empty files.
    pub fn len(&self) -> usize {
        self.files.len()
    }
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
    /// Total decoded payload bytes, excluding names and externally retained readers.
    pub fn decoded_bytes(&self) -> usize {
        self.bytes
    }
    /// Remove an asset. Existing Arc readers remain valid.
    pub fn remove(&mut self, path: &str) -> Option<Arc<[u8]>> {
        let bytes = self.files.remove(path)?;
        self.bytes -= bytes.len();
        Some(bytes)
    }
    /// Apply puts (`Some(bytes)`) and removals (`None`) in order, committing only
    /// if every path and intermediate capacity is valid. Payloads are Arc-shared.
    /// Assets with unchanged final content retain their original payload Arcs,
    /// including removal/reinsertion or edits reverted within this transaction.
    pub fn apply_edits<'a>(
        &mut self,
        edits: impl IntoIterator<Item = (&'a str, Option<&'a [u8]>)>,
    ) -> Result<(), AssetError> {
        *self = self
            .edited_with_paths(edits, false)
            .map_err(|error| error.error)?;
        Ok(())
    }
    /// Apply a transaction using [`normalize_path`], reporting the first failing
    /// edit's index, original path and cause. Limits apply after each edit, just
    /// as with [`Self::apply_edits`]. Aliases of a path are applied in input order;
    /// removing an absent asset is a successful no-op.
    /// Assets with unchanged final content retain their original payload Arcs.
    ///
    /// ```
    /// use ferravex_engine::assets::Bundle;
    /// let mut bundle = Bundle::default();
    /// bundle.apply_edits_checked([(r".\meta\version", Some(&b"v1"[..]))])?;
    /// let before = bundle.encode();
    /// let error = bundle.apply_edits_checked([
    ///     ("meta/version", None), ("../escape", None),
    /// ]).unwrap_err();
    /// assert_eq!(error.index, 1);
    /// assert_eq!(bundle.encode(), before);
    /// # Ok::<(), ferravex_engine::assets::AssetEditError>(())
    /// ```
    pub fn apply_edits_checked<'a>(
        &mut self,
        edits: impl IntoIterator<Item = (&'a str, Option<&'a [u8]>)>,
    ) -> Result<(), AssetEditError> {
        *self = self.edited_with_paths(edits, true)?;
        Ok(())
    }
    fn edited_with_paths<'a>(
        &self,
        edits: impl IntoIterator<Item = (&'a str, Option<&'a [u8]>)>,
        normalize: bool,
    ) -> Result<Self, AssetEditError> {
        let mut next = self.clone();
        for (index, (path, value)) in edits.into_iter().enumerate() {
            let diagnostic = |error| AssetEditError {
                index,
                path: path.into(),
                error,
            };
            let path = if normalize {
                normalize_path(path)
            } else if valid_path(path) {
                Ok(Cow::Borrowed(path))
            } else {
                Err(AssetError::Path)
            }
            .map_err(diagnostic)?;
            match value {
                Some(bytes) => next.insert(&path, bytes).map_err(diagnostic)?,
                None => {
                    next.remove(&path);
                }
            }
        }
        // Reuse readers for net no-ops, even when an earlier edit removed or
        // replaced the asset. Keep every intermediate validation/limit above.
        for (path, bytes) in &mut next.files {
            if let Some(old) = self.files.get(path)
                && !Arc::ptr_eq(old, bytes)
                && old == bytes
            {
                *bytes = old.clone();
            }
        }
        Ok(next)
    }
    /// Copy a directory's contents with the directory prefix stripped.
    /// Empty prefix selects the complete bundle. Payload allocations are shared.
    pub fn subtree(&self, directory: &str) -> Result<Self, AssetError> {
        if directory.is_empty() {
            return Ok(self.clone());
        }
        if !valid_path(directory) {
            return Err(AssetError::Path);
        }
        let prefix = format!("{directory}/");
        let mut result = Self::default();
        for (name, bytes) in self.files.range(prefix.clone()..) {
            let Some(relative) = name.strip_prefix(&prefix) else {
                break;
            };
            result.bytes += bytes.len();
            result.files.insert(relative.into(), bytes.clone());
        }
        Ok(result)
    }
    /// Compare decoded content, ignoring Arc identity and encoding choices.
    pub fn diff(&self, next: &Self) -> BundleDiff {
        let mut diff = BundleDiff::default();
        for (name, old) in &self.files {
            match next.files.get(name) {
                None => diff.removed.push(name.clone()),
                Some(new) if old != new => diff.changed.push(name.clone()),
                _ => {}
            }
        }
        diff.added.extend(
            next.files
                .keys()
                .filter(|name| !self.files.contains_key(*name))
                .cloned(),
        );
        diff
    }
    pub fn get(&self, path: &str) -> Option<Arc<[u8]>> {
        self.files.get(path).cloned()
    }
    /// Retrieve shared bytes using a normalized authoring path. Distinguishes
    /// invalid paths from missing assets. The reader survives edits and reloads.
    /// Successful canonical lookups allocate no memory and copy no payloads.
    ///
    /// ```
    /// use ferravex_engine::assets::{AssetLookupError, Bundle};
    /// let mut bundle = Bundle::default();
    /// bundle.insert("meta/version", b"v1")?;
    /// let reader = bundle.get_checked(r".\meta\version")?;
    /// bundle.remove("meta/version");
    /// assert_eq!(&*reader, b"v1");
    /// assert!(matches!(bundle.get_checked("meta/version"),
    ///     Err(AssetLookupError::Missing { .. })));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn get_checked(&self, path: &str) -> Result<Arc<[u8]>, AssetLookupError> {
        self.lookup(path).map(|(_, bytes)| bytes.clone())
    }
    /// Borrow validated UTF-8 text. Invalid text reports its canonical path and
    /// [`std::str::Utf8Error`], including the first invalid byte's offset.
    ///
    /// ```
    /// use ferravex_engine::assets::Bundle;
    /// let mut bundle = Bundle::default();
    /// bundle.insert("dialogue/greeting", "Hello, 世界!".as_bytes())?;
    /// assert_eq!(bundle.get_text("./dialogue//greeting")?, "Hello, 世界!");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn get_text(&self, path: &str) -> Result<&str, AssetLookupError> {
        let (path, bytes) = self.lookup(path)?;
        std::str::from_utf8(bytes).map_err(|error| AssetLookupError::InvalidUtf8 {
            path: path.into(),
            error,
        })
    }
    /// Borrow an exact-size byte array, reporting both expected and actual
    /// lengths on mismatch. Numeric formats should decode explicit endianness;
    /// this does not reinterpret memory or impose an implicit wire format.
    ///
    /// ```
    /// use ferravex_engine::assets::Bundle;
    /// let mut bundle = Bundle::default();
    /// bundle.insert("level/seed", &123_u32.to_le_bytes())?;
    /// let seed = u32::from_le_bytes(*bundle.get_array::<4>("level/seed")?);
    /// assert_eq!(seed, 123);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn get_array<const N: usize>(&self, path: &str) -> Result<&[u8; N], AssetLookupError> {
        let (path, bytes) = self.lookup(path)?;
        bytes
            .as_ref()
            .try_into()
            .map_err(|_| AssetLookupError::WrongLength {
                path: path.into(),
                expected: N,
                actual: bytes.len(),
            })
    }
    fn lookup(&self, path: &str) -> Result<(&str, &Arc<[u8]>), AssetLookupError> {
        let path = normalize_path(path)
            .map_err(|_| AssetLookupError::InvalidPath { path: path.into() })?;
        self.files
            .get_key_value(path.as_ref())
            .map(|(name, bytes)| (name.as_str(), bytes))
            .ok_or_else(|| AssetLookupError::Missing {
                path: path.into_owned(),
            })
    }
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
    /// Iterate read-only metadata in ascending canonical path order.
    ///
    /// Paths borrow this bundle and are already normalized; checked edits may
    /// accept authoring aliases, but the manifest contains only canonical names.
    /// No allocation, payload copy or Arc clone is needed. Fingerprints are
    /// computed lazily over each yielded asset's decoded bytes, so a complete
    /// traversal scans at most 64 MiB across at most 4096 entries. Results depend
    /// only on paths and content, not insertion order or encoding choices.
    ///
    /// Retain [`LiveBundle::current`] to inspect one generation across reloads.
    /// Collect entries if repeated scans are needed; their paths still borrow
    /// that retained bundle. This does not add a serialized manifest format or
    /// change FXPK0001 encoding. See [`AssetManifestEntry::fingerprint`] for the
    /// fixed algorithm and collision limitations.
    ///
    /// ```
    /// use ferravex_engine::assets::Bundle;
    /// let mut bundle = Bundle::default();
    /// bundle.apply_edits_checked([
    ///     (r".\textures\\flat", Some(&[7; 16][..])),
    ///     ("./meta//empty", Some(&b""[..])),
    /// ])?;
    /// let entries: Vec<_> = bundle.manifest().collect();
    /// assert_eq!(entries.iter().map(|entry| entry.path).collect::<Vec<_>>(),
    ///     ["meta/empty", "textures/flat"]);
    /// assert_eq!(entries.iter().map(|entry| entry.byte_len).sum::<usize>(),
    ///     bundle.decoded_bytes());
    /// let decoded = Bundle::decode(&bundle.encode())?;
    /// assert_eq!(entries, decoded.manifest().collect::<Vec<_>>());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn manifest(
        &self,
    ) -> impl ExactSizeIterator<Item = AssetManifestEntry<'_>>
    + DoubleEndedIterator
    + std::iter::FusedIterator
    + '_ {
        self.files.iter().map(|(path, bytes)| AssetManifestEntry {
            path,
            byte_len: bytes.len(),
            fingerprint: checksum(bytes),
        })
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
                    for pair in stored.as_chunks::<2>().0 {
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
                    for pair in stored.as_chunks::<2>().0 {
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
        && path.len() <= MAX_PATH_BYTES
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
    /// Decode and validate a candidate before publishing it. A validator can
    /// require typed assets or check an application's schema. It runs once after
    /// decoding, even for unchanged content; it is never called on decode failure.
    ///
    /// Returns the existing sorted content diff. Unlike legacy [`Self::reload`],
    /// an unchanged candidate retains both the generation and current Arc. A
    /// changed candidate increments the generation exactly once. Any failure,
    /// including validation or generation overflow, preserves the last good bundle.
    ///
    /// ```
    /// use ferravex_engine::assets::{AssetLookupError, Bundle, LiveBundle};
    /// let mut authored = Bundle::default();
    /// authored.insert("level/seed", &42_u32.to_le_bytes())?;
    /// let validate = |bundle: &Bundle| -> Result<(), AssetLookupError> {
    ///     bundle.get_array::<4>("level/seed")?;
    ///     Ok(())
    /// };
    /// let mut live = LiveBundle::default();
    /// let diff = live.reload_checked(&authored.encode(), validate)?;
    /// assert_eq!(diff.added, ["level/seed"]);
    /// let generation = live.generation();
    /// authored.insert("level/seed", b"bad")?;
    /// assert!(live.reload_checked(&authored.encode(), validate).is_err());
    /// assert_eq!(live.generation(), generation);
    /// assert_eq!(live.current().get_array::<4>("level/seed")?, &42_u32.to_le_bytes());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn reload_checked<E>(
        &mut self,
        bytes: &[u8],
        validate: impl FnOnce(&Bundle) -> Result<(), E>,
    ) -> Result<BundleDiff, BundleUpdateError<E>> {
        let next = Bundle::decode(bytes).map_err(BundleUpdateError::Decode)?;
        self.commit_checked(next, validate)
    }
    /// Apply normalized edits and validate the whole candidate before publication.
    /// Reuses [`Bundle::apply_edits_checked`], including ordered edits and
    /// intermediate limits. Validation sees the final candidate only. Generation,
    /// reader retention and no-op behavior match [`Self::reload_checked`].
    ///
    /// ```
    /// use ferravex_engine::assets::{AssetLookupError, Bundle, LiveBundle};
    /// let validate = |bundle: &Bundle| -> Result<(), AssetLookupError> {
    ///     bundle.get_text("meta/title")?;
    ///     Ok(())
    /// };
    /// let mut live = LiveBundle::default();
    /// live.apply_edits_checked([
    ///     (r".\meta\title", Some(&b"My game"[..])),
    /// ], validate)?;
    /// let old = live.current();
    /// assert!(live.apply_edits_checked([("meta/title", None)], validate).is_err());
    /// assert!(std::sync::Arc::ptr_eq(&old, &live.current()));
    /// # Ok::<(), ferravex_engine::assets::BundleUpdateError<AssetLookupError>>(())
    /// ```
    pub fn apply_edits_checked<'a, E>(
        &mut self,
        edits: impl IntoIterator<Item = (&'a str, Option<&'a [u8]>)>,
        validate: impl FnOnce(&Bundle) -> Result<(), E>,
    ) -> Result<BundleDiff, BundleUpdateError<E>> {
        let next = self
            .current
            .edited_with_paths(edits, true)
            .map_err(BundleUpdateError::Edit)?;
        self.commit_checked(next, validate)
    }
    fn commit_checked<E>(
        &mut self,
        next: Bundle,
        validate: impl FnOnce(&Bundle) -> Result<(), E>,
    ) -> Result<BundleDiff, BundleUpdateError<E>> {
        validate(&next).map_err(BundleUpdateError::Validation)?;
        let diff = self.current.diff(&next);
        if !diff.added.is_empty() || !diff.removed.is_empty() || !diff.changed.is_empty() {
            let generation = self
                .generation
                .checked_add(1)
                .ok_or(BundleUpdateError::GenerationExhausted)?;
            self.current = Arc::new(next);
            self.generation = generation;
        }
        Ok(diff)
    }
}
#[cfg(test)]
mod manifest_tests {
    use super::*;

    #[test]
    fn sorted_borrowed_manifest_has_fixed_content_fingerprints() {
        let records: [(&str, &[u8], u64); 5] = [
            ("A/empty", b"", 0xcbf29ce484222325),
            ("a/one", b"a", 0xaf63dc4c8601ec8c),
            ("b/binary", &[0, 255, 128, 1], 0xd865b27bf628a893),
            ("f/word", b"foo", 0xdcb27518fed9d577),
            ("h/greeting", b"hello", 0xa430d84680aabd0b),
        ];
        let mut bundle = Bundle::default();
        let mut aliases = Bundle::default();
        for &(path, bytes, _) in records.iter().rev() {
            bundle.insert(path, bytes).unwrap();
        }
        for &(path, bytes, _) in &records {
            aliases
                .apply_edits_checked([(
                    format!(".\\{}", path.replace('/', "\\\\")).as_str(),
                    Some(bytes),
                )])
                .unwrap();
        }
        let reader = bundle.get("b/binary").unwrap();
        let readers = Arc::strong_count(&reader);
        let before = bundle.encode();
        let entries: Vec<_> = bundle.manifest().collect();
        assert_eq!(entries.len(), bundle.len());
        assert_eq!(entries, aliases.manifest().collect::<Vec<_>>());
        for (entry, &(path, bytes, fingerprint)) in entries.iter().zip(&records) {
            assert_eq!(
                *entry,
                AssetManifestEntry {
                    path,
                    byte_len: bytes.len(),
                    fingerprint,
                }
            );
            let stored_path = bundle.files.get_key_value(path).unwrap().0;
            assert_eq!(entry.path.as_ptr(), stored_path.as_ptr());
        }
        assert_eq!(
            entries.iter().map(|entry| entry.byte_len).sum::<usize>(),
            bundle.decoded_bytes()
        );
        assert_eq!(Arc::strong_count(&reader), readers);
        assert_eq!(bundle.encode(), before);
    }

    #[test]
    fn manifest_iterators_cover_empty_and_mixed_direction_traversals() {
        let mut bundle = Bundle::default();
        {
            let mut empty = bundle.manifest();
            assert_eq!(empty.len(), 0);
            assert_eq!(empty.size_hint(), (0, Some(0)));
            assert_eq!(empty.next(), None);
            assert_eq!(empty.next_back(), None);
        }
        for path in ["c", "a", "b"] {
            bundle.insert(path, b"").unwrap();
        }
        let mut entries = bundle.manifest();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries.next().unwrap().path, "a");
        assert_eq!(entries.next_back().unwrap().path, "c");
        assert_eq!(entries.size_hint(), (1, Some(1)));
        assert_eq!(entries.next().unwrap().path, "b");
        assert_eq!(entries.len(), 0);
        assert_eq!(entries.next_back(), None);
        assert_eq!(entries.next(), None);
    }

    #[test]
    fn fingerprints_ignore_encoding_and_preserve_fxpk0001_golden_bytes() {
        // Fixed FXPK0001 fixtures for one asset named "payload", containing "aaaa".
        const RAW: &[u8] = b"FXPK0001\x01\x00\x00\x00\x07\x00payload\x00\
            \x04\x00\x00\x00\x04\x00\x00\x00\x59\x22\x91\xea\x83\x61\x21\xf5\
            aaaa\xd7\x23\x9f\xcb\x23\x86\xff\x8e";
        const RLE: &[u8] = b"FXPK0001\x01\x00\x00\x00\x07\x00payload\x01\
            \x04\x00\x00\x00\x02\x00\x00\x00\x59\x22\x91\xea\x83\x61\x21\xf5\
            \x04a\x81\xcf\x1b\xe7\x7b\x11\xe9\x22";
        let raw = Bundle::decode(RAW).unwrap();
        let rle = Bundle::decode(RLE).unwrap();
        let mut authored = Bundle::default();
        authored.insert("payload", b"aaaa").unwrap();
        let expected = [AssetManifestEntry {
            path: "payload",
            byte_len: 4,
            fingerprint: 0xf5216183ea912259,
        }];
        for bundle in [&raw, &rle, &authored] {
            assert_eq!(bundle.manifest().collect::<Vec<_>>(), expected);
            assert_eq!(bundle.encode(), RLE);
        }
        let mut live = LiveBundle::default();
        live.reload_checked(RAW, |_| Ok::<(), ()>(())).unwrap();
        let retained = live.current();
        assert_eq!(
            live.reload_checked(RLE, |_| Ok::<(), ()>(())),
            Ok(BundleDiff::default())
        );
        assert_eq!(live.generation(), 1);
        assert!(Arc::ptr_eq(&retained, &live.current()));
    }

    #[test]
    fn fingerprints_follow_content_through_subtrees_edits_and_retained_snapshots() {
        let mut bundle = Bundle::default();
        bundle.insert("dir/asset", b"hello").unwrap();
        bundle.insert("copy", b"hello").unwrap();
        let old = bundle.clone();
        let old_entries: Vec<_> = old.manifest().collect();
        assert_eq!(old_entries[0].fingerprint, old_entries[1].fingerprint);
        let subtree = bundle.subtree("dir").unwrap();
        let entry = subtree.manifest().next().unwrap();
        assert_eq!(entry.path, "asset");
        assert_eq!(entry.fingerprint, old_entries[1].fingerprint);
        bundle.insert("dir/asset", b"jello").unwrap();
        bundle.remove("copy");
        let changed = bundle.manifest().next().unwrap();
        assert_eq!(changed.byte_len, old_entries[1].byte_len);
        assert_ne!(changed.fingerprint, old_entries[1].fingerprint);
        assert_eq!(old.manifest().collect::<Vec<_>>(), old_entries);
        assert_eq!(bundle.len(), 1);
        assert_eq!(bundle.decoded_bytes(), 5);
    }

    #[test]
    fn identical_insert_and_net_unchanged_edits_preserve_payload_identity() {
        for checked in [false, true] {
            let mut bundle = Bundle::default();
            for path in ["same", "restore", "revert", "changed"] {
                bundle.insert(path, b"old").unwrap();
            }
            bundle.insert("empty", b"").unwrap();
            let old = bundle.clone();
            for path in ["same", "restore", "revert", "changed", "empty"] {
                let bytes = old.get(path).unwrap();
                bundle.insert(path, &bytes).unwrap();
                assert!(Arc::ptr_eq(&bytes, &bundle.get(path).unwrap()));
            }
            let edits = [
                (if checked { "./same" } else { "same" }, Some(&b"old"[..])),
                ("restore", None),
                ("restore", Some(&b"old"[..])),
                ("revert", Some(&b"temporary"[..])),
                ("revert", Some(&b"old"[..])),
                ("empty", None),
                ("empty", Some(&b""[..])),
                ("absent", None),
                ("changed", Some(&b"new"[..])),
            ];
            if checked {
                bundle.apply_edits_checked(edits).unwrap();
            } else {
                bundle.apply_edits(edits).unwrap();
            }
            for path in ["same", "restore", "revert", "empty"] {
                assert!(Arc::ptr_eq(
                    &old.get(path).unwrap(),
                    &bundle.get(path).unwrap()
                ));
            }
            assert!(!Arc::ptr_eq(
                &old.get("changed").unwrap(),
                &bundle.get("changed").unwrap()
            ));
            assert_eq!(old.diff(&bundle).changed, ["changed"]);
            assert_eq!(&*old.get("changed").unwrap(), b"old");
            assert_eq!(&*bundle.get("changed").unwrap(), b"new");
            assert_eq!(bundle.len(), old.len());
            assert_eq!(bundle.decoded_bytes(), old.decoded_bytes());
        }
    }

    #[test]
    fn noop_edits_at_capacity_keep_readers_and_still_check_intermediate_limits() {
        let mut bundle = Bundle::default();
        for index in 0..MAX_FILES {
            bundle.insert(&format!("file{index}"), b"").unwrap();
        }
        let reader = bundle.get("file0").unwrap();
        let before = bundle.encode();
        bundle.insert("file0", b"").unwrap();
        bundle
            .apply_edits_checked([("file0", None), ("./file0", Some(&b""[..]))])
            .unwrap();
        assert!(Arc::ptr_eq(&reader, &bundle.get("file0").unwrap()));
        assert_eq!(bundle.manifest().len(), MAX_FILES);
        assert!(
            bundle
                .manifest()
                .all(|entry| entry.byte_len == 0 && entry.fingerprint == 0xcbf29ce484222325)
        );
        let error = bundle
            .apply_edits_checked([("extra", Some(&b""[..])), ("extra", None)])
            .unwrap_err();
        assert_eq!(error.index, 0);
        assert_eq!(error.error, AssetError::Limit);
        assert_eq!(bundle.insert("./file0", b""), Err(AssetError::Path));
        assert_eq!(bundle.encode(), before);
        assert!(Arc::ptr_eq(&reader, &bundle.get("file0").unwrap()));
    }

    #[test]
    fn live_manifest_and_payload_identity_survive_noops_and_failed_validation() {
        let mut live = LiveBundle::default();
        live.apply_edits_checked([("data", Some(&b"hello"[..]))], |_| Ok::<(), ()>(()))
            .unwrap();
        let old = live.current();
        let entries: Vec<_> = old.manifest().collect();
        let reader = old.get("data").unwrap();
        let mut validations = 0;
        let diff = live
            .apply_edits_checked(
                [("data", None), ("./data", Some(&b"hello"[..]))],
                |candidate| {
                    validations += 1;
                    assert_eq!(candidate.manifest().collect::<Vec<_>>(), entries);
                    assert!(Arc::ptr_eq(&reader, &candidate.get("data").unwrap()));
                    Ok::<(), ()>(())
                },
            )
            .unwrap();
        assert_eq!(validations, 1);
        assert_eq!(diff, BundleDiff::default());
        assert_eq!(live.generation(), 1);
        assert!(Arc::ptr_eq(&old, &live.current()));
        assert_eq!(
            live.apply_edits_checked([("data", Some(&b"other"[..]))], |candidate| {
                assert_ne!(candidate.manifest().collect::<Vec<_>>(), entries);
                Err("rejected")
            }),
            Err(BundleUpdateError::Validation("rejected"))
        );
        assert_eq!(live.generation(), 1);
        assert!(Arc::ptr_eq(&old, &live.current()));
        assert_eq!(live.current().manifest().collect::<Vec<_>>(), entries);
        assert!(Arc::ptr_eq(&reader, &live.current().get("data").unwrap()));
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

#[cfg(test)]
mod removal_tests {
    use super::*;
    #[test]
    fn accounting_and_retained_readers_survive_remove_and_reinsert() {
        let mut b = Bundle::default();
        b.insert("a", b"old").unwrap();
        b.insert("empty", b"").unwrap();
        let reader = b.get("a").unwrap();
        assert_eq!((b.len(), b.decoded_bytes()), (2, 3));
        assert_eq!(&*b.remove("a").unwrap(), b"old");
        assert_eq!(&*reader, b"old");
        assert!(b.remove("missing").is_none());
        assert_eq!(b.decoded_bytes(), 0);
        b.insert("a", b"newer").unwrap();
        assert_eq!(b.decoded_bytes(), 5);
        b.remove("empty");
        b.remove("a");
        assert!(b.is_empty());
        assert!(Bundle::decode(&b.encode()).unwrap().is_empty());
    }
}

#[cfg(test)]
mod transactions_tests {
    use super::*;
    #[test]
    fn bad_later_edit_rolls_back_earlier_mutations() {
        let mut b = Bundle::default();
        b.insert("a", b"old").unwrap();
        let original = b.encode();
        assert_eq!(
            b.apply_edits([("a", Some(&b"new"[..])), ("../bad", None)]),
            Err(AssetError::Path)
        );
        assert_eq!(b.encode(), original);
        b.apply_edits([("a", None), ("b", Some(&b"ok"[..]))])
            .unwrap();
        assert!(b.get("a").is_none());
        assert_eq!(b.decoded_bytes(), 2);
        assert_eq!(&*b.get("b").unwrap(), b"ok");
    }
}

#[cfg(test)]
mod subtrees_tests {
    use super::*;
    #[test]
    fn directory_boundaries_and_nested_names_are_preserved() {
        let mut b = Bundle::default();
        for name in ["tex/a", "tex/nested/b", "texture/c", "tex"] {
            b.insert(name, b"x").unwrap();
        }
        let sub = b.subtree("tex").unwrap();
        assert_eq!(sub.names().collect::<Vec<_>>(), ["a", "nested/b"]);
        assert_eq!(sub.decoded_bytes(), 2);
        assert!(Arc::ptr_eq(
            &b.get("tex/a").unwrap(),
            &sub.get("a").unwrap()
        ));
        assert_eq!(b.subtree("").unwrap().encode(), b.encode());
        assert!(b.subtree("absent").unwrap().is_empty());
        assert!(b.subtree("../").is_err());
    }
}

#[cfg(test)]
mod diff_tests {
    use super::*;
    #[test]
    fn compares_content_in_sorted_order() {
        let mut old = Bundle::default();
        for (n, v) in [("gone", b"a"), ("same", b"b"), ("edit", b"c")] {
            old.insert(n, v).unwrap();
        }
        let mut next = old.clone();
        next.remove("gone");
        next.insert("same", b"b").unwrap();
        next.insert("edit", b"d").unwrap();
        next.insert("z", b"").unwrap();
        next.insert("a", b"").unwrap();
        assert_eq!(
            old.diff(&next),
            BundleDiff {
                added: vec!["a".into(), "z".into()],
                removed: vec!["gone".into()],
                changed: vec!["edit".into()]
            }
        );
        assert_eq!(
            next.diff(&Bundle::decode(&next.encode()).unwrap()),
            BundleDiff::default()
        );
    }
}

#[cfg(test)]
mod checked_tests {
    use super::*;

    fn validate_seed(bundle: &Bundle) -> Result<(), AssetLookupError> {
        bundle.get_array::<4>("level/seed")?;
        Ok(())
    }

    fn accept(_: &Bundle) -> Result<(), AssetLookupError> {
        Ok(())
    }

    #[test]
    fn normalizes_authoring_paths_without_weakening_cooked_paths() {
        assert!(matches!(
            normalize_path("Tex/.hidden-file_1.rgba"),
            Ok(Cow::Borrowed(_))
        ));
        for path in [
            "./Tex//flat.rgba/",
            r".\Tex\\.\flat.rgba",
            "Tex/./flat.rgba",
        ] {
            assert_eq!(normalize_path(path).unwrap(), "Tex/flat.rgba");
        }
        for path in [
            "",
            ".",
            "././",
            "/root",
            r"\root",
            "//server/a",
            r"C:\a",
            "C:a",
            "../escape",
            "safe/../escape",
            r"safe\..\escape",
            "a b",
            "café",
            "a\0b",
        ] {
            assert_eq!(normalize_path(path), Err(AssetError::Path), "{path:?}");
        }
        let mut bundle = Bundle::default();
        bundle.insert("Tex/flat.rgba", b"pixels").unwrap();
        assert_eq!(
            bundle.insert("./Tex/flat.rgba", b"other"),
            Err(AssetError::Path)
        );
        assert!(bundle.get(r"Tex\flat.rgba").is_none());
        assert!(bundle.get_checked("tex/flat.rgba").is_err());
        assert_eq!(&*bundle.get_checked(r"Tex\flat.rgba").unwrap(), b"pixels");
    }

    #[test]
    fn normalized_length_bound_applies_after_normalization_and_never_overflows() {
        let max = "a".repeat(MAX_PATH_BYTES);
        assert_eq!(normalize_path(&format!("./{max}//./")).unwrap(), max);
        assert_eq!(normalize_path(&format!("{max}/b")), Err(AssetError::Path));
        assert_eq!(normalize_path(&format!("{max}b")), Err(AssetError::Path));
        let nested = format!("{}/b", "a".repeat(MAX_PATH_BYTES - 2));
        assert_eq!(normalize_path(&format!("./{nested}")).unwrap(), nested);
        assert_eq!(
            normalize_path(&format!("{}a", "./".repeat(2000))).unwrap(),
            "a"
        );
    }

    #[test]
    fn typed_lookups_report_precise_failures_and_borrow_payloads() {
        let mut bundle = Bundle::default();
        bundle
            .insert("text/greeting", "Hello, 世界!".as_bytes())
            .unwrap();
        bundle.insert("text/broken", &[b'a', 0xff]).unwrap();
        bundle.insert("level/seed", &42_u32.to_le_bytes()).unwrap();
        bundle.insert("empty", b"").unwrap();
        assert_eq!(bundle.get_text("./text//greeting").unwrap(), "Hello, 世界!");
        assert_eq!(
            u32::from_le_bytes(*bundle.get_array::<4>("level/seed").unwrap()),
            42
        );
        assert_eq!(bundle.get_array::<0>("empty").unwrap(), &[]);
        assert_eq!(bundle.get_text("empty").unwrap(), "");
        let reader = bundle.get_checked("level/seed").unwrap();
        assert_eq!(
            reader.as_ptr(),
            bundle.get_array::<4>("level/seed").unwrap().as_ptr()
        );
        assert!(Arc::ptr_eq(&reader, &bundle.get("level/seed").unwrap()));
        assert_eq!(
            bundle.get_text("../bad"),
            Err(AssetLookupError::InvalidPath {
                path: "../bad".into()
            })
        );
        assert_eq!(
            bundle.get_checked("./missing//file"),
            Err(AssetLookupError::Missing {
                path: "missing/file".into()
            })
        );
        assert_eq!(
            bundle.get_array::<3>("./level/seed"),
            Err(AssetLookupError::WrongLength {
                path: "level/seed".into(),
                expected: 3,
                actual: 4,
            })
        );
        match bundle.get_text(r"text\broken").unwrap_err() {
            AssetLookupError::InvalidUtf8 { path, error } => {
                assert_eq!(path, "text/broken");
                assert_eq!(error.valid_up_to(), 1);
                assert_eq!(error.error_len(), Some(1));
            }
            error => panic!("unexpected error: {error}"),
        }
        bundle.remove("level/seed");
        assert_eq!(&*reader, &42_u32.to_le_bytes());
    }

    #[test]
    fn checked_edits_report_first_failure_and_preserve_readers_and_accounting() {
        let mut bundle = Bundle::default();
        bundle.insert("meta/version", b"v1").unwrap();
        let before = bundle.encode();
        let reader = bundle.get("meta/version").unwrap();
        let error = bundle
            .apply_edits_checked([
                (r"meta\version", Some(&b"v2"[..])),
                ("meta/../escape", None),
                ("unreached", Some(&b"new"[..])),
            ])
            .unwrap_err();
        assert_eq!(
            error,
            AssetEditError {
                index: 1,
                path: "meta/../escape".into(),
                error: AssetError::Path
            }
        );
        assert_eq!(bundle.encode(), before);
        assert_eq!(bundle.decoded_bytes(), 2);
        assert!(Arc::ptr_eq(&reader, &bundle.get("meta/version").unwrap()));
        assert!(error.to_string().contains("edit 1"));
        assert!(std::error::Error::source(&error).is_some());
        bundle
            .apply_edits_checked([
                (r"meta\version", Some(&b"v2"[..])),
                ("./meta//version", None),
                ("meta/./version", Some(&b"version 3"[..])),
            ])
            .unwrap();
        assert_eq!(bundle.len(), 1);
        assert_eq!(bundle.decoded_bytes(), 9);
        assert_eq!(bundle.get_text("meta/version").unwrap(), "version 3");
        assert_eq!(&*reader, b"v1");
        // Legacy transactions must continue to reject authoring aliases.
        assert_eq!(
            bundle.apply_edits([("./meta/version", None)]),
            Err(AssetError::Path)
        );
    }

    #[test]
    fn checked_encoding_is_deterministic_across_aliases_and_edit_order() {
        let mut canonical = Bundle::default();
        canonical.insert("a/seed", &[4; 600]).unwrap();
        canonical.insert("z/empty", b"").unwrap();
        let mut authored = Bundle::default();
        authored
            .apply_edits_checked([
                (r".\z\empty", Some(&b""[..])),
                ("a//./seed", Some(&[4; 600][..])),
            ])
            .unwrap();
        assert_eq!(authored.encode(), canonical.encode());
        assert_eq!(
            Bundle::decode(&authored.encode()).unwrap().encode(),
            canonical.encode()
        );
        assert_eq!(authored.names().collect::<Vec<_>>(), ["a/seed", "z/empty"]);
    }

    #[test]
    fn failed_reload_validation_and_corruption_preserve_last_good_generation() {
        let mut bundle = Bundle::default();
        bundle.insert("level/seed", &7_u32.to_le_bytes()).unwrap();
        let mut live = LiveBundle::default();
        live.reload_checked(&bundle.encode(), validate_seed)
            .unwrap();
        let old = live.current();
        bundle.insert("level/seed", b"bad").unwrap();
        assert!(matches!(
            live.reload_checked(&bundle.encode(), validate_seed),
            Err(BundleUpdateError::Validation(
                AssetLookupError::WrongLength {
                    expected: 4,
                    actual: 3,
                    ..
                }
            ))
        ));
        assert_eq!(live.generation(), 1);
        assert!(Arc::ptr_eq(&old, &live.current()));
        let mut corrupt = bundle.encode();
        corrupt[20] ^= 1;
        let mut called = false;
        let result = live.reload_checked(&corrupt, |_| {
            called = true;
            Ok::<(), AssetLookupError>(())
        });
        assert_eq!(result, Err(BundleUpdateError::Decode(AssetError::Checksum)));
        assert!(!called);
        assert_eq!(live.generation(), 1);
        assert!(Arc::ptr_eq(&old, &live.current()));
        bundle.insert("level/seed", &8_u32.to_le_bytes()).unwrap();
        let diff = live
            .reload_checked(&bundle.encode(), validate_seed)
            .unwrap();
        assert_eq!(diff.changed, ["level/seed"]);
        assert_eq!(live.generation(), 2);
        assert_eq!(
            old.get_array::<4>("level/seed").unwrap(),
            &7_u32.to_le_bytes()
        );
        assert_eq!(
            live.current().get_array::<4>("level/seed").unwrap(),
            &8_u32.to_le_bytes()
        );
    }

    #[test]
    fn live_edits_validate_only_final_candidate_and_rollback_every_failure() {
        let mut live = LiveBundle::default();
        live.apply_edits_checked(
            [("level/seed", Some(&7_u32.to_le_bytes()[..]))],
            validate_seed,
        )
        .unwrap();
        let old = live.current();
        assert!(matches!(
            live.apply_edits_checked([("level/seed", None)], validate_seed),
            Err(BundleUpdateError::Validation(
                AssetLookupError::Missing { .. }
            ))
        ));
        assert!(Arc::ptr_eq(&old, &live.current()));
        let mut called = false;
        let result = live.apply_edits_checked([("level/seed", None), ("../bad", None)], |_| {
            called = true;
            Ok::<(), AssetLookupError>(())
        });
        assert!(matches!(
            result,
            Err(BundleUpdateError::Edit(AssetEditError { index: 1, .. }))
        ));
        assert!(!called);
        assert!(Arc::ptr_eq(&old, &live.current()));
        assert_eq!(live.generation(), 1);
        let mut calls = 0;
        live.apply_edits_checked(
            [
                ("level/seed", None),
                ("./level/seed", Some(&8_u32.to_le_bytes()[..])),
            ],
            |bundle| {
                calls += 1;
                validate_seed(bundle)
            },
        )
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(live.generation(), 2);
        assert_eq!(
            live.current().get_array::<4>("level/seed").unwrap(),
            &8_u32.to_le_bytes()
        );
    }

    #[test]
    fn unchanged_updates_keep_arc_and_generation_but_still_validate() {
        let mut live = LiveBundle::default();
        live.apply_edits_checked(
            [("level/seed", Some(&7_u32.to_le_bytes()[..]))],
            validate_seed,
        )
        .unwrap();
        let old = live.current();
        assert_eq!(
            live.reload_checked(&old.encode(), validate_seed).unwrap(),
            BundleDiff::default()
        );
        assert_eq!(
            live.apply_edits_checked(
                [
                    ("level/seed", None),
                    (r"level\seed", Some(&7_u32.to_le_bytes()[..])),
                    ("absent", None),
                ],
                validate_seed
            )
            .unwrap(),
            BundleDiff::default()
        );
        assert!(Arc::ptr_eq(&old, &live.current()));
        assert_eq!(live.generation(), 1);
        assert_eq!(
            live.reload_checked(&old.encode(), |_| Err("unsupported schema")),
            Err(BundleUpdateError::Validation("unsupported schema"))
        );
        assert!(Arc::ptr_eq(&old, &live.current()));
        assert_eq!(live.generation(), 1);
        // Existing reload semantics remain compatible, including unchanged content.
        live.reload(&old.encode()).unwrap();
        assert_eq!(live.generation(), 2);
    }

    #[test]
    fn generation_overflow_rejects_changes_but_allows_checked_noops() {
        let mut live = LiveBundle {
            generation: u64::MAX,
            ..LiveBundle::default()
        };
        let old = live.current();
        assert_eq!(
            live.reload_checked(&old.encode(), accept).unwrap(),
            BundleDiff::default()
        );
        assert_eq!(
            live.apply_edits_checked([("absent", None)], accept)
                .unwrap(),
            BundleDiff::default()
        );
        assert_eq!(
            live.apply_edits_checked([("new", Some(&b"data"[..]))], accept),
            Err(BundleUpdateError::GenerationExhausted)
        );
        let mut next = Bundle::default();
        next.insert("new", b"data").unwrap();
        assert_eq!(
            live.reload_checked(&next.encode(), accept),
            Err(BundleUpdateError::GenerationExhausted)
        );
        assert_eq!(live.reload(&old.encode()), Err(AssetError::Limit));
        assert!(Arc::ptr_eq(&old, &live.current()));
        assert_eq!(live.generation(), u64::MAX);
    }

    #[test]
    fn file_limit_is_transactional_and_replacements_do_not_consume_slots() {
        let mut bundle = Bundle::default();
        for i in 0..MAX_FILES {
            bundle.insert(&format!("file{i}"), b"").unwrap();
        }
        let mut live = LiveBundle::default();
        live.reload_checked(&bundle.encode(), accept).unwrap();
        let old = live.current();
        assert!(matches!(
            live.apply_edits_checked(
                [("file0", Some(&b"changed"[..])), ("extra", Some(&b""[..])),],
                accept
            ),
            Err(BundleUpdateError::Edit(AssetEditError {
                index: 1,
                error: AssetError::Limit,
                ..
            }))
        ));
        assert!(Arc::ptr_eq(&old, &live.current()));
        assert_eq!(live.generation(), 1);
        assert_eq!(live.current().decoded_bytes(), 0);
        let diff = live
            .apply_edits_checked(
                [
                    ("./file0", Some(&b"changed"[..])),
                    ("file1", None),
                    ("extra", Some(&b""[..])),
                ],
                accept,
            )
            .unwrap();
        assert_eq!(diff.added, ["extra"]);
        assert_eq!(diff.removed, ["file1"]);
        assert_eq!(diff.changed, ["file0"]);
        assert_eq!(live.current().len(), MAX_FILES);
        assert_eq!(live.current().decoded_bytes(), 7);
    }

    #[test]
    fn decoded_byte_limit_rejects_edit_before_publication() {
        let mut bundle = Bundle::default();
        bundle.insert("full", &vec![0; MAX_BYTES]).unwrap();
        let mut live = LiveBundle {
            current: Arc::new(bundle),
            generation: 1,
        };
        let old = live.current();
        let result = live.apply_edits_checked(
            [("empty", Some(&b""[..])), ("overflow", Some(&b"x"[..]))],
            accept,
        );
        assert!(matches!(
            result,
            Err(BundleUpdateError::Edit(AssetEditError {
                index: 1,
                error: AssetError::Limit,
                ..
            }))
        ));
        assert!(Arc::ptr_eq(&old, &live.current()));
        assert_eq!(live.generation(), 1);
        assert_eq!(live.current().decoded_bytes(), MAX_BYTES);
        live.apply_edits_checked(
            [("full", None), ("replacement", Some(&b"small"[..]))],
            accept,
        )
        .unwrap();
        assert_eq!(live.current().decoded_bytes(), 5);
        assert_eq!(old.get_checked("full").unwrap().len(), MAX_BYTES);
    }
}
