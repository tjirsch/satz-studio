//! The pack review, an entry of the commands palette ("Review a pack for the preset
//! library", ADR 0031): `satz review-preset` over a pack the operator chooses, and the two
//! places a reviewed pack goes.
//!
//! The rules are satz's; the view runs the command ([`EstateAction::ReviewPack`]) and
//! shows what it said: the verdict, what the pack creates, each finding — clickable, and
//! in the drawer at its line — and the pack's own text with the chosen finding's line
//! marked. Then the two destinations. **Upstream** is a pull request to the satz
//! repository, by hand until satz ships `contribute-pack`: the card says what it takes and
//! offers the file's path and folder, and automates nothing. **Private** places the
//! reviewed bytes in the estate's `presets_dir` as `<stem>.local.satz`
//! ([`EstateAction::PlacePrivate`]), the suffix satz's updates never touch.

use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use satz_studio_core::diag::Diagnostic;
use satz_studio_core::satz::reports::FindingSeverity;
use satz_studio_core::satz::review::{self, ReviewedPack};

use dioxus_icons::lucide::{
    CircleAlert, CircleCheck, Copy as CopyIcon, FileInput, FileText, FolderOpen,
    History as HistoryIcon, Info, ListPlus, Lock, MessageSquareText, RefreshCw, Shield,
    TriangleAlert, Upload, X as XIcon,
};

use crate::components::badge::{Badge, BadgeVariant};
use crate::components::button::{Button, ButtonVariant};
use crate::components::card::{Card, CardContent, CardHeader};
use crate::components::item::{
    Item, ItemContent, ItemDescription, ItemGroup, ItemMedia, ItemSize, ItemTitle,
};
use crate::components::progress::Progress;
use crate::state::{
    AppStore, AppStoreStoreExt, DiagnosticSelection, EstateAction, EstateStoreStoreExt,
    PackReviewState,
};
use crate::views::commands::{REVIEW_LABEL, copy_to_clipboard};
use crate::widgets::SwitchRow;

/// The OS open dialog for the pack, in the directory of the last pack reviewed, else the
/// estate's library; the chosen file goes to the estate coroutine.
fn choose_and_review(handle: Coroutine<EstateAction>, start: PathBuf, against: bool) {
    spawn(async move {
        let picked = rfd::AsyncFileDialog::new()
            .set_title("Choose the pack to review")
            .set_directory(&start)
            .add_filter("Satz", &["satz"])
            .pick_file()
            .await;
        if let Some(file) = picked {
            handle.send(EstateAction::ReviewPack {
                pack: file.path().to_path_buf(),
                against,
            });
        }
    });
}

/// The line the drawer's selection marks in the pack's text: the selected finding's
/// line, when it is a finding about this pack.
pub fn marked_line(selected: Option<&Diagnostic>, pack: &Path) -> Option<u32> {
    selected
        .filter(|d| d.file.as_deref() == Some(pack))
        .and_then(|d| d.line)
}

