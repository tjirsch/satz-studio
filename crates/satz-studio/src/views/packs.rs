//! The Packs destination: the estate's workload packs. A pack is a template — one file,
//! taken once from a source and then the repository's own to adapt — where a preset is
//! part of the one library every organization keeps current
//! ([ADR 0029](../../../../docs/adr/0029-presets-and-packs-are-two-destinations.md)). An
//! organization's estate has both destinations; an estate in a delegated folder or project
//! has this one alone. What a pack asks is Decisions'.

use dioxus::prelude::*;

use crate::state::View;
use crate::views::path::NextStep;
use crate::views::workload_packs::WorkloadPacksPane;

#[component]
pub fn PacksView() -> Element {
    rsx! {
        div { class: "view packs-view",
            h1 { class: "view__title", "Packs" }
            p { class: "view__lead",
                "A pack is a template for a workload or a part of one: take it from a source, add it to the estate, and adapt it — a pack changed here is this repository's own. What a pack asks is in Decisions."
            }
            WorkloadPacksPane {}
            NextStep { from: View::Packs }
        }
    }
}
