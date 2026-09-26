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

/// `all <type> under <folder>` stays inside its export, and a `private <type>.<label>`
/// statement is one statement kept whole: the tree is the file byte for byte, has no
/// `Error` node, and satz-core parses it.
#[test]
fn all_under_and_a_private_statement_are_kept_whole() {
    let text = "estate acme\n\nexport \"team\" = all google_project under google_folder.team_a description \"The team's projects\"\n\nprivate google_storage_bucket.logs\n";
    let cst = Cst::parse(text).unwrap();
    assert_eq!(cst.text(), text);
    assert_eq!(
        labels(&cst, cst.root()),
        ["Header", "Opaque(export)", "Opaque(private)"]
    );
    assert!(
        cst.nodes()
            .all(|(_, n)| !matches!(n.kind, NodeKind::Error { .. })),
        "an Error node where satz accepts the file"
    );
    let private = child(&cst, cst.root(), 2);
    assert_eq!(
        cst.slice(cst.node(private).span),
        "private google_storage_bucket.logs"
    );
    cst.lower().expect("satz-core parses it");
}

/// What satz v0.84.1 adds inside the two statements — `use interface` in both forms and
/// with a gate, an export's `attach [ … ]`, and `all <type>` — stays inside them: the
/// tree keeps each statement whole, is the file byte for byte, has no `Error` node, and
/// takes no `use interface` for a pack line.
#[test]
fn use_interface_attach_and_all_stay_inside_their_statements() {
    let text = "estate acme\n\nparams {\n  want_extra = true\n}\n\nexport \"folders\" = all google_folder description \"Every folder, by label\"\nexport \"buckets\" = all google_storage_bucket\n\ninterface \"audit\" {\n  export \"audit_bucket\" = \"${{google_storage_bucket.b.name}}\" description \"The audit bucket\"\n}\n\ninterface \"extra\" {\n  export \"extra_bucket\" = \"${{google_storage_bucket.b.url}}\"\n}\n\ninterface \"team\" {\n  use interface \"audit\"\n  use interface \"extra\" when want_extra\n  // the team grants on its own project\n  export \"project\" = \"${{google_project.p.project_id}}\" attach [\"google_project_iam_member\"] description \"The team's project\"\n}\n\ninterface \"ops\" {\n  use interface [\"audit\", \"extra\"]\n}\n";
    let cst = Cst::parse(text).unwrap();
    assert_eq!(cst.text(), text);
    assert_eq!(
        labels(&cst, cst.root()),
        [
            "Header",
            "Params",
            "Opaque(export)",
            "Opaque(export)",
            "Opaque(interface)",
            "Opaque(interface)",
            "Opaque(interface)",
            "Opaque(interface)"
        ]
    );
    assert!(
        cst.nodes()
            .all(|(_, n)| !matches!(n.kind, NodeKind::Error { .. })),
        "an Error node where satz accepts the file"
    );
    let team = child(&cst, cst.root(), 6);
    let whole = cst.slice(cst.node(team).span);
    assert!(whole.starts_with("interface \"team\""), "{whole}");
    assert!(whole.contains("attach [\"google_project_iam_member\"]"));
    assert_eq!(labels(&cst, team), ["Comment"]);
    assert!(
        satz_studio_core::cst::scan_uses(&cst).is_empty(),
        "`use interface` is not a pack line"
    );
    cst.lower().expect("satz-core parses it");
}

