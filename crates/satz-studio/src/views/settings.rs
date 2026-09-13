//! The Settings view: every field of `Settings` as a form, saved as one file; the
//! detected satz beside its path; the credential's source and the keychain key.

use std::path::PathBuf;

use dioxus::prelude::*;
use satz_studio_core::llm::{CredentialSource, Effort};
use satz_studio_core::satz::Allow;
use satz_studio_core::settings::{ProviderChoice, Settings, Theme, settings_path};

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Icon, Segment, SegmentedButton, Switch, TextField,
};
use crate::state::{
    AppAction, AppStore, AppStoreStoreExt, CredentialStatus, SatzStatus, ToastKind, toast,
};

#[component]
pub fn SettingsView() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<AppAction>();
    let saved = app.settings().cloned();
    let mut draft = use_signal(|| saved.clone());
    let dirty = draft() != saved;
    let satz = app.satz().cloned();
    let satz_text = match &satz {
        SatzStatus::Unknown => "locating satz".to_string(),
        SatzStatus::Located(bin) => format!("satz {} at {}", bin.version, bin.path.display()),
        SatzStatus::TooOld { found, required } => {
            format!("satz {found} found; {required} or newer is needed")
        }
        SatzStatus::Missing(why) => why.clone(),
    };
    let satz_error = matches!(satz, SatzStatus::TooOld { .. } | SatzStatus::Missing(_));
    let file = settings_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| e.to_string());

    let allow_value = draft().mcp_allow.as_arg().to_string();
    let provider_value = match draft().provider {
        ProviderChoice::Claude => "claude",
        ProviderChoice::OpenAiCompat { .. } => "openai_compat",
        ProviderChoice::Ollama { .. } => "ollama",
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
        ProviderChoice::Claude => None,
        ProviderChoice::OpenAiCompat { base_url, model }
        | ProviderChoice::Ollama { base_url, model } => Some((base_url.clone(), model.clone())),
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
                        options: vec![Segment::new("claude", "Claude"), Segment::new("openai_compat", "OpenAI-compatible"), Segment::new("ollama", "Ollama")],
                        selected: provider_value,
                        onselect: move |v: String| {
                            let current = draft().provider;
                            let (base_url, model) = match current {
                                ProviderChoice::Claude => (String::new(), String::new()),
                                ProviderChoice::OpenAiCompat { base_url, model } | ProviderChoice::Ollama { base_url, model } => (base_url, model),
                            };
                            draft.write().provider = match v.as_str() {
                                "openai_compat" => ProviderChoice::OpenAiCompat { base_url: if base_url.is_empty() { "http://localhost:1234/v1".to_string() } else { base_url }, model },
                                "ollama" => ProviderChoice::Ollama { base_url: if base_url.is_empty() { "http://localhost:11434".to_string() } else { base_url }, model },
                                _ => ProviderChoice::Claude,
                            };
                        },
                    }
                    if let Some((base_url, model)) = endpoint {
                        TextField { label: "Base URL", value: base_url, monospace: true, oninput: move |v: String| set_endpoint(&mut draft, Some(v), None) }
                        TextField { label: "Model", value: model, monospace: true, supporting: "the model name the endpoint serves", oninput: move |v: String| set_endpoint(&mut draft, None, Some(v)) }
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
        ProviderChoice::Claude => {}
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
    rsx! {
        Card { variant: CardVariant::Outlined, class: "settings__card",
            h2 { class: "settings__heading", Icon { name: "vpn_key", size: 20 } "Claude credential" }
            p { class: "settings__status", class: if error { "settings__status--error" },
                Icon { name: icon, size: 20 }
                span { "{text}" }
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
