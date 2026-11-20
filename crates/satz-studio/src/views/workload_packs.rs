//! The Packs view's pane: a two-line lead with the rest of the explanation behind a help
//! button, the packs the estate's file uses, the estate's pack sources
//! ([`PackSourcesCard`]), then the packs in the estate's `packs_dir` as `satz packs
//! --source <name>` lists them against the chosen source — the repository's own, the ones
//! fetched and how each stands — and what the source holds beyond them. A fetch is `satz
//! get-pack --source`, an update `satz update-pack` from each pack's recorded source, and
//! a pack edited here is shown with its difference and written only when the operator
//! says so (ADR 0024). The rows are satz's report; the app derives no state of its own.
//! A row says only what is worth acting on: no chip for a pack that is current, "update
//! available" for one that is behind, and its actions as icon buttons (ADR 0031).

use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use satz_studio_core::cst::UseState;
use satz_studio_core::edit::Edit;
use satz_studio_core::satz::reports::{
    PackNeed, PackState, UpdateAction, UpdatedPack, WorkloadPackRow, WorkloadPacksReport,
};
use satz_studio_core::satz::workload::Which;

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, Dialog, Icon, IconButton,
    IconButtonVariant, LinearProgress, Tooltip,
};
use crate::state::{
    AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, PackOp, SourceOp, ToastKind,
    toast,
};
use crate::views::pack_sources::PackSourcesCard;

/// Which delegated values a pack reads, as its chip names them; none for a pack that
/// reads neither. Each is the default of a param of the pack: an estate that was not
/// delegated that value answers the param itself, so no pack is left out for it.
fn needs_text(needs: &[PackNeed]) -> Option<String> {
    let words: Vec<&str> = needs
        .iter()
        .map(|n| match n {
            PackNeed::Folder => "folder",
            PackNeed::Project => "project",
        })
        .collect();
    (!words.is_empty()).then(|| format!("delegated {}", words.join(" · ")))
}

/// What the "delegated …" chip means, as its tooltip says it.
const NEEDS_MEANING: &str = "Some of the pack's params default to the folder or project the organization delegated to this estate. An estate without that delegation answers them itself.";

/// The word and the icon of a pack's state chip; none where the state asks for nothing —
/// a pack that is current, or one the Available card already says is available.
fn state_chip(state: PackState) -> Option<(&'static str, &'static str)> {
    match state {
        PackState::Own => Some(("own", "edit_document")),
        PackState::Current | PackState::Available => None,
        PackState::Behind => Some(("update available", "update")),
        PackState::Edited => Some(("edited here", "edit")),
        PackState::Gone => Some(("gone from the source", "cloud_off")),
        PackState::NotChecked => Some(("not checked", "help")),
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

/// What a state means for the operator, in one sentence: a chip's tooltip, and the list
/// in the help dialog.
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
            "Fetched from another pack source than the one listed. Choose that source to check it."
        }
        PackState::Available => "The source holds it and this estate does not.",
    }
}

/// Every state, in the order the help dialog explains them.
const STATES: [PackState; 7] = [
    PackState::Own,
    PackState::Current,
    PackState::Behind,
    PackState::Edited,
    PackState::Gone,
    PackState::NotChecked,
    PackState::Available,
];

/// The word the help dialog gives a state: its chip's, or what a row without a chip is.
fn state_word(state: PackState) -> &'static str {
    match state_chip(state) {
        Some((word, _)) => word,
        None if state == PackState::Current => "no chip",
        None => "in Available",
    }
}

/// What the operator is asked before a write that cannot be taken back from here.
#[derive(Debug, Clone, PartialEq)]
enum Confirm {
    /// overwrite a pack edited here with the source's file
    TakeUpstream(String),
    /// forget where a fetched pack came from
    Detach(String),
    /// take a pack source out of `config.toml`
    RemoveSource(String),
}

/// A commit shortened to the seven characters git shows.
fn short(commit: &str) -> String {
    commit.chars().take(7).collect()
}

/// A fetched pack's source as its row names it: the name, and the commit of a GitHub
/// source, shortened.
fn origin_text(row: &WorkloadPackRow) -> Option<String> {
    let source = row.source.as_deref()?;
    Some(match &row.commit {
        Some(commit) => format!("{source} @ {}", short(commit)),
        None => source.to_string(),
    })
}

