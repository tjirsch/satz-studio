//! The Settings view: every field of `Settings` as a form, saved as one file; the
//! detected satz beside its path; and the agentic client the Agent destination starts.
//!
//! There is no credential here and no engine to choose: satz-studio runs no model
//! ([ADR 0020](../../../../docs/adr/0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md)).

use std::path::PathBuf;

use dioxus::prelude::*;
use satz_studio_core::agent::{self, AgentError, DEFAULT_AGENT_COMMAND};
use satz_studio_core::satz::Allow;
use satz_studio_core::settings::{Theme, settings_path};

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Icon, Segment, SegmentedButton, TextField,
};
use crate::state::{
    AppAction, AppStore, AppStoreStoreExt, SatzStatus, ToastKind, UpdateStoreStoreExt,
    ahead_sentence, toast,
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

    // the command is looked up when it is used, not held: a client installed while the
    // window is open is found by the next press. What the field says is what is there now
    let (agent_text, agent_error) = agent_status(&draft().agent_command);

    let allow_value = draft().mcp_allow.as_arg().to_string();
    let theme_value = match draft().theme {
        Theme::System => "system",
        Theme::Light => "light",
        Theme::Dark => "dark",
    }
    .to_string();

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
                    p { class: "settings__label", "It is the ceiling of the app's own satz mcp child AND of the one the Agent destination writes into an agent's configuration: lowering it here lowers both." }
                }
                Card { variant: CardVariant::Outlined, class: "settings__card",
                    h2 { class: "settings__heading", Icon { name: "smart_toy", size: 20 } "Agent" }
                    TextField {
                        label: "Agent command",
                        value: draft().agent_command,
                        placeholder: DEFAULT_AGENT_COMMAND,
                        monospace: true,
                        supporting: agent_text,
                        error: agent_error,
                        oninput: move |v: String| draft.write().agent_command = v,
                    }
                    p { class: "settings__label", "The agentic client the Agent destination starts in the estate's directory, as a command line. satz-studio runs no model of its own: the agent is that client, driven by the satz MCP server the Agent destination configures for the open estate." }
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

/// The line under the agent command: where that client is, or why there is none. A
/// command that is not installed is a fault — "Open in …" would have nothing to run.
fn agent_status(command: &str) -> (String, bool) {
    match agent::locate(command) {
        Ok(path) => (path.display().to_string(), false),
        Err(AgentError::NoCommand) => (
            "no client named; the Agent destination has nothing to start".to_string(),
            true,
        ),
        Err(e) => (e.to_string(), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_that_is_installed_is_its_path_and_one_that_is_not_is_a_fault() {
        let (text, error) = agent_status("cargo");
        assert!(!error, "{text}");
        assert!(text.contains("cargo"));

        let (text, error) = agent_status("satz-studio-no-such-agent --here");
        assert!(error);
        assert!(text.contains("satz-studio-no-such-agent"));
    }

    #[test]
    fn a_field_with_nothing_in_it_says_there_is_no_client_to_start() {
        for empty in ["", "   "] {
            let (text, error) = agent_status(empty);
            assert!(error);
            assert_eq!(
                text,
                "no client named; the Agent destination has nothing to start"
            );
        }
    }
}