#[component]
pub fn PackReviewCard() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let selection = use_context::<DiagnosticSelection>();
    let review = app.estate().review().cloned();
    let reviewing = app.estate().reviewing().cloned();
    let mut against = use_signal(|| false);
    let Some(open) = app.open().cloned() else {
        return rsx! {};
    };
    let presets = open.session.dir.presets_dir();
    let start = review
        .as_ref()
        .and_then(|r| r.pack().parent().map(Path::to_path_buf))
        .unwrap_or_else(|| presets.clone());

    // A finding picked in the drawer or in the list scrolls its line of the text into view.
    // The review is peeked: a new review must not scroll to a line the reader left.
    use_effect(move || {
        let picked = (selection.0)();
        let pack = app
            .estate()
            .review()
            .peek()
            .as_ref()
            .map(|r| r.pack().to_path_buf());
        if let Some(line) = pack.and_then(|p| marked_line(picked.as_ref(), &p)) {
            let _ = document::eval(&format!(
                "document.getElementById('review-line-{line}')?.scrollIntoView({{block: 'center', behavior: 'smooth'}})"
            ));
        }
    });

    rsx! {
        Card { class: "review",
            CardHeader {
                div { class: "review__head",
                    MessageSquareText { size: 24 }
                    h2 { class: "review__title", "{REVIEW_LABEL}" }
                    span { class: "grow" }
                    SwitchRow {
                        checked: against(),
                        label: "Judge it inside {open.name}",
                        disabled: reviewing,
                        onchange: move |v: bool| against.set(v),
                    }
                    Button {
                        disabled: reviewing,
                        onclick: {
                            let start = start.clone();
                            move |_| choose_and_review(handle, start.clone(), against())
                        },
                        FileInput { size: 16 }
                        "Choose a pack…"
                    }
                }
            }
            CardContent {
                p { class: "review__lead",
                    "satz review-preset holds a pack to the library's own bar: it parses, it is formatted, its header says what it is, its version has a changelog row, it declares no membership, it runs no legacy constraint beside its managed replacement, every type it creates has a prerequisite row, and it transpiles. The pack is folded into an estate made of the documented example values, or into "
                    code { "{open.name}" }
                    " with the switch on — for a pack that estate uses. Each finding is in the drawer at its line."
                }
                if reviewing {
                    Progress { value: None::<f64>, "aria-label": "working" }
                }
                match review {
                    None => rsx! {},
                    Some(PackReviewState::Failed { pack, error }) => rsx! {
                        div { class: "review__failed", role: "alert",
                            CircleAlert { size: 20 }
                            div { class: "grow",
                                code { "{pack.display()}" }
                                p { "{error}" }
                            }
                            Button {
                                variant: ButtonVariant::Secondary,
                                disabled: reviewing,
                                onclick: {
                                    let pack = pack.clone();
                                    move |_| handle.send(EstateAction::ReviewPack { pack: pack.clone(), against: against() })
                                },
                                RefreshCw { size: 16 }
                                "Review again"
                            }
                            Button { variant: ButtonVariant::Ghost, onclick: move |_| handle.send(EstateAction::CloseReview), XIcon { size: 16 } "Close" }
                        }
                    },
                    Some(PackReviewState::Reviewed(reviewed)) => rsx! {
                        Reviewed { reviewed, presets: presets.clone(), reviewing }
                    },
                }
            }
        }
    }
}

