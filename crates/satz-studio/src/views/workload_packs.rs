//! The Packs view's pane: the estate's pack sources ([`PackSourcesCard`]), then the packs
//! of the estate's `packs_dir` — first every one the estate `use`s, read from its own `use`
//! lines, then the others. `satz packs --local`, which reads no pack source, gives each its
//! state and declarations as soon as the view stands; once a source is listed with `satz
//! packs --source <name>`, its rows win, saying how each pack stands against it, and what
//! the source holds beyond them is listed too. A fetch is
//! `satz get-pack --source`, an update `satz update-pack` from each pack's recorded
//! source, and a pack edited here is shown with its difference and written only when the
//! operator says so (ADR 0024). The app writes no `use` line (ADR 0038): a pack here that
//! the estate does not use shows the line satz names for it, to copy and write into the
//! estate file ([`UseLine`]). A new pack is the repository's own: written, formatted and
//! opened in the Editor setting's command, as Edit opens any pack (ADR 0031, ADR 0033);
//! once it works, "Store in a pack source…" copies it into a folder source with `satz
//! publish-pack` ([`PublishDialog`]). Each listed pack — fetched or only available — expands to what flows
//! through it ([`PackFlow`]): the interface values it reads, grouped by what gives each
//! against the open estate as `satz packs --estate` matched it (satz's ADR 0100), and the
//! values it exports, grouped by interface; a read the estate does not give marks the row
//! "Unmet read". Its details ([`PackDetails`]) hold its params and list params. When an external editor is done with a pack is
//! not knowable, so the card's Reload reads the estate and the listed source again. The
//! states and the matches are satz's report; the app derives none of its own.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use dioxus::prelude::*;
use satz_studio_core::cst::UseState;
use satz_studio_core::satz::reports::{
    InterfaceFolder, PackExport, PackState, QuestionRow, UpdateAction, UpdatePackReport,
    UpdatedPack, WorkloadPackRow, WorkloadPacksReport,
};
use satz_studio_core::satz::workload::{Which, new_pack_id};

use dioxus_icons::lucide::{
    ArrowDown, ArrowUp, Check, ChevronDown, ChevronUp, CircleAlert, CloudDownload, CloudOff,
    Copy as CopyIcon, Diff, Download, FilePlus, FolderInput, FolderUp, History, Hourglass, Package,
    Pencil, RefreshCw, SlidersHorizontal, Tag, TriangleAlert, Unlink,
};

use crate::components::badge::{Badge, BadgeVariant};
use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::card::{Card, CardContent, CardHeader};
use crate::components::collapsible::{Collapsible, CollapsibleContent, CollapsibleTrigger};
use crate::components::dialog::{Dialog, DialogTitle};
use crate::components::progress::Progress;
use crate::state::{
    AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, PackOp, SourceOp, edit_file,
};
use crate::views::commands::copy_to_clipboard;
use crate::views::pack_sources::{PackSourcesCard, Sheet};
use crate::views::publish_pack::PublishDialog;
use crate::widgets::{Field, HelpButton, Tip};

/// What gives a value a pack reads, as satz matched the read against the estate: the
/// group the expander lists it under.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Given {
    /// the interface folder that gives it — the first of `met_by` where several do
    Folder(String),
    /// the estate sets the param the read is the value of, so the read is not made
    Estate,
    /// the estate does not give it
    Unmet,
}

impl Given {
    /// The folders first, in the order the pack first reads from them, then the estate's,
    /// then what is not given.
    fn rank(&self) -> u8 {
        match self {
            Given::Folder(_) => 0,
            Given::Estate => 1,
            Given::Unmet => 2,
        }
    }
}

/// A param a read feeds, with the question that asks for it, where one does.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FedParam {
    name: String,
    prompt: Option<String>,
    why: Option<String>,
}

/// One value a pack reads, as the expander lists it: `folder.value` or `value`, the
/// description of the export that gives it, satz's reason where nothing does, and the
/// params it feeds.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Incoming {
    written: String,
    description: Option<String>,
    reason: Option<String>,
    params: Vec<FedParam>,
}

/// A pack's reads grouped by what gives them: one group per interface folder that meets
/// a read (a read several folders meet stands under the first), then the reads whose param
/// the estate sets, then the ones it does not give. A value read for several params is one
/// entry naming each.
fn incoming(row: &WorkloadPackRow) -> Vec<(Given, Vec<Incoming>)> {
    let mut groups: Vec<(Given, Vec<Incoming>)> = Vec::new();
    for read in &row.reads {
        let given = if read.bound_by_estate {
            Given::Estate
        } else if let Some(folder) = read.met_by.first() {
            Given::Folder(folder.clone())
        } else {
            Given::Unmet
        };
        let entries = match groups.iter_mut().find(|(g, _)| *g == given) {
            Some((_, entries)) => entries,
            None => {
                groups.push((given, Vec::new()));
                &mut groups.last_mut().expect("just pushed").1
            }
        };
        let written = read.written();
        let entry = match entries.iter_mut().position(|e| e.written == written) {
            Some(i) => &mut entries[i],
            None => {
                entries.push(Incoming {
                    written,
                    description: read.description.clone(),
                    reason: read.unmet_reason.clone().filter(|_| read.unmet()),
                    params: Vec::new(),
                });
                entries.last_mut().expect("just pushed")
            }
        };
        if let Some(name) = &read.param
            && entry.params.iter().all(|p| &p.name != name)
        {
            let declared = row.params.iter().find(|p| &p.name == name);
            entry.params.push(FedParam {
                name: name.clone(),
                prompt: declared.and_then(|p| p.prompt.clone()),
                why: declared.and_then(|p| p.why.clone()),
            });
        }
    }
    groups.sort_by_key(|(g, _)| g.rank());
    groups
}

