//! The edges between pack rows, held to the packs. Every `ask_when` the files an estate
//! reads declare is parsed here with satz-core, independently of the model, and the
//! model's edges must be exactly the ones whose question and gate are both rows: nothing
//! missed, nothing invented, one parent per gate, no cycle, and a child marked as
//! following its parent exactly where the binding that applies is that parent by
//! reference. The estates are the skeleton `satz interview --create` writes, as written
//! and with every pack line on, so a pin bump that brings a new dependency is checked
//! here without anyone naming it.

#[path = "fixtures/e2e/support.rs"]
mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use satz_core::satz::{File, Value};
use satz_studio_core::cst::{Cst, UseState, scan_uses};
use satz_studio_core::diag::{DiagSource, Severity};
use satz_studio_core::estate::EstateDir;
use satz_studio_core::model::{EstateModel, PackDecls};
use satz_studio_core::satz::reports::QuestionsReport;
use satz_studio_core::schema::ResourceRegistry;

/// One `ask_when` as the files declare it.
#[derive(Debug)]
struct Declared {
    parent: String,
    child: String,
    /// the question's own gates: the param, or the options of a `oneof`
    own: Vec<String>,
    /// the declaring file defaults the param to `parent` by reference
    by_reference: bool,
}

/// Every file an estate's `use` lines name, active or commented, parsed once each, in
/// order.
fn files_named(dir: &EstateDir, main: &Path, text: &str) -> Vec<(String, File)> {
    let load = dir.loader(main);
    let mut seen = BTreeSet::new();
    scan_uses(&Cst::parse(text).unwrap())
        .into_iter()
        .filter(|u| seen.insert(u.path.clone()))
        .map(|u| {
            let src = load(&u.path).unwrap_or_else(|e| panic!("{}: {e}", u.path));
            let file = satz_core::satz::parse(&src)
                .unwrap_or_else(|e| panic!("{}: line {}: {}", u.path, e.line, e.msg));
            (u.path, file)
        })
        .collect()
}

fn declared(files: &[(String, File)]) -> Vec<Declared> {
    files
        .iter()
        .flat_map(|(_, file)| {
            file.questions.iter().filter_map(move |q| {
                let parent = q.ask_when.clone()?;
                let default = file
                    .params
                    .iter()
                    .find(|(n, _, _)| *n == q.subject)
                    .map(|(_, v, _)| v);
                Some(Declared {
                    by_reference: !q.oneof
                        && matches!(default, Some(Value::Ref(r)) if *r == parent),
                    own: if q.oneof {
                        q.options.iter().map(|o| o.param.clone()).collect()
                    } else {
                        vec![q.subject.clone()]
                    },
                    child: q.subject.clone(),
                    parent,
                })
            })
        })
        .collect()
}

async fn model_of(estate: &support::Estate, main: &Path) -> EstateModel {
    let dir = EstateDir::open(&estate.root).unwrap();
    let cli = estate.cli().await;
    let report: QuestionsReport =
        support::within(cli.json_report(&["questions".to_string(), main.display().to_string()]))
            .await
            .unwrap();
    let text = support::read(main);
    let cst = Cst::parse(&text).unwrap();
    let env = dir.params(main).unwrap();
    let registry = ResourceRegistry::load_all(&dir.schema_dir()).unwrap();
    let decls = PackDecls::read(main, &cst, &dir.loader(main));
    EstateModel::build(main, &cst, Ok(&registry), &env, &report, &decls, Vec::new()).unwrap()
}

