use dioxus::prelude::*;

use crate::components::{NavRail, NavRailItem};
use crate::state::{AppStore, AppStoreStoreExt, EstateStoreStoreExt, View, debug_routes};
use crate::views::overview::{Facts, owed};

/// The rail: the six primary destinations in the order the work happens, and Chat,
/// Commands and Settings bottom-aligned under them. Commands is not a destination — it
/// toggles the palette over whatever is showing — and it stands in the footer because
/// that is where the things that are not places stand. It needs an estate, like Chat: the
/// palette's entries all act on one. With no estate open there is nothing to work on,
/// so the rail carries Settings alone and the window stands on the Start screen — the
/// doors. Overview carries the count of what the estate owes and Decisions the count of
/// unanswered questions.
#[component]
pub fn NavigationRail() -> Element {
    let app = use_context::<Store<AppStore>>();
    let current = app.nav().cloned();
    let has_estate = app.open().is_some();
    let questions = app.estate().questions().cloned();
    let unanswered = questions
        .as_ref()
        .map(|q| q.summary.unanswered)
        .unwrap_or(0);
    let owes = if has_estate {
        let open = app.open().cloned();
        let model = app.estate().model().cloned();
        let diagnostics = app.estate().diagnostics().cloned();
        let hcl = app.estate().hcl().cloned();
        let work_tree = app.estate().work_tree().cloned();
        open.map(|open| {
            owed(&Facts {
                estate: &open.name,
                deployment_mode: open.deployment_mode.as_deref(),
                hcl,
                work_tree: work_tree.as_ref(),
                questions: questions.as_ref(),
                model: model.as_deref(),
                diagnostics: &diagnostics,
            })
            .len()
        })
        .unwrap_or(0)
    } else {
        0
    };
    let primary: &[View] = if has_estate { &View::PRIMARY } else { &[] };
    let mut secondary: Vec<View> = View::SECONDARY
        .into_iter()
        .filter(|v| has_estate || !v.needs_estate())
        .collect();
    if debug_routes() {
        secondary.push(View::Gallery);
    }

    // the shortcut is ⌘K on macOS and Ctrl+K elsewhere, and the button says which of the
    // two this machine takes
    let (palette_icon, palette_label) = if cfg!(target_os = "macos") {
        ("keyboard_command_key", "Commands (⌘K)")
    } else {
        ("keyboard", "Commands (Ctrl+K)")
    };

    rsx! {
        NavRail {
            footer: rsx! {
                for view in secondary {
                    NavRailItem {
                        key: "{view.label()}",
                        icon: view.icon().to_string(),
                        label: view.label().to_string(),
                        selected: view == current,
                        onclick: move |_| app.nav().set(view),
                    }
                    if view == View::Chat {
                        NavRailItem {
                            key: "{palette_label}",
                            icon: palette_icon.to_string(),
                            label: palette_label.to_string(),
                            selected: app.palette_open().cloned(),
                            onclick: move |_| app.palette_open().toggle(),
                        }
                    }
                }
            },
            for view in primary.iter().copied() {
                NavRailItem {
                    key: "{view.label()}",
                    icon: view.icon().to_string(),
                    label: view.label().to_string(),
                    selected: view == current,
                    badge: match view {
                        View::Overview => owes,
                        View::Decisions => unanswered,
                        _ => 0,
                    },
                    onclick: move |_| app.nav().set(view),
                }
            }
        }
    }
}
