//! Expansion keeps the authored document immutable and publishes a scene only
//! after every definition, instance and reference has passed validation. The
//! existing [`Document::compile`] and [`Document::load_with_diagnostics`] APIs
//! report dependency paths, including unused definitions. Locations identify the
//! node declaring the failing dependency; the parser does not retain `use` spans.
//!
//! ```
//! use ferravex_play::authoring::{Document, ErrorKind, Schema};
//! let source = "ferravex_scene 1;\n\
//!     prefab assembly { node root { use piece; } }\n\
//!     prefab piece { node root { use assembly; } }";
//! let error = Document::load_with_diagnostics(source, &Schema::new()).err().unwrap();
//! assert_eq!(error.error.kind, ErrorKind::PrefabCycle);
//! assert!(error.error.context.contains("assembly -> piece -> assembly"));
//! assert_eq!(error.error.span.line, 3);
//! assert_eq!(error.token.as_deref(), Some("root"));
//! ```

use super::*;

fn dependency_error(
    kind: ErrorKind,
    span: SourceSpan,
    stack: &[String],
    next: &str,
    node: &str,
) -> ContentError {
    let reason = match kind {
        ErrorKind::PrefabCycle => "prefab dependency cycle".into(),
        ErrorKind::UnknownPrefab => "unknown prefab dependency".into(),
        _ => format!("prefab nesting exceeds {MAX_PREFAB_DEPTH}"),
    };
    let mut chain = stack.to_vec();
    chain.push(next.into());
    ContentError::new(
        kind,
        span,
        format!("{reason}: {}; node `{node}`", chain.join(" -> ")),
    )
}

fn expansion_error(mut error: ContentError, stack: &[String], node: &str) -> ContentError {
    error.context.push_str(&format!("; node `{node}`"));
    if !stack.is_empty() {
        error
            .context
            .push_str(&format!("; prefab path: {}", stack.join(" -> ")));
    }
    error
}

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
fn resolve_reference(reference: &str, scope: Option<&str>, instance: &str) -> String {
    if let Some(absolute) = reference.strip_prefix('/') {
        absolute.into()
    } else if let Some(relative) = reference.strip_prefix('@') {
        qualify(Some(instance), relative)
    } else {
        qualify(scope, reference)
    }
}

