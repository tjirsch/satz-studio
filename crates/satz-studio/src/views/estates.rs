//! The Estates view: choose a folder, see every `config.toml` under it with the
//! estates beside each, and open one.

use std::path::PathBuf;

use dioxus::prelude::*;

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, CircularProgress, Icon, IconButton,
    LinearProgress, TextField,
};
use crate::state::{
    AppAction, AppStore, AppStoreStoreExt, EstateFile, EstateSummary, ToastKind, View, toast,
};

/// Open the OS folder picker; a choice becomes [`AppAction::Discover`].
pub fn pick_root(handle: Coroutine<AppAction>) {
    spawn(async move {
        if let Some(folder) = rfd::AsyncFileDialog::new()
            .set_title("Choose the folder that holds your estates")
            .pick_folder()
            .await
        {
            handle.send(AppAction::Discover(folder.path().to_path_buf()));
        }
    });
}

#[component]
pub fn EstatesView() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<AppAction>();
    let root = app.root().cloned();
    let discovering = app.discovering().cloned();
    let summaries = app.estates().cloned();
    let mut typed = use_signal(|| {
        root.as_ref()
            .map(|r| r.display().to_string())
            .unwrap_or_default()
    });
    let rescan = move || {
        let path = typed();
        if path.trim().is_empty() {
            toast(app, ToastKind::Info, "Choose a folder first");
        } else {
            handle.send(AppAction::Discover(PathBuf::from(path.trim())));
        }
    };

    rsx! {
        div { class: "view estates",
            h1 { class: "view__title", "Estates" }
            div { class: "estates__toolbar",
                TextField {
                    label: "Folder",
                    value: typed(),
                    leading_icon: "folder",
                    class: "estates__root",
                    supporting: "Every config.toml under it, six levels deep",
                    oninput: move |v| typed.set(v),
                    onenter: move |_| rescan(),
                }
                Button { icon: "folder_open", variant: ButtonVariant::Filled, onclick: move |_| pick_root(handle), "Choose folder" }
                Button { icon: "refresh", variant: ButtonVariant::Tonal, disabled: discovering, onclick: move |_| rescan(), "Rescan" }
            }
            if discovering {
                LinearProgress {}
            }
            if summaries.is_empty() && !discovering {
                Card { variant: CardVariant::Filled, class: "estates__empty",
                    Icon { name: "home_storage", size: 48, class: "placeholder__icon" }
                    if root.is_some() {
                        p { "No config.toml here. Choose another folder." }
                    } else {
                        p { "Choose the folder that holds your estates." }
                    }
                }
            }
            div { class: "estates__grid",
                for summary in summaries {
                    EstateCard { key: "{summary.config.display()}", summary, root: root.clone() }
                }
            }
        }
    }
}

#[component]
fn EstateCard(summary: EstateSummary, root: Option<PathBuf>) -> Element {
    let app = use_context::<Store<AppStore>>();
    let title = root
        .as_deref()
        .and_then(|r| summary.dir.strip_prefix(r).ok())
        .map(|p| p.display().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| summary.dir.display().to_string());
    let dir = summary.dir.clone();
    rsx! {
        Card { variant: CardVariant::Outlined, class: "estate-card",
            div { class: "estate-card__header",
                Icon { name: "folder", class: "estate-card__folder" }
                div { class: "estate-card__titles",
                    h2 { class: "estate-card__title", "{title}" }
                    span { class: "estate-card__path", "{summary.config.display()}" }
                }
                IconButton {
                    icon: "open_in_new",
                    label: "Show in the file manager",
                    onclick: move |_| {
                        if let Err(e) = open::that(&dir) {
                            toast(app, ToastKind::Error, format!("{}: {e}", dir.display()));
                        }
                    },
                }
            }
            if let Some(error) = &summary.error {
                p { class: "estate-card__error", Icon { name: "error", size: 18 } "{error}" }
            }
            if summary.estates.is_empty() && summary.error.is_none() {
                p { class: "estate-card__none", "No .satz file here declares an estate." }
            }
            for estate in summary.estates.iter().cloned() {
                EstateRow { key: "{estate.path.display()}", config: summary.config.clone(), estate }
            }
        }
    }
}

#[component]
fn EstateRow(config: PathBuf, estate: EstateFile) -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<AppAction>();
    let opening = app.opening().read().as_deref() == Some(estate.path.as_path());
    let is_open = app
        .open()
        .read()
        .as_ref()
        .map(|o| o.main == estate.path)
        .unwrap_or(false);
    let busy = app.opening().is_some();
    let path = estate.path.clone();
    let (mode_icon, mode_label, mode_error) = match &estate.deployment_mode {
        Ok(Some(mode)) => (
            if mode == "cloud" { "cloud" } else { "computer" },
            mode.clone(),
            false,
        ),
        Ok(None) => ("help", "deployment mode not set".to_string(), false),
        Err(e) => (
            "error",
            e.lines().next().unwrap_or_default().to_string(),
            true,
        ),
    };
    rsx! {
        div { class: "estate-row", class: if is_open { "estate-row--open" },
            Icon { name: "description", size: 20 }
            span { class: "estate-row__name", "{estate.name}" }
            Chip { kind: ChipKind::Assist, icon: mode_icon, label: mode_label, error: mode_error }
            span { class: "grow" }
            if opening {
                CircularProgress { size: 24 }
            }
            if is_open {
                Chip { kind: ChipKind::Assist, icon: "check_circle", label: "open", class: "estate-row__open" }
                Button { variant: ButtonVariant::Text, onclick: move |_| handle.send(AppAction::CloseEstate), "Close" }
            } else {
                Button {
                    variant: ButtonVariant::Filled,
                    icon: "login",
                    disabled: busy,
                    onclick: move |_| {
                        handle.send(AppAction::OpenEstate { config: config.clone(), estate: path.clone() });
                        app.nav().set(View::Estates);
                    },
                    "Open"
                }
            }
        }
    }
}
