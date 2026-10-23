use super::*;

/// A scene value's type, including its built-in validation constraints.
///
/// Numbers must be finite; vector and quaternion elements are finite `f32`s.
/// Quaternions must have squared length greater than `1e-20` (they need not be
/// normalized). Text is UTF-8, at most [`MAX_STRING`] bytes, and excludes NUL.
/// Colors have four `u8` channels. References must resolve to a concrete node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    Bool,
    Integer,
    Number,
    Text,
    Vec2,
    Vec3,
    Quaternion,
    Color,
    Reference,
}
impl ValueKind {
    /// The type keyword accepted by the scene text parser. These names also
    /// identify types in [`Schema::write_json`]; they are not Rust debug names.
    ///
    /// ```
    /// use ferravex_play::authoring::ValueKind;
    /// assert_eq!(ValueKind::Integer.keyword(), "int");
    /// assert_eq!(ValueKind::Quaternion.keyword(), "quat");
    /// assert_eq!(ValueKind::Color.keyword(), "rgba");
    /// assert_eq!(ValueKind::Reference.keyword(), "ref");
    /// ```
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Integer => "int",
            Self::Number => "number",
            Self::Text => "text",
            Self::Vec2 => "vec2",
            Self::Vec3 => "vec3",
            Self::Quaternion => "quat",
            Self::Color => "rgba",
            Self::Reference => "ref",
        }
    }
}

/// A field's registration rule and read-only introspection view.
///
/// Builders are checked by [`Schema::register`]. Rules obtained from a schema
/// are already validated; inspection borrows the registered data without cloning.
///
/// ```
/// use ferravex_play::authoring::{FieldRule, Schema, ValueKind};
/// let mut schema = Schema::new();
/// schema.register("body", &[
///     FieldRule::required("mass", ValueKind::Number).range(0.0, 100.0),
///     FieldRule::optional("count", ValueKind::Integer).integer_range(0, i64::MAX),
///     FieldRule::optional("target", ValueKind::Reference).target("body"),
/// ])?;
/// let mass = schema.field("body", "mass").unwrap();
/// assert_eq!(mass.name(), "mass");
/// assert_eq!(mass.kind(), ValueKind::Number);
/// assert!(mass.is_required());
/// assert_eq!(mass.number_bounds(), Some((0.0, 100.0)));
/// assert_eq!(mass.default_value(), None);
/// assert_eq!(schema.field("body", "count").unwrap().integer_bounds(), Some((0, i64::MAX)));
/// assert_eq!(schema.field("body", "target").unwrap().target_component(), Some("body"));
/// # Ok::<(), ferravex_play::authoring::ContentError>(())
/// ```
#[derive(Clone, Debug)]
pub struct FieldRule {
    pub(crate) name: String,
    pub(crate) kind: ValueKind,
    pub(crate) required: bool,
    pub(crate) bounds: Option<(f64, f64)>,
    pub(crate) integer_bounds: Option<(i64, i64)>,
    pub(crate) target: Option<String>,
}
impl FieldRule {
    pub fn required(name: &str, kind: ValueKind) -> Self {
        Self {
            name: name.into(),
            kind,
            required: true,
            bounds: None,
            integer_bounds: None,
            target: None,
        }
    }
    pub fn optional(name: &str, kind: ValueKind) -> Self {
        Self {
            required: false,
            ..Self::required(name, kind)
        }
    }
    pub fn range(mut self, min: f64, max: f64) -> Self {
        self.bounds = Some((min, max));
        self
    }
    pub fn integer_range(mut self, min: i64, max: i64) -> Self {
        self.integer_bounds = Some((min, max));
        self
    }
    pub fn target(mut self, component: &str) -> Self {
        self.target = Some(component.into());
        self
    }

    /// The case-sensitive authored field name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The exact value type; integers and numbers are not interchangeable.
    pub fn kind(&self) -> ValueKind {
        self.kind
    }

    /// Whether the field must exist on each concrete instance of its component.
    /// Prefab definitions may omit it until instance expansion supplies it.
    pub fn is_required(&self) -> bool {
        self.required
    }

    /// Explicit inclusive bounds for a number or each vector/quaternion element.
    /// `None` adds no range restriction beyond the type's built-in constraints.
    pub fn number_bounds(&self) -> Option<(f64, f64)> {
        self.bounds
    }

