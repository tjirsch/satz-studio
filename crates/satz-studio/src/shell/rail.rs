use dioxus::prelude::*;

use crate::components::{Fab, FabSize, NavRail, NavRailItem, Tooltip};
use crate::state::{AppAction, AppStore, AppStoreStoreExt, EstateStoreStoreExt, View};
use crate::views::estates::pick_root;

/// The rail: "Open estate" as the primary action, one destination per [`View`], the
/// open-question count on Interview and the diagnostics count on Resources while an
/// estate is open.
#[component]
pub fn NavigationRail() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<AppAction>();
    let current = app.nav().cloned();
    let has_estate = app.open().is_some();
    let questions = if has_estate {
        app.estate()
            .questions()
            .read()
            .as_ref()
            .map(|q| q.summary.unanswered)
            .unwrap_or(0)
    } else {
        0
    };
    let diagnostics = if has_estate {
        app.estate().diagnostics().len()
    } else {
        0
    };
    rsx! {
        NavRail {
            fab: rsx! {
                Tooltip { text: "Open estate",
                    Fab {
                        icon: "folder_open",
                        label: "Open estate",
                        size: FabSize::Medium,
                        class: "rail-fab",
                        onclick: move |_| {
                            app.nav().set(View::Estates);
                            pick_root(handle);
                        },
                    }
                }
            },
            for view in View::ALL {
                NavRailItem {
                    key: "{view.label()}",
                    icon: view.icon().to_string(),
                    label: view.label().to_string(),
                    selected: view == current,
                    badge: match view {
                        View::Interview => questions,
                        View::Resources => diagnostics,
                        _ => 0,
                    },
                    onclick: move |_| app.nav().set(view),
                }
            }
        }
    }
}
