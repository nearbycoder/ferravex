use super::*;
fn qualify(prefix: Option<&str>, value: &str) -> String {
    if let Some(prefix) = prefix {
        if value == "root" {
            prefix.into()
        } else {
            format!("{prefix}/{}", value.strip_prefix("root/").unwrap_or(value))
        }
    } else {
        value.into()
    }
}
struct Expander<'a> {
    doc: &'a Document,
    nodes: Vec<DataNode>,
    bytes: usize,
    values: usize,
}
impl Expander<'_> {
    fn account(&mut self, index: usize, old: (usize, usize)) -> Result<(), ContentError> {
        let n = &self.nodes[index];
        let now = size(n);
        self.bytes = self.bytes - old.0 + now.0;
        self.values = self.values - old.1 + now.1;
        if self.bytes > MAX_COOKED_BYTES
            || self.values > MAX_VALUES
            || n.components.len() > MAX_COMPONENTS
            || n.components.values().any(|c| c.fields.len() > MAX_FIELDS)
        {
            return Err(ContentError::new(
                ErrorKind::Limit,
                n.span,
                "expanded component data exceeds budget",
            ));
        }
        Ok(())
    }
    fn merge(
        &mut self,
        index: usize,
        components: &BTreeMap<String, Component>,
        scope: Option<&str>,
        instance: &str,
    ) -> Result<(), ContentError> {
        let old = size(&self.nodes[index]);
        for (name, incoming) in components {
            let destination = self.nodes[index]
                .components
                .entry(name.clone())
                .or_insert_with(|| Component {
                    fields: BTreeMap::new(),
                    span: incoming.span,
                });
            for (field, p) in &incoming.fields {
                let mut property = p.clone();
                if let Value::Reference(reference) = &mut property.value {
                    let resolved = if let Some(absolute) = reference.strip_prefix('/') {
                        absolute.into()
                    } else if let Some(relative) = reference.strip_prefix('@') {
                        qualify(Some(instance), relative)
                    } else {
                        qualify(scope, reference)
                    };
                    if !path(&resolved) {
                        return Err(ContentError::new(
                            ErrorKind::Reference,
                            p.span,
                            "invalid reference path",
                        ));
                    }
                    *reference = resolved;
                }
                destination.fields.insert(field.clone(), property);
            }
        }
        self.account(index, old)
    }
    fn scope(
        &mut self,
        specs: &[NodeSpec],
        prefix: Option<&str>,
        parent: Option<&str>,
        stack: &mut Vec<String>,
    ) -> Result<(), ContentError> {
        for spec in specs {
            let id = qualify(prefix, &spec.id);
            if !path(&id) {
                return Err(ContentError::new(
                    ErrorKind::Limit,
                    spec.span,
                    "expanded node path exceeds limit",
                ));
            }
            let parent = if prefix.is_some() && spec.id == "root" {
                parent.map(str::to_owned)
            } else if let Some(p) = &spec.parent {
                if p.starts_with('/') && prefix.is_some() {
                    return Err(ContentError::new(
                        ErrorKind::Reference,
                        spec.span,
                        "prefab parent cannot escape its scope",
                    ));
                }
                Some(qualify(prefix, p.strip_prefix('/').unwrap_or(p)))
            } else {
                prefix.map(str::to_owned)
            };
            let index = if let Some(prefab) = &spec.prefab {
                if stack.len() >= MAX_PREFAB_DEPTH {
                    return Err(ContentError::new(
                        ErrorKind::Limit,
                        spec.span,
                        "prefab nesting exceeds 16",
                    ));
                }
                if stack.contains(prefab) {
                    return Err(ContentError::new(ErrorKind::PrefabCycle, spec.span, prefab));
                }
                let definitions = self.doc.prefabs.get(prefab).ok_or_else(|| {
                    ContentError::new(ErrorKind::UnknownPrefab, spec.span, prefab)
                })?;
                let start = self.nodes.len();
                stack.push(prefab.clone());
                self.scope(definitions, Some(&id), parent.as_deref(), stack)?;
                stack.pop();
                let root = (start..self.nodes.len())
                    .find(|i| self.nodes[*i].id == id)
                    .ok_or_else(|| {
                        ContentError::new(
                            ErrorKind::MissingNode,
                            spec.span,
                            "prefab requires root node",
                        )
                    })?;
                if let Some(local) = spec.local {
                    self.nodes[root].local = local;
                }
                self.merge(root, &spec.components, prefix, &id)?;
                for (target, patch) in &spec.overrides {
                    let full = qualify(Some(&id), target);
                    let i = (start..self.nodes.len())
                        .find(|i| self.nodes[*i].id == full)
                        .ok_or_else(|| {
                            ContentError::new(
                                ErrorKind::MissingNode,
                                spec.span,
                                format!("override {full}"),
                            )
                        })?;
                    if let Some(local) = patch.local {
                        self.nodes[i].local = local;
                    }
                    self.merge(i, &patch.components, prefix, &id)?;
                }
                root
            } else {
                if !spec.overrides.is_empty() {
                    return Err(ContentError::new(
                        ErrorKind::Domain,
                        spec.span,
                        "override requires a prefab instance",
                    ));
                }
                if self.nodes.len() >= MAX_NODES {
                    return Err(ContentError::new(
                        ErrorKind::Limit,
                        spec.span,
                        "more than 4096 expanded nodes",
                    ));
                }
                let i = self.nodes.len();
                self.nodes.push(DataNode {
                    id: id.clone(),
                    parent,
                    local: spec.local.unwrap_or_default(),
                    world: crate::scene::IDENTITY,
                    components: BTreeMap::new(),
                    span: spec.span,
                });
                self.account(i, (0, 0))?;
                self.merge(i, &spec.components, prefix, &id)?;
                i
            };
            if !self.nodes[index].local.valid() {
                return Err(ContentError::new(
                    ErrorKind::InvalidTransform,
                    spec.span,
                    &id,
                ));
            }
        }
        Ok(())
    }
}
fn size(node: &DataNode) -> (usize, usize) {
    let mut bytes = 64 + node.id.len() + node.parent.as_ref().map_or(0, String::len);
    let mut values = 0;
    for (kind, c) in &node.components {
        bytes += kind.len() + 4;
        for (name, p) in &c.fields {
            bytes += name.len() + p.value.size() + 4;
            values += 1;
        }
    }
    (bytes, values)
}
/// Check unused definitions too: malformed/unknown data must not hide in an unused
/// prefab. Missing required fields/references are checked on concrete instances.
fn definitions(doc: &Document, schema: &Schema) -> Result<(), ContentError> {
    fn check(spec: &NodeSpec, schema: &Schema) -> Result<(), ContentError> {
        schema.validate_definition(&spec.components)?;
        for patch in spec.overrides.values() {
            schema.validate_definition(&patch.components)?;
        }
        Ok(())
    }
    fn visit(
        name: &str,
        doc: &Document,
        stack: &mut Vec<String>,
        done: &mut std::collections::BTreeSet<String>,
    ) -> Result<(), ContentError> {
        if stack.iter().any(|s| s == name) {
            return Err(ContentError::new(
                ErrorKind::PrefabCycle,
                SourceSpan::default(),
                name,
            ));
        }
        if stack.len() >= MAX_PREFAB_DEPTH {
            return Err(ContentError::new(
                ErrorKind::Limit,
                SourceSpan::default(),
                "prefab nesting exceeds 16",
            ));
        }
        if done.contains(name) {
            return Ok(());
        }
        let specs = doc.prefabs.get(name).ok_or_else(|| {
            ContentError::new(ErrorKind::UnknownPrefab, SourceSpan::default(), name)
        })?;
        stack.push(name.into());
        for spec in specs {
            if let Some(next) = &spec.prefab {
                visit(next, doc, stack, done)?;
            }
        }
        stack.pop();
        done.insert(name.into());
        Ok(())
    }
    let mut done = std::collections::BTreeSet::new();
    for (name, specs) in &doc.prefabs {
        let root = specs.iter().find(|s| s.id == "root").ok_or_else(|| {
            ContentError::new(
                ErrorKind::MissingNode,
                SourceSpan::default(),
                format!("prefab {name} needs node root"),
            )
        })?;
        if root.parent.is_some() {
            return Err(ContentError::new(
                ErrorKind::Reference,
                root.span,
                "prefab root cannot declare parent",
            ));
        }
        for spec in specs {
            check(spec, schema)?;
        }
        visit(name, doc, &mut Vec::new(), &mut done)?;
    }
    for spec in &doc.nodes {
        check(spec, schema)?;
        if let Some(p) = &spec.prefab
            && !doc.prefabs.contains_key(p)
        {
            return Err(ContentError::new(ErrorKind::UnknownPrefab, spec.span, p));
        }
    }
    for (name, specs) in &doc.prefabs {
        let mut template = Expander {
            doc,
            nodes: Vec::new(),
            bytes: 0,
            values: 0,
        };
        template.scope(specs, Some("p"), None, &mut vec![name.clone()])?;
        let lookup: BTreeMap<_, _> = template
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        if lookup.len() != template.nodes.len() {
            return Err(ContentError::new(
                ErrorKind::Duplicate,
                SourceSpan::default(),
                name,
            ));
        }
        let (parents, order) = topology(&template.nodes, &lookup)?;
        for i in order {
            let local = template.nodes[i].local.matrix().map_err(|_| {
                ContentError::new(ErrorKind::InvalidTransform, template.nodes[i].span, name)
            })?;
            template.nodes[i].world = parents[i].map_or(local, |p| {
                crate::scene::multiply(template.nodes[p].world, local)
            });
            if !template.nodes[i].world.iter().all(|v| v.is_finite()) {
                return Err(ContentError::new(
                    ErrorKind::InvalidTransform,
                    template.nodes[i].span,
                    name,
                ));
            }
        }
    }

    Ok(())
}
pub(super) fn compile(doc: &Document, schema: &Schema) -> Result<SceneData, ContentError> {
    definitions(doc, schema)?;
    let mut expander = Expander {
        doc,
        nodes: Vec::new(),
        bytes: 0,
        values: 0,
    };
    expander.scope(&doc.nodes, None, None, &mut Vec::new())?;
    finish(expander.nodes, schema)
}
pub(super) fn finish(mut nodes: Vec<DataNode>, schema: &Schema) -> Result<SceneData, ContentError> {
    if nodes.len() > MAX_NODES {
        return Err(ContentError::new(
            ErrorKind::Limit,
            SourceSpan::default(),
            "too many nodes",
        ));
    }
    let mut lookup = BTreeMap::new();
    let mut bytes = 0;
    let mut values = 0;
    for (i, n) in nodes.iter().enumerate() {
        if !path(&n.id) || n.parent.as_ref().is_some_and(|p| !path(p)) {
            return Err(ContentError::new(ErrorKind::Reference, n.span, &n.id));
        }
        if lookup.insert(n.id.clone(), i).is_some() {
            return Err(ContentError::new(ErrorKind::Duplicate, n.span, &n.id));
        }
        let size = size(n);
        bytes += size.0;
        values += size.1;
    }
    if bytes > MAX_COOKED_BYTES || values > MAX_VALUES {
        return Err(ContentError::new(
            ErrorKind::Limit,
            SourceSpan::default(),
            "scene data exceeds budget",
        ));
    }
    schema.validate(&nodes, &lookup)?;
    let (parents, order) = topology(&nodes, &lookup)?;
    let mut hierarchy = Scene::new();
    let mut handles = vec![None; nodes.len()];
    for i in order {
        let local = nodes[i].local.matrix().map_err(|_| {
            ContentError::new(ErrorKind::InvalidTransform, nodes[i].span, &nodes[i].id)
        })?;
        nodes[i].world =
            parents[i].map_or(local, |p| crate::scene::multiply(nodes[p].world, local));
        if !nodes[i].world.iter().all(|v| v.is_finite()) {
            return Err(ContentError::new(
                ErrorKind::InvalidTransform,
                nodes[i].span,
                "world matrix overflow",
            ));
        }
        handles[i] = Some(
            hierarchy
                .create(
                    nodes[i].id.clone(),
                    nodes[i].local,
                    parents[i].and_then(|p| handles[p]),
                )
                .map_err(|_| {
                    ContentError::new(ErrorKind::InvalidTransform, nodes[i].span, &nodes[i].id)
                })?,
        );
    }
    Ok(SceneData {
        nodes,
        lookup,
        hierarchy,
        handles: handles.into_iter().map(Option::unwrap).collect(),
    })
}

