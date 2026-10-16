use ferravex_play::{authoring::*, scene::transform_point};
fn schema() -> Schema {
    let mut s = Schema::new();
    s.register(
        "body",
        &[
            FieldRule::required("size", ValueKind::Vec3).range(0.01, 100.),
            FieldRule::optional("mass", ValueKind::Number).range(0., 1000.),
            FieldRule::optional("target", ValueKind::Reference).target("marker"),
            FieldRule::optional("enabled", ValueKind::Bool),
            FieldRule::optional("label", ValueKind::Text),
            FieldRule::optional("count", ValueKind::Integer).integer_range(i64::MIN, i64::MAX),
            FieldRule::optional("uv", ValueKind::Vec2),
            FieldRule::optional("rotation", ValueKind::Quaternion),
            FieldRule::optional("color", ValueKind::Color),
        ],
    )
    .unwrap();
    s.register("marker", &[]).unwrap();
    s
}
const SOURCE: &str = r#"ferravex_scene 1;
# Root may be declared after children. References are scoped per instance.
prefab beam {
 node flag { parent root; transform 0 2 0 0 0 0 1 1 1 1; component marker {} }
 node root { component body { size vec3 2 3 4; target ref flag; label text "Original λ"; } }
}
node parent { transform 10 0 0 0 0 0 1 2 2 2; }
node a { use beam; parent parent; transform 1 0 0 0 0 0 1 1 1 1;
 component body { mass number 12.5; enabled bool true; count int -9223372036854775808; uv vec2 .25 .75; rotation quat 0 0 0 2; color rgba 20 80 120 255; }
 override flag { transform 0 3 0 0 0 0 1 1 1 1; }
}
node b { use beam; transform -4 0 0 0 0 0 1 1 1 1; component body { size vec3 4 5 6; target ref @flag; } }
"#;
#[test]
fn hierarchical_prefabs_overrides_refs_and_world_matrices_are_independent() {
    let data = Document::load(SOURCE, &schema()).unwrap();
    assert_eq!(data.nodes().len(), 5);
    assert_eq!(data.node("a/flag").unwrap().position(), [12., 6., 0.]);
    assert_eq!(data.node("b/flag").unwrap().position(), [-4., 2., 0.]);
    assert_eq!(
        data.node("a")
            .unwrap()
            .component("body")
            .unwrap()
            .get("target")
            .unwrap()
            .reference(),
        Some("a/flag")
    );
    assert_eq!(
        data.node("b")
            .unwrap()
            .component("body")
            .unwrap()
            .get("target")
            .unwrap()
            .reference(),
        Some("b/flag")
    );
    let handle = data.handle("a/flag").unwrap();
    assert_eq!(
        transform_point(data.hierarchy().world_matrix(handle).unwrap(), [0.; 3]),
        [12., 6., 0.]
    );
    assert_eq!(data.of_kind("marker").count(), 2);
    assert_eq!(
        data.node("b")
            .unwrap()
            .component("body")
            .unwrap()
            .get("size")
            .unwrap()
            .vec3(),
        Some([4., 5., 6.])
    );
}
#[test]
fn cooked_roundtrip_is_deterministic_and_rechecks_schema() {
    let data = Document::load(SOURCE, &schema()).unwrap();
    let bytes = data.encode().unwrap();
    assert_eq!(bytes, data.encode().unwrap());
    let decoded = SceneData::decode(&bytes, &schema()).unwrap();
    assert_eq!(bytes, decoded.encode().unwrap());
    for (a, b) in data.nodes().iter().zip(decoded.nodes()) {
        assert_eq!(a.id, b.id);
        assert_eq!(a.parent, b.parent);
        assert_eq!(a.local, b.local);
        assert_eq!(a.world, b.world);
        assert_eq!(
            a.component("body").map(|c| c.fields().collect::<Vec<_>>()),
            b.component("body").map(|c| c.fields().collect::<Vec<_>>())
        );
    }
    assert_eq!(
        SceneData::decode(&bytes, &Schema::new())
            .err()
            .unwrap()
            .kind,
        ErrorKind::UnknownComponent
    );
    for len in [0, 7, 19, bytes.len() - 1] {
        assert!(SceneData::decode(&bytes[..len], &schema()).is_err());
    }
    let mut corrupt = bytes.clone();
    corrupt[35] ^= 8;
    assert_eq!(
        SceneData::decode(&corrupt, &schema()).err().unwrap().kind,
        ErrorKind::Checksum
    );
}
fn failure(source: &str, kind: ErrorKind) {
    let e = Document::load(source, &schema())
        .err()
        .expect("accepted invalid content");
    assert_eq!(e.kind, kind, "{e}");
}
#[test]
fn schema_rejects_unknown_missing_wrong_types_and_ranges_with_locations() {
    for (body, kind) in [
        ("component unknown {}", ErrorKind::UnknownComponent),
        ("component body {}", ErrorKind::MissingField),
        ("component body { size vec2 1 2; }", ErrorKind::Type),
        (
            "component body { size vec3 1 2 3; typo bool true; }",
            ErrorKind::UnknownField,
        ),
        ("component body { size vec3 1 0 3; }", ErrorKind::Range),
        (
            "component body { size vec3 1 2 3; mass number NaN; }",
            ErrorKind::Range,
        ),
        (
            "component body { size vec3 1 2 3; rotation quat 0 0 0 0; }",
            ErrorKind::Range,
        ),
    ] {
        let text = format!("ferravex_scene 1;\nnode bad {{\n {body}\n}}");
        let e = Document::load(&text, &schema()).err().unwrap();
        assert_eq!(e.kind, kind, "{e}");
        assert!(e.span.line >= 2);
        assert!(e.span.column > 0);
    }
    failure("ferravex_scene 9;", ErrorKind::Version);
    failure(
        "ferravex_scene 1; node x { component body {size vec3 1 2 3;size vec3 1 2 3;} }",
        ErrorKind::Duplicate,
    );
    failure(
        "ferravex_scene 1; node x {} node x {}",
        ErrorKind::Duplicate,
    );
    failure(
        "ferravex_scene 1; prefab unused {node root {component bad {}}}",
        ErrorKind::UnknownComponent,
    );
}
#[test]
fn cycles_dangling_references_wrong_targets_and_unused_prefabs_are_rejected() {
    failure(
        "ferravex_scene 1; node a {parent b;} node b {parent a;}",
        ErrorKind::ParentCycle,
    );
    failure(
        "ferravex_scene 1; node a {parent absent;}",
        ErrorKind::MissingNode,
    );
    failure(
        "ferravex_scene 1; prefab a {node root {use b;}} prefab b {node root {use a;}}",
        ErrorKind::PrefabCycle,
    );
    failure(
        "ferravex_scene 1; prefab bad {node child {}}",
        ErrorKind::MissingNode,
    );
    failure(
        "ferravex_scene 1; prefab bad {node root {} node child {parent missing;}}",
        ErrorKind::MissingNode,
    );
    failure(
        "ferravex_scene 1; prefab bad {node root {} node a {parent b;} node b {parent a;}}",
        ErrorKind::ParentCycle,
    );
    failure(
        "ferravex_scene 1; node x {use absent;}",
        ErrorKind::UnknownPrefab,
    );
    failure(
        "ferravex_scene 1; node x {component body {size vec3 1 2 3;target ref missing;}}",
        ErrorKind::Reference,
    );
    failure(
        "ferravex_scene 1; node x {component body {size vec3 1 2 3;target ref y;}} node y {}",
        ErrorKind::Reference,
    );
    failure(
        "ferravex_scene 1; prefab p {node root {}} node x {use p;override missing {}}",
        ErrorKind::MissingNode,
    );
}
#[test]
fn nested_instances_and_absolute_references_resolve_without_aliasing() {
    let text = r#"ferravex_scene 1; prefab cap {node root {component marker {}}} prefab assembly {node root {component body {size vec3 1 2 3;target ref tip;}} node tip {use cap;}} node world {component marker {}} node first {use assembly;} node second {use assembly;component body {target ref /world;} override tip {transform 1 2 3 0 0 0 1 1 1 1;}}"#;
    let data = Document::load(text, &schema()).unwrap();
    assert_eq!(
        data.node("first")
            .unwrap()
            .component("body")
            .unwrap()
            .get("target")
            .unwrap()
            .reference(),
        Some("first/tip")
    );
    assert_eq!(
        data.node("second")
            .unwrap()
            .component("body")
            .unwrap()
            .get("target")
            .unwrap()
            .reference(),
        Some("world")
    );
    assert_eq!(data.node("second/tip").unwrap().position(), [1., 2., 3.]);
}
#[test]
fn limits_utf8_escaping_numeric_overflow_and_integer_precision_are_checked() {
    let text = "ferravex_scene 1; node a {component body {size vec3 1 2 3;label text \"a\\n\\\"λ\\\\\";count int 9223372036854775807;}}";
    let scene = Document::load(text, &schema()).unwrap();
    let c = scene.node("a").unwrap().component("body").unwrap();
    assert_eq!(c.get("label").unwrap().text(), Some("a\n\"λ\\"));
    assert_eq!(c.get("count").unwrap().integer(), Some(i64::MAX));
    failure(
        "ferravex_scene 1; node a {component body {size vec3 1e100 2 3;}}",
        ErrorKind::Range,
    );
    failure(
        "ferravex_scene 1; node a {transform 0 0 0 0 0 0 0 1 1 1;}",
        ErrorKind::InvalidTransform,
    );
    assert_eq!(
        Document::parse(&"x".repeat(MAX_SOURCE_BYTES + 1))
            .err()
            .unwrap()
            .kind,
        ErrorKind::Limit
    );
    let mut limited = Schema::new();
    limited
        .register(
            "n",
            &[FieldRule::required("v", ValueKind::Integer)
                .integer_range(9007199254740992, 9007199254740992)],
        )
        .unwrap();
    assert!(
        Document::load(
            "ferravex_scene 1; node n {component n {v int 9007199254740993;}}",
            &limited
        )
        .is_err()
    );
    let mut deep = String::from("ferravex_scene 1;");
    for i in 0..=MAX_HIERARCHY_DEPTH {
        deep.push_str(&format!(
            "node n{i} {{ {} }}",
            if i > 0 {
                format!("parent n{};", i - 1)
            } else {
                String::new()
            }
        ));
    }
    failure(&deep, ErrorKind::Limit);
}