/// A review satz answered: the verdict, the findings, the text, the two destinations.
#[component]
fn Reviewed(reviewed: ReviewedPack, presets: PathBuf, reviewing: bool) -> Element {
    let handle = use_coroutine_handle::<EstateAction>();
    let selection = use_context::<DiagnosticSelection>();
    let r = &reviewed.review;
    let passed = r.passed();
    let errors = r.count(FindingSeverity::Error);
    let warnings = r.count(FindingSeverity::Warning);
    let infos = r.count(FindingSeverity::Info);
    let folded = if reviewed.against {
        "folded into the open estate"
    } else {
        "folded into example values"
    };
    let creates = if r.emits.is_empty() {
        "nothing".to_string()
    } else {
        r.emits.join(", ")
    };
    let diagnostics = reviewed.diagnostics();
    let selected = (selection.0)();
    let marked = marked_line(selected.as_ref(), &reviewed.path);
    let path = reviewed.path.clone();

    rsx! {
        div { class: "review__summary",
            code { class: "review__path", "{reviewed.path.display()}" }
            span { class: "grow" }
            Badge { variant: if passed { BadgeVariant::Outline } else { BadgeVariant::Destructive },
                if passed { CircleCheck { size: 12 } } else { CircleAlert { size: 12 } }
                if passed { "clears the bar" } else { "does not clear the bar yet" }
            }
            Badge { variant: if errors > 0 { BadgeVariant::Destructive } else { BadgeVariant::Outline },
                CircleAlert { size: 12 }
                "{errors} errors"
            }
            Badge { variant: BadgeVariant::Outline,
                TriangleAlert { size: 12 }
                "{warnings} warnings"
            }
            Badge { variant: BadgeVariant::Outline,
                Info { size: 12 }
                "{infos} notes"
            }
            Button {
                variant: ButtonVariant::Secondary,
                disabled: reviewing,
                onclick: {
                    let path = path.clone();
                    let against = reviewed.against;
                    move |_| handle.send(EstateAction::ReviewPack { pack: path.clone(), against })
                },
                RefreshCw { size: 16 }
                "Review again"
            }
            Button { variant: ButtonVariant::Ghost, onclick: move |_| handle.send(EstateAction::CloseReview), XIcon { size: 16 } "Close" }
        }
        p { class: "review__emits", "{folded} · creates {creates}" }
        div { class: "review__panes",
            ItemGroup { class: "review__findings",
                for (i, d) in diagnostics.into_iter().enumerate() {
                    {
                        let icon = match d.severity {
                            satz_studio_core::diag::Severity::Error => rsx! {
                                CircleAlert { size: 20, class: "drawer__icon drawer__icon--error" }
                            },
                            satz_studio_core::diag::Severity::Warning => rsx! {
                                TriangleAlert { size: 20, class: "drawer__icon drawer__icon--warning" }
                            },
                            satz_studio_core::diag::Severity::Info => rsx! {
                                Info { size: 20, class: "drawer__icon drawer__icon--info" }
                            },
                        };
                        let headline = d.message.lines().next().unwrap_or_default().to_string();
                        let fix = r.findings.get(i).and_then(|f| f.fix.clone());
                        let supporting = match (d.line, fix) {
                            (Some(l), Some(f)) => format!("line {l} · {f}"),
                            (Some(l), None) => format!("line {l}"),
                            (None, Some(f)) => format!("the whole pack · {f}"),
                            (None, None) => "the whole pack".to_string(),
                        };
                        let is_selected = selected.as_ref() == Some(&d);
                        let mut select = selection.0;
                        rsx! {
                            Item {
                                key: "{i}",
                                size: ItemSize::Sm,
                                onclick: move |_| select.set(Some(d.clone())),
                                "data-selected": if is_selected { "true" } else { "false" },
                                ItemMedia { {icon} }
                                ItemContent {
                                    ItemTitle { "{headline}" }
                                    ItemDescription { "{supporting}" }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "review__source", role: "region", "aria-label": "the pack's text",
                for (n, line) in reviewed.text.lines().enumerate() {
                    {
                        let number = n as u32 + 1;
                        rsx! {
                            div {
                                key: "{number}",
                                id: "review-line-{number}",
                                class: "review__line",
                                class: if marked == Some(number) { "review__line--marked" },
                                span { class: "review__number", "{number}" }
                                span { class: "review__text", "{line}" }
                            }
                        }
                    }
                }
            }
        }
        div { class: "review__destinations",
            Upstream { reviewed: reviewed.clone() }
            Private { reviewed, presets, reviewing }
        }
    }
}

/// Destination A: a pull request to the satz repository, by hand.
#[component]
fn Upstream(reviewed: ReviewedPack) -> Element {
    let app = use_context::<Store<AppStore>>();
    let passed = reviewed.review.passed();
    let name = review::upstream_name(&reviewed.path);
    let path = reviewed.path.display().to_string();
    let folder = reviewed.path.parent().map(Path::to_path_buf);
    rsx! {
        Card { class: "review__destination",
            CardHeader {
                div { class: "review__destination-head",
                    Upload { size: 24 }
                    h3 { class: "review__destination-title", "Upstream" }
                }
            }
            CardContent {
                p { "The pack joins satz's library, for every estate. Sending it is manual until satz ships contribute-pack: a pull request to the satz repository with" }
                ul { class: "review__steps",
                    li {
                        FileText { size: 18 }
                        match &name {
                            Ok(n) => rsx! { span { "the file as " code { "{n}" } ", or in the folder of its kind under " code { "presets/" } } },
                            Err(e) => rsx! { span { "{e}" } },
                        }
                    }
                    li { class: if !passed { "review__step--open" },
                        if passed { CircleCheck { size: 18 } } else { CircleAlert { size: 18 } }
                        span { if passed { "a clean review: this pack clears the bar" } else { "a clean review: every error above answered first" } }
                    }
                    li {
                        HistoryIcon { size: 18 }
                        span { "a row for its version under " code { "## Changelog" } " in " code { "presets/README.md" } }
                    }
                    li {
                        Shield { size: 18 }
                        span { "no organization in the file: the review reads every organization and project id, e-mail address and domain that is not a documented example value as an error above, and each one becomes a param before the pack goes upstream." }
                    }
                }
                div { class: "review__actions",
                    Button {
                        variant: ButtonVariant::Secondary,
                        onclick: move |_| copy_to_clipboard(app, &path),
                        CopyIcon { size: 16 }
                        "Copy the path"
                    }
                    if let Some(folder) = folder {
                        Button {
                            variant: ButtonVariant::Ghost,
                            onclick: move |_| {
                                if let Err(e) = open::that(&folder) {
                                    crate::state::toast(app, crate::state::ToastKind::Error, format!("{}: {e}", folder.display()));
                                }
                            },
                            FolderOpen { size: 16 }
                            "Open the folder"
                        }
                    }
                }
            }
        }
    }
}

/// Destination B: the estate's own library, as `<stem>.local.satz`.
#[component]
fn Private(reviewed: ReviewedPack, presets: PathBuf, reviewing: bool) -> Element {
    let handle = use_coroutine_handle::<EstateAction>();
    let target = review::local_target(&presets, &reviewed.path);
    let placed = reviewed.is_in_library(&presets);
    rsx! {
        Card { class: "review__destination",
            CardHeader {
                div { class: "review__destination-head",
                    Lock { size: 24 }
                    h3 { class: "review__destination-title", "Private" }
                }
            }
            CardContent {
                p { "The pack stays with this estate, in its library, under the " code { ".local.satz" } " name satz's updates never touch. A " code { "use" } " line in the estate is what switches it on." }
                p { class: "review__note", "A type satz's prerequisite table has no row for is an error either way: that row goes upstream, into satz's " code { "src/prerequisites.rs" } ", even for a pack that stays here." }
                match target {
                    Err(e) => rsx! { p { class: "review__refused", role: "alert", "{e}" } },
                    Ok(target) if placed => rsx! {
                        p { class: "review__placed",
                            CircleCheck { size: 18 }
                            span { "This is the library's own " code { "{target.display()}" } "." }
                        }
                    },
                    Ok(target) => rsx! {
                        p { class: "review__target", "Goes to " code { "{target.display()}" } }
                        div { class: "review__actions",
                            Button {
                                disabled: reviewing,
                                onclick: move |_| handle.send(EstateAction::PlacePrivate),
                                ListPlus { size: 16 }
                                "Place in the library"
                            }
                        }
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use satz_studio_core::diag::DiagSource;

    #[test]
    fn the_marked_line_is_the_selected_finding_about_this_pack() {
        let pack = Path::new("/home/packs/team-access.satz");
        let about_pack = Diagnostic::error("m", DiagSource::Command("review-preset".to_string()))
            .at("/home/packs/team-access.satz", 17);
        let elsewhere = Diagnostic::error("m", DiagSource::Check).at("/e/yaml/C0example.satz", 17);
        assert_eq!(marked_line(Some(&about_pack), pack), Some(17));
        assert_eq!(marked_line(Some(&elsewhere), pack), None);
        assert_eq!(marked_line(None, pack), None);
    }
}
