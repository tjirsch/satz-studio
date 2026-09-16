use dioxus::prelude::*;

use crate::components::{Button, ButtonVariant, Icon, LinearProgress};
use crate::state::{AppAction, AppStore, AppStoreStoreExt, SatzStatus, UpdateStoreStoreExt, View};

/// A full-width banner while satz is missing or too old, naming the fix; a thin
/// progress line while it is being located; nothing once it is found.
#[component]
pub fn SatzBanner() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<AppAction>();
    let status = app.satz().cloned();
    // A too-old satz can still update itself, and that is the only thing the operator can
    // do about it without leaving the window. `Missing` offers nothing: there is no binary.
    let updatable = matches!(status, SatzStatus::TooOld { .. });
    let updating = app.update().running().cloned();
    let text = match &status {
        SatzStatus::Located(_) => return rsx! {},
        SatzStatus::Unknown => {
            return rsx! {
                div { class: "banner banner--progress", LinearProgress {} }
            };
        }
        SatzStatus::TooOld {
            found, required, ..
        } => {
            format!(
                "satz {found} is too old: satz-studio needs {required} or newer. Update it here, or point Settings at a newer binary."
            )
        }
        SatzStatus::Missing(why) => format!("{why}. Install satz, or set its path in Settings."),
    };
    rsx! {
        div { class: "banner banner--error", role: "alert",
            Icon { name: "error", filled: true }
            span { class: "banner__text", "{text}" }
            if updatable {
                Button {
                    variant: ButtonVariant::Filled,
                    class: "banner__action",
                    disabled: updating,
                    onclick: move |_| handle.send(AppAction::UpdateSatz { check_only: false }),
                    if updating { "Updating satz…" } else { "Update satz" }
                }
            }
            Button { variant: ButtonVariant::Text, class: "banner__action", onclick: move |_| handle.send(AppAction::LocateSatz), "Try again" }
            Button { variant: ButtonVariant::Tonal, class: "banner__action", onclick: move |_| app.nav().set(View::Settings), "Settings" }
        }
    }
}
