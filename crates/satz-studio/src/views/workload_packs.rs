//! The Estate destination's Workload packs tab: the packs in the estate's `packs_dir` as
//! `satz packs` lists them against a source — the repository's own, the ones fetched and
//! how each stands — and what the source holds beyond them. A fetch is `satz get-pack`,
//! an update `satz update-pack`, and a pack edited here is shown with its difference and
//! written only when the operator says so (ADR 0024). The rows are satz's report; the
//! app derives no state of its own.

use std::path::PathBuf;

use dioxus::prelude::*;
use satz_studio_core::edit::Edit;
use satz_studio_core::satz::reports::{
    PackState, UpdateAction, UpdatedPack, WorkloadPackRow, WorkloadPacksReport,
};
use satz_studio_core::satz::workload::{PackSource, Which};

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, Dialog, Icon, LinearProgress,
};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, PackOp};

/// The word and the icon of a pack's state chip.
fn state_chip(state: PackState) -> (&'static str, &'static str) {
    match state {
        PackState::Own => ("own", "edit_document"),
        PackState::Current => ("current", "check_circle"),
        PackState::Behind => ("behind", "update"),
        PackState::Edited => ("edited", "edit"),
        PackState::Gone => ("gone from the source", "cloud_off"),
        PackState::NotChecked => ("not checked", "help"),
        PackState::Available => ("available", "cloud_download"),
    }
}

/// The versions of a row: the local one, the source's where it differs.
fn versions(row: &WorkloadPackRow) -> String {
    match (&row.version, &row.upstream_version) {
        (Some(v), Some(u)) if v != u => format!("{v} → {u}"),
        (Some(v), _) => v.clone(),
        (None, Some(u)) => u.clone(),
        (None, None) => String::new(),
    }
}

/// What a state means for the operator, in one sentence.
fn meaning(state: PackState) -> &'static str {
    match state {
        PackState::Own => {
            "No recorded origin: this repository's own pack. No fetch and no update writes it."
        }
        PackState::Current => "Fetched, unchanged since, and the source holds the same file.",
        PackState::Behind => "Fetched and unchanged since; the source holds a newer file.",
        PackState::Edited => {
            "Changed here since it was fetched. An update leaves it as it is and shows the difference."
        }
        PackState::Gone => "Fetched, and the source holds it no longer.",
        PackState::NotChecked => {
            "Fetched from another kind of source than the one read. Read that source to check it."
        }
        PackState::Available => "The source holds it and this estate does not.",
    }
}

/// The OS folder dialog for a source of packs; the chosen folder is listed.
fn choose_folder(handle: Coroutine<EstateAction>, start: Option<PathBuf>) {
    spawn(async move {
        let mut dialog =
            rfd::AsyncFileDialog::new().set_title("Choose the folder that holds the packs");
        if let Some(dir) = start {
            dialog = dialog.set_directory(dir);
        }
        if let Some(folder) = dialog.pick_folder().await {
            handle.send(EstateAction::Packs(PackOp::List(PackSource::Folder(
                folder.path().to_path_buf(),
            ))));
        }
    });
}

/// What the operator is asked before a write that cannot be taken back from here.
#[derive(Debug, Clone, PartialEq)]
enum Confirm {
    /// overwrite a pack edited here with the source's file
    TakeUpstream(String),
    /// forget where a fetched pack came from
    Detach(String),
}

