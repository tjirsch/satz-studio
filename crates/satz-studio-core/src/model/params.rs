//! The `params { }` block as rows, each joined with the question that asks for it. A
//! param that is a pack's gate in satz's pack report is not a row here (the Packs view
//! switches it), so one fact has one place; nor is an option of any `oneof`, which is
//! answered as a choice and never typed.
//!
//! Beside the rows, the rule that picks the shape an answer is typed in.

use std::collections::BTreeSet;

use satz_core::pipeline::Env;

use super::value::{decode, mode_of};
use super::{ModelError, ParamKind, ParamRow, SourceValue};
use crate::cst::{Cst, NodeKind};
use crate::satz::reports::{QuestionKind, QuestionRow, QuestionsReport, Shape};

/// The rows, minus the params in `gates`.
pub(super) fn build(
    cst: &Cst,
    env: &Env,
    questions: &QuestionsReport,
    gates: &BTreeSet<&str>,
) -> Result<Vec<ParamRow>, ModelError> {
    let Some(params) = cst.params() else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for &id in &cst.node(params).children {
        let node = cst.node(id);
        let NodeKind::ParamEntry { name, value, .. } = node.kind else {
            continue;
        };
        let name = cst.slice(name).to_string();
        if gates.contains(name.as_str()) {
            continue;
        }
        let Some(value) = decode(cst, value, env)? else {
            continue;
        };
        let question = questions
            .questions
            .iter()
            .find(|q| q.kind == QuestionKind::Param && q.subject == name)
            .cloned();
        out.push(ParamRow {
            id,
            kind: kind_of(&value),
            one_way_door: question.as_ref().is_some_and(|q| q.one_way_door()),
            mode: mode_of(&value),
            question,
            value,
            name,
            line: node.line,
        });
    }
    Ok(out)
}

/// The shape `interview::parse_answer` reads an answer in: a bool, a number, a list,
/// else a string. A reference takes the shape of what it resolves to.
fn kind_of(value: &SourceValue) -> ParamKind {
    match value {
        SourceValue::Bool(_) => ParamKind::Bool,
        SourceValue::Num(_) => ParamKind::Number,
        SourceValue::List(_) => ParamKind::List,
        SourceValue::Str { .. } | SourceValue::Obj => ParamKind::String,
        SourceValue::Ref { resolved, .. } => resolved
            .as_ref()
            .map_or(ParamKind::String, ParamKind::of_json),
    }
}

/// The shape the answer to the param question `q` is typed in, read off the report the
/// same way satz's own `parse_answer` reads it: the shape the pack declares the param
/// with, else the shape of the value the interview offers. satz offers no empty value,
/// so a param its pack declares `[]` offers nothing and is still a list, and one
/// address typed for it is written as a list of one.
///
/// `None` is no typed field: a map, which satz answers only by an edit to the estate's
/// params, and a param whose declaration names no shape and which offers nothing.
pub fn answer_kind(q: &QuestionRow) -> Option<ParamKind> {
    match q.shape {
        Some(Shape::Bool) => Some(ParamKind::Bool),
        Some(Shape::Number) => Some(ParamKind::Number),
        Some(Shape::List) => Some(ParamKind::List),
        Some(Shape::String) => Some(ParamKind::String),
        Some(Shape::Map) => None,
        None => q.offered().map(ParamKind::of_json),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn question(
        current: Option<serde_json::Value>,
        default: Option<serde_json::Value>,
        shape: Option<Shape>,
    ) -> QuestionRow {
        let mut q: QuestionRow = serde_json::from_value(json!({
            "subject": "access_approval_notification_emails", "kind": "param", "prompt": "p",
            "reversal": "edit", "blast": "low", "state": "unanswered", "blocking": true,
            "pack_description": "d", "from": "f", "pack": "p"
        }))
        .unwrap();
        q.current = current;
        q.default = default;
        q.shape = shape;
        q
    }

    #[test]
    fn a_value_has_the_shape_parse_answer_reads_it_in() {
        assert_eq!(ParamKind::of_json(&json!(true)), ParamKind::Bool);
        assert_eq!(ParamKind::of_json(&json!(30)), ParamKind::Number);
        assert_eq!(ParamKind::of_json(&json!(["a"])), ParamKind::List);
        assert_eq!(ParamKind::of_json(&json!("x")), ParamKind::String);
        assert_eq!(ParamKind::of_json(&json!({"a": 1})), ParamKind::String);
        assert_eq!(ParamKind::of_json(&json!(null)), ParamKind::String);
    }

    #[test]
    fn the_declared_shape_decides_and_an_offer_answers_for_a_param_declared_without_one() {
        // nothing offered: the declaration's `[]` is a list in the report
        assert_eq!(
            answer_kind(&question(None, None, Some(Shape::List))),
            Some(ParamKind::List)
        );
        // the declaration outranks the offer, as satz's `parse_answer` reads it: the
        // string this estate carries for a param its pack declares a list is still
        // answered as a list
        assert_eq!(
            answer_kind(&question(Some(json!("x")), None, Some(Shape::List))),
            Some(ParamKind::List)
        );
        assert_eq!(
            answer_kind(&question(None, Some(json!(30)), Some(Shape::Number))),
            Some(ParamKind::Number)
        );
        assert_eq!(
            answer_kind(&question(None, None, Some(Shape::Bool))),
            Some(ParamKind::Bool)
        );
        assert_eq!(
            answer_kind(&question(None, None, Some(Shape::String))),
            Some(ParamKind::String)
        );
        // no declared shape: the offer answers for it
        assert_eq!(
            answer_kind(&question(None, Some(json!(true)), None)),
            Some(ParamKind::Bool)
        );
        // a map is written into the estate's params by hand, and a param with neither
        // a declared shape nor an offer has no field
        assert_eq!(answer_kind(&question(None, None, Some(Shape::Map))), None);
        assert_eq!(answer_kind(&question(None, None, None)), None);
    }
}
