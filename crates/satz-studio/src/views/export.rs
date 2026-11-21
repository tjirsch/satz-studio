//! The export card: the decisions sheet and the workbook, the two documents a customer
//! receives, written by `satz questions <estate> --format <format> --out <file>`.
//!
//! The card stands in the Documents destination, once: the sheet is read at two moments —
//! as the decisions sheet before anything is touched and as the record after the
//! apply — and it is one document at both. A format, a destination, and the document
//! opened once satz has written it. The formats are the ones the installed satz
//! lists in `satz questions --help`, read when the session opens
//! ([`satz_studio_core::satz::export::formats`]); the card offers exactly those and
//! renders nothing satz cannot produce. The sheet is derived from the estate, so after an
//! edit "Export again" writes the same format over the same file in one click.

use std::collections::HashSet;
use std::path::PathBuf;

use dioxus::prelude::*;
use dioxus_icons::lucide::{FileOutput, RefreshCw};
use satz_studio_core::satz::QuestionsFormat;
use satz_studio_core::satz::export;

use crate::components::button::{Button, ButtonVariant};
use crate::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::components::toggle_group::{ToggleGroup, ToggleItem};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt};

/// The card's title and explanation.
const TITLE: &str = "Decisions sheet";
const TEXT: &str = "Every decision this estate rests on: what it is set to, whether it was chosen or taken as offered, and what changing it later costs. Signed before an organization is touched, kept as the record after the apply.";

/// The format the card starts on: the decisions sheet, `markdown`, when satz offers it;
/// otherwise none, and Export waits for a choice.
pub fn initial_format(formats: &[QuestionsFormat]) -> Option<String> {
    formats
        .iter()
        .find(|f| f.name == "markdown")
        .map(|f| f.name.clone())
}

/// The OS save dialog for the export's destination, opened in the estate's directory
/// with the name [`export::proposed_name`] gives; the chosen path, with the format's
/// extension when the name has none, goes to the estate coroutine.
fn choose_and_export(
    handle: Coroutine<EstateAction>,
    dir: PathBuf,
    estate: String,
    format: String,
) {
    spawn(async move {
        let picked = rfd::AsyncFileDialog::new()
            .set_title("Export the decisions")
            .set_directory(&dir)
            .set_file_name(export::proposed_name(&estate, &format))
            .add_filter(format.clone(), &[export::extension(&format)])
            .save_file()
            .await;
        if let Some(file) = picked {
            handle.send(EstateAction::Export {
                out: export::destination(file.path(), &format),
                format,
            });
        }
    });
}

#[component]
pub fn ExportCard() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let formats = app.estate().export_formats().cloned();
    let last = app.estate().last_export().cloned();
    let running = app.estate().running().cloned();
    let mut chosen = use_signal(|| None::<String>);
    let Some(open) = app.open().cloned() else {
        return rsx! {};
    };

    let body = match formats {
        None => rsx! { p { class: "export__note", "Reading the formats satz offers…" } },
        Some(Err(e)) => rsx! {
            p { class: "export__error", "The formats satz offers could not be read: {e}" }
        },
        Some(Ok(formats)) => {
            let current = chosen().or_else(|| initial_format(&formats));
            let description = current
                .as_deref()
                .and_then(|c| formats.iter().find(|f| f.name == c))
                .and_then(|f| f.description.clone());
            let names: Vec<String> = formats.iter().map(|f| f.name.clone()).collect();
            let current_index = current
                .as_deref()
                .and_then(|c| names.iter().position(|n| n == c));
            let dir = open.dir.clone();
            let estate = open.name.clone();
            rsx! {
                ToggleGroup {
                    horizontal: true,
                    pressed: Some(current_index.into_iter().collect::<HashSet<usize>>()),
                    on_pressed_change: {
                        let names = names.clone();
                        move |now: HashSet<usize>| {
                            // a click on the pressed one leaves it pressed
                            if let Some(name) = now
                                .into_iter()
                                .find(|i| Some(*i) != current_index)
                                .and_then(|i| names.get(i))
                            {
                                chosen.set(Some(name.clone()));
                            }
                        }
                    },
                    for (i, name) in names.iter().enumerate() {
                        ToggleItem { key: "{name}", index: i, "{name}" }
                    }
                }
                if let Some(d) = description {
                    p { class: "export__note", "{d}" }
                }
                div { class: "export__actions",
                    Button {
                        disabled: running || current.is_none(),
                        onclick: move |_| {
                            if let Some(format) = current.clone() {
                                choose_and_export(handle, dir.clone(), estate.clone(), format);
                            }
                        },
                        FileOutput { size: 16 }
                        "Export…"
                    }
                    if let Some(last) = last {
                        Button {
                            variant: ButtonVariant::Secondary,
                            disabled: running,
                            onclick: {
                                let last = last.clone();
                                move |_| handle.send(EstateAction::Export {
                                    format: last.format.clone(),
                                    out: last.path.clone(),
                                })
                            },
                            RefreshCw { size: 16 }
                            "Export again"
                        }
                        span { class: "export__last",
                            "{last.format} · "
                            code { "{last.path.display()}" }
                        }
                    }
                }
            }
        }
    };

    rsx! {
        Card {
            CardHeader {
                CardTitle { "{TITLE}" }
                CardDescription { "{TEXT}" }
            }
            CardContent {
                div { class: "export", {body} }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(name: &str) -> QuestionsFormat {
        QuestionsFormat {
            name: name.to_string(),
            description: None,
        }
    }

    #[test]
    fn the_card_starts_on_the_decisions_sheet_when_satz_offers_it() {
        let offered: Vec<QuestionsFormat> = ["text", "markdown", "pdf", "json", "xlsx"]
            .into_iter()
            .map(format)
            .collect();
        assert_eq!(initial_format(&offered).as_deref(), Some("markdown"));
        // a satz that offers no markdown gets no starting format, not a guessed one
        assert_eq!(initial_format(&[format("xlsx")]), None);
    }
}