/// The file of a pack in `packs_dir`, which satz names `<id>.satz`; `packs_dir` is the
/// estate's, relative to the estate's directory unless it is absolute.
fn pack_file(estate_dir: &Path, packs_dir: &str, id: &str) -> PathBuf {
    estate_dir.join(packs_dir).join(format!("{id}.satz"))
}

#[component]
pub fn WorkloadPacksPane() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let report = app.estate().workload_packs().cloned();
    let listed = app.estate().pack_source().cloned();
    let busy = app.estate().packs_busy().cloned() || app.estate().loading().cloned();
    let update = app.estate().pack_update().cloned();
    let uses = app
        .estate()
        .cst()
        .read()
        .as_deref()
        .map(satz_studio_core::cst::scan_uses)
        .unwrap_or_default();
    let used: Vec<String> = uses.iter().map(|u| u.path.clone()).collect();
    let estate_dir = app.open().read().as_ref().map(|o| o.dir.clone());
    let mut confirm = use_signal(|| None::<Confirm>);
    let mut help = use_signal(|| false);

    let body = match report {
        None if listed.is_some() => rsx! {},
        None => rsx! {
            Card { variant: CardVariant::Filled, class: "interfaces__empty",
                Icon { name: "inventory", size: 48, class: "placeholder__icon" }
                p { "No pack source has been listed yet. Choose one above to list its packs." }
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
            PackCards { report, used, estate_dir, update, busy, confirm }
        },
    };

    rsx! {
        div { class: "pane interfaces",
            div { class: "wpacks__head",
                p { class: "view__lead grow",
                    "A pack is a template for a workload or a part of one: take it from a source, add it to the estate, and adapt it — a pack changed here is this repository's own. What a pack asks is in Decisions."
                }
                IconButton {
                    icon: "help",
                    label: "About workload packs",
                    onclick: move |_| help.set(true),
                }
            }
            if busy {
                LinearProgress {}
            }
            UsedPacks { uses: uses.iter().map(|u| (u.path.clone(), u.state == UseState::Active)).collect::<Vec<_>>() }
            PackSourcesCard {
                busy,
                confirm_remove: move |name: String| confirm.set(Some(Confirm::RemoveSource(name))),
            }
            {body}
        }
        Dialog {
            open: help(),
            title: "About workload packs",
            icon: "help",
            class: "interfaces__add",
            ondismiss: move |_| help.set(false),
            actions: rsx! {
                Button { variant: ButtonVariant::Text, onclick: move |_| help.set(false), "Close" }
            },
            div { class: "wpacks__help",
                p { "satz packs lists the packs of this estate's packs_dir against the pack source you choose: the ones this repository wrote itself, the ones fetched and how each stands against its source, and what the source holds beyond them." }
                p { "A fetched pack is recorded in origin.toml beside the packs, with the name of its source; a pack without a row there is this repository's own. Update reads each pack from the source it was fetched from; a pack edited here is never overwritten without asking." }
                p { "The pack sources are config.toml's, and satz pack-source writes them. A GitHub source is a network read, so it is read only when you choose it." }
                h3 { class: "wpacks__help-title", "What a row says" }
                dl { class: "wpacks__states",
                    for state in STATES {
                        dt { "{state_word(state)}" }
                        dd { {meaning(state)} }
                    }
                    dt { "delegated folder · project" }
                    dd { {NEEDS_MEANING} }
                }
            }
        }
        Dialog {
            open: confirm().is_some(),
            title: match confirm() {
                Some(Confirm::TakeUpstream(_)) => "Overwrite your changes?".to_string(),
                Some(Confirm::Detach(_)) => "Keep it as your own pack?".to_string(),
                Some(Confirm::RemoveSource(_)) => "Remove the pack source?".to_string(),
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
                            Some(Confirm::RemoveSource(name)) => handle.send(EstateAction::PackSources(SourceOp::Remove(name))),
                            None => {}
                        }
                        confirm.set(None);
                    },
                    match confirm() {
                        Some(Confirm::TakeUpstream(_)) => "Overwrite",
                        Some(Confirm::RemoveSource(_)) => "Remove",
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
                Some(Confirm::RemoveSource(name)) => rsx! {
                    p { "satz pack-source remove " code { "{name}" } " takes it out of config.toml. satz refuses while origin.toml names it for a fetched pack: keep that pack as your own first." }
                },
                None => rsx! {},
            }
        }
    }
}

