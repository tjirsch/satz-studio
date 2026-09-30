//! The Estate destination's Interfaces tab: what the estate publishes to the projects
//! that read it, as `satz interfaces` reports it — the core exports every interface
//! carries, then one card per declared interface with what it uses and what it exports,
//! and what projects may request. The rows are satz's report; the app derives no export
//! and no interface of its own (ADR 0023).

use dioxus::prelude::*;
use satz_studio_core::satz::reports::{
    ExportHow, ExportRow, InterfaceRow, InterfacesReport, RequestRow,
};

use crate::components::{Card, CardVariant, Chip, ChipKind, Icon, LinearProgress};
use crate::state::{AppStore, AppStoreStoreExt, EstateStoreStoreExt};

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
            ReportCards { report }
        },
    };

    rsx! {
        div { class: "pane interfaces",
            div { class: "interfaces__head",
                p { class: "view__lead",
                    "What the estate publishes to the projects that read it, as satz interfaces reports it: the core exports every interface carries, then each interface with what it uses and what it exports. satz transpile writes each one to interfaces/<name>/. A project is onboarded by an entry of projects in presets/project-onboarding.satz."
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
/// that names an entry, the fields an entry may carry with the pattern each value matches,
/// how many entries it holds.
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
                    code { class: "interfaces__value", {r.fields_shown()} }
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
