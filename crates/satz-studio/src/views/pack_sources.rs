//! The pack sources of the Packs view: every source `satz pack-source list` names — the
//! built-in `satz` first, then each `[[pack_source]]` of `config.toml` with where it is —
//! as a selectable list; choosing one lists its packs. A source is added through a dialog
//! that runs `satz pack-source add` and removed with `satz pack-source remove`, so
//! `config.toml` is written by satz alone; a refusal is shown as satz says it (ADR 0030).

use std::path::PathBuf;

use dioxus::prelude::*;
use satz_studio_core::satz::reports::{PackPlace, PackSourceRow};
use satz_studio_core::satz::workload::{NewPlace, NewSource};

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Dialog, Icon, IconButton, LinearProgress, List,
    ListItem, Segment, SegmentedButton, TextField,
};
use crate::state::{
    AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, PackOp, SourceOp,
};

/// The leading icon of a source: a repository or a folder.
fn place_icon(row: &PackSourceRow) -> &'static str {
    match row.place {
        PackPlace::Github { .. } => "cloud",
        PackPlace::Folder(_) => "folder",
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

/// The sources card. `confirm_remove` asks before a source leaves `config.toml`.
#[component]
pub fn PackSourcesCard(busy: bool, confirm_remove: EventHandler<String>) -> Element {
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

    let mut adding = use_signal(|| false);
    let mut name = use_signal(String::new);
    let mut kind = use_signal(|| "github".to_string());
    let mut repo = use_signal(String::new);
    let mut git_ref = use_signal(String::new);
    let mut path = use_signal(String::new);
    let mut folder = use_signal(|| None::<PathBuf>);
    // a source satz wrote closes the dialog and empties the form; a refusal keeps both
    let mut asked = use_signal(|| None::<String>);
    use_effect(move || {
        let change = app.estate().source_change().cloned();
        let wanted = asked.peek().clone();
        if let (Some(Ok(written)), Some(wanted)) = (change, wanted)
            && written == wanted
        {
            asked.set(None);
            adding.set(false);
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

    let rows = match sources {
        None => rsx! {},
        Some(Err(e)) => rsx! {
            div { class: "interfaces__error",
                Icon { name: "error", size: 20 }
                div {
                    p { "satz pack-source list could not list the pack sources:" }
                    pre { "{e}" }
                }
            }
        },
        Some(Ok(report)) => rsx! {
            List {
                for row in report.sources {
                    {
                        let chosen = listed.as_deref() == Some(row.name.as_str());
                        let list_name = row.name.clone();
                        let remove_name = row.name.clone();
                        let supporting = if row.builtin {
                            format!("{} · built in", row.location())
                        } else {
                            row.location()
                        };
                        rsx! {
                            ListItem {
                                key: "{row.name}",
                                headline: row.name.clone(),
                                supporting,
                                selected: chosen,
                                leading: rsx! { Icon { name: place_icon(&row), size: 22 } },
                                onclick: move |_| {
                                    if !busy {
                                        handle.send(EstateAction::Packs(PackOp::List(list_name.clone())));
                                    }
                                },
                                trailing: if row.builtin { None } else { Some(rsx! {
                                    IconButton {
                                        icon: "delete",
                                        label: "Remove the pack source",
                                        disabled: busy,
                                        onclick: move |e: MouseEvent| {
                                            e.stop_propagation();
                                            confirm_remove.call(remove_name.clone());
                                        },
                                    }
                                }) },
                            }
                        }
                    }
                }
            }
        },
    };

    rsx! {
        Card { variant: CardVariant::Outlined, class: "interfaces__card",
            div { class: "interfaces__card-head",
                Icon { name: "source", size: 22 }
                h2 { class: "interfaces__name", "Pack sources" }
                span { class: "grow" }
                Button {
                    variant: ButtonVariant::Tonal,
                    icon: "add",
                    disabled: busy,
                    onclick: move |_| {
                        app.estate().source_change().set(None);
                        adding.set(true);
                    },
                    "Add a source…"
                }
            }
            p { class: "interfaces__supporting",
                "Choose a source to list its packs. A GitHub source is a network read, so it is read when you choose it. The sources are config.toml's; satz pack-source writes them."
            }
            {rows}
            if !adding() {
                if let Some(said) = &refused {
                    div { class: "interfaces__error",
                        Icon { name: "error", size: 20 }
                        div {
                            p { "satz refused:" }
                            pre { "{said}" }
                        }
                    }
                }
            }
        }
        Dialog {
            open: adding(),
            title: "Add a pack source",
            icon: "add",
            class: "interfaces__add",
            ondismiss: move |_| adding.set(false),
            actions: rsx! {
                Button { variant: ButtonVariant::Text, onclick: move |_| adding.set(false), "Cancel" }
                Button {
                    variant: ButtonVariant::Filled,
                    icon: "check",
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
                    "Add"
                }
            },
            div { class: "wpacks__form",
                TextField {
                    label: "Name",
                    value: name(),
                    monospace: true,
                    supporting: "a lowercase letter, then lowercase letters, digits and -; origin.toml records it for every pack fetched from the source",
                    oninput: move |v: String| name.set(v),
                }
                SegmentedButton {
                    options: vec![
                        Segment::new("github", "GitHub repository").with_icon("cloud"),
                        Segment::new("folder", "Folder").with_icon("folder"),
                    ],
                    selected: kind(),
                    onselect: move |v: String| kind.set(v),
                }
                if kind() == "github" {
                    TextField {
                        label: "Repository",
                        value: repo(),
                        monospace: true,
                        placeholder: "owner/repository",
                        supporting: "a private repository is read with this machine's GitHub sign-in (gh auth login)",
                        oninput: move |v: String| repo.set(v),
                    }
                    TextField {
                        label: "Branch or tag",
                        value: git_ref(),
                        monospace: true,
                        supporting: "the repository's default branch when empty",
                        oninput: move |v: String| git_ref.set(v),
                    }
                    TextField {
                        label: "Folder of the repository",
                        value: path(),
                        monospace: true,
                        supporting: "packs when empty",
                        oninput: move |v: String| path.set(v),
                    }
                } else {
                    div { class: "wpacks__actions",
                        Button {
                            variant: ButtonVariant::Outlined,
                            icon: "folder_open",
                            onclick: move |_| {
                                spawn(async move {
                                    let dialog = rfd::AsyncFileDialog::new()
                                        .set_title("Choose the folder that holds the packs");
                                    if let Some(chosen) = dialog.pick_folder().await {
                                        folder.set(Some(chosen.path().to_path_buf()));
                                    }
                                });
                            },
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
                    div { class: "interfaces__error",
                        Icon { name: "error", size: 20 }
                        div {
                            p { "satz refused:" }
                            pre { "{said}" }
                        }
                    }
                }
                if busy {
                    LinearProgress {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
