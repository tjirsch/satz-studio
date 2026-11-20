//! The Documents destination: what the estate hands to people as a file. Three kinds
//! stand here, in the order they are made: the decisions sheet, which satz writes from
//! the estate ([`crate::views::export`]); the compliance reports, which satz writes from
//! the estate and the live organization ([`COMPLIANCE`]); and the documents an agent
//! writes with a skill — the app runs no model, so those are started in the configured
//! client and made there
//! ([ADR 0027](../../../../docs/adr/0027-documents-is-a-destination-of-its-own.md),
//! [ADR 0028](../../../../docs/adr/0028-apply-is-one-destination-and-starts-with-who-is-signed-in.md)).

use dioxus::prelude::*;
use satz_studio_core::agent;

use dioxus_icons::lucide::{FileText, Play, ShieldCheck};

use crate::components::button::{Button, ButtonVariant};
use crate::components::card::{Card, CardContent, CardDescription, CardHeader};
use crate::state::{AppStore, AppStoreStoreExt, EstateStoreStoreExt, ToastKind, toast};
use crate::views::commands::{COMPLIANCE, CommandDeck};
use crate::views::export::ExportCard;

/// A document an agent writes with one of satz's skills (`skills/` in the satz
/// repository).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentDocument {
    /// the skill's name, which is also the command that starts it in the client
    pub skill: &'static str,
    pub title: &'static str,
    pub text: &'static str,
}

pub const AGENT_DOCUMENTS: &[AgentDocument] = &[
    AgentDocument {
        skill: "estate-documentation",
        title: "Estate documentation",
        text: "The short documentation of the organization as Word documents, written from the estate: its main settings, the infrastructure project, and what is applied.",
    },
    AgentDocument {
        skill: "security-review",
        title: "Security review",
        text: "A review against the CIS Google Cloud Foundations benchmark with live evidence from the organization: the checklist of every control with its status, and the remediation plan.",
    },
];

impl AgentDocument {
    /// The Lucide icon of the document, by its skill.
    fn icon(&self, size: u32) -> Element {
        match self.skill {
            "security-review" => rsx! { ShieldCheck { size } },
            _ => rsx! { FileText { size } },
        }
    }
}

/// The command line that starts the configured client on a skill: the client's own
/// command with the skill as its opening prompt, `claude "/estate-documentation"`.
/// `None` when Settings names no client.
pub fn skill_command(agent_command: &str, skill: &str) -> Option<String> {
    agent::program(agent_command)?;
    Some(format!("{} \"/{skill}\"", agent_command.trim()))
}

#[component]
pub fn DocumentsView() -> Element {
    let app = use_context::<Store<AppStore>>();
    let Some(open) = app.open().cloned() else {
        return rsx! {};
    };
    // the catalogs are part of the preset library, `<presets_dir>/catalogs/`
    let catalogs = app
        .estate()
        .model()
        .read()
        .as_deref()
        .is_none_or(|m| m.packs.has_presets());
    let agent_command = app.settings().read().agent_command.clone();
    let program = agent::program(&agent_command);

    rsx! {
        div { class: "view documents",
            h1 { class: "view__title", "Documents" }
            p { class: "view__lead",
                "What this estate hands to people as a file. Each document is derived from the estate as it is now: make it again after a change."
            }
            ExportCard {}

            h2 { class: "documents__section", "Compliance" }
            if catalogs {
                p { class: "view__lead",
                    "The estate against a catalog, and the evidence report. Both read the live organization, so they need you signed in (Apply says whether you are)."
                }
                CommandDeck { ids: COMPLIANCE.to_vec() }
            } else {
                p { class: "view__lead",
                    "The compliance reports are made from the catalogs of the preset library, which an organization's estate holds. This estate has none: its compliance is reported in the organization's estate."
                }
            }

            h2 { class: "documents__section", "Written by an agent" }
            p { class: "view__lead",
                "These take a model working through the estate with you, so satz-studio does not make them: it starts the client Settings names in this estate's directory with the skill as its first instruction. The skills are in the satz repository under "
                code { "skills/" }
                " and have to be installed for that client."
            }
            div { class: "documents__agent",
                for doc in AGENT_DOCUMENTS {
                    Card { key: "{doc.skill}", class: "export",
                        CardHeader {
                            header { class: "export__head",
                                {doc.icon(22)}
                                h3 { class: "export__title", "{doc.title}" }
                            }
                            CardDescription { "{doc.text}" }
                        }
                        CardContent {
                        div { class: "export__actions",
                            Button {
                                variant: ButtonVariant::Secondary,
                                disabled: program.is_none(),
                                onclick: {
                                    let dir = open.dir.clone();
                                    let command = skill_command(&agent_command, doc.skill);
                                    move |_| {
                                        let Some(command) = &command else { return };
                                        match agent::start(command, &dir) {
                                            Ok(_) => toast(app, ToastKind::Info, format!("{command} started in {}", dir.display())),
                                            Err(e) => toast(app, ToastKind::Error, e.to_string()),
                                        }
                                    }
                                },
                                Play { size: 16 }
                                match &program {
                                    Some(p) => format!("Start in {p}"),
                                    None => "No agent configured".to_string(),
                                }
                            }
                            code { class: "export__last", "/{doc.skill}" }
                        }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The client is started on the skill by its own command line; no client, no command.
    #[test]
    fn a_skill_is_started_by_the_clients_own_command() {
        assert_eq!(
            skill_command("claude", "estate-documentation").as_deref(),
            Some("claude \"/estate-documentation\"")
        );
        assert_eq!(
            skill_command("  claude --model x ", "security-review").as_deref(),
            Some("claude --model x \"/security-review\"")
        );
        assert_eq!(skill_command("   ", "security-review"), None);
    }

    #[test]
    fn every_agent_document_names_its_skill_once() {
        let mut skills: Vec<&str> = AGENT_DOCUMENTS.iter().map(|d| d.skill).collect();
        skills.sort_unstable();
        skills.dedup();
        assert_eq!(skills.len(), AGENT_DOCUMENTS.len());
    }
}
