//! The Agent destination: an agentic client, configured on the open estate and started
//! on it.
//!
//! satz-studio runs no model
//! ([ADR 0020](../../../../docs/adr/0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md)).
//! The agent's half of the work belongs to a client that speaks the Model Context
//! Protocol — Claude Code, cowork, Claude Desktop — and satz serves one, so what the
//! window does is point a client at the estate on screen.
//!
//! The configuration is satz's: `satz mcp-config <estate> --client <client> --allow
//! <ceiling>` prints the block on stdout and what it cannot say on stderr, and the same
//! command with `--write` merges satz's own key into the file that client reads. The
//! window renders what satz printed and nothing of its own — a refusal included — and
//! beside it what the client's file holds for the estate, because that, not the
//! setting, is the ceiling the agent runs at
//! ([ADR 0021](../../../../docs/adr/0021-the-settings-ceiling-is-the-agents-and-studio-writes-at-its-own.md)).

use dioxus::prelude::*;
use satz_studio_core::agent;
use satz_studio_core::satz::mcp_config::{self, Client, OnDisk, Printed, Run, Written};

use crate::components::{Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, Icon};
use crate::state::{AppStore, AppStoreStoreExt, ToastKind, toast};
use crate::views::commands::copy_to_clipboard;

/// What the last `--write` on one card came to, in satz's own words.
#[derive(Clone, PartialEq)]
struct Said {
    ok: bool,
    /// satz's line about the write, or its refusal, verbatim
    text: String,
    /// the refusal `--force` answers: satz's key is there with other arguments, and
    /// replacing it is the operator's call
    offer_force: bool,
}

#[component]
pub fn AgentView() -> Element {
    let app = use_context::<Store<AppStore>>();
    let settings = app.settings().cloned();
    if app.open().read().is_none() {
        return rsx! {};
    }

    rsx! {
        div { class: "view agent",
            h1 { class: "view__title", "Agent" }
            Card { variant: CardVariant::Filled, class: "agent__lead",
                Icon { name: "smart_toy", size: 22 }
                p {
                    "satz-studio runs no model. It configures an agentic client on this estate and starts it: satz prints the block the client reads and writes it into the client's own file, with the capability ceiling Settings holds. The ceiling bounds the satz server only: an agent with a shell can run satz commands directly. The estate is re-read when this window comes back to the front."
                }
                span { class: "grow" }
                Chip { kind: ChipKind::Assist, icon: "shield", label: "Settings: {settings.mcp_allow}" }
            }
            for client in Client::ALL {
                ClientCard { key: "{client.as_arg()}", client }
            }
        }
    }
}

