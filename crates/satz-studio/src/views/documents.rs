//! The Documents destination: what the estate hands to people as a file. Today that is
//! one document, the decisions sheet ([`crate::views::export`]); the view is the place a
//! further document gets its card
//! ([ADR 0027](../../../../docs/adr/0027-documents-is-a-destination-of-its-own.md)).

use dioxus::prelude::*;

use crate::state::{AppStore, AppStoreStoreExt};
use crate::views::export::ExportCard;

#[component]
pub fn DocumentsView() -> Element {
    let app = use_context::<Store<AppStore>>();
    if app.open().cloned().is_none() {
        return rsx! {};
    }
    rsx! {
        div { class: "view documents",
            h1 { class: "view__title", "Documents" }
            p { class: "view__lead",
                "What this estate hands to people as a file. Each document is derived from the estate as it is now: export it again after a change."
            }
            ExportCard {}
        }
    }
}