/// What every model must satisfy against what the files declare. Returns how many edges
/// there are and how many of them follow their parent.
fn holds_to_the_files(model: &EstateModel, declared: &[Declared]) -> (usize, usize) {
    let gates: BTreeSet<&str> = model
        .packs
        .iter()
        .filter_map(|r| r.gate.as_deref())
        .collect();

    // exactly the declarations whose gate and at least one own gate are rows
    let want: BTreeSet<(&str, &str)> = declared
        .iter()
        .filter(|d| gates.contains(d.parent.as_str()))
        .filter(|d| d.own.iter().any(|g| gates.contains(g.as_str())))
        .map(|d| (d.parent.as_str(), d.child.as_str()))
        .collect();
    let drawn: BTreeSet<(&str, &str)> = model
        .pack_edges
        .iter()
        .map(|e| (e.parent.as_str(), e.child.as_str()))
        .collect();
    assert_eq!(drawn, want);
    assert_eq!(drawn.len(), model.pack_edges.len(), "an edge twice");

    // every endpoint is a row, and a child's gates are its own
    for e in &model.pack_edges {
        assert!(gates.contains(e.parent.as_str()), "{e:?}");
        assert!(!e.gates.is_empty(), "{e:?}");
        let d = declared
            .iter()
            .find(|d| d.parent == e.parent && d.child == e.child)
            .unwrap();
        for g in &e.gates {
            assert!(gates.contains(g.as_str()), "{e:?}: `{g}` is no row");
            assert!(d.own.contains(g), "{e:?}: `{g}` is not the question's");
        }
        assert_eq!(e.follows, d.by_reference, "{e:?}");
    }

    // a forest: one parent per gate, and no gate is its own ancestor
    let mut parent_of: BTreeMap<&str, &str> = BTreeMap::new();
    for e in &model.pack_edges {
        for g in &e.gates {
            assert!(
                parent_of.insert(g.as_str(), e.parent.as_str()).is_none(),
                "`{g}` has two parents"
            );
        }
    }
    for &start in parent_of.keys() {
        let mut seen = BTreeSet::from([start]);
        let mut cur = start;
        while let Some(&p) = parent_of.get(cur) {
            assert!(seen.insert(p), "a cycle through `{start}`");
            cur = p;
        }
    }

    // nothing went unread and nothing was left flat
    let edge_notes: Vec<&str> = model
        .diagnostics
        .iter()
        .filter(|d| d.source == DiagSource::Model && d.severity == Severity::Info)
        .map(|d| d.message.as_str())
        .filter(|m| {
            m.contains("was not read") || m.contains("round a cycle") || m.contains("so none of")
        })
        .collect();
    assert!(edge_notes.is_empty(), "{edge_notes:#?}");

    (
        model.pack_edges.len(),
        model.pack_edges.iter().filter(|e| e.follows).count(),
    )
}

/// The skeleton as written: every pack line commented, the map line too. The packs
/// behind the commented lines are read all the same, so the tree is there before any
/// switch is on.
#[tokio::test]
async fn the_skeleton_as_written_has_the_edges_its_commented_packs_declare() {
    let estate = support::estate_dir(None);
    let main = estate.create_skeleton("C0example.satz").await;
    let dir = EstateDir::open(&estate.root).unwrap();
    let text = support::read(&main);
    assert!(
        scan_uses(&Cst::parse(&text).unwrap())
            .iter()
            .any(|u| u.state == UseState::Commented),
        "the skeleton carries commented pack lines"
    );
    let declared = declared(&files_named(&dir, &main, &text));
    assert!(
        !declared.is_empty(),
        "the library declares no ask_when at all"
    );
    let model = model_of(&estate, &main).await;
    let (edges, follows) = holds_to_the_files(&model, &declared);
    assert!(
        edges > 0,
        "the map's dependencies are edges on the skeleton"
    );
    eprintln!("skeleton: {edges} edges, {follows} following their parent");
}