/// One client's card: the configuration satz prints for it, the button that writes it,
/// and — for the client that is started from here — the one that opens it.
#[component]
fn ClientCard(client: Client) -> Element {
    let app = use_context::<Store<AppStore>>();
    let mut shown = use_signal(|| None::<Result<Printed, String>>);
    let mut said = use_signal(|| None::<Said>);
    // what the client's file holds for this estate, read after every run
    let mut disk = use_signal(|| None::<Result<Written, String>>);

    // satz renders the configuration when the card opens, and again whenever the estate
    // or the ceiling it is derived from changes.
    use_effect(move || {
        let Some(open) = app.open().cloned() else {
            return;
        };
        let allow = app.settings().read().mcp_allow;
        shown.set(None);
        said.set(None);
        disk.set(None);
        spawn(async move {
            let printed = mcp_config::run(&open.session.cli, &open.name, client, allow, Run::Show)
                .await
                .map_err(|e| mcp_config::refusal(&e));
            disk.set(printed.as_ref().ok().map(read_disk));
            shown.set(Some(printed));
        });
    });

    let write = use_callback(move |run: Run| {
        let Some(open) = app.open().cloned() else {
            return;
        };
        let allow = app.settings().read().mcp_allow;
        said.set(None);
        spawn(async move {
            let result = mcp_config::run(&open.session.cli, &open.name, client, allow, run).await;
            if let Some(Ok(printed)) = shown.peek().as_ref() {
                disk.set(Some(read_disk(printed)));
            }
            match result {
                Ok(printed) => {
                    let line = mcp_config::outcome(&printed.stderr);
                    if !line.is_empty() {
                        toast(app, ToastKind::Info, line);
                    }
                    said.set(Some(Said {
                        ok: true,
                        text: printed.stderr.trim_end().to_string(),
                        offer_force: false,
                    }));
                }
                Err(e) => {
                    let text = mcp_config::refusal(&e);
                    said.set(Some(Said {
                        ok: false,
                        offer_force: mcp_config::force_would_answer(&text),
                        text: text.trim_end().to_string(),
                    }));
                }
            }
        });
    });

    // the entry on disk that is not satz's for the setting carries its own "Replace it"
    let differs = matches!(
        disk(),
        Some(Ok(Written {
            on_disk: OnDisk::Differs { .. },
            ..
        }))
    );
    let block = match shown() {
        Some(Ok(printed)) => Some(printed.stdout.clone()),
        _ => None,
    };

    rsx! {
        Card { variant: CardVariant::Outlined, class: "agent__card",
            h2 { class: "agent__heading",
                Icon { name: icon(client), size: 20 }
                "{client.label()}"
            }
            p { class: "agent__text", {blurb(client)} }
            DiskState { disk: disk(), setting: app.settings().read().mcp_allow.as_arg().to_string(), onreplace: move |_| write.call(Run::Replace) }
            match shown() {
                None => rsx! { p { class: "agent__text", "satz is rendering the configuration…" } },
                Some(Ok(printed)) => rsx! {
                    pre { class: "agent__config", "{printed.stdout}" }
                    pre { class: "agent__notes", "{printed.stderr.trim_end()}" }
                },
                Some(Err(e)) => rsx! {
                    div { class: "agent__refusal",
                        pre { "{e}" }
                    }
                },
            }
            div { class: "agent__actions",
                Button {
                    variant: ButtonVariant::Filled,
                    icon: "settings_applications",
                    onclick: move |_| write.call(Run::Write),
                    "Configure {client.label()}"
                }
                Button {
                    variant: ButtonVariant::Tonal,
                    icon: "content_copy",
                    disabled: block.is_none(),
                    onclick: {
                        let block = block.clone();
                        move |_| {
                            if let Some(block) = &block {
                                copy_to_clipboard(app, block);
                            }
                        }
                    },
                    "Copy"
                }
                if client == Client::ClaudeCode {
                    span { class: "grow" }
                    OpenInClient {}
                }
            }
            if client == Client::ClaudeCode {
                p { class: "agent__text", {open_sentence(&app.settings().read().agent_command)} }
            }
            if let Some(said) = said() {
                if said.ok {
                    pre { class: "agent__notes", "{said.text}" }
                } else {
                    div { class: "agent__refusal",
                        pre { "{said.text}" }
                        if said.offer_force && !differs {
                            div { class: "agent__actions",
                                Button {
                                    variant: ButtonVariant::Filled,
                                    icon: "save_as",
                                    onclick: move |_| write.call(Run::Replace),
                                    "Replace it"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// What the client's file holds for the estate, read with the block satz printed: the
/// file and satz's key in it, or the reason it cannot be read.
fn read_disk(printed: &Printed) -> Result<Written, String> {
    mcp_config::written(printed).map_err(|e| e.to_string())
}

/// The configuration on disk beside the setting: the ceiling the client's file holds
/// for this estate — the one the agent runs at — and, where that entry is not the one
/// satz printed for the setting, the run that writes the setting over it.
#[component]
fn DiskState(
    disk: Option<Result<Written, String>>,
    setting: String,
    onreplace: EventHandler<MouseEvent>,
) -> Element {
    let Some(disk) = disk else {
        return rsx! {};
    };
    let written = match disk {
        Ok(written) => written,
        Err(e) => {
            return rsx! {
                div { class: "agent__refusal", pre { "{e}" } }
            };
        }
    };
    let file = written.file.display().to_string();
    let key = written.key.clone();
    match written.on_disk {
        OnDisk::Absent => rsx! {
            div { class: "agent__state",
                Chip { kind: ChipKind::Assist, icon: "draft", label: "not configured" }
                p { class: "agent__text", "{file} holds no server {key} yet." }
            }
        },
        OnDisk::Same { allow } => rsx! {
            div { class: "agent__state",
                Chip { kind: ChipKind::Assist, icon: "shield", label: "configured: {ceiling(&allow)}" }
                p { class: "agent__text", "{file} holds this block under {key}." }
            }
        },
        OnDisk::Differs { allow } => rsx! {
            div { class: "agent__differs",
                div { class: "agent__state",
                    Chip { kind: ChipKind::Assist, icon: "shield", label: "configured: {ceiling(&allow)}" }
                    p { class: "agent__text",
                        "{file} holds another entry under {key}: the agent runs at {ceiling(&allow)}, and Settings holds {setting}. Replace writes the block above over it."
                    }
                }
                div { class: "agent__actions",
                    Button { variant: ButtonVariant::Filled, icon: "save_as", onclick: move |e| onreplace.call(e), "Replace it" }
                }
            }
        },
        OnDisk::Unreadable(reason) => rsx! {
            div { class: "agent__refusal", pre { "{reason}" } }
        },
    }
}

/// The ceiling an entry on disk carries, or that it carries none — which `satz mcp`
/// reads as its own default, `read`.
fn ceiling(allow: &Option<String>) -> String {
    match allow {
        Some(a) => a.clone(),
        None => "no --allow (satz's default, read)".to_string(),
    }
}

/// "Open in <client>": the command Settings names, run in the estate's directory in the
/// OS terminal. A command that is not named disables the button; one that is not
/// installed is an error toast naming it.
#[component]
fn OpenInClient() -> Element {
    let app = use_context::<Store<AppStore>>();
    let command = app.settings().read().agent_command.clone();
    let program = agent::program(&command);
    let Some(open) = app.open().cloned() else {
        return rsx! {};
    };
    let dir = open.dir.clone();

    rsx! {
        Button {
            variant: ButtonVariant::Tonal,
            icon: "play_arrow",
            disabled: program.is_none(),
            onclick: {
                let command = command.clone();
                let dir = dir.clone();
                move |_| match agent::start(&command, &dir) {
                    Ok(_) => toast(
                        app,
                        ToastKind::Info,
                        format!("{command} started in {}", dir.display()),
                    ),
                    Err(e) => toast(app, ToastKind::Error, e.to_string()),
                }
            },
            match &program {
                Some(p) => format!("Open in {p}"),
                None => "No agent configured".to_string(),
            }
        }
    }
}

/// The line under the Claude Code card's actions: what "Open in <client>" will run, or
/// that Settings name no client to start.
fn open_sentence(command: &str) -> String {
    match agent::program(command) {
        Some(p) => format!(
            "\"Open in {p}\" runs that command in this estate's directory, in your terminal. Settings names the client."
        ),
        None => {
            "Settings names the agentic client to start; until one is named, there is nothing to open.".to_string()
        }
    }
}

fn icon(client: Client) -> &'static str {
    match client {
        Client::ClaudeCode => "terminal",
        Client::ClaudeDesktop => "desktop_windows",
    }
}

/// What the client reads, and what writing it does to what is already there.
fn blurb(client: Client) -> &'static str {
    match client {
        Client::ClaudeCode => {
            "Claude Code reads .mcp.json in the directory it starts in. Written into the estate's directory, it gives every session in that folder this estate's satz tools and nothing else."
        }
        Client::ClaudeDesktop => {
            "Claude Desktop keeps every server on the machine in one configuration file, and reads it when it starts. satz writes its own key into that file and leaves every other server in it as it is."
        }
    }
}
