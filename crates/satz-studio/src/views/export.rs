//! The export card: the decisions sheet and the workbook, the two documents a customer
//! receives, written by `satz questions <estate> --format <format> --out <file>`.
//!
//! The card stands at the two moments the sheet is read — in Decisions, as the sign-off
//! page before anything is touched, and in the Overview, as the handover record after
//! the rollout — and it is the same card at both: a format, a destination, and the
//! document opened once satz has written it. The formats are the ones the installed satz
//! lists in `satz questions --help`, read when the session opens
//! ([`satz_studio_core::satz::export::formats`]); the card offers exactly those and
//! renders nothing satz cannot produce. The sheet is derived from the estate, so after an
//! edit "Export again" writes the same format over the same file in one click.

use std::path::PathBuf;

use dioxus::prelude::*;
use satz_studio_core::satz::QuestionsFormat;
use satz_studio_core::satz::export;

use crate::components::{Button, ButtonVariant, Card, CardVariant, Icon, Segment, SegmentedButton};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt};

/// Where the card stands, which decides only what it says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moment {
    /// in Decisions, once the packs are chosen: the page the customer signs
    SignOff,
    /// in the Overview, after the rollout: the record the customer keeps
    Handover,
}

impl Moment {
    pub fn title(self) -> &'static str {
        match self {
            Moment::SignOff => "Sign-off sheet",
            Moment::Handover => "Handover record",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Moment::SignOff => "assignment_turned_in",
            Moment::Handover => "inventory",
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Moment::SignOff => {
                "Every decision this estate rests on — what it is set to, whether it was chosen or taken as offered, and what changing it later costs — as the sheet the customer signs before an organisation is touched, or the workbook they fill in and send back."
            }
            Moment::Handover => {
                "The decisions the estate is rolled out with, as the record the customer keeps. The sheet is derived from the estate: export it again after any change."
            }
        }
    }
}

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
pub fn ExportCard(moment: Moment) -> Element {
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
            let options: Vec<Segment> = formats
                .iter()
                .map(|f| Segment::new(f.name.clone(), f.name.clone()))
                .collect();
            let dir = open.dir.clone();
            let estate = open.name.clone();
            rsx! {
                SegmentedButton {
                    options,
                    selected: current.clone().unwrap_or_default(),
                    onselect: move |v: String| chosen.set(Some(v)),
                }
                if let Some(d) = description {
                    p { class: "export__note", "{d}" }
                }
                div { class: "export__actions",
                    Button {
                        variant: ButtonVariant::Filled,
                        icon: "file_export",
                        disabled: running || current.is_none(),
                        onclick: move |_| {
                            if let Some(format) = current.clone() {
                                choose_and_export(handle, dir.clone(), estate.clone(), format);
                            }
                        },
                        "Export…"
                    }
                    if let Some(last) = last {
                        Button {
                            variant: ButtonVariant::Tonal,
                            icon: "refresh",
                            disabled: running,
                            onclick: {
                                let last = last.clone();
                                move |_| handle.send(EstateAction::Export {
                                    format: last.format.clone(),
                                    out: last.path.clone(),
                                })
                            },
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
        Card { variant: CardVariant::Outlined, class: "export",
            header { class: "export__head",
                Icon { name: moment.icon(), size: 22 }
                h2 { class: "export__title", "{moment.title()}" }
            }
            p { class: "export__text", "{moment.text()}" }
            {body}
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
