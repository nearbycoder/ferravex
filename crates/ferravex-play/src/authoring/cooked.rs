use super::*;
use std::collections::BTreeSet;

const MAGIC: &[u8; 8] = b"FXSC0001";
const HEADER_BYTES: usize = 12;
const CHECKSUM_BYTES: usize = 8;
// Shortest valid records: a one-byte identifier, empty optional collections,
// and (for a property) a bool tag and value.
const MIN_NODE_BYTES: usize = 2 + 1 + 2 + 40 + 2;
const MIN_COMPONENT_BYTES: usize = 2 + 1 + 2;
const MIN_FIELD_BYTES: usize = 2 + 1 + 1 + 1;

/// Read-only metadata from a checksum-verified `FXSC0001` envelope.
///
/// Counts describe the header, not a validated scene. Use [`SceneData::decode`]
/// to check the body, schema, references and hierarchy before using scene data.
/// The FNV-1a checksum detects accidental corruption; it is not authentication.
///
/// ```
/// use ferravex_play::authoring::{Document, SceneData, Schema};
/// let bytes = Document::load("ferravex_scene 1; node root {}", &Schema::new())?
///     .encode()?;
/// let metadata = SceneData::inspect_cooked(&bytes)?;
/// assert_eq!(metadata.format_version(), 1);
/// assert_eq!(metadata.node_count(), 1);
/// assert_eq!(metadata.byte_len(), bytes.len());
/// assert_eq!(&bytes[bytes.len() - 8..], &metadata.checksum().to_le_bytes());
/// # Ok::<(), ferravex_play::authoring::ContentError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CookedMetadata {
    node_count: usize,
    byte_len: usize,
    checksum: u64,
}

impl CookedMetadata {
    /// Format version accepted by this inspector (`FXSC0001` is version 1).
    pub fn format_version(&self) -> u32 {
        1
    }

    /// Number of nodes declared in the header, bounded by [`MAX_NODES`].
    pub fn node_count(&self) -> usize {
        self.node_count
    }

    /// Entire encoded length, including the header and trailing checksum.
    pub fn byte_len(&self) -> usize {
        self.byte_len
    }

    /// Verified FNV-1a checksum of every byte before the trailing checksum.
    pub fn checksum(&self) -> u64 {
        self.checksum
    }
}

impl SceneData {
    /// Inspect a cooked envelope without allocating or constructing a scene.
    ///
    /// Checks the size, version marker, checksum, node limit and minimum space
    /// for the declared node count. Body contents (including trailing bytes),
    /// schema and hierarchy are checked only by [`Self::decode`]. Errors use
    /// zero-based binary byte offsets with line/column set to zero.
    ///
    /// ```
    /// use ferravex_play::authoring::{Document, ErrorKind, SceneData, Schema};
    /// let mut bytes = Document::load("ferravex_scene 1;", &Schema::new())?.encode()?;
    /// assert_eq!(SceneData::inspect_cooked(&bytes)?.node_count(), 0);
    /// let last = bytes.len() - 1;
    /// bytes[last] ^= 1;
    /// assert_eq!(SceneData::inspect_cooked(&bytes).unwrap_err().kind, ErrorKind::Checksum);
    /// # Ok::<(), ferravex_play::authoring::ContentError>(())
    /// ```
    pub fn inspect_cooked(bytes: &[u8]) -> Result<CookedMetadata, ContentError> {
        envelope(bytes).map(|(metadata, _)| metadata)
    }
}

fn error_at(kind: ErrorKind, offset: usize, message: &str) -> ContentError {
    ContentError::new(
        kind,
        SourceSpan {
            offset,
            ..Default::default()
        },
        message,
    )
}

fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325_u64, |h, v| {
        (h ^ u64::from(*v)).wrapping_mul(0x100000001b3)
    })
}
fn string(out: &mut Vec<u8>, value: &str) {
    out.extend((value.len() as u16).to_le_bytes());
    out.extend(value.as_bytes());
}
pub(super) fn encode(scene: &SceneData) -> Result<Vec<u8>, ContentError> {
    let mut out = MAGIC.to_vec();
    out.extend((scene.nodes.len() as u32).to_le_bytes());
    for n in &scene.nodes {
        string(&mut out, &n.id);
        string(&mut out, n.parent.as_deref().unwrap_or(""));
        for v in n
            .local
            .translation
            .into_iter()
            .chain(n.local.rotation)
            .chain(n.local.scale)
        {
            out.extend(v.to_le_bytes());
        }
        out.extend((n.components.len() as u16).to_le_bytes());
        for (kind, c) in &n.components {
            string(&mut out, kind);
            out.extend((c.fields.len() as u16).to_le_bytes());
            for (name, p) in &c.fields {
                string(&mut out, name);
                match &p.value {
                    Value::Bool(v) => out.extend([0, u8::from(*v)]),
                    Value::Integer(v) => {
                        out.push(1);
                        out.extend(v.to_le_bytes());
                    }
                    Value::Number(v) => {
                        out.push(2);
                        out.extend(v.to_le_bytes());
                    }
                    Value::Text(v) => {
                        out.push(3);
                        string(&mut out, v);
                    }
                    Value::Vec2(v) => {
                        out.push(4);
                        for x in v {
                            out.extend(x.to_le_bytes());
                        }
                    }
                    Value::Vec3(v) => {
                        out.push(5);
                        for x in v {
                            out.extend(x.to_le_bytes());
                        }
                    }
                    Value::Quaternion(v) => {
                        out.push(6);
                        for x in v {
                            out.extend(x.to_le_bytes());
                        }
                    }
                    Value::Color(v) => {
                        out.push(7);
                        out.extend(v);
                    }
                    Value::Reference(v) => {
                        out.push(8);
                        string(&mut out, v);
                    }
                }
            }
        }
        if out.len() > MAX_COOKED_BYTES - 8 {
            return Err(ContentError::new(
                ErrorKind::Limit,
                n.span,
                "cooked data exceeds 8 MiB",
            ));
        }
    }
    out.extend(checksum(&out).to_le_bytes());
    Ok(out)
}
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn span(&self) -> SourceSpan {
        SourceSpan {
            offset: self.at,
            ..Default::default()
        }
    }
    fn error(&self, message: &str) -> ContentError {
        ContentError::new(ErrorKind::Format, self.span(), message)
    }
    fn take(&mut self, len: usize) -> Result<&'a [u8], ContentError> {
        let end = self
            .at
            .checked_add(len)
            .ok_or_else(|| self.error("length overflow"))?;
        if end > self.bytes.len() {
            return Err(self.error("truncated cooked data"));
        }
        let start = self.at;
        self.at = end;
        Ok(&self.bytes[start..end])
    }
    fn u8(&mut self) -> Result<u8, ContentError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<usize, ContentError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()) as usize)
    }
    fn count(
        &mut self,
        limit: usize,
        minimum: usize,
        context: &str,
    ) -> Result<usize, ContentError> {
        let at = self.at;
        let count = self.u16()?;
        self.check_count(count, limit, minimum, at, context)?;
        Ok(count)
    }
    fn check_count(
        &self,
        count: usize,
        limit: usize,
        minimum: usize,
        at: usize,
        context: &str,
    ) -> Result<(), ContentError> {
        if count > limit {
            return Err(error_at(ErrorKind::Limit, at, context));
        }
        // Division avoids overflowing a length multiplication on narrow targets.
        if count > (self.bytes.len() - self.at) / minimum {
            return Err(error_at(
                ErrorKind::Format,
                at,
                "cooked count exceeds remaining data",
            ));
        }
        Ok(())
    }
    fn string(&mut self, limit: usize) -> Result<&'a str, ContentError> {
        let length_at = self.at;
        let len = self.u16()?;
        if len > limit {
            return Err(error_at(
                ErrorKind::Format,
                length_at,
                "string exceeds limit",
            ));
        }
        let at = self.at;
        let value = std::str::from_utf8(self.take(len)?).map_err(|error| {
            error_at(ErrorKind::Format, at + error.valid_up_to(), "invalid UTF-8")
        })?;
        if let Some(index) = value.find('\0') {
            return Err(error_at(ErrorKind::Format, at + index, "NUL in string"));
        }
        Ok(value)
    }
    fn floats<const N: usize>(&mut self) -> Result<[f32; N], ContentError> {
        let mut v = [0.; N];
        for n in &mut v {
            let at = self.at;
            *n = f32::from_le_bytes(self.take(4)?.try_into().unwrap());
            if !n.is_finite() {
                return Err(error_at(ErrorKind::Format, at, "non-finite float"));
            }
        }
        Ok(v)
    }
}
fn envelope(bytes: &[u8]) -> Result<(CookedMetadata, Reader<'_>), ContentError> {
    if bytes.len() < HEADER_BYTES + CHECKSUM_BYTES
        || bytes.len() > MAX_COOKED_BYTES
        || &bytes[..MAGIC.len()] != MAGIC
    {
        return Err(error_at(ErrorKind::Format, 0, "invalid cooked header/size"));
    }
    let end = bytes.len() - CHECKSUM_BYTES;
    let sum = u64::from_le_bytes(bytes[end..].try_into().unwrap());
    if checksum(&bytes[..end]) != sum {
        return Err(error_at(
            ErrorKind::Checksum,
            end,
            "cooked checksum mismatch",
        ));
    }
    let mut reader = Reader {
        bytes: &bytes[..end],
        at: MAGIC.len(),
    };
    let count = usize::try_from(u32::from_le_bytes(reader.take(4)?.try_into().unwrap()))
        .map_err(|_| error_at(ErrorKind::Limit, MAGIC.len(), "too many cooked nodes"))?;
    reader.check_count(
        count,
        MAX_NODES,
        MIN_NODE_BYTES,
        MAGIC.len(),
        "too many cooked nodes",
    )?;
    Ok((
        CookedMetadata {
            node_count: count,
            byte_len: bytes.len(),
            checksum: sum,
        },
        reader,
    ))
}

