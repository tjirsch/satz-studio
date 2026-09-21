//! The pack rows, derived from three sources the estate already has and no table: the
//! `use` lines (`scan_uses`, active or commented, with their phase comment), the
//! questions report (the question whose subject is a gate, or the `oneof` one of whose
//! options is), and the resolved params (what a gate is now). The map line comes
//! first; then one row per `use` line in document order — a [`PackRowKind::Choice`]
//! where a gate names it, a [`PackRowKind::Plain`] where none does; then one `Absent`
//! row per choice of the map pack that gates no line in the file, whose remedy is
//! `satz merge-presets`.
//!
//! An un-gated line was dropped until 2026-09-17, which is how satz's CIS baseline —
//! `use`d with no `when` before satz v0.64.0 gave it one — could be in an estate and in
//! no row of this view. A pack the estate runs is a row whether or not a question
//! decides it; what a `Plain` row does NOT get is a switch, because there is no param
//! to write.
//!
//! Beside the rows, the edges between them: every `ask_when` the files declare
//! ([`PackDecls`]) whose question and gate are both rows.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use satz_core::pipeline::Env;

use super::decls::PackDecls;
use super::value::truthy;
use super::{Choice, LineState, MAP_PACK, MAP_PATH, PackEdge, PackRow, PackRowKind};
use crate::cst::{UseLine, UseState};
use crate::diag::{DiagSource, Diagnostic, Severity};
use crate::satz::reports::{OptionRow, QuestionKind, QuestionRow, QuestionsReport};

/// The rows, and one `Info` per line that is active while its gate is false: satz
/// leaves such a pack out, and nothing else says so.
pub(super) fn build(
    main: &Path,
    env: &Env,
    questions: &QuestionsReport,
    uses: &[UseLine],
) -> (Vec<PackRow>, Vec<Diagnostic>) {
    let mut rows = Vec::new();
    let mut notes = Vec::new();

    let map_lines = uses.iter().filter(|u| u.path == MAP_PATH);
    let mut any_map = false;
    for u in map_lines {
        any_map = true;
        rows.push(PackRow {
            kind: PackRowKind::Map,
            gate: None,
            path: Some(u.path.clone()),
            state: state_of(u.state),
            choice: Choice::Line,
            question: None,
            phase: u.phase_comment.clone(),
            line: Some(u.line),
        });
    }
    if !any_map {
        rows.push(PackRow {
            kind: PackRowKind::Map,
            gate: None,
            path: Some(MAP_PATH.to_string()),
            state: LineState::Absent,
            choice: Choice::Line,
            question: None,
            phase: None,
            line: None,
        });
    }

    for u in uses {
        let Some(gate) = u.gate.as_deref() else {
            // the map is its own row above, and every other un-gated line is the file's
            // own decision: a row that states it, with nothing to switch
            if u.path != MAP_PATH {
                rows.push(PackRow {
                    kind: PackRowKind::Plain,
                    gate: None,
                    path: Some(u.path.clone()),
                    state: state_of(u.state),
                    choice: Choice::Line,
                    question: None,
                    phase: u.phase_comment.clone(),
                    line: Some(u.line),
                });
            }
            continue;
        };
        let (choice, question) = choice_of(gate, env, questions);
        rows.push(PackRow {
            kind: PackRowKind::Choice,
            gate: Some(gate.to_string()),
            path: Some(u.path.clone()),
            state: state_of(u.state),
            choice,
            question,
            phase: u.phase_comment.clone(),
            line: Some(u.line),
        });
        if u.state == UseState::Active && env.get(gate).is_some_and(|v| !truthy(Some(v))) {
            notes.push(
                Diagnostic {
                    file: None,
                    line: None,
                    severity: Severity::Info,
                    kind: None,
                    message: format!(
                        "line active, gate false: `use \"{}\" when {gate}` is in, but `{gate}` is false, so the pack is left out",
                        u.path
                    ),
                    source: DiagSource::Model,
                }
                .at(main, u.line),
            );
        }
    }

    let gated = |param: &str| uses.iter().any(|u| u.gate.as_deref() == Some(param));
    for q in questions.questions.iter().filter(|q| q.pack == MAP_PACK) {
        match q.kind {
            QuestionKind::Param => {
                if !gated(&q.subject) {
                    rows.push(absent(&q.subject, bool_choice(&q.subject, env, Some(q)), q));
                }
            }
            QuestionKind::Oneof => {
                for o in q.options.iter().filter(|o| !gated(&o.param)) {
                    rows.push(absent(&o.param, oneof_choice(q, o), q));
                }
            }
        }
    }

    (rows, notes)
}

