//! "Store in a pack source…": the basic dialog over one of the repository's own packs that
//! runs `satz publish-pack <id> --source <name> --estate <the open estate>` (satz's ADR
//! 0099). It offers the estate's folder sources alone — satz writes no GitHub source — and,
//! with none, the form that adds one. satz checks the pack against `review-pack`'s bar
//! inside the estate, copies it into the folder and records it in `origin.toml` as fetched
//! from there. A refusal keeps the dialog open with satz's words; another file of that id
//! in the folder is overwritten only after a line that names it.

use dioxus::prelude::*;
use satz_studio_core::satz::workload::{PublishRefusal, folder_sources};

use dioxus_icons::lucide::{CircleAlert, FolderUp, Plus, TriangleAlert};

use crate::components::button::{Button, ButtonVariant};
use crate::components::dialog::{Dialog, DialogTitle};
use crate::components::progress::Progress;
use crate::components::radio_group::{RadioGroup, RadioItem};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, PackOp};
use crate::views::pack_sources::{Refused, Sheet};

/// The dialog, open while `publishing` names a pack. `sheet` is the pack sources' dialog,
/// opened on the form that adds one when the estate has no folder source.
#[component]
pub fn PublishDialog(
    mut publishing: Signal<Option<String>>,
    mut sheet: Signal<Sheet>,
    busy: bool,
) -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let mut chosen = use_signal(|| None::<String>);
    let mut confirming = use_signal(|| false);
    // the pack a publish was asked for: its report closes the dialog
    let mut asked = use_signal(|| None::<String>);
    use_effect(move || {
        let outcome = app.estate().pack_publish().cloned();
        let wanted = asked.peek().clone();
        if let (Some(Ok(published)), Some(wanted)) = (outcome, wanted)
            && published.id == wanted
        {
            asked.set(None);
            confirming.set(false);
            chosen.set(None);
            publishing.set(None);
        }
    });

    let Some(id) = publishing() else {
        return rsx! {};
    };
    let mut close = move || {
        asked.set(None);
        confirming.set(false);
        chosen.set(None);
        publishing.set(None);
    };
    let folders = match app.estate().pack_sources().cloned() {
        Some(Ok(report)) => Some(folder_sources(&report)),
        _ => None,
    };
    let target = chosen().or_else(|| {
        folders
            .as_ref()
            .and_then(|f| f.first().map(|(name, _)| name.clone()))
    });
    let refused = match app.estate().pack_publish().cloned() {
        Some(Err(said)) => Some(said),
        _ => None,
    };
    let refusal = refused.as_deref().map(PublishRefusal::read);
    let exists = match &refusal {
        Some(PublishRefusal::Exists { file }) => Some(file.clone()),
        _ => None,
    };
    let mut run = move |id: String, source: String, force: bool| {
        asked.set(Some(id.clone()));
        confirming.set(false);
        handle.send(EstateAction::Packs(PackOp::Publish { id, source, force }));
    };

    let actions = match (&folders, &exists) {
        (Some(f), _) if f.is_empty() => rsx! {
            Button { variant: ButtonVariant::Ghost, onclick: move |_| close(), "Cancel" }
            Button {
                variant: ButtonVariant::Secondary,
                onclick: move |_| {
                    app.estate().source_change().set(None);
                    close();
                    sheet.set(Sheet::Add);
                },
                Plus { size: 16 }
                "Add a source…"
            }
        },
        (_, Some(_)) if confirming() => {
            let (id, target) = (id.clone(), target.clone());
            rsx! {
                Button { variant: ButtonVariant::Ghost, onclick: move |_| confirming.set(false), "Cancel" }
                Button {
                    variant: ButtonVariant::Destructive,
                    disabled: busy || target.is_none(),
                    onclick: move |_| {
                        if let Some(source) = target.clone() {
                            run(id.clone(), source, true);
                        }
                    },
                    TriangleAlert { size: 16 }
                    "Overwrite"
                }
            }
        }
        _ => {
            let (id, target) = (id.clone(), target.clone());
            rsx! {
                Button { variant: ButtonVariant::Ghost, onclick: move |_| close(), "Cancel" }
                if exists.is_some() {
                    Button {
                        variant: ButtonVariant::Ghost,
                        disabled: busy,
                        onclick: move |_| confirming.set(true),
                        TriangleAlert { size: 16 }
                        "Overwrite…"
                    }
                }
                Button {
                    disabled: busy || target.is_none(),
                    onclick: move |_| {
                        if let Some(source) = target.clone() {
                            run(id.clone(), source, false);
                        }
                    },
                    FolderUp { size: 16 }
                    "Store"
                }
            }
        }
    };

    rsx! {
        Dialog {
            open: true,
            on_open_change: move |now: bool| {
                if !now {
                    close();
                }
            },
            class: "interfaces__add",
            DialogTitle { "Store {id} in a pack source" }
            div { class: "dialog-body",
            match &folders {
                None => rsx! {
                    p { class: "interfaces__supporting", "The pack sources are being read." }
                },
                Some(f) if f.is_empty() => rsx! {
                    p { "This estate has no folder pack source. A pack is stored in a folder; a GitHub source, satz's own among them, is written by a pull request to its repository." }
                },
                Some(f) => rsx! {
                    p { class: "interfaces__supporting",
                        "satz publish-pack checks the pack inside this estate against review-pack's bar, copies it into the folder and records it in origin.toml as fetched from there: from then on update-pack follows the source."
                    }
                    RadioGroup {
                        class: "wpacks__form",
                        value: target.clone(),
                        disabled: busy,
                        on_value_change: move |name: String| {
                            chosen.set(Some(name));
                            confirming.set(false);
                            app.estate().pack_publish().set(None);
                        },
                        for (i, (name, dir)) in f.iter().cloned().enumerate() {
                            RadioItem { key: "{name}", value: name.clone(), index: i, "{name} · {dir}" }
                        }
                    }
                },
            }
            match (&refusal, &refused) {
                (Some(PublishRefusal::Bar { lines, command }), _) => rsx! {
                    div { class: "interfaces__error",
                        CircleAlert { size: 20 }
                        div {
                            p { "{id} does not clear review-pack's bar inside this estate:" }
                            ul {
                                for line in lines.iter() {
                                    li { key: "{line}", "{line}" }
                                }
                            }
                            p { "The full list of findings comes from satz review-pack:" }
                            if let Some(command) = command {
                                pre { "{command}" }
                            }
                        }
                    }
                },
                (Some(_), Some(said)) => rsx! {
                    Refused { said: said.clone() }
                },
                _ => rsx! {},
            }
            if confirming() {
                if let Some(file) = &exists {
                    p { "Overwrite " code { "{file}" } " with this estate's {id}? The file there is replaced, and every estate that fetches {id} from that source gets this one." }
                }
            }
            if busy {
                Progress { value: None::<f64>, "aria-label": "working" }
            }
            }
            div { class: "dialog-actions", {actions} }
        }
    }
}