/// How many values flow into a pack and out of it: the distinct values it reads, and its
/// exports.
fn flow_counts(row: &WorkloadPackRow) -> (usize, usize) {
    let mut read: Vec<String> = row.reads.iter().map(|r| r.written()).collect();
    read.sort();
    read.dedup();
    (read.len(), row.exports.len())
}

/// A pack's exports, grouped by the interface they stand in: the core exports first,
/// then each interface in the order the pack first names it.
fn export_groups(exports: &[PackExport]) -> Vec<(Option<String>, Vec<PackExport>)> {
    let mut groups: Vec<(Option<String>, Vec<PackExport>)> = Vec::new();
    for e in exports {
        match groups.iter_mut().find(|(i, _)| *i == e.interface) {
            Some((_, list)) => list.push(e.clone()),
            None => groups.push((e.interface.clone(), vec![e.clone()])),
        }
    }
    groups.sort_by_key(|(i, _)| i.is_some());
    groups
}

/// The badge a pack's state earns: the word, and whether it is an error. A pack that is
/// current, the repository's own or only available needs no word. `local` says the row is
/// `satz packs --local`'s, which read no source: its `not-checked` is a fetched file
/// unchanged since its fetch, not a pack of another source.
fn badge(state: PackState, local: bool) -> Option<(&'static str, bool)> {
    match state {
        PackState::Behind => Some(("Update available", false)),
        PackState::Edited => Some(("Edited here", false)),
        PackState::Gone => Some(("Gone from source", true)),
        PackState::NotChecked if local => Some(("Unchanged since fetched", false)),
        PackState::NotChecked => Some(("Not checked", false)),
        PackState::Own | PackState::Current | PackState::Available => None,
    }
}

/// What the badge a pack's state earns means, as its tooltip says it.
fn badge_tip(state: PackState, local: bool) -> &'static str {
    match state {
        PackState::Behind => {
            "The pack source has a newer version of this pack than the one in this estate's packs folder. Update takes it; a pack edited here is compared first."
        }
        PackState::Edited => {
            "The copy in this estate's packs folder differs from the version it was fetched as: it was changed here. An update shows the difference before it replaces anything."
        }
        PackState::Gone => {
            "The pack source no longer has this pack. The copy in this estate's packs folder stays as it is and gets no updates."
        }
        PackState::NotChecked if local => {
            "The copy in this estate's packs folder is the one fetched; this list read no pack source, so whether the source has a newer version is not known. Choose a source to compare."
        }
        PackState::NotChecked => {
            "Whether the pack source has a newer version of this pack was not checked."
        }
        PackState::Own | PackState::Current | PackState::Available => "",
    }
}

/// What "Unmet read" means for this pack: each value it reads from an interface folder
/// that the estate does not give, by the param it would fill, and what makes the pack
/// usable — the estate setting that param itself, or using an interface folder that
/// exports the value.
pub fn unmet_tip(row: &WorkloadPackRow) -> String {
    let unmet: Vec<String> = row
        .reads
        .iter()
        .filter(|r| r.unmet())
        .map(|r| match &r.param {
            Some(p) => format!("`{}` (for `{p}`)", r.value),
            None => format!("`{}`", r.value),
        })
        .collect();
    let params: Vec<String> = row
        .reads
        .iter()
        .filter(|r| r.unmet())
        .filter_map(|r| r.param.as_ref().map(|p| format!("`{p}`")))
        .collect();
    let fix = if params.is_empty() {
        "use an interface folder that exports it".to_string()
    } else {
        format!(
            "set {} in this estate's params, or use an interface folder that exports it",
            params.join(", ")
        )
    };
    format!(
        "This pack reads {} from an interface folder, and no interface folder this estate uses exports it. Used here, the transpile refuses it until you {fix}. The arrows list each value and why.",
        unmet.join(", ")
    )
}

