use dioxus::prelude::*;
use satz_studio_core::model::SchemaStatus;

use crate::components::{Badge, Chip, ChipKind, CircularProgress, IconButton, TopAppBar};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, SatzStatus};

/// The top bar: the open estate's name and directory, its identity, deployment mode
/// and schema as chips, the satz version, reload and close, and the drawer toggle.
#[component]
pub fn TopBar() -> Element {
    let app = use_context::<Store<AppStore>>();
    let estate_handle = try_use_context::<Coroutine<EstateAction>>();
    let open = app.open().cloned();
    let satz = app.satz().cloned();
    let schema = app
        .estate()
        .model()
        .read()
        .as_ref()
        .map(|m| m.schema.clone());
    let loading = app.estate().loading().cloned();
    let diagnostics = app.estate().diagnostics().len();
    let drawer_open = app.drawer_open().cloned();

    let (title, subtitle) = match &open {
        Some(o) => (o.name.clone(), o.dir.display().to_string()),
        None => ("satz-studio".to_string(), String::new()),
    };
    let (satz_label, satz_error) = match &satz {
        SatzStatus::Unknown => ("locating satz".to_string(), false),
        SatzStatus::Located(bin) => (format!("satz {}", bin.version), false),
        SatzStatus::TooOld { found, .. } => (format!("satz {found} is too old"), true),
        SatzStatus::Missing(_) => ("satz not found".to_string(), true),
    };

    rsx! {
        TopAppBar { title, subtitle,
            if let Some(o) = &open {
                Chip { kind: ChipKind::Assist, icon: "badge", label: o.runs_as.clone().unwrap_or_else(|| "runs as the ADC identity".to_string()) }
                if let Some(mode) = &o.deployment_mode {
                    Chip { kind: ChipKind::Assist, icon: if mode == "cloud" { "cloud" } else { "computer" }, label: mode.clone() }
                }
                {
                    let (icon, label, error) = match &schema {
                        Some(SchemaStatus::Loaded { providers, resources }) => ("schema", format!("{}: {resources} types", providers.join(", ")), false),
                        Some(SchemaStatus::Missing(_)) => ("schema", "no schema: run update-schema".to_string(), true),
                        None => ("schema", "schema not read".to_string(), false),
                    };
                    rsx! { Chip { kind: ChipKind::Assist, icon, label, error } }
                }
                if loading {
                    CircularProgress { size: 24 }
                }
                if let Some(estate_handle) = estate_handle {
                    IconButton { icon: "refresh", label: "Reload estate", onclick: move |_| estate_handle.send(EstateAction::Reload) }
                    IconButton { icon: "close", label: "Close estate", onclick: move |_| estate_handle.send(EstateAction::Close) }
                }
            }
            Chip { kind: ChipKind::Assist, icon: "terminal", label: satz_label, error: satz_error }
            Badge { count: diagnostics,
                IconButton {
                    icon: "troubleshoot",
                    label: "Diagnostics",
                    selected: drawer_open,
                    onclick: move |_| app.drawer_open().toggle(),
                }
            }
        }
    }
}
