//! The Estate destination's Interfaces tab: what the estate publishes to the projects
//! that read it, as `satz interfaces` reports it — the core exports every interface
//! carries, then one card per declared interface with what it uses and what it exports,
//! and the list params it declares. The rows are satz's report; the app derives no export
//! and no interface of its own (ADR 0023).

use dioxus::prelude::*;
use std::collections::BTreeMap;

use satz_studio_core::satz::reports::{
    AddEntryArgs, EntryField, ExportHow, ExportRow, InterfaceRow, InterfacesReport, RequestRow,
};

use dioxus_icons::lucide::{
    Braces, Check, CircleAlert, FolderOpen, Globe, Inbox, Key as KeyIcon, Link as LinkIcon,
    Package, Plus, Search, Tag, Type as TypeIcon, Waypoints,
};

use crate::components::badge::{Badge, BadgeVariant};
use crate::components::button::{Button, ButtonVariant};
use crate::components::card::{Card, CardContent, CardHeader};
use crate::components::dialog::{Dialog, DialogTitle};
use crate::components::progress::Progress;
use crate::state::{
    AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, ToastKind, toast,
};
use crate::widgets::{Field, SwitchRow};

/// The icon of an export's `how` badge.
fn how_icon(how: ExportHow) -> Element {
    match how {
        ExportHow::Static => rsx! { TypeIcon { size: 12 } },
        ExportHow::Lookup => rsx! { Search { size: 12 } },
        ExportHow::Map => rsx! { Braces { size: 12 } },
    }
}

/// The label of an export's `how` badge: the word satz writes, and the type of an `all` map.
fn how_label(row: &ExportRow) -> String {
    match &row.all {
        Some(t) => format!("{} · {t}", row.how.as_str()),
        None => row.how.as_str().to_string(),
    }
}

#[component]
pub fn InterfacesPane() -> Element {
    let app = use_context::<Store<AppStore>>();
    let interfaces = app.estate().interfaces().cloned();

    let body = match interfaces {
        None => rsx! {
            Card {
                CardContent { class: "interfaces__empty",
                    Progress { value: None }
                    p { "Reading what the estate publishes…" }
                }
            }
        },
        Some(Err(e)) => rsx! {
            div { class: "interfaces__error",
                CircleAlert { size: 20 }
                div {
                    p { "satz interfaces could not say what the estate publishes:" }
                    pre { "{e}" }
                }
            }
        },
        Some(Ok(report)) => rsx! {
            ReportCards { report }
        },
    };

    rsx! {
        div { class: "pane interfaces",
            div { class: "interfaces__head",
                p { class: "view__lead",
                    "What the estate publishes for other estates to read, as satz interfaces reports it: the core exports every interface carries, then each interface with what it uses and what it exports. satz transpile writes each one to an interface folder under interfaces/outgoing/. Delegated access is given by an entry of the list param delegations in presets/delegation.satz."
                }
            }
            {body}
        }
    }
}

/// The core card and one card per interface.
#[component]
fn ReportCards(report: InterfacesReport) -> Element {
    if report.exports.is_empty() && report.interfaces.is_empty() {
        return rsx! {
            Card {
                CardContent { class: "interfaces__empty",
                    Waypoints { size: 48, class: "placeholder__icon" }
                    p { "The estate publishes nothing to a project: it declares no export and no interface." }
                }
            }
        };
    }
    let core: Vec<ExportRow> = report.core().cloned().collect();
    rsx! {
        Card { class: "interfaces__card",
            CardHeader {
                div { class: "interfaces__card-head",
                    Package { size: 22 }
                    h2 { class: "interfaces__name", "core" }
                    span { class: "interfaces__supporting", "every interface carries these" }
                }
            }
            CardContent {
                if core.is_empty() {
                    p { class: "interfaces__supporting", "No core export." }
                }
                for row in core {
                    ExportLine { key: "{row.name}", row: row.clone() }
                }
            }
        }
        for iface in report.interfaces.clone() {
            InterfaceCard {
                key: "{iface.name}",
                rows: report.of(&iface.name).cloned().collect::<Vec<_>>(),
                iface: iface.clone(),
            }
        }
        if !report.list_params.is_empty() {
            RequestsCard { requests: report.list_params.clone() }
        }
    }
}

