use dioxus::prelude::*;

use crate::components::{Badge, CircularProgress, IconButton, TopAppBar};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt};

/// The top bar: the open estate's name and directory, what acts on that estate beside
/// them — reload, "Switch estate", "Close estate" — and the drawer toggle at the far end.
///
/// It carries the estate's ADDRESS and nothing else about it. What the estate IS — the
/// customer it stands for, whom its commands run as, its deployment mode, its schema, how
/// far its HCL has been taken — is the Overview's identity card, so each fact has one
/// place. The versions are not here either: the window title carries satz-studio's, a
/// newer release of either is the banner's business, and Settings holds the satz binary
/// with its update buttons.
#[component]
pub fn TopBar() -> Element {
    let app = use_context::<Store<AppStore>>();
    let estate_handle = try_use_context::<Coroutine<EstateAction>>();
    let open = app.open().cloned();
    let loading = app.estate().loading().cloned();
    let diagnostics = app.estate().diagnostics().len();
    let drawer_open = app.drawer_open().cloned();

    let (title, subtitle) = match &open {
        Some(o) => (o.name.clone(), o.dir.display().to_string()),
        None => ("satz-studio".to_string(), String::new()),
    };

    rsx! {
        TopAppBar {
            title,
            subtitle,
            beside: rsx! {
                if let Some(estate_handle) = estate_handle {
                    IconButton {
                        icon: "refresh",
                        label: "Reload estate",
                        onclick: move |_| estate_handle.send(EstateAction::Reload),
                    }
                    IconButton {
                        icon: "swap_horiz",
                        label: "Switch estate",
                        onclick: move |_| estate_handle.send(EstateAction::Switch),
                    }
                    IconButton {
                        icon: "close",
                        label: "Close estate",
                        onclick: move |_| estate_handle.send(EstateAction::Close),
                    }
                    if loading {
                        CircularProgress { size: 24 }
                    }
                }
            },
            Badge { count: diagnostics,
                IconButton {
                    icon: "troubleshoot",
                    label: "Diagnostics",
                    selected: drawer_open,
                    onclick: move |_| app.drawer_open().toggle(),
                }
            }
        }
    }
}
