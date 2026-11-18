use dioxus::prelude::*;
use dioxus_icons::lucide::{Activity, ArrowLeftRight, LoaderCircle, RefreshCw, X};

use crate::components::badge::{Badge, BadgeVariant};
use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::sidebar::SidebarTrigger;
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt};

/// The top bar: the button that folds the sidebar, the open estate's name and directory,
/// what acts on that estate beside them — reload, "Switch estate", "Close estate" — and
/// the drawer toggle at the far end.
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
        header { class: "top-bar",
            SidebarTrigger {}
            div { class: "top-bar__title",
                strong { "{title}" }
                if !subtitle.is_empty() {
                    span { class: "top-bar__subtitle", "{subtitle}" }
                }
            }
            if let Some(estate_handle) = estate_handle {
                div { class: "top-bar__estate",
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Icon,
                        title: "Reload estate",
                        "aria-label": "Reload estate",
                        onclick: move |_| estate_handle.send(EstateAction::Reload),
                        RefreshCw { size: 16 }
                    }
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Icon,
                        title: "Switch estate",
                        "aria-label": "Switch estate",
                        onclick: move |_| estate_handle.send(EstateAction::Switch),
                        ArrowLeftRight { size: 16 }
                    }
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Icon,
                        title: "Close estate",
                        "aria-label": "Close estate",
                        onclick: move |_| estate_handle.send(EstateAction::Close),
                        X { size: 16 }
                    }
                    if loading {
                        LoaderCircle { size: 16, class: "spin" }
                    }
                }
            }
            div { class: "top-bar__actions",
                Button {
                    variant: if drawer_open { ButtonVariant::Secondary } else { ButtonVariant::Ghost },
                    title: "Diagnostics",
                    "aria-label": "Diagnostics",
                    "aria-pressed": if drawer_open { "true" } else { "false" },
                    onclick: move |_| app.drawer_open().toggle(),
                    Activity { size: 16 }
                    if diagnostics > 0 {
                        Badge { variant: BadgeVariant::Secondary, "{diagnostics}" }
                    }
                }
            }
        }
    }
}
