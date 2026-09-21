//! The Settings view: every field of `Settings` as a form, saved as one file; the
//! detected satz beside its path; the credential's source and the keychain key; and
//! the Claude Code CLI with the claude.ai account it is signed in to (ADR 0010).
//!
//! A card says which engine is IN USE, not only which account is signed in: a CLI that
//! is signed in while another engine is selected is an offer, and [`EngineOffer`] is
//! that decision. The Chat view's empty state renders from the same function.

use std::path::PathBuf;

use dioxus::prelude::*;
use satz_studio_core::llm::{AuthStatus, ClaudeCodeCli, CredentialSource, Effort};
use satz_studio_core::satz::Allow;
use satz_studio_core::settings::{ProviderChoice, Settings, Theme, settings_path};

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Icon, Segment, SegmentedButton, Switch, TextField,
};
use crate::state::{
    AppAction, AppStore, AppStoreStoreExt, CredentialStatus, SatzStatus, ToastKind,
    UpdateStoreStoreExt, ahead_sentence, toast,
};
use crate::views::satz_release::SatzReleaseActions;
use satz_studio_core::satz::CliLine;

#[component]
pub fn SettingsView() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<AppAction>();
    let saved = app.settings().cloned();
    let mut draft = use_signal(|| saved.clone());
    let dirty = draft() != saved;
    // The banner's "Dismiss" writes `dismissed_satz` into the saved settings behind this
    // draft: the draft follows it, so a Save here does not bring the notice back.
    use_effect(move || {
        let dismissed = app.settings().read().dismissed_satz.clone();
        if draft.peek().dismissed_satz != dismissed {
            draft.write().dismissed_satz = dismissed;
        }
    });
    let satz = app.satz().cloned();
    let satz_text = match &satz {
        SatzStatus::Unknown => "locating satz".to_string(),
        SatzStatus::Located(bin) => match bin.ahead_of_build() {
            None => format!("satz {} at {}", bin.version, bin.path.display()),
            Some(ahead) => format!(
                "satz {} at {}; this satz-studio was built and tested against satz {}. {}",
                bin.version,
                bin.path.display(),
                satz_studio_core::satz::SatzBinary::built_against(),
                ahead_sentence(ahead)
            ),
        },
        SatzStatus::TooOld {
            found, required, ..
        } => {
            format!("satz {found} found; {required} or newer is needed")
        }
        SatzStatus::Missing(why) | SatzStatus::Unusable(why) => why.clone(),
    };
    let satz_error = matches!(
        satz,
        SatzStatus::TooOld { .. } | SatzStatus::Missing(_) | SatzStatus::Unusable(_)
    );
    // There is something to update whenever there is a binary — a too-old one included,
    // which is the case that matters most.
    let satz_updatable = app.satz().read().updatable().is_some();
    let satz_updating = app.update().running().cloned();
    let update_command = app.update().command().cloned();
    let update_log = app.update().log().cloned();
    let update_outcome = app.update().outcome().cloned();
    let file = settings_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| e.to_string());

    let allow_value = draft().mcp_allow.as_arg().to_string();
    let provider_value = match draft().provider {
        ProviderChoice::Claude => "claude",
        ProviderChoice::OpenAiCompat { .. } => "openai_compat",
        ProviderChoice::Ollama { .. } => "ollama",
        ProviderChoice::ClaudeCode { .. } => "claude_code",
    }
    .to_string();
    let effort_value = effort_key(draft().effort).to_string();
    let theme_value = match draft().theme {
        Theme::System => "system",
        Theme::Light => "light",
        Theme::Dark => "dark",
    }
    .to_string();
    let endpoint = match &draft().provider {
        ProviderChoice::Claude | ProviderChoice::ClaudeCode { .. } => None,
        ProviderChoice::OpenAiCompat { base_url, model }
        | ProviderChoice::Ollama { base_url, model } => Some((base_url.clone(), model.clone())),
    };
    let claude_code_model = match &draft().provider {
        ProviderChoice::ClaudeCode { model } => Some(model.clone().unwrap_or_default()),
        _ => None,
    };

    rsx! {
        div { class: "view settings",
            h1 { class: "view__title", "Settings" }
            div { class: "settings__grid",
                Card { variant: CardVariant::Outlined, class: "settings__card",
                    h2 { class: "settings__heading", Icon { name: "terminal", size: 20 } "satz" }
                    TextField {
                        label: "satz binary",
                        value: draft().satz_binary.map(|p| p.display().to_string()).unwrap_or_default(),
                        placeholder: "on PATH, then ~/.local/bin/satz",
                        monospace: true,
                        supporting: satz_text,
                        error: satz_error,
                        oninput: move |v: String| draft.write().satz_binary = if v.trim().is_empty() { None } else { Some(PathBuf::from(v.trim())) },
                    }
                    // satz owns its own updater, so the app runs it rather than fetching
                    // anything: this is the same command the terminal instruction used to
                    // name, with the browser it would otherwise open turned off.
                    if satz_updatable {
                        div { class: "settings__row",
                            Button {
                                variant: ButtonVariant::Tonal,
                                disabled: satz_updating,
                                onclick: move |_| handle.send(AppAction::UpdateSatz { check_only: false }),
                                if satz_updating { "Updating satz…" } else { "Update satz" }
                            }
                            Button {
                                variant: ButtonVariant::Text,
                                disabled: satz_updating,
                                onclick: move |_| handle.send(AppAction::UpdateSatz { check_only: true }),
                                "Check only"
                            }
                            if satz_updating {
                                Button { variant: ButtonVariant::Text, onclick: move |_| handle.send(AppAction::CancelUpdate), "Cancel" }
                            }
                        }
                        if let Some(command) = update_command {
                            Card { variant: CardVariant::Filled, class: "settings__log-card",
                                code { class: "settings__log-title", "{command}" }
                                if let Some(outcome) = update_outcome {
                                    p {
                                        class: if outcome.ok { "settings__log-outcome" } else { "settings__log-outcome settings__log-outcome--error" },
                                        "{outcome.text}"
                                    }
                                }
                                pre { class: "log",
                                    for (i, line) in update_log.iter().enumerate() {
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
                    SatzReleaseActions {}
                    p { class: "settings__label", "Capability ceiling of every satz mcp this app starts" }
                    SegmentedButton {
                        options: vec![
                            Segment::new("read", "read").with_icon("visibility"),
                            Segment::new("read,write", "read, write").with_icon("edit"),
                            Segment::new("read,write,exec", "read, write, exec").with_icon("bolt"),
                        ],
                        selected: allow_value,
                        onselect: move |v: String| {
                            draft.write().mcp_allow = match v.as_str() {
                                "read" => Allow::Read,
                                "read,write,exec" => Allow::ReadWriteExec,
                                _ => Allow::ReadWrite,
                            };
                        },
                    }
                    Switch { label: "Run non-destructive write tools the agent asks for without an approval card", checked: draft().auto_approve_writes, onchange: move |v| draft.write().auto_approve_writes = v }
                }
                Card { variant: CardVariant::Outlined, class: "settings__card",
                    h2 { class: "settings__heading", Icon { name: "smart_toy", size: 20 } "Model" }
                    p { class: "settings__label", "Provider" }
                    SegmentedButton {
                        options: vec![
                            Segment::new("claude", "Claude"),
                            Segment::new("claude_code", "Claude Code (your claude.ai account)"),
                            Segment::new("openai_compat", "OpenAI-compatible"),
                            Segment::new("ollama", "Ollama"),
                        ],
                        selected: provider_value,
                        onselect: move |v: String| {
                            let current = draft().provider;
                            let (base_url, model) = match current {
                                ProviderChoice::Claude | ProviderChoice::ClaudeCode { .. } => (String::new(), String::new()),
                                ProviderChoice::OpenAiCompat { base_url, model } | ProviderChoice::Ollama { base_url, model } => (base_url, model),
                            };
                            draft.write().provider = match v.as_str() {
                                "openai_compat" => ProviderChoice::OpenAiCompat { base_url: if base_url.is_empty() { "http://localhost:1234/v1".to_string() } else { base_url }, model },
                                "ollama" => ProviderChoice::Ollama { base_url: if base_url.is_empty() { "http://localhost:11434".to_string() } else { base_url }, model },
                                "claude_code" => ProviderChoice::ClaudeCode { model: None },
                                _ => ProviderChoice::Claude,
                            };
                        },
                    }
                    if let Some((base_url, model)) = endpoint {
                        TextField { label: "Base URL", value: base_url, monospace: true, oninput: move |v: String| set_endpoint(&mut draft, Some(v), None) }
                        TextField { label: "Model", value: model, monospace: true, supporting: "the model name the endpoint serves", oninput: move |v: String| set_endpoint(&mut draft, None, Some(v)) }
                    } else if let Some(model) = claude_code_model {
                        TextField {
                            label: "Claude Code model",
                            value: model,
                            monospace: true,
                            placeholder: "opus",
                            supporting: "what Claude Code is given as --model; empty leaves it its own default",
                            oninput: move |v: String| {
                                let v = v.trim().to_string();
                                draft.write().provider = ProviderChoice::ClaudeCode { model: (!v.is_empty()).then_some(v) };
                            },
                        }
                    } else {
                        TextField { label: "Claude model", value: draft().model, monospace: true, oninput: move |v: String| draft.write().model = v }
                    }
                    p { class: "settings__label", "Effort" }
                    SegmentedButton {
                        options: vec![Segment::new("low", "low"), Segment::new("medium", "medium"), Segment::new("high", "high"), Segment::new("xhigh", "xhigh"), Segment::new("max", "max")],
                        selected: effort_value,
                        onselect: move |v: String| {
                            if let Some(e) = effort_from(&v) {
                                draft.write().effort = e;
                            }
                        },
                    }
                    Switch { label: "Server-side refusal fallbacks", checked: draft().fallbacks, onchange: move |v| draft.write().fallbacks = v }
                    Switch { label: "Keep transcripts under the app's data directory", checked: draft().persist_transcripts, onchange: move |v| draft.write().persist_transcripts = v }
                }
                Card { variant: CardVariant::Outlined, class: "settings__card",
                    h2 { class: "settings__heading", Icon { name: "contrast", size: 20 } "Appearance" }
                    p { class: "settings__label", "Theme" }
                    SegmentedButton {
                        options: vec![
                            Segment::new("system", "System").with_icon("brightness_auto"),
                            Segment::new("light", "Light").with_icon("light_mode"),
                            Segment::new("dark", "Dark").with_icon("dark_mode"),
                        ],
                        selected: theme_value,
                        onselect: move |v: String| {
                            draft.write().theme = match v.as_str() {
                                "light" => Theme::Light,
                                "dark" => Theme::Dark,
                                _ => Theme::System,
                            };
                        },
                    }
                }
                CredentialCard {}
                ClaudeCodeCard { draft }
            }
            div { class: "settings__actions",
                span { class: "settings__file", "{file}" }
                Button {
                    variant: ButtonVariant::Text,
                    icon: "folder_open",
                    onclick: move |_| {
                        let dir = settings_path().ok().and_then(|p| p.parent().map(std::path::Path::to_path_buf));
                        match dir {
                            Some(dir) => {
                                if let Err(e) = std::fs::create_dir_all(&dir).map_err(|e| e.to_string()).and_then(|()| open::that(&dir).map_err(|e| e.to_string())) {
                                    toast(app, ToastKind::Error, format!("{}: {e}", dir.display()));
                                }
                            }
                            None => toast(app, ToastKind::Error, "no configuration directory on this system"),
                        }
                    },
                    "Show file"
                }
                span { class: "grow" }
                Button { variant: ButtonVariant::Text, disabled: !dirty, onclick: move |_| draft.set(app.settings().cloned()), "Discard" }
                Button { variant: ButtonVariant::Filled, icon: "save", disabled: !dirty, onclick: move |_| handle.send(AppAction::SaveSettings(draft())), "Save" }
            }
        }
    }
}

fn set_endpoint(draft: &mut Signal<Settings>, base_url: Option<String>, model: Option<String>) {
    let mut d = draft.write();
    match &mut d.provider {
        ProviderChoice::OpenAiCompat {
            base_url: b,
            model: m,
        }
        | ProviderChoice::Ollama {
            base_url: b,
            model: m,
        } => {
            if let Some(v) = base_url {
                *b = v;
            }
            if let Some(v) = model {
                *m = v;
            }
        }
        ProviderChoice::Claude | ProviderChoice::ClaudeCode { .. } => {}
    }
}

fn effort_key(e: Effort) -> &'static str {
    match e {
        Effort::Low => "low",
        Effort::Medium => "medium",
        Effort::High => "high",
        Effort::Xhigh => "xhigh",
        Effort::Max => "max",
    }
}

fn effort_from(s: &str) -> Option<Effort> {
    Some(match s {
        "low" => Effort::Low,
        "medium" => Effort::Medium,
        "high" => Effort::High,
        "xhigh" => Effort::Xhigh,
        "max" => Effort::Max,
        _ => return None,
    })
}

/// Where the Claude credential comes from, and a key for the keychain.
#[component]
fn CredentialCard() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<AppAction>();
    use_hook(|| handle.send(AppAction::ResolveCredential));
    let mut key = use_signal(String::new);
    let status = app.credential().cloned();
    let (icon, text, error) = match &status {
        CredentialStatus::Unknown => ("hourglass_empty", "checking".to_string(), false),
        CredentialStatus::Resolved(source) => (
            "key",
            match source {
                CredentialSource::ApiKeyEnv => "ANTHROPIC_API_KEY from the environment",
                CredentialSource::AuthTokenEnv => "ANTHROPIC_AUTH_TOKEN from the environment",
                CredentialSource::AntProfile => "the `ant auth login` profile",
                CredentialSource::Keychain => "the key stored in the OS keychain",
            }
            .to_string(),
            false,
        ),
        CredentialStatus::Error(e) => ("key_off", e.clone(), true),
    };
    // the credential is the Messages API engine's; whether that engine is the one the
    // chat runs on is the other half of the sentence
    let in_use = match app.settings().read().provider {
        ProviderChoice::Claude => "in use",
        _ => "not the selected engine",
    };
    rsx! {
        Card { variant: CardVariant::Outlined, class: "settings__card",
            h2 { class: "settings__heading", Icon { name: "vpn_key", size: 20 } "Claude credential" }
            p { class: "settings__status", class: if error { "settings__status--error" },
                Icon { name: icon, size: 20 }
                span { "{text}" }
                span { class: "settings__label", "{in_use}" }
                Button { variant: ButtonVariant::Text, onclick: move |_| handle.send(AppAction::ResolveCredential), "Check again" }
            }
            p { class: "settings__label", "Resolution order: ANTHROPIC_API_KEY, ANTHROPIC_AUTH_TOKEN, the ant profile, the keychain. Nothing is written to the settings file." }
            div { class: "settings__key",
                TextField { label: "API key", value: key(), password: true, monospace: true, class: "grow", oninput: move |v| key.set(v) }
                Button {
                    variant: ButtonVariant::Tonal,
                    icon: "lock",
                    disabled: key().trim().is_empty(),
                    onclick: move |_| {
                        handle.send(AppAction::StoreKey(key().trim().to_string()));
                        key.set(String::new());
                    },
                    "Store in keychain"
                }
            }
        }
    }
}

/// What the Claude Code engine is right now: the one decision the Settings card and
/// the Chat view's empty state both render from. Being signed in and being the engine
/// the chat runs on are two different statements, and this tells them apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineOffer {
    /// the CLI has not answered yet
    Checking,
    /// signed in, and Claude Code is the selected engine
    InUse { account: String },
    /// signed in, and another engine is selected: one button switches
    Ready { account: String },
    /// the CLI answered and is signed out
    SignedOut,
    /// no CLI answered: none is installed, or the path in Settings names nothing
    Absent,
}

impl EngineOffer {
    /// The CLI is signed in, whichever engine is selected.
    pub fn signed_in(&self) -> bool {
        matches!(self, EngineOffer::InUse { .. } | EngineOffer::Ready { .. })
    }

    /// The account, as `claude auth status` reported it; empty while there is none.
    pub fn account(&self) -> &str {
        match self {
            EngineOffer::InUse { account } | EngineOffer::Ready { account } => account,
            _ => "",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            EngineOffer::Checking => "hourglass_empty",
            EngineOffer::InUse { .. } => "check_circle",
            EngineOffer::Ready { .. } => "account_circle",
            EngineOffer::SignedOut | EngineOffer::Absent => "account_circle_off",
        }
    }

    /// The card's state line: the account, and whether this engine is the one in use.
    pub fn status_line(&self) -> String {
        match self {
            EngineOffer::Checking => "checking".to_string(),
            EngineOffer::InUse { account } => format!("{account}, and in use"),
            EngineOffer::Ready { account } => format!("{account}, not the selected engine"),
            EngineOffer::SignedOut => "not signed in".to_string(),
            EngineOffer::Absent => "no CLI to ask".to_string(),
        }
    }

    /// There is nothing to run this engine on, so the line reads as a fault.
    pub fn is_error(&self) -> bool {
        matches!(self, EngineOffer::SignedOut | EngineOffer::Absent)
    }
}

/// What the Claude Code card says, from what `claude auth status` answered and which
/// engine the saved settings select. `None` is a CLI that did not answer.
pub fn engine_offer(status: Option<&AuthStatus>, provider: &ProviderChoice) -> EngineOffer {
    let Some(status) = status else {
        return EngineOffer::Absent;
    };
    if !status.logged_in {
        return EngineOffer::SignedOut;
    }
    let account = account_line(status);
    match provider {
        ProviderChoice::ClaudeCode { .. } => EngineOffer::InUse { account },
        _ => EngineOffer::Ready { account },
    }
}

/// The account `claude auth status` reported, as a sentence. The address is shown
/// here and nowhere else — it is never logged and never written to the settings file.
fn account_line(status: &AuthStatus) -> String {
    match (&status.email, &status.auth_method) {
        (Some(email), Some(method)) => format!("signed in as {email} via {method}"),
        (Some(email), None) => format!("signed in as {email}"),
        (None, Some(method)) => format!("signed in via {method}"),
        (None, None) => "signed in".to_string(),
    }
}

/// What the Claude Code CLI answered: the binary, and the account it is signed in to.
#[derive(Clone, PartialEq)]
enum ClaudeCodeProbe {
    Checking,
    Ready(ClaudeCodeCli, AuthStatus),
    Failed(String),
}

/// The Claude Code CLI: where it is, which version, which claude.ai account it is
/// signed in to, and whether it is the engine the chat runs on. Signing in and out run
/// in the user's own terminal — the login opens a browser — and the app reads no
/// credential of Claude Code's, only what `claude auth status` reports. The stream log
/// switch and "Reveal logs" are here too: the log is this engine's alone.
#[component]
fn ClaudeCodeCard(draft: Signal<Settings>) -> Element {
    use satz_studio_core::llm::claude_code::StreamLogConfig;
    use satz_studio_core::llm::claude_code::log::{MAX_BYTES, MAX_FILES};
    use satz_studio_core::llm::claude_code::session::MAX_MCP_OUTPUT_TOKENS;

    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<AppAction>();
    let probe = use_signal(|| ClaudeCodeProbe::Checking);
    let check = move || {
        let path = draft.peek().claude_code_binary.clone();
        // the signal is Copy: taken by value here, so the closure itself only reads
        let mut probe = probe;
        probe.set(ClaudeCodeProbe::Checking);
        spawn(async move {
            let found = match ClaudeCodeCli::locate(path.as_deref()).await {
                Ok(cli) => match cli.auth_status().await {
                    Ok(status) => ClaudeCodeProbe::Ready(cli, status),
                    Err(e) => ClaudeCodeProbe::Failed(e.to_string()),
                },
                Err(e) => ClaudeCodeProbe::Failed(e.to_string()),
            };
            probe.set(found);
        });
    };
    use_hook(check);
    let found = probe();
    let binary_text = match &found {
        ClaudeCodeProbe::Checking => "locating the Claude Code CLI".to_string(),
        ClaudeCodeProbe::Ready(cli, _) => {
            format!("Claude Code {} at {}", cli.version, cli.path.display())
        }
        ClaudeCodeProbe::Failed(e) => e.clone(),
    };
    let binary_error = matches!(found, ClaudeCodeProbe::Failed(_));
    // "in use" is the SAVED provider: that is what the chat builds its engine from,
    // and the draft may hold a selection nobody has saved yet
    let selected = app.settings().read().provider.clone();
    let offer = match &found {
        ClaudeCodeProbe::Checking => EngineOffer::Checking,
        ClaudeCodeProbe::Ready(_, status) => engine_offer(Some(status), &selected),
        ClaudeCodeProbe::Failed(_) => EngineOffer::Absent,
    };
    let signed_in = offer.signed_in();
    let cli = match &found {
        ClaudeCodeProbe::Ready(cli, _) => Some(cli.clone()),
        _ => None,
    };
    let sign = move |line: String| {
        if let Err(e) = ClaudeCodeCli::open_in_terminal(&line) {
            toast(app, ToastKind::Error, e.to_string());
        }
    };
    // the second door to the provider selector above: the same draft, the same save
    let select = move |_| {
        draft.write().provider = ProviderChoice::ClaudeCode { model: None };
        handle.send(AppAction::SaveSettings(draft.peek().clone()));
    };
    // the directory exists before a session has written into it, so the file manager
    // opens on the place the logs go rather than failing on a path that is not there yet
    let reveal_logs = move |_| {
        let opened = StreamLogConfig::default_dir()
            .map_err(|e| e.to_string())
            .and_then(|dir| {
                std::fs::create_dir_all(&dir)
                    .and_then(|()| open::that(&dir))
                    .map_err(|e| format!("{}: {e}", dir.display()))
            });
        if let Err(e) = opened {
            toast(app, ToastKind::Error, e);
        }
    };
    let output_limit = format!(
        "The app starts Claude Code with MAX_MCP_OUTPUT_TOKENS at {MAX_MCP_OUTPUT_TOKENS}, above satz's largest result — an evidence report for every framework the estate is held to — so no satz result reaches the model cut short."
    );
    let log_bounds = format!(
        "One file per conversation under the app's data directory, the {MAX_FILES} newest kept, each up to {} MiB; a change applies from the next conversation.",
        MAX_BYTES / (1024 * 1024)
    );
    rsx! {
        Card { variant: CardVariant::Outlined, class: "settings__card",
            h2 { class: "settings__heading", Icon { name: "terminal", size: 20 } "Claude Code" }
            TextField {
                label: "Claude Code binary",
                value: draft().claude_code_binary.map(|p| p.display().to_string()).unwrap_or_default(),
                placeholder: "on PATH, then ~/.local/bin/claude",
                monospace: true,
                supporting: binary_text,
                error: binary_error,
                oninput: move |v: String| draft.write().claude_code_binary = if v.trim().is_empty() { None } else { Some(PathBuf::from(v.trim())) },
                onblur: move |_| check(),
            }
            p { class: "settings__status", class: if offer.is_error() { "settings__status--error" },
                Icon { name: offer.icon(), size: 20 }
                span { "{offer.status_line()}" }
                Button { variant: ButtonVariant::Text, onclick: move |_| check(), "Check again" }
            }
            p { class: "settings__label", "This engine runs on the claude.ai subscription the CLI is signed in to; no API key is used and none is stored. Signing in opens a browser from your terminal." }
            p { class: "settings__label", "{output_limit}" }
            Switch { label: "Log every line Claude Code and the app exchange", checked: draft().claude_code_log, onchange: move |v| draft.write().claude_code_log = v }
            p { class: "settings__label", "The log holds the estate's contents, its resource names and everything you type, and never leaves this machine." }
            p { class: "settings__status",
                Icon { name: "folder", size: 20 }
                span { class: "settings__label grow", "{log_bounds}" }
                Button { variant: ButtonVariant::Text, icon: "folder_open", onclick: reveal_logs, "Reveal logs" }
            }
            div { class: "settings__key",
                if matches!(offer, EngineOffer::Ready { .. }) {
                    Button {
                        variant: ButtonVariant::Filled,
                        icon: "swap_horiz",
                        onclick: select,
                        "Use this engine"
                    }
                }
                if let Some(cli) = cli {
                    Button {
                        variant: ButtonVariant::Tonal,
                        icon: "login",
                        disabled: signed_in,
                        onclick: {
                            let line = cli.login_command();
                            move |_| sign(line.clone())
                        },
                        "Sign in"
                    }
                    Button {
                        variant: ButtonVariant::Text,
                        icon: "logout",
                        disabled: !signed_in,
                        onclick: {
                            let line = cli.logout_command();
                            move |_| sign(line.clone())
                        },
                        "Sign out"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(logged_in: bool, email: Option<&str>, method: Option<&str>) -> AuthStatus {
        AuthStatus {
            logged_in,
            auth_method: method.map(str::to_string),
            api_provider: None,
            email: email.map(str::to_string),
        }
    }

    fn subscription() -> AuthStatus {
        status(true, Some("first.admin@example.com"), Some("claude.ai"))
    }

    #[test]
    fn a_signed_in_cli_is_in_use_only_when_claude_code_is_the_selected_engine() {
        let account = "signed in as first.admin@example.com via claude.ai";
        assert_eq!(
            engine_offer(
                Some(&subscription()),
                &ProviderChoice::ClaudeCode { model: None }
            ),
            EngineOffer::InUse {
                account: account.to_string()
            }
        );
        for other in [
            ProviderChoice::Claude,
            ProviderChoice::Ollama {
                base_url: "http://localhost:11434".to_string(),
                model: "qwen3".to_string(),
            },
            ProviderChoice::OpenAiCompat {
                base_url: "http://localhost:1234/v1".to_string(),
                model: "local".to_string(),
            },
        ] {
            assert_eq!(
                engine_offer(Some(&subscription()), &other),
                EngineOffer::Ready {
                    account: account.to_string()
                },
                "{other:?} selected: the signed-in CLI is an offer, not the engine"
            );
        }
    }

    #[test]
    fn the_state_line_says_signed_in_and_whether_that_engine_is_the_one_in_use() {
        let in_use = engine_offer(
            Some(&subscription()),
            &ProviderChoice::ClaudeCode { model: None },
        );
        assert_eq!(
            in_use.status_line(),
            "signed in as first.admin@example.com via claude.ai, and in use"
        );
        assert!(!in_use.is_error());
        let ready = engine_offer(Some(&subscription()), &ProviderChoice::Claude);
        assert_eq!(
            ready.status_line(),
            "signed in as first.admin@example.com via claude.ai, not the selected engine"
        );
        assert!(!ready.is_error());
        assert_eq!(
            ready.account(),
            "signed in as first.admin@example.com via claude.ai"
        );
    }

    #[test]
    fn a_signed_out_cli_an_absent_one_and_one_still_answering_are_three_states() {
        let signed_out = engine_offer(Some(&status(false, None, None)), &ProviderChoice::Claude);
        assert_eq!(signed_out, EngineOffer::SignedOut);
        assert_eq!(signed_out.status_line(), "not signed in");
        assert!(signed_out.is_error());
        assert!(!signed_out.signed_in());

        let absent = engine_offer(None, &ProviderChoice::ClaudeCode { model: None });
        assert_eq!(absent, EngineOffer::Absent);
        assert_eq!(absent.status_line(), "no CLI to ask");
        assert!(absent.is_error());

        // still answering is not a fault, and says nothing about an account
        assert_eq!(EngineOffer::Checking.status_line(), "checking");
        assert!(!EngineOffer::Checking.is_error());
        assert_eq!(EngineOffer::Checking.account(), "");
    }

    #[test]
    fn the_account_line_says_what_auth_status_reported_and_no_more() {
        assert_eq!(
            account_line(&subscription()),
            "signed in as first.admin@example.com via claude.ai"
        );
        assert_eq!(
            account_line(&status(true, Some("first.admin@example.com"), None)),
            "signed in as first.admin@example.com"
        );
        assert_eq!(
            account_line(&status(true, None, Some("claude.ai"))),
            "signed in via claude.ai"
        );
        assert_eq!(account_line(&status(true, None, None)), "signed in");
    }
}
