//! The `params { }` block as rows, each joined with the question that asks for it. A
//! param that is a pack row's gate — it gates a `use … when` line, or the map asks it —
//! is not a row here ([`super::packs`] has it), so one fact has one place; nor is an
//! option of any `oneof`, which is answered as a choice and never typed.
//!
//! Beside the rows, the shape of every param the fold binds, and the rule that picks
//! the shape an answer is typed in from it.

use std::collections::{BTreeMap, BTreeSet};

use satz_core::pipeline::Env;

use super::value::{decode, mode_of};
use super::{ModelError, ParamKind, ParamRow, SourceValue};
use crate::cst::{Cst, NodeKind};
use crate::satz::reports::{QuestionKind, QuestionRow, QuestionsReport};

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

/// The shape of every param `env` binds. `env` is the fold — the estate's own bindings
/// first, then every pack it uses — so a param the estate does not bind has the shape
/// its pack declares it with, a reference already resolved.
pub(super) fn shapes(env: &Env) -> BTreeMap<String, ParamKind> {
    env.iter()
        .map(|(name, value)| (name.clone(), shape_of(value)))
        .collect()
}

/// [`ParamKind::of_json`] over the YAML value the fold carries.
fn shape_of(value: &serde_yaml::Value) -> ParamKind {
    match value {
        serde_yaml::Value::Bool(_) => ParamKind::Bool,
        serde_yaml::Value::Number(_) => ParamKind::Number,
        serde_yaml::Value::Sequence(_) => ParamKind::List,
        _ => ParamKind::String,
    }
}

/// The shape the answer to the param question `q` is typed in.
///
/// The value the interview offers decides when there is one, as it decides for satz's
/// `parse_answer`. A question that offers nothing takes the shape of its param in the
/// fold, `shapes` ([`super::EstateModel::shapes`]): satz offers no empty value, so a
/// param its pack declares `[]` offers nothing and is still a list, and one address
/// typed for it is a list of one. A param the fold binds to nothing has no shape to
/// take and is a string.
///
/// `None` only when nothing is offered and the fold is not known — the estate's params
/// did not resolve — because then no shape can be told from a guess.
pub fn answer_kind(
    q: &QuestionRow,
    shapes: Option<&BTreeMap<String, ParamKind>>,
) -> Option<ParamKind> {
    match (q.offered(), shapes) {
        (Some(offered), _) => Some(ParamKind::of_json(offered)),
        (None, Some(shapes)) => Some(shapes.get(&q.subject).copied().unwrap_or(ParamKind::String)),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn question(
        current: Option<serde_json::Value>,
        default: Option<serde_json::Value>,
    ) -> QuestionRow {
        let mut q: QuestionRow = serde_json::from_value(json!({
            "subject": "access_approval_notification_emails", "kind": "param", "prompt": "p",
            "reversal": "edit", "blast": "low", "state": "unanswered", "blocking": true,
            "pack_description": "d", "from": "f", "pack": "p"
        }))
        .unwrap();
        q.current = current;
        q.default = default;
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

    /// The fold carries YAML and the report JSON; the two rules are one rule.
    #[test]
    fn the_fold_s_shapes_agree_with_the_offered_value_s() {
        for text in [
            "true", "400", "1.5", "[]", "[a, b]", "\"x\"", "\"\"", "null", "{a: 1}",
        ] {
            let yaml: serde_yaml::Value = serde_yaml::from_str(text).unwrap();
            let json = serde_json::to_value(&yaml).unwrap();
            assert_eq!(shape_of(&yaml), ParamKind::of_json(&json), "{text}");
        }
    }

    #[test]
    fn the_offered_value_decides_and_a_question_that_offers_none_takes_its_declared_shape() {
        let shapes = BTreeMap::from([(
            "access_approval_notification_emails".to_string(),
            ParamKind::List,
        )]);
        // nothing offered: the declaration's `[]`
        assert_eq!(
            answer_kind(&question(None, None), Some(&shapes)),
            Some(ParamKind::List)
        );
        // an offer decides, the estate's own value before the pack's default
        assert_eq!(
            answer_kind(
                &question(None, Some(json!(["in:eu-locations"]))),
                Some(&shapes)
            ),
            Some(ParamKind::List)
        );
        assert_eq!(
            answer_kind(&question(Some(json!("x")), Some(json!([]))), Some(&shapes)),
            Some(ParamKind::String)
        );
        // an offer needs no fold
        assert_eq!(
            answer_kind(&question(None, Some(json!(true))), None),
            Some(ParamKind::Bool)
        );
        // a param the fold binds to nothing is a string; an unknown fold is no shape
        assert_eq!(
            answer_kind(&question(None, None), Some(&BTreeMap::new())),
            Some(ParamKind::String)
        );
        assert_eq!(answer_kind(&question(None, None), None), None);
    }
}