fn state_of(state: UseState) -> LineState {
    match state {
        UseState::Active => LineState::On,
        UseState::Commented => LineState::Off,
    }
}

fn absent(gate: &str, choice: Choice, q: &QuestionRow) -> PackRow {
    PackRow {
        kind: PackRowKind::Choice,
        gate: Some(gate.to_string()),
        path: None,
        state: LineState::Absent,
        choice,
        question: Some(q.clone()),
        phase: None,
        line: None,
    }
}

/// The choice a gate is: an option of a `oneof` when one names it, else a bool with
/// the question whose subject it is, when there is one.
fn choice_of(gate: &str, env: &Env, questions: &QuestionsReport) -> (Choice, Option<QuestionRow>) {
    let oneof = questions.questions.iter().find_map(|q| {
        (q.kind == QuestionKind::Oneof)
            .then(|| q.options.iter().find(|o| o.param == gate).map(|o| (q, o)))
            .flatten()
    });
    if let Some((q, o)) = oneof {
        return (oneof_choice(q, o), Some(q.clone()));
    }
    let question = questions
        .questions
        .iter()
        .find(|q| q.kind == QuestionKind::Param && q.subject == gate);
    (bool_choice(gate, env, question), question.cloned())
}

fn bool_choice(gate: &str, env: &Env, question: Option<&QuestionRow>) -> Choice {
    Choice::Bool {
        current: env.get(gate).map(|v| truthy(Some(v))),
        default: question.and_then(|q| q.default.as_ref().and_then(serde_json::Value::as_bool)),
    }
}

fn oneof_choice(q: &QuestionRow, o: &OptionRow) -> Choice {
    Choice::OneofOption {
        group: q.subject.clone(),
        selected: o.selected,
    }
}

