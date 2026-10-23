use ferravex_play::authoring::{
    ContentError, Document, ErrorKind, FieldRule, SceneData, SceneDiagnostic, Schema, SourceSpan,
    ValueKind,
};
use std::error::Error;

fn schema() -> Schema {
    let mut schema = Schema::new();
    schema
        .register(
            "body",
            &[
                FieldRule::required("mass", ValueKind::Number).range(0., 100.),
                FieldRule::optional("label", ValueKind::Text),
                FieldRule::optional("count", ValueKind::Integer)
                    .integer_range(9007199254740992, 9007199254740992),
                FieldRule::optional("target", ValueKind::Reference).target("marker"),
            ],
        )
        .unwrap();
    schema.register("marker", &[]).unwrap();
    schema
}

fn load_error(source: &str, schema: &Schema) -> SceneDiagnostic {
    let original = Document::load(source, schema).err().unwrap();
    let diagnostic = Document::load_with_diagnostics(source, schema)
        .err()
        .unwrap();
    assert_eq!(diagnostic.error, original);
    assert_eq!(
        diagnostic.source().unwrap().to_string(),
        original.to_string()
    );
    diagnostic
}

#[test]
fn malformed_values_identify_the_exact_token_and_keep_original_errors() {
    for (value, token, kind, expected) in [
        ("bool perhaps", "perhaps", ErrorKind::Type, "true or false"),
        ("number NaN", "NaN", ErrorKind::Range, "finite number"),
        ("vec3 1 1e99 2", "1e99", ErrorKind::Range, "exceeds f32"),
        ("int 1.5", "1.5", ErrorKind::Range, "i64"),
        (
            "int 9223372036854775808",
            "9223372036854775808",
            ErrorKind::Range,
            "i64",
        ),
        ("rgba 1 2 300 4", "300", ErrorKind::Range, "0..255"),
        ("float 1", "float", ErrorKind::Type, "unknown value kind"),
    ] {
        let source =
            format!("ferravex_scene 1;\nnode n {{\n component body {{\n  mass {value};\n }}\n}}");
        let original = Document::parse(&source).unwrap_err();
        let diagnostic = Document::parse_with_diagnostics(&source).unwrap_err();
        assert_eq!(diagnostic.error, original);
        assert_eq!(diagnostic.error.kind, kind);
        assert_eq!(diagnostic.error.span.line, 4);
        assert_eq!(diagnostic.token.as_deref(), Some(token));
        assert!(diagnostic.to_string().contains(expected));
        assert!(
            diagnostic
                .to_string()
                .contains(&"^".repeat(token.chars().count()))
        );
        assert_eq!(original.diagnostic(&source), diagnostic);
    }
}

#[test]
fn unknown_fields_include_the_right_component_and_allowed_fields() {
    for source in [
        "ferravex_scene 1;\nnode n { component body { mass number 1;\n  weigth number 2;\n} }",
        "ferravex_scene 1;\nprefab unused { node root { component body {\n  weigth number 2;\n} } }",
        "ferravex_scene 1;\nnode n { override child { component body {\n  weigth number 2;\n} } }",
    ] {
        let error = load_error(source, &schema());
        assert_eq!(error.error.kind, ErrorKind::UnknownField);
        assert_eq!(error.token.as_deref(), Some("weigth"));
        assert_eq!(error.error.span.line, 3);
        assert_eq!(error.source_line.as_deref(), Some("  weigth number 2;"));
        let help = error.help.unwrap();
        assert!(help.contains("Component `body`"));
        assert!(help.contains("count, label, mass, target"));
        assert!(help.contains("Rename or remove"));
    }
    let error = load_error(
        "ferravex_scene 1; node n { component marker { mass number 2; } }",
        &schema(),
    );
    assert!(error.help.unwrap().contains("`marker` accepts no fields"));
}

#[test]
fn schema_type_bounds_and_missing_fields_have_repair_guidance() {
    for (fields, kind, hints) in [
        (
            "mass text heavy;",
            ErrorKind::Type,
            vec!["body.mass", "number", "0..100"],
        ),
        (
            "mass number 101;",
            ErrorKind::Range,
            vec!["body.mass", "inclusive range 0..100"],
        ),
        ("", ErrorKind::MissingField, vec!["body.mass", "required"]),
        (
            "mass number 1; count int 9007199254740993;",
            ErrorKind::Range,
            vec!["body.count", "9007199254740992..9007199254740992"],
        ),
        (
            "mass number 1; target ref absent;",
            ErrorKind::Reference,
            vec!["body.target", "existing node path", "component `marker`"],
        ),
    ] {
        let source = format!("ferravex_scene 1; node n {{ component body {{ {fields} }} }}");
        let diagnostic = load_error(&source, &schema());
        assert_eq!(diagnostic.error.kind, kind);
        let help = diagnostic.help.unwrap();
        for hint in hints {
            assert!(help.contains(hint), "missing {hint:?} in {help:?}");
        }
    }
}

#[test]
fn unknown_component_lists_registrations_and_handles_an_empty_schema() {
    let source = "ferravex_scene 1; node n { component mesh {} }";
    let diagnostic = load_error(source, &schema());
    assert_eq!(diagnostic.error.kind, ErrorKind::UnknownComponent);
    assert_eq!(diagnostic.token.as_deref(), Some("mesh"));
    assert!(diagnostic.help.unwrap().contains("body, marker"));
    assert!(
        load_error(source, &Schema::new())
            .help
            .unwrap()
            .contains("No components are registered")
    );
}

