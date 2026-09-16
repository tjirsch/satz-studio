use dioxus::prelude::*;

use crate::components::{Badge, Chip, ChipKind, CircularProgress, IconButton, TopAppBar};
use crate::state::{
    AppAction, AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, STUDIO_VERSION,
    SatzStatus, StudioLookStoreStoreExt, ToastKind, UpdateStoreStoreExt, satz_available,
    studio_available, toast,
};

/// The top bar: the open estate's name and directory, the identity every command in
/// every view runs as, the satz-studio version, the satz version, the commands palette,
/// reload, "Switch estate" and the drawer toggle.
///
/// The two version chips say "update available" when the launch look found a newer
/// release, and act on it: the satz-studio chip opens that release's page — the app does
/// not update itself — and the satz chip runs `satz self-update`, satz's own updater.
///
/// The bar carries what is true of the WINDOW — which estate is open, whom it acts as,
/// which satz compiles it. What is true of the ESTATE — its deployment mode, its
/// schema, how far its HCL has been taken — is the Overview's identity card, so each
/// fact has one place.
#[component]
pub fn TopBar() -> Element {
    let app = use_context::<Store<AppStore>>();
    let estate_handle = try_use_context::<Coroutine<EstateAction>>();
    let handle = use_coroutine_handle::<AppAction>();
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
        SatzStatus::Unusable(_) => ("satz does not run".to_string(), true),
    };
    // what the launch look found, while it is still newer than what runs
    let found = app.update().found().cloned();
    let updating = app.update().running().cloned();
    let satz_update = satz
        .binary()
        .and_then(|b| satz_available(found.as_ref(), Some(&b.version)))
        .map(|v| v.to_string());
    let look = app.studio_look().outcome().cloned();
    let studio_update =
        studio_available(look.as_ref()).map(|(v, page)| (v.to_string(), page.to_string()));

    rsx! {
        TopAppBar { title, subtitle,
            if let Some(o) = &open {
                Chip { kind: ChipKind::Assist, icon: "badge", label: o.runs_as.clone().unwrap_or_else(|| "runs as the ADC identity".to_string()) }
                if loading {
                    CircularProgress { size: 24 }
                }
            }
            match studio_update {
                Some((version, page)) => rsx! {
                    Chip {
                        kind: ChipKind::Assist,
                        icon: "open_in_new",
                        label: format!("satz-studio {STUDIO_VERSION} · update available: {version}"),
                        onclick: move |_| {
                            if let Err(e) = open::that(&page) {
                                toast(app, ToastKind::Error, format!("{page}: {e}"));
                            }
                        },
                    }
                },
                None => rsx! {
                    Chip { kind: ChipKind::Assist, icon: "info", label: format!("satz-studio {STUDIO_VERSION}") }
                },
            }
            match satz_update {
                Some(version) if updating => rsx! {
                    Chip { kind: ChipKind::Assist, icon: "downloading", label: format!("{satz_label} · updating to {version}…") }
                },
                Some(version) => rsx! {
                    Chip {
                        kind: ChipKind::Assist,
                        icon: "download",
                        label: format!("{satz_label} · update available: {version}"),
                        onclick: move |_| handle.send(AppAction::UpdateSatz { check_only: false }),
                    }
                },
                None => rsx! {
                    Chip { kind: ChipKind::Assist, icon: "terminal", label: satz_label, error: satz_error }
                },
            }
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