/// The edges between `rows`, and one `Info` per declaration that could not become one.
///
/// An `ask_when` is an edge when its gate is a row's gate and at least one of the
/// question's own gates is too — the param a question answers, or the options of a
/// `oneof`; `gates` keeps the ones that are rows. A question between params that gate
/// no line is a question's dependency, not a pack's, and is no edge.
///
/// The edges form a forest: a question waits on at most one gate. What would break
/// that is noted and drawn nowhere — a gate two declarations make wait on different
/// gates, and gates that wait on each other round a cycle — so the view never has to
/// choose a parent, and a note says why the pair is flat.
///
/// Every file that did not load or parse is a note as well, at the estate's line that
/// names it: the dependencies it declares are unknown.
pub(super) fn edges(
    main: &Path,
    rows: &[PackRow],
    decls: &PackDecls,
) -> (Vec<PackEdge>, Vec<Diagnostic>) {
    let gates: BTreeSet<&str> = rows.iter().filter_map(|r| r.gate.as_deref()).collect();
    let mut notes: Vec<Diagnostic> = decls
        .unread
        .iter()
        .map(|u| {
            let note = model_note(format!(
                "`{}` was not read, so the dependencies it declares are not drawn: {}",
                u.path, u.why
            ));
            match u.line {
                Some(line) => Diagnostic::at(note, main, line),
                None => note,
            }
        })
        .collect();

    let candidates: Vec<PackEdge> = decls
        .asks
        .iter()
        .filter(|a| gates.contains(a.when.as_str()))
        .filter_map(|a| {
            let own: Vec<&String> = if a.oneof {
                a.options.iter().collect()
            } else {
                vec![&a.subject]
            };
            let child_gates: Vec<String> = own
                .into_iter()
                .filter(|g| gates.contains(g.as_str()))
                .cloned()
                .collect();
            (!child_gates.is_empty()).then(|| PackEdge {
                parent: a.when.clone(),
                child: a.subject.clone(),
                gates: child_gates,
                follows: a.follows,
                file: a.file.clone(),
                line: a.line,
            })
        })
        .collect();
    let at = |e: &PackEdge| format!("`{}` ({} line {})", e.parent, e.file, e.line);

    let mut dropped: BTreeSet<usize> = BTreeSet::new();
    let mut waits: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, e) in candidates.iter().enumerate() {
        for g in &e.gates {
            waits.entry(g.as_str()).or_default().push(i);
        }
    }
    for (gate, on) in &waits {
        if on.len() > 1 {
            let named: Vec<String> = on.iter().map(|&i| at(&candidates[i])).collect();
            notes.push(model_note(format!(
                "`{gate}` is asked only when {}: a pack waits on one gate, so none of these is drawn",
                named.join(" and when ")
            )));
            dropped.extend(on);
        }
    }

    let parent_of: BTreeMap<&str, usize> = waits
        .iter()
        .filter(|(_, on)| on.len() == 1)
        .map(|(gate, on)| (*gate, on[0]))
        .collect();
    let mut cyclic: Vec<usize> = Vec::new();
    for i in (0..candidates.len()).filter(|i| !dropped.contains(i)) {
        let mut seen = BTreeSet::from([i]);
        let mut cur = candidates[i].parent.as_str();
        while let Some(&j) = parent_of.get(cur) {
            if j == i {
                cyclic.push(i);
                break;
            }
            if dropped.contains(&j) || !seen.insert(j) {
                break;
            }
            cur = candidates[j].parent.as_str();
        }
    }
    if !cyclic.is_empty() {
        let named: Vec<String> = cyclic
            .iter()
            .map(|&i| format!("`{}` on {}", candidates[i].child, at(&candidates[i])))
            .collect();
        notes.push(model_note(format!(
            "these questions wait on each other round a cycle, so none of them is drawn: {}",
            named.join(", ")
        )));
        dropped.extend(cyclic);
    }

    let edges = candidates
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !dropped.contains(i))
        .map(|(_, e)| e)
        .collect();
    (edges, notes)
}

fn model_note(message: String) -> Diagnostic {
    Diagnostic {
        file: None,
        line: None,
        severity: Severity::Info,
        kind: None,
        message,
        source: DiagSource::Model,
    }
}

#[cfg(test)]
mod tests {
    use super::super::decls::{AskWhen, Unread};
    use super::*;

    fn row(gate: &str) -> PackRow {
        PackRow {
            kind: PackRowKind::Choice,
            gate: Some(gate.to_string()),
            path: Some(format!("presets/{gate}.satz")),
            state: LineState::Off,
            choice: Choice::Bool {
                current: None,
                default: None,
            },
            question: None,
            phase: None,
            line: Some(1),
        }
    }

    fn use_line(path: &str, gate: &str, line: u32) -> UseLine {
        UseLine {
            path: path.to_string(),
            gate: Some(gate.to_string()),
            as_key: None,
            state: UseState::Commented,
            span: crate::cst::Span { start: 0, end: 0 },
            line,
            phase_comment: None,
        }
    }

    /// One row per line, not per gate: two packs may hang off one answer, and an
    /// estate that shows one of them is an estate whose other pack nobody can see.
    #[test]
    fn two_lines_on_one_gate_are_two_rows() {
        let uses = [
            use_line("presets/ci/runner.satz", "use_runner", 10),
            use_line("presets/ci/runner-grant.satz", "use_runner", 11),
        ];
        let questions = QuestionsReport {
            estate: "acme.satz".to_string(),
            questions: Vec::new(),
            summary: Default::default(),
        };
        let (rows, notes) = build(Path::new("acme.satz"), &Env::new(), &questions, &uses);
        assert!(notes.is_empty(), "{notes:?}");
        let gated: Vec<&PackRow> = rows
            .iter()
            .filter(|r| r.gate.as_deref() == Some("use_runner"))
            .collect();
        assert_eq!(gated.len(), 2, "{rows:?}");
        assert_eq!(
            gated
                .iter()
                .map(|r| r.path.as_deref().unwrap())
                .collect::<Vec<_>>(),
            ["presets/ci/runner.satz", "presets/ci/runner-grant.satz"]
        );
    }

