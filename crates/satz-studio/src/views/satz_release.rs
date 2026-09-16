//! The half of the Settings satz card that is about releases: what the launch looks found
//! for satz and for satz-studio, the look for a satz-studio release again, and satz's
//! installer while there is no satz. The banner and the top bar offer the same actions;
//! this is where the reasons stay — a look that failed says why here — with the install's
//! log.

use dioxus::prelude::*;
use satz_studio_core::github::StudioUpdate;
use satz_studio_core::satz::CliLine;

use crate::components::{Button, ButtonVariant, Card, CardVariant};
use crate::state::{
    AppAction, AppStore, AppStoreStoreExt, CommandOutcome, InstallStoreStoreExt, SatzStatus,
    StudioLookStoreStoreExt, ToastKind, UpdateStoreStoreExt, install_offer, satz_release_sentence,
    studio_look_sentence, toast,
};

/// The rows under the satz binary field and its update buttons: what the satz check found
/// (or why none ran at launch), "Look for a satz-studio update" with what it found, and
/// "Install satz" with its log while no satz is found.
#[component]
pub fn SatzReleaseActions() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<AppAction>();
    let status = app.satz().cloned();
    let looking = app.studio_look().looking().cloned();
    let look = app.studio_look().outcome().cloned();
    let release_page = match &look {
        Some(Ok(StudioUpdate::Available { version, page })) => {
            Some((version.to_string(), page.clone()))
        }
        _ => None,
    };
    // the satz check is read against the satz it asked, as long as that satz is the one in use
    let found = app.update().found().cloned();
    let not_checked = app.update().not_checked().cloned();
    let satz_found = match (&found, &status) {
        (Some(found), SatzStatus::Located(bin)) => Some(satz_release_sentence(found, &bin.version)),
        (Some(Err(why)), _) => Some(format!("The look for a satz release failed: {why}")),
        _ => None,
    };
    let missing = matches!(status, SatzStatus::Missing(_));
    // the SAVED path decides, as it decides what `locate` searched
    let offer = install_offer(app.settings().read().satz_binary.is_some());
    let installing = app.install().running().cloned();
    let install_command = app.install().command().cloned();
    let install_log = app.install().log().cloned();
    let install_outcome = app.install().outcome().cloned();

    rsx! {
        if let Some(said) = satz_found {
            p { class: "settings__label", "{said}" }
        } else if let Some(why) = not_checked {
            p { class: "settings__label", "{why}" }
        }
        div { class: "settings__row",
            Button {
                variant: ButtonVariant::Text,
                icon: "travel_explore",
                disabled: looking,
                onclick: move |_| handle.send(AppAction::LookForStudioUpdate),
                if looking { "Looking…" } else { "Look for a satz-studio update" }
            }
            if let Some((version, page)) = release_page {
                Button {
                    variant: ButtonVariant::Text,
                    icon: "open_in_new",
                    onclick: move |_| {
                        if let Err(e) = open::that(&page) {
                            toast(app, ToastKind::Error, format!("{page}: {e}"));
                        }
                    },
                    "Open satz-studio {version}"
                }
            }
        }
        if looking {
            p { class: "settings__label", "Looking for a satz-studio release…" }
        } else if let Some(look) = &look {
            p { class: "settings__label", "{studio_look_sentence(look)}" }
        }
        if missing {
            if offer.offered() {
                div { class: "settings__row",
                    Button {
                        variant: ButtonVariant::Tonal,
                        icon: "download",
                        disabled: installing,
                        onclick: move |_| handle.send(AppAction::InstallSatz),
                        if installing { "Installing satz…" } else { "Install satz" }
                    }
                    if installing {
                        Button { variant: ButtonVariant::Text, onclick: move |_| handle.send(AppAction::CancelInstall), "Cancel" }
                    }
                }
            }
            p { class: "settings__label", "{offer.sentence()}" }
        }
        // the log outlives the offer: after an install, satz is found and this is what ran
        if let Some(command) = install_command {
            RunLog { command, outcome: install_outcome, log: install_log }
        }
    }
}

/// One streamed run in a filled card: the command, how it ended, and its lines.
#[component]
fn RunLog(command: String, outcome: Option<CommandOutcome>, log: Vec<CliLine>) -> Element {
    rsx! {
        Card { variant: CardVariant::Filled, class: "settings__log-card",
            code { class: "settings__log-title", "{command}" }
            if let Some(outcome) = outcome {
                p {
                    class: if outcome.ok { "settings__log-outcome" } else { "settings__log-outcome settings__log-outcome--error" },
                    "{outcome.text}"
                }
            }
            pre { class: "log",
                for (i, line) in log.iter().enumerate() {
                    {
                        let (class, text) = match line {
                            CliLine::Stdout(s) => ("log__line", s),
                            CliLine::Stderr(s) => ("log__line log__line--stderr", s),
                        };
                        rsx! { span { key: "{i}", class: "{class}", "{text}\n" } }
                    }
                }
            }
        }
    }
}
