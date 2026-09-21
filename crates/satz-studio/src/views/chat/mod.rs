//! The Chat view: the merged agent loop over the open estate. A rail of the estate's
//! transcripts, the conversation streamed turn by turn — text, the summarised
//! thinking, one card per tool call, the approval card for a write — the composer
//! with the model and the effort, the usage footer, and, while it is switched on, the
//! debug panel with every call's JSON. The store lives with the
//! view (`state.rs`); the coroutine that owns the agent is `actions.rs`.

mod actions;
mod approval_card;
mod composer;
mod debug_panel;
mod footer;
mod rail;
mod state;
mod summary;
mod tool_card;
mod transcript_list;

use dioxus::prelude::*;
use satz_studio_core::llm::{AuthStatus, ClaudeCodeCli};
use satz_studio_core::settings::ProviderChoice;

use self::actions::chat_coroutine;
use self::state::ChatStore;

use self::actions::ChatAction;
use self::approval_card::ApprovalCard;
use self::composer::Composer;
use self::debug_panel::DebugPanel;
use self::footer::UsageFooter;
use self::rail::TranscriptRail;
use self::state::{AgentStatus, ChatStoreStoreExt, EngineKind};
use self::transcript_list::{NoticeCard, TranscriptList};
use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Icon, IconButton, LinearProgress,
};
use crate::state::{AppStore, AppStoreStoreExt, OpenEstate, View};
use crate::views::settings::{EngineOffer, engine_offer};

const CHAT_CSS: Asset = asset!("/assets/css/chat.css");

#[component]
pub fn ChatView() -> Element {
    let app = use_context::<Store<AppStore>>();
    match app.open().cloned() {
        Some(open) => rsx! { ChatHost { open } },
        None => rsx! {},
    }
}

/// One per open estate: the store and the coroutine live here, so a reopened estate
/// starts a fresh chat.
#[component]
fn ChatHost(open: OpenEstate) -> Element {
    let app = use_context::<Store<AppStore>>();
    let chat = use_store(|| {
        let settings = app.settings();
        let settings = settings.peek();
        let engine = match settings.provider {
            ProviderChoice::ClaudeCode { .. } => EngineKind::ClaudeCode,
            _ => EngineKind::Api,
        };
        ChatStore::new(settings.model.clone(), settings.effort, engine)
    });
    use_context_provider(|| chat);
    let session = open.session.clone();
    use_coroutine(move |rx| chat_coroutine(rx, app, chat, session.clone()));
    let debug_on = app.settings().read().chat_debug_log;

    rsx! {
        document::Stylesheet { href: CHAT_CSS }
        div { class: "chat", class: if debug_on { "chat--debug" },
            TranscriptRail {}
            section { class: "chat__main",
                AgentStatusCard {}
                ErrorLine {}
                TranscriptList {}
                NoticeCard {}
                ApprovalCard {}
                Composer {}
                UsageFooter {}
            }
            if debug_on {
                DebugPanel {}
            }
        }
    }
}

/// Nothing while the engine is ready; a progress line while it starts; the empty
/// state naming the four credential sources when none answered, or the Claude Code
/// sign-in when that engine is signed out; the error when the engine could not be
/// built.
#[component]
fn AgentStatusCard() -> Element {
    let app = use_context::<Store<AppStore>>();
    let chat = use_context::<Store<ChatStore>>();
    let handle = use_coroutine_handle::<ChatAction>();
    match chat.agent().cloned() {
        AgentStatus::Ready => rsx! {},
        AgentStatus::Starting => rsx! {
            div { class: "chat__starting", LinearProgress {} }
        },
        AgentStatus::NoCredential { tried } => rsx! { NoCredentialCard { tried } },
        AgentStatus::NotSignedIn { login } => rsx! {
            div { class: "chat__empty",
                Card { variant: CardVariant::Filled, class: "chat__empty-card",
                    Icon { name: "account_circle_off", size: 48, class: "chat__empty-icon" }
                    h2 { "Claude Code is not signed in" }
                    p { "This engine runs on the claude.ai account the Claude Code CLI is signed in to. Signing in opens a browser from your terminal; satz-studio never reads the credential." }
                    div { class: "chat__actions",
                        Button {
                            variant: ButtonVariant::Filled,
                            icon: "login",
                            onclick: {
                                let login = login.clone();
                                move |_| {
                                    if let Err(e) = ClaudeCodeCli::open_in_terminal(&login) {
                                        chat.error().set(Some(e.to_string()));
                                    }
                                }
                            },
                            "Sign in"
                        }
                        Button { variant: ButtonVariant::Outlined, icon: "refresh", onclick: move |_| handle.send(ChatAction::Restart), "Check again" }
                    }
                }
            }
        },
        AgentStatus::Failed(error) => rsx! {
            div { class: "chat__empty",
                Card { variant: CardVariant::Filled, class: "chat__empty-card chat__empty-card--error",
                    Icon { name: "error", size: 48, filled: true, class: "chat__empty-icon" }
                    h2 { "The agent could not start" }
                    p { class: "chat__empty-error", "{error}" }
                    Button { icon: "settings", onclick: move |_| app.nav().set(View::Settings), "Open Settings" }
                }
            }
        },
    }
}

/// What the card shows, from what the Claude Code probe has answered so far: nothing
/// yet is [`EngineOffer::Checking`], and a CLI that did not answer is
/// [`EngineOffer::Absent`] — the reason is shown beside it.
fn offer_from_probe(
    answered: Option<&Result<AuthStatus, String>>,
    provider: &ProviderChoice,
) -> EngineOffer {
    match answered {
        None => EngineOffer::Checking,
        Some(Ok(status)) => engine_offer(Some(status), provider),
        Some(Err(_)) => EngineOffer::Absent,
    }
}

