use std::path::Path;

use dioxus::prelude::*;
use satz_studio_core::diag::{DiagSource, Diagnostic, Severity};

use crate::components::{Chip, ChipKind, Icon, List, ListItem};
use crate::state::{AppStore, AppStoreStoreExt, DiagnosticSelection, EstateStoreStoreExt};

/// The bottom drawer: the open estate's diagnostics grouped by severity, each with its
/// `file:line` and source; clicking one sets [`DiagnosticSelection`].
#[component]
pub fn DiagnosticsDrawer() -> Element {
    let app = use_context::<Store<AppStore>>();
    let selection = use_context::<DiagnosticSelection>();
    let open = app.drawer_open().cloned();
    let diagnostics = app.estate().diagnostics().cloned();
    let base = app.open().read().as_ref().map(|o| o.dir.clone());
    let count = |s: Severity| diagnostics.iter().filter(|d| d.severity == s).count();
    let (errors, warnings, notes) = (
        count(Severity::Error),
        count(Severity::Warning),
        count(Severity::Note),
    );
    let selected = (selection.0)();

    rsx! {
        section { class: "drawer", class: if open { "drawer--open" }, "aria-label": "diagnostics",
            header { class: "drawer__header", onclick: move |_| app.drawer_open().toggle(),
                Icon { name: if open { "expand_more" } else { "expand_less" }, size: 20 }
                span { class: "drawer__title", "Diagnostics" }
                Chip { kind: ChipKind::Assist, icon: "error", label: format!("{errors} errors"), error: errors > 0 }
                Chip { kind: ChipKind::Assist, icon: "warning", label: format!("{warnings} warnings") }
                Chip { kind: ChipKind::Assist, icon: "info", label: format!("{notes} notes") }
            }
            if open {
                div { class: "drawer__body",
                    if diagnostics.is_empty() {
                        p { class: "drawer__empty", "No diagnostics." }
                    }
                    for (severity, title, icon) in [(Severity::Error, "Errors", "error"), (Severity::Warning, "Warnings", "warning"), (Severity::Note, "Notes", "info")] {
                        {
                            let group: Vec<Diagnostic> = diagnostics.iter().filter(|d| d.severity == severity).cloned().collect();
                            let base = base.clone();
                            rsx! {
                                if !group.is_empty() {
                                    h3 { class: "drawer__group", "{title}" }
                                    List {
                                        for (i, d) in group.into_iter().enumerate() {
                                            {
                                                let is_selected = selected.as_ref() == Some(&d);
                                                let location = location(&d, base.as_deref());
                                                let headline = d.message.lines().next().unwrap_or_default().to_string();
                                                let item = d.clone();
                                                let mut select = selection.0;
                                                rsx! {
                                                    ListItem {
                                                        key: "{title}-{i}",
                                                        headline,
                                                        supporting: location,
                                                        selected: is_selected,
                                                        leading: rsx! { Icon { name: icon, size: 20, class: "drawer__icon drawer__icon--{icon}" } },
                                                        onclick: move |_| select.set(Some(item.clone())),
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
            }
        }
    }
}

/// `file:line · source`, the file shown relative to the estate directory.
fn location(d: &Diagnostic, base: Option<&Path>) -> String {
    let file = d.file.as_deref().map(|f| {
        base.and_then(|b| f.strip_prefix(b).ok())
            .unwrap_or(f)
            .display()
            .to_string()
    });
    let place = match (file, d.line) {
        (Some(f), Some(l)) => format!("{f}:{l}"),
        (Some(f), None) => f,
        (None, Some(l)) => format!("line {l}"),
        (None, None) => String::new(),
    };
    let source = match &d.source {
        DiagSource::Cst => "document".to_string(),
        DiagSource::Parse => "parse".to_string(),
        DiagSource::Compile => "compile".to_string(),
        DiagSource::Check => "check".to_string(),
        DiagSource::Command(c) => format!("satz {c}"),
        DiagSource::Tool(t) => t.clone(),
    };
    if place.is_empty() {
        source
    } else {
        format!("{place} · {source}")
    }
}