/// The Lucide icon of the badge a pack's state earns, `size` pixels square.
fn badge_icon(state: PackState, local: bool, size: u32) -> Element {
    match state {
        PackState::Behind => rsx! { RefreshCw { size } },
        PackState::Edited => rsx! { Pencil { size } },
        PackState::Gone => rsx! { CloudOff { size } },
        PackState::NotChecked if local => rsx! { History { size } },
        _ => rsx! { Hourglass { size } },
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
/// where it does, the names it is used under, satz's row for it once a listing names it,
/// and whether that row is the local listing's.
#[derive(Debug, Clone, PartialEq)]
struct HereRow {
    id: String,
    used: Option<String>,
    /// the names of the named uses that read the pack, `use "…" named <name>`, in the
    /// order of their lines (satz's ADR 0109); empty where only a flat line uses it
    named: Vec<String>,
    row: Option<WorkloadPackRow>,
    local: bool,
}

/// The packs of `packs_dir`: every one the estate uses, in the order of its `use` lines,
/// then the others a listing names, by id. A pack's row is the source listing's where it
/// names the pack, else the local listing's. `used` is each active line's pack id, its
/// path and, for a named use, its name: a pack used under several names is one row that
/// carries them all.
fn here_rows(
    source: Option<&WorkloadPacksReport>,
    local: Option<&WorkloadPacksReport>,
    used: &[(String, String, Option<String>)],
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
    for (id, path, named) in used {
        match rows.iter_mut().find(|r| &r.id == id) {
            Some(r) => r.named.extend(named.clone()),
            None => {
                let found = row_of(id);
                rows.push(HereRow {
                    id: id.clone(),
                    used: Some(path.clone()),
                    named: named.iter().cloned().collect(),
                    local: found.as_ref().is_some_and(|(_, l)| *l),
                    row: found.map(|(p, _)| p),
                });
            }
        }
    }
    let mut others: Vec<HereRow> = listed
        .iter()
        .filter(|(p, _)| used.iter().all(|(id, _, _)| id != &p.id))
        .map(|(p, is_local)| HereRow {
            id: p.id.clone(),
            used: None,
            named: Vec::new(),
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

/// Each pack's description by its id, as the questions report carries it: satz's pack
/// listing has no description, and the questions report has one for each pack the estate
/// uses that asks a question (`pack_description`, its header's first paragraph). `id_of`
/// is the workload pack a question's `from` — the path its `use` line reads — names, `None`
/// for a file outside `packs_dir`. A pack that asks nothing, or that the estate does not
/// use, has none here.
fn pack_descriptions(
    questions: &[QuestionRow],
    id_of: impl Fn(&str) -> Option<String>,
) -> BTreeMap<String, String> {
    let mut seen = BTreeSet::new();
    let mut out = BTreeMap::new();
    for q in questions.iter().filter(|q| !q.pack_description.is_empty()) {
        if !seen.insert(q.from.as_str()) {
            continue;
        }
        if let Some(id) = id_of(&q.from) {
            out.entry(id).or_insert_with(|| q.pack_description.clone());
        }
    }
    out
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
    // the packs the estate uses: its active `use` lines that read a file of packs_dir,
    // each with the name of a named use
    let used: Vec<(String, String, Option<String>)> = app
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
                    Some((id, u.path, u.named))
                })
                .collect()
        })
        .unwrap_or_default();
    let descriptions = app
        .estate()
        .questions()
        .read()
        .as_ref()
        .map(|r| {
            pack_descriptions(&r.questions, |from| {
                open.session.dir.pack_of_use(&open.main, from)
            })
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
    // the folders are the estate's, the same in either listing: a pack's expander names
    // the estate that wrote each
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
                Progress { value: None::<f64>, "aria-label": "working" }
            }
            PackSourcesCard {
                busy,
                sheet,
                confirm_remove: move |name: String| confirm.set(Some(Confirm::RemoveSource(name))),
            }
            HereCard {
                rows,
                descriptions,
                folders: folders.clone(),
                listed: listed.is_some(),
                local_listed: local.is_some(),
                failed: failed.or(local_failed),
                packs_dir: open.session.dir.tool.packs_dir.clone(),
                files: packs_dir,
                estate_file: open.main.clone(),
                update,
                busy,
                confirm,
                publishing,
            }
            if let Some(report) = listed.filter(|_| !available.is_empty()) {
                Card { class: "interfaces__card",
                    CardHeader {
                        div { class: "interfaces__card-head",
                            CloudDownload { size: 22 }
                            h2 { class: "interfaces__name", "Available" }
                            code { class: "interfaces__at", "{read_from(&report)}" }
                        }
                    }
                    CardContent {
                        for row in available {
                            PackLine { key: "{row.id}", id: row.id.clone(), row: Some(row.clone()), folders: folders.clone(), local: false, used: false, named: Vec::new(), description: None, file: None, estate_file: open.main.clone(), edited: None, busy, confirm, publishing }
                        }
                    }
                }
            }
        }
        PublishDialog { publishing, sheet, busy }
        Dialog {
            open: confirm().is_some(),
            on_open_change: move |now: bool| {
                if !now {
                    confirm.set(None);
                }
            },
            DialogTitle {
                match confirm() {
                    Some(Confirm::TakeUpstream(_)) => "Overwrite your changes?",
                    Some(Confirm::Detach(_)) => "Keep it as your own pack?",
                    Some(Confirm::RemoveSource(_)) => "Remove the pack source?",
                    None => "",
                }
            }
            div { class: "dialog-body",
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
            div { class: "dialog-actions",
                Button { variant: ButtonVariant::Ghost, onclick: move |_| confirm.set(None), "Cancel" }
                Button {
                    variant: match confirm() {
                        Some(Confirm::TakeUpstream(_) | Confirm::RemoveSource(_)) => ButtonVariant::Destructive,
                        _ => ButtonVariant::Primary,
                    },
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
            }
        }
    }
}

/// The packs of `packs_dir`: the used ones first, each with what can be done; "New
/// pack…" in its head, and "Update all" while a pack has an update available.
#[component]
fn HereCard(
    rows: Vec<HereRow>,
    descriptions: BTreeMap<String, String>,
    folders: Vec<InterfaceFolder>,
    listed: bool,
    local_listed: bool,
    failed: Option<String>,
    packs_dir: String,
    files: PathBuf,
    estate_file: PathBuf,
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
        Card { class: "interfaces__card",
            CardHeader {
            div { class: "interfaces__card-head",
                Package { size: 22 }
                h2 { class: "interfaces__name", "In this estate" }
                code { class: "interfaces__at", "{packs_dir}" }
                span { class: "grow" }
                if behind {
                    Button {
                        variant: ButtonVariant::Secondary,
                        disabled: busy,
                        onclick: move |_| handle.send(EstateAction::Packs(PackOp::Update { which: Which::All, from: None, take_upstream: false })),
                        RefreshCw { size: 16 }
                        "Update all"
                    }
                }
                Button {
                    variant: ButtonVariant::Ghost,
                    disabled: busy,
                    onclick: move |_| {
                        name.set(String::new());
                        creating.set(true);
                    },
                    FilePlus { size: 16 }
                    "New pack…"
                }
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Icon,
                    title: "Reload",
                    "aria-label": "Reload",
                    disabled: busy,
                    onclick: move |_| {
                        handle.send(EstateAction::Reload);
                        handle.send(EstateAction::Packs(match app.estate().pack_source().cloned() {
                            Some(source) => PackOp::List(source),
                            None => PackOp::ListLocal,
                        }));
                    },
                    RefreshCw { size: 16 }
                }
                HelpButton { title: "Workload packs",
                    p { "The packs this estate uses come first, then the other files of {packs_dir}. A fetched pack is recorded in origin.toml beside the packs with the name of its source; a pack without a row there is this repository's own, and no fetch and no update writes it." }
                    p { "Choosing a pack source runs satz packs against it, and each fetched pack says how it stands: Update available — unchanged here, and the source holds a newer file; Edited here — changed since it was fetched, so an update leaves it as it is and shows the difference; Gone from source — the source holds it no longer; Not checked — fetched from another source than the one listed." }
                    p { "Before a source is chosen, satz packs --local lists the packs here without reading any source, so nothing goes over the network: Edited here — changed since it was fetched; Unchanged since fetched — the file is the one that was fetched, and whether its source holds a newer one is known once that source is chosen." }
                    p { "The arrows on a pack count what flows through it, seen from the pack: in, the values it reads from the interface folders this estate uses; out, the values it exports. Opening them lists both. In is grouped by what gives each value — an interface folder, named with the organization's estate that wrote it, the estate itself where it sets the param the value would fill, or not given, with why — each value with its export's description and the param it fills with that param's question. Out is grouped by interface, the core exports first, each with its description. Unmet read marks a pack with a value this estate does not give; the transpile refuses it once the estate uses the pack." }
                    p { "Details shows the rest of what a pack declares: its params with their defaults and the questions that ask for them, and its list params." }
                    p { "Reload reads the estate again, the packs here and the listed pack source with them: the way to see a pack after New pack or Edit, since the window cannot tell when the editor is done with the file." }
                    p { "New pack writes {packs_dir}/<name>.satz and opens it in the editor Settings → Editor names, or this computer's text editor; Edit opens a pack the same way. satz-studio writes no use line: a pack here that the estate does not use shows the line that reads it, to copy and write into the estate file by hand — for a pack used under a name, its line ends in named <name>, and <name> is replaced by the use's name. Keep as own forgets where a fetched pack came from. What a pack asks is answered in Decisions." }
                    p { "Store in a pack source, on your own pack, copies it into a folder pack source with satz publish-pack once it clears review-pack's bar inside this estate, and records it as fetched from there, so other estates fetch it and update-pack follows it here too." }
                }
            }
            }
            CardContent {
            if let Some(e) = &failed {
                div { class: "interfaces__error",
                    CircleAlert { size: 20 }
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
                    named: here.named.clone(),
                    description: descriptions.get(&here.id).cloned(),
                    edited: edited_of(&here.id),
                    file: Some(files.join(format!("{}.satz", here.id))),
                    estate_file: estate_file.clone(),
                    row: here.row,
                    folders: folders.clone(),
                    local: here.local,
                    busy,
                    confirm,
                    publishing,
                }
            }
            }
        }
        Dialog {
            open: creating(),
            on_open_change: move |now: bool| creating.set(now),
            class: "interfaces__add",
            DialogTitle { "New pack" }
            div { class: "dialog-body wpacks__form",
                Field {
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
            div { class: "dialog-actions",
                Button { variant: ButtonVariant::Ghost, onclick: move |_| creating.set(false), "Cancel" }
                Button {
                    disabled: busy || checked.is_err(),
                    onclick: move |_| {
                        handle.send(EstateAction::NewPack(name().trim().to_string()));
                        creating.set(false);
                    },
                    Check { size: 16 }
                    "Create"
                }
            }
        }
    }
}

/// One pack: its id, version, badge, where it comes from and what can be done. `row` is
/// satz's, once a listing names it, and `local` says that listing read no source; `used`
/// says the estate reads it, and `named` the names of the named uses that read it — a
/// badge each; `file` is where it is, for a pack that is here; `estate_file` is the
/// estate's main file, which an unused pack's line is written into ([`UseLine`]);
/// `folders` are the interface folders the estate uses, which the expander names.
#[component]
fn PackLine(
    id: String,
    row: Option<WorkloadPackRow>,
    folders: Vec<InterfaceFolder>,
    local: bool,
    used: bool,
    named: Vec<String>,
    description: Option<String>,
    file: Option<PathBuf>,
    estate_file: PathBuf,
    edited: Option<UpdatedPack>,
    busy: bool,
    confirm: Signal<Option<Confirm>>,
    mut publishing: Signal<Option<String>>,
) -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let mut details_open = use_signal(|| false);
    let mut flow_open = use_signal(|| false);
    let mut about_open = use_signal(|| false);
    let state = row.as_ref().map(|r| r.state);
    let unmet = row.as_ref().is_some_and(WorkloadPackRow::has_unmet_read);
    let use_line = row
        .as_ref()
        .and_then(|r| r.use_line.clone())
        .filter(|_| !used);
    let op = |op: PackOp| move |_| handle.send(EstateAction::Packs(op.clone()));
    let one = Which::One(id.clone());
    rsx! {
        Collapsible {
            class: "interfaces__export",
            open: flow_open(),
            on_open_change: move |now: bool| flow_open.set(now),
            div { class: "interfaces__export-head",
                if let Some(description) = description {
                    Collapsible {
                        class: "wpacks__about",
                        open: about_open(),
                        on_open_change: move |now: bool| about_open.set(now),
                        div { class: "wpacks__toggle",
                            CollapsibleTrigger {
                                r#as: {
                                    let id = id.clone();
                                    move |mut trigger: Vec<Attribute>| {
                                        let title = if about_open() { "Hide what the pack is for" } else { "Show what the pack is for" };
                                        trigger.push(Attribute::new("title", title, None, false));
                                        rsx! {
                                            Button {
                                                variant: ButtonVariant::Ghost,
                                                size: ButtonSize::Sm,
                                                attributes: trigger,
                                                code { class: "interfaces__export-name", "{id}" }
                                                if about_open() {
                                                    ChevronUp { size: 16 }
                                                } else {
                                                    ChevronDown { size: 16 }
                                                }
                                            }
                                        }
                                    }
                                },
                            }
                        }
                        CollapsibleContent {
                            p { class: "wpacks__description", "{description}" }
                        }
                    }
                } else {
                    code { class: "interfaces__export-name", "{id}" }
                }
                if let Some(row) = &row {
                    span { class: "interfaces__supporting", {versions(row)} }
                }
                for name in named {
                    Tip { key: "{name}", text: format!("The estate uses this pack under the name {name}: its params are bound as {name}.<param>, and what it creates is addressed {name}__<label>."),
                        Badge { variant: BadgeVariant::Outline,
                            Tag { size: 12 }
                            "named {name}"
                        }
                    }
                }
                if let Some(s) = state
                    && let Some((word, error)) = badge(s, local)
                {
                    Tip { text: badge_tip(s, local).to_string(),
                        Badge { variant: if error { BadgeVariant::Destructive } else { BadgeVariant::Outline },
                            {badge_icon(s, local, 12)}
                            "{word}"
                        }
                    }
                }
                if unmet && let Some(row) = &row {
                    Tip { text: unmet_tip(row),
                        Badge { variant: BadgeVariant::Destructive,
                            TriangleAlert { size: 12 }
                            "Unmet read"
                        }
                    }
                }
                if let Some((into, out)) = row.as_ref().map(flow_counts) {
                    div { class: "wpacks__toggle",
                    CollapsibleTrigger {
                        r#as: move |mut trigger: Vec<Attribute>| {
                            let title = if flow_open() { "Hide what the pack reads and exports" } else { "Show what the pack reads and exports" };
                            trigger.push(Attribute::new("title", title, None, false));
                            rsx! {
                            Button {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Sm,
                                attributes: trigger,
                                ArrowDown { size: 16 }
                                span { "{into} in" }
                                span { "·" }
                                ArrowUp { size: 16 }
                                span { "{out} out" }
                                if flow_open() {
                                    ChevronUp { size: 16 }
                                } else {
                                    ChevronDown { size: 16 }
                                }
                            }
                            }
                        },
                    }
                    }
                }
                if row.is_some() {
                    Button { variant: ButtonVariant::Ghost, size: ButtonSize::Sm, onclick: move |_| details_open.set(true), "Details" }
                }
                if let Some(origin) = row.as_ref().and_then(origin_text) {
                    code { class: "interfaces__at", "{origin}" }
                }
                span { class: "grow" }
                if state == Some(PackState::Available) {
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Sm,
                        disabled: busy,
                        onclick: op(PackOp::Get(one.clone())),
                        Download { size: 16 }
                        "Fetch"
                    }
                }
                if state == Some(PackState::Behind) {
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Sm,
                        disabled: busy,
                        onclick: op(PackOp::Update { which: one.clone(), from: None, take_upstream: false }),
                        RefreshCw { size: 16 }
                        "Update"
                    }
                }
                if state == Some(PackState::Edited) {
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Icon,
                        title: "Show the difference to the source's file",
                        "aria-label": "Show the difference to the source's file",
                        disabled: busy,
                        onclick: op(PackOp::Update { which: one.clone(), from: None, take_upstream: false }),
                        Diff { size: 16 }
                    }
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Icon,
                        title: "Take the source's file…",
                        "aria-label": "Take the source's file…",
                        disabled: busy,
                        onclick: { let id = id.clone(); move |_| confirm.set(Some(Confirm::TakeUpstream(id.clone()))) },
                        CloudDownload { size: 16 }
                    }
                }
                if matches!(state, Some(PackState::Edited | PackState::Gone | PackState::Current | PackState::Behind | PackState::NotChecked)) {
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Icon,
                        title: "Keep as own…",
                        "aria-label": "Keep as own…",
                        disabled: busy,
                        onclick: { let id = id.clone(); move |_| confirm.set(Some(Confirm::Detach(id.clone()))) },
                        Unlink { size: 16 }
                    }
                }
                if state == Some(PackState::Own) {
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Icon,
                        title: "Store in a pack source…",
                        "aria-label": "Store in a pack source…",
                        disabled: busy,
                        onclick: {
                            let id = id.clone();
                            move |_| {
                                app.estate().pack_publish().set(None);
                                publishing.set(Some(id.clone()));
                            }
                        },
                        FolderUp { size: 16 }
                    }
                }
                if let Some(file) = file {
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Icon,
                        title: "Edit {id}.satz",
                        "aria-label": "Edit {id}.satz",
                        onclick: move |_| edit_file(app, &file),
                        Pencil { size: 16 }
                    }
                }
            }
            CollapsibleContent {
                if let Some(row) = row.as_ref().filter(|_| flow_open()) {
                    PackFlow { row: row.clone(), folders: folders.clone() }
                }
            }
            if let Some(line) = use_line {
                UseLine { line, estate_file }
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
                on_open_change: move |now: bool| details_open.set(now),
                class: "wpacks__details",
                DialogTitle { "{id}" }
                div { class: "dialog-body", PackDetails { row } }
                div { class: "dialog-actions",
                    Button { variant: ButtonVariant::Ghost, onclick: move |_| details_open.set(false), "Close" }
                }
            }
        }
    }
}

