//! The Deploy destination: what hands the estate off. `transpile` writes the HCL,
//! `hcl-init` prepares the directory, `plan` reads it back — those run in the app. The
//! three that change a live organisation run in your terminal: `apply` because the
//! tool's approval prompt is the safety step, `bootstrap` because it creates day 0 as
//! you, and `migrate` because it rewrites the estate and copies the state to the other
//! backend without asking
//! ([ADR 0006](../../../../docs/adr/0006-apply-and-bootstrap-run-in-the-users-terminal.md),
//! [ADR 0012](../../../../docs/adr/0012-migrate-hands-off-to-the-terminal.md)).

use dioxus::prelude::*;

use crate::components::{Card, CardVariant, Chip, ChipKind, Icon};
use crate::state::{AppStore, AppStoreStoreExt, EstateStoreStoreExt};
use crate::views::commands::{CommandDeck, DEPLOY};

#[component]
pub fn DeployView() -> Element {
    let app = use_context::<Store<AppStore>>();
    let open = app.open().cloned();
    let hcl = app.estate().hcl().cloned();
    let Some(open) = open else {
        return rsx! {};
    };
    let dir = open.session.dir.hcl_dir();

    rsx! {
        div { class: "view deploy",
            h1 { class: "view__title", "Deploy" }
            Card { variant: CardVariant::Filled, class: "deploy__state",
                Icon { name: "folder_special", size: 22 }
                code { class: "deploy__dir", "{dir.display()}" }
                span { class: "grow" }
                Chip {
                    kind: ChipKind::Assist,
                    icon: if hcl.transpiled { "check_circle" } else { "pending" },
                    label: if hcl.transpiled { "main.tf written" } else { "nothing emitted" },
                }
                Chip {
                    kind: ChipKind::Assist,
                    icon: if hcl.initialised { "check_circle" } else { "pending" },
                    label: if hcl.initialised { "initialised" } else { "not initialised" },
                }
            }
            CommandDeck { ids: DEPLOY.to_vec() }
        }
    }
}
