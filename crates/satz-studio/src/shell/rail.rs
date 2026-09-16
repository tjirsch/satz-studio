use dioxus::prelude::*;

use crate::components::{NavRail, NavRailItem};
use crate::state::{AppStore, AppStoreStoreExt, EstateStoreStoreExt, View, debug_routes};
use crate::views::overview::{Facts, owed};

/// The rail: the six primary destinations in the order the work happens, and Chat and
/// Settings bottom-aligned under them. With no estate open there is nothing to work on,
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
        open.map(|open| {
            owed(&Facts {
                estate: &open.name,
                deployment_mode: open.deployment_mode.as_deref(),
                hcl,
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
