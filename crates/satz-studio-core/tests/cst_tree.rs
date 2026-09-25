//! The shape of the tree over one file with every construct, and the tree over text
//! satz refuses: it still exists, still is the file, and names what is wrong.

use satz_studio_core::cst::{Cst, NodeId, NodeKind, Span, ValueKind};

const SAMPLE: &str = "// header\nestate showcase\n\nparams {\n  a = 1   # trailing\n  b = \"{a}-x\"\n}\n\n// use \"presets/x.satz\" when use_x\ngoogle_folder infra {\n  display_name = \"Infrastructure\"\n  use \"p.satz\" as k when g\n  nested {\n    list = [1, \"two\", ref, { k = true }]\n  }\n}\n\nclaim \"cis-gcp\" \"4.0\" \"1.1\" implements {\n  // inside\n  resources = [\"google_folder.infra\"]\n}\nhcl trust \"why\" {\n  resource \"x\" \"y\" { a = 1 } // c\n}\n";

fn label(kind: &NodeKind) -> String {
    match kind {
        NodeKind::Document => "Document".into(),
        NodeKind::Header { .. } => "Header".into(),
        NodeKind::Params => "Params".into(),
        NodeKind::ParamEntry { .. } => "ParamEntry".into(),
        NodeKind::Use { .. } => "Use".into(),
        NodeKind::Block { .. } => "Block".into(),
        NodeKind::Attr { .. } => "Attr".into(),
        NodeKind::Value(k) => format!("{k:?}"),
        NodeKind::Opaque { statement } => format!("Opaque({statement})"),
        NodeKind::Comment => "Comment".into(),
        NodeKind::Error { message } => format!("Error({message})"),
    }
}

fn labels(cst: &Cst, id: NodeId) -> Vec<String> {
    cst.node(id)
        .children
        .iter()
        .map(|&c| label(&cst.node(c).kind))
        .collect()
}

fn child(cst: &Cst, id: NodeId, index: usize) -> NodeId {
    cst.node(id).children[index]
}

#[test]
fn the_root_lists_the_statements_in_order() {
    let cst = Cst::parse(SAMPLE).unwrap();
    let root = cst.node(cst.root());
    assert_eq!(root.kind, NodeKind::Document);
    assert_eq!(
        root.span,
        Span {
            start: 0,
            end: SAMPLE.len()
        }
    );
    assert_eq!(root.line, 1);
    assert_eq!(
        labels(&cst, cst.root()),
        [
            "Comment",
            "Header",
            "Params",
            "Comment",
            "Block",
            "Opaque(claim)",
            "Opaque(hcl)"
        ]
    );
    let header = child(&cst, cst.root(), 1);
    assert_eq!(
        cst.node(header).kind,
        NodeKind::Header {
            keyword: "estate".into(),
            name: "showcase".into()
        }
    );
    assert_eq!(cst.node(header).line, 2);
    assert_eq!(cst.slice(cst.node(header).span), "estate showcase");
}

#[test]
fn params_carry_name_eq_and_value_spans_and_a_trailing_comment_is_a_node() {
    let cst = Cst::parse(SAMPLE).unwrap();
    let a = cst.param("a").expect("param a");
    let NodeKind::ParamEntry { name, eq, value } = cst.node(a).kind else {
        panic!("{:?}", cst.node(a).kind)
    };
    assert_eq!(cst.slice(name), "a");
    assert_eq!(cst.slice(eq), "=");
    assert_eq!(cst.node(value).kind, NodeKind::Value(ValueKind::Num));
    assert_eq!(
        cst.slice(cst.node(value).span),
        "1",
        "the value span excludes the trailing comment"
    );
    assert_eq!(cst.node(a).line, 5);
    let comment = cst
        .nodes_at_line(5)
        .into_iter()
        .find(|&id| cst.node(id).kind == NodeKind::Comment)
        .expect("the # comment on line 5 is a node");
    assert_eq!(cst.slice(cst.node(comment).span), "# trailing");
    let b = cst.param("b").expect("param b");
    let NodeKind::ParamEntry { value, .. } = cst.node(b).kind else {
        panic!()
    };
    assert_eq!(cst.node(value).kind, NodeKind::Value(ValueKind::Str));
    assert_eq!(cst.slice(cst.node(value).span), "\"{a}-x\"");
    assert!(cst.param("c").is_none());
}

