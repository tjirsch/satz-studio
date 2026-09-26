//! The Estate destination's Interfaces tab: what the estate publishes to the projects
//! that read it, as `satz interfaces` reports it — the core exports every interface
//! carries, then one card per declared interface with what it uses and what it exports —
//! and the "New interface" wizard, which runs `satz add-project` through the estate
//! coroutine (`EstateAction::AddProject`). The rows are satz's report; the app derives no
//! export and no interface of its own (ADR 0023).

use dioxus::prelude::*;
use satz_studio_core::satz::project::AddProjectArgs;
use satz_studio_core::satz::reports::{
    ExportHow, ExportRow, InterfaceRow, InterfacesReport, RequestRow,
};

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Checkbox, Chip, ChipKind, Dialog, Icon,
    LinearProgress, Segment, SegmentedButton, TextField,
};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, command_line};

/// The two things the wizard can add, as its segmented button offers them.
const PROJECT: &str = "project";
const INTERFACE_ONLY: &str = "interface";

/// The wizard's form as typed: the owner group is kept while "Interface only" is chosen,
/// so switching back does not lose it, and dropped from the arguments.
#[derive(Debug, Clone, PartialEq, Default)]
struct Draft {
    name: String,
    owner_group: String,
    interface_only: bool,
    uses: Vec<String>,
    exports: Vec<String>,
}

impl Draft {
    /// The arguments the Create button sends: the owner group only for a project — one
    /// left blank is none, so the form says a project needs one rather than asking satz
    /// for `--owner-group ''` — and only the picks the chosen name still allows: an
    /// interface cannot use itself.
    fn args(&self) -> AddProjectArgs {
        let name = self.name.trim().to_string();
        let group = self.owner_group.trim();
        AddProjectArgs {
            owner_group: (!self.interface_only && !group.is_empty()).then(|| group.to_string()),
            interface_only: self.interface_only,
            uses: self.uses.iter().filter(|u| **u != name).cloned().collect(),
            exports: self
                .exports
                .iter()
                .filter(|x| !x.starts_with(&format!("{name}.")))
                .cloned()
                .collect(),
            name,
        }
    }
}

/// The interfaces the new one may use: every declared interface but itself.
fn usable<'a>(report: &'a InterfacesReport, name: &'a str) -> Vec<&'a InterfaceRow> {
    report
        .interfaces
        .iter()
        .filter(|i| i.name != name.trim())
        .collect()
}

/// The exports the new interface may carry again, grouped by the interface that declares
/// them in the report's order: the non-core ones, since every interface carries the core
/// exports already and satz refuses to copy one.
fn carriable<'a>(
    report: &'a InterfacesReport,
    name: &'a str,
) -> Vec<(&'a InterfaceRow, Vec<&'a ExportRow>)> {
    usable(report, name)
        .into_iter()
        .map(|i| (i, report.of(&i.name).collect::<Vec<_>>()))
        .filter(|(_, rows)| !rows.is_empty())
        .collect()
}

/// The icon of an export's `how` chip.
fn how_icon(how: ExportHow) -> &'static str {
    match how {
        ExportHow::Static => "text_fields",
        ExportHow::Lookup => "search",
        ExportHow::Map => "data_object",
    }
}

/// The label of an export's `how` chip: the word satz writes, and the type of an `all` map.
fn how_label(row: &ExportRow) -> String {
    match &row.all {
        Some(t) => format!("{} · {t}", row.how.as_str()),
        None => row.how.as_str().to_string(),
    }
}

