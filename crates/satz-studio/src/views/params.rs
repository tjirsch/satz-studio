//! The Params pane of the Estate destination: every param the estate binds that is not
//! a pack choice, grouped by the pack whose question asks for it — a named use's params
//! under the use, each by the pack's own name of the param — each as a typed field
//! in value mode or as its Satz source in source mode. A commit is one
//! `Edit::ReplaceParam` through the app's own writer. A group's pack name opens the pack
//! where it is listed — Packs for a workload pack, Presets for a library preset
//! ([`pack_place`]) — and a param a question asks opens that question in Decisions.

use dioxus::prelude::*;
use dioxus_icons::lucide::{
    Boxes, CircleQuestionMark, Code, FileText, MessageCircleQuestionMark, SlidersHorizontal,
    TextAlignStart,
};
use satz_studio_core::cst::TypedValue;
use satz_studio_core::edit::Edit;
use satz_studio_core::model::{EditMode, ParamRow, SourceValue};
use satz_studio_core::satz::reports::{PackRole, PacksReport};

use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::card::{Card, CardContent};
use crate::components::tooltip::{Tooltip, TooltipContent, TooltipTrigger};
use crate::state::{
    AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, Focus, View, go_to, scroll_to,
    take_focus,
};
use crate::views::{line_text, value_source};
use crate::widgets::marks::OneWayBadge;
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

/// The file a group's pack is read from, as its `use` line writes it: a named use's path,
/// else the `from` of the first question that asks one of its params. `None` for the
/// estate's own group, which no pack heads.
pub fn group_file(rows: &[ParamRow]) -> Option<&str> {
    let first = rows.first()?;
    match &first.named {
        Some(u) => Some(u.path.as_str()),
        None => rows
            .iter()
            .find_map(|r| r.question.as_ref())
            .map(|q| q.from.as_str()),
    }
}

/// Where the pack in `file` is listed: its row in Packs for a workload pack of
/// `packs_dir` — `workload` is its id, as `EstateDir::pack_of_use` found it — else its
/// card in Presets for a library preset the presets report lists by its path or by the
/// fork its line names, while the estate has the Presets destination. The map has no
/// card of its own, and a file neither view lists has no place: `None`, and the heading
/// is not a link.
pub fn pack_place(file: &str, workload: Option<String>, presets: &PacksReport) -> Option<Focus> {
    if let Some(id) = workload {
        return Some(Focus::Pack(id));
    }
    if !presets.has_presets() {
        return None;
    }
    presets
        .packs
        .iter()
        .filter(|p| p.role != PackRole::Map)
        .find(|p| p.path == file || p.written.as_deref() == Some(file))
        .map(|p| Focus::Preset(p.path.clone()))
}

#[component]
pub fn ParamsPane() -> Element {
    let app = use_context::<Store<AppStore>>();
    let open = app.open().cloned();
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
    let groups: Vec<(String, Vec<ParamRow>, Option<Focus>)> = grouped(&model.params)
        .into_iter()
        .map(|(group, rows)| {
            let place = group_file(&rows).and_then(|file| {
                let workload = open
                    .as_ref()
                    .and_then(|o| o.session.dir.pack_of_use(&o.main, file));
                pack_place(file, workload, &model.packs)
            });
            (group, rows, place)
        })
        .collect();
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
            for (group, rows, place) in groups {
                section { key: "{group}", class: "params__group",
                    header { class: "params__group-head",
                        if group == ESTATE_GROUP {
                            FileText { size: 20 }
                        } else {
                            Boxes { size: 20 }
                        }
                        h2 { class: "params__group-title",
                            if let Some(place) = place {
                                Button {
                                    variant: ButtonVariant::Link,
                                    size: ButtonSize::Sm,
                                    title: if place.view() == View::Presets { "Open the preset in Presets" } else { "Open the pack in Packs" },
                                    onclick: move |_| go_to(app, place.clone()),
                                    span { class: "params__group-name", "{group}" }
                                }
                            } else {
                                "{group}"
                            }
                        }
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

/// The question card a row's "Asked in Decisions" opens: the question that asks for the
/// param, by its subject; `None` for a param no question asks.
pub fn asked_in(row: &ParamRow) -> Option<Focus> {
    row.question
        .as_ref()
        .map(|q| Focus::Question(q.subject.clone()))
}

#[component]
fn ParamRowView(row: ParamRow, source: String, line: String, loading: bool) -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let here = Focus::Param(row.name.clone());
    let anchor = here.anchor();
    // a link to this row scrolls it into view
    use_effect(move || {
        if take_focus(app, &here) {
            scroll_to(&here.anchor());
        }
    });
    let asked = asked_in(&row);
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
        div { class: "param-row", id: "{anchor}",
            div { class: "param-row__name",
                code { title: "{row.name}", "{row.own_name()}" }
                span { class: "param-row__line", "line {row.line}" }
                if row.one_way_door {
                    OneWayBadge {}
                }
                if let Some(asked) = asked {
                    Button {
                        variant: ButtonVariant::Link,
                        size: ButtonSize::Xs,
                        title: "Open the question that asks for this param",
                        onclick: move |_| go_to(app, asked.clone()),
                        MessageCircleQuestionMark { size: 12 }
                        "Asked in Decisions"
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

    fn presets() -> PacksReport {
        serde_json::from_str(include_str!(
            "../../../satz-studio-core/tests/fixtures/presets-smoke.json"
        ))
        .unwrap()
    }

    /// A workload pack of `packs_dir` opens in Packs; a library preset the presets report
    /// lists opens in Presets; the map, which has no card, and a file neither view lists
    /// have no place, and an estate without the Presets destination places no preset.
    #[test]
    fn a_heading_links_to_packs_or_presets_or_nowhere() {
        let report = presets();
        assert_eq!(
            pack_place("packs/vm.satz", Some("vm".into()), &report),
            Some(Focus::Pack("vm".into()))
        );
        assert_eq!(
            pack_place("presets/estate-core.satz", None, &report),
            Some(Focus::Preset("presets/estate-core.satz".into()))
        );
        assert_eq!(pack_place("presets/estate-map.satz", None, &report), None);
        assert_eq!(pack_place("elsewhere/own.satz", None, &report), None);
        let mut workload = report.clone();
        workload.works_in = satz_studio_core::satz::reports::EstateMode::Workload;
        workload.packs.clear();
        assert_eq!(
            pack_place("presets/estate-core.satz", None, &workload),
            None
        );
    }

    /// A group's file is the path its questions name, a named use's the path of its line;
    /// the estate's own group has none.
    #[test]
    fn a_group_is_read_from_its_question_s_file_or_its_use_s() {
        let mut flat = row("vm_zone", Some("vm"));
        flat.question.as_mut().unwrap().from = "packs/vm.satz".into();
        assert_eq!(group_file(&[row("x", None), flat]), Some("packs/vm.satz"));
        let mut named = row("web.vm_zone", Some("vm"));
        named.named = Some(NamedUse {
            name: "web".to_string(),
            path: "packs/vm.satz".to_string(),
        });
        assert_eq!(group_file(&[named]), Some("packs/vm.satz"));
        assert_eq!(group_file(&[row("b", None)]), None);
    }

    /// "Asked in Decisions" stands on a param a question asks, and opens that question.
    #[test]
    fn only_an_asked_param_leads_to_decisions() {
        assert_eq!(
            asked_in(&row("region", Some("core"))),
            Some(Focus::Question("region".into()))
        );
        assert_eq!(asked_in(&row("region", None)), None);
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
