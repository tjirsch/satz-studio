//! The Packs view's pane: the estate's pack sources ([`PackSourcesCard`]), then the packs
//! of the estate's `packs_dir` — first every one the estate `use`s, read from its own
//! `use` lines, then the others — and, once a source is listed with `satz packs --source
//! <name>`, how each stands against it and what the source holds beyond them. A fetch is
//! `satz get-pack --source`, an update `satz update-pack` from each pack's recorded
//! source, and a pack edited here is shown with its difference and written only when the
//! operator says so (ADR 0024). A new pack is the repository's own: written and formatted,
//! its `use` line added through the app's writer, and opened in the application the
//! system gives `.satz` files, as Edit opens any pack (ADR 0031); once it works, "Store in
//! a pack source…" copies it into a folder source with `satz publish-pack`
//! ([`PublishDialog`]). The states are satz's report; the app derives none of its own.

use std::path::PathBuf;

use dioxus::prelude::*;
use satz_studio_core::cst::UseState;
use satz_studio_core::edit::Edit;
use satz_studio_core::satz::reports::{
    PackNeed, PackState, UpdateAction, UpdatePackReport, UpdatedPack, WorkloadPackRow,
    WorkloadPacksReport,
};
use satz_studio_core::satz::workload::{Which, new_pack_id};

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, Dialog, HelpButton, Icon, IconButton,
    LinearProgress, TextField,
};
use crate::state::{
    AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, PackOp, SourceOp, open_file,
};
use crate::views::pack_sources::{PackSourcesCard, Sheet};
use crate::views::publish_pack::PublishDialog;

/// A delegated value a pack reads, in words.
fn need_word(need: &PackNeed) -> &'static str {
    match need {
        PackNeed::Folder => "the delegated folder",
        PackNeed::Project => "the delegated project",
    }
}

/// How many delegated values a pack reads, as its row says it; none for a pack that
/// reads none.
fn needs_count(needs: &[PackNeed]) -> Option<String> {
    match needs.len() {
        0 => None,
        1 => Some("Reads 1 value".to_string()),
        n => Some(format!("Reads {n} values")),
    }
}