#[component]
pub fn WorkloadPacksPane() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let report = app.estate().workload_packs().cloned();
    let source = app.estate().pack_source().cloned();
    let busy = app.estate().packs_busy().cloned() || app.estate().loading().cloned();
    let update = app.estate().pack_update().cloned();
    let used: Vec<String> = app
        .estate()
        .cst()
        .read()
        .as_deref()
        .map(|cst| {
            satz_studio_core::cst::scan_uses(cst)
                .into_iter()
                .map(|u| u.path)
                .collect()
        })
        .unwrap_or_default();
    let mut confirm = use_signal(|| None::<Confirm>);
    let folder_start = match &source {
        Some(PackSource::Folder(dir)) => Some(dir.clone()),
        _ => None,
    };

    let body = match report {
        None => rsx! {
            Card { variant: CardVariant::Filled, class: "interfaces__empty",
                Icon { name: "inventory", size: 48, class: "placeholder__icon" }
                p { "Nothing has been read yet. Reading the satz repository is a network read, so it happens when you ask for it." }
            }
        },
        Some(Err(e)) => rsx! {
            div { class: "interfaces__error",
                Icon { name: "error", size: 20 }
                div {
                    p { "satz packs could not list the packs:" }
                    pre { "{e}" }
                }
            }
        },
        Some(Ok(report)) => rsx! {
            PackCards { report, used, update, busy, confirm }
        },
    };

    rsx! {
        div { class: "pane interfaces",
            div { class: "interfaces__head",
                p { class: "view__lead",
                    "The workload packs of this estate, as satz packs lists them: the ones this repository wrote itself, the ones fetched from a source and how each stands against it, and what the source holds beyond them. A fetched pack is recorded in origin.toml beside the packs; a pack without a row there is this repository's own."
                }
                div { class: "wpacks__sources",
                    Button {
                        variant: ButtonVariant::Tonal,
                        icon: "cloud_sync",
                        disabled: busy,
                        onclick: move |_| handle.send(EstateAction::Packs(PackOp::List(PackSource::Repository))),
                        "Read the satz repository"
                    }
                    Button {
                        variant: ButtonVariant::Outlined,
                        icon: "folder_open",
                        disabled: busy,
                        onclick: {
                            let start = folder_start.clone();
                            move |_| choose_folder(handle, start.clone())
                        },
                        "Read a folder…"
                    }
                    if let Some(source) = &source {
                        span { class: "interfaces__supporting", "read from " code { "{source.label()}" } }
                    }
                }
                if busy {
                    LinearProgress {}
                }
            }
            {body}
        }
        Dialog {
            open: confirm().is_some(),
            title: match confirm() {
                Some(Confirm::TakeUpstream(_)) => "Overwrite your changes?".to_string(),
                Some(Confirm::Detach(_)) => "Keep it as your own pack?".to_string(),
                None => String::new(),
            },
            icon: "warning",
            ondismiss: move |_| confirm.set(None),
            actions: rsx! {
                Button { variant: ButtonVariant::Text, onclick: move |_| confirm.set(None), "Cancel" }
                Button {
                    variant: ButtonVariant::Filled,
                    onclick: move |_| {
                        match confirm() {
                            Some(Confirm::TakeUpstream(id)) => handle.send(EstateAction::Packs(PackOp::Update {
                                which: Which::One(id),
                                take_upstream: true,
                            })),
                            Some(Confirm::Detach(id)) => handle.send(EstateAction::Packs(PackOp::Detach(id))),
                            None => {}
                        }
                        confirm.set(None);
                    },
                    match confirm() {
                        Some(Confirm::TakeUpstream(_)) => "Overwrite",
                        _ => "Keep as own",
                    }
                }
            },
            match confirm() {
                Some(Confirm::TakeUpstream(id)) => rsx! {
                    p { "satz update-pack " code { "{id}" } " --take-upstream replaces the file with the source's. What you changed in it since it was fetched is gone; git is the way back." }
                },
                Some(Confirm::Detach(id)) => rsx! {
                    p { "satz detach-pack " code { "{id}" } " removes its row from origin.toml. The file stays as it is, and from then on no update writes it or tells you the source moved." }
                },
                None => rsx! {},
            }
        }
    }
}

/// The packs that are here, then what the source holds beyond them.
#[component]
fn PackCards(
    report: WorkloadPacksReport,
    used: Vec<String>,
    update: Option<satz_studio_core::satz::reports::UpdatePackReport>,
    busy: bool,
    confirm: Signal<Option<Confirm>>,
) -> Element {
    let handle = use_coroutine_handle::<EstateAction>();
    if report.packs.is_empty() {
        return rsx! {
            Card { variant: CardVariant::Filled, class: "interfaces__empty",
                Icon { name: "inventory", size: 48, class: "placeholder__icon" }
                p { "No workload pack is here, and " code { "{report.source}" } " holds none." }
            }
        };
    }
    let (here, available): (Vec<WorkloadPackRow>, Vec<WorkloadPackRow>) = report
        .packs
        .iter()
        .cloned()
        .partition(|p| p.state != PackState::Available);
    let behind = here.iter().any(|p| p.state == PackState::Behind);
    let updated = |id: &str| -> Option<UpdatedPack> {
        update
            .as_ref()?
            .packs
            .iter()
            .find(|p| p.id == id && p.action == UpdateAction::Edited)
            .cloned()
    };
    rsx! {
        Card { variant: CardVariant::Outlined, class: "interfaces__card",
            div { class: "interfaces__card-head",
                Icon { name: "inventory_2", size: 22 }
                h2 { class: "interfaces__name", "In this estate" }
                code { class: "interfaces__at", "{report.packs_dir}" }
                span { class: "grow" }
                if behind {
                    Button {
                        variant: ButtonVariant::Tonal,
                        icon: "update",
                        disabled: busy,
                        onclick: move |_| handle.send(EstateAction::Packs(PackOp::Update { which: Which::All, take_upstream: false })),
                        "Update all"
                    }
                }
            }
            if here.is_empty() {
                p { class: "interfaces__supporting", "No workload pack is here yet." }
            }
            for row in here {
                PackLine {
                    key: "{row.id}",
                    in_use: row.use_path().is_some_and(|p| used.iter().any(|u| u == p)),
                    edited: updated(&row.id),
                    row: row.clone(),
                    busy,
                    confirm,
                }
            }
        }
        if !available.is_empty() {
            Card { variant: CardVariant::Outlined, class: "interfaces__card",
                div { class: "interfaces__card-head",
                    Icon { name: "cloud_download", size: 22 }
                    h2 { class: "interfaces__name", "Available" }
                    code { class: "interfaces__at", "{report.source}" }
                }
                for row in available {
                    PackLine { key: "{row.id}", row: row.clone(), in_use: false, edited: None, busy, confirm }
                }
            }
        }
    }
}

