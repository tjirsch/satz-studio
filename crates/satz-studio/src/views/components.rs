//! The Components destination: what the estate is built from, by what it works in
//! ([ADR 0029](../../../../docs/adr/0029-components-is-one-destination-for-presets-and-packs.md)).
//! An organisation's estate is built from the presets of the library
//! ([`crate::views::map::PresetsView`]); an estate in a delegated folder or project is
//! built from workload packs ([`crate::views::workload_packs::WorkloadPacksPane`]). Both
//! are optional, both have dependencies, and the questions of both are Decisions'.

use dioxus::prelude::*;
use satz_studio_core::satz::reports::EstateMode;

use crate::state::{AppStore, AppStoreStoreExt, EstateStoreStoreExt, View};
use crate::views::map::PresetsView;
use crate::views::path::NextStep;
use crate::views::workload_packs::WorkloadPacksPane;

/// Which kind of component an estate in this mode is built from.
pub fn built_from(mode: EstateMode) -> &'static str {
    match mode {
        EstateMode::Organisation => "presets",
        EstateMode::Folder | EstateMode::Project => "workload packs",
    }
}

#[component]
pub fn ComponentsView() -> Element {
    let app = use_context::<Store<AppStore>>();
    let model = app.estate().model().cloned();
    let Some(model) = model else {
        // the model is not read: the presets view says so, with what the drawer holds
        return rsx! { PresetsView {} };
    };
    let mode = model.packs.works_in;
    if mode == EstateMode::Organisation {
        return rsx! { PresetsView {} };
    }
    let library = !model.packs.packs.is_empty();
    rsx! {
        div { class: "view components",
            h1 { class: "view__title", "Components" }
            p { class: "view__lead",
                "This estate works in a {mode.label().to_lowercase()}, so it is built from {built_from(mode)}: each is one file, fetched from a source or written by this repository. What a pack asks is in Decisions."
            }
            if library {
                p { class: "view__lead",
                    "This estate also holds a preset library. Presets are shown for an organisation's estate; here they are switched in the estate file or from the commands palette."
                }
            }
            WorkloadPacksPane {}
            NextStep { from: View::Components }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_organisation_is_built_from_presets_and_a_delegated_estate_from_packs() {
        assert_eq!(built_from(EstateMode::Organisation), "presets");
        assert_eq!(built_from(EstateMode::Folder), "workload packs");
        assert_eq!(built_from(EstateMode::Project), "workload packs");
    }
}