/// The badge a pack's state earns: the word, its icon, and whether it is in the error
/// color. A pack that is current, the repository's own or only available needs no word.
fn badge(state: PackState) -> Option<(&'static str, &'static str, bool)> {
    match state {
        PackState::Behind => Some(("Update available", "update", false)),
        PackState::Edited => Some(("Edited here", "edit", false)),
        PackState::Gone => Some(("Gone from source", "cloud_off", true)),
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
/// where it does, and satz's row for it, once a source is listed.
#[derive(Debug, Clone, PartialEq)]
struct HereRow {
    id: String,
    used: Option<String>,
    row: Option<WorkloadPackRow>,
}

/// The packs of `packs_dir`: every one the estate uses, in the order of its `use` lines,
/// then the others the listed report names, by id. `used` is each used pack's id and the
/// path of its line.
fn here_rows(report: Option<&WorkloadPacksReport>, used: &[(String, String)]) -> Vec<HereRow> {
    let listed: Vec<&WorkloadPackRow> = report
        .map(|r| {
            r.packs
                .iter()
                .filter(|p| p.state != PackState::Available)
                .collect()
        })
        .unwrap_or_default();
    let row_of = |id: &str| listed.iter().find(|p| p.id == id).map(|p| (*p).clone());
    let mut rows: Vec<HereRow> = Vec::new();
    for (id, path) in used {
        if rows.iter().all(|r| &r.id != id) {
            rows.push(HereRow {
                id: id.clone(),
                used: Some(path.clone()),
                row: row_of(id),
            });
        }
    }
    let mut others: Vec<HereRow> = listed
        .iter()
        .filter(|p| used.iter().all(|(id, _)| id != &p.id))
        .map(|p| HereRow {
            id: p.id.clone(),
            used: None,
            row: Some((*p).clone()),
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
    let rows = here_rows(listed.as_ref(), &used);
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
            HereCard {
                rows,
                listed: listed.is_some(),
                failed,
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
                        PackLine { key: "{row.id}", id: row.id.clone(), row: Some(row.clone()), used: true, file: None, edited: None, busy, confirm, publishing }
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
    failed: Option<String>,
    packs_dir: String,
    files: PathBuf,
    update: Option<UpdatePackReport>,
    busy: bool,
    confirm: Signal<Option<Confirm>>,
    publishing: Signal<Option<String>>,
) -> Element {
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
                HelpButton { title: "Workload packs",
                    p { "The packs this estate uses come first, then the other files of {packs_dir}. A fetched pack is recorded in origin.toml beside the packs with the name of its source; a pack without a row there is this repository's own, and no fetch and no update writes it." }
                    p { "Choosing a pack source runs satz packs against it, and each fetched pack says how it stands: Update available — unchanged here, and the source holds a newer file; Edited here — changed since it was fetched, so an update leaves it as it is and shows the difference; Gone from source — the source holds it no longer; Not checked — fetched from another source than the one listed." }
                    p { "Reads … values names the delegated folder or project a pack reads by default; an estate that was not delegated that value answers the param itself." }
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
            } else if !listed && !rows.is_empty() {
                p { class: "interfaces__supporting", "Choose a pack source to see how each pack stands against it." }
            }
            if rows.is_empty() {
                p { class: "interfaces__supporting",
                    if listed { "No workload pack is here yet." } else { "The estate uses no workload pack. Choose a pack source to list what it offers." }
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

/// One pack: its id, version, badge, where it comes from and what can be done. `row` is
/// satz's, once a source is listed; `used` says the estate reads it; `file` is where it
/// is, for a pack that is here.
#[component]
fn PackLine(
    id: String,
    row: Option<WorkloadPackRow>,
    used: bool,
    file: Option<PathBuf>,
    edited: Option<UpdatedPack>,
    busy: bool,
    confirm: Signal<Option<Confirm>>,
    mut publishing: Signal<Option<String>>,
) -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let mut needs_open = use_signal(|| false);
    let state = row.as_ref().map(|r| r.state);
    let needs = row.as_ref().map(|r| r.needs.clone()).unwrap_or_default();
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
                if let Some((word, icon, error)) = state.and_then(badge) {
                    Chip { kind: ChipKind::Assist, icon, label: word, error }
                }
                if let Some(count) = needs_count(&needs) {
                    Button { variant: ButtonVariant::Text, icon: "rule", onclick: move |_| needs_open.set(true), "{count}" }
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
                        onclick: move |_| open_file(app, &file),
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
        Dialog {
            open: needs_open(),
            title: "What {id} reads",
            icon: "rule",
            ondismiss: move |_| needs_open.set(false),
            actions: rsx! {
                Button { variant: ButtonVariant::Text, onclick: move |_| needs_open.set(false), "Close" }
            },
            ul {
                for need in needs.iter() {
                    li { key: "{need_word(need)}", "{need_word(need)}" }
                }
            }
            p { "Each is the default of a param of the pack: an estate that was not delegated that value answers the param itself." }
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
            needs: Vec::new(),
        }
    }

    #[test]
    fn a_packs_needs_are_counted_and_named() {
        assert_eq!(needs_count(&[]), None);
        assert_eq!(
            needs_count(&[PackNeed::Project]).as_deref(),
            Some("Reads 1 value")
        );
        assert_eq!(
            needs_count(&[PackNeed::Folder, PackNeed::Project]).as_deref(),
            Some("Reads 2 values")
        );
        assert_eq!(need_word(&PackNeed::Folder), "the delegated folder");
    }

    #[test]
    fn only_a_state_that_needs_attention_earns_a_badge() {
        assert_eq!(
            badge(PackState::Behind).map(|b| b.0),
            Some("Update available")
        );
        assert_eq!(badge(PackState::Edited).map(|b| b.0), Some("Edited here"));
        assert_eq!(
            badge(PackState::Gone),
            Some(("Gone from source", "cloud_off", true))
        );
        assert_eq!(
            badge(PackState::NotChecked).map(|b| b.0),
            Some("Not checked")
        );
        for quiet in [PackState::Current, PackState::Own, PackState::Available] {
            assert_eq!(badge(quiet), None, "{quiet:?}");
        }
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
        let unlisted = here_rows(None, &used);
        assert_eq!(
            unlisted.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["zeta", "alpha"]
        );
        assert!(unlisted.iter().all(|r| r.row.is_none() && r.used.is_some()));

        let report = WorkloadPacksReport {
            packs_dir: "packs".into(),
            sources: Vec::new(),
            packs: vec![
                with("alpha", PackState::Own),
                with("beta", PackState::Behind),
                with("gamma", PackState::Available),
                with("aardvark", PackState::Current),
            ],
        };
        let rows = here_rows(Some(&report), &used);
        assert_eq!(
            rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["zeta", "alpha", "aardvark", "beta"]
        );
        assert_eq!(rows[0].row, None, "a used pack satz did not list");
        assert_eq!(rows[1].row.as_ref().map(|r| r.state), Some(PackState::Own));
        assert_eq!(rows[3].used, None);
    }
}
