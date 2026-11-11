//! The Estate destination: the main file as it stands — the params it binds, the
//! resources it declares, the interfaces it publishes to the projects that read it, and
//! the workload packs it keeps: four tabs over one estate.
//!
//! Decisions and this are not two views of one thing: Decisions is the worklist of what
//! the estate has NOT decided, and this is what it currently says. A param that answers
//! a question is editable in both, and that is the point — the question is where the
//! choice is explained, the file is where it lives.

use dioxus::prelude::*;

use crate::components::{Tab, Tabs};
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
                Tab {
                    label: format!("Params ({params})"),
                    icon: "tune",
                    selected: pane() == Pane::Params,
                    onclick: move |_| pane.set(Pane::Params),
                }
                Tab {
                    label: format!("Resources ({resources})"),
                    icon: "account_tree",
                    selected: pane() == Pane::Resources,
                    onclick: move |_| pane.set(Pane::Resources),
                }
                Tab {
                    label: format!("Interfaces ({interfaces})"),
                    icon: "hub",
                    selected: pane() == Pane::Interfaces,
                    onclick: move |_| pane.set(Pane::Interfaces),
                }
                Tab {
                    label: format!("Workload packs ({packs})"),
                    icon: "inventory",
                    selected: pane() == Pane::WorkloadPacks,
                    onclick: move |_| pane.set(Pane::WorkloadPacks),
                }
            }
            match pane() {
                Pane::Params => rsx! { ParamsPane {} },
                Pane::Resources => rsx! { ResourcesPane {} },
                Pane::Interfaces => rsx! { InterfacesPane {} },
                Pane::WorkloadPacks => rsx! { WorkloadPacksPane {} },
            }
        }
    }
}
