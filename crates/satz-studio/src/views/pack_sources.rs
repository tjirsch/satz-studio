//! The pack sources of the Packs view: a card that names them in one line — how many,
//! and the first two — with "Manage", a dialog over every source `satz pack-source list`
//! names, the built-in `satz` first, then each `[[pack_source]]` of `config.toml` with
//! where it is. In the dialog a source is chosen to list its packs, its fetched packs are
//! updated from it (`satz update-pack --all --source <name>`), and a configured one is
//! removed with `satz pack-source remove`; "Add a source…" turns the dialog into the form
//! that runs `satz pack-source add`. `config.toml` is written by satz alone, and a refusal
//! is shown as satz says it (ADR 0030).

use std::path::PathBuf;

use dioxus::prelude::*;
use satz_studio_core::satz::reports::{PackPlace, PackSourceRow};
use satz_studio_core::satz::workload::{NewPlace, NewSource, Which};

use std::collections::HashSet;

use dioxus_icons::lucide::{
    Check, CircleAlert, Cloud, Folder, FolderOpen, Library, Plus, RefreshCw, Settings2, Trash2,
};

use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::card::{Card, CardHeader};
use crate::components::dialog::{Dialog, DialogTitle};
use crate::components::item::{
    Item, ItemActions, ItemContent, ItemDescription, ItemGroup, ItemMedia, ItemSize, ItemTitle,
};
use crate::components::progress::Progress;
use crate::components::toggle_group::{ToggleGroup, ToggleItem};
use crate::state::{
    AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, PackOp, SourceOp,
};
use crate::widgets::{Field, HelpButton};

/// The kinds of source the form adds, in the order their buttons stand.
const KINDS: [&str; 2] = ["github", "folder"];

/// The leading icon of a source: a repository or a folder.
fn place_icon(row: &PackSourceRow, size: u32) -> Element {
    match row.place {
        PackPlace::Github { .. } => rsx! { Cloud { size } },
        PackPlace::Folder(_) => rsx! { Folder { size } },
    }
}

/// What the form holds, read into the source satz is asked to add; `None` while a part
/// the source cannot go without is empty. satz judges the name and the repository.
fn new_source(
    name: &str,
    kind: &str,
    repo: &str,
    git_ref: &str,
    path: &str,
    folder: Option<&PathBuf>,
) -> Option<NewSource> {
    let name = name.trim();
    let given = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_string());
    let place = match kind {
        "github" => NewPlace::Github {
            repo: given(repo)?,
            git_ref: given(git_ref),
            path: given(path),
        },
        _ => NewPlace::Folder(folder?.clone()),
    };
    (!name.is_empty()).then(|| NewSource {
        name: name.to_string(),
        place,
    })
}

/// The sources in one line: how many, and the names of the first two — "1 pack source:
/// satz", "3 pack sources: satz, team, …".
pub fn sources_summary(names: &[&str]) -> String {
    let count = match names.len() {
        0 => return "No pack source".to_string(),
        1 => "1 pack source".to_string(),
        n => format!("{n} pack sources"),
    };
    let mut shown = names.iter().take(2).copied().collect::<Vec<_>>().join(", ");
    if names.len() > 2 {
        shown.push_str(", …");
    }
    format!("{count}: {shown}")
}

/// What the dialog over the sources shows. The Packs view holds it, so the dialog that
/// stores a pack in a source opens the form that adds one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sheet {
    Closed,
    /// every source, each chosen, updated from or removed
    Manage,
    /// the form that adds one
    Add,
}

/// What a dialog's own close asks of `sheet`. A dialog closes itself on Escape and on a
/// pointer or focus outside it — and the one that just closed still listens while its
/// closing animation runs, so moving from the list to the form would hear the form's own
/// focus as "outside" and close everything. A dialog therefore closes `sheet` only while
/// `sheet` still names it; once the other one stands, its close changes nothing.
pub fn on_own_close(now: Sheet, mine: Sheet, then: Sheet) -> Option<Sheet> {
    (now == mine).then_some(then)
}

/// A refusal of satz's, as it said it.
#[component]
pub fn Refused(said: String) -> Element {
    rsx! {
        div { class: "interfaces__error",
            CircleAlert { size: 20 }
            div {
                p { "satz refused:" }
                pre { "{said}" }
            }
        }
    }
}