    /// Explicit inclusive integer bounds, preserving all 64 bits of precision.
    /// `None` permits the entire `i64` range.
    pub fn integer_bounds(&self) -> Option<(i64, i64)> {
        self.integer_bounds
    }

    /// The component a referenced node must contain, if one was specified.
    /// References must still resolve when this returns `None`. A target's
    /// component need not have been registered before this field's component.
    pub fn target_component(&self) -> Option<&str> {
        self.target.as_deref()
    }

    /// The schema-provided default, currently always `None`.
    ///
    /// Schema rules do not supply values: omitted optional fields remain absent,
    /// and omitted required fields fail validation after prefab expansion.
    /// Prefab values and application fallbacks are outside the schema; this
    /// method does not guess a zero, identity or other type-based default.
    pub fn default_value(&self) -> Option<&Value> {
        None
    }
}
#[derive(Default)]
pub struct Schema {
    pub(crate) components: BTreeMap<String, BTreeMap<String, FieldRule>>,
}
impl Schema {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registered component names in ascending, case-sensitive lexical order,
    /// independent of registration order. The iterator does not allocate.
    ///
    /// ```
    /// use ferravex_play::authoring::Schema;
    /// let mut schema = Schema::new();
    /// schema.register("zebra", &[])?;
    /// schema.register("actor", &[])?;
    /// assert_eq!(schema.component_names().collect::<Vec<_>>(), ["actor", "zebra"]);
    /// # Ok::<(), ferravex_play::authoring::ContentError>(())
    /// ```
    pub fn component_names(&self) -> impl ExactSizeIterator<Item = &str> + DoubleEndedIterator {
        self.components.keys().map(String::as_str)
    }

    /// Borrow a component's rules in ascending field-name order without
    /// allocation. `None` means an unknown component; an empty iterator means a
    /// registered component with no fields. Inspection never mutates the rules.
    ///
    /// ```
    /// use ferravex_play::authoring::{FieldRule, Schema, ValueKind};
    /// let mut schema = Schema::new();
    /// schema.register("body", &[
    ///     FieldRule::optional("size", ValueKind::Vec3),
    ///     FieldRule::required("mass", ValueKind::Number),
    /// ])?;
    /// let names = schema.fields("body").unwrap().map(FieldRule::name).collect::<Vec<_>>();
    /// assert_eq!(names, ["mass", "size"]);
    /// assert!(schema.fields("unknown").is_none());
    /// assert!(schema.field("body", "mass").unwrap().is_required());
    /// assert!(schema.field("body", "missing").is_none());
    /// # Ok::<(), ferravex_play::authoring::ContentError>(())
    /// ```
    pub fn fields(
        &self,
        component: &str,
    ) -> Option<impl ExactSizeIterator<Item = &FieldRule> + DoubleEndedIterator> {
        self.components.get(component).map(|fields| fields.values())
    }

    /// Borrow a validated rule, or return `None` for an unknown component/field.
    /// See [`Self::fields`] for an inspection example.
    pub fn field(&self, component: &str, field: &str) -> Option<&FieldRule> {
        self.components.get(component)?.get(field)
    }

