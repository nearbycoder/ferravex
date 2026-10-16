use super::*;
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
}
#[derive(Default)]
pub struct Schema {
    pub(crate) components: BTreeMap<String, BTreeMap<String, FieldRule>>,
}
impl Schema {
    pub fn new() -> Self {
        Self::default()
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
