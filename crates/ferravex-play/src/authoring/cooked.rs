use super::*;
const MAGIC: &[u8; 8] = b"FXSC0001";
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
impl Reader<'_> {
    fn span(&self) -> SourceSpan {
        SourceSpan {
            offset: self.at,
            ..Default::default()
        }
    }
    fn error(&self, message: &str) -> ContentError {
        ContentError::new(ErrorKind::Format, self.span(), message)
    }
    fn take(&mut self, len: usize) -> Result<&[u8], ContentError> {
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
    fn string(&mut self, limit: usize) -> Result<String, ContentError> {
        let len = self.u16()?;
        if len > limit {
            return Err(self.error("string exceeds limit"));
        }
        let at = self.span();
        let value = std::str::from_utf8(self.take(len)?)
            .map_err(|_| ContentError::new(ErrorKind::Format, at, "invalid UTF-8"))?;
        if value.contains('\0') {
            return Err(self.error("NUL in string"));
        }
        Ok(value.into())
    }
    fn floats<const N: usize>(&mut self) -> Result<[f32; N], ContentError> {
        let mut v = [0.; N];
        for n in &mut v {
            *n = f32::from_le_bytes(self.take(4)?.try_into().unwrap());
            if !n.is_finite() {
                return Err(self.error("non-finite float"));
            }
        }
        Ok(v)
    }
}
pub(super) fn decode(bytes: &[u8], schema: &Schema) -> Result<SceneData, ContentError> {
    let fail = |kind, message: &str| ContentError::new(kind, SourceSpan::default(), message);
    if bytes.len() < 20 || bytes.len() > MAX_COOKED_BYTES || &bytes[..8] != MAGIC {
        return Err(fail(ErrorKind::Format, "invalid cooked header/size"));
    }
    let end = bytes.len() - 8;
    if checksum(&bytes[..end]) != u64::from_le_bytes(bytes[end..].try_into().unwrap()) {
        return Err(fail(ErrorKind::Checksum, "cooked checksum mismatch"));
    }
    let mut reader = Reader {
        bytes: &bytes[..end],
        at: 8,
    };
    let count = u32::from_le_bytes(reader.take(4)?.try_into().unwrap()) as usize;
    if count > MAX_NODES {
        return Err(fail(ErrorKind::Limit, "too many cooked nodes"));
    }
    let mut nodes = Vec::with_capacity(count.min(64));
    let mut values = 0;
    for _ in 0..count {
        let span = reader.span();
        let id = reader.string(512)?;
        let parent = reader.string(512)?;
        let local = Transform {
            translation: reader.floats()?,
            rotation: reader.floats()?,
            scale: reader.floats()?,
        };
        let count = reader.u16()?;
        if count > MAX_COMPONENTS {
            return Err(fail(ErrorKind::Limit, "too many cooked components"));
        }
        let mut components = BTreeMap::new();
        for _ in 0..count {
            let component_span = reader.span();
            let kind = reader.string(64)?;
            if !identifier(&kind) {
                return Err(reader.error("invalid component identifier"));
            }
            let count = reader.u16()?;
            if count > MAX_FIELDS {
                return Err(fail(ErrorKind::Limit, "too many cooked fields"));
            }
            let mut fields = BTreeMap::new();
            for _ in 0..count {
                values += 1;
                if values > MAX_VALUES {
                    return Err(fail(ErrorKind::Limit, "too many cooked values"));
                }
                let span = reader.span();
                let name = reader.string(64)?;
                if !identifier(&name) {
                    return Err(reader.error("invalid field identifier"));
                }
                let value = match reader.u8()? {
                    0 => Value::Bool(match reader.u8()? {
                        0 => false,
                        1 => true,
                        _ => return Err(reader.error("invalid bool")),
                    }),
                    1 => Value::Integer(i64::from_le_bytes(reader.take(8)?.try_into().unwrap())),
                    2 => {
                        let value = f64::from_le_bytes(reader.take(8)?.try_into().unwrap());
                        if !value.is_finite() {
                            return Err(reader.error("non-finite number"));
                        }
                        Value::Number(value)
                    }
                    3 => Value::Text(reader.string(MAX_STRING)?),
                    4 => Value::Vec2(reader.floats()?),
                    5 => Value::Vec3(reader.floats()?),
                    6 => Value::Quaternion(reader.floats()?),
                    7 => Value::Color(reader.take(4)?.try_into().unwrap()),
                    8 => Value::Reference(reader.string(512)?),
                    _ => return Err(reader.error("unknown value tag")),
                };
                if fields.insert(name, Property { value, span }).is_some() {
                    return Err(ContentError::new(
                        ErrorKind::Duplicate,
                        span,
                        "cooked property",
                    ));
                }
            }
            if components
                .insert(
                    kind,
                    Component {
                        fields,
                        span: component_span,
                    },
                )
                .is_some()
            {
                return Err(ContentError::new(
                    ErrorKind::Duplicate,
                    component_span,
                    "cooked component",
                ));
            }
        }
        nodes.push(DataNode {
            id,
            parent: (!parent.is_empty()).then_some(parent),
            local,
            world: crate::scene::IDENTITY,
            components,
            span,
        });
    }
    if reader.at != end {
        return Err(reader.error("trailing cooked data"));
    }
    expand::finish(nodes, schema)
}