/// Every line the skeleton carries, and the lines a pack's header says are added by
/// hand, all on, with every gate bound on — one option of each `oneof`, and a gate its
/// pack defaults to another by reference left to that reference. The skeleton's own order
/// does not fold with every pack on (a later phase's default names a param an earlier
/// line's pack declares further down), so the lines stand in an order satz's fold
/// resolves, found by trying. Here every `ask_when` of every file the estate names is
/// between two rows, so every one of them is an edge.
#[tokio::test]
async fn with_every_pack_on_every_ask_when_the_files_declare_is_an_edge() {
    let estate = support::estate_dir(None);
    let main = estate.create_skeleton("C0example.satz").await;
    let dir = EstateDir::open(&estate.root).unwrap();
    let load = dir.loader(&main);
    let skeleton = support::read(&main);

    let mut lines: Vec<(String, Option<String>)> = Vec::new();
    for u in scan_uses(&Cst::parse(&skeleton).unwrap()) {
        if !lines.iter().any(|(p, g)| *p == u.path && *g == u.gate) {
            lines.push((u.path, u.gate));
        }
    }
    let carried: BTreeSet<String> = lines.iter().map(|(p, _)| p.clone()).collect();
    let mut by_hand = 0;
    for path in &carried {
        let src = load(path).unwrap();
        for line in src.lines() {
            let t = line.trim_start().trim_start_matches("//").trim();
            let Some((shown, tail)) = t.strip_prefix("use \"").and_then(|r| r.split_once('"'))
            else {
                continue;
            };
            let Some(gate) = tail.trim().strip_prefix("when ") else {
                continue;
            };
            if !lines.iter().any(|(p, _)| p == shown) {
                lines.push((shown.to_string(), Some(gate.trim().to_string())));
                by_hand += 1;
            }
        }
    }
    let use_lines = |lines: &[(String, Option<String>)]| -> String {
        lines
            .iter()
            .map(|(p, g)| match g {
                Some(g) => format!("use \"{p}\" when {g}\n"),
                None => format!("use \"{p}\"\n"),
            })
            .collect()
    };

    let files = files_named(&dir, &main, &use_lines(&lines));
    let by_reference: BTreeSet<&str> = files
        .iter()
        .flat_map(|(_, f)| f.params.iter())
        .filter(|(_, v, _)| matches!(v, Value::Ref(_)))
        .map(|(n, _, _)| n.as_str())
        .collect();
    let not_first: BTreeSet<&str> = files
        .iter()
        .flat_map(|(_, f)| f.questions.iter().filter(|q| q.oneof))
        .flat_map(|q| q.options.iter().skip(1).map(|o| o.param.as_str()))
        .collect();
    let gates: BTreeSet<&str> = lines.iter().filter_map(|(_, g)| g.as_deref()).collect();
    let bindings: String = gates
        .iter()
        .filter(|g| !by_reference.contains(*g))
        .map(|g| format!("  {g} = {}\n", !not_first.contains(*g)))
        .collect();
    let text = |lines: &[(String, Option<String>)]| {
        format!(
            "estate acme\n\nparams {{\n{bindings}}}\n\n{}",
            use_lines(lines)
        )
    };

    let name = main.to_string_lossy().into_owned();
    let mut placed: Vec<(String, Option<String>)> = Vec::new();
    let mut waiting = lines;
    while !waiting.is_empty() {
        let before = waiting.len();
        let mut errors = Vec::new();
        waiting.retain(|candidate| {
            let mut trial = placed.clone();
            trial.push(candidate.clone());
            match satz_core::pipeline::estate_params(&name, &text(&trial), &load) {
                Ok(_) => {
                    placed.push(candidate.clone());
                    false
                }
                Err(e) => {
                    errors.push(format!("{}: {}:{}: {}", candidate.0, e.file, e.line, e.msg));
                    true
                }
            }
        });
        assert!(
            waiting.len() < before,
            "no order resolves these lines:\n{}",
            errors.join("\n")
        );
    }
    std::fs::write(&main, text(&placed)).unwrap();

    let after = support::read(&main);
    let files = files_named(&dir, &main, &after);
    let declared = declared(&files);
    let model = model_of(&estate, &main).await;
    let (edges, follows) = holds_to_the_files(&model, &declared);
    assert_eq!(
        edges,
        declared.len(),
        "every ask_when is an edge: {declared:#?}"
    );
    eprintln!(
        "every pack on: {edges} edges from {} files, {follows} following their parent, {by_hand} lines added by hand",
        files.len()
    );
}
