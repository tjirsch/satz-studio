//! The Params pane of the Estate destination: every param the estate binds that is not
//! a pack choice, grouped by the pack whose question asks for it, each as a typed field
//! in value mode or as its Satz source in source mode. A commit is one
//! `Edit::ReplaceParam` through the app's own writer.

use dioxus::prelude::*;
use satz_studio_core::cst::TypedValue;
use satz_studio_core::edit::Edit;
use satz_studio_core::model::{EditMode, ParamRow};

use crate::components::{
    Card, CardVariant, Chip, ChipKind, Draft, FieldKind, Icon, IconButton, SourceChips, TextField,
    Tooltip, TypedField,
};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt};
use crate::views::{line_text, value_source};

/// The group a row belongs to: the pack whose question asks for it, else the estate.
pub const ESTATE_GROUP: &str = "estate";

/// The rows by group in order of first appearance: a row with a question under its
/// pack, one without under [`ESTATE_GROUP`].
pub fn grouped(rows: &[ParamRow]) -> Vec<(String, Vec<ParamRow>)> {
    let mut out: Vec<(String, Vec<ParamRow>)> = Vec::new();
    for row in rows {
        let group = row
            .question
            .as_ref()
            .map(|q| q.pack.clone())
            .unwrap_or_else(|| ESTATE_GROUP.to_string());
        match out.iter_mut().find(|(g, _)| *g == group) {
            Some((_, rows)) => rows.push(row.clone()),
            None => out.push((group, vec![row.clone()])),
        }
    }
    out
}