/// One pack: its id, version and state, what the state means, and what can be done.
#[component]
fn PackLine(
    row: WorkloadPackRow,
    in_use: bool,
    edited: Option<UpdatedPack>,
    busy: bool,
    confirm: Signal<Option<Confirm>>,
) -> Element {
    let handle = use_coroutine_handle::<EstateAction>();
    let (word, icon) = state_chip(row.state);
    let id = row.id.clone();
    let use_path = row.use_path().map(str::to_string);
    rsx! {
        div { class: "interfaces__export",
            div { class: "interfaces__export-head",
                code { class: "interfaces__export-name", "{row.id}" }
                span { class: "interfaces__supporting", {versions(&row)} }
                Chip { kind: ChipKind::Assist, icon, label: word }
                if in_use {
                    Chip { kind: ChipKind::Assist, icon: "link", label: "used by the estate" }
                }
                span { class: "grow" }
                if let Some(source) = &row.source {
                    code { class: "interfaces__at", "{source}" }
                }
            }
            p { class: "interfaces__supporting", {meaning(row.state)} }
            if let Some(line) = &row.use_line {
                code { class: "interfaces__value", "{line}" }
            }
            if let Some(edited) = &edited {
                if let Some(diff) = &edited.diff {
                    p { class: "interfaces__supporting", "This file (−) against the source's (+):" }
                    pre { class: "wpacks__diff", "{diff}" }
                }
            }
            div { class: "wpacks__actions",
                if row.state == PackState::Available {
                    Button {
                        variant: ButtonVariant::Tonal,
                        icon: "download",
                        disabled: busy,
                        onclick: { let id = id.clone(); move |_| handle.send(EstateAction::Packs(PackOp::Get(Which::One(id.clone())))) },
                        "Fetch"
                    }
                }
                if row.state == PackState::Behind {
                    Button {
                        variant: ButtonVariant::Tonal,
                        icon: "update",
                        disabled: busy,
                        onclick: { let id = id.clone(); move |_| handle.send(EstateAction::Packs(PackOp::Update { which: Which::One(id.clone()), take_upstream: false })) },
                        "Update"
                    }
                }
                if row.state == PackState::Edited {
                    Button {
                        variant: ButtonVariant::Outlined,
                        icon: "difference",
                        disabled: busy,
                        onclick: { let id = id.clone(); move |_| handle.send(EstateAction::Packs(PackOp::Update { which: Which::One(id.clone()), take_upstream: false })) },
                        "Show the difference"
                    }
                    Button {
                        variant: ButtonVariant::Outlined,
                        icon: "cloud_download",
                        disabled: busy,
                        onclick: { let id = id.clone(); move |_| confirm.set(Some(Confirm::TakeUpstream(id.clone()))) },
                        "Take the source's file…"
                    }
                }
                if matches!(row.state, PackState::Edited | PackState::Gone | PackState::Current | PackState::Behind | PackState::NotChecked) {
                    Button {
                        variant: ButtonVariant::Text,
                        icon: "link_off",
                        disabled: busy,
                        onclick: { let id = id.clone(); move |_| confirm.set(Some(Confirm::Detach(id.clone()))) },
                        "Keep as own…"
                    }
                }
                if let Some(path) = use_path {
                    if !in_use {
                        Button {
                            variant: ButtonVariant::Filled,
                            icon: "add_link",
                            disabled: busy,
                            onclick: move |_| handle.send(EstateAction::CommitEdit(Edit::AddUse { path: path.clone() })),
                            "Add to the estate"
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(state: PackState, version: Option<&str>, upstream: Option<&str>) -> WorkloadPackRow {
        WorkloadPackRow {
            id: "demo".into(),
            state,
            version: version.map(str::to_string),
            upstream_version: upstream.map(str::to_string),
            source: None,
            use_line: None,
        }
    }

    #[test]
    fn a_row_shows_the_version_it_has_and_the_one_it_would_get() {
        assert_eq!(
            versions(&row(PackState::Behind, Some("1.0"), Some("1.1"))),
            "1.0 → 1.1"
        );
        assert_eq!(
            versions(&row(PackState::Current, Some("1.0"), Some("1.0"))),
            "1.0"
        );
        assert_eq!(versions(&row(PackState::Own, Some("0.1"), None)), "0.1");
        assert_eq!(
            versions(&row(PackState::Available, None, Some("2.0"))),
            "2.0"
        );
    }

    #[test]
    fn every_state_has_a_word_an_icon_and_a_sentence() {
        for state in [
            PackState::Own,
            PackState::Current,
            PackState::Behind,
            PackState::Edited,
            PackState::Gone,
            PackState::NotChecked,
            PackState::Available,
        ] {
            let (word, icon) = state_chip(state);
            assert!(!word.is_empty() && !icon.is_empty() && !meaning(state).is_empty());
        }
    }
}
