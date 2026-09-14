//! The Chat view: the merged agent loop over the open estate. A rail of the estate's
//! transcripts, the conversation streamed turn by turn — text, the summarised
//! thinking, one card per tool call, the approval card for a write — the composer
//! with the model and the effort, and the usage footer. The store lives with the
//! view (`state.rs`); the coroutine that owns the agent is `actions.rs`.

mod actions;
mod approval_card;
mod composer;
mod footer;
mod rail;
mod state;
mod tool_card;
mod transcript_list;

use dioxus::prelude::*;
use satz_studio_core::llm::ClaudeCodeCli;
use satz_studio_core::settings::ProviderChoice;

use self::actions::chat_coroutine;
use self::state::ChatStore;

use self::actions::ChatAction;
use self::approval_card::ApprovalCard;
use self::composer::Composer;
use self::footer::UsageFooter;
use self::rail::TranscriptRail;
use self::state::{AgentStatus, ChatStoreStoreExt, EngineKind};
use self::transcript_list::{NoticeCard, TranscriptList};
use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Icon, IconButton, LinearProgress,
};
use crate::state::{AppStore, AppStoreStoreExt, OpenEstate, View};

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

    rsx! {
        document::Stylesheet { href: CHAT_CSS }
        div { class: "chat",
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
        AgentStatus::NoCredential { tried } => rsx! {
            div { class: "chat__empty",
                Card { variant: CardVariant::Filled, class: "chat__empty-card",
                    Icon { name: "key_off", size: 48, class: "chat__empty-icon" }
                    h2 { "No Claude credential" }
                    p { "The chat needs one. It is taken from the first of these that answers:" }
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
                    Button { icon: "settings", onclick: move |_| app.nav().set(View::Settings), "Open Settings" }
                }
            }
        },
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
