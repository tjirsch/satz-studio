//! The Packs view's pane: the estate's pack sources ([`PackSourcesCard`]), the interface
//! folders the estate uses with what each exports ([`FoldersCard`]), then the packs of the
//! estate's `packs_dir` — first every one the estate `use`s, read from its own `use`
//! lines, then the others. `satz packs --local`, which reads no pack source, gives each its
//! state and declarations as soon as the view stands; once a source is listed with `satz
//! packs --source <name>`, its rows win, saying how each pack stands against it, and what
//! the source holds beyond them is listed too. A fetch is
//! `satz get-pack --source`, an update `satz update-pack` from each pack's recorded
//! source, and a pack edited here is shown with its difference and written only when the
//! operator says so (ADR 0024). A new pack is the repository's own: written and formatted,
//! its `use` line added through the app's writer, and opened in the application the
//! system gives `.satz` files, as Edit opens any pack (ADR 0031); once it works, "Store in
//! a pack source…" copies it into a folder source with `satz publish-pack`
//! ([`PublishDialog`]). Each listed pack — fetched or only available — opens its details
//! ([`PackDetails`]): what it declares and how each interface value it reads stands against
//! the open estate, as `satz packs --estate` matched it (satz's ADR 0100); a read the estate
//! does not give marks the row "Unmet read". When an external editor is done with a pack is
//! not knowable, so the card's Reload reads the estate and the listed source again. The
//! states and the matches are satz's report; the app derives none of its own.

use std::path::PathBuf;

use dioxus::prelude::*;
use satz_studio_core::cst::UseState;
use satz_studio_core::edit::Edit;
use satz_studio_core::satz::reports::{
    InterfaceFolder, PackExport, PackRead, PackState, UpdateAction, UpdatePackReport, UpdatedPack,
    WorkloadPackRow, WorkloadPacksReport,
};
use satz_studio_core::satz::workload::{Which, new_pack_id};

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, Dialog, HelpButton, Icon, IconButton,
    LinearProgress, TextField,
};
use crate::state::{
    AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, PackOp, SourceOp, edit_file,
};
use crate::views::overview::{DelegatedLine, ExportsDialog};
use crate::views::pack_sources::{PackSourcesCard, Sheet};
use crate::views::publish_pack::PublishDialog;

/// What a read of a pack comes to against the estate, as satz matched it: the param the
/// estate binds in its place, the interface folders that give it, or satz's reason that
/// none does — the last in the error color.
fn read_status(read: &PackRead) -> (String, bool) {
    if read.bound_by_estate {
        let param = read.param.as_deref().unwrap_or_default();
        return (format!("set by the estate in {param}"), false);
    }
    if !read.met_by.is_empty() {
        return (format!("from {}", read.met_by.join(", ")), false);
    }
    match &read.unmet_reason {
        Some(reason) => (reason.clone(), true),
        None => ("given by no interface folder".to_string(), true),
    }
}

/// A pack's exports, grouped by the interface they stand in: the core exports first,
/// then each interface in the order the pack first names it.
fn export_groups(exports: &[PackExport]) -> Vec<(Option<String>, Vec<String>)> {
    let mut groups: Vec<(Option<String>, Vec<String>)> = Vec::new();
    for e in exports {
        match groups.iter_mut().find(|(i, _)| *i == e.interface) {
            Some((_, names)) => names.push(e.name.clone()),
            None => groups.push((e.interface.clone(), vec![e.name.clone()])),
        }
    }
    groups.sort_by_key(|(i, _)| i.is_some());
    groups
}