#[test]
fn expansion_and_prefab_depth_budgets_are_enforced() {
    let mut source =
        String::from("ferravex_scene 1; prefab large {node root {} node a {} node b {}}");
    for i in 0..1500 {
        source.push_str(&format!("node n{i} {{use large;}}"));
    }
    failure(&source, ErrorKind::Limit);
    let mut deep = String::from("ferravex_scene 1;");
    for i in 0..=MAX_PREFAB_DEPTH {
        deep.push_str(&format!(
            "prefab p{i} {{node root {{ {} }} }}",
            if i < MAX_PREFAB_DEPTH {
                format!("use p{};", i + 1)
            } else {
                String::new()
            }
        ));
    }
    failure(&deep, ErrorKind::Limit);
    let mut source = String::from("ferravex_scene 1;prefab many {node root {component numbers {");
    let mut rules = Vec::new();
    for i in 0..32 {
        source.push_str(&format!("f{i} number 1;"));
        rules.push(FieldRule::required(&format!("f{i}"), ValueKind::Number));
    }
    source.push_str("}}}");
    for i in 0..2049 {
        source.push_str(&format!("node n{i} {{use many;}}"));
    }
    let mut schema = Schema::new();
    schema.register("numbers", &rules).unwrap();
    assert_eq!(
        Document::load(&source, &schema).err().unwrap().kind,
        ErrorKind::Limit
    );
}
fn resign(bytes: &mut [u8]) {
    let end = bytes.len() - 8;
    let sum = bytes[..end].iter().fold(0xcbf29ce484222325_u64, |h, v| {
        (h ^ u64::from(*v)).wrapping_mul(0x100000001b3)
    });
    bytes[end..].copy_from_slice(&sum.to_le_bytes());
}
#[test]
fn cooked_semantics_are_checked_after_a_valid_checksum() {
    let bytes = Document::load(SOURCE, &schema()).unwrap().encode().unwrap();
    let mut bad = bytes.clone();
    let at = bad.windows(4).position(|s| s == b"size").unwrap();
    bad[at..at + 4].copy_from_slice(b"typo");
    resign(&mut bad);
    assert_eq!(
        SceneData::decode(&bad, &schema()).err().unwrap().kind,
        ErrorKind::UnknownField
    );
    let mut bad = bytes.clone();
    let at = bad.windows(7).position(|s| s == b"enabled").unwrap();
    bad[at + 8] = 2;
    resign(&mut bad);
    assert_eq!(
        SceneData::decode(&bad, &schema()).err().unwrap().kind,
        ErrorKind::Format
    );
    let mut bad = bytes.clone();
    bad[8..12].copy_from_slice(&5000_u32.to_le_bytes());
    resign(&mut bad);
    assert_eq!(
        SceneData::decode(&bad, &schema()).err().unwrap().kind,
        ErrorKind::Limit
    );
    let mut bad = bytes.clone();
    let end = bad.len() - 8;
    bad.insert(end, 0);
    resign(&mut bad);
    assert_eq!(
        SceneData::decode(&bad, &schema()).err().unwrap().kind,
        ErrorKind::Format
    );
}
#[test]
fn world_overflow_is_rejected_instead_of_exposing_nonfinite_matrices() {
    failure(
        "ferravex_scene 1;node a {transform 0 0 0 0 0 0 1 1e30 1e30 1e30;}node b {parent a;transform 0 0 0 0 0 0 1 1e30 1e30 1e30;}",
        ErrorKind::InvalidTransform,
    );
}
