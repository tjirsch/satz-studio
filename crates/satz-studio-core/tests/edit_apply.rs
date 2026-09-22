//! `EditSession::apply` over the showcase estate: one value's bytes change and nothing
//! else on its line, a param is rewritten in place or appended as satz appends it,
//! several edits land in one pass, a string with quotes and braces reads back through
//! satz-core, and what is not a value or not one node is refused. Nothing here writes.

#[path = "fixtures/edit/support.rs"]
mod support;

use satz_core::satz::{StrPart, Value};
use satz_studio_core::cst::{NodeId, NodeKind, TypedValue};
use satz_studio_core::edit::{Edit, EditError, EditSession};

fn showcase() -> EditSession {
    EditSession::open(&support::smoke_yaml().join("showcase.satz")).unwrap()
}

/// The session's text with `node`'s span replaced by `with`.
fn spliced(session: &EditSession, node: NodeId, with: &str) -> String {
    let span = session.cst().node(node).span;
    format!(
        "{}{}{}",
        &session.text()[..span.start],
        with,
        &session.text()[span.end..]
    )
}

fn num(n: &str) -> TypedValue {
    TypedValue::Num(n.to_string())
}

fn string(s: &str) -> TypedValue {
    TypedValue::Str(s.to_string())
}

#[test]
fn replace_value_changes_only_the_value_bytes_of_an_aligned_commented_line() {
    let s = showcase();
    let entry = s.cst().param("audit_retention_days").unwrap();
    let value = support::param_value(s.cst(), "audit_retention_days");
    let p = s
        .apply(&[Edit::ReplaceValue {
            node: entry,
            value: num("30"),
        }])
        .unwrap();
    assert_eq!(p.text(), spliced(&s, value, "30"));
    let before: Vec<&str> = s.text().lines().collect();
    let after: Vec<&str> = p.text().lines().collect();
    assert_eq!(before.len(), after.len());
    assert_eq!(
        before[18],
        "  audit_retention_days     = 400            # a number; interpolates like a string"
    );
    assert_eq!(after[18], before[18].replacen("400", "30", 1));
    for (i, (a, b)) in before.iter().zip(&after).enumerate() {
        if i != 18 {
            assert_eq!(a, b, "line {}", i + 1);
        }
    }
    // naming the value node itself is the same edit
    let q = s
        .apply(&[Edit::ReplaceValue {
            node: value,
            value: num("30"),
        }])
        .unwrap();
    assert_eq!(q.text(), p.text());
}

#[test]
fn replace_param_rewrites_in_place_and_appends_when_absent() {
    let s = showcase();
    let value = support::param_value(s.cst(), "default_region");
    let p = s
        .apply(&[Edit::ReplaceParam {
            name: "default_region".to_string(),
            value: string("europe-west1"),
        }])
        .unwrap();
    assert_eq!(p.text(), spliced(&s, value, "\"europe-west1\""));
    assert!(
        p.text()
            .contains("  default_region           = \"europe-west1\"\n"),
        "the alignment stays"
    );

    let p = s
        .apply(&[Edit::ReplaceParam {
            name: "logsink_retention_days".to_string(),
            value: num("400"),
        }])
        .unwrap();
    // an append lays the block out as `satz fmt` does; the fixture is formatted, so the
    // one change is the new line before the block's `}`, in its run's `=` column. The
    // block's last entry is a list spanning lines, and a value that does not finish on
    // its line ends the run it stands in — so the appended line is a run of its own and
    // its `=` sits one space past its own name
    let close = s.text().find("\n}\n").unwrap() + 1;
    let expected = format!(
        "{}  logsink_retention_days = 400\n{}",
        &s.text()[..close],
        &s.text()[close..]
    );
    assert_eq!(p.text(), expected);
}

/// Where `ch` stands on the line binding `name`.
fn at(text: &str, name: &str, ch: char) -> Option<usize> {
    text.lines()
        .find(|l| l.trim_start().starts_with(&format!("{name} ")))
        .and_then(|l| l.find(ch))
}