struct Expander<'a> {
    doc: &'a Document,
    nodes: Vec<DataNode>,
    bytes: usize,
    values: usize,
}
impl Expander<'_> {
    fn budget(
        &self,
        removed: (usize, usize),
        added: (usize, usize),
        span: SourceSpan,
    ) -> Result<(usize, usize), ContentError> {
        let bytes = self
            .bytes
            .checked_sub(removed.0)
            .and_then(|n| n.checked_add(added.0))
            .filter(|n| *n <= MAX_COOKED_BYTES)
            .ok_or_else(|| {
                ContentError::new(
                    ErrorKind::Limit,
                    span,
                    format!("expanded data exceeds {MAX_COOKED_BYTES} byte budget"),
                )
            })?;
        let values = self
            .values
            .checked_sub(removed.1)
            .and_then(|n| n.checked_add(added.1))
            .filter(|n| *n <= MAX_VALUES)
            .ok_or_else(|| {
                ContentError::new(
                    ErrorKind::Limit,
                    span,
                    format!("expanded data exceeds {MAX_VALUES} value budget"),
                )
            })?;
        Ok((bytes, values))
    }
    fn merge(
        &mut self,
        index: usize,
        components: &BTreeMap<String, Component>,
        scope: Option<&str>,
        instance: &str,
    ) -> Result<(), ContentError> {
        // Check the final merged size before cloning component data. Subtract
        // replacements together so a shrinking field can fund a growing one,
        // regardless of key order. Only one bounded resolved path is allocated
        // at a time during this pass.
        let node = &self.nodes[index];
        let mut removed = (0_usize, 0_usize);
        let mut added = (0_usize, 0_usize);
        let mut component_count = node.components.len();
        let mut span = node.span;
        for (name, incoming) in components {
            span = incoming.span;
            let previous = node.components.get(name);
            if previous.is_none() {
                component_count += 1;
                if component_count > MAX_COMPONENTS {
                    return Err(ContentError::new(
                        ErrorKind::Limit,
                        span,
                        format!("expanded node exceeds {MAX_COMPONENTS} components"),
                    ));
                }
                added.0 += name.len() + 4;
            }
            let mut field_count = previous.map_or(0, |c| c.fields.len());
            for (field, property) in &incoming.fields {
                span = property.span;
                if let Some(old) = previous.and_then(|c| c.fields.get(field)) {
                    removed.0 += field.len() + old.value.size() + 4;
                    removed.1 += 1;
                } else {
                    field_count += 1;
                    if field_count > MAX_FIELDS {
                        return Err(ContentError::new(
                            ErrorKind::Limit,
                            span,
                            format!("expanded component `{name}` exceeds {MAX_FIELDS} fields"),
                        ));
                    }
                }
                let bytes = if let Value::Reference(reference) = &property.value {
                    let resolved = resolve_reference(reference, scope, instance);
                    if !path(&resolved) {
                        return Err(ContentError::new(
                            ErrorKind::Reference,
                            span,
                            format!("invalid expanded reference path `{resolved}`"),
                        ));
                    }
                    resolved.len() + 2
                } else {
                    property.value.size()
                };
                added.0 += field.len() + bytes + 4;
                added.1 += 1;
            }
        }
        let (bytes, values) = self.budget(removed, added, span)?;
        // Release replaced values before adding data so even the commit pass
        // stays within the final byte/value budget. No fallible checks remain.
        for (name, incoming) in components {
            if let Some(destination) = self.nodes[index].components.get_mut(name) {
                for field in incoming.fields.keys() {
                    destination.fields.remove(field);
                }
            }
        }
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
                    *reference = resolve_reference(reference, scope, instance);
                }
                destination.fields.insert(field.clone(), property);
            }
        }
        self.bytes = bytes;
        self.values = values;
        Ok(())
    }
    fn scope(
        &mut self,
        specs: &[NodeSpec],
        prefix: Option<&str>,
        parent: Option<&str>,
        stack: &mut Vec<String>,
    ) -> Result<(), ContentError> {
        // A scope only appends nodes and patches nodes appended within itself.
        // Restore its checkpoint on every failure, including a failed override
        // after successful nested expansion. No partial instance survives.
        let checkpoint = (self.nodes.len(), self.bytes, self.values, stack.len());
        let result = self.expand_scope(specs, prefix, parent, stack);
        stack.truncate(checkpoint.3);
        if result.is_err() {
            self.nodes.truncate(checkpoint.0);
            self.bytes = checkpoint.1;
            self.values = checkpoint.2;
        }
        result
    }
    fn expand_scope(
        &mut self,
        specs: &[NodeSpec],
        prefix: Option<&str>,
        parent: Option<&str>,
        stack: &mut Vec<String>,
    ) -> Result<(), ContentError> {
        for spec in specs {
            let id = qualify(prefix, &spec.id);
            let fail = |kind, span, context: &str, stack: &[String]| {
                expansion_error(ContentError::new(kind, span, context), stack, &id)
            };
            if !path(&id) {
                return Err(fail(
                    ErrorKind::Limit,
                    spec.span,
                    "expanded node path exceeds limit",
                    stack,
                ));
            }
            let parent = if prefix.is_some() && spec.id == "root" {
                parent.map(str::to_owned)
            } else if let Some(p) = &spec.parent {
                if p.starts_with('/') && prefix.is_some() {
                    return Err(fail(
                        ErrorKind::Reference,
                        spec.span,
                        "prefab parent cannot escape its scope",
                        stack,
                    ));
                }
                Some(qualify(prefix, p.strip_prefix('/').unwrap_or(p)))
            } else {
                prefix.map(str::to_owned)
            };
            let index = if let Some(prefab) = &spec.prefab {
                if stack.contains(prefab) {
                    return Err(dependency_error(
                        ErrorKind::PrefabCycle,
                        spec.span,
                        stack,
                        prefab,
                        &id,
                    ));
                }
                if stack.len() >= MAX_PREFAB_DEPTH {
                    return Err(dependency_error(
                        ErrorKind::Limit,
                        spec.span,
                        stack,
                        prefab,
                        &id,
                    ));
                }
                let definitions = self.doc.prefabs.get(prefab).ok_or_else(|| {
                    dependency_error(ErrorKind::UnknownPrefab, spec.span, stack, prefab, &id)
                })?;
                let start = self.nodes.len();
                stack.push(prefab.clone());
                self.scope(definitions, Some(&id), parent.as_deref(), stack)?;
                let root = (start..self.nodes.len())
                    .find(|i| self.nodes[*i].id == id)
                    .ok_or_else(|| {
                        fail(
                            ErrorKind::MissingNode,
                            spec.span,
                            "prefab requires root node",
                            stack,
                        )
                    })?;
                if let Some(local) = spec.local {
                    self.nodes[root].local = local;
                }
                self.merge(root, &spec.components, prefix, &id)
                    .map_err(|error| expansion_error(error, stack, &id))?;
                for (target, patch) in &spec.overrides {
                    let full = qualify(Some(&id), target);
                    let i = (start..self.nodes.len())
                        .find(|i| self.nodes[*i].id == full)
                        .ok_or_else(|| {
                            fail(
                                ErrorKind::MissingNode,
                                spec.span,
                                &format!("override target `{full}` does not exist"),
                                stack,
                            )
                        })?;
                    if let Some(local) = patch.local {
                        self.nodes[i].local = local;
                    }
                    self.merge(i, &patch.components, prefix, &id)
                        .map_err(|error| expansion_error(error, stack, &full))?;
                }
                stack.pop();
                root
            } else {
                if !spec.overrides.is_empty() {
                    return Err(fail(
                        ErrorKind::Domain,
                        spec.span,
                        "override requires a prefab instance",
                        stack,
                    ));
                }
                if self.nodes.len() >= MAX_NODES {
                    return Err(fail(
                        ErrorKind::Limit,
                        spec.span,
                        &format!("more than {MAX_NODES} expanded nodes"),
                        stack,
                    ));
                }
                let i = self.nodes.len();
                let node = DataNode {
                    id: id.clone(),
                    parent,
                    local: spec.local.unwrap_or_default(),
                    world: crate::scene::IDENTITY,
                    components: BTreeMap::new(),
                    span: spec.span,
                };
                let (bytes, values) = self
                    .budget((0, 0), size(&node), spec.span)
                    .map_err(|error| expansion_error(error, stack, &id))?;
                self.nodes.push(node);
                self.bytes = bytes;
                self.values = values;
                self.merge(i, &spec.components, prefix, &id)
                    .map_err(|error| expansion_error(error, stack, &id))?;
                i
            };
            if !self.nodes[index].local.valid() {
                return Err(fail(ErrorKind::InvalidTransform, spec.span, &id, stack));
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
        done: &mut BTreeMap<String, usize>,
        span: SourceSpan,
        node: &str,
    ) -> Result<(), ContentError> {
        if stack.iter().any(|s| s == name) {
            return Err(dependency_error(
                ErrorKind::PrefabCycle,
                span,
                stack,
                name,
                node,
            ));
        }
        if stack.len() >= MAX_PREFAB_DEPTH {
            return Err(dependency_error(ErrorKind::Limit, span, stack, name, node));
        }
        // Reusing a definition at a deeper depth must recheck its descendants.
        // Cache the deepest validated entry, bounding visits to at most
        // MAX_PREFAB_DEPTH per definition without enumerating every DAG path.
        if done.get(name).is_some_and(|depth| *depth >= stack.len()) {
            return Ok(());
        }
        let specs = doc
            .prefabs
            .get(name)
            .ok_or_else(|| dependency_error(ErrorKind::UnknownPrefab, span, stack, name, node))?;
        stack.push(name.into());
        let result: Result<(), ContentError> = (|| {
            for spec in specs {
                if let Some(next) = &spec.prefab {
                    visit(next, doc, stack, done, spec.span, &spec.id)?;
                }
            }
            Ok(())
        })();
        stack.pop();
        result?;
        done.insert(name.into(), stack.len());
        Ok(())
    }
    let mut done = BTreeMap::new();
    for (name, specs) in &doc.prefabs {
        let root = specs.iter().find(|s| s.id == "root").ok_or_else(|| {
            ContentError::new(
                ErrorKind::MissingNode,
                specs
                    .first()
                    .map_or(SourceSpan::default(), |spec| spec.span),
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
        visit(name, doc, &mut Vec::new(), &mut done, root.span, &root.id)?;
    }
    for spec in &doc.nodes {
        check(spec, schema)?;
        if let Some(p) = &spec.prefab
            && !doc.prefabs.contains_key(p)
        {
            return Err(dependency_error(
                ErrorKind::UnknownPrefab,
                spec.span,
                &[],
                p,
                &spec.id,
            ));
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
        let mut lookup = BTreeMap::new();
        for (i, node) in template.nodes.iter().enumerate() {
            if lookup.insert(node.id.clone(), i).is_some() {
                return Err(ContentError::new(
                    ErrorKind::Duplicate,
                    node.span,
                    format!("prefab `{name}` has duplicate expanded node `{}`", node.id),
                ));
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn expander(doc: &Document) -> Expander<'_> {
        Expander {
            doc,
            nodes: Vec::new(),
            bytes: 0,
            values: 0,
        }
    }

    fn failure(source: &str) -> ContentError {
        Document::load(source, &Schema::new()).err().unwrap()
    }

    #[test]
    fn dependency_cycles_report_the_full_path_and_closing_authored_node() {
        let source = "ferravex_scene 1;\n\
            prefab a { node root { use b; } }\n\
            prefab b { node root {} node closing { use c; } }\n\
            prefab c { node root { use b; } }";
        let doc = Document::parse(source).unwrap();
        let before = format!("{doc:?}");
        let error = doc.compile(&Schema::new()).err().unwrap();
        assert_eq!(error.kind, ErrorKind::PrefabCycle);
        assert!(error.context.contains("a -> b -> c -> b"));
        assert_eq!(error.span, doc.prefabs["c"][0].span);
        assert_eq!(error.diagnostic(source).token.as_deref(), Some("root"));
        assert_eq!(format!("{doc:?}"), before);
        assert_eq!(doc.compile(&Schema::new()).err().unwrap(), error);

        let error = failure("ferravex_scene 1; prefab self_cycle {node root {use self_cycle;}}");
        assert_eq!(error.kind, ErrorKind::PrefabCycle);
        assert!(error.context.contains("self_cycle -> self_cycle"));
        assert_eq!(error.span.line, 1);
    }

    #[test]
    fn unknown_dependencies_report_paths_in_unused_definitions_and_scene_instances() {
        let source = "ferravex_scene 1;\n\
            prefab a {node root {use b;}}\n\
            prefab b {node root {} node socket {use missing;}}";
        let doc = Document::parse(source).unwrap();
        let error = doc.compile(&Schema::new()).err().unwrap();
        assert_eq!(error.kind, ErrorKind::UnknownPrefab);
        assert!(error.context.contains("a -> b -> missing"));
        assert!(error.context.contains("node `socket`"));
        assert_eq!(error.span, doc.prefabs["b"][1].span);
        assert_eq!(error.diagnostic(source).token.as_deref(), Some("socket"));

        let error = failure("ferravex_scene 1;\nnode placed {use missing;}");
        assert_eq!(error.kind, ErrorKind::UnknownPrefab);
        assert!(error.context.contains("missing; node `placed`"));
        assert_eq!(error.span.line, 2);
    }

    fn dependency_chain(count: usize, instantiate: bool) -> String {
        // Lexical ordering visits each suffix before discovering its deeper
        // callers, exercising memoized depth checks on shared dependencies.
        let mut source = String::from("ferravex_scene 1;\nprefab p00 {node root {}}\n");
        for i in 1..count {
            source.push_str(&format!(
                "prefab p{i:02} {{node root {{use p{:02};}}}}\n",
                i - 1
            ));
        }
        if instantiate {
            source.push_str(&format!("node instance {{use p{:02};}}", count - 1));
        }
        source
    }

    #[test]
    fn shared_dependency_depth_is_checked_at_the_boundary_with_a_complete_path() {
        let allowed = dependency_chain(MAX_PREFAB_DEPTH, true);
        let scene = Document::load(&allowed, &Schema::new()).unwrap();
        assert_eq!(scene.nodes().len(), 1);
        assert_eq!(scene.nodes()[0].id, "instance");

        for instantiate in [false, true] {
            let source = dependency_chain(MAX_PREFAB_DEPTH + 1, instantiate);
            let doc = Document::parse(&source).unwrap();
            let error = doc.compile(&Schema::new()).err().unwrap();
            let chain = (0..=MAX_PREFAB_DEPTH)
                .rev()
                .map(|i| format!("p{i:02}"))
                .collect::<Vec<_>>()
                .join(" -> ");
            assert_eq!(error.kind, ErrorKind::Limit);
            assert!(error.context.contains(&chain), "{error}");
            assert_eq!(error.span, doc.prefabs["p01"][0].span);
        }
    }

    #[test]
    fn structural_prefab_errors_retain_available_source_locations() {
        let source = "ferravex_scene 1;\nprefab incomplete {node child {}}";
        let error = failure(source);
        assert_eq!(error.kind, ErrorKind::MissingNode);
        assert_eq!(error.diagnostic(source).token.as_deref(), Some("child"));

        let source = "ferravex_scene 1;\n\
            prefab inner {node root {} node child {}}\n\
            prefab outer {node root {use inner;} node child {}}";
        let doc = Document::parse(source).unwrap();
        let error = doc.compile(&Schema::new()).err().unwrap();
        assert_eq!(error.kind, ErrorKind::Duplicate);
        assert!(error.context.contains("outer"));
        assert!(error.context.contains("p/child"));
        assert_eq!(error.span, doc.prefabs["outer"][1].span);
    }

    #[test]
    fn a_late_override_failure_rolls_back_nodes_budgets_and_stack_then_allows_retry() {
        let source = "ferravex_scene 1;\n\
            prefab leaf {node root {component data {value int 7;}}}\n\
            prefab assembly {node root {use leaf;} node child {}}\n\
            node keep {}\n\
            node broken {use assembly; override root {component data {value int 9;}} override zzmissing {}}\n\
            node recovered {use assembly;}";
        let doc = Document::parse(source).unwrap();
        let original = format!("{doc:?}");
        let mut expansion = expander(&doc);
        let mut stack = Vec::new();
        expansion
            .scope(&doc.nodes[..1], None, None, &mut stack)
            .unwrap();
        let before = format!("{:?}", expansion.nodes);
        let budget = (expansion.bytes, expansion.values);
        let error = expansion
            .scope(&doc.nodes[1..2], None, None, &mut stack)
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::MissingNode);
        assert!(error.context.contains("broken/zzmissing"));
        assert!(error.context.contains("prefab path: assembly"));
        assert_eq!(error.span, doc.nodes[1].span);
        assert_eq!(format!("{:?}", expansion.nodes), before);
        assert_eq!((expansion.bytes, expansion.values), budget);
        assert!(stack.is_empty());
        expansion
            .scope(&doc.nodes[2..], None, None, &mut stack)
            .unwrap();
        assert_eq!(expansion.nodes.len(), 3);
        assert_eq!(expansion.nodes[1].id, "recovered");
        assert_eq!(
            expansion.nodes[1].component("data").unwrap().get("value"),
            Some(&Value::Integer(7))
        );
        assert_eq!(format!("{doc:?}"), original);
    }

    #[test]
    fn invalid_expanded_references_do_not_partially_merge_earlier_fields() {
        let reference = vec!["r".repeat(63); 8].join("/");
        assert!(path(&reference));
        let source = format!(
            "ferravex_scene 1; node base {{component data {{a int 1;}}}}\n\
             node patch {{component data {{a int 2; z ref {reference};}}}}"
        );
        let doc = Document::parse(&source).unwrap();
        let mut expansion = expander(&doc);
        expansion
            .scope(&doc.nodes[..1], None, None, &mut Vec::new())
            .unwrap();
        let before = format!("{:?}", expansion.nodes);
        let budget = (expansion.bytes, expansion.values);
        let error = expansion
            .merge(0, &doc.nodes[1].components, Some("scope"), "base")
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Reference);
        assert_eq!(error.span, doc.nodes[1].components["data"].fields["z"].span);
        assert_eq!(format!("{:?}", expansion.nodes), before);
        assert_eq!((expansion.bytes, expansion.values), budget);
    }

    #[test]
    fn component_and_field_union_limits_leave_the_destination_unchanged() {
        for components in [false, true] {
            let mut source = String::from("ferravex_scene 1; node base {");
            if components {
                for i in 0..MAX_COMPONENTS {
                    source.push_str(&format!("component c{i:02} {{}}"));
                }
                source
                    .push_str("} node patch {component c00 {changed int 1;} component zzextra {}}");
            } else {
                source.push_str("component data {");
                for i in 0..MAX_FIELDS {
                    source.push_str(&format!("f{i:02} int 1;"));
                }
                source.push_str("}} node patch {component data {f00 int 2; zzextra int 3;}}");
            }
            let doc = Document::parse(&source).unwrap();
            let mut expansion = expander(&doc);
            expansion
                .scope(&doc.nodes[..1], None, None, &mut Vec::new())
                .unwrap();
            let before = format!("{:?}", expansion.nodes);
            let budget = (expansion.bytes, expansion.values);
            let error = expansion
                .merge(0, &doc.nodes[1].components, None, "base")
                .unwrap_err();
            assert_eq!(error.kind, ErrorKind::Limit);
            assert_eq!(error.diagnostic(&source).token.as_deref(), Some("zzextra"));
            assert_eq!(format!("{:?}", expansion.nodes), before);
            assert_eq!((expansion.bytes, expansion.values), budget);
        }
    }

    #[test]
    fn exponential_prefab_growth_stops_at_the_node_budget_without_partial_output() {
        let mut source = String::from("ferravex_scene 1; prefab p00 {node root {}}");
        for i in 1..=12 {
            source.push_str(&format!(
                "prefab p{i:02} {{node root {{use p{:02};}} node side{i:02} {{use p{:02};}}}}",
                i - 1,
                i - 1
            ));
        }
        source.push_str("node instance {use p12;}");
        let scene = Document::load(&source, &Schema::new()).unwrap();
        assert_eq!(scene.nodes().len(), MAX_NODES);

        source.push_str("\nnode extra {}");
        let doc = Document::parse(&source).unwrap();
        let error = doc.compile(&Schema::new()).err().unwrap();
        assert_eq!(error.kind, ErrorKind::Limit);
        assert!(error.context.contains("node `extra`"));
        assert_eq!(error.span, doc.nodes[1].span);
        let mut expansion = expander(&doc);
        assert!(
            expansion
                .scope(&doc.nodes, None, None, &mut Vec::new())
                .is_err()
        );
        assert!(expansion.nodes.is_empty());
        assert_eq!((expansion.bytes, expansion.values), (0, 0));
    }

    #[test]
    fn value_budget_accepts_replacements_at_capacity_and_rejects_additions_atomically() {
        let mut source = String::from("ferravex_scene 1; prefab full {node root {component data {");
        for i in 0..MAX_FIELDS {
            source.push_str(&format!("f{i:02} int 1;"));
        }
        source.push_str("}}}");
        for i in 0..MAX_VALUES / MAX_FIELDS {
            source.push_str(&format!("node n{i} {{use full;}}"));
        }
        source.push_str(
            "node patch {component data {f00 int 2;}} node extra {component extra {value int 3;}}",
        );
        let doc = Document::parse(&source).unwrap();
        let mut expansion = expander(&doc);
        let count = doc.nodes.len() - 2;
        expansion
            .scope(&doc.nodes[..count], None, None, &mut Vec::new())
            .unwrap();
        assert_eq!(expansion.values, MAX_VALUES);
        let budget = (expansion.bytes, expansion.values);
        expansion
            .merge(0, &doc.nodes[count].components, None, "n0")
            .unwrap();
        assert_eq!((expansion.bytes, expansion.values), budget);
        assert_eq!(
            expansion.nodes[0].component("data").unwrap().get("f00"),
            Some(&Value::Integer(2))
        );
        let before = format!("{:?}", expansion.nodes[0]);
        let error = expansion
            .merge(0, &doc.nodes[count + 1].components, None, "n0")
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Limit);
        assert!(error.context.contains("value budget"));
        assert_eq!(
            error.span,
            doc.nodes[count + 1].components["extra"].fields["value"].span
        );
        assert_eq!(format!("{:?}", expansion.nodes[0]), before);
        assert_eq!((expansion.bytes, expansion.values), budget);
    }

    #[test]
    fn byte_budget_rejects_repeated_payloads_and_restores_the_scope_checkpoint() {
        let mut source = String::from("ferravex_scene 1; prefab full {node root {component data {");
        for i in 0..32 {
            source.push_str(&format!("f{i:02} text \"{}\";", "x".repeat(MAX_STRING)));
        }
        source.push_str("}}} node first {use full;}");
        let doc = Document::parse(&source).unwrap();
        let mut expansion = expander(&doc);
        expansion
            .scope(&doc.nodes[..1], None, None, &mut Vec::new())
            .unwrap();
        let node_bytes = expansion.bytes;
        let count = MAX_COOKED_BYTES / node_bytes;
        // Expand unique, equally sized instance IDs; parsed source stays small.
        for i in 1..count {
            let prefix = format!("n{i:04}");
            expansion
                .scope(
                    &doc.prefabs["full"],
                    Some(&prefix),
                    None,
                    &mut vec!["full".into()],
                )
                .unwrap();
        }
        assert_eq!(expansion.bytes, count * node_bytes);
        let budget = (expansion.bytes, expansion.values);
        let mut stack = vec!["full".into()];
        let error = expansion
            .scope(&doc.prefabs["full"], Some("extra"), None, &mut stack)
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Limit);
        assert!(error.context.contains("byte budget"));
        assert!(error.context.contains("node `extra`; prefab path: full"));
        assert_eq!(expansion.nodes.len(), count);
        assert_eq!((expansion.bytes, expansion.values), budget);
        assert_eq!(stack, ["full"]);
    }

    #[test]
    fn valid_nested_output_matches_flat_authored_bytes_and_preserves_property_spans() {
        let source = "ferravex_scene 1;\n\
            prefab leaf {node root {component data {a text \"\"; z text \"longer\"; target ref root;}}}\n\
            prefab pair {node root {use leaf;} node child {use leaf;}}\n\
            node instance {use pair; override child {component data {a text \"longer\"; z text \"\"; target ref @root;}}}";
        let flat = "ferravex_scene 1;\n\
            prefab flat {\n\
            node root {component data {a text \"\"; z text \"longer\"; target ref root;}}\n\
            node child {parent root; component data {a text \"longer\"; z text \"\"; target ref root;}}}\n\
            node instance {use flat;}";
        let flat_doc = Document::parse(flat).unwrap();
        let mut schema = Schema::new();
        schema
            .register(
                "data",
                &[
                    FieldRule::optional("a", ValueKind::Text),
                    FieldRule::optional("z", ValueKind::Text),
                    FieldRule::optional("target", ValueKind::Reference),
                ],
            )
            .unwrap();
        let doc = Document::parse(source).unwrap();
        let original = format!("{doc:?}");
        let scene = doc.compile(&schema).unwrap();
        assert_eq!(
            scene.encode().unwrap(),
            flat_doc.compile(&schema).unwrap().encode().unwrap()
        );
        assert_eq!(
            scene.node("instance").unwrap().span,
            doc.prefabs["leaf"][0].span
        );
        let property = &scene.node("instance/child").unwrap().components["data"].fields["a"];
        assert_eq!(
            property.span,
            doc.nodes[0].overrides["child"].components["data"].fields["a"].span
        );
        assert_eq!(format!("{doc:?}"), original);
        assert_eq!(
            scene.encode().unwrap(),
            doc.compile(&schema).unwrap().encode().unwrap()
        );
    }
}
