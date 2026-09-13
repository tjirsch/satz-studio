//! The pack rows, derived from three sources the estate already has and no table: the
//! `use` lines (`scan_uses`, active or commented, with their phase comment), the
//! questions report (the question whose subject is a gate, or the `oneof` one of whose
//! options is), and the resolved params (what a gate is now). The map line comes
//! first; then one row per gated line in document order; then one `Absent` row per
//! choice of the map pack that gates no line in the file, whose remedy is
//! `satz merge-presets`.

use std::path::Path;

use satz_core::pipeline::Env;

use super::value::truthy;
use super::{Choice, LineState, MAP_PACK, MAP_PATH, PackRow, PackRowKind};
use crate::cst::{UseLine, UseState};
use crate::diag::{DiagSource, Diagnostic, Severity};
use crate::satz::reports::{OptionRow, QuestionKind, QuestionRow, QuestionsReport};

/// The rows, and one `Note` per line that is active while its gate is false: satz
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
                    severity: Severity::Note,
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
