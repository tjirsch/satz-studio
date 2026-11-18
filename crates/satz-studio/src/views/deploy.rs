//! The Deploy destination: what hands the estate off. `transpile` writes the HCL,
//! `hcl-init` prepares the directory, `plan` reads it back — those run in the app. The
//! three that change a live organisation run in your terminal: `apply` because the
//! tool's approval prompt is the safety step, `bootstrap` because it creates day 0 as
//! you, and `migrate` because it rewrites the estate and copies the state to the other
//! backend without asking
//! ([ADR 0006](../../../../docs/adr/0006-apply-and-bootstrap-run-in-the-users-terminal.md),
//! [ADR 0012](../../../../docs/adr/0012-migrate-hands-off-to-the-terminal.md)).

use dioxus::prelude::*;

use dioxus_icons::lucide::{CircleCheck, CircleEllipsis, FolderHeart};

use crate::components::badge::{Badge, BadgeVariant};
use crate::components::card::{Card, CardContent};
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
            Card { class: "deploy__state",
                CardContent {
                    FolderHeart { size: 22 }
                    code { class: "deploy__dir", "{dir.display()}" }
                    span { class: "grow" }
                    Badge { variant: BadgeVariant::Outline,
                        if hcl.transpiled {
                            CircleCheck { size: 12 }
                            "main.tf written"
                        } else {
                            CircleEllipsis { size: 12 }
                            "nothing emitted"
                        }
                    }
                    Badge { variant: BadgeVariant::Outline,
                        if hcl.initialised {
                            CircleCheck { size: 12 }
                            "initialised"
                        } else {
                            CircleEllipsis { size: 12 }
                            "not initialised"
                        }
                    }
                }
            }
            CommandDeck { ids: DEPLOY.to_vec() }
        }
    }
}