pub(super) fn decode(bytes: &[u8], schema: &Schema) -> Result<SceneData, ContentError> {
    let (metadata, mut reader) = envelope(bytes)?;
    let mut nodes = Vec::with_capacity(metadata.node_count.min(64));
    // Borrow IDs from the checked input, avoiding extra string allocations.
    let mut ids = BTreeSet::new();
    let mut values = 0_usize;
    for _ in 0..metadata.node_count {
        let span = reader.span();
        let id = reader.string(512)?;
        if !path(id) {
            return Err(ContentError::new(ErrorKind::Reference, span, id));
        }
        if !ids.insert(id) {
            return Err(ContentError::new(ErrorKind::Duplicate, span, id));
        }
        let parent_span = reader.span();
        let parent = reader.string(512)?;
        if !parent.is_empty() && !path(parent) {
            return Err(ContentError::new(ErrorKind::Reference, parent_span, parent));
        }
        let local = Transform {
            translation: reader.floats()?,
            rotation: reader.floats()?,
            scale: reader.floats()?,
        };
        let count = reader.count(
            MAX_COMPONENTS,
            MIN_COMPONENT_BYTES,
            "too many cooked components",
        )?;
        let mut components = BTreeMap::new();
        for _ in 0..count {
            let component_span = reader.span();
            let kind = reader.string(64)?;
            if !identifier(kind) {
                return Err(ContentError::new(
                    ErrorKind::Format,
                    component_span,
                    "invalid component identifier",
                ));
            }
            if components.contains_key(kind) {
                return Err(ContentError::new(
                    ErrorKind::Duplicate,
                    component_span,
                    "cooked component",
                ));
            }
            let count_at = reader.at;
            let count = reader.count(MAX_FIELDS, MIN_FIELD_BYTES, "too many cooked fields")?;
            if count > MAX_VALUES - values {
                return Err(error_at(
                    ErrorKind::Limit,
                    count_at,
                    "too many cooked values",
                ));
            }
            values += count;
            let mut fields = BTreeMap::new();
            for _ in 0..count {
                let span = reader.span();
                let name = reader.string(64)?;
                if !identifier(name) {
                    return Err(ContentError::new(
                        ErrorKind::Format,
                        span,
                        "invalid field identifier",
                    ));
                }
                if fields.contains_key(name) {
                    return Err(ContentError::new(
                        ErrorKind::Duplicate,
                        span,
                        "cooked property",
                    ));
                }
                let tag_at = reader.at;
                let value = match reader.u8()? {
                    0 => {
                        let at = reader.at;
                        Value::Bool(match reader.u8()? {
                            0 => false,
                            1 => true,
                            _ => return Err(error_at(ErrorKind::Format, at, "invalid bool")),
                        })
                    }
                    1 => Value::Integer(i64::from_le_bytes(reader.take(8)?.try_into().unwrap())),
                    2 => {
                        let at = reader.at;
                        let value = f64::from_le_bytes(reader.take(8)?.try_into().unwrap());
                        if !value.is_finite() {
                            return Err(error_at(ErrorKind::Format, at, "non-finite number"));
                        }
                        Value::Number(value)
                    }
                    3 => Value::Text(reader.string(MAX_STRING)?.into()),
                    4 => Value::Vec2(reader.floats()?),
                    5 => Value::Vec3(reader.floats()?),
                    6 => Value::Quaternion(reader.floats()?),
                    7 => Value::Color(reader.take(4)?.try_into().unwrap()),
                    8 => Value::Reference(reader.string(512)?.into()),
                    _ => return Err(error_at(ErrorKind::Format, tag_at, "unknown value tag")),
                };
                fields.insert(name.into(), Property { value, span });
            }
            components.insert(
                kind.into(),
                Component {
                    fields,
                    span: component_span,
                },
            );
        }
        nodes.push(DataNode {
            id: id.into(),
            parent: (!parent.is_empty()).then(|| parent.into()),
            local,
            world: crate::scene::IDENTITY,
            components,
            span,
        });
    }
    if reader.at != reader.bytes.len() {
        return Err(reader.error("trailing cooked data"));
    }
    expand::finish(nodes, schema)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"ferravex_scene 1; node n { component data {
        b bool true; c rgba 1 2 3 4; i int -9223372036854775808; n number -0.0;
        q quat 0 0 0 2; r ref n; t text "λ🚀"; v vec2 .25 -.75; w vec3 1 2 3;
    }}"#;
    // Independently assembled version-1 bytes, including the FNV-1a trailer.
    const GOLDEN: &str = concat!(
        "46585343303030310100000001006e000000000000000000000000000000000000",
        "00000000000000000000803f0000803f0000803f0000803f01000400646174610900",
        "0100620001010063070102030401006901000000000000008001006e0200000000",
        "0000008001007106000000000000000000000000000000400100720801006e0100",
        "74030600cebbf09f9a80010076040000803e000040bf010077050000803f00000040",
        "0000404019144c0c4662b553",
    );

    fn schema() -> Schema {
        let mut schema = Schema::new();
        schema
            .register(
                "data",
                &[
                    FieldRule::required("b", ValueKind::Bool),
                    FieldRule::required("c", ValueKind::Color),
                    FieldRule::required("i", ValueKind::Integer),
                    FieldRule::required("n", ValueKind::Number),
                    FieldRule::required("q", ValueKind::Quaternion),
                    FieldRule::required("r", ValueKind::Reference),
                    FieldRule::required("t", ValueKind::Text),
                    FieldRule::required("v", ValueKind::Vec2),
                    FieldRule::required("w", ValueKind::Vec3),
                ],
            )
            .unwrap();
        schema
    }

    fn fixture() -> Vec<u8> {
        GOLDEN
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    fn resign(bytes: &mut [u8]) {
        let end = bytes.len() - CHECKSUM_BYTES;
        let sum = checksum(&bytes[..end]);
        bytes[end..].copy_from_slice(&sum.to_le_bytes());
    }

    fn invalid(mut bytes: Vec<u8>, kind: ErrorKind, offset: usize) -> ContentError {
        resign(&mut bytes);
        let error = SceneData::decode(&bytes, &schema()).err().unwrap();
        assert_eq!(error.kind, kind, "{error:?}");
        assert_eq!(error.span.offset, offset, "{error:?}");
        assert_eq!((error.span.line, error.span.column), (0, 0));
        error
    }

    fn value_at(bytes: &[u8], name: u8, tag: u8) -> usize {
        bytes
            .windows(4)
            .position(|w| w == [1, 0, name, tag])
            .unwrap()
            + 4
    }

    #[test]
    fn version_one_golden_bytes_cover_every_value_tag() {
        let bytes = fixture();
        assert_eq!(bytes.len(), 179);
        let scene = Document::load(SOURCE, &schema()).unwrap();
        assert_eq!(scene.encode().unwrap(), bytes);
        let decoded = SceneData::decode(&bytes, &schema()).unwrap();
        assert_eq!(decoded.encode().unwrap(), bytes);
        let data = decoded.node("n").unwrap().component("data").unwrap();
        assert!(
            matches!(data.get("n"), Some(Value::Number(v)) if v.to_bits() == (-0.0_f64).to_bits())
        );
        assert!(matches!(data.get("t"), Some(Value::Text(v)) if v == "λ🚀"));
        let metadata = SceneData::inspect_cooked(&bytes).unwrap();
        assert_eq!(metadata.format_version(), 1);
        assert_eq!(metadata.node_count(), 1);
        assert_eq!(metadata.byte_len(), 179);
        assert_eq!(metadata.checksum(), 0x53b562460c4c1419);
    }

    #[test]
    fn empty_and_shortest_node_envelopes_are_accepted() {
        for (source, count, len) in [
            ("ferravex_scene 1;", 0, HEADER_BYTES + CHECKSUM_BYTES),
            (
                "ferravex_scene 1; node n {}",
                1,
                HEADER_BYTES + MIN_NODE_BYTES + CHECKSUM_BYTES,
            ),
        ] {
            let bytes = Document::load(source, &Schema::new())
                .unwrap()
                .encode()
                .unwrap();
            assert_eq!(bytes.len(), len);
            assert_eq!(
                SceneData::inspect_cooked(&bytes).unwrap().node_count(),
                count
            );
            assert_eq!(
                SceneData::decode(&bytes, &Schema::new())
                    .unwrap()
                    .nodes()
                    .len(),
                count
            );
        }
    }

    #[test]
    fn every_truncation_is_rejected_even_with_a_recomputed_checksum() {
        let bytes = fixture();
        for end in 0..bytes.len() {
            assert!(
                SceneData::decode(&bytes[..end], &schema()).is_err(),
                "{end}"
            );
        }
        for end in HEADER_BYTES..bytes.len() - CHECKSUM_BYTES {
            let mut truncated = bytes[..end].to_vec();
            truncated.extend([0; CHECKSUM_BYTES]);
            resign(&mut truncated);
            let error = SceneData::decode(&truncated, &schema()).err().unwrap();
            assert_eq!(error.kind, ErrorKind::Format, "at {end}: {error:?}");
        }
    }

    #[test]
    fn counts_are_checked_against_limits_and_remaining_bytes() {
        for (offset, count, kind) in [
            (8, u32::MAX, ErrorKind::Limit),
            (8, MAX_NODES as u32, ErrorKind::Format),
            (57, u16::MAX as u32, ErrorKind::Limit),
            (57, MAX_COMPONENTS as u32, ErrorKind::Format),
            (65, u16::MAX as u32, ErrorKind::Limit),
            (65, MAX_FIELDS as u32, ErrorKind::Format),
        ] {
            let mut bytes = fixture();
            let width = if offset == 8 { 4 } else { 2 };
            bytes[offset..offset + width].copy_from_slice(&count.to_le_bytes()[..width]);
            invalid(bytes, kind, offset);
        }
    }

    #[test]
    fn string_lengths_are_checked_before_reading_or_allocating() {
        let fixture = fixture();
        let text = value_at(&fixture, b't', 3);
        let reference = value_at(&fixture, b'r', 8);
        for offset in [12, 15, 59, 67, text, reference] {
            let mut bytes = fixture.clone();
            bytes[offset..offset + 2].copy_from_slice(&u16::MAX.to_le_bytes());
            invalid(bytes, ErrorKind::Format, offset);
        }
        let mut bytes = fixture;
        bytes[text..text + 2].copy_from_slice(&(MAX_STRING as u16).to_le_bytes());
        let error = invalid(bytes, ErrorKind::Format, text + 2);
        assert_eq!(error.context, "truncated cooked data");
    }

    #[test]
    fn utf8_and_nul_errors_point_to_the_offending_byte_in_each_string_kind() {
        let bytes = fixture();
        let text = value_at(&bytes, b't', 3) + 2;
        let reference = value_at(&bytes, b'r', 8) + 2;
        // The first Unicode scalar remains valid before the malformed suffix.
        for offset in [14, 61, 69, text + "λ".len(), reference] {
            for byte in [0xff, 0] {
                let mut malformed = bytes.clone();
                malformed[offset] = byte;
                if offset == text + "λ".len() && byte == 0 {
                    // Retain valid UTF-8 after the NUL as well.
                    malformed[offset + 1..offset + 4].fill(b'x');
                }
                let error = invalid(malformed, ErrorKind::Format, offset);
                assert_eq!(
                    error.context,
                    if byte == 0 {
                        "NUL in string"
                    } else {
                        "invalid UTF-8"
                    }
                );
            }
        }
        let parent = Document::load(
            "ferravex_scene 1; node n {} node m { parent n; }",
            &Schema::new(),
        )
        .unwrap()
        .encode()
        .unwrap();
        let offset = HEADER_BYTES + MIN_NODE_BYTES + 5;
        for byte in [0xff, 0] {
            let mut malformed = parent.clone();
            malformed[offset] = byte;
            invalid(malformed, ErrorKind::Format, offset);
        }
    }

    #[test]
    fn all_numeric_slots_reject_nan_and_infinities_at_their_start() {
        let bytes = fixture();
        let mut offsets: Vec<_> = (0..10).map(|i| 17 + i * 4).collect();
        for (name, tag, count) in [(b'v', 4, 2), (b'w', 5, 3), (b'q', 6, 4)] {
            let at = value_at(&bytes, name, tag);
            offsets.extend((0..count).map(|i| at + i * 4));
        }
        for offset in offsets {
            for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                let mut malformed = bytes.clone();
                malformed[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
                invalid(malformed, ErrorKind::Format, offset);
            }
        }
        let offset = value_at(&bytes, b'n', 2);
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut malformed = bytes.clone();
            malformed[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
            invalid(malformed, ErrorKind::Format, offset);
        }
    }

    #[test]
    fn duplicate_ids_fail_before_decoding_their_invalid_payloads() {
        let mut nodes = Document::load("ferravex_scene 1; node n {} node m {}", &Schema::new())
            .unwrap()
            .encode()
            .unwrap();
        let second = HEADER_BYTES + MIN_NODE_BYTES;
        nodes[second + 2] = b'n';
        nodes[second + 5..second + 9].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(invalid(nodes, ErrorKind::Duplicate, second).context, "n");

        let mut schema = Schema::new();
        schema.register("a", &[]).unwrap();
        schema.register("b", &[]).unwrap();
        let mut components = Document::load(
            "ferravex_scene 1; node n { component a {} component b {} }",
            &schema,
        )
        .unwrap()
        .encode()
        .unwrap();
        let second = HEADER_BYTES + MIN_NODE_BYTES + MIN_COMPONENT_BYTES;
        components[second + 2] = b'a';
        components[second + 3..second + 5].copy_from_slice(&u16::MAX.to_le_bytes());
        invalid(components, ErrorKind::Duplicate, second);

        let mut fields = fixture();
        let second = value_at(&fields, b'c', 7) - 4;
        fields[second + 2] = b'b';
        fields[second + 3] = 0xff;
        invalid(fields, ErrorKind::Duplicate, second);
    }

    #[test]
    fn invalid_identifiers_and_tags_have_binary_locations() {
        for (offset, kind) in [
            (12, ErrorKind::Reference),
            (59, ErrorKind::Format),
            (67, ErrorKind::Format),
        ] {
            let mut bytes = fixture();
            bytes[offset + 2] = b'/';
            invalid(bytes, kind, offset);
        }
        let bytes = fixture();
        let boolean = value_at(&bytes, b'b', 0);
        for (offset, byte, context) in [
            (boolean, 2, "invalid bool"),
            (boolean - 1, 255, "unknown value tag"),
        ] {
            let mut malformed = bytes.clone();
            malformed[offset] = byte;
            assert_eq!(
                invalid(malformed, ErrorKind::Format, offset).context,
                context
            );
        }
    }

    #[test]
    fn metadata_checks_envelope_without_claiming_body_or_schema_validity() {
        let bytes = fixture();
        assert_eq!(
            SceneData::decode(&bytes, &Schema::new())
                .err()
                .unwrap()
                .kind,
            ErrorKind::UnknownComponent
        );
        assert_eq!(SceneData::inspect_cooked(&bytes).unwrap().node_count(), 1);
        let tag = value_at(&bytes, b'b', 0) - 1;
        let mut malformed = bytes.clone();
        malformed[tag] = 0xff;
        resign(&mut malformed);
        assert!(SceneData::inspect_cooked(&malformed).is_ok());
        assert!(SceneData::decode(&malformed, &schema()).is_err());
        let end = bytes.len() - CHECKSUM_BYTES;
        let mut trailing = bytes;
        trailing.insert(end, 0);
        resign(&mut trailing);
        assert_eq!(
            SceneData::inspect_cooked(&trailing).unwrap().byte_len(),
            trailing.len()
        );
        assert_eq!(
            invalid(trailing, ErrorKind::Format, end).context,
            "trailing cooked data"
        );
    }

    #[test]
    fn header_checksum_and_size_validation_are_shared() {
        for malformed in [
            Vec::new(),
            b"FXSC0001".to_vec(),
            vec![0; HEADER_BYTES + CHECKSUM_BYTES],
            vec![0; MAX_COOKED_BYTES + 1],
        ] {
            let inspect = SceneData::inspect_cooked(&malformed).unwrap_err();
            assert_eq!(inspect.kind, ErrorKind::Format);
            assert_eq!(
                SceneData::decode(&malformed, &schema()).err().unwrap(),
                inspect
            );
        }
        let mut bytes = fixture();
        bytes[7] = b'2';
        resign(&mut bytes);
        assert_eq!(
            SceneData::inspect_cooked(&bytes).unwrap_err().kind,
            ErrorKind::Format
        );
        let mut bytes = fixture();
        bytes[14] ^= 1;
        let inspect = SceneData::inspect_cooked(&bytes).unwrap_err();
        assert_eq!(inspect.kind, ErrorKind::Checksum);
        assert_eq!(inspect.span.offset, bytes.len() - CHECKSUM_BYTES);
        assert_eq!(SceneData::decode(&bytes, &schema()).err().unwrap(), inspect);
    }

    #[test]
    fn finite_but_invalid_semantics_still_reach_scene_validation() {
        let bytes = fixture();
        let mut quaternion = bytes.clone();
        let at = value_at(&bytes, b'q', 6);
        quaternion[at..at + 16].fill(0);
        invalid(quaternion, ErrorKind::Range, at - 4);
        let mut transform = bytes.clone();
        transform[29..45].fill(0);
        invalid(transform, ErrorKind::InvalidTransform, HEADER_BYTES);
        let mut reference = bytes.clone();
        let at = value_at(&bytes, b'r', 8);
        reference[at + 2] = b'x';
        invalid(reference, ErrorKind::Reference, at - 4);
    }

    #[test]
    fn utf8_text_at_the_byte_limit_roundtrips() {
        let text = "λ".repeat(MAX_STRING / "λ".len());
        let source = SOURCE.replace("λ🚀", &text);
        let bytes = Document::load(&source, &schema())
            .unwrap()
            .encode()
            .unwrap();
        assert_eq!(
            SceneData::decode(&bytes, &schema())
                .unwrap()
                .encode()
                .unwrap(),
            bytes
        );
    }

    #[test]
    fn aggregate_value_budget_accepts_the_boundary_and_rejects_one_more() {
        let mut schema = Schema::new();
        let rules: Vec<_> = (0..MAX_FIELDS)
            .map(|i| FieldRule::optional(&format!("f{i}"), ValueKind::Bool))
            .collect();
        let mut source = String::from("ferravex_scene 1; prefab dense { node root {");
        for i in 0..MAX_COMPONENTS {
            let kind = format!("c{i}");
            schema.register(&kind, &rules).unwrap();
            source.push_str(&format!("component {kind} {{"));
            for j in 0..MAX_FIELDS {
                source.push_str(&format!("f{j} bool true;"));
            }
            source.push('}');
        }
        source.push_str("}}");
        let count = MAX_VALUES / (MAX_COMPONENTS * MAX_FIELDS);
        assert_eq!(count * MAX_COMPONENTS * MAX_FIELDS, MAX_VALUES);
        for i in 0..count {
            source.push_str(&format!("node n{i} {{ use dense; }}"));
        }
        let mut bytes = Document::load(&source, &schema).unwrap().encode().unwrap();
        assert_eq!(
            SceneData::decode(&bytes, &schema)
                .unwrap()
                .encode()
                .unwrap(),
            bytes
        );
        let extra = Document::load(
            "ferravex_scene 1; node extra { component c0 { f0 bool true; } }",
            &schema,
        )
        .unwrap()
        .encode()
        .unwrap();
        let extra_start = bytes.len() - CHECKSUM_BYTES;
        let count_at =
            extra_start + extra.windows(2).position(|w| w == b"c0").unwrap() + 2 - HEADER_BYTES;
        bytes.truncate(extra_start);
        bytes.extend_from_slice(&extra[HEADER_BYTES..extra.len() - CHECKSUM_BYTES]);
        bytes.extend([0; CHECKSUM_BYTES]);
        bytes[8..12].copy_from_slice(&((count + 1) as u32).to_le_bytes());
        assert_eq!(
            invalid(bytes, ErrorKind::Limit, count_at).context,
            "too many cooked values"
        );
    }

    #[test]
    fn checked_reader_overflow_does_not_advance() {
        let mut reader = Reader { bytes: &[0], at: 1 };
        let error = reader.take(usize::MAX).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Format);
        assert_eq!(error.span.offset, 1);
        assert_eq!(reader.at, 1);
    }
}
