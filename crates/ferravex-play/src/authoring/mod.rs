//! First-party typed scene authoring, hierarchical prefab expansion and compact
//! cooked data. Parsing/validation happens on load, never during scene traversal.
mod cooked;
mod expand;
mod parser;
mod schema;
use crate::scene::{Matrix, NodeId, Scene, Transform};
pub use cooked::CookedMetadata;
pub use schema::*;
use std::{collections::BTreeMap, fmt};
pub const MAX_SOURCE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_COOKED_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_NODES: usize = 4096;
pub const MAX_PREFABS: usize = 256;
pub const MAX_COMPONENTS: usize = 32;
pub const MAX_FIELDS: usize = 64;
pub const MAX_VALUES: usize = 65536;
pub const MAX_STRING: usize = 1024;
pub const MAX_PREFAB_DEPTH: usize = 16;
pub const MAX_HIERARCHY_DEPTH: usize = 128;
/// A source position: one-based line and Unicode scalar column, and a zero-based
/// UTF-8 byte offset. A zero line denotes an error without a text location (for
/// example, a cooked-data or schema-registration error).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceSpan {
    pub line: usize,
    pub column: usize,
    pub offset: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Syntax,
    Version,
    Limit,
    Duplicate,
    UnknownPrefab,
    PrefabCycle,
    MissingNode,
    ParentCycle,
    InvalidTransform,
    UnknownComponent,
    UnknownField,
    MissingField,
    Type,
    Range,
    Reference,
    Format,
    Checksum,
    Domain,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentError {
    pub kind: ErrorKind,
    pub span: SourceSpan,
    pub context: String,
}
impl ContentError {
    /// Attach a source excerpt and offending token without changing this error.
    /// Supply the original source; inconsistent or non-text positions produce no
    /// excerpt. Tabs count as one column, as do individual Unicode scalars.
    ///
    /// ```
    /// use ferravex_play::authoring::Document;
    /// let source = "ferravex_scene 9;";
    /// let error = Document::parse(source).unwrap_err();
    /// let diagnostic = error.diagnostic(source);
    /// assert_eq!(diagnostic.token.as_deref(), Some("9"));
    /// assert_eq!(diagnostic.error, error);
    /// assert!(diagnostic.to_string().contains("expected authoring version 1"));
    /// ```
    pub fn diagnostic(&self, source: &str) -> SceneDiagnostic {
        parser::diagnostic(source, self.clone())
    }

    pub fn domain(context: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Domain,
            span: SourceSpan::default(),
            context: context.into(),
        }
    }
    pub(crate) fn new(kind: ErrorKind, span: SourceSpan, context: impl Into<String>) -> Self {
        Self {
            kind,
            span,
            context: context.into(),
        }
    }
}
impl fmt::Display for ContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.span.line > 0 {
            write!(f, "{}:{}: ", self.span.line, self.span.column)?;
        }
        write!(f, "{:?}: {}", self.kind, self.context)
    }
}
impl std::error::Error for ContentError {}

