//! Every `.satz` file under `vendor/satz`, through the document layer and through
//! satz-core: `text()` is the file, the tree has no `Error` node where satz accepts the
//! file, and the tree and the AST agree on the header, the params and every entry.

use std::fs;
use std::path::{Path, PathBuf};

use satz_core::satz::{Entry, File, Key, StrPart, Value};
use satz_studio_core::cst::{Cst, NodeKind};

fn vendor() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/satz")
}

/// Every `.satz` file below `dir` that is not a `.diff.satz` (an adoption delta, not a
/// file of the language).
fn satz_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let path = entry.unwrap().path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if path.is_dir() {
            if name == "target" || name == ".git" {
                continue;
            }
            satz_files(&path, out);
        } else if name.ends_with(".satz") && !name.ends_with(".diff.satz") {
            out.push(path);
        }
    }
}

fn corpus() -> Vec<(String, String)> {
    let root = vendor();
    let mut files = Vec::new();
    satz_files(&root, &mut files);
    files.sort();
    assert!(
        files.len() >= 70,
        "expected the satz corpus under {}, found {} files",
        root.display(),
        files.len()
    );
    files
        .into_iter()
        .map(|p| {
            let name = p.strip_prefix(&root).unwrap().display().to_string();
            let text = fs::read_to_string(&p).unwrap_or_else(|e| panic!("{name}: {e}"));
            (name, text)
        })
        .collect()
}

fn errors(cst: &Cst) -> Vec<String> {
    cst.nodes()
        .filter_map(|(_, n)| match &n.kind {
            NodeKind::Error { message } => Some(format!("line {}: {message}", n.line)),
            _ => None,
        })
        .collect()
}

#[test]
fn text_is_the_file_and_errors_stand_only_where_satz_refuses() {
    let mut refused = Vec::new();
    for (name, text) in corpus() {
        let cst = Cst::parse(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(cst.text(), text, "{name}: text() is not the file");
        let errors = errors(&cst);
        match cst.lower() {
            Ok(_) => assert!(
                errors.is_empty(),
                "{name}: satz accepts the file but the tree has errors: {errors:?}"
            ),
            Err(e) => refused.push(format!(
                "{name}: satz refuses it ({e}); tree errors: {errors:?}"
            )),
        }
    }
    assert!(
        refused.is_empty(),
        "the pinned corpus holds no file satz refuses, yet:\n{}",
        refused.join("\n")
    );
}

#[test]
fn every_node_is_in_document_order_inside_its_parent() {
    for (name, text) in corpus() {
        let cst = Cst::parse(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut last_start = 0;
        for (id, node) in cst.nodes() {
            assert!(
                node.span.start >= last_start,
                "{name}: node {id} at {:?} is before its predecessor",
                node.span
            );
            last_start = node.span.start;
            for &child in &node.children {
                assert!(
                    child > id,
                    "{name}: node {id} has child {child} with a smaller id"
                );
                let c = cst.node(child);
                assert!(
                    c.span.start >= node.span.start && c.span.end <= node.span.end,
                    "{name}: child {child} {:?} is outside its parent {id} {:?}",
                    c.span,
                    node.span
                );
            }
            let expected_line = text[..node.span.start].matches('\n').count() as u32 + 1;
            assert_eq!(node.line, expected_line, "{name}: node {id} line");
        }
    }
}

#[test]
fn the_tree_and_the_ast_agree() {
    for (name, text) in corpus() {
        let cst = Cst::parse(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        let file = cst
            .lower()
            .unwrap_or_else(|e| panic!("{name}: satz refuses it: {e}"));
        differential(&name, &cst, &file);
    }
}

fn differential(name: &str, cst: &Cst, file: &File) {
    let header = cst.nodes().find_map(|(_, n)| match &n.kind {
        NodeKind::Header { keyword, name } => Some((keyword.clone(), name.clone())),
        _ => None,
    });
    match header {
        Some((keyword, hname)) => {
            assert_eq!(
                file.estate.as_deref(),
                Some(hname.as_str()),
                "{name}: the header name"
            );
            assert_eq!(
                file.is_pack,
                keyword == "pack",
                "{name}: the header keyword is `{keyword}`"
            );
        }
        None => assert!(
            file.estate.is_none(),
            "{name}: satz sees a header the tree does not"
        ),
    }

    let got: Vec<(String, usize)> = cst
        .params()
        .map(|p| {
            cst.node(p)
                .children
                .iter()
                .filter_map(|&c| match &cst.node(c).kind {
                    NodeKind::ParamEntry { name, .. } => {
                        Some((cst.slice(*name).to_string(), cst.node(c).line as usize))
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    let want: Vec<(String, usize)> = file
        .params
        .iter()
        .map(|(n, _, l)| (n.clone(), *l))
        .collect();
    assert_eq!(got, want, "{name}: the params");

    let mut want: Vec<(usize, String)> = Vec::new();
    walk_entries(&file.items, &mut want);
    for (_, v, _) in &file.params {
        walk_value(v, &mut want);
    }
    want.sort();
    let mut got: Vec<(usize, String)> = cst
        .nodes()
        .filter_map(|(_, n)| match &n.kind {
            NodeKind::Attr { key, .. } | NodeKind::Block { key, .. } => {
                Some((n.line as usize, cst_key(cst.slice(*key))))
            }
            NodeKind::Use { path, .. } => Some((n.line as usize, cst_key(cst.slice(*path)))),
            _ => None,
        })
        .collect();
    got.sort();
    if got != want {
        let only_tree: Vec<_> = got.iter().filter(|g| !want.contains(g)).collect();
        let only_ast: Vec<_> = want.iter().filter(|w| !got.contains(w)).collect();
        panic!(
            "{name}: entries differ\n  only in the tree: {only_tree:?}\n  only in the AST:  {only_ast:?}"
        );
    }
}

fn walk_entries(entries: &[Entry], out: &mut Vec<(usize, String)>) {
    for e in entries {
        match e {
            Entry::Attr { key, value, line } => {
                out.push((*line, key_form(key)));
                walk_value(value, out);
            }
            Entry::Map {
                key, body, line, ..
            } => {
                out.push((*line, key_form(key)));
                walk_entries(body, out);
            }
            Entry::Use { path, line, .. } => {
                out.push((*line, format!("str:{:?}", vec![StrPart::Lit(path.clone())])));
            }
        }
    }
}

fn walk_value(v: &Value, out: &mut Vec<(usize, String)>) {
    match v {
        Value::Obj(entries) => walk_entries(entries, out),
        Value::List(items) => items.iter().for_each(|i| walk_value(i, out)),
        _ => {}
    }
}

fn key_form(k: &Key) -> String {
    match k {
        Key::Ident(s) => format!("ident:{s}"),
        Key::Str(parts) => format!("str:{parts:?}"),
    }
}

/// The tree's key text in the AST's form: a quoted key through satz's own string
/// reader, a bare one as the identifier.
fn cst_key(slice: &str) -> String {
    if !slice.starts_with('"') {
        return format!("ident:{slice}");
    }
    let file = satz_core::satz::parse(&format!("x = {slice}\n"))
        .unwrap_or_else(|e| panic!("key {slice}: {e}"));
    match &file.items[..] {
        [
            Entry::Attr {
                value: Value::Str(parts),
                ..
            },
        ] => format!("str:{parts:?}"),
        other => panic!("key {slice} read as {other:?}"),
    }
}