/// The line that reads a pack the estate does not use, as satz names it, for the
/// estate's author to write into the estate file: the app writes no `use` line (ADR
/// 0038). A pack whose header says `multiple` is used under a name, and its line ends in
/// `named <name>` ([`takes_a_name`]). "Copy" puts the line on the clipboard; the pencil
/// opens the estate file in the Editor setting's command, as Edit opens a pack.
#[component]
fn UseLine(line: String, estate_file: PathBuf) -> Element {
    let app = use_context::<Store<AppStore>>();
    let file_name = estate_file.file_name().map_or_else(
        || estate_file.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let say = if takes_a_name(&line) {
        format!(
            "Write this line into the estate file {file_name}, and replace <name> with the use's name:"
        )
    } else {
        format!("Write this line into the estate file {file_name}:")
    };
    rsx! {
        div { class: "wpacks__use",
            p { class: "interfaces__supporting", "{say}" }
            div { class: "wpacks__actions",
                code { class: "wpacks__use-line", "{line}" }
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    onclick: {
                        let line = line.clone();
                        move |_| copy_to_clipboard(app, &line)
                    },
                    CopyIcon { size: 16 }
                    "Copy"
                }
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Icon,
                    title: "Edit {file_name}",
                    "aria-label": "Edit {file_name}",
                    onclick: move |_| edit_file(app, &estate_file),
                    Pencil { size: 16 }
                }
            }
        }
    }
}