/// The sources card. `sheet` is which of its dialogs stands; `confirm_remove` asks before
/// a source leaves `config.toml`.
#[component]
pub fn PackSourcesCard(
    busy: bool,
    mut sheet: Signal<Sheet>,
    confirm_remove: EventHandler<String>,
) -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let sources = app.estate().pack_sources().cloned();
    let listed = app.estate().pack_source().cloned();
    let change = app.estate().source_change().cloned();
    // the sources are config.toml's: read once when the view first stands, no network
    use_hook(move || {
        if app.estate().pack_sources().peek().is_none() {
            handle.send(EstateAction::PackSources(SourceOp::List));
        }
    });

    let mut name = use_signal(String::new);
    let mut kind = use_signal(|| "github".to_string());
    let mut repo = use_signal(String::new);
    let mut git_ref = use_signal(String::new);
    let mut path = use_signal(String::new);
    let mut folder = use_signal(|| None::<PathBuf>);
    // a source satz wrote returns to the list and empties the form; a refusal keeps both
    let mut asked = use_signal(|| None::<String>);
    use_effect(move || {
        let change = app.estate().source_change().cloned();
        let wanted = asked.peek().clone();
        if let (Some(Ok(written)), Some(wanted)) = (change, wanted)
            && written == wanted
        {
            asked.set(None);
            sheet.set(Sheet::Manage);
            name.set(String::new());
            repo.set(String::new());
            git_ref.set(String::new());
            path.set(String::new());
            folder.set(None);
        }
    });
    let ready = new_source(
        &name(),
        &kind(),
        &repo(),
        &git_ref(),
        &path(),
        folder().as_ref(),
    );
    let refused = match &change {
        Some(Err(said)) => Some(said.clone()),
        _ => None,
    };

    let (summary, rows) = match sources {
        None => (String::new(), rsx! {}),
        Some(Err(e)) => (
            String::new(),
            rsx! {
                div { class: "interfaces__error",
                    CircleAlert { size: 20 }
                    div {
                        p { "satz pack-source list could not list the pack sources:" }
                        pre { "{e}" }
                    }
                }
            },
        ),
        Some(Ok(report)) => (
            sources_summary(
                &report
                    .sources
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>(),
            ),
            rsx! {
                ItemGroup {
                    for row in report.sources {
                        {
                            let chosen = listed.as_deref() == Some(row.name.as_str());
                            let list_name = row.name.clone();
                            let update_name = row.name.clone();
                            let remove_name = row.name.clone();
                            let supporting = if row.builtin {
                                format!("{} · built in", row.location())
                            } else {
                                row.location()
                            };
                            rsx! {
                                Item {
                                    key: "{row.name}",
                                    size: ItemSize::Sm,
                                    "data-selected": if chosen { "true" } else { "false" },
                                    onclick: move |_| {
                                        if !busy {
                                            handle.send(EstateAction::Packs(PackOp::List(list_name.clone())));
                                            sheet.set(Sheet::Closed);
                                        }
                                    },
                                    ItemMedia { {place_icon(&row, 22)} }
                                    ItemContent {
                                        ItemTitle { "{row.name}" }
                                        ItemDescription { "{supporting}" }
                                    }
                                    ItemActions {
                                        Button {
                                            variant: ButtonVariant::Ghost,
                                            size: ButtonSize::Icon,
                                            title: "Update the packs fetched from {row.name}",
                                            "aria-label": "Update the packs fetched from {row.name}",
                                            disabled: busy,
                                            onclick: move |e: MouseEvent| {
                                                e.stop_propagation();
                                                handle.send(EstateAction::Packs(PackOp::Update {
                                                    which: Which::All,
                                                    from: Some(update_name.clone()),
                                                    take_upstream: false,
                                                }));
                                            },
                                            RefreshCw { size: 16 }
                                        }
                                        if !row.builtin {
                                            Button {
                                                variant: ButtonVariant::Ghost,
                                                size: ButtonSize::Icon,
                                                title: "Remove the pack source",
                                                "aria-label": "Remove the pack source",
                                                disabled: busy,
                                                onclick: move |e: MouseEvent| {
                                                    e.stop_propagation();
                                                    confirm_remove.call(remove_name.clone());
                                                },
                                                Trash2 { size: 16 }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
        ),
    };
    let listing = listed
        .as_deref()
        .map(|l| format!(" · packs listed from {l}"))
        .unwrap_or_default();

    let kind_pressed: HashSet<usize> = KINDS
        .iter()
        .position(|k| *k == kind())
        .into_iter()
        .collect();

    rsx! {
        Card { class: "interfaces__card",
            CardHeader {
                div { class: "interfaces__card-head",
                    Library { size: 22 }
                    h2 { class: "interfaces__name", "Pack sources" }
                    span { class: "interfaces__supporting", "{summary}{listing}" }
                    span { class: "grow" }
                    Button {
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| {
                            app.estate().source_change().set(None);
                            sheet.set(Sheet::Manage);
                        },
                        Settings2 { size: 16 }
                        "Manage"
                    }
                    HelpButton { title: "Pack sources",
                        p { "A pack source is where workload packs are fetched from: satz, the satz repository's packs folder, built in; and each source config.toml lists, a GitHub repository or a folder." }
                        p { "Manage lists them. Choose one to list its packs against this estate: a GitHub source is read over the network, so it is read only when you choose it. Update fetches every pack that came from that source again, leaving a pack edited here as it is." }
                        p { "The sources are config.toml's, and satz pack-source writes them: Add runs satz pack-source add, and Remove runs satz pack-source remove, which satz refuses while origin.toml names the source for a fetched pack." }
                    }
                }
            }
        }
        Dialog {
            open: sheet() == Sheet::Manage,
            on_open_change: move |now: bool| {
                let shown = *sheet.peek();
                if !now && let Some(next) = on_own_close(shown, Sheet::Manage, Sheet::Closed) {
                    sheet.set(next);
                }
            },
            class: "interfaces__add",
            DialogTitle { "Pack sources" }
            div { class: "dialog-body",
                p { class: "interfaces__supporting", "Choose a source to list its packs; update or remove it from its row." }
                {rows}
                if let Some(said) = &refused {
                    Refused { said: said.clone() }
                }
                if busy {
                    Progress { value: None::<f64>, "aria-label": "working" }
                }
            }
            div { class: "dialog-actions",
                Button {
                    variant: ButtonVariant::Ghost,
                    disabled: busy,
                    onclick: move |_| {
                        app.estate().source_change().set(None);
                        sheet.set(Sheet::Add);
                    },
                    Plus { size: 16 }
                    "Add a source…"
                }
                Button { variant: ButtonVariant::Ghost, onclick: move |_| sheet.set(Sheet::Closed), "Close" }
            }
        }
        Dialog {
            open: sheet() == Sheet::Add,
            on_open_change: move |now: bool| {
                let shown = *sheet.peek();
                if !now && let Some(next) = on_own_close(shown, Sheet::Add, Sheet::Manage) {
                    sheet.set(next);
                }
            },
            class: "interfaces__add",
            DialogTitle { "Add a pack source" }
            div { class: "dialog-body wpacks__form",
                Field {
                    label: "Name",
                    value: name(),
                    monospace: true,
                    supporting: "a lowercase letter, then lowercase letters, digits and -; origin.toml records it for every pack fetched from the source",
                    oninput: move |v: String| name.set(v),
                }
                ToggleGroup {
                    horizontal: true,
                    pressed: Some(kind_pressed.clone()),
                    on_pressed_change: move |now: HashSet<usize>| {
                        // a click on the pressed one leaves it pressed
                        if let Some(k) = now.difference(&kind_pressed).next().and_then(|i| KINDS.get(*i)) {
                            kind.set((*k).to_string());
                        }
                    },
                    ToggleItem { index: 0usize, Cloud { size: 16 } "GitHub repository" }
                    ToggleItem { index: 1usize, Folder { size: 16 } "Folder" }
                }
                if kind() == "github" {
                    Field {
                        label: "Repository",
                        value: repo(),
                        monospace: true,
                        placeholder: "owner/repository",
                        supporting: "a private repository is read with this machine's GitHub sign-in (gh auth login)",
                        oninput: move |v: String| repo.set(v),
                    }
                    Field {
                        label: "Branch or tag",
                        value: git_ref(),
                        monospace: true,
                        supporting: "the repository's default branch when empty",
                        oninput: move |v: String| git_ref.set(v),
                    }
                    Field {
                        label: "Folder of the repository",
                        value: path(),
                        monospace: true,
                        supporting: "packs when empty",
                        oninput: move |v: String| path.set(v),
                    }
                } else {
                    div { class: "wpacks__actions",
                        Button {
                            variant: ButtonVariant::Outline,
                            onclick: move |_| {
                                spawn(async move {
                                    let dialog = rfd::AsyncFileDialog::new()
                                        .set_title("Choose the folder that holds the packs");
                                    if let Some(chosen) = dialog.pick_folder().await {
                                        folder.set(Some(chosen.path().to_path_buf()));
                                    }
                                });
                            },
                            FolderOpen { size: 16 }
                            "Choose a folder…"
                        }
                        if let Some(dir) = folder() {
                            code { class: "interfaces__at", "{dir.display()}" }
                        }
                    }
                    p { class: "interfaces__supporting",
                        "config.toml writes it relative to the estate's directory, so every clone laid out alike reads the same folder."
                    }
                }
                if let Some(said) = &refused {
                    Refused { said: said.clone() }
                }
                if busy {
                    Progress { value: None::<f64>, "aria-label": "working" }
                }
            }
            div { class: "dialog-actions",
                Button { variant: ButtonVariant::Ghost, onclick: move |_| sheet.set(Sheet::Manage), "Cancel" }
                Button {
                    disabled: busy || ready.is_none(),
                    onclick: {
                        let ready = ready.clone();
                        move |_| {
                            if let Some(source) = ready.clone() {
                                asked.set(Some(source.name.clone()));
                                handle.send(EstateAction::PackSources(SourceOp::Add(source)));
                            }
                        }
                    },
                    Check { size: 16 }
                    "Add"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Moving from the list to the form closes the list, and the list still hears the
    /// form's focus as outside: its close must leave the form standing.
    #[test]
    fn a_dialog_closes_the_sheet_only_while_it_is_the_one_shown() {
        assert_eq!(
            on_own_close(Sheet::Manage, Sheet::Manage, Sheet::Closed),
            Some(Sheet::Closed)
        );
        assert_eq!(on_own_close(Sheet::Add, Sheet::Manage, Sheet::Closed), None);
        assert_eq!(
            on_own_close(Sheet::Add, Sheet::Add, Sheet::Manage),
            Some(Sheet::Manage)
        );
        assert_eq!(on_own_close(Sheet::Closed, Sheet::Add, Sheet::Manage), None);
    }

    #[test]
    fn the_sources_are_counted_and_the_first_two_named() {
        assert_eq!(sources_summary(&[]), "No pack source");
        assert_eq!(sources_summary(&["satz"]), "1 pack source: satz");
        assert_eq!(
            sources_summary(&["satz", "team"]),
            "2 pack sources: satz, team"
        );
        assert_eq!(
            sources_summary(&["satz", "team", "local"]),
            "3 pack sources: satz, team, …"
        );
    }

    #[test]
    fn the_form_is_a_source_once_its_name_and_place_are_given() {
        assert_eq!(new_source("", "github", "acme/packs", "", "", None), None);
        assert_eq!(new_source("team", "github", " ", "", "", None), None);
        assert_eq!(
            new_source("team", "github", "acme/packs", " v1 ", "", None),
            Some(NewSource {
                name: "team".into(),
                place: NewPlace::Github {
                    repo: "acme/packs".into(),
                    git_ref: Some("v1".into()),
                    path: None,
                },
            })
        );
        assert_eq!(
            new_source("local", "folder", "acme/packs", "", "", None),
            None
        );
        let dir = PathBuf::from("/e/packs");
        assert_eq!(
            new_source("local", "folder", "", "", "", Some(&dir)),
            Some(NewSource {
                name: "local".into(),
                place: NewPlace::Folder(dir.clone()),
            })
        );
    }
}
