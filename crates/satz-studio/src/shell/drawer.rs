use std::path::Path;

use dioxus::prelude::*;
use satz_studio_core::diag::{DiagSource, Diagnostic, Severity};

use dioxus_icons::lucide::{ChevronDown, ChevronUp, CircleAlert, Info, TriangleAlert};

use crate::components::badge::{Badge, BadgeVariant};
use crate::components::item::{
    Item, ItemActions, ItemContent, ItemDescription, ItemGroup, ItemMedia, ItemSize, ItemTitle,
};
use crate::state::{AppStore, AppStoreStoreExt, DiagnosticSelection, EstateStoreStoreExt, View};

/// The bottom drawer, Findings: the open estate's findings, and those of the pack the
/// palette's pack review judged, grouped by severity, each with its `file:line`, its source, and —
/// for one of satz's findings — a badge naming the check that raised it; clicking one sets
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
        section { class: "drawer", class: if open { "drawer--open" }, "aria-label": "findings",
            header { class: "drawer__header", onclick: move |_| app.drawer_open().toggle(),
                if open {
                    ChevronDown { size: 16 }
                } else {
                    ChevronUp { size: 16 }
                }
                span { class: "drawer__title", "Findings" }
                Badge { variant: if errors > 0 { BadgeVariant::Destructive } else { BadgeVariant::Outline },
                    CircleAlert { size: 12 }
                    "{errors} errors"
                }
                Badge { variant: BadgeVariant::Outline,
                    TriangleAlert { size: 12 }
                    "{warnings} warnings"
                }
                Badge { variant: BadgeVariant::Outline,
                    Info { size: 12 }
                    "{notes} notes"
                }
            }
            if open {
                div { class: "drawer__body",
                    if diagnostics.is_empty() {
                        p { class: "drawer__empty", "No findings." }
                    }
                    for (severity, title) in [(Severity::Error, "Errors"), (Severity::Warning, "Warnings"), (Severity::Info, "Info")] {
                        {
                            let group: Vec<Diagnostic> = diagnostics.iter().filter(|d| d.severity == severity).cloned().collect();
                            let base = base.clone();
                            rsx! {
                                if !group.is_empty() {
                                    h3 { class: "drawer__group", "{title}" }
                                    ItemGroup {
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
                                                    Item {
                                                        key: "{title}-{i}",
                                                        size: ItemSize::Sm,
                                                        "data-selected": if is_selected { "true" } else { "false" },
                                                        onclick: move |_| {
                                                            select.set(Some(item.clone()));
                                                            match go {
                                                                Some(Go::View(view)) => app.nav().set(view),
                                                                Some(Go::Review) => {
                                                                    app.palette_review().set(true);
                                                                    app.palette_open().set(true);
                                                                }
                                                                None => {}
                                                            }
                                                        },
                                                        ItemMedia {
                                                            match severity {
                                                                Severity::Error => rsx! { CircleAlert { size: 16, class: "drawer__icon--error" } },
                                                                Severity::Warning => rsx! { TriangleAlert { size: 16, class: "drawer__icon--warning" } },
                                                                Severity::Info => rsx! { Info { size: 16, class: "drawer__icon--info" } },
                                                            }
                                                        }
                                                        ItemContent {
                                                            ItemTitle { "{headline}" }
                                                            ItemDescription { "{location}" }
                                                        }
                                                        if let Some(kind) = kind {
                                                            ItemActions {
                                                                Badge { variant: BadgeVariant::Outline, "{kind}" }
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
    }
}

/// Where a click on a finding takes the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Go {
    /// a destination
    View(View),
    /// the commands palette, on the pack review
    Review,
}

/// Where a click on `d` takes the window: a line of the main file is in Estate; a finding
/// about the reviewed pack is in the palette's pack review, which shows the pack's text
/// with the line marked; anything else stays where it is.
pub fn destination(d: &Diagnostic, main: Option<&Path>, reviewed: Option<&Path>) -> Option<Go> {
    let file = d.file.as_deref()?;
    if Some(file) == main && d.line.is_some() {
        Some(Go::View(View::Estate))
    } else if Some(file) == reviewed {
        Some(Go::Review)
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
    fn a_line_of_the_main_file_is_in_estate_and_one_of_the_reviewed_pack_in_the_review() {
        let main = Path::new("/e/yaml/C0example.satz");
        let pack = Path::new("/home/packs/team-access.satz");
        let at = |file: &str, line: Option<u32>| Diagnostic {
            line,
            ..Diagnostic::error("m", DiagSource::Command("review-preset".to_string())).at(file, 1)
        };
        assert_eq!(
            destination(
                &at("/e/yaml/C0example.satz", Some(3)),
                Some(main),
                Some(pack)
            ),
            Some(Go::View(View::Estate))
        );
        assert_eq!(
            destination(
                &at("/home/packs/team-access.satz", Some(17)),
                Some(main),
                Some(pack)
            ),
            Some(Go::Review)
        );
        // a finding about the whole pack still stands in the review
        assert_eq!(
            destination(
                &at("/home/packs/team-access.satz", None),
                Some(main),
                Some(pack)
            ),
            Some(Go::Review)
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