/// Whether a `use` line is one satz names for a pack used under a name: it ends in
/// `named <name>`, the placeholder the estate's author replaces.
fn takes_a_name(line: &str) -> bool {
    line.ends_with(" named <name>")
}

/// What flows through a pack, seen from the pack: what it reads, grouped by what gives
/// each value against the open estate ([`incoming`]), and what it exports, grouped by
/// interface ([`export_groups`]). The panel under the row its toggle opens.
#[component]
fn PackFlow(row: WorkloadPackRow, folders: Vec<InterfaceFolder>) -> Element {
    let into = incoming(&row);
    let out = export_groups(&row.exports);
    let wrote = |folder: &str| {
        folders
            .iter()
            .find(|f| f.name == folder)
            .map(|f| format!("written by {}", f.estate))
    };
    rsx! {
        div { class: "wpacks__flow",
            section { class: "wpacks__flow-side",
                h3 { ArrowDown { size: 18 } "Incoming" }
                if into.is_empty() {
                    p { class: "interfaces__supporting", "nothing" }
                }
                for (given, entries) in into {
                    div { key: "{given:?}", class: "wpacks__flow-group",
                        match &given {
                            Given::Folder(folder) => rsx! {
                                div { class: "wpacks__flow-source",
                                    FolderInput { size: 18 }
                                    code { "{folder}" }
                                    if let Some(said) = wrote(folder) {
                                        span { class: "interfaces__supporting", "{said}" }
                                    }
                                }
                            },
                            Given::Estate => rsx! {
                                div { class: "wpacks__flow-source",
                                    SlidersHorizontal { size: 18 }
                                    "set by the estate"
                                }
                            },
                            Given::Unmet => rsx! {
                                div { class: "wpacks__flow-source wpacks__unmet",
                                    TriangleAlert { size: 18 }
                                    "not given"
                                }
                            },
                        }
                        ul { class: "wpacks__flow-list",
                            for entry in entries {
                                li { key: "{entry.written}",
                                    div { class: "wpacks__flow-line",
                                        code { "{entry.written}" }
                                        if let Some(d) = &entry.description {
                                            span { class: "interfaces__supporting", "{d}" }
                                        }
                                    }
                                    if let Some(reason) = &entry.reason {
                                        p { class: "wpacks__flow-why wpacks__unmet", "{reason}" }
                                    }
                                    for param in entry.params.iter() {
                                        div { key: "{param.name}", class: "wpacks__flow-line",
                                            "→ "
                                            code { "{param.name}" }
                                            if let Some(prompt) = &param.prompt {
                                                span { "{prompt}" }
                                            }
                                        }
                                        if let Some(why) = &param.why {
                                            p { class: "wpacks__flow-why interfaces__supporting", "{why}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            section { class: "wpacks__flow-side",
                h3 { ArrowUp { size: 18 } "Outgoing" }
                if out.is_empty() {
                    p { class: "interfaces__supporting", "nothing" }
                }
                for (interface, exports) in out {
                    div { key: "{interface:?}", class: "wpacks__flow-group",
                        div { class: "wpacks__flow-source",
                            ArrowUp { size: 18 }
                            match &interface {
                                Some(name) => rsx! { "interface " code { "{name}" } },
                                None => rsx! { "core" },
                            }
                        }
                        ul { class: "wpacks__flow-list",
                            for export in exports {
                                li { key: "{export.name}",
                                    div { class: "wpacks__flow-line",
                                        code { "{export.name}" }
                                        if let Some(d) = &export.description {
                                            span { class: "interfaces__supporting", "{d}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// What a pack declares beyond what flows through it, as satz's row gives it (satz's ADR
/// 0100): its params, each with the question that asks for it, and its list params.
#[component]
fn PackDetails(row: WorkloadPackRow) -> Element {
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
                            None => rsx! { span { class: "interfaces__supporting", "none — must be provided" } },
                        }
                        if param.question {
                            span { class: "interfaces__supporting",
                                match &param.prompt {
                                    Some(prompt) => format!(" — asked: {prompt}"),
                                    None => " — asked".to_string(),
                                }
                            }
                        }
                        if let Some(why) = &param.why {
                            p { class: "wpacks__flow-why interfaces__supporting", "{why}" }
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
    }
}

#[cfg(test)]
mod tests {
    use satz_studio_core::satz::reports::{PackParam, PackRead};

    use super::*;

    fn question(subject: &str, from: &str, description: &str) -> QuestionRow {
        serde_json::from_value(serde_json::json!({
            "subject": subject, "kind": "param", "prompt": "p", "reversal": "edit", "blast": "low",
            "state": "unanswered", "blocking": false, "pack_description": description,
            "from": from, "pack": "p"
        }))
        .unwrap()
    }

    #[test]
    fn a_pack_description_comes_from_the_questions_of_the_pack_the_use_line_reads() {
        let questions = vec![
            question(
                "a",
                "packs/rag-store-workload.satz",
                "The RAG store, for one workload.",
            ),
            question(
                "b",
                "packs/rag-store-workload.satz",
                "The RAG store, for one workload.",
            ),
            question("c", "packs/quiet.satz", ""),
            question("d", "presets/estate-core.satz", "The bootstrap."),
        ];
        let id_of = |from: &str| {
            from.strip_prefix("packs/")
                .and_then(|f| f.strip_suffix(".satz"))
                .map(str::to_string)
        };
        let got = pack_descriptions(&questions, id_of);
        assert_eq!(
            got.get("rag-store-workload").map(String::as_str),
            Some("The RAG store, for one workload.")
        );
        // a pack whose header says nothing has no description, and a file outside
        // packs_dir is no workload pack
        assert_eq!(got.len(), 1);
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
            params: Vec::new(),
            list_params: Vec::new(),
            exports: Vec::new(),
            reads: Vec::new(),
        }
    }

    fn read(
        written: (Option<&str>, &str),
        param: Option<&str>,
        bound: bool,
        met_by: &[&str],
        reason: Option<&str>,
        description: Option<&str>,
    ) -> PackRead {
        PackRead {
            folder: written.0.map(str::to_string),
            value: written.1.into(),
            param: param.map(str::to_string),
            bound_by_estate: bound,
            met_by: met_by.iter().map(|f| f.to_string()).collect(),
            unmet_reason: reason.map(str::to_string),
            description: description.map(str::to_string),
        }
    }

    /// "Unmet read" says which value is not given, for which param, and how the estate
    /// gives it; a read the estate gives or binds is not named.
    #[test]
    fn the_unmet_read_tip_names_the_value_the_param_and_the_way_out() {
        let mut r = row(PackState::Available, Some("1.0"), None);
        r.reads = vec![
            read(
                (None, "delegated_folder"),
                Some("workload_project_parent"),
                false,
                &[],
                Some("no used interface folder has a value `delegated_folder`"),
                None,
            ),
            read(
                (None, "project_id"),
                Some("rag_project_id"),
                true,
                &[],
                None,
                None,
            ),
        ];
        let tip = unmet_tip(&r);
        assert!(
            tip.contains("`delegated_folder` (for `workload_project_parent`)"),
            "{tip}"
        );
        assert!(
            tip.contains("set `workload_project_parent` in this estate's params"),
            "{tip}"
        );
        assert!(!tip.contains("rag_project_id"), "{tip}");
        // every badge a state earns has a tooltip, and a state with no badge has none
        for s in [
            PackState::Behind,
            PackState::Edited,
            PackState::Gone,
            PackState::NotChecked,
            PackState::Own,
            PackState::Current,
            PackState::Available,
        ] {
            for local in [true, false] {
                assert_eq!(
                    badge(s, local).is_some(),
                    !badge_tip(s, local).is_empty(),
                    "{s:?} {local}"
                );
            }
        }
    }

    fn param(name: &str, prompt: Option<&str>, why: Option<&str>) -> PackParam {
        PackParam {
            name: name.into(),
            default: None,
            question: prompt.is_some(),
            prompt: prompt.map(str::to_string),
            why: why.map(str::to_string),
        }
    }

    /// The reads stand under the first folder that meets each, in the order the pack
    /// first reads from it, then under the estate where it sets the param, then under not
    /// given with satz's reason; a value read for two params is one entry naming both,
    /// each with its question's prompt and why. Only a value not given earns "Unmet read".
    #[test]
    fn the_reads_are_grouped_by_what_gives_them() {
        let mut r = row(PackState::Available, None, Some("1.0"));
        r.params = vec![
            param(
                "parent",
                Some("The folder it goes in"),
                Some("Policies follow it"),
            ),
            param("other_parent", None, None),
        ];
        r.reads = vec![
            read((Some("vm"), "zone"), None, false, &["vm"], None, None),
            read(
                (None, "delegated_folder"),
                Some("parent"),
                false,
                &["shop", "vm"],
                None,
                Some("The folder"),
            ),
            read((None, "billing"), Some("billing"), true, &[], None, None),
            read(
                (None, "delegated_folder"),
                Some("other_parent"),
                false,
                &["shop", "vm"],
                None,
                Some("The folder"),
            ),
            read(
                (Some("gone"), "x"),
                None,
                false,
                &[],
                Some("no folder gone"),
                None,
            ),
        ];
        assert!(r.has_unmet_read());
        let groups = incoming(&r);
        assert_eq!(
            groups.iter().map(|(g, _)| g.clone()).collect::<Vec<_>>(),
            [
                Given::Folder("vm".into()),
                Given::Folder("shop".into()),
                Given::Estate,
                Given::Unmet,
            ]
        );
        assert_eq!(groups[0].1[0].written, "vm.zone");
        let folder = &groups[1].1;
        assert_eq!(folder.len(), 1, "one entry for the value read twice");
        assert_eq!(folder[0].written, "delegated_folder");
        assert_eq!(folder[0].description.as_deref(), Some("The folder"));
        assert_eq!(
            folder[0].params,
            [
                FedParam {
                    name: "parent".into(),
                    prompt: Some("The folder it goes in".into()),
                    why: Some("Policies follow it".into()),
                },
                FedParam {
                    name: "other_parent".into(),
                    prompt: None,
                    why: None,
                },
            ]
        );
        assert_eq!(groups[2].1[0].written, "billing");
        assert_eq!(groups[2].1[0].params[0].name, "billing");
        assert_eq!(groups[2].1[0].reason, None);
        assert_eq!(groups[3].1[0].written, "gone.x");
        assert_eq!(groups[3].1[0].reason.as_deref(), Some("no folder gone"));

        r.reads.pop();
        assert!(!r.has_unmet_read());
    }

    /// In counts the distinct values a pack reads, out its exports; a pack with neither
    /// has no group on either side.
    #[test]
    fn the_counts_are_the_distinct_reads_and_the_exports() {
        let mut r = row(PackState::Own, Some("1.0"), None);
        assert_eq!(flow_counts(&r), (0, 0));
        assert!(incoming(&r).is_empty() && export_groups(&r.exports).is_empty());
        r.reads = vec![
            read((None, "a"), Some("p"), false, &["shop"], None, None),
            read((None, "a"), Some("q"), false, &["shop"], None, None),
            read((Some("shop"), "a"), None, false, &["shop"], None, None),
        ];
        r.exports = vec![
            PackExport {
                interface: None,
                name: "x".into(),
                description: None,
            };
            3
        ];
        assert_eq!(flow_counts(&r), (2, 3));
    }

    /// The core exports come first, then each interface in the order the pack names it,
    /// each export with its description.
    #[test]
    fn exports_are_grouped_by_interface_core_first() {
        let e = |interface: Option<&str>, name: &str| PackExport {
            interface: interface.map(str::to_string),
            name: name.into(),
            description: Some(format!("the {name}")),
        };
        let groups = export_groups(&[
            e(Some("shop"), "bucket"),
            e(None, "budget_name"),
            e(Some("ks-{each.id}"), "key"),
            e(Some("shop"), "region"),
            e(None, "budget_id"),
        ]);
        let names = |list: &[PackExport]| list.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
        assert_eq!(
            groups
                .iter()
                .map(|(i, l)| (i.clone(), names(l)))
                .collect::<Vec<_>>(),
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
        assert_eq!(
            groups[0].1[0].description.as_deref(),
            Some("the budget_name")
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
                Some(("Gone from source", true))
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
            Some(("Unchanged since fetched", false))
        );
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

    /// A pack used under two names, and flat beside them, is one row that names both uses
    /// in the order of their lines; a pack used flat alone names none.
    #[test]
    fn a_pack_used_under_names_is_one_row_with_its_names() {
        let line = |id: &str, named: Option<&str>| {
            (
                id.to_string(),
                format!("packs/{id}.satz"),
                named.map(str::to_string),
            )
        };
        let used = vec![
            line("vm", Some("web")),
            line("budget", None),
            line("vm", None),
            line("vm", Some("db")),
        ];
        let rows = here_rows(None, None, &used);
        assert_eq!(
            rows.iter()
                .map(|r| (r.id.as_str(), r.named.clone()))
                .collect::<Vec<_>>(),
            [
                ("vm", vec!["web".to_string(), "db".to_string()]),
                ("budget", Vec::new()),
            ]
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
            ("zeta".to_string(), "packs/zeta.satz".to_string(), None),
            ("alpha".to_string(), "packs/alpha.satz".to_string(), None),
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
        let used = vec![("alpha".to_string(), "packs/alpha.satz".to_string(), None)];
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

    /// A line that ends in `named <name>` is satz's for a pack used under a name; a
    /// plain line, and a name already written, are not.
    #[test]
    fn a_named_line_is_one_whose_name_is_still_to_write() {
        assert!(takes_a_name("use \"packs/vm.satz\" named <name>"));
        assert!(!takes_a_name("use \"packs/vm.satz\""));
        assert!(!takes_a_name("use \"packs/vm.satz\" named web"));
    }
}