#[test]
fn a_block_carries_key_name_and_braces_and_its_entries_are_children() {
    let cst = Cst::parse(SAMPLE).unwrap();
    let block = child(&cst, cst.root(), 4);
    let NodeKind::Block {
        key,
        name,
        open,
        close,
    } = cst.node(block).kind
    else {
        panic!()
    };
    assert_eq!(cst.slice(key), "google_folder");
    assert_eq!(cst.slice(name.expect("named")), "infra");
    assert_eq!(cst.slice(open), "{");
    assert_eq!(cst.slice(close), "}");
    assert_eq!(close.end, cst.node(block).span.end);
    assert_eq!(labels(&cst, block), ["Attr", "Use", "Block"]);

    let attr = child(&cst, block, 0);
    let NodeKind::Attr { key, eq, value } = cst.node(attr).kind else {
        panic!()
    };
    assert_eq!(cst.slice(key), "display_name");
    assert_eq!(cst.slice(eq), "=");
    assert_eq!(cst.slice(cst.node(value).span), "\"Infrastructure\"");

    let use_ = child(&cst, block, 1);
    let NodeKind::Use { path, as_key, when } = cst.node(use_).kind else {
        panic!()
    };
    assert_eq!(cst.slice(path), "\"p.satz\"");
    assert_eq!(cst.slice(as_key.unwrap()), "k");
    assert_eq!(cst.slice(when.unwrap()), "g");

    let nested = child(&cst, block, 2);
    let NodeKind::Block { key, name, .. } = cst.node(nested).kind else {
        panic!()
    };
    assert_eq!(cst.slice(key), "nested");
    assert!(name.is_none());
    let list_attr = child(&cst, nested, 0);
    let NodeKind::Attr { value: list, .. } = cst.node(list_attr).kind else {
        panic!()
    };
    assert_eq!(cst.node(list).kind, NodeKind::Value(ValueKind::List));
    assert_eq!(labels(&cst, list), ["Num", "Str", "Ref", "Obj"]);
    let obj = child(&cst, list, 3);
    assert_eq!(labels(&cst, obj), ["Attr"]);
    let NodeKind::Attr { key, value, .. } = cst.node(child(&cst, obj, 0)).kind else {
        panic!()
    };
    assert_eq!(cst.slice(key), "k");
    assert_eq!(cst.node(value).kind, NodeKind::Value(ValueKind::Bool));
}

#[test]
fn opaque_statements_span_whole_and_keep_their_comments() {
    let cst = Cst::parse(SAMPLE).unwrap();
    let claim = child(&cst, cst.root(), 5);
    assert_eq!(
        cst.node(claim).kind,
        NodeKind::Opaque {
            statement: "claim".into()
        }
    );
    assert!(
        cst.slice(cst.node(claim).span)
            .starts_with("claim \"cis-gcp\"")
    );
    assert!(cst.slice(cst.node(claim).span).ends_with('}'));
    assert_eq!(labels(&cst, claim), ["Comment"]);
    assert_eq!(cst.slice(cst.node(child(&cst, claim, 0)).span), "// inside");
    let hcl = child(&cst, cst.root(), 6);
    assert_eq!(
        cst.node(hcl).kind,
        NodeKind::Opaque {
            statement: "hcl".into()
        }
    );
    assert_eq!(labels(&cst, hcl), ["Comment"]);
    assert_eq!(cst.slice(cst.node(child(&cst, hcl, 0)).span), "// c");
}

#[test]
fn the_pack_header_and_a_version_string() {
    let cst = Cst::parse("pack showcase_pack version \"2.11\"\n\"key\" = 1\n").unwrap();
    let header = child(&cst, cst.root(), 0);
    assert_eq!(
        cst.node(header).kind,
        NodeKind::Header {
            keyword: "pack".into(),
            name: "showcase_pack".into()
        }
    );
    assert_eq!(labels(&cst, header), ["Str"]);
    assert_eq!(cst.slice(cst.node(child(&cst, header, 0)).span), "\"2.11\"");
    let attr = child(&cst, cst.root(), 1);
    let NodeKind::Attr { key, .. } = cst.node(attr).kind else {
        panic!()
    };
    assert_eq!(cst.slice(key), "\"key\"");
}

#[test]
fn an_empty_file_is_a_document_without_children() {
    let cst = Cst::parse("").unwrap();
    assert_eq!(cst.nodes().count(), 1);
    assert_eq!(cst.node(cst.root()).kind, NodeKind::Document);
    assert!(cst.params().is_none());
}

#[test]
fn malformed_text_still_yields_a_tree_that_is_the_file_and_names_the_error() {
    for (label, text) in [
        ("missing value", "estate x\nparams {\n  a =\n}\n"),
        ("missing close brace", "google_folder {\n  a = 1\n"),
        ("unterminated string", "a = \"unterminated\n"),
        ("param without =", "params { a 1 }\n"),
        ("header without name", "estate\n"),
        ("garbage", "!!! garbage\n"),
        ("stray brace", "estate x\n}\n"),
    ] {
        let cst = Cst::parse(text).unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_eq!(cst.text(), text, "{label}");
        let errors: Vec<_> = cst
            .nodes()
            .filter_map(|(_, n)| match &n.kind {
                NodeKind::Error { message } => Some(message.clone()),
                _ => None,
            })
            .collect();
        assert!(!errors.is_empty(), "{label}: no Error node");
        assert!(
            errors.iter().all(|m| !m.is_empty()),
            "{label}: an empty error message"
        );
        assert!(cst.lower().is_err(), "{label}: satz accepts it");
    }
}

/// `export` and `interface` — what an estate publishes to the HCL beside it — are
/// statements the tree keeps whole, as satz v0.84.0's `init` writes them.
#[test]
fn an_export_and_an_interface_are_kept_whole() {
    let text = "estate acme\n\nexport \"workload_folder\" = \"organizations/{customer_organization_id}\" description \"where the teams' folders live\"\n\ninterface \"team_a\" {\n  // the team's own\n  export \"bucket\" = \"${{google_storage_bucket.b.name}}\"\n}\n";
    let cst = Cst::parse(text).unwrap();
    assert_eq!(
        labels(&cst, cst.root()),
        ["Header", "Opaque(export)", "Opaque(interface)"]
    );
    let interface = child(&cst, cst.root(), 2);
    assert!(
        cst.slice(cst.node(interface).span)
            .starts_with("interface \"team_a\"")
    );
    assert_eq!(labels(&cst, interface), ["Comment"]);
}
