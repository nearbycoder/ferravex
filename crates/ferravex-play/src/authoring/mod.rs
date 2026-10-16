//! First-party typed scene authoring, hierarchical prefab expansion and compact
//! cooked data. Parsing/validation happens on load, never during scene traversal.
mod cooked;
mod expand;
mod parser;
mod schema;
use crate::scene::{Matrix, NodeId, Scene, Transform};
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
    pub fn compile(&self, schema: &Schema) -> Result<SceneData, ContentError> {
        expand::compile(self, schema)
    }
    pub fn load(source: &str, schema: &Schema) -> Result<SceneData, ContentError> {
        Self::parse(source)?.compile(schema)
    }
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
