use std::path::Path;

use dioxus::prelude::*;
use satz_studio_core::diag::{DiagSource, Diagnostic, Severity};

use crate::components::{Chip, ChipKind, Icon, List, ListItem};
use crate::state::{AppStore, AppStoreStoreExt, DiagnosticSelection, EstateStoreStoreExt, View};

/// The bottom drawer: the open estate's diagnostics, and the findings of the pack the
/// Packs view reviewed, grouped by severity, each with its `file:line`, its source, and —
/// for one of satz's findings — a chip naming the check that raised it; clicking one sets
/// [`DiagnosticSelection`] and opens the destination where its line is ([`destination`]).
#[component]
pub fn DiagnosticsDrawer() -> Element {
    let app = use_context::<Store<AppStore>>();
    let selection = use_context::<DiagnosticSelection>();
    let open = app.drawer_open().cloned();
    let review = app.estate().review().cloned();
    let reviewed = review.as_ref().map(|r| r.pack().to_path_buf());
    let mut diagnostics = app.estate().diagnostics().cloned();
    diagnostics.extend(review.map(|r| r.diagnostics()).unwrap_or_default());
    let base = app.open().read().as_ref().map(|o| o.dir.clone());
    let main = app.open().read().as_ref().map(|o| o.main.clone());
    let count = |s: Severity| diagnostics.iter().filter(|d| d.severity == s).count();
    let (errors, warnings, notes) = (
        count(Severity::Error),
        count(Severity::Warning),
        count(Severity::Info),
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
                    for (severity, title, icon) in [(Severity::Error, "Errors", "error"), (Severity::Warning, "Warnings", "warning"), (Severity::Info, "Info", "info")] {
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
                                                let kind = d.kind.clone();
                                                let item = d.clone();
                                                let go = destination(&d, main.as_deref(), reviewed.as_deref());
                                                let mut select = selection.0;
                                                rsx! {
                                                    ListItem {
                                                        key: "{title}-{i}",
                                                        headline,
                                                        supporting: location,
                                                        selected: is_selected,
                                                        leading: rsx! { Icon { name: icon, size: 20, class: "drawer__icon drawer__icon--{icon}" } },
                                                        trailing: kind.map(|kind| rsx! { Chip { kind: ChipKind::Assist, label: kind } }),
                                                        onclick: move |_| {
                                                            select.set(Some(item.clone()));
                                                            if let Some(view) = go {
                                                                app.nav().set(view);
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
                    }
                }
            }
        }
    }
}

/// Where a click on `d` takes the window: a line of the main file is in Estate; a finding
/// about the reviewed pack is in Packs, where the review shows the pack's text with the
/// line marked; anything else stays where it is.
pub fn destination(d: &Diagnostic, main: Option<&Path>, reviewed: Option<&Path>) -> Option<View> {
    let file = d.file.as_deref()?;
    if Some(file) == main && d.line.is_some() {
        Some(View::Estate)
    } else if Some(file) == reviewed {
        Some(View::Packs)
    } else {
        None
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_of_the_main_file_is_in_estate_and_one_of_the_reviewed_pack_in_packs() {
        let main = Path::new("/e/yaml/C0example.satz");
        let pack = Path::new("/home/packs/team-access.satz");
        let at = |file: &str, line: Option<u32>| Diagnostic {
            line,
            ..Diagnostic::error("m", DiagSource::Command("review-pack".to_string())).at(file, 1)
        };
        assert_eq!(
            destination(
                &at("/e/yaml/C0example.satz", Some(3)),
                Some(main),
                Some(pack)
            ),
            Some(View::Estate)
        );
        assert_eq!(
            destination(
                &at("/home/packs/team-access.satz", Some(17)),
                Some(main),
                Some(pack)
            ),
            Some(View::Packs)
        );
        // a finding about the whole pack still stands in the review
        assert_eq!(
            destination(
                &at("/home/packs/team-access.satz", None),
                Some(main),
                Some(pack)
            ),
            Some(View::Packs)
        );
        assert_eq!(
            destination(&at("/e/yaml/C0example.satz", None), Some(main), Some(pack)),
            None
        );
        assert_eq!(
            destination(
                &at("/e/presets/other.satz", Some(2)),
                Some(main),
                Some(pack)
            ),
            None
        );
    }
}