#[test]
fn unicode_columns_byte_offsets_quotes_tabs_and_crlf_are_preserved() {
    let source = format!(
        "{}ferravex_scene 1;\r\nnode n {{ component body {{\r\n\tlabel text \"λ🚀e\u{301}\\n\"; mass number \"坏\\\"值\";\r\n}} }}",
        "# 注释\r\n".repeat(8)
    );
    let error = Document::parse_with_diagnostics(&source).unwrap_err();
    let offset = source.find("\"坏").unwrap();
    let line_start = source[..offset].rfind('\n').unwrap() + 1;
    let column = source[line_start..offset].chars().count() + 1;
    assert_eq!(
        error.error.span,
        SourceSpan {
            line: 11,
            column,
            offset
        }
    );
    assert_ne!(column, offset - line_start + 1);
    assert_eq!(error.token.as_deref(), Some("\"坏\\\"值\""));
    let line = error.source_line.as_deref().unwrap();
    assert!(!line.ends_with('\r'));
    let rendered = error.to_string();
    let caret = rendered.lines().find(|line| line.contains('^')).unwrap();
    let padding: String = source[line_start..offset]
        .chars()
        .map(|c| if c == '\t' { '\t' } else { ' ' })
        .collect();
    assert_eq!(caret, format!("   | {padding}^^^^^^"));
    assert!(rendered.contains(&format!("11 | {line}")));
}

#[test]
fn malformed_strings_keep_raw_spelling_and_multiline_excerpts_are_local() {
    for (value, token, message) in [
        ("\"λ\\q\"", "\"λ\\q\"", "invalid escape"),
        ("\"λ\nnext line\"", "\"λ", "unescaped control in string"),
        ("\"λ", "\"λ; } }", "unterminated string"),
    ] {
        let source =
            format!("ferravex_scene 1;\nnode n {{ component body {{ label text {value}; }} }}");
        let diagnostic = Document::parse_with_diagnostics(&source).unwrap_err();
        assert_eq!(diagnostic.error.span.line, 2);
        assert_eq!(diagnostic.error.context, message);
        assert_eq!(diagnostic.token.as_deref(), Some(token));
        assert!(!diagnostic.source_line.as_deref().unwrap().contains('\n'));
        assert!(diagnostic.help.is_some());
    }
}

#[test]
fn eof_has_a_real_position_and_a_single_caret_even_on_an_empty_line() {
    for (source, line, column, excerpt) in [
        ("", 1, 1, ""),
        ("ferravex_scene 1;\nnode n {\n", 3, 1, ""),
        ("ferravex_scene 1;\nnode n {", 2, 9, "node n {"),
    ] {
        let diagnostic = Document::parse_with_diagnostics(source).unwrap_err();
        assert_eq!(
            diagnostic.error.span,
            SourceSpan {
                line,
                column,
                offset: source.len()
            }
        );
        assert_eq!(diagnostic.token, None);
        assert_eq!(diagnostic.source_line.as_deref(), Some(excerpt));
        let display = diagnostic.to_string();
        assert!(display.contains("found end of input"));
        assert_eq!(display.chars().filter(|&c| c == '^').count(), 1);
    }
}

#[test]
fn externally_supplied_invalid_spans_never_slice_inside_unicode_or_invent_locations() {
    for span in [
        SourceSpan::default(),
        SourceSpan {
            line: 1,
            column: 1,
            offset: usize::MAX,
        },
        SourceSpan {
            line: 1,
            column: 2,
            offset: 1,
        }, // Inside λ.
        SourceSpan {
            line: 4,
            column: 1,
            offset: 0,
        },
        SourceSpan {
            line: 1,
            column: 9,
            offset: 0,
        },
    ] {
        let error = ContentError {
            kind: ErrorKind::Domain,
            span,
            context: "custom error".into(),
        };
        let diagnostic = error.diagnostic("λ🚀");
        assert_eq!(diagnostic.error, error);
        assert_eq!(diagnostic.token, None);
        assert_eq!(diagnostic.source_line, None);
        assert!(!diagnostic.to_string().contains('^'));
    }
}

#[test]
fn compatible_success_paths_produce_identical_cooked_bytes_and_unicode_values() {
    let source = "ferravex_scene 1;\nprefab p { node root { component body { mass number 1; label text \"λ🚀\\nsecond line\"; } } }\nnode n { use p; }";
    let schema = schema();
    let old = Document::load(source, &schema).unwrap().encode().unwrap();
    let diagnostic_parse = Document::parse_with_diagnostics(source)
        .unwrap()
        .compile(&schema)
        .unwrap()
        .encode()
        .unwrap();
    let data = Document::load_with_diagnostics(source, &schema).unwrap();
    assert_eq!(data.encode().unwrap(), old);
    assert_eq!(diagnostic_parse, old);
    assert_eq!(
        SceneData::decode(&old, &schema).unwrap().encode().unwrap(),
        old
    );
    assert_eq!(
        data.node("n")
            .unwrap()
            .component("body")
            .unwrap()
            .get("label")
            .unwrap()
            .text(),
        Some("λ🚀\nsecond line")
    );
}