    /// Stream deterministic UTF-8 JSON describing registered component rules.
    ///
    /// This is a separate introspection format, `ferravex_scene_schema` version
    /// `1`, not a scene document, cooked data, or the JSON Schema standard. It
    /// does not change either existing scene format. Components and fields are
    /// arrays sorted by name; object keys have a fixed order. Empty components
    /// are included, and unknown components/fields remain errors on load.
    ///
    /// `types` describes built-in value constraints. Each field exports `name`,
    /// `type` (its [`ValueKind::keyword`]), `required`, `default`, `number_bounds`,
    /// `integer_bounds`, and `target_component`. Bounds are inclusive two-element
    /// arrays, or `null` if no explicit bounds were registered. Number bounds
    /// apply to each vector/quaternion element as well as scalar numbers.
    /// **Integer bounds are decimal strings**, including the built-in `i64`
    /// limits, so consumers do not round them through an IEEE-754 JSON number.
    /// Every `default` is `null`: the schema never inserts a value. Prefab values
    /// and application fallbacks cannot be inferred from component rules.
    ///
    /// Output uses bounded schema storage directly, without an intermediate
    /// document or output buffer. Writer errors propagate immediately; on error
    /// the writer may contain a partial document, and the schema is unchanged.
    /// The caller controls flushing and any output-storage limit.
    ///
    /// ```
    /// use ferravex_play::authoring::{FieldRule, Schema, ValueKind};
    /// let mut schema = Schema::new();
    /// schema.register("counter", &[
    ///     FieldRule::required("value", ValueKind::Integer).integer_range(0, i64::MAX),
    /// ])?;
    /// let mut bytes = Vec::new();
    /// schema.write_json(&mut bytes)?;
    /// let json = String::from_utf8(bytes)?;
    /// assert!(json.contains(r#""integer_bounds":["0","9223372036854775807"]"#));
    /// assert!(json.contains(r#""default":null"#));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn write_json<W: std::io::Write + ?Sized>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(
            concat!(
                "{\"format\":\"ferravex_scene_schema\",\"version\":1,",
                "\"unknown_components\":\"reject\",\"unknown_fields\":\"reject\",",
                "\"types\":{\"bool\":{\"storage\":\"bool\"},",
            )
            .as_bytes(),
        )?;
        write!(
            writer,
            "\"int\":{{\"storage\":\"i64\",\"minimum\":\"{}\",\"maximum\":\"{}\"}},",
            i64::MIN,
            i64::MAX,
        )?;
        writer.write_all(b"\"number\":{\"storage\":\"f64\",\"finite\":true},")?;
        write!(
            writer,
            "\"text\":{{\"storage\":\"utf8\",\"max_bytes\":{MAX_STRING},\"allow_nul\":false}},",
        )?;
        writer.write_all(
            concat!(
                "\"vec2\":{\"storage\":\"f32\",\"length\":2,\"finite\":true},",
                "\"vec3\":{\"storage\":\"f32\",\"length\":3,\"finite\":true},",
                "\"quat\":{\"storage\":\"f32\",\"length\":4,\"finite\":true,",
                "\"squared_length_exclusive_minimum\":1e-20},",
                "\"rgba\":{\"storage\":\"u8\",\"length\":4,\"minimum\":0,\"maximum\":255},",
                "\"ref\":{\"storage\":\"node_path\",\"must_resolve\":true}},\"components\":[",
            )
            .as_bytes(),
        )?;
        for (component_index, (name, fields)) in self.components.iter().enumerate() {
            if component_index != 0 {
                writer.write_all(b",")?;
            }
            // Registration admits only ASCII alphanumerics, '_' and '-'. All
            // emitted names/targets are validated identifiers, not arbitrary
            // strings requiring JSON escaping.
            write!(writer, "{{\"name\":\"{name}\",\"fields\":[")?;
            for (field_index, field) in fields.values().enumerate() {
                if field_index != 0 {
                    writer.write_all(b",")?;
                }
                write!(
                    writer,
                    "{{\"name\":\"{}\",\"type\":\"{}\",\"required\":{},\"default\":null,\"number_bounds\":",
                    field.name(),
                    field.kind().keyword(),
                    field.is_required(),
                )?;
                match field.number_bounds() {
                    Some((min, max)) => write!(writer, "[{min},{max}]")?,
                    None => writer.write_all(b"null")?,
                }
                writer.write_all(b",\"integer_bounds\":")?;
                match field.integer_bounds() {
                    Some((min, max)) => write!(writer, "[\"{min}\",\"{max}\"]")?,
                    None => writer.write_all(b"null")?,
                }
                writer.write_all(b",\"target_component\":")?;
                match field.target_component() {
                    Some(target) => write!(writer, "\"{target}\"")?,
                    None => writer.write_all(b"null")?,
                }
                writer.write_all(b"}")?;
            }
            writer.write_all(b"]}")?;
        }
        writer.write_all(b"]}")
    }

    /// Registration is transactional; unknown components/properties are errors on
    /// load. Integer bounds stay i64, without a lossy floating-point comparison.
    pub fn register(&mut self, name: &str, fields: &[FieldRule]) -> Result<(), ContentError> {
        let fail = || {
            ContentError::new(
                ErrorKind::Domain,
                SourceSpan::default(),
                format!("invalid schema {name}"),
            )
        };
        if !identifier(name)
            || self.components.contains_key(name)
            || self.components.len() >= 256
            || fields.len() > MAX_FIELDS
        {
            return Err(fail());
        }
        let mut rules = BTreeMap::new();
        for f in fields {
            if !identifier(&f.name)
                || rules.contains_key(&f.name)
                || f.bounds.is_some_and(|(a, b)| {
                    !a.is_finite()
                        || !b.is_finite()
                        || a > b
                        || !matches!(
                            f.kind,
                            ValueKind::Number
                                | ValueKind::Vec2
                                | ValueKind::Vec3
                                | ValueKind::Quaternion
                        )
                })
                || f.integer_bounds
                    .is_some_and(|(a, b)| a > b || f.kind != ValueKind::Integer)
                || f.target
                    .as_ref()
                    .is_some_and(|t| f.kind != ValueKind::Reference || !identifier(t))
            {
                return Err(fail());
            }
            rules.insert(f.name.clone(), f.clone());
        }
        self.components.insert(name.into(), rules);
        Ok(())
    }
    pub(crate) fn validate_definition(
        &self,
        components: &BTreeMap<String, Component>,
    ) -> Result<(), ContentError> {
        for (kind, component) in components {
            let rules = self.components.get(kind).ok_or_else(|| {
                ContentError::new(ErrorKind::UnknownComponent, component.span, kind)
            })?;
            for (name, property) in &component.fields {
                let rule = rules.get(name).ok_or_else(|| {
                    ContentError::new(ErrorKind::UnknownField, property.span, name)
                })?;
                if !valid_type(&property.value, rule.kind) {
                    return Err(ContentError::new(ErrorKind::Type, property.span, name));
                }
                let valid = match &property.value {
                    Value::Integer(v) => {
                        rule.integer_bounds.is_none_or(|(a, b)| *v >= a && *v <= b)
                    }
                    Value::Number(v) => {
                        v.is_finite() && rule.bounds.is_none_or(|(a, b)| *v >= a && *v <= b)
                    }
                    Value::Vec2(v) => valid_numbers(v, rule.bounds),
                    Value::Vec3(v) => valid_numbers(v, rule.bounds),
                    Value::Quaternion(v) => {
                        valid_numbers(v, rule.bounds)
                            && v.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() > 1e-20
                    }
                    Value::Text(v) => v.len() <= MAX_STRING && !v.contains('\0'),
                    Value::Reference(v) => {
                        let v = v
                            .strip_prefix('/')
                            .or_else(|| v.strip_prefix('@'))
                            .unwrap_or(v);
                        path(v)
                    }
                    _ => true,
                };
                if !valid {
                    return Err(ContentError::new(ErrorKind::Range, property.span, name));
                }
            }
        }
        Ok(())
    }
    pub(crate) fn validate(
        &self,
        nodes: &[DataNode],
        lookup: &BTreeMap<String, usize>,
    ) -> Result<(), ContentError> {
        let mut total = 0_usize;
        for node in nodes {
            if node.components.len() > MAX_COMPONENTS {
                return Err(ContentError::new(ErrorKind::Limit, node.span, &node.id));
            }
            for (kind, component) in &node.components {
                let rules = self.components.get(kind).ok_or_else(|| {
                    ContentError::new(
                        ErrorKind::UnknownComponent,
                        component.span,
                        format!("{}.{kind}", node.id),
                    )
                })?;
                for (name, property) in &component.fields {
                    total += 1;
                    if total > MAX_VALUES {
                        return Err(ContentError::new(
                            ErrorKind::Limit,
                            property.span,
                            "too many component values",
                        ));
                    }
                    let context = || format!("{}.{kind}.{name}", node.id);
                    let rule = rules.get(name).ok_or_else(|| {
                        ContentError::new(ErrorKind::UnknownField, property.span, context())
                    })?;
                    if !valid_type(&property.value, rule.kind) {
                        return Err(ContentError::new(ErrorKind::Type, property.span, context()));
                    }
                    let within =
                        match &property.value {
                            Value::Integer(v) => {
                                rule.integer_bounds.is_none_or(|(a, b)| *v >= a && *v <= b)
                            }
                            Value::Number(v) => {
                                v.is_finite() && rule.bounds.is_none_or(|(a, b)| *v >= a && *v <= b)
                            }
                            Value::Vec2(v) => valid_numbers(v, rule.bounds),
                            Value::Vec3(v) => valid_numbers(v, rule.bounds),
                            Value::Quaternion(v) => {
                                valid_numbers(v, rule.bounds)
                                    && v.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() > 1e-20
                            }
                            Value::Text(v) => v.len() <= MAX_STRING && !v.contains('\0'),
                            Value::Reference(id) => {
                                let target = lookup.get(id).ok_or_else(|| {
                                    ContentError::new(
                                        ErrorKind::Reference,
                                        property.span,
                                        format!("{} -> {id}", context()),
                                    )
                                })?;
                                if rule.target.as_ref().is_some_and(|kind| {
                                    !nodes[*target].components.contains_key(kind)
                                }) {
                                    return Err(ContentError::new(
                                        ErrorKind::Reference,
                                        property.span,
                                        format!("{} has wrong target component", context()),
                                    ));
                                }
                                true
                            }
                            _ => true,
                        };
                    if !within {
                        return Err(ContentError::new(
                            ErrorKind::Range,
                            property.span,
                            context(),
                        ));
                    }
                }
                for (name, rule) in rules {
                    if rule.required && !component.fields.contains_key(name) {
                        return Err(ContentError::new(
                            ErrorKind::MissingField,
                            component.span,
                            format!("{}.{kind}.{name}", node.id),
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
fn valid_numbers<const N: usize>(values: &[f32; N], bounds: Option<(f64, f64)>) -> bool {
    values.iter().all(|v| {
        v.is_finite() && bounds.is_none_or(|(a, b)| f64::from(*v) >= a && f64::from(*v) <= b)
    })
}
fn valid_type(value: &Value, kind: ValueKind) -> bool {
    matches!(
        (value, kind),
        (Value::Bool(_), ValueKind::Bool)
            | (Value::Integer(_), ValueKind::Integer)
            | (Value::Number(_), ValueKind::Number)
            | (Value::Text(_), ValueKind::Text)
            | (Value::Vec2(_), ValueKind::Vec2)
            | (Value::Vec3(_), ValueKind::Vec3)
            | (Value::Quaternion(_), ValueKind::Quaternion)
            | (Value::Color(_), ValueKind::Color)
            | (Value::Reference(_), ValueKind::Reference)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Write};

    fn json(schema: &Schema) -> String {
        let mut bytes = Vec::new();
        schema.write_json(&mut bytes).unwrap();
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn views_are_borrowed_sorted_and_distinguish_missing_from_empty() {
        let mut schema = Schema::new();
        schema.register("empty", &[]).unwrap();
        schema
            .register(
                "Body",
                &[
                    FieldRule::optional("z", ValueKind::Reference).target("empty"),
                    FieldRule::required("a", ValueKind::Number).range(-2.5, 10.0),
                    FieldRule::optional("count", ValueKind::Integer)
                        .integer_range(i64::MIN, i64::MAX),
                ],
            )
            .unwrap();
        assert_eq!(schema.component_names().len(), 2);
        assert_eq!(
            schema.component_names().collect::<Vec<_>>(),
            ["Body", "empty"]
        );
        assert_eq!(schema.fields("empty").unwrap().len(), 0);
        assert!(schema.fields("body").is_none());
        assert!(schema.field("body", "a").is_none());
        assert!(schema.field("Body", "missing").is_none());
        let fields = schema.fields("Body").unwrap();
        assert_eq!(fields.len(), 3);
        assert_eq!(
            fields.map(FieldRule::name).collect::<Vec<_>>(),
            ["a", "count", "z"]
        );
        let mass = schema.field("Body", "a").unwrap();
        assert!(std::ptr::eq(
            mass,
            schema.fields("Body").unwrap().next().unwrap()
        ));
        assert_eq!(mass.kind(), ValueKind::Number);
        assert!(mass.is_required());
        assert_eq!(mass.number_bounds(), Some((-2.5, 10.0)));
        assert_eq!(mass.integer_bounds(), None);
        assert_eq!(mass.target_component(), None);
        assert_eq!(mass.default_value(), None);
        let count = schema.field("Body", "count").unwrap();
        assert!(!count.is_required());
        assert_eq!(count.integer_bounds(), Some((i64::MIN, i64::MAX)));
        assert_eq!(
            schema.field("Body", "z").unwrap().target_component(),
            Some("empty")
        );
    }

    #[test]
    fn json_order_and_exact_integer_bounds_are_stable() {
        let rules = [
            FieldRule::optional("target", ValueKind::Reference).target("marker"),
            FieldRule::required("count", ValueKind::Integer).integer_range(i64::MIN, i64::MAX),
            FieldRule::optional("mass", ValueKind::Number).range(-2.5, 10.0),
        ];
        let mut first = Schema::new();
        first.register("marker", &[]).unwrap();
        first.register("body", &rules).unwrap();
        let mut second = Schema::new();
        second
            .register("body", &rules.into_iter().rev().collect::<Vec<_>>())
            .unwrap();
        second.register("marker", &[]).unwrap();
        let output = json(&first);
        assert_eq!(output, json(&second));
        assert_eq!(output, json(&first));
        let components = output.split_once("\"components\":").unwrap().1;
        assert_eq!(
            components,
            concat!(
                r#"[{"name":"body","fields":["#,
                r#"{"name":"count","type":"int","required":true,"default":null,"number_bounds":null,"integer_bounds":["-9223372036854775808","9223372036854775807"],"target_component":null},"#,
                r#"{"name":"mass","type":"number","required":false,"default":null,"number_bounds":[-2.5,10],"integer_bounds":null,"target_component":null},"#,
                r#"{"name":"target","type":"ref","required":false,"default":null,"number_bounds":null,"integer_bounds":null,"target_component":"marker"}"#,
                r#"]},{"name":"marker","fields":[]}]}"#,
            )
        );
    }

    #[test]
    fn empty_export_has_versioned_type_constraints_and_no_components() {
        let output = json(&Schema::new());
        assert_eq!(
            output,
            concat!(
                r#"{"format":"ferravex_scene_schema","version":1,"unknown_components":"reject","unknown_fields":"reject","types":{"bool":{"storage":"bool"},"#,
                r#""int":{"storage":"i64","minimum":"-9223372036854775808","maximum":"9223372036854775807"},"#,
                r#""number":{"storage":"f64","finite":true},"text":{"storage":"utf8","max_bytes":1024,"allow_nul":false},"#,
                r#""vec2":{"storage":"f32","length":2,"finite":true},"vec3":{"storage":"f32","length":3,"finite":true},"#,
                r#""quat":{"storage":"f32","length":4,"finite":true,"squared_length_exclusive_minimum":1e-20},"#,
                r#""rgba":{"storage":"u8","length":4,"minimum":0,"maximum":255},"ref":{"storage":"node_path","must_resolve":true}},"components":[]}"#,
            )
        );
    }

    #[test]
    fn every_kind_keyword_loads_and_inspection_preserves_cooked_bytes() {
        let cases = [
            (ValueKind::Bool, "true"),
            (ValueKind::Integer, "9223372036854775807"),
            (ValueKind::Number, "-2.5"),
            (ValueKind::Text, "\"λ\\nhello\""),
            (ValueKind::Vec2, "1 2"),
            (ValueKind::Vec3, "1 2 3"),
            (ValueKind::Quaternion, "0 0 0 2"),
            (ValueKind::Color, "0 1 254 255"),
            (ValueKind::Reference, "/target"),
        ];
        let rules: Vec<_> = cases
            .iter()
            .map(|(kind, _)| FieldRule::required(kind.keyword(), *kind))
            .collect();
        let mut schema = Schema::new();
        schema.register("values", &rules).unwrap();
        let mut source =
            String::from("ferravex_scene 1; node target {} node a { component values {");
        for (kind, literal) in cases {
            let keyword = kind.keyword();
            source.push_str(&format!("{keyword} {keyword} {literal};"));
        }
        source.push_str("} }");
        let scene = Document::load(&source, &schema).unwrap();
        let before = scene.encode().unwrap();
        let output = json(&schema);
        for (kind, _) in cases {
            assert!(output.contains(&format!("\"type\":\"{}\"", kind.keyword())));
            let field = schema.field("values", kind.keyword()).unwrap();
            assert_eq!(field.kind(), kind);
            assert!(valid_type(
                scene
                    .node("a")
                    .unwrap()
                    .component("values")
                    .unwrap()
                    .get(field.name())
                    .unwrap(),
                kind
            ));
        }
        assert_eq!(scene.encode().unwrap(), before);
        assert_eq!(
            SceneData::decode(&before, &schema)
                .unwrap()
                .encode()
                .unwrap(),
            before
        );
        assert_eq!(
            Document::load(&source, &schema).unwrap().encode().unwrap(),
            before
        );
    }

    #[test]
    fn optional_fields_stay_absent_and_prefab_values_are_not_schema_defaults() {
        let mut schema = Schema::new();
        schema
            .register(
                "body",
                &[
                    FieldRule::optional("optional", ValueKind::Bool),
                    FieldRule::required("required", ValueKind::Integer),
                ],
            )
            .unwrap();
        let source = "ferravex_scene 1; prefab p { node root { component body { required int 42; } } } node a { use p; }";
        let before = json(&schema);
        let scene = Document::load(source, &schema).unwrap();
        let component = scene.node("a").unwrap().component("body").unwrap();
        assert_eq!(component.get("optional"), None);
        assert_eq!(component.get("required"), Some(&Value::Integer(42)));
        assert!(
            schema
                .fields("body")
                .unwrap()
                .all(|field| field.default_value().is_none())
        );
        assert_eq!(json(&schema), before);
        let error = Document::load("ferravex_scene 1; node a { component body {} }", &schema)
            .err()
            .unwrap();
        assert_eq!(error.kind, ErrorKind::MissingField);
    }

    #[test]
    fn failed_registration_does_not_publish_unchecked_or_partial_rules() {
        let mut schema = Schema::new();
        schema.register("good", &[]).unwrap();
        let before = json(&schema);
        for rule in [
            FieldRule::required("bad\"name", ValueKind::Bool),
            FieldRule::required("value", ValueKind::Number).range(f64::NAN, 1.0),
            FieldRule::required("value", ValueKind::Number).range(0.0, f64::INFINITY),
            FieldRule::required("value", ValueKind::Number).range(1.0, -1.0),
            FieldRule::required("value", ValueKind::Bool).range(0.0, 1.0),
            FieldRule::required("value", ValueKind::Integer).integer_range(1, 0),
            FieldRule::required("value", ValueKind::Number).integer_range(0, 1),
            FieldRule::required("value", ValueKind::Reference).target("bad\\target"),
        ] {
            assert!(
                schema
                    .register("bad", &[FieldRule::optional("a", ValueKind::Text), rule])
                    .is_err()
            );
            assert!(schema.fields("bad").is_none());
            assert_eq!(json(&schema), before);
        }
        for name in ["", "quote\"", "slash\\", "line\n", "λ"] {
            assert!(schema.register(name, &[]).is_err());
            assert_eq!(json(&schema), before);
        }
        assert!(schema.register("good", &[]).is_err());
        let duplicate = FieldRule::optional("same", ValueKind::Bool);
        assert!(
            schema
                .register("duplicate", &[duplicate.clone(), duplicate])
                .is_err()
        );
        assert_eq!(json(&schema), before);
    }

    #[test]
    fn short_writes_and_writer_errors_are_handled_without_mutation() {
        struct ShortWriter(Vec<u8>);
        impl Write for ShortWriter {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                let count = bytes.len().min(3);
                self.0.extend_from_slice(&bytes[..count]);
                Ok(count)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        struct FailingWriter;
        impl Write for FailingWriter {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "test writer"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut schema = Schema::new();
        schema
            .register(
                "body",
                &[FieldRule::required("mass", ValueKind::Number).range(-f64::MAX, f64::MAX)],
            )
            .unwrap();
        let expected = json(&schema);
        let mut short = ShortWriter(Vec::new());
        let writer: &mut dyn Write = &mut short;
        schema.write_json(writer).unwrap();
        assert_eq!(short.0, expected.as_bytes());
        let error = schema.write_json(&mut FailingWriter).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(error.to_string(), "test writer");
        let mut storage = [0_u8; 20];
        let error = schema.write_json(&mut storage.as_mut_slice()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WriteZero);
        assert_eq!(&storage, &expected.as_bytes()[..20]);
        assert_eq!(json(&schema), expected);
    }

    #[test]
    fn registration_limits_remain_enforced_and_full_schema_can_stream() {
        let rules: Vec<_> = (0..MAX_FIELDS)
            .map(|index| FieldRule::optional(&format!("f{index:03}"), ValueKind::Number))
            .collect();
        let mut schema = Schema::new();
        let mut too_many = rules.clone();
        too_many.push(FieldRule::optional("overflow", ValueKind::Number));
        assert!(schema.register("too_many", &too_many).is_err());
        assert_eq!(schema.component_names().len(), 0);
        for index in (0..256).rev() {
            schema.register(&format!("c{index:03}"), &rules).unwrap();
        }
        assert_eq!(schema.component_names().len(), 256);
        assert!(schema.register("overflow", &[]).is_err());
        assert!(
            schema
                .component_names()
                .all(|name| schema.fields(name).unwrap().len() == MAX_FIELDS)
        );
        schema.write_json(&mut io::sink()).unwrap();
    }
}