/// The file satz v0.85.0 generates for a project, `interfaces/<project>/<name>/satz/
/// interface.satz`: the header `interface "<name>"` alone on its line and nothing but
/// `central`, `output`, `lookup` and `managed` blocks. The tree is the file, the header
/// names the interface without its quotes and carries no value — satz wrote the name,
/// nothing edits it — and satz-core reads it as an interface file.
#[test]
fn a_generated_interface_file_is_its_header_and_four_kinds_of_block() {
    let text = "interface \"archive\"\n\ncentral {\n  estate        = \"showcase\"\n  organizations = [\"organizations/123456789012\"]\n}\n\noutput \"archive_project_id\" {\n  value       = \"acme-archive-001\"\n  attach      = [\"google_project_iam_member\"]\n  targets     = [\"google_project.archive\"]\n  description = \"The project's Google project\"\n}\n\nlookup \"data.google_project.archive\" {\n  reads      = \"google_project.archive\"\n  permission = \"resourcemanager.projects.get\"\n  arguments {\n    project_id = \"acme-archive-001\"\n  }\n}\n\nmanaged \"google_project.archive\" {\n  ids = [\"acme-archive-001\"]\n  keys {\n    project_id = \"acme-archive-001\"\n  }\n}\n";
    let cst = Cst::parse(text).unwrap();
    assert_eq!(cst.text(), text);
    assert_eq!(
        labels(&cst, cst.root()),
        ["Header", "Block", "Block", "Block", "Block"]
    );
    let header = child(&cst, cst.root(), 0);
    assert_eq!(
        cst.node(header).kind,
        NodeKind::Header {
            keyword: "interface".into(),
            name: "archive".into()
        }
    );
    assert!(labels(&cst, header).is_empty(), "the name is no value");
    assert_eq!(cst.slice(cst.node(header).span), "interface \"archive\"");
    assert!(
        cst.nodes()
            .all(|(_, n)| !matches!(n.kind, NodeKind::Error { .. })),
        "an Error node where satz accepts the file"
    );
    assert!(cst.params().is_none());
    assert!(satz_studio_core::cst::scan_uses(&cst).is_empty());
    let file = cst.lower().expect("satz-core reads it");
    assert!(file.estate.is_none());
    let iface = file.interface_file.expect("an interface file");
    assert_eq!(iface.name, "archive");
    assert_eq!(iface.estate, "showcase");
}

/// `interface "x" common { … }` is an interface statement kept whole, and the two sides
/// of a project estate — its `use` of a generated interface file and its
/// `${{interface.<export>}}` strings — are an ordinary use line and ordinary string
/// values: the tree is the file, the reference is the value's own text, and satz-core
/// parses both files.
#[test]
fn a_common_interface_and_an_interface_reference_round_trip() {
    let central = "estate acme\n\ninterface \"audit\" common {\n  export \"audit_bucket_name\" = \"${{google_storage_bucket.audit_logs.name}}\" description \"The audit log bucket\"\n}\n";
    let cst = Cst::parse(central).unwrap();
    assert_eq!(cst.text(), central);
    assert_eq!(labels(&cst, cst.root()), ["Header", "Opaque(interface)"]);
    let audit = child(&cst, cst.root(), 1);
    assert!(
        cst.slice(cst.node(audit).span)
            .starts_with("interface \"audit\" common {")
    );
    let file = cst.lower().expect("satz-core parses the central estate");
    assert!(
        file.interfaces
            .iter()
            .any(|i| i.name == "audit" && i.common)
    );

    let project = "estate archive_project\n\nuse \"vendor/archive/archive/satz/interface.satz\"\n\ngoogle_project {\n  archive_work {\n    name      = \"acme-archive-work\"\n    folder_id = \"${{interface.infra_folder}}\"\n    labels = {\n      central = \"${{interface.customer_domain}}\"\n    }\n  }\n}\n";
    let cst = Cst::parse(project).unwrap();
    assert_eq!(cst.text(), project);
    assert!(
        cst.nodes()
            .all(|(_, n)| !matches!(n.kind, NodeKind::Error { .. })),
        "an Error node where satz accepts the file"
    );
    let uses = satz_studio_core::cst::scan_uses(&cst);
    assert_eq!(uses.len(), 1);
    assert_eq!(uses[0].path, "vendor/archive/archive/satz/interface.satz");
    let strings: Vec<&str> = cst
        .nodes()
        .filter(|(_, n)| n.kind == NodeKind::Value(ValueKind::Str))
        .map(|(_, n)| cst.slice(n.span))
        .collect();
    assert!(
        strings.contains(&"\"${{interface.infra_folder}}\""),
        "{strings:?}"
    );
    assert!(
        strings.contains(&"\"${{interface.customer_domain}}\""),
        "{strings:?}"
    );
    cst.lower().expect("satz-core parses the project estate");
}
