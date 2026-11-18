//! The Estate destination: the main file as it stands — the params it binds, the
//! resources it declares, the interfaces it publishes to the projects that read it, and
//! the workload packs it keeps: four tabs over one estate.
//!
//! Decisions and this are not two views of one thing: Decisions is the worklist of what
//! the estate has NOT decided, and this is what it currently says. A param that answers
//! a question is editable in both, and that is the point — the question is where the
//! choice is explained, the file is where it lives.

use dioxus::prelude::*;

use dioxus_icons::lucide::{ClipboardList, Network, SlidersHorizontal, Waypoints};

use crate::components::tabs::{TabList, TabTrigger, Tabs};
use crate::state::{AppStore, AppStoreStoreExt, DiagnosticSelection, EstateStoreStoreExt};
use crate::views::interfaces::InterfacesPane;
use crate::views::params::ParamsPane;
use crate::views::resources::ResourcesPane;
use crate::views::workload_packs::WorkloadPacksPane;
use satz_studio_core::satz::reports::PackState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pane {
    Params,
    Resources,
    Interfaces,
    WorkloadPacks,
}

impl Pane {
    const ALL: [Pane; 4] = [
        Pane::Params,
        Pane::Resources,
        Pane::Interfaces,
        Pane::WorkloadPacks,
    ];

    /// The value the pane's tab carries.
    fn key(self) -> &'static str {
        match self {
            Pane::Params => "params",
            Pane::Resources => "resources",
            Pane::Interfaces => "interfaces",
            Pane::WorkloadPacks => "workload-packs",
        }
    }
}

#[component]
pub fn EstateView() -> Element {
    let app = use_context::<Store<AppStore>>();
    let selection = use_context::<DiagnosticSelection>();
    let open = app.open().cloned();
    let mut pane = use_signal(|| Pane::Params);

    // A diagnostic names a line, and a line is a node: the drawer's selection belongs
    // to the tree, so choosing one opens the tab that can show it.
    use_effect(move || {
        if (selection.0)().is_some() {
            pane.set(Pane::Resources);
        }
    });

    let Some(open) = open else {
        return rsx! {};
    };
    let model = app.estate().model().cloned();
    // An organisation's components are the preset library's packs, in Presets; workload
    // packs are for an estate in a delegated folder or project.
    let workload_packs = model.as_deref().is_some_and(|m| {
        m.packs.works_in != satz_studio_core::satz::reports::EstateMode::Organisation
    });
    let (params, resources) = model
        .as_deref()
        .map(|m| (m.params.len(), m.outline.len()))
        .unwrap_or((0, 0));
    // the count is the interfaces the estate declares; the core, which every one of
    // them carries, is not one
    let interfaces = match &*app.estate().interfaces().read() {
        Some(Ok(report)) => report.interfaces.len().to_string(),
        Some(Err(_)) => "!".to_string(),
        None => "…".to_string(),
    };

    // the packs that are here; what the source only offers is not the estate's
    let packs = match &*app.estate().workload_packs().read() {
        Some(Ok(report)) => report
            .packs
            .iter()
            .filter(|p| p.state != PackState::Available)
            .count()
            .to_string(),
        Some(Err(_)) => "!".to_string(),
        None => "…".to_string(),
    };

    rsx! {
        div { class: "view estate",
            header { class: "estate__head",
                h1 { class: "view__title", "Estate" }
                code { class: "estate__file", "{open.name}" }
            }
            Tabs {
                value: Some(pane().key().to_string()),
                on_value_change: move |key: String| {
                    if let Some(chosen) = Pane::ALL.into_iter().find(|p| p.key() == key) {
                        pane.set(chosen);
                    }
                },
                horizontal: true,
                TabList {
                    TabTrigger { value: Pane::Params.key(), index: 0usize,
                        SlidersHorizontal { size: 16 }
                        "Params ({params})"
                    }
                    TabTrigger { value: Pane::Resources.key(), index: 1usize,
                        Network { size: 16 }
                        "Resources ({resources})"
                    }
                    TabTrigger { value: Pane::Interfaces.key(), index: 2usize,
                        Waypoints { size: 16 }
                        "Interfaces ({interfaces})"
                    }
                    if workload_packs {
                        TabTrigger { value: Pane::WorkloadPacks.key(), index: 3usize,
                            ClipboardList { size: 16 }
                            "Workload packs ({packs})"
                        }
                    }
                }
            }
            match pane() {
                Pane::Params => rsx! { ParamsPane {} },
                Pane::Resources => rsx! { ResourcesPane {} },
                Pane::Interfaces => rsx! { InterfacesPane {} },
                Pane::WorkloadPacks if workload_packs => rsx! { WorkloadPacksPane {} },
                // the tab is not this estate's: an organisation's estate that stood on it
                Pane::WorkloadPacks => rsx! { ParamsPane {} },
            }
        }
    }
}