/// The fields of a new entry as `satz_add_entry` takes them: what was typed, a switch as
/// `true` or `false`, and nothing for a field left as its default.
pub fn entry_fields(
    request: &RequestRow,
    typed: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    request
        .fields
        .iter()
        .filter_map(|f| {
            let value = typed.get(f)?.trim().to_string();
            if value.is_empty() || request.default_text(f).as_deref() == Some(value.as_str()) {
                return None;
            }
            Some((f.clone(), value))
        })
        .collect()
}

/// The list params the estate's packs declare, each with the entries it holds and a form that
/// adds one: a delegation, a delegated resource, a subnet. The form is built from
/// the declaration — one field per declared field, its pattern under it, its default as
/// the value an untouched field keeps — and the entry is written by satz's own writer
/// (`satz_add_entry`), which checks it against the same declaration.
#[component]
fn RequestsCard(requests: Vec<RequestRow>) -> Element {
    let handle = use_coroutine_handle::<EstateAction>();
    let app = use_context::<Store<AppStore>>();
    let loading = app.estate().loading().cloned();
    // the list an entry is being added to, and what was typed for it
    let mut adding = use_signal(|| None::<RequestRow>);
    let mut typed = use_signal(BTreeMap::<String, String>::new);
    let current = adding();
    rsx! {
        Card { class: "interfaces__card",
            CardHeader {
                div { class: "interfaces__card-head",
                    Inbox { size: 22 }
                    h2 { class: "interfaces__name", "Lists that take entries" }
                }
            }
            CardContent {
                p { class: "interfaces__supporting",
                    "A pack declares what an entry of its list is: delegated access, a subnet, a budget. \"Add\" writes one into this estate ("
                    code { "satz add-entry" }
                    "); a workload's team asks for one in a request file, "
                    code { "contributes_<list param>" }
                    ", checked with "
                    code { "satz check-request" }
                    "."
                }
                for r in requests {
                    div { key: "{r.param}", class: "interfaces__export",
                        div { class: "interfaces__export-head",
                            code { class: "interfaces__export-name", "{r.param}" }
                            Badge { variant: BadgeVariant::Outline,
                                KeyIcon { size: 12 }
                                "key {r.key}"
                            }
                            span { class: "interfaces__supporting",
                                {format!("{} entr{}", r.entries, if r.entries == 1 { "y" } else { "ies" })}
                            }
                            span { class: "grow" }
                            code { class: "interfaces__at", "{r.file}:{r.line}" }
                            Button {
                                variant: ButtonVariant::Secondary,
                                disabled: loading,
                                onclick: {
                                    let r = r.clone();
                                    move |_| {
                                        typed.set(BTreeMap::new());
                                        adding.set(Some(r.clone()));
                                    }
                                },
                                Plus { size: 16 }
                                "Add"
                            }
                        }
                        code { class: "interfaces__value", {r.fields_shown()} }
                        if let Some(d) = r.description.clone() {
                            p { class: "interfaces__supporting", "{d}" }
                        }
                        if !r.keys().is_empty() {
                            div { class: "interfaces__entries",
                                for k in r.keys() {
                                    Badge { key: "{k}", variant: BadgeVariant::Outline,
                                        Tag { size: 12 }
                                        "{k}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Dialog {
            open: current.is_some(),
            on_open_change: move |now: bool| {
                if !now {
                    adding.set(None);
                }
            },
            class: "interfaces__add",
            DialogTitle {
                {current.as_ref().map(|r| format!("Add an entry to {}", r.param)).unwrap_or_default()}
            }
            div { class: "dialog-body",
                if let Some(r) = current.clone() {
                    if let Some(d) = r.description.clone() {
                        p { class: "interfaces__supporting", "{d}" }
                    }
                    for f in r.fields.clone() {
                        {
                            let key = f.clone();
                            let value = typed().get(&f).cloned().unwrap_or_else(|| r.default_text(&f).unwrap_or_default());
                            match r.field_kind(&f) {
                                EntryField::Bool => rsx! {
                                    SwitchRow {
                                        key: "{f}",
                                        label: f.clone(),
                                        checked: value == "true",
                                        onchange: move |v: bool| { typed.write().insert(key.clone(), v.to_string()); },
                                    }
                                },
                                kind => rsx! {
                                    Field {
                                        key: "{f}",
                                        label: if f == r.key { format!("{f} (names the entry)") } else { f.clone() },
                                        value,
                                        monospace: true,
                                        supporting: match (r.patterns.get(&f), kind) {
                                            (Some(p), _) => format!("matches {p}"),
                                            (None, EntryField::List) => "several values, separated by commas".to_string(),
                                            _ => String::new(),
                                        },
                                        oninput: move |v: String| { typed.write().insert(key.clone(), v); },
                                    }
                                },
                            }
                        }
                    }
                }
            }
            div { class: "dialog-actions",
                Button { variant: ButtonVariant::Ghost, onclick: move |_| adding.set(None), "Cancel" }
                Button {
                    onclick: move |_| {
                        if let Some(r) = adding() {
                            let fields = entry_fields(&r, &typed());
                            if !fields.contains_key(&r.key) {
                                toast(app, ToastKind::Error, format!("an entry is named by its {}", r.key));
                                return;
                            }
                            handle.send(EstateAction::AddEntry(AddEntryArgs { list: r.param.clone(), fields }));
                            adding.set(None);
                        }
                    },
                    Check { size: 16 }
                    "Add"
                }
            }
        }
    }
}

#[component]
fn InterfaceCard(iface: InterfaceRow, rows: Vec<ExportRow>) -> Element {
    let app = use_context::<Store<AppStore>>();
    // the interface folder the last transpile wrote: what a workload's team copies
    let folder = app
        .open()
        .cloned()
        .map(|o| o.session.dir.outgoing_dir().join(&iface.name));
    rsx! {
        Card { class: "interfaces__card",
            CardHeader {
                div { class: "interfaces__card-head",
                    Waypoints { size: 22 }
                    h2 { class: "interfaces__name", "{iface.name}" }
                    if iface.common {
                        Badge { variant: BadgeVariant::Outline, class: "interfaces__common",
                            Globe { size: 12 }
                            "common"
                        }
                    }
                    span { class: "grow" }
                    code { class: "interfaces__at", "{iface.file}:{iface.line}" }
                    if let Some(folder) = folder.filter(|_| !iface.common) {
                        Button {
                            variant: ButtonVariant::Ghost,
                            onclick: move |_| {
                                if !folder.is_dir() {
                                    toast(app, ToastKind::Info, format!("{} is not written yet: Apply → transpile writes it", folder.display()));
                                } else if let Err(e) = open::that(&folder) {
                                    toast(app, ToastKind::Error, format!("{}: {e}", folder.display()));
                                }
                            },
                            FolderOpen { size: 16 }
                            "Show folder"
                        }
                    }
                }
            }
            CardContent {
                if iface.common {
                    p { class: "interfaces__supporting",
                        "In the library every project's folder carries."
                    }
                }
                if !iface.uses.is_empty() {
                    div { class: "interfaces__uses",
                        span { class: "interfaces__supporting", "uses" }
                        for u in iface.uses.clone() {
                            Badge { key: "{u}", variant: BadgeVariant::Outline,
                                Waypoints { size: 12 }
                                "{u}"
                            }
                        }
                    }
                }
                if rows.is_empty() {
                    p { class: "interfaces__supporting",
                        "Declares no export of its own: it carries the core exports and those of the interfaces it uses."
                    }
                }
                for row in rows {
                    ExportLine { key: "{row.name}", row: row.clone() }
                }
            }
        }
    }
}

/// One export: its name, how a project holds it, the value, what may be attached to it,
/// its description and where it is declared.
#[component]
fn ExportLine(row: ExportRow) -> Element {
    rsx! {
        div { class: "interfaces__export",
            div { class: "interfaces__export-head",
                code { class: "interfaces__export-name", "{row.name}" }
                Badge { variant: BadgeVariant::Outline,
                    {how_icon(row.how)}
                    {how_label(&row)}
                }
                for a in row.attach.clone() {
                    Badge { key: "{a}", variant: BadgeVariant::Outline,
                        LinkIcon { size: 12 }
                        "attach {a}"
                    }
                }
                span { class: "grow" }
                code { class: "interfaces__at", "{row.file}:{row.line}" }
            }
            code { class: "interfaces__value", "{row.value}" }
            if let Some(d) = &row.description {
                p { class: "interfaces__supporting", "{d}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOWCASE: &str =
        include_str!("../../../satz-studio-core/tests/fixtures/interfaces-showcase.json");

    fn report() -> InterfacesReport {
        serde_json::from_str(SHOWCASE).unwrap()
    }

    #[test]
    fn a_map_names_its_type() {
        let r = report();
        let map = r
            .exports
            .iter()
            .find(|e| e.how == ExportHow::Map)
            .expect("the showcase exports a map");
        assert!(how_label(map).starts_with("map · "), "{}", how_label(map));
    }
    fn projects() -> RequestRow {
        serde_json::from_value(serde_json::json!({
            "param": "delegated_projects", "key": "name",
            "fields": ["name", "delegation", "apis", "budgets"],
            "patterns": {"name": "[a-z][a-z0-9-]*"},
            "description": null, "entries": 1,
            "rows": [{"name": "payments", "delegation": "payments", "apis": ["iam.googleapis.com"], "budgets": false}],
            "defaults": {"apis": ["iam.googleapis.com", "storage.googleapis.com"], "budgets": false},
            "file": "presets/delegated-projects.satz", "line": 60
        }))
        .unwrap()
    }

    /// The form is the declaration: a field is typed as its default says, shows its
    /// default as satz takes it, and the list's entries are named by their key.
    #[test]
    fn a_declared_list_says_how_each_field_is_typed_and_what_it_holds() {
        let r = projects();
        assert_eq!(r.field_kind("budgets"), EntryField::Bool);
        assert_eq!(r.field_kind("apis"), EntryField::List);
        assert_eq!(r.field_kind("name"), EntryField::Text);
        assert_eq!(
            r.default_text("apis").as_deref(),
            Some("iam.googleapis.com,storage.googleapis.com")
        );
        assert_eq!(r.default_text("budgets").as_deref(), Some("false"));
        assert_eq!(r.default_text("name"), None);
        assert_eq!(r.keys(), ["payments"]);
    }

    /// Only what was typed and differs from the default is sent: satz leaves a default out
    /// of the entry it writes, and so does the form.
    #[test]
    fn an_entry_carries_what_was_typed_and_no_untouched_default() {
        let r = projects();
        let typed = BTreeMap::from([
            ("name".to_string(), " rag ".to_string()),
            ("delegation".to_string(), "rag".to_string()),
            ("budgets".to_string(), "true".to_string()),
            (
                "apis".to_string(),
                "iam.googleapis.com,storage.googleapis.com".to_string(),
            ),
        ]);
        assert_eq!(
            entry_fields(&r, &typed),
            BTreeMap::from([
                ("name".to_string(), "rag".to_string()),
                ("delegation".to_string(), "rag".to_string()),
                ("budgets".to_string(), "true".to_string()),
            ])
        );
        let untouched = BTreeMap::from([
            ("budgets".to_string(), "false".to_string()),
            ("delegation".to_string(), String::new()),
        ]);
        assert!(entry_fields(&r, &untouched).is_empty());
    }
}