fn topology(
    nodes: &[DataNode],
    lookup: &BTreeMap<String, usize>,
) -> Result<(Vec<Option<usize>>, Vec<usize>), ContentError> {
    let parents: Vec<_> = nodes
        .iter()
        .map(|n| {
            n.parent
                .as_ref()
                .map(|p| {
                    lookup
                        .get(p)
                        .copied()
                        .ok_or_else(|| ContentError::new(ErrorKind::MissingNode, n.span, p))
                })
                .transpose()
        })
        .collect::<Result<_, _>>()?;
    let mut states = vec![0_u8; nodes.len()];
    let mut stack = Vec::new();
    let mut order = Vec::with_capacity(nodes.len());
    let mut depth = vec![0_usize; nodes.len()];
    for start in 0..nodes.len() {
        if states[start] != 0 {
            continue;
        }
        let mut current = start;
        loop {
            if states[current] == 1 {
                return Err(ContentError::new(
                    ErrorKind::ParentCycle,
                    nodes[current].span,
                    &nodes[current].id,
                ));
            }
            if states[current] == 2 {
                break;
            }
            states[current] = 1;
            stack.push(current);
            if stack.len() > MAX_HIERARCHY_DEPTH {
                return Err(ContentError::new(
                    ErrorKind::Limit,
                    nodes[current].span,
                    "hierarchy too deep",
                ));
            }
            if let Some(parent) = parents[current] {
                current = parent;
            } else {
                break;
            }
        }
        while let Some(i) = stack.pop() {
            depth[i] = parents[i].map_or(1, |p| depth[p] + 1);
            if depth[i] > MAX_HIERARCHY_DEPTH {
                return Err(ContentError::new(
                    ErrorKind::Limit,
                    nodes[i].span,
                    "hierarchy too deep",
                ));
            }
            states[i] = 2;
            order.push(i);
        }
    }
    Ok((parents, order))
}