    fn ask(subject: &str, when: &str) -> AskWhen {
        AskWhen {
            subject: subject.to_string(),
            oneof: false,
            options: Vec::new(),
            when: when.to_string(),
            follows: false,
            file: "presets/estate-map.satz".to_string(),
            line: 1,
        }
    }

    fn decls(asks: Vec<AskWhen>) -> PackDecls {
        PackDecls {
            asks,
            ..PackDecls::default()
        }
    }

    fn pairs(edges: &[PackEdge]) -> Vec<(String, String)> {
        edges
            .iter()
            .map(|e| (e.parent.clone(), e.child.clone()))
            .collect()
    }

    #[test]
    fn an_ask_when_between_two_rows_is_an_edge_and_one_between_plain_params_is_not() {
        let rows = [row("use_a"), row("use_b")];
        let (e, notes) = edges(
            Path::new("acme.satz"),
            &rows,
            &decls(vec![
                ask("use_b", "use_a"),
                ask("emails", "use_a"),
                ask("use_b_mode", "use_b_on"),
            ]),
        );
        assert_eq!(pairs(&e), [("use_a".to_string(), "use_b".to_string())]);
        assert_eq!(e[0].gates, ["use_b"]);
        assert!(notes.is_empty(), "{notes:?}");
    }

    #[test]
    fn a_oneof_under_a_gate_keeps_the_options_that_are_rows() {
        let rows = [row("use_a"), row("pick_x")];
        let mut pick = ask("pick", "use_a");
        pick.oneof = true;
        pick.options = vec!["pick_x".to_string(), "pick_y".to_string()];
        let (e, _) = edges(Path::new("acme.satz"), &rows, &decls(vec![pick]));
        assert_eq!(pairs(&e), [("use_a".to_string(), "pick".to_string())]);
        assert_eq!(e[0].gates, ["pick_x"]);
    }

    #[test]
    fn a_gate_that_waits_on_two_gates_is_noted_and_drawn_under_neither() {
        let rows = [row("use_a"), row("use_b"), row("use_c")];
        let (e, notes) = edges(
            Path::new("acme.satz"),
            &rows,
            &decls(vec![ask("use_c", "use_a"), ask("use_c", "use_b")]),
        );
        assert!(e.is_empty(), "{e:?}");
        assert_eq!(notes.len(), 1);
        assert!(
            notes[0]
                .message
                .contains("`use_c` is asked only when `use_a`")
        );
    }

    #[test]
    fn gates_that_wait_on_each_other_are_noted_and_left_flat() {
        let rows = [row("use_a"), row("use_b"), row("use_c")];
        let (e, notes) = edges(
            Path::new("acme.satz"),
            &rows,
            &decls(vec![
                ask("use_a", "use_b"),
                ask("use_b", "use_a"),
                ask("use_c", "use_a"),
            ]),
        );
        assert_eq!(pairs(&e), [("use_a".to_string(), "use_c".to_string())]);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].message.contains("round a cycle"), "{notes:?}");
    }

    #[test]
    fn a_file_that_was_not_read_is_a_note_at_the_line_that_names_it() {
        let d = PackDecls {
            unread: vec![Unread {
                path: "presets/gone.satz".to_string(),
                line: Some(7),
                why: "use \"presets/gone.satz\": file not found".to_string(),
            }],
            ..PackDecls::default()
        };
        let (_, notes) = edges(Path::new("acme.satz"), &[], &d);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].line, Some(7));
        assert_eq!(notes[0].severity, Severity::Info);
        assert!(notes[0].message.contains("presets/gone.satz"));
    }
}
