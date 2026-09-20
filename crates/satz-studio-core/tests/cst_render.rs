//! `render_value` writes what satz's lexer reads back, lists in the style asked for;
//! `style_of` reads the style of the line a node sits on.

use proptest::prelude::*;
use satz_core::satz::{Entry, StrPart, Value};
use satz_studio_core::cst::{Cst, NodeId, NodeKind, StyleCtx, TypedValue, render_value, style_of};

fn showcase() -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor/satz/tests/smoke/yaml/showcase.satz");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// `x = <rendered>` through satz-core is one literal part equal to `s`, and through the
/// grammar it is a tree without errors.
fn reads_back(s: &str) {
    let rendered = render_value(&TypedValue::Str(s.to_string()), &StyleCtx::default());
    let text = format!("x = {rendered}\n");
    let file = satz_core::satz::parse(&text)
        .unwrap_or_else(|e| panic!("{s:?} rendered as {rendered}: {e}"));
    match &file.items[..] {
        [
            Entry::Attr {
                value: Value::Str(parts),
                ..
            },
        ] => {
            assert_eq!(
                parts,
                &[StrPart::Lit(s.to_string())],
                "{s:?} rendered as {rendered}"
            );
        }
        other => panic!("{s:?} rendered as {rendered} read as {other:?}"),
    }
    let cst = Cst::parse(&text).unwrap();
    assert!(
        !cst.nodes()
            .any(|(_, n)| matches!(n.kind, NodeKind::Error { .. })),
        "{s:?} rendered as {rendered}: the grammar refuses it"
    );
}

#[test]
fn quotes_backslashes_braces_and_newlines_read_back() {
    for s in [
        "",
        "plain",
        "a\"b",
        "a\\b",
        "\\",
        "\"",
        "{x}",
        "{{",
        "}",
        "}}",
        "a}}b",
        "{",
        "}{",
        "line\nbreak",
        "tab\tand\rreturn",
        "//not a comment",
        "# not a comment",
        "/* not a comment */",
        "${google_storage_bucket.x.name}",
        "ünïcödé — “quotes”",
    ] {
        reads_back(s);
    }
}

proptest! {
    #[test]
    fn any_string_reads_back(s in "[^\\x00]{0,48}") {
        reads_back(&s);
    }
}

#[test]
fn scalars_are_verbatim() {
    let ctx = StyleCtx::default();
    assert_eq!(render_value(&TypedValue::Num("-1.5".into()), &ctx), "-1.5");
    assert_eq!(
        render_value(&TypedValue::Ref("infra_project_name".into()), &ctx),
        "infra_project_name"
    );
    assert_eq!(render_value(&TypedValue::Bool(true), &ctx), "true");
    assert_eq!(render_value(&TypedValue::Bool(false), &ctx), "false");
    assert_eq!(
        render_value(&TypedValue::Raw("\"{a}-{b}\"".into()), &ctx),
        "\"{a}-{b}\""
    );
}

#[test]
fn list_styles() {
    let items = vec![
        TypedValue::Str("a".into()),
        TypedValue::Num("2".into()),
        TypedValue::Ref("r".into()),
        TypedValue::Bool(true),
    ];
    let list = TypedValue::List(items);
    let inline = StyleCtx {
        indent: "  ".into(),
        list_multiline: false,
        trailing_comma: false,
    };
    assert_eq!(render_value(&list, &inline), "[\"a\", 2, r, true]");
    let trailing = StyleCtx {
        indent: "  ".into(),
        list_multiline: true,
        trailing_comma: true,
    };
    assert_eq!(
        render_value(&list, &trailing),
        "[\n    \"a\",\n    2,\n    r,\n    true,\n  ]"
    );
    let open = StyleCtx {
        indent: "  ".into(),
        list_multiline: true,
        trailing_comma: false,
    };
    assert_eq!(
        render_value(&list, &open),
        "[\n    \"a\",\n    2,\n    r,\n    true\n  ]"
    );
    assert_eq!(render_value(&TypedValue::List(vec![]), &trailing), "[]");
    let nested = TypedValue::List(vec![TypedValue::List(vec![TypedValue::Num("1".into())])]);
    assert_eq!(
        render_value(&nested, &trailing),
        "[\n    [\n      1,\n    ],\n  ]"
    );
    assert_eq!(render_value(&nested, &inline), "[[1]]");
}

