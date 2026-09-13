//! The `params { }` block as rows, each joined with the question that asks for it. A
//! param that is a pack row's gate — it gates a `use … when` line, or the map asks it —
//! is not a row here ([`super::packs`] has it), so one fact has one place; nor is an
//! option of any `oneof`, which is answered as a choice and never typed.

use std::collections::BTreeSet;

use satz_core::pipeline::Env;

use super::value::{decode, mode_of};
use super::{ModelError, ParamKind, ParamRow, SourceValue};
use crate::cst::{Cst, NodeKind};
use crate::satz::reports::{QuestionKind, QuestionsReport};

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
        SourceValue::Ref { resolved, .. } => match resolved {
            Some(serde_json::Value::Bool(_)) => ParamKind::Bool,
            Some(serde_json::Value::Number(_)) => ParamKind::Number,
            Some(serde_json::Value::Array(_)) => ParamKind::List,
            _ => ParamKind::String,
        },
    }
}