/// Every pack the estate's file reads — each `use` line, active or switched off — as one
/// row of chips. It needs no pack source: it is the file's own text.
#[component]
fn UsedPacks(uses: Vec<(String, bool)>) -> Element {
    rsx! {
        Card { variant: CardVariant::Outlined, class: "interfaces__card",
            div { class: "interfaces__card-head",
                Icon { name: "link", size: 22 }
                h2 { class: "interfaces__name", "Used by this estate" }
                span { class: "interfaces__supporting", "{uses.len()}" }
            }
            if uses.is_empty() {
                p { class: "interfaces__supporting", "The estate's file has no use line." }
            } else {
                div { class: "interfaces__entries",
                    for (path, active) in uses {
                        Chip {
                            key: "{path}",
                            kind: ChipKind::Assist,
                            icon: if active { "link" } else { "link_off" },
                            label: if active { path.clone() } else { format!("{path} · off") },
                        }
                    }
                }
            }
        }
    }
}

/// The sources a list read, as the report names them: each one's location, with the
/// commit of a GitHub source shortened.
fn read_from(report: &WorkloadPacksReport) -> String {
    report
        .sources
        .iter()
        .map(|s| match &s.commit {
            Some(c) => format!("{} @ {}", s.location, short(c)),
            None => s.location.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The packs that are here, then what the source holds beyond them.
#[component]
fn PackCards(
    report: WorkloadPacksReport,
    used: Vec<String>,
    estate_dir: Option<PathBuf>,
    update: Option<satz_studio_core::satz::reports::UpdatePackReport>,
    busy: bool,
    confirm: Signal<Option<Confirm>>,
) -> Element {
    let handle = use_coroutine_handle::<EstateAction>();
    if report.packs.is_empty() {
        return rsx! {
            Card { variant: CardVariant::Filled, class: "interfaces__empty",
                Icon { name: "inventory", size: 48, class: "placeholder__icon" }
                p { "No workload pack is here, and " code { "{read_from(&report)}" } " holds none." }
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
    let file = |id: &str| {
        estate_dir
            .as_deref()
            .map(|dir| pack_file(dir, &report.packs_dir, id))
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
                    file: file(&row.id),
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
                    code { class: "interfaces__at", "{read_from(&report)}" }
                }
                for row in available {
                    PackLine { key: "{row.id}", row: row.clone(), in_use: false, edited: None, file: None, busy, confirm }
                }
            }
        }
    }
}

/// One pack on one line: its id and version, a chip only where its state asks for
/// something, where it came from, and what can be done as icon buttons whose tooltip
/// names the action. An edited pack's difference stands below once it was asked for.
#[component]
fn PackLine(
    row: WorkloadPackRow,
    in_use: bool,
    edited: Option<UpdatedPack>,
    /// the pack's file, for a pack that is here
    file: Option<PathBuf>,
    busy: bool,
    confirm: Signal<Option<Confirm>>,
) -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let id = row.id.clone();
    let use_path = row.use_path().map(str::to_string);
    rsx! {
        div { class: "interfaces__export wpacks__row",
            div { class: "interfaces__export-head",
                code { class: "interfaces__export-name", "{row.id}" }
                span { class: "interfaces__supporting", {versions(&row)} }
                if let Some((word, icon)) = state_chip(row.state) {
                    Tooltip { text: meaning(row.state),
                        Chip { kind: ChipKind::Assist, icon, label: word, selected: row.state == PackState::Behind }
                    }
                }
                if let Some(needs) = needs_text(&row.needs) {
                    Tooltip { text: NEEDS_MEANING,
                        Chip { kind: ChipKind::Assist, icon: "rule", label: needs }
                    }
                }
                if in_use {
                    Chip { kind: ChipKind::Assist, icon: "link", label: "in use" }
                }
                span { class: "grow" }
                if let Some(origin) = origin_text(&row) {
                    code { class: "interfaces__at", "{origin}" }
                }
                div { class: "wpacks__actions",
                    if row.state == PackState::Available {
                        IconButton {
                            icon: "download",
                            label: "Fetch",
                            variant: IconButtonVariant::Tonal,
                            disabled: busy,
                            onclick: { let id = id.clone(); move |_| handle.send(EstateAction::Packs(PackOp::Get(Which::One(id.clone())))) },
                        }
                    }
                    if row.state == PackState::Behind {
                        IconButton {
                            icon: "update",
                            label: "Update",
                            variant: IconButtonVariant::Tonal,
                            disabled: busy,
                            onclick: { let id = id.clone(); move |_| handle.send(EstateAction::Packs(PackOp::Update { which: Which::One(id.clone()), take_upstream: false })) },
                        }
                    }
                    if row.state == PackState::Edited {
                        IconButton {
                            icon: "difference",
                            label: "Show the difference",
                            disabled: busy,
                            onclick: { let id = id.clone(); move |_| handle.send(EstateAction::Packs(PackOp::Update { which: Which::One(id.clone()), take_upstream: false })) },
                        }
                        IconButton {
                            icon: "cloud_download",
                            label: "Take the source's file…",
                            disabled: busy,
                            onclick: { let id = id.clone(); move |_| confirm.set(Some(Confirm::TakeUpstream(id.clone()))) },
                        }
                    }
                    if matches!(row.state, PackState::Edited | PackState::Gone | PackState::Current | PackState::Behind | PackState::NotChecked) {
                        IconButton {
                            icon: "link_off",
                            label: "Keep as own…",
                            disabled: busy,
                            onclick: { let id = id.clone(); move |_| confirm.set(Some(Confirm::Detach(id.clone()))) },
                        }
                    }
                    if let Some(file) = file {
                        IconButton {
                            icon: "open_in_new",
                            label: "Open in your editor",
                            onclick: move |_| {
                                if let Err(e) = open::that(&file) {
                                    toast(app, ToastKind::Error, format!("{}: {e}", file.display()));
                                }
                            },
                        }
                    }
                    if let Some(path) = use_path {
                        if !in_use {
                            IconButton {
                                icon: "add_link",
                                label: "Add to the estate: {path}",
                                variant: IconButtonVariant::Filled,
                                disabled: busy,
                                onclick: move |_| handle.send(EstateAction::CommitEdit(Edit::AddUse { path: path.clone() })),
                            }
                        }
                    }
                }
            }
            if let Some(edited) = &edited {
                if let Some(diff) = &edited.diff {
                    p { class: "interfaces__supporting", "This file (−) against the source's (+):" }
                    pre { class: "wpacks__diff", "{diff}" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_packs_needs_are_named_on_one_chip() {
        assert_eq!(
            needs_text(&[PackNeed::Project]).as_deref(),
            Some("delegated project")
        );
        assert_eq!(
            needs_text(&[PackNeed::Folder, PackNeed::Project]).as_deref(),
            Some("delegated folder · project")
        );
        assert_eq!(needs_text(&[]), None);
    }

    fn row(state: PackState, version: Option<&str>, upstream: Option<&str>) -> WorkloadPackRow {
        WorkloadPackRow {
            id: "demo".into(),
            state,
            version: version.map(str::to_string),
            upstream_version: upstream.map(str::to_string),
            source: None,
            commit: None,
            use_line: None,
            needs: Vec::new(),
        }
    }

    #[test]
    fn a_fetched_row_names_its_source_and_short_commit() {
        let mut fetched = row(PackState::Current, Some("1.0"), Some("1.0"));
        assert_eq!(origin_text(&fetched), None);
        fetched.source = Some("up".into());
        assert_eq!(origin_text(&fetched).as_deref(), Some("up"));
        fetched.source = Some("satz".into());
        fetched.commit = Some("0123456789abcdef0123456789abcdef01234567".into());
        assert_eq!(origin_text(&fetched).as_deref(), Some("satz @ 0123456"));
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

    /// A current pack carries no chip: only a state that asks for something does, and a
    /// pack behind its source says "update available".
    #[test]
    fn only_a_state_that_asks_for_something_has_a_chip() {
        assert_eq!(state_chip(PackState::Current), None);
        assert_eq!(state_chip(PackState::Available), None);
        assert_eq!(
            state_chip(PackState::Behind),
            Some(("update available", "update"))
        );
        for state in STATES {
            assert!(!meaning(state).is_empty() && !state_word(state).is_empty());
        }
    }

    #[test]
    fn a_packs_file_is_its_id_in_packs_dir_under_the_estate() {
        assert_eq!(
            pack_file(Path::new("/e"), "packs", "team-access"),
            PathBuf::from("/e/packs/team-access.satz")
        );
        assert_eq!(
            pack_file(Path::new("/e"), "/abs/packs", "x"),
            PathBuf::from("/abs/packs/x.satz")
        );
    }
}
