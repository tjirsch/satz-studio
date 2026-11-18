//! The Estate destination's Interfaces tab: what the estate publishes to the projects
//! that read it, as `satz interfaces` reports it — the core exports every interface
//! carries, then one card per declared interface with what it uses and what it exports,
//! and what projects may request. The rows are satz's report; the app derives no export
//! and no interface of its own (ADR 0023).

use dioxus::prelude::*;
use satz_studio_core::satz::reports::{
    ExportHow, ExportRow, InterfaceRow, InterfacesReport, RequestRow,
};

use dioxus_icons::lucide::{
    Braces, CircleAlert, Globe, Inbox, Key as KeyIcon, Link as LinkIcon, Package, Search,
    Type as TypeIcon, Waypoints,
};

use crate::components::badge::{Badge, BadgeVariant};
use crate::components::card::{Card, CardContent, CardHeader};
use crate::components::progress::Progress;
use crate::state::{AppStore, AppStoreStoreExt, EstateStoreStoreExt};

/// The icon of an export's `how` badge.
fn how_icon(how: ExportHow, size: u32) -> Element {
    match how {
        ExportHow::Static => rsx! { TypeIcon { size } },
        ExportHow::Lookup => rsx! { Search { size } },
        ExportHow::Map => rsx! { Braces { size } },
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
            Card { class: "interfaces__empty",
                CardContent {
                    Progress { value: None::<f64>, "aria-label": "working" }
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
                    "What the estate publishes to the projects that read it, as satz interfaces reports it: the core exports every interface carries, then each interface with what it uses and what it exports. satz transpile writes each one to interfaces/<name>/. Access to a project is delegated to a team by an entry of project_workload_projects in presets/project-workload.satz."
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
            Card { class: "interfaces__empty",
                CardContent {
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
        if !report.requests.is_empty() {
            RequestsCard { requests: report.requests.clone() }
        }
    }
}

/// What a project may ask the estate for: one row per request point — the list, the field
/// that names an entry, the fields an entry may carry with the pattern each value matches,
/// how many entries it holds.
#[component]
fn RequestsCard(requests: Vec<RequestRow>) -> Element {
    rsx! {
        Card { class: "interfaces__card",
            CardHeader {
                div { class: "interfaces__card-head",
                    Inbox { size: 22 }
                    h2 { class: "interfaces__name", "What projects may request" }
                }
            }
            CardContent {
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
                            Badge { variant: BadgeVariant::Outline,
                                KeyIcon { size: 12 }
                                "key {r.key}"
                            }
                            span { class: "interfaces__supporting",
                                {format!("{} entr{}", r.entries, if r.entries == 1 { "y" } else { "ies" })}
                            }
                            code { class: "interfaces__at", "{r.file}:{r.line}" }
                        }
                        code { class: "interfaces__value", {r.fields_shown()} }
                        if let Some(d) = r.description.clone() {
                            p { class: "interfaces__supporting", "{d}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn InterfaceCard(iface: InterfaceRow, rows: Vec<ExportRow>) -> Element {
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
                    {how_icon(row.how, 12)}
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
}
