use dioxus::prelude::*;

use crate::components::{NavRail, NavRailItem};
use crate::state::{AppStore, AppStoreStoreExt, EstateStoreStoreExt, View, debug_routes};
use crate::views::overview::owed_now;

/// The rail: the six primary destinations in the order the work happens, and Agent,
/// Commands and Settings bottom-aligned under them. Commands is not a destination — it
/// toggles the palette over whatever is showing — and it stands in the footer because
/// that is where the things that are not places stand. It needs an estate, like Agent: the
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
    let owes = owed_now(app).map_or(0, |owed| owed.len());
    // a workload estate without a preset library of its own has no Presets
    // destination; until the model is read the rail shows every destination
    let presets = app
        .estate()
        .model()
        .read()
        .as_deref()
        .is_none_or(|m| m.packs.has_presets());
    let primary: Vec<View> = if has_estate {
        View::primary(presets)
    } else {
        Vec::new()
    };
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
                    if view == View::Agent {
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