#[component]
pub fn InterfacesPane() -> Element {
    let app = use_context::<Store<AppStore>>();
    let mut wizard = use_signal(|| false);
    let interfaces = app.estate().interfaces().cloned();

    let body = match interfaces {
        None => rsx! {
            Card { variant: CardVariant::Filled, class: "interfaces__empty",
                LinearProgress {}
                p { "Reading what the estate publishes…" }
            }
        },
        Some(Err(e)) => rsx! {
            div { class: "interfaces__error",
                Icon { name: "error", size: 20 }
                div {
                    p { "satz interfaces could not say what the estate publishes:" }
                    pre { "{e}" }
                }
            }
        },
        Some(Ok(report)) => rsx! {
            ReportCards { report: report.clone() }
            ProjectWizard { open: wizard(), report, onclose: move |_| wizard.set(false) }
        },
    };

    rsx! {
        div { class: "pane interfaces",
            div { class: "interfaces__head",
                p { class: "view__lead",
                    "What the estate publishes to the projects that read it, as satz interfaces reports it: the core exports every interface carries, then each interface with what it uses and what it exports. satz transpile writes each one to interfaces/<name>/."
                }
                span { class: "grow" }
                Button {
                    variant: ButtonVariant::Filled,
                    icon: "add",
                    disabled: !matches!(app.estate().interfaces().cloned(), Some(Ok(_))),
                    onclick: move |_| wizard.set(true),
                    "New interface"
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
            Card { variant: CardVariant::Filled, class: "interfaces__empty",
                Icon { name: "hub", size: 48, class: "placeholder__icon" }
                p { "The estate publishes nothing to a project: it declares no export and no interface." }
            }
        };
    }
    let core: Vec<ExportRow> = report.core().cloned().collect();
    rsx! {
        Card { variant: CardVariant::Outlined, class: "interfaces__card",
            div { class: "interfaces__card-head",
                Icon { name: "deployed_code", size: 22 }
                h2 { class: "interfaces__name", "core" }
                span { class: "interfaces__supporting", "every interface carries these" }
            }
            if core.is_empty() {
                p { class: "interfaces__supporting", "No core export." }
            }
            for row in core {
                ExportLine { key: "{row.name}", row: row.clone() }
            }
        }
        for iface in report.interfaces.clone() {
            InterfaceCard {
                key: "{iface.name}",
                rows: report.of(&iface.name).cloned().collect::<Vec<_>>(),
                iface: iface.clone(),
            }
        }
        if !report.requests.is_empty() {
            RequestsCard { requests: report.requests.clone() }
        }
    }
}

/// What a project may ask the estate for: one row per request point — the list, the field
/// that names an entry, the fields an entry may carry, how many entries it holds.
#[component]
fn RequestsCard(requests: Vec<RequestRow>) -> Element {
    rsx! {
        Card { variant: CardVariant::Outlined, class: "interfaces__card",
            div { class: "interfaces__card-head",
                Icon { name: "move_to_inbox", size: 22 }
                h2 { class: "interfaces__name", "What projects may request" }
            }
            p { class: "interfaces__supporting",
                "A project adds entries to one of these lists in a pack of its own, "
                code { "contributes_<list>" }
                ", checked with "
                code { "satz check-request" }
                " and handed to the estate by pull request."
            }
            for r in requests {
                div { key: "{r.param}", class: "interfaces__export",
                    div { class: "interfaces__export-head",
                        code { class: "interfaces__export-name", "{r.param}" }
                        Chip { kind: ChipKind::Assist, icon: "key", label: format!("key {}", r.key) }
                        span { class: "interfaces__supporting",
                            {format!("{} entr{}", r.entries, if r.entries == 1 { "y" } else { "ies" })}
                        }
                        code { class: "interfaces__at", "{r.file}:{r.line}" }
                    }
                    code { class: "interfaces__value", {r.fields.join(", ")} }
                    if let Some(d) = r.description.clone() {
                        p { class: "interfaces__supporting", "{d}" }
                    }
                }
            }
        }
    }
}

#[component]
fn InterfaceCard(iface: InterfaceRow, rows: Vec<ExportRow>) -> Element {
    rsx! {
        Card { variant: CardVariant::Outlined, class: "interfaces__card",
            div { class: "interfaces__card-head",
                Icon { name: "hub", size: 22 }
                h2 { class: "interfaces__name", "{iface.name}" }
                if iface.common {
                    Chip { kind: ChipKind::Assist, icon: "public", label: "common", class: "interfaces__common" }
                }
                span { class: "grow" }
                code { class: "interfaces__at", "{iface.file}:{iface.line}" }
            }
            if iface.common {
                p { class: "interfaces__supporting",
                    "In the library every project's folder carries."
                }
            }
            if !iface.uses.is_empty() {
                div { class: "interfaces__uses",
                    span { class: "interfaces__supporting", "uses" }
                    for u in iface.uses.clone() {
                        Chip { key: "{u}", kind: ChipKind::Assist, icon: "hub", label: u.clone() }
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

/// One export: its name, how a project holds it, the value, what may be attached to it,
/// its description and where it is declared.
#[component]
fn ExportLine(row: ExportRow) -> Element {
    rsx! {
        div { class: "interfaces__export",
            div { class: "interfaces__export-head",
                code { class: "interfaces__export-name", "{row.name}" }
                Chip { kind: ChipKind::Assist, icon: how_icon(row.how), label: how_label(&row) }
                for a in row.attach.clone() {
                    Chip { key: "{a}", kind: ChipKind::Assist, icon: "link", label: format!("attach {a}") }
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

/// The "New interface" wizard: `satz add-project` as a form, its command line as it will
/// run, and Create. It closes when the write lands; satz's refusal stands in it.
#[component]
fn ProjectWizard(open: bool, report: InterfacesReport, onclose: EventHandler<()>) -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let mut draft = use_signal(Draft::default);
    // the refusal shown is the one of a Create pressed in this wizard, not an older one
    let mut sent = use_signal(|| false);
    let adding = app.estate().adding_project().cloned();
    let added = app.estate().added_project().cloned();

    // A write that landed closes the wizard and empties it. The wizard's own signals are
    // written, never read, so opening it does not run this again.
    use_effect(move || {
        if let Some(Ok(_)) = app.estate().added_project().cloned() {
            draft.set(Draft::default());
            sent.set(false);
            onclose.call(());
        }
    });

    let Some(estate) = app.open().cloned() else {
        return rsx! {};
    };
    let d = draft();
    let args = d.args();
    let problem = args.problem();
    let name_problem = if d.name.trim().is_empty() {
        None
    } else {
        satz_studio_core::satz::project::valid_name(d.name.trim()).err()
    };
    let preview = command_line(&estate.dir, &args.argv(&estate.main));
    let refusal = match (&added, sent(), adding) {
        (Some(Err(e)), true, false) => Some(e.clone()),
        _ => None,
    };
    let usable: Vec<String> = usable(&report, &d.name)
        .into_iter()
        .map(|i| i.name.clone())
        .collect();
    let carriable: Vec<(String, Vec<(String, String)>)> = carriable(&report, &d.name)
        .into_iter()
        .map(|(i, rows)| {
            (
                i.name.clone(),
                rows.into_iter()
                    .filter_map(|r| r.qualified().map(|q| (q, r.name.clone())))
                    .collect(),
            )
        })
        .collect();

    rsx! {
        Dialog {
            open,
            title: "New interface",
            icon: "hub",
            class: "project-dialog",
            ondismiss: move |_| {
                if !adding {
                    sent.set(false);
                    onclose.call(());
                }
            },
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Text,
                    disabled: adding,
                    onclick: move |_| {
                        sent.set(false);
                        onclose.call(());
                    },
                    "Cancel"
                }
                Button {
                    variant: ButtonVariant::Filled,
                    icon: "add",
                    disabled: adding || problem.is_some(),
                    onclick: move |_| {
                        sent.set(true);
                        handle.send(EstateAction::AddProject(draft().args()));
                    },
                    "Create"
                }
            },
            p {
                "satz add-project appends the section to the end of the estate file, and the next transpile writes interfaces/<name>/."
            }
            TextField {
                label: "Name",
                value: d.name.clone(),
                monospace: true,
                placeholder: "payments",
                disabled: adding,
                error: name_problem.is_some(),
                supporting: name_problem.clone().unwrap_or_else(|| "The interface's name and its folder: lowercase letters, digits and -, starting with a letter".to_string()),
                oninput: move |v: String| draft.write().name = v,
            }
            SegmentedButton {
                options: vec![
                    Segment::new(PROJECT, "Onboard a project").with_icon("add_business"),
                    Segment::new(INTERFACE_ONLY, "Interface only").with_icon("hub"),
                ],
                selected: if d.interface_only { INTERFACE_ONLY } else { PROJECT },
                onselect: move |v: String| draft.write().interface_only = v == INTERFACE_ONLY,
            }
            if d.interface_only {
                p { class: "project-dialog__note",
                    "The interface block alone, for a workload that brings its own Google project. It carries the core exports and what is picked below, and needs at least one pick."
                }
            } else {
                p { class: "project-dialog__note",
                    "A Google project under the workload folder, its IaC service account, its state bucket and their grants, and the interface that publishes project_id, project_number, iac_account and state_bucket."
                }
                TextField {
                    label: "Owner group",
                    value: d.owner_group.clone(),
                    monospace: true,
                    placeholder: "payments-owners@example.com",
                    disabled: adding,
                    supporting: "The group that reads the project and may become its IaC service account: its address, <name>@<domain>",
                    oninput: move |v: String| draft.write().owner_group = v,
                }
            }
            if !usable.is_empty() {
                p { class: "project-dialog__label", "Interfaces to use" }
                div { class: "project-dialog__chips",
                    for u in usable {
                        {
                            let picked = d.uses.contains(&u);
                            let name = u.clone();
                            rsx! {
                                Chip {
                                    key: "{u}",
                                    kind: ChipKind::Filter,
                                    label: u.clone(),
                                    selected: picked,
                                    onclick: move |_| {
                                        let mut w = draft.write();
                                        if picked {
                                            w.uses.retain(|x| *x != name);
                                        } else {
                                            w.uses.push(name.clone());
                                        }
                                    },
                                }
                            }
                        }
                    }
                }
            }
            if !carriable.is_empty() {
                p { class: "project-dialog__label", "Exports to carry again" }
                div { class: "project-dialog__exports",
                    for (iface, rows) in carriable {
                        div { key: "{iface}", class: "project-dialog__group",
                            span { class: "project-dialog__group-name", "{iface}" }
                            for (qualified, name) in rows {
                                {
                                    let picked = d.exports.contains(&qualified);
                                    let q = qualified.clone();
                                    rsx! {
                                        Checkbox {
                                            key: "{qualified}",
                                            label: name,
                                            checked: picked,
                                            disabled: adding,
                                            onchange: move |on: bool| {
                                                let mut w = draft.write();
                                                if on {
                                                    w.exports.push(q.clone());
                                                } else {
                                                    w.exports.retain(|x| *x != q);
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            code { class: "project-dialog__preview", "{preview}" }
            if let Some(p) = &problem {
                p { class: "project-dialog__problem", "{p}" }
            }
            if adding {
                LinearProgress {}
            }
            if let Some(r) = refusal {
                div { class: "project-dialog__refusal",
                    p { "satz refused, and the file is as it was:" }
                    pre { "{r}" }
                }
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

    /// The new interface is offered every declared interface but itself, and the exports
    /// of those alone, never a core one.
    #[test]
    fn the_wizard_offers_every_other_interface_and_no_core_export() {
        let r = report();
        let all: Vec<&str> = usable(&r, "").iter().map(|i| i.name.as_str()).collect();
        assert!(
            all.contains(&"audit") && all.contains(&"archive"),
            "{all:?}"
        );
        let without: Vec<&str> = usable(&r, "archive")
            .iter()
            .map(|i| i.name.as_str())
            .collect();
        assert!(!without.contains(&"archive"), "{without:?}");
        for (i, rows) in carriable(&r, "") {
            assert!(
                rows.iter()
                    .all(|x| x.interface.as_deref() == Some(i.name.as_str())),
                "{}",
                i.name
            );
        }
        assert!(
            carriable(&r, "")
                .iter()
                .flat_map(|(_, rows)| rows)
                .any(|x| x.qualified().as_deref() == Some("archive.archive_project_number"))
        );
    }

    /// A pick the name no longer allows is dropped, a blank owner group is none, and the
    /// owner group is not sent with "Interface only".
    #[test]
    fn the_draft_sends_what_the_chosen_shape_takes() {
        let d = Draft {
            name: "archive".to_string(),
            owner_group: "  ".to_string(),
            uses: vec!["audit".to_string(), "archive".to_string()],
            exports: vec![
                "archive.archive_project_id".to_string(),
                "audit.x".to_string(),
            ],
            ..Default::default()
        };
        let args = d.args();
        assert_eq!(args.uses, ["audit"]);
        assert_eq!(args.exports, ["audit.x"]);
        assert_eq!(args.owner_group, None);
        assert!(args.problem().unwrap().contains("owner group"));
        let only = Draft {
            interface_only: true,
            owner_group: "g@example.com".to_string(),
            ..d
        };
        assert_eq!(only.args().owner_group, None);
        assert_eq!(only.args().problem(), None);
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
}
