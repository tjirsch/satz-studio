//! The Params pane of the Estate destination: every param the estate binds that is not
//! a pack choice, grouped by the pack whose question asks for it — a named use's params
//! under the use, each by the pack's own name of the param — each as a typed field
//! in value mode or as its Satz source in source mode. A commit is one
//! `Edit::ReplaceParam` through the app's own writer.

use dioxus::prelude::*;
use dioxus_icons::lucide::{
    Boxes, CircleQuestionMark, Code, DoorOpen, FileText, SlidersHorizontal, TextAlignStart,
};
use satz_studio_core::cst::TypedValue;
use satz_studio_core::edit::Edit;
use satz_studio_core::model::{EditMode, ParamRow, SourceValue};

use crate::components::badge::{Badge, BadgeVariant};
use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::card::{Card, CardContent};
use crate::components::tooltip::{Tooltip, TooltipContent, TooltipTrigger};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt};
use crate::views::{line_text, value_source};
use crate::widgets::{Draft, Field, FieldKind, SourceTags, TypedField};

/// The group a row belongs to: the pack whose question asks for it, else the estate.
pub const ESTATE_GROUP: &str = "estate";

/// The rows by group in order of first appearance: a named use's param under its use
/// (`web — packs/vm.satz`, as satz's interview heads it), a row with a question under its
/// pack, one without under [`ESTATE_GROUP`].
pub fn grouped(rows: &[ParamRow]) -> Vec<(String, Vec<ParamRow>)> {
    let mut out: Vec<(String, Vec<ParamRow>)> = Vec::new();
    for row in rows {
        let group = match (&row.named, &row.question) {
            (Some(u), _) => u.heading(),
            (None, Some(q)) => q.pack.clone(),
            (None, None) => ESTATE_GROUP.to_string(),
        };
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
                Card {
                    CardContent { class: "params__empty",
                        SlidersHorizontal { size: 48, class: "placeholder__icon" }
                        p { "The estate model is not available — the drawer says why." }
                    }
                }
            }
        };
    };
    let groups = grouped(&model.params);
    rsx! {
        div { class: "pane params",
            if groups.is_empty() {
                Card {
                    CardContent { class: "params__empty",
                        SlidersHorizontal { size: 48, class: "placeholder__icon" }
                        p { "The estate binds no param that is not a pack choice; the Presets destination has those." }
                    }
                }
            }
            for (group, rows) in groups {
                section { key: "{group}", class: "params__group",
                    header { class: "params__group-head",
                        if group == ESTATE_GROUP {
                            FileText { size: 20 }
                        } else {
                            Boxes { size: 20 }
                        }
                        h2 { class: "params__group-title", "{group}" }
                        if let Some(d) = rows.iter().find_map(|r| r.question.as_ref()).map(|q| q.pack_description.clone()) {
                            span { class: "params__group-description", "{d}" }
                        }
                    }
                    Card { class: "params__table",
                        CardContent {
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
}

/// The line under a row's typed field: the question's prompt, and for a param the file
/// declares `none` that it has no value and one must be provided — the value typed is
/// written in its place.
fn supporting_text(row: &ParamRow) -> String {
    let prompt = row.question.as_ref().map(|q| q.prompt.as_str());
    match (&row.value, prompt) {
        (SourceValue::None, Some(prompt)) => format!("no value — must be provided · {prompt}"),
        (SourceValue::None, None) => "no value — must be provided".to_string(),
        (_, prompt) => prompt.unwrap_or_default().to_string(),
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
    let supporting = supporting_text(&row);

    rsx! {
        div { class: "param-row",
            div { class: "param-row__name",
                code { title: "{row.name}", "{row.own_name()}" }
                span { class: "param-row__line", "line {row.line}" }
                if row.one_way_door {
                    Badge { variant: BadgeVariant::Destructive, class: "param-row__door",
                        DoorOpen { size: 12 }
                        "one-way door"
                    }
                }
            }
            div { class: "param-row__field",
                if in_source {
                    div { class: "param-row__tags", SourceTags { value: row.value.clone() } }
                    Field {
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
                        label_hidden: true,
                        subject: row.name.clone(),
                        disabled: loading,
                        supporting: supporting.clone(),
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
                    Tooltip {
                        TooltipTrigger {
                            CircleQuestionMark { size: 20, class: "param-row__help" }
                        }
                        TooltipContent { "{why}" }
                    }
                }
                Button {
                    variant: if in_source { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    size: ButtonSize::Icon,
                    title: if in_source { "Edit as a value" } else { "Edit the Satz source" },
                    "aria-label": if in_source { "Edit as a value" } else { "Edit the Satz source" },
                    "aria-pressed": if in_source { "true" } else { "false" },
                    disabled: value_draft.is_none(),
                    onclick: move |_| {
                        mode.set(if mode() == EditMode::Source { EditMode::Value } else { EditMode::Source });
                    },
                    Code { size: 16 }
                }
                Button {
                    variant: if show_raw() { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    size: ButtonSize::Icon,
                    title: "Show the line",
                    "aria-label": "Show the line",
                    "aria-pressed": if show_raw() { "true" } else { "false" },
                    onclick: move |_| show_raw.toggle(),
                    TextAlignStart { size: 16 }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use satz_studio_core::model::{NamedUse, ParamKind, StrPart};
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
            named: None,
            line: 1,
        }
    }

    /// A param declared `none` says it has no value beside its prompt; a param with a
    /// value shows the prompt alone.
    #[test]
    fn a_none_param_says_it_has_no_value() {
        let mut none = row("region", Some("core"));
        none.value = SourceValue::None;
        assert_eq!(supporting_text(&none), "no value — must be provided · p");
        let mut bare = row("region", None);
        bare.value = SourceValue::None;
        assert_eq!(supporting_text(&bare), "no value — must be provided");
        assert_eq!(supporting_text(&row("region", Some("core"))), "p");
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

    /// A named use's params stand under the use, apart from the flat use of the same
    /// pack, and each is shown by the pack's own name.
    #[test]
    fn a_named_use_s_params_group_under_the_use() {
        let named = |name: &str, use_: &str| {
            let mut r = row(name, Some("vm"));
            r.named = Some(NamedUse {
                name: use_.to_string(),
                path: "packs/vm.satz".to_string(),
            });
            r
        };
        let rows = vec![
            named("web.vm_zone", "web"),
            row("vm_zone", Some("vm")),
            named("db.vm_zone", "db"),
            named("web.vm_name", "web"),
        ];
        let shape: Vec<(String, Vec<String>)> = grouped(&rows)
            .into_iter()
            .map(|(k, v)| (k, v.iter().map(|r| r.own_name().to_string()).collect()))
            .collect();
        assert_eq!(
            shape,
            vec![
                (
                    "web — packs/vm.satz".to_string(),
                    vec!["vm_zone".to_string(), "vm_name".to_string()]
                ),
                ("vm".to_string(), vec!["vm_zone".to_string()]),
                (
                    "db — packs/vm.satz".to_string(),
                    vec!["vm_zone".to_string()]
                ),
            ]
        );
    }
}
