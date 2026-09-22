//! The Agent destination: an agent, set up on the open estate and started on it.
//!
//! satz-studio runs no model
//! ([ADR 0020](../../../../docs/adr/0020-the-agent-is-an-external-client-that-studio-configures-and-starts.md)).
//! The agent's half of the work belongs to a client that speaks the Model Context
//! Protocol — Claude Code, cowork, Claude Desktop — and satz already serves one, so what
//! the window does is point one at the estate on screen: the configuration in the two
//! shapes a client reads, written or copied, and the client started in the estate's
//! directory.
//!
//! Everything shown is derived from the open estate and the settings: the satz binary
//! the app located, the root that estate's `satz mcp` is confined to, and the capability
//! ceiling. Nothing is remembered between sessions.

use std::path::PathBuf;

use dioxus::prelude::*;
use satz_studio_core::handoff::{self, Handoff, HandoffError};

use crate::components::{Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, Icon};
use crate::state::{AppStore, AppStoreStoreExt, ToastKind, View, toast};
use crate::views::commands::copy_to_clipboard;

#[component]
pub fn AgentView() -> Element {
    let app = use_context::<Store<AppStore>>();
    let open = app.open().cloned();
    let satz = app.satz().read().binary().cloned();
    let settings = app.settings().cloned();
    // the file the last "Write .mcp.json" refused to replace; "Replace it" is the only
    // way past it, and it is forgotten as soon as either button is pressed
    let mut clash = use_signal(|| Option::<PathBuf>::None);

    let Some(open) = open else {
        return rsx! {};
    };
    let Some(satz) = satz else {
        return rsx! {
            div { class: "view agent",
                h1 { class: "view__title", "Agent" }
                Card { variant: CardVariant::Outlined, class: "agent__card",
                    p { "An agent is pointed at this estate by the satz it runs. No satz is located, so there is no configuration to write; the banner says what to do about it." }
                }
            }
        };
    };

    let dir = open.session.dir.dir.clone();
    let handoff = Handoff::new(
        &satz.path,
        &open.session.root,
        &open.name,
        settings.mcp_allow,
    );
    let project = handoff.project_file();
    let desktop = handoff.desktop_block();
    let command = settings.agent_command.clone();
    let program = handoff::program(&command);

    let write = use_callback({
        let handoff = handoff.clone();
        let dir = dir.clone();
        move |overwrite: bool| {
            clash.set(None);
            match handoff.write_project_file(&dir, overwrite) {
                Ok(written) => toast(app, ToastKind::Info, written.message()),
                Err(e) => {
                    if let HandoffError::Exists { path } = &e {
                        clash.set(Some(path.clone()));
                    }
                    toast(app, ToastKind::Error, e.to_string());
                }
            }
        }
    });
    let start = {
        let command = command.clone();
        let dir = dir.clone();
        move |_| match handoff::start(&command, &dir) {
            Ok(_) => toast(
                app,
                ToastKind::Info,
                format!("{command} started in {}", dir.display()),
            ),
            Err(e) => toast(app, ToastKind::Error, e.to_string()),
        }
    };

    rsx! {
        div { class: "view agent",
            h1 { class: "view__title", "Agent" }
            Card { variant: CardVariant::Filled, class: "agent__lead",
                Icon { name: "smart_toy", size: 22 }
                p {
                    "satz-studio runs no model. It sets an agent up on this estate and starts it: configure your agentic client and open it from here once the mechanics are done, or configure the satz MCP server and work from the client from the start. The estate is re-read when this window comes back to the front."
                }
            }
            Card { variant: CardVariant::Filled, class: "agent__state",
                Icon { name: "lan", size: 22 }
                code { class: "agent__line", "{handoff.command_line()}" }
                span { class: "grow" }
                Chip { kind: ChipKind::Assist, icon: "shield", label: "ceiling: {settings.mcp_allow}" }
            }
            Card { variant: CardVariant::Outlined, class: "agent__card",
                h2 { class: "agent__heading", Icon { name: "terminal", size: 20 } "A client started here" }
                p { class: "agent__text",
                    "Claude Code reads "
                    code { "{handoff::PROJECT_FILE}" }
                    " in the directory it starts in. Written into the estate's directory, it gives every session in that folder this estate's satz tools and nothing else."
                }
                pre { class: "agent__config", "{project}" }
                div { class: "agent__actions",
                    Button {
                        variant: ButtonVariant::Filled,
                        icon: "note_add",
                        onclick: move |_| write.call(false),
                        "Write {handoff::PROJECT_FILE}"
                    }
                    Button {
                        variant: ButtonVariant::Tonal,
                        icon: "content_copy",
                        onclick: {
                            let project = project.clone();
                            move |_| copy_to_clipboard(app, &project)
                        },
                        "Copy"
                    }
                    span { class: "grow" }
                    Button {
                        variant: ButtonVariant::Tonal,
                        icon: "play_arrow",
                        disabled: program.is_none(),
                        onclick: start,
                        match &program {
                            Some(p) => format!("Open in {p}"),
                            None => "No agent configured".to_string(),
                        }
                    }
                }
                if let Some(path) = clash() {
                    div { class: "agent__clash",
                        Icon { name: "warning", size: 20 }
                        span { class: "grow", "{path.display()} holds a different configuration. Replace it only if nothing else needs what is in it." }
                        Button {
                            variant: ButtonVariant::Text,
                            onclick: move |_| clash.set(None),
                            "Keep it"
                        }
                        Button {
                            variant: ButtonVariant::Filled,
                            icon: "save_as",
                            onclick: move |_| write.call(true),
                            "Replace it"
                        }
                    }
                }
                p { class: "agent__text",
                    match &program {
                        Some(p) => format!("\"Open in {p}\" runs that command in this estate's directory, in your terminal. Settings names the client."),
                        None => "Settings names the agentic client to start; until one is named, there is nothing to open.".to_string(),
                    }
                    Button {
                        variant: ButtonVariant::Text,
                        icon: "settings",
                        onclick: move |_| app.nav().set(View::Settings),
                        "Settings"
                    }
                }
            }
            Card { variant: CardVariant::Outlined, class: "agent__card",
                h2 { class: "agent__heading", Icon { name: "desktop_windows", size: 20 } "A client configured once" }
                p { class: "agent__text",
                    "Claude Desktop keeps every server in one configuration file, which it opens from Settings → Developer → Edit Config. Paste this under its "
                    code { "mcpServers" }
                    " and restart it; the server is named after the estate, so several estates stand beside each other."
                }
                pre { class: "agent__config", "{desktop}" }
                div { class: "agent__actions",
                    Button {
                        variant: ButtonVariant::Filled,
                        icon: "content_copy",
                        onclick: {
                            let desktop = desktop.clone();
                            move |_| copy_to_clipboard(app, &desktop)
                        },
                        "Copy MCP config"
                    }
                }
            }
        }
    }
}