#[component]
pub fn ParamsPane() -> Element {
    let app = use_context::<Store<AppStore>>();
    let model = app.estate().model().cloned();
    let cst = app.estate().cst().cloned();
    let loading = app.estate().loading().cloned();
    let (Some(model), Some(cst)) = (model, cst) else {
        return rsx! {
            div { class: "pane params",
                Card { variant: CardVariant::Filled, class: "params__empty",
                    Icon { name: "tune", size: 48, class: "placeholder__icon" }
                    p { "The estate model is not available — the drawer says why." }
                }
            }
        };
    };
    let groups = grouped(&model.params);
    rsx! {
        div { class: "pane params",
            if groups.is_empty() {
                Card { variant: CardVariant::Filled, class: "params__empty",
                    Icon { name: "tune", size: 48, class: "placeholder__icon" }
                    p { "The estate binds no param that is not a pack choice; the Packs destination has those." }
                }
            }
            for (group, rows) in groups {
                section { key: "{group}", class: "params__group",
                    header { class: "params__group-head",
                        Icon { name: if group == ESTATE_GROUP { "description" } else { "inventory_2" }, size: 20 }
                        h2 { class: "params__group-title", "{group}" }
                        if let Some(d) = rows.iter().find_map(|r| r.question.as_ref()).map(|q| q.pack_description.clone()) {
                            span { class: "params__group-description", "{d}" }
                        }
                    }
                    Card { variant: CardVariant::Outlined, class: "params__table",
                        for row in rows {
                            {
                                let source = value_source(&cst, row.id).unwrap_or_default();
                                let line = line_text(&cst, row.line);
                                rsx! {
                                    ParamRowView { key: "{row.name}:{source}", row, source, line, loading }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ParamRowView(row: ParamRow, source: String, line: String, loading: bool) -> Element {
    let handle = use_coroutine_handle::<EstateAction>();
    let kind = FieldKind::of_param(row.kind);
    let value_draft = Draft::of_source(&row.value, kind);
    let mut mode = use_signal(|| row.mode);
    let mut show_raw = use_signal(|| false);
    let mut raw = use_signal(|| source.clone());
    let name = row.name.clone();
    let in_source = mode() == EditMode::Source || value_draft.is_none();
    let raw_empty = raw().trim().is_empty();
    let commit_raw = {
        let name = name.clone();
        let given = source.clone();
        move || {
            let text = raw();
            if text.trim().is_empty() || text == given {
                return;
            }
            handle.send(EstateAction::CommitEdit(Edit::ReplaceParam {
                name: name.clone(),
                value: TypedValue::Raw(text),
            }));
        }
    };
    let why = row.question.as_ref().and_then(|q| q.why.clone());
    let prompt = row.question.as_ref().map(|q| q.prompt.clone());

    rsx! {
        div { class: "param-row",
            div { class: "param-row__name",
                code { "{row.name}" }
                span { class: "param-row__line", "line {row.line}" }
                if row.one_way_door {
                    Chip { kind: ChipKind::Assist, icon: "door_front", label: "one-way door", error: true, class: "param-row__door" }
                }
            }
            div { class: "param-row__field",
                if in_source {
                    div { class: "param-row__chips", SourceChips { value: row.value.clone() } }
                    TextField {
                        label: "Satz source",
                        value: raw(),
                        monospace: true,
                        disabled: loading,
                        error: raw_empty,
                        supporting: if raw_empty { "a value is needed".to_string() } else { "written as typed: a string keeps its quotes, a bare name is a param reference".to_string() },
                        oninput: move |v: String| raw.set(v),
                        onenter: {
                            let commit_raw = commit_raw.clone();
                            move |_| commit_raw()
                        },
                        onblur: move |_| commit_raw(),
                    }
                } else if let Some(draft) = value_draft.clone() {
                    TypedField {
                        kind,
                        draft,
                        label: row.name.clone(),
                        subject: row.name.clone(),
                        disabled: loading,
                        supporting: prompt.clone().unwrap_or_default(),
                        oncommit: {
                            let name = name.clone();
                            move |d: Draft| {
                                handle.send(EstateAction::CommitEdit(Edit::ReplaceParam { name: name.clone(), value: d.to_typed(kind) }));
                            }
                        },
                    }
                }
                if show_raw() {
                    pre { class: "param-row__raw", "{row.line}  {line}" }
                }
            }
            div { class: "param-row__tools",
                if let Some(why) = why {
                    Tooltip { text: why,
                        Icon { name: "help", size: 20, class: "param-row__help" }
                    }
                }
                IconButton {
                    icon: "code",
                    label: if in_source { "Edit as a value" } else { "Edit the Satz source" },
                    selected: in_source,
                    disabled: value_draft.is_none(),
                    onclick: move |_| {
                        mode.set(if mode() == EditMode::Source { EditMode::Value } else { EditMode::Source });
                    },
                }
                IconButton {
                    icon: "subject",
                    label: "Show the line",
                    selected: show_raw(),
                    onclick: move |_| show_raw.toggle(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use satz_studio_core::model::{ParamKind, SourceValue, StrPart};
    use satz_studio_core::satz::reports::QuestionRow;

    fn row(name: &str, pack: Option<&str>) -> ParamRow {
        let question = pack.map(|p| {
            serde_json::from_value::<QuestionRow>(serde_json::json!({
                "subject": name, "kind": "param", "prompt": "p", "reversal": "edit", "blast": "low",
                "state": "answered", "blocking": false, "pack_description": "d", "from": "f", "pack": p
            }))
            .unwrap()
        });
        ParamRow {
            id: 0,
            name: name.to_string(),
            value: SourceValue::Str {
                raw: String::new(),
                parts: vec![StrPart::Lit(String::new())],
            },
            kind: ParamKind::String,
            question,
            one_way_door: false,
            mode: EditMode::Value,
            line: 1,
        }
    }

    #[test]
    fn rows_group_by_their_question_s_pack_in_order_of_first_appearance() {
        let rows = vec![
            row("a", Some("core")),
            row("b", None),
            row("c", Some("cis")),
            row("d", Some("core")),
            row("e", None),
        ];
        let g = grouped(&rows);
        let shape: Vec<(String, Vec<String>)> = g
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().map(|r| r.name).collect()))
            .collect();
        assert_eq!(
            shape,
            vec![
                ("core".to_string(), vec!["a".to_string(), "d".to_string()]),
                (
                    ESTATE_GROUP.to_string(),
                    vec!["b".to_string(), "e".to_string()]
                ),
                ("cis".to_string(), vec!["c".to_string()]),
            ]
        );
    }
}
