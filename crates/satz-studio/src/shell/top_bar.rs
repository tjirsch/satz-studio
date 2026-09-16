use dioxus::prelude::*;

use crate::components::{Badge, Chip, ChipKind, CircularProgress, IconButton, TopAppBar};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, SatzStatus};

/// The top bar: the open estate's name and directory, the identity every command in
/// every view runs as, the satz version, the commands palette, reload, "Switch estate"
/// and the drawer toggle.
///
/// The bar carries what is true of the WINDOW — which estate is open, whom it acts as,
/// which satz compiles it. What is true of the ESTATE — its deployment mode, its
/// schema, how far its HCL has been taken — is the Overview's identity card, so each
/// fact has one place.
#[component]
pub fn TopBar() -> Element {
    let app = use_context::<Store<AppStore>>();
    let estate_handle = try_use_context::<Coroutine<EstateAction>>();
    let open = app.open().cloned();
    let satz = app.satz().cloned();
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
                if loading {
                    CircularProgress { size: 24 }
                }
            }
            Chip { kind: ChipKind::Assist, icon: "terminal", label: satz_label, error: satz_error }
            if let Some(estate_handle) = estate_handle {
                IconButton {
                    // the shortcut is ⌘K on macOS and Ctrl+K elsewhere, and the button
                    // says which of the two this machine takes
                    icon: if cfg!(target_os = "macos") { "keyboard_command_key" } else { "keyboard" },
                    label: if cfg!(target_os = "macos") { "Commands (⌘K)" } else { "Commands (Ctrl+K)" },
                    selected: app.palette_open().cloned(),
                    onclick: move |_| app.palette_open().toggle(),
                }
                IconButton { icon: "refresh", label: "Reload estate", onclick: move |_| estate_handle.send(EstateAction::Reload) }
                IconButton {
                    icon: "swap_horiz",
                    label: "Switch estate",
                    onclick: move |_| estate_handle.send(EstateAction::Close),
                }
            }
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
