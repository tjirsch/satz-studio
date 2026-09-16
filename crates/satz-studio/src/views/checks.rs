//! The Checks destination: what judges the estate. The compile itself
//! (`transpile --check`), what its resource types oblige it to declare
//! (`update-prerequisites`), the goal view against a catalog (`require`), the evidence
//! report (`report-compliance`), and the read-only day-0 check (`bootstrap --dry-run`).
//!
//! `update-prerequisites` is the one entry here that can WRITE. The palette runs it
//! `--report-only`, because a command cannot rewrite the estate under the window; the
//! writing run is offered as its own button, which goes through satz's own writer under
//! the estate's write lock and is followed by a reload, like every other write.

use dioxus::prelude::*;

use crate::components::{Button, ButtonVariant, Card, CardVariant, Icon};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt};
use crate::views::commands::{CHECKS, CommandDeck};

#[component]
pub fn ChecksView() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let diagnostics = app.estate().diagnostics().cloned();
    let loading = app.estate().loading().cloned();
    let prerequisites: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.kind.as_deref() == Some("prerequisites"))
        .map(|d| d.message.clone())
        .collect();

    rsx! {
        div { class: "view checks",
            h1 { class: "view__title", "Checks" }
            p { class: "view__lead", "Every reload compiles the estate in memory and puts what it found in the drawer. These are the checks that go further — and the two of them that call Google." }
            if !prerequisites.is_empty() {
                Card { variant: CardVariant::Outlined, class: "checks__prerequisites",
                    header { class: "checks__head",
                        Icon { name: "admin_panel_settings", size: 22 }
                        h2 { class: "checks__title", "The prerequisites are not declared" }
                    }
                    for (i, message) in prerequisites.iter().enumerate() {
                        p { key: "{i}", class: "checks__finding", "{message}" }
                    }
                    p { class: "checks__note", "Writing them binds the roles on the estate's IaC service account and the APIs on its infrastructure project, in the estate file. satz works both out offline, from the resource types this estate emits; the write goes through the same check and rollback as an answer." }
                    div { class: "checks__actions",
                        Button {
                            variant: ButtonVariant::Filled,
                            icon: "edit_note",
                            disabled: loading,
                            onclick: move |_| handle.send(EstateAction::WritePrerequisites),
                            "Write them into the estate"
                        }
                    }
                }
            }
            CommandDeck { ids: CHECKS.to_vec(), tools: true }
        }
    }
}