/// The badge a pack's state earns: the word, its icon, and whether it is in the error
/// color. A pack that is current, the repository's own or only available needs no word.
/// `local` says the row is `satz packs --local`'s, which read no source: its `not-checked`
/// is a fetched file unchanged since its fetch, not a pack of another source.
fn badge(state: PackState, local: bool) -> Option<(&'static str, &'static str, bool)> {
    match state {
        PackState::Behind => Some(("Update available", "update", false)),
        PackState::Edited => Some(("Edited here", "edit", false)),
        PackState::Gone => Some(("Gone from source", "cloud_off", true)),
        PackState::NotChecked if local => Some(("Unchanged since fetched", "history", false)),
        PackState::NotChecked => Some(("Not checked", "pending", false)),
        PackState::Own | PackState::Current | PackState::Available => None,
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

/// A commit shortened to the seven characters git shows.
fn short(commit: &str) -> String {
    commit.chars().take(7).collect()
}

/// Where a pack comes from, as its row names it: "Own pack" for the repository's own,
/// else the source's name and the commit of a GitHub source, shortened.
fn origin_text(row: &WorkloadPackRow) -> Option<String> {
    if row.state == PackState::Own {
        return Some("Own pack".to_string());
    }
    let source = row.source.as_deref()?;
    Some(match &row.commit {
        Some(commit) => format!("{source} @ {}", short(commit)),
        None => source.to_string(),
    })
}

/// One pack of `packs_dir`, as the card lists it: the `use` path the estate reads it by,
/// where it does, satz's row for it once a listing names it, and whether that row is the
/// local listing's.
#[derive(Debug, Clone, PartialEq)]
struct HereRow {
    id: String,
    used: Option<String>,
    row: Option<WorkloadPackRow>,
    local: bool,
}

/// The packs of `packs_dir`: every one the estate uses, in the order of its `use` lines,
/// then the others a listing names, by id. A pack's row is the source listing's where it
/// names the pack, else the local listing's. `used` is each used pack's id and the path of
/// its line.
fn here_rows(
    source: Option<&WorkloadPacksReport>,
    local: Option<&WorkloadPacksReport>,
    used: &[(String, String)],
) -> Vec<HereRow> {
    let here = |r: &WorkloadPacksReport, is_local: bool| -> Vec<(WorkloadPackRow, bool)> {
        r.packs
            .iter()
            .filter(|p| p.state != PackState::Available)
            .map(|p| (p.clone(), is_local))
            .collect()
    };
    let mut listed: Vec<(WorkloadPackRow, bool)> =
        source.map(|r| here(r, false)).unwrap_or_default();
    for (row, is_local) in local.map(|r| here(r, true)).unwrap_or_default() {
        if listed.iter().all(|(p, _)| p.id != row.id) {
            listed.push((row, is_local));
        }
    }
    let row_of = |id: &str| listed.iter().find(|(p, _)| p.id == id).cloned();
    let mut rows: Vec<HereRow> = Vec::new();
    for (id, path) in used {
        if rows.iter().all(|r| &r.id != id) {
            let found = row_of(id);
            rows.push(HereRow {
                id: id.clone(),
                used: Some(path.clone()),
                local: found.as_ref().is_some_and(|(_, l)| *l),
                row: found.map(|(p, _)| p),
            });
        }
    }
    let mut others: Vec<HereRow> = listed
        .iter()
        .filter(|(p, _)| used.iter().all(|(id, _)| id != &p.id))
        .map(|(p, is_local)| HereRow {
            id: p.id.clone(),
            used: None,
            row: Some(p.clone()),
            local: *is_local,
        })
        .collect();
    others.sort_by(|a, b| a.id.cmp(&b.id));
    rows.extend(others);
    rows
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

#[component]
pub fn WorkloadPacksPane() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let Some(open) = app.open().cloned() else {
        return rsx! {};
    };
    let report = app.estate().workload_packs().cloned();
    let local_report = app.estate().local_packs().cloned();
    // the local listing reads no source: run when the view first stands
    use_hook(move || {
        if app.estate().local_packs().peek().is_none() {
            handle.send(EstateAction::Packs(PackOp::ListLocal));
        }
    });
    let busy = app.estate().packs_busy().cloned() || app.estate().loading().cloned();
    let update = app.estate().pack_update().cloned();
    // the packs the estate uses: its active `use` lines that read a file of packs_dir
    let used: Vec<(String, String)> = app
        .estate()
        .cst()
        .read()
        .as_deref()
        .map(|cst| {
            satz_studio_core::cst::scan_uses(cst)
                .into_iter()
                .filter(|u| u.state == UseState::Active)
                .filter_map(|u| {
                    let id = open.session.dir.pack_of_use(&open.main, &u.path)?;
                    Some((id, u.path))
                })
                .collect()
        })
        .unwrap_or_default();
    let packs_dir = open.session.dir.packs_dir();
    let mut confirm = use_signal(|| None::<Confirm>);
    let sheet = use_signal(|| Sheet::Closed);
    let publishing = use_signal(|| None::<String>);

    let (listed, failed) = match &report {
        Some(Ok(r)) => (Some(r.clone()), None),
        Some(Err(e)) => (None, Some(e.clone())),
        None => (None, None),
    };
    let (local, local_failed) = match &local_report {
        Some(Ok(r)) => (Some(r.clone()), None),
        Some(Err(e)) => (None, Some(e.clone())),
        None => (None, None),
    };
    let rows = here_rows(listed.as_ref(), local.as_ref(), &used);
    // the folders are the estate's, the same in either listing
    let folders: Vec<InterfaceFolder> = local
        .as_ref()
        .or(listed.as_ref())
        .map(|r| r.interface_folders.clone())
        .unwrap_or_default();
    let available: Vec<WorkloadPackRow> = listed
        .as_ref()
        .map(|r| {
            r.packs
                .iter()
                .filter(|p| p.state == PackState::Available)
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    rsx! {
        div { class: "pane interfaces",
            if busy {
                LinearProgress {}
            }
            PackSourcesCard {
                busy,
                sheet,
                confirm_remove: move |name: String| confirm.set(Some(Confirm::RemoveSource(name))),
            }
            if !folders.is_empty() {
                FoldersCard { folders }
            }
            HereCard {
                rows,
                listed: listed.is_some(),
                local_listed: local.is_some(),
                failed: failed.or(local_failed),
                packs_dir: open.session.dir.tool.packs_dir.clone(),
                files: packs_dir,
                update,
                busy,
                confirm,
                publishing,
            }
            if let Some(report) = listed.filter(|_| !available.is_empty()) {
                Card { variant: CardVariant::Outlined, class: "interfaces__card",
                    div { class: "interfaces__card-head",
                        Icon { name: "cloud_download", size: 22 }
                        h2 { class: "interfaces__name", "Available" }
                        code { class: "interfaces__at", "{read_from(&report)}" }
                    }
                    for row in available {
                        PackLine { key: "{row.id}", id: row.id.clone(), row: Some(row.clone()), local: false, used: true, file: None, edited: None, busy, confirm, publishing }
                    }
                }
            }
        }
        PublishDialog { publishing, sheet, busy }
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
                                from: None,
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

/// The packs of `packs_dir`: the used ones first, each with what can be done; "New
/// pack…" in its head, and "Update all" while a pack has an update available.
#[component]
fn HereCard(
    rows: Vec<HereRow>,
    listed: bool,
    local_listed: bool,
    failed: Option<String>,
    packs_dir: String,
    files: PathBuf,
    update: Option<UpdatePackReport>,
    busy: bool,
    confirm: Signal<Option<Confirm>>,
    publishing: Signal<Option<String>>,
) -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let mut creating = use_signal(|| false);
    let mut name = use_signal(String::new);
    let behind = rows
        .iter()
        .any(|r| r.row.as_ref().is_some_and(|p| p.state == PackState::Behind));
    let edited_of = |id: &str| -> Option<UpdatedPack> {
        update
            .as_ref()?
            .packs
            .iter()
            .find(|p| p.id == id && p.action == UpdateAction::Edited)
            .cloned()
    };
    let checked = new_pack_id(name().trim());
    let problem = match &checked {
        Err(e) if !name().trim().is_empty() => e.clone(),
        _ => String::new(),
    };

    rsx! {
        Card { variant: CardVariant::Outlined, class: "interfaces__card",
            div { class: "interfaces__card-head",
                Icon { name: "inventory_2", size: 22 }
                h2 { class: "interfaces__name", "In this estate" }
                code { class: "interfaces__at", "{packs_dir}" }
                span { class: "grow" }
                if behind {
                    Button {
                        variant: ButtonVariant::Tonal,
                        icon: "update",
                        disabled: busy,
                        onclick: move |_| handle.send(EstateAction::Packs(PackOp::Update { which: Which::All, from: None, take_upstream: false })),
                        "Update all"
                    }
                }
                Button {
                    variant: ButtonVariant::Text,
                    icon: "note_add",
                    disabled: busy,
                    onclick: move |_| {
                        name.set(String::new());
                        creating.set(true);
                    },
                    "New pack…"
                }
                IconButton {
                    icon: "refresh",
                    label: "Reload",
                    disabled: busy,
                    onclick: move |_| {
                        handle.send(EstateAction::Reload);
                        handle.send(EstateAction::Packs(match app.estate().pack_source().cloned() {
                            Some(source) => PackOp::List(source),
                            None => PackOp::ListLocal,
                        }));
                    },
                }
                HelpButton { title: "Workload packs",
                    p { "The packs this estate uses come first, then the other files of {packs_dir}. A fetched pack is recorded in origin.toml beside the packs with the name of its source; a pack without a row there is this repository's own, and no fetch and no update writes it." }
                    p { "Choosing a pack source runs satz packs against it, and each fetched pack says how it stands: Update available — unchanged here, and the source holds a newer file; Edited here — changed since it was fetched, so an update leaves it as it is and shows the difference; Gone from source — the source holds it no longer; Not checked — fetched from another source than the one listed." }
                    p { "Before a source is chosen, satz packs --local lists the packs here without reading any source, so nothing goes over the network: Edited here — changed since it was fetched; Unchanged since fetched — the file is the one that was fetched, and whether its source holds a newer one is known once that source is chosen." }
                    p { "Details shows what a pack declares — its params with their defaults and the questions that ask for them, its list params, its exports by interface — and each value it reads from the interface folders this estate uses: the folders that give it, the param the estate sets in its place, or why none does. Unmet read marks a pack with a read this estate does not give; the transpile refuses it once the estate uses the pack." }
                    p { "Reload reads the estate again, the packs here and the listed pack source with them: the way to see a pack after New pack or Edit, since the window cannot tell when the editor is done with the file." }
                    p { "New pack writes {packs_dir}/<name>.satz, adds its use line to the estate and opens the file in the application this computer gives .satz files; Edit opens a pack the same way. Keep as own forgets where a fetched pack came from. What a pack asks is answered in Decisions." }
                    p { "Store in a pack source, on your own pack, copies it into a folder pack source with satz publish-pack once it clears review-pack's bar inside this estate, and records it as fetched from there, so other estates fetch it and update-pack follows it here too." }
                }
            }
            if let Some(e) = &failed {
                div { class: "interfaces__error",
                    Icon { name: "error", size: 20 }
                    div {
                        p { "satz packs could not list the packs:" }
                        pre { "{e}" }
                    }
                }
            } else if !listed && local_listed && !rows.is_empty() {
                p { class: "interfaces__supporting", "Choose a pack source to check each fetched pack against it and see what it offers." }
            }
            if rows.is_empty() && (listed || local_listed) {
                p { class: "interfaces__supporting",
                    if listed { "No workload pack is here yet." } else { "No workload pack is here yet. Choose a pack source to list what it offers." }
                }
            }
            for here in rows {
                PackLine {
                    key: "{here.id}",
                    id: here.id.clone(),
                    used: here.used.is_some(),
                    edited: edited_of(&here.id),
                    file: Some(files.join(format!("{}.satz", here.id))),
                    row: here.row,
                    local: here.local,
                    busy,
                    confirm,
                    publishing,
                }
            }
        }
        Dialog {
            open: creating(),
            title: "New pack",
            icon: "note_add",
            class: "interfaces__add",
            ondismiss: move |_| creating.set(false),
            actions: rsx! {
                Button { variant: ButtonVariant::Text, onclick: move |_| creating.set(false), "Cancel" }
                Button {
                    variant: ButtonVariant::Filled,
                    icon: "check",
                    disabled: busy || checked.is_err(),
                    onclick: move |_| {
                        handle.send(EstateAction::NewPack(name().trim().to_string()));
                        creating.set(false);
                    },
                    "Create"
                }
            },
            div { class: "wpacks__form",
                TextField {
                    label: "Name",
                    value: name(),
                    monospace: true,
                    error: !problem.is_empty(),
                    supporting: if problem.is_empty() {
                        format!("a lowercase letter, then lowercase letters, digits and -; the file is {packs_dir}/<name>.satz")
                    } else {
                        problem.clone()
                    },
                    oninput: move |v: String| name.set(v),
                }
                p { class: "interfaces__supporting",
                    "The pack starts at version 0.1 with an empty params block, the estate gets its use line, and the file opens in the application this computer gives .satz files."
                }
            }
        }
    }
}

/// The interface folders the estate uses, one row each: its name, the organization's
/// estate that wrote it and the first [`crate::views::overview::EXPORTS_SHOWN`] of its exports, "and N more"
/// opening the full list in the dialog the Overview's "Delegated by" row opens.
#[component]
fn FoldersCard(folders: Vec<InterfaceFolder>) -> Element {
    let exports_open = use_signal(|| None::<DelegatedLine>);
    rsx! {
        Card { variant: CardVariant::Outlined, class: "interfaces__card",
            div { class: "interfaces__card-head",
                Icon { name: "folder_shared", size: 22 }
                h2 { class: "interfaces__name", "Interface folders" }
                span { class: "grow" }
                HelpButton { title: "Interface folders",
                    p { "Each interface folder this estate uses was written by an organization's estate and holds the values it exports for workloads. A pack reads them as ${{{{interface.<folder>.<value>}}}}, or by the value's name alone, and Details on a pack says which folder gives each read." }
                }
            }
            for line in folders.iter().map(folder_line) {
                {
                    let (named, more) = line.shown();
                    let all = line.clone();
                    let mut exports_open = exports_open;
                    rsx! {
                        div { key: "{line.folder}", class: "interfaces__export",
                            div { class: "interfaces__export-head",
                                code { class: "interfaces__export-name", "{line.folder}" }
                                span { class: "interfaces__supporting", "{line.said}" }
                            }
                            p { class: "interfaces__supporting",
                                if line.exports.is_empty() {
                                    "exports nothing"
                                } else {
                                    "exports {named}"
                                }
                                if more > 0 {
                                    " "
                                    Button {
                                        variant: ButtonVariant::Text,
                                        onclick: move |_| exports_open.set(Some(all.clone())),
                                        "and {more} more"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        ExportsDialog { open: exports_open }
    }
}

/// An interface folder as a [`DelegatedLine`]: its name, "written by <estate>", its
/// exports.
fn folder_line(folder: &InterfaceFolder) -> DelegatedLine {
    DelegatedLine {
        folder: folder.name.clone(),
        said: format!("written by {}", folder.estate),
        exports: folder.exports.clone(),
    }
}

/// One pack: its id, version, badge, where it comes from and what can be done. `row` is
/// satz's, once a listing names it, and `local` says that listing read no source; `used`
/// says the estate reads it; `file` is where it is, for a pack that is here.
#[component]
fn PackLine(
    id: String,
    row: Option<WorkloadPackRow>,
    local: bool,
    used: bool,
    file: Option<PathBuf>,
    edited: Option<UpdatedPack>,
    busy: bool,
    confirm: Signal<Option<Confirm>>,
    mut publishing: Signal<Option<String>>,
) -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let mut details_open = use_signal(|| false);
    let state = row.as_ref().map(|r| r.state);
    let unmet = row.as_ref().is_some_and(WorkloadPackRow::has_unmet_read);
    let use_path = row.as_ref().and_then(|r| r.use_path()).map(str::to_string);
    let op = |op: PackOp| move |_| handle.send(EstateAction::Packs(op.clone()));
    let one = Which::One(id.clone());
    rsx! {
        div { class: "interfaces__export",
            div { class: "interfaces__export-head",
                code { class: "interfaces__export-name", "{id}" }
                if let Some(row) = &row {
                    span { class: "interfaces__supporting", {versions(row)} }
                }
                if let Some((word, icon, error)) = state.and_then(|s| badge(s, local)) {
                    Chip { kind: ChipKind::Assist, icon, label: word, error }
                }
                if unmet {
                    Chip { kind: ChipKind::Assist, icon: "report", label: "Unmet read", error: true }
                }
                if row.is_some() {
                    Button { variant: ButtonVariant::Text, onclick: move |_| details_open.set(true), "Details" }
                }
                if let Some(origin) = row.as_ref().and_then(origin_text) {
                    code { class: "interfaces__at", "{origin}" }
                }
                span { class: "grow" }
                if state == Some(PackState::Available) {
                    Button {
                        variant: ButtonVariant::Text,
                        icon: "download",
                        disabled: busy,
                        onclick: op(PackOp::Get(one.clone())),
                        "Fetch"
                    }
                }
                if state == Some(PackState::Behind) {
                    Button {
                        variant: ButtonVariant::Text,
                        icon: "update",
                        disabled: busy,
                        onclick: op(PackOp::Update { which: one.clone(), from: None, take_upstream: false }),
                        "Update"
                    }
                }
                if !used {
                    if let Some(path) = use_path {
                        Button {
                            variant: ButtonVariant::Text,
                            icon: "add_link",
                            disabled: busy,
                            onclick: move |_| handle.send(EstateAction::CommitEdit(Edit::AddUse { path: path.clone() })),
                            "Add to the estate"
                        }
                    }
                }
                if state == Some(PackState::Edited) {
                    IconButton {
                        icon: "difference",
                        label: "Show the difference to the source's file",
                        disabled: busy,
                        onclick: op(PackOp::Update { which: one.clone(), from: None, take_upstream: false }),
                    }
                    IconButton {
                        icon: "cloud_download",
                        label: "Take the source's file…",
                        disabled: busy,
                        onclick: { let id = id.clone(); move |_| confirm.set(Some(Confirm::TakeUpstream(id.clone()))) },
                    }
                }
                if matches!(state, Some(PackState::Edited | PackState::Gone | PackState::Current | PackState::Behind | PackState::NotChecked)) {
                    IconButton {
                        icon: "link_off",
                        label: "Keep as own…",
                        disabled: busy,
                        onclick: { let id = id.clone(); move |_| confirm.set(Some(Confirm::Detach(id.clone()))) },
                    }
                }
                if state == Some(PackState::Own) {
                    IconButton {
                        icon: "drive_folder_upload",
                        label: "Store in a pack source…",
                        disabled: busy,
                        onclick: {
                            let id = id.clone();
                            move |_| {
                                app.estate().pack_publish().set(None);
                                publishing.set(Some(id.clone()));
                            }
                        },
                    }
                }
                if let Some(file) = file {
                    IconButton {
                        icon: "edit",
                        label: "Edit {id}.satz",
                        onclick: move |_| edit_file(app, &file),
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
        if let Some(row) = row.filter(|_| details_open()) {
            Dialog {
                open: true,
                title: "{id}",
                class: "wpacks__details",
                ondismiss: move |_| details_open.set(false),
                actions: rsx! {
                    Button { variant: ButtonVariant::Text, onclick: move |_| details_open.set(false), "Close" }
                },
                PackDetails { row }
            }
        }
    }
}

/// What a pack declares and reads, as satz's row gives it (satz's ADR 0100): its params,
/// its list params, its exports by interface, and each interface value it reads with what
/// that comes to against the open estate.
#[component]
fn PackDetails(row: WorkloadPackRow) -> Element {
    let groups = export_groups(&row.exports);
    rsx! {
        section { class: "wpacks__section",
            h3 { "Params" }
            if row.params.is_empty() {
                p { class: "interfaces__supporting", "none" }
            }
            ul { class: "wpacks__decls",
                for param in row.params.iter() {
                    li { key: "{param.name}",
                        code { "{param.name}" }
                        " — "
                        match &param.default {
                            Some(default) => rsx! { code { "{default}" } },
                            None => rsx! { span { class: "interfaces__supporting", "no default" } },
                        }
                        if param.question {
                            span { class: "interfaces__supporting",
                                match &param.prompt {
                                    Some(prompt) => format!(" — asked: {prompt}"),
                                    None => " — asked".to_string(),
                                }
                            }
                        }
                    }
                }
            }
        }
        section { class: "wpacks__section",
            h3 { "List params" }
            if row.list_params.is_empty() {
                p { class: "interfaces__supporting", "none" }
            }
            ul { class: "wpacks__decls",
                for list in row.list_params.iter() {
                    li { key: "{list.param}",
                        code { "{list.param}" }
                        " — key "
                        code { "{list.key}" }
                        {format!("; fields {}", list.fields.join(", "))}
                        if !list.defaults.is_empty() {
                            "; defaults "
                            {
                                list.defaults
                                    .iter()
                                    .map(|(field, value)| format!("{field} = {value}"))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            }
                        }
                    }
                }
            }
        }
        section { class: "wpacks__section",
            h3 { "Exports" }
            if groups.is_empty() {
                p { class: "interfaces__supporting", "none" }
            }
            ul { class: "wpacks__decls",
                for (interface, names) in groups {
                    li { key: "{interface:?}",
                        match &interface {
                            Some(name) => rsx! { "interface " code { "{name}" } },
                            None => rsx! { "core" },
                        }
                        {format!(": {}", names.join(", "))}
                    }
                }
            }
        }
        section { class: "wpacks__section",
            h3 { "Reads" }
            if row.reads.is_empty() {
                p { class: "interfaces__supporting", "none" }
            }
            ul { class: "wpacks__decls",
                for (i, read) in row.reads.iter().enumerate() {
                    {
                        let (status, error) = read_status(read);
                        rsx! {
                            li { key: "{i}",
                                code { "{read.written()}" }
                                " — "
                                span { class: if error { "wpacks__unmet" } else { "" }, "{status}" }
                            }
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
            commit: None,
            use_line: None,
            params: Vec::new(),
            list_params: Vec::new(),
            exports: Vec::new(),
            reads: Vec::new(),
        }
    }

    fn read(param: Option<&str>, bound: bool, met_by: &[&str], reason: Option<&str>) -> PackRead {
        PackRead {
            folder: Some("shop".into()),
            value: "project_id".into(),
            param: param.map(str::to_string),
            bound_by_estate: bound,
            met_by: met_by.iter().map(|f| f.to_string()).collect(),
            unmet_reason: reason.map(str::to_string),
        }
    }

    /// A read says the param the estate binds instead, the folders that give it, or satz's
    /// reason in the error color — and only the last earns the row its badge.
    #[test]
    fn a_read_says_what_it_comes_to_against_the_estate() {
        let bound = read(Some("budget_project_id"), true, &[], None);
        assert_eq!(
            read_status(&bound),
            ("set by the estate in budget_project_id".to_string(), false)
        );
        let met = read(None, false, &["shop", "vm"], None);
        assert_eq!(read_status(&met), ("from shop, vm".to_string(), false));
        let unmet = read(None, false, &[], Some("no folder shop"));
        assert_eq!(read_status(&unmet), ("no folder shop".to_string(), true));
        assert_eq!(unmet.written(), "shop.project_id");

        let mut r = row(PackState::Available, None, Some("1.0"));
        r.reads = vec![bound, met];
        assert!(!r.has_unmet_read());
        r.reads.push(unmet);
        assert!(r.has_unmet_read());
    }

    /// The core exports come first, then each interface in the order the pack names it.
    #[test]
    fn exports_are_grouped_by_interface_core_first() {
        let e = |interface: Option<&str>, name: &str| PackExport {
            interface: interface.map(str::to_string),
            name: name.into(),
        };
        let groups = export_groups(&[
            e(Some("shop"), "bucket"),
            e(None, "budget_name"),
            e(Some("ks-{each.id}"), "key"),
            e(Some("shop"), "region"),
            e(None, "budget_id"),
        ]);
        assert_eq!(
            groups,
            [
                (
                    None,
                    vec!["budget_name".to_string(), "budget_id".to_string()]
                ),
                (
                    Some("shop".to_string()),
                    vec!["bucket".to_string(), "region".to_string()]
                ),
                (Some("ks-{each.id}".to_string()), vec!["key".to_string()]),
            ]
        );
    }

    #[test]
    fn only_a_state_that_needs_attention_earns_a_badge() {
        for local in [false, true] {
            assert_eq!(
                badge(PackState::Behind, local).map(|b| b.0),
                Some("Update available")
            );
            assert_eq!(
                badge(PackState::Edited, local).map(|b| b.0),
                Some("Edited here")
            );
            assert_eq!(
                badge(PackState::Gone, local),
                Some(("Gone from source", "cloud_off", true))
            );
            for quiet in [PackState::Current, PackState::Own, PackState::Available] {
                assert_eq!(badge(quiet, local), None, "{quiet:?}");
            }
        }
        assert_eq!(
            badge(PackState::NotChecked, false).map(|b| b.0),
            Some("Not checked")
        );
    }

    /// The local listing read no source: its `not-checked` says the fetched file is the
    /// one that was fetched, not that another source was read.
    #[test]
    fn a_local_not_checked_pack_is_unchanged_since_fetched() {
        assert_eq!(
            badge(PackState::NotChecked, true),
            Some(("Unchanged since fetched", "history", false))
        );
    }

    /// An interface folder's line names the estate that wrote it and its exports, cut at
    /// the Overview's count with how many more.
    #[test]
    fn an_interface_folder_says_who_wrote_it_and_what_it_exports() {
        let folder = InterfaceFolder {
            name: "shop".into(),
            estate: "c0example".into(),
            exports: (1..=7).map(|i| format!("v{i}")).collect(),
        };
        let line = folder_line(&folder);
        assert_eq!(line.folder, "shop");
        assert_eq!(line.said, "written by c0example");
        assert_eq!(line.shown(), ("v1, v2, v3, v4, v5".to_string(), 2));
    }

    #[test]
    fn a_row_names_its_source_and_short_commit_or_says_it_is_own() {
        let mut fetched = row(PackState::Current, Some("1.0"), Some("1.0"));
        assert_eq!(origin_text(&fetched), None);
        fetched.source = Some("up".into());
        assert_eq!(origin_text(&fetched).as_deref(), Some("up"));
        fetched.source = Some("satz".into());
        fetched.commit = Some("0123456789abcdef0123456789abcdef01234567".into());
        assert_eq!(origin_text(&fetched).as_deref(), Some("satz @ 0123456"));
        let own = row(PackState::Own, Some("0.1"), None);
        assert_eq!(origin_text(&own).as_deref(), Some("Own pack"));
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

    /// The used packs lead in the order of their lines, with satz's row where a source
    /// was listed; the other packs here follow by id; an available one is no row here.
    #[test]
    fn the_used_packs_come_first_and_the_others_follow() {
        let with = |id: &str, state: PackState| WorkloadPackRow {
            id: id.into(),
            ..row(state, Some("1.0"), None)
        };
        let used = vec![
            ("zeta".to_string(), "packs/zeta.satz".to_string()),
            ("alpha".to_string(), "packs/alpha.satz".to_string()),
        ];
        let unlisted = here_rows(None, None, &used);
        assert_eq!(
            unlisted.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["zeta", "alpha"]
        );
        assert!(unlisted.iter().all(|r| r.row.is_none() && r.used.is_some()));

        let report = WorkloadPacksReport {
            packs_dir: "packs".into(),
            estate: "yaml/wp.satz".into(),
            interface_folders: Vec::new(),
            sources: Vec::new(),
            packs: vec![
                with("alpha", PackState::Own),
                with("beta", PackState::Behind),
                with("gamma", PackState::Available),
                with("aardvark", PackState::Current),
            ],
        };
        let rows = here_rows(Some(&report), None, &used);
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["zeta", "alpha", "aardvark", "beta"]
        );
        assert_eq!(rows[0].row, None, "a used pack satz did not list");
        assert_eq!(rows[1].row.as_ref().map(|r| r.state), Some(PackState::Own));
        assert_eq!(rows[3].used, None);
        assert!(rows.iter().all(|r| !r.local));
    }

    /// The local listing gives every pack here its row before a source is listed; once one
    /// is, its rows win for the packs it names and the local listing keeps the rest.
    #[test]
    fn the_local_listing_fills_the_rows_and_a_source_listing_wins() {
        let with = |id: &str, state: PackState| WorkloadPackRow {
            id: id.into(),
            ..row(state, Some("1.0"), None)
        };
        let report = |packs: Vec<WorkloadPackRow>| WorkloadPacksReport {
            packs_dir: "packs".into(),
            estate: "yaml/wp.satz".into(),
            interface_folders: Vec::new(),
            sources: Vec::new(),
            packs,
        };
        let used = vec![("alpha".to_string(), "packs/alpha.satz".to_string())];
        let local = report(vec![
            with("alpha", PackState::NotChecked),
            with("fresh", PackState::Own),
        ]);
        let rows = here_rows(None, Some(&local), &used);
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["alpha", "fresh"]
        );
        assert!(rows.iter().all(|r| r.local && r.row.is_some()));

        let source = report(vec![
            with("alpha", PackState::Behind),
            with("offered", PackState::Available),
        ]);
        let rows = here_rows(Some(&source), Some(&local), &used);
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["alpha", "fresh"]
        );
        assert_eq!(
            rows[0].row.as_ref().map(|r| r.state),
            Some(PackState::Behind)
        );
        assert!(!rows[0].local, "the source's row wins");
        assert!(
            rows[1].local,
            "a pack the source listing does not name keeps its local row"
        );
    }
}