fn attr_by_key(cst: &Cst, key: &str) -> NodeId {
    cst.nodes()
        .find(|(_, n)| matches!(&n.kind, NodeKind::Attr { key: k, .. } if cst.slice(*k) == key))
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("no attribute `{key}`"))
}

fn value_of(cst: &Cst, entry: NodeId) -> NodeId {
    match cst.node(entry).kind {
        NodeKind::Attr { value, .. } | NodeKind::ParamEntry { value, .. } => value,
        ref other => panic!("{other:?} is not an entry"),
    }
}

#[test]
fn style_of_the_showcase_params() {
    let text = showcase();
    let cst = Cst::parse(&text).unwrap();
    let entry = cst
        .param("audit_retention_days")
        .expect("audit_retention_days");
    assert_eq!(
        style_of(&cst, entry),
        StyleCtx {
            indent: "  ".into(),
            list_multiline: false,
            trailing_comma: false
        }
    );
    let value = value_of(&cst, entry);
    assert_eq!(style_of(&cst, value), style_of(&cst, entry));
    assert_eq!(
        cst.slice(cst.node(value).span),
        "400",
        "the value span excludes the trailing comment"
    );
    let comment = cst
        .nodes_at_line(cst.node(entry).line)
        .into_iter()
        .find(|&id| cst.node(id).kind == NodeKind::Comment)
        .expect("the # comment on the same line");
    assert!(cst.slice(cst.node(comment).span).starts_with("# a number"));

    let columns: Vec<usize> = cst
        .node(cst.params().unwrap())
        .children
        .iter()
        .filter_map(|&c| match cst.node(c).kind {
            NodeKind::ParamEntry { eq, .. } => {
                Some(eq.start - text[..eq.start].rfind('\n').unwrap() - 1)
            }
            _ => None,
        })
        .collect();
    // a run, however many params the showcase carries: what is under test is that the
    // `=` of a run stand in one column, not how long the run is
    assert!(
        columns.len() >= 2,
        "the showcase declares a run of params: {columns:?}"
    );
    assert!(
        columns.iter().all(|&c| c == columns[0]),
        "the `=` are aligned: {columns:?}"
    );
}

#[test]
fn style_of_lists() {
    let text = showcase();
    let cst = Cst::parse(&text).unwrap();
    let services = attr_by_key(&cst, "project_service");
    assert_eq!(
        style_of(&cst, services),
        StyleCtx {
            indent: "        ".into(),
            list_multiline: true,
            trailing_comma: true
        }
    );
    let member = attr_by_key(&cst, "member");
    assert_eq!(
        style_of(&cst, member),
        StyleCtx {
            indent: "    ".into(),
            list_multiline: true,
            trailing_comma: true
        }
    );

    let cst = Cst::parse("x {\n  a = [1, 2]\n  b = [\n    1,\n    2\n  ]\n\tc = [ ]\n}\n").unwrap();
    assert_eq!(
        style_of(&cst, attr_by_key(&cst, "a")),
        StyleCtx {
            indent: "  ".into(),
            list_multiline: false,
            trailing_comma: false
        }
    );
    assert_eq!(
        style_of(&cst, attr_by_key(&cst, "b")),
        StyleCtx {
            indent: "  ".into(),
            list_multiline: true,
            trailing_comma: false
        }
    );
    assert_eq!(
        style_of(&cst, attr_by_key(&cst, "c")),
        StyleCtx {
            indent: "\t".into(),
            list_multiline: false,
            trailing_comma: false
        }
    );
}
