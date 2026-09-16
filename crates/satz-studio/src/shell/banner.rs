use dioxus::prelude::*;

use crate::components::{Button, ButtonVariant, Icon, LinearProgress};
use crate::state::{
    AppAction, AppStore, AppStoreStoreExt, InstallStoreStoreExt, SatzStatus,
    StudioLookStoreStoreExt, ToastKind, UpdateStoreStoreExt, View, install_offer,
    newer_satz_sentence, satz_notice, studio_available, studio_look_sentence, toast,
};

/// A full-width banner while satz cannot be used as it stands, naming what can be done; a
/// thin progress line while it is being located; nothing once it is found.
///
/// Two kinds. An error while satz is too old, missing or does not run — no estate opens,
/// and the banner offers the fix. A notice while satz is NEWER than the one this build was
/// tested against — every estate opens, and the banner says what satz's own release rule
/// makes of the difference until the operator dismisses it for that satz version.
#[component]
pub fn SatzBanner() -> Element {
    let app = use_context::<Store<AppStore>>();
    let status = app.satz().cloned();
    let dismissed = app.settings().read().dismissed_satz.clone();
    match &status {
        SatzStatus::Located(_) => match satz_notice(&status, dismissed.as_deref()) {
            Some((binary, ahead)) => rsx! {
                NoticeBanner { version: binary.version.to_string(), text: newer_satz_sentence(binary, ahead) }
            },
            None => rsx! {},
        },
        SatzStatus::Unknown => rsx! {
            div { class: "banner banner--progress", LinearProgress {} }
        },
        SatzStatus::TooOld {
            found, required, ..
        } => rsx! {
            ErrorBanner {
                text: format!("satz {found} is too old: satz-studio needs {required} or newer. Update it here, or point Settings at a newer binary."),
                update: true,
                install: false,
            }
        },
        SatzStatus::Missing(why) => {
            let offer = install_offer(app.settings().read().satz_binary.is_some());
            rsx! {
                ErrorBanner { text: format!("{why}. {}", offer.sentence()), update: false, install: offer.offered() }
            }
        }
        SatzStatus::Unusable(why) => rsx! {
            ErrorBanner {
                text: format!("{why}. Point Settings at a satz that runs."),
                update: false,
                install: false,
            }
        },
    }
}

/// satz is too old, missing or does not run. "Update satz" for a too-old binary, which can
/// update itself; "Install satz" for none at all, where the installer is offered.
#[component]
fn ErrorBanner(text: String, update: bool, install: bool) -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<AppAction>();
    let updating = app.update().running().cloned();
    let installing = app.install().running().cloned();
    rsx! {
        div { class: "banner banner--error", role: "alert",
            Icon { name: "error", filled: true }
            span { class: "banner__text", "{text}" }
            if update {
                Button {
                    variant: ButtonVariant::Filled,
                    class: "banner__action",
                    disabled: updating,
                    onclick: move |_| handle.send(AppAction::UpdateSatz { check_only: false }),
                    if updating { "Updating satz…" } else { "Update satz" }
                }
            }
            if install {
                Button {
                    variant: ButtonVariant::Filled,
                    class: "banner__action",
                    disabled: installing,
                    onclick: move |_| handle.send(AppAction::InstallSatz),
                    if installing { "Installing satz…" } else { "Install satz" }
                }
                if installing {
                    Button { variant: ButtonVariant::Text, class: "banner__action", onclick: move |_| handle.send(AppAction::CancelInstall), "Cancel" }
                }
            }
            Button { variant: ButtonVariant::Text, class: "banner__action", onclick: move |_| handle.send(AppAction::LocateSatz), "Try again" }
            Button { variant: ButtonVariant::Tonal, class: "banner__action", onclick: move |_| app.nav().set(View::Settings), "Settings" }
        }
    }
}

/// satz is newer than the build, and runs. The fact, what satz's release rule says it
/// means, and what the launch look found for satz-studio — a release built against a newer
/// satz is the answer to the difference.
#[component]
fn NoticeBanner(version: String, text: String) -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<AppAction>();
    let looking = app.studio_look().looking().cloned();
    let look = app.studio_look().outcome().cloned();
    let release =
        studio_available(look.as_ref()).map(|(v, page)| (v.to_string(), page.to_string()));
    let look_again = !looking && !matches!(look, Some(Ok(_)));
    rsx! {
        div { class: "banner banner--notice", role: "status",
            Icon { name: "new_releases", filled: true }
            div { class: "banner__text",
                p { "{text}" }
                if looking {
                    p { class: "banner__detail", "Looking for a satz-studio release…" }
                } else if let Some(look) = &look {
                    p { class: "banner__detail", "{studio_look_sentence(look)}" }
                }
            }
            if let Some((version, page)) = release {
                Button {
                    variant: ButtonVariant::Filled,
                    class: "banner__action",
                    icon: "open_in_new",
                    onclick: move |_| {
                        if let Err(e) = open::that(&page) {
                            toast(app, ToastKind::Error, format!("{page}: {e}"));
                        }
                    },
                    "Open satz-studio {version}"
                }
            }
            if look_again {
                Button {
                    variant: ButtonVariant::Tonal,
                    class: "banner__action",
                    onclick: move |_| handle.send(AppAction::LookForStudioUpdate),
                    "Look for a satz-studio update"
                }
            }
            Button {
                variant: ButtonVariant::Text,
                class: "banner__action",
                onclick: move |_| handle.send(AppAction::DismissSatzNotice(version.clone())),
                "Dismiss"
            }
        }
    }
}