/// The Messages API engine found no credential. Claude Code is probed once while the
/// card renders: when that CLI is signed in the card leads with it and one button puts
/// the chat on it, and the four API sources stay below as the alternative. When it is
/// signed out or not installed, the card is the four sources with one line naming
/// Claude Code and what it needs.
#[component]
fn NoCredentialCard(tried: Vec<String>) -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<ChatAction>();
    let probe = use_signal(|| None::<Result<AuthStatus, String>>);
    // once per mount, and off the render: locating the CLI and asking it runs two
    // processes
    use_hook(move || {
        let path = app.settings().peek().claude_code_binary.clone();
        let mut probe = probe;
        spawn(async move {
            let answered = match ClaudeCodeCli::locate(path.as_deref()).await {
                Ok(cli) => cli.auth_status().await.map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            };
            probe.set(Some(answered));
        });
    });
    let answered = probe();
    let provider = app.settings().read().provider.clone();
    let offer = offer_from_probe(answered.as_ref(), &provider);
    let reason = match &answered {
        Some(Err(e)) => e.clone(),
        _ => String::new(),
    };
    let sources_intro = if offer.signed_in() {
        "It needs a credential, taken from the first of these that answers:"
    } else {
        "The chat needs one. It is taken from the first of these that answers:"
    };

    rsx! {
        div { class: "chat__empty",
            Card { variant: CardVariant::Filled, class: "chat__empty-card",
                if offer.signed_in() {
                    div { class: "chat__lead",
                        h3 { class: "chat__lead-title",
                            Icon { name: "check_circle", size: 24, filled: true }
                            "Claude Code is ready"
                        }
                        p { class: "chat__lead-account", "{offer.account()}." }
                        p { "It runs on that claude.ai subscription; no API key is used and satz-studio reads no credential of Claude Code's. The button selects Claude Code in Settings and starts this chat on it." }
                        div { class: "chat__actions",
                            Button {
                                variant: ButtonVariant::Filled,
                                icon: "bolt",
                                onclick: move |_| handle.send(ChatAction::UseClaudeCode),
                                "Use Claude Code"
                            }
                        }
                    }
                    h3 { class: "chat__alt", "Or use the Messages API" }
                } else {
                    Icon { name: "key_off", size: 48, class: "chat__empty-icon" }
                    h2 { "No Claude credential" }
                }
                p { "{sources_intro}" }
                ol { class: "chat__sources",
                    li { code { "ANTHROPIC_API_KEY" } " in the environment" }
                    li { code { "ANTHROPIC_AUTH_TOKEN" } " in the environment" }
                    li { "the " code { "ant auth login" } " profile, when " code { "ant" } " is on PATH" }
                    li { "the key stored in the OS keychain from Settings" }
                }
                p { class: "chat__tried-title", "What each answered:" }
                ul { class: "chat__tried",
                    for (i, note) in tried.iter().enumerate() {
                        li { key: "{i}", "{note}" }
                    }
                }
                match &offer {
                    EngineOffer::Checking => rsx! {
                        p { class: "chat__note", "Checking whether Claude Code is signed in." }
                    },
                    EngineOffer::SignedOut => rsx! {
                        p { class: "chat__note",
                            "Claude Code is the other way: it runs on a claude.ai subscription, and this one is signed out. Run "
                            code { "claude auth login" }
                            " in a terminal, then select Claude Code in Settings."
                        }
                    },
                    EngineOffer::Absent => rsx! {
                        p { class: "chat__note",
                            "Claude Code is the other way: it runs on a claude.ai subscription. No CLI answered here — {reason}. Install it, run "
                            code { "claude auth login" }
                            ", then select Claude Code in Settings."
                        }
                    },
                    // the lead above says it: the CLI is signed in
                    EngineOffer::InUse { .. } | EngineOffer::Ready { .. } => rsx! {},
                }
                Button { icon: "settings", onclick: move |_| app.nav().set(View::Settings), "Open Settings" }
            }
        }
    }
}

/// An error outside a turn — a transcript not written, an action refused — until dismissed.
#[component]
fn ErrorLine() -> Element {
    let chat = use_context::<Store<ChatStore>>();
    let Some(error) = chat.error().cloned() else {
        return rsx! {};
    };
    rsx! {
        div { class: "chat__error", role: "alert",
            Icon { name: "error", size: 20, filled: true }
            span { class: "chat__error-text", "{error}" }
            IconButton { icon: "close", label: "Dismiss", onclick: move |_| chat.error().set(None) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answered(logged_in: bool) -> Result<AuthStatus, String> {
        Ok(AuthStatus {
            logged_in,
            auth_method: Some("claude.ai".to_string()),
            api_provider: None,
            email: Some("first.admin@example.com".to_string()),
        })
    }

    #[test]
    fn the_probe_reduces_to_checking_then_to_what_the_cli_answered() {
        let api = ProviderChoice::Claude;
        // nothing back yet: the card says so and offers nothing
        assert_eq!(offer_from_probe(None, &api), EngineOffer::Checking);
        // signed in, and the Messages API is the selected engine: the card offers the switch
        assert_eq!(
            offer_from_probe(Some(&answered(true)), &api),
            EngineOffer::Ready {
                account: "signed in as first.admin@example.com via claude.ai".to_string()
            }
        );
        assert_eq!(
            offer_from_probe(Some(&answered(false)), &api),
            EngineOffer::SignedOut
        );
    }

    #[test]
    fn a_cli_that_did_not_answer_is_absent_and_never_reads_as_ready() {
        let failed = Err("the Claude Code CLI was not found".to_string());
        let offer = offer_from_probe(Some(&failed), &ProviderChoice::Claude);
        assert_eq!(offer, EngineOffer::Absent);
        assert!(!offer.signed_in());
    }
}