/// An owned, source-aware view of the first scene error. Existing error kinds,
/// contexts and positions remain available through `error`. Display includes a
/// caret excerpt and any repair guidance, suitable for an editor or CLI.
///
/// ```
/// use ferravex_play::authoring::Document;
/// let error = Document::parse_with_diagnostics("ferravex_scene 2;").unwrap_err();
/// assert_eq!(error.error.span.column, 16);
/// assert_eq!(error.source_line.as_deref(), Some("ferravex_scene 2;"));
/// println!("{error}");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneDiagnostic {
    /// The unchanged error returned by the compatible parsing/loading API.
    pub error: ContentError,
    /// Raw source spelling, including quotes and escapes. None at end of input
    /// or when the error has no matching source position. Malformed strings are
    /// clipped at the first line break.
    pub token: Option<Box<str>>,
    /// The original line without its LF/CRLF terminator, if the position matches.
    pub source_line: Option<Box<str>>,
    /// A concrete repair hint, including schema requirements when available.
    pub help: Option<Box<str>>,
}
impl fmt::Display for SceneDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.error)?;
        if let Some(token) = &self.token {
            write!(f, "; found {token:?}")?;
        } else if self.source_line.is_some() {
            write!(f, "; found end of input")?;
        }
        if let Some(line) = &self.source_line {
            let number = self.error.span.line;
            let width = number.to_string().len();
            write!(
                f,
                "\n{empty:width$} |\n{number} | {line}\n{empty:width$} | ",
                empty = ""
            )?;
            for c in line.chars().take(self.error.span.column.saturating_sub(1)) {
                f.write_str(if c == '\t' { "\t" } else { " " })?;
            }
            let width = self.token.as_ref().map_or(1, |t| t.chars().count().max(1));
            for _ in 0..width {
                f.write_str("^")?;
            }
        }
        if let Some(help) = &self.help {
            write!(f, "\nhelp: {help}")?;
        }
        Ok(())
    }
}
impl std::error::Error for SceneDiagnostic {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Integer(i64),
    Number(f64),
    Text(String),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Quaternion([f32; 4]),
    Color([u8; 4]),
    Reference(String),
}
impl Value {
    pub fn boolean(&self) -> Option<bool> {
        if let Self::Bool(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    pub fn integer(&self) -> Option<i64> {
        if let Self::Integer(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    pub fn number(&self) -> Option<f64> {
        if let Self::Number(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    pub fn text(&self) -> Option<&str> {
        if let Self::Text(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn vec2(&self) -> Option<[f32; 2]> {
        if let Self::Vec2(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    pub fn vec3(&self) -> Option<[f32; 3]> {
        if let Self::Vec3(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    pub fn quaternion(&self) -> Option<[f32; 4]> {
        if let Self::Quaternion(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    pub fn color(&self) -> Option<[u8; 4]> {
        if let Self::Color(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    pub fn reference(&self) -> Option<&str> {
        if let Self::Reference(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub(crate) fn size(&self) -> usize {
        match self {
            Self::Bool(_) => 1,
            Self::Integer(_) | Self::Number(_) => 8,
            Self::Text(v) | Self::Reference(v) => v.len() + 2,
            Self::Vec2(_) => 8,
            Self::Vec3(_) => 12,
            Self::Quaternion(_) => 16,
            Self::Color(_) => 4,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Property {
    pub value: Value,
    pub span: SourceSpan,
}
#[derive(Clone, Debug, Default)]
pub struct Component {
    pub(crate) fields: BTreeMap<String, Property>,
    pub(crate) span: SourceSpan,
}
impl Component {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.fields.get(name).map(|p| &p.value)
    }
    pub fn fields(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.fields.iter().map(|(k, v)| (k.as_str(), &v.value))
    }
    pub fn required(&self, name: &str) -> Result<&Value, ContentError> {
        self.get(name)
            .ok_or_else(|| ContentError::new(ErrorKind::MissingField, self.span, name))
    }
}
#[derive(Clone, Debug)]
pub(crate) struct NodeSpec {
    pub id: String,
    pub span: SourceSpan,
    pub parent: Option<String>,
    pub prefab: Option<String>,
    pub local: Option<Transform>,
    pub components: BTreeMap<String, Component>,
    pub overrides: BTreeMap<String, NodeOverride>,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct NodeOverride {
    pub local: Option<Transform>,
    pub components: BTreeMap<String, Component>,
}
#[derive(Clone, Debug)]
pub struct Document {
    pub(crate) nodes: Vec<NodeSpec>,
    pub(crate) prefabs: BTreeMap<String, Vec<NodeSpec>>,
}
impl Document {
    pub fn parse(source: &str) -> Result<Self, ContentError> {
        parser::parse(source)
    }
    /// Parse with an offending token, source excerpt and repair guidance.
    /// Reports the same first error as [`Self::parse`]; schema validation requires
    /// [`Self::load_with_diagnostics`]. No source is retained on success.
    ///
    /// ```
    /// use ferravex_play::authoring::{Document, ErrorKind};
    /// let source = "ferravex_scene 1;\nnode x { component body { mass number NaN; } }";
    /// let error = Document::parse_with_diagnostics(source).unwrap_err();
    /// assert_eq!(error.error.kind, ErrorKind::Range);
    /// assert_eq!(error.error.span.line, 2);
    /// assert_eq!(error.token.as_deref(), Some("NaN"));
    /// ```
    pub fn parse_with_diagnostics(source: &str) -> Result<Self, SceneDiagnostic> {
        Self::parse(source).map_err(|error| parser::diagnostic(source, error))
    }
    pub fn compile(&self, schema: &Schema) -> Result<SceneData, ContentError> {
        expand::compile(self, schema)
    }
    pub fn load(source: &str, schema: &Schema) -> Result<SceneData, ContentError> {
        Self::parse(source)?.compile(schema)
    }
    /// Parse, expand and validate with source-aware diagnostics. In addition to
    /// syntax hints, schema errors list allowed fields/components, required value
    /// types and inclusive bounds. Reports the same first error as [`Self::load`]
    /// and produces identical scene/cooked data on success.
    ///
    /// ```
    /// use ferravex_play::authoring::{Document, FieldRule, Schema, ValueKind};
    /// let mut schema = Schema::new();
    /// schema.register("body", &[FieldRule::optional("mass", ValueKind::Number)])?;
    /// let source = "ferravex_scene 1; node x { component body { mas number 1; } }";
    /// let error = Document::load_with_diagnostics(source, &schema).err().unwrap();
    /// assert_eq!(error.token.as_deref(), Some("mas"));
    /// assert!(error.help.as_deref().unwrap().contains("mass"));
    /// # Ok::<(), ferravex_play::authoring::ContentError>(())
    /// ```
    pub fn load_with_diagnostics(
        source: &str,
        schema: &Schema,
    ) -> Result<SceneData, SceneDiagnostic> {
        let document = Self::parse_with_diagnostics(source)?;
        document
            .compile(schema)
            .map_err(|error| document.schema_diagnostic(source, schema, error))
    }

    fn schema_diagnostic(
        &self,
        source: &str,
        schema: &Schema,
        error: ContentError,
    ) -> SceneDiagnostic {
        let mut diagnostic = parser::diagnostic(source, error);
        let error = &diagnostic.error;
        if error.kind == ErrorKind::UnknownComponent {
            diagnostic.help = Some(if schema.components.is_empty() {
                "No components are registered; register this component before loading.".into()
            } else {
                format!(
                    "Use a registered component: {}. Register custom components before loading.",
                    schema
                        .components
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                )
                .into()
            });
        }
        if error.span.line == 0 {
            return diagnostic;
        }
        let components = self
            .nodes
            .iter()
            .chain(self.prefabs.values().flatten())
            .flat_map(|node| {
                node.components.iter().chain(
                    node.overrides
                        .values()
                        .flat_map(|patch| patch.components.iter()),
                )
            });
        for (kind, component) in components {
            let field = component.fields.iter().find(|(_, p)| p.span == error.span);
            if field.is_none() && component.span != error.span {
                continue;
            }
            let Some(rules) = schema.components.get(kind) else {
                break;
            };
            if error.kind == ErrorKind::UnknownField {
                diagnostic.help = Some(if rules.is_empty() {
                    format!("Component `{kind}` accepts no fields; remove this field.").into()
                } else {
                    format!(
                        "Component `{kind}` accepts these fields: {}. Rename or remove this field.",
                        rules.keys().cloned().collect::<Vec<_>>().join(", ")
                    )
                    .into()
                });
            } else if matches!(
                error.kind,
                ErrorKind::Type | ErrorKind::Range | ErrorKind::MissingField | ErrorKind::Reference
            ) {
                let name = field.map_or_else(
                    || error.context.rsplit('.').next().unwrap_or(&error.context),
                    |(name, _)| name.as_str(),
                );
                if let Some(rule) = rules.get(name) {
                    diagnostic.help = Some(field_help(kind, name, rule).into());
                }
            }
            break;
        }
        diagnostic
    }
}

fn field_help(component: &str, name: &str, rule: &FieldRule) -> String {
    let expected = match rule.kind {
        ValueKind::Bool => "bool true or bool false",
        ValueKind::Integer => "int followed by a signed 64-bit integer",
        ValueKind::Number => "number followed by a finite number",
        ValueKind::Text => "text followed by a string without NUL characters",
        ValueKind::Vec2 => "vec2 followed by two finite f32 numbers",
        ValueKind::Vec3 => "vec3 followed by three finite f32 numbers",
        ValueKind::Quaternion => {
            "quat followed by four finite f32 numbers with squared length > 1e-20"
        }
        ValueKind::Color => "rgba followed by four integer channels in 0..255",
        ValueKind::Reference => "ref followed by an existing node path",
    };
    let mut help = format!("Field `{component}.{name}` expects {expected}.");
    if let Some((min, max)) = rule.bounds {
        help.push_str(&format!(
            " Each value must be in the inclusive range {min}..{max}."
        ));
    }
    if let Some((min, max)) = rule.integer_bounds {
        help.push_str(&format!(
            " Use an integer in the inclusive range {min}..{max}."
        ));
    }
    if let Some(target) = &rule.target {
        help.push_str(&format!(" The target node must have component `{target}`."));
    }
    if rule.required {
        help.push_str(" This field is required.");
    }
    help
}
#[derive(Clone, Debug)]
pub struct DataNode {
    pub id: String,
    pub parent: Option<String>,
    pub local: Transform,
    pub world: Matrix,
    pub(crate) components: BTreeMap<String, Component>,
    pub(crate) span: SourceSpan,
}
impl DataNode {
    pub fn component(&self, kind: &str) -> Option<&Component> {
        self.components.get(kind)
    }
    pub fn components(&self) -> impl Iterator<Item = (&str, &Component)> {
        self.components.iter().map(|(k, v)| (k.as_str(), v))
    }
    pub fn position(&self) -> [f32; 3] {
        [self.world[12], self.world[13], self.world[14]]
    }
}
/// Owns a scene hierarchy and validated typed data. Handles belong to this
/// generation only; retain the owner while holding handles/components.
pub struct SceneData {
    nodes: Vec<DataNode>,
    lookup: BTreeMap<String, usize>,
    hierarchy: Scene,
    handles: Vec<NodeId>,
}
impl SceneData {
    pub fn nodes(&self) -> &[DataNode] {
        &self.nodes
    }
    pub fn node(&self, id: &str) -> Option<&DataNode> {
        self.lookup.get(id).map(|i| &self.nodes[*i])
    }
    pub fn of_kind<'a>(
        &'a self,
        kind: &'a str,
    ) -> impl Iterator<Item = (&'a DataNode, &'a Component)> {
        self.nodes
            .iter()
            .filter_map(move |n| n.component(kind).map(|c| (n, c)))
    }
    pub fn hierarchy(&self) -> &Scene {
        &self.hierarchy
    }
    pub fn handle(&self, id: &str) -> Option<NodeId> {
        self.lookup.get(id).map(|i| self.handles[*i])
    }
    pub fn encode(&self) -> Result<Vec<u8>, ContentError> {
        cooked::encode(self)
    }
    pub fn decode(bytes: &[u8], schema: &Schema) -> Result<Self, ContentError> {
        cooked::decode(bytes, schema)
    }
}
pub(crate) fn identifier(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}
pub(crate) fn path(id: &str) -> bool {
    !id.is_empty() && id.len() <= 512 && id.split('/').all(identifier)
}