#[test]
fn several_edits_land_in_one_apply() {
    let s = showcase();
    let retention = support::param_value(s.cst(), "audit_retention_days");
    let p = s
        .apply(&[
            Edit::ReplaceValue {
                node: retention,
                value: num("30"),
            },
            Edit::ReplaceParam {
                name: "customer_shortname".to_string(),
                value: string("acme"),
            },
            Edit::ReplaceParam {
                name: "new_flag".to_string(),
                value: TypedValue::Bool(true),
            },
            Edit::ReplaceParam {
                name: "new_list".to_string(),
                value: TypedValue::List(vec![string("a"), string("b")]),
            },
        ])
        .unwrap();
    let text = p.text();
    // the appends lay the block out: the `=` of every run in one column, and the
    // trailing comments in theirs, the shortened value's included
    assert!(text.contains("  audit_retention_days     = 30 "), "{text}");
    assert_eq!(
        at(text, "audit_retention_days", '#'),
        at(text, "want_optional", '#'),
        "{text}"
    );
    assert!(
        text.contains("  customer_shortname       = \"acme\"\n"),
        "{text}"
    );
    // the two appends are a run of their own: the list that ends the block spans lines
    // and closes the run above it, so they align with each other and with nothing else
    assert_eq!(
        at(text, "new_flag", '='),
        at(text, "new_list", '='),
        "{text}"
    );
    assert_eq!(at(text, "new_flag", '='), Some(11), "{text}");
    assert!(text.contains(" = true\n  new_list"), "{text}");
    assert!(text.contains(" = [\"a\", \"b\"]\n}\n"), "{text}");
    assert!(
        text.contains("  customer_id              = \"C0example\"\n"),
        "{text}"
    );
    assert_eq!(text.lines().count(), s.text().lines().count() + 2);

    let same = support::param_value(s.cst(), "customer_id");
    let err = s
        .apply(&[
            Edit::ReplaceValue {
                node: same,
                value: string("C0example"),
            },
            Edit::ReplaceParam {
                name: "customer_id".to_string(),
                value: string("C0example"),
            },
        ])
        .unwrap_err();
    assert!(matches!(err, EditError::Duplicate(_)), "{err}");
}

#[test]
fn a_node_that_is_not_a_value_is_refused() {
    let s = showcase();
    let params = s.cst().params().unwrap();
    let block = s
        .cst()
        .nodes()
        .find(|(_, n)| matches!(n.kind, NodeKind::Block { .. }))
        .map(|(id, _)| id)
        .unwrap();
    let comment = s
        .cst()
        .nodes()
        .find(|(_, n)| n.kind == NodeKind::Comment)
        .map(|(id, _)| id)
        .unwrap();
    for node in [params, block, comment] {
        let err = s
            .apply(&[Edit::ReplaceValue {
                node,
                value: num("1"),
            }])
            .unwrap_err();
        assert!(matches!(err, EditError::NotAValue(n) if n == node), "{err}");
    }
    let err = s
        .apply(&[Edit::ReplaceValue {
            node: 1_000_000,
            value: num("1"),
        }])
        .unwrap_err();
    assert!(matches!(err, EditError::NodeNotFound(1_000_000)), "{err}");
}

#[test]
fn a_raw_value_that_breaks_the_structure_is_refused() {
    let s = showcase();
    let id = support::param_value(s.cst(), "customer_id");
    let err = s
        .apply(&[Edit::ReplaceValue {
            node: id,
            value: TypedValue::Raw("\"x\" y = 1".to_string()),
        }])
        .unwrap_err();
    assert!(
        matches!(
            err,
            EditError::Syntax { .. } | EditError::ChangedElsewhere { line: 13 }
        ),
        "{err}"
    );
    let err = s
        .apply(&[Edit::ReplaceValue {
            node: id,
            value: TypedValue::Raw("\"open".to_string()),
        }])
        .unwrap_err();
    assert!(matches!(err, EditError::Syntax { .. }), "{err}");
}

#[test]
fn a_string_with_quotes_and_braces_reads_back_through_satz_core() {
    let s = showcase();
    let wanted = "say \"hi\" {x} \\ end";
    let p = s
        .apply(&[Edit::ReplaceParam {
            name: "customer_domain".to_string(),
            value: string(wanted),
        }])
        .unwrap();
    assert!(
        p.text()
            .contains("  customer_domain          = \"say \\\"hi\\\" {{x}} \\\\ end\"\n"),
        "{}",
        p.text()
    );
    let file = satz_core::satz::parse(p.text()).unwrap();
    let (_, value, _) = file
        .params
        .iter()
        .find(|(n, _, _)| n == "customer_domain")
        .unwrap();
    let Value::Str(parts) = value else {
        panic!("{value:?}")
    };
    let mut read = String::new();
    for part in parts {
        match part {
            StrPart::Lit(l) => read.push_str(l),
            other => panic!("not a literal: {other:?}"),
        }
    }
    assert_eq!(read, wanted);
}

#[test]
fn a_file_without_params_refuses_an_append_and_another_extension_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let bare = dir.path().join("bare.satz");
    std::fs::write(&bare, "estate bare\n").unwrap();
    let s = EditSession::open(&bare).unwrap();
    let err = s
        .apply(&[Edit::ReplaceParam {
            name: "a".to_string(),
            value: num("1"),
        }])
        .unwrap_err();
    assert!(matches!(err, EditError::NoParamsBlock), "{err}");

    let other = dir.path().join("notes.txt");
    std::fs::write(&other, "estate bare\n").unwrap();
    assert!(matches!(
        EditSession::open(&other),
        Err(EditError::NotSatz(p)) if p == other
    ));
}
