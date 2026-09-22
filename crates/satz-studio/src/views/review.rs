//! The pack review in Packs: `satz review-pack` over a pack the operator chooses, and the
//! two places a reviewed pack goes.
//!
//! The rules are satz's; the view runs the command ([`EstateAction::ReviewPack`]) and
//! shows what it said: the verdict, what the pack emits, each finding — clickable, and
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

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, Icon, LinearProgress, List, ListItem,
    Switch,
};
use crate::state::{
    AppStore, AppStoreStoreExt, DiagnosticSelection, EstateAction, EstateStoreStoreExt,
    PackReviewState,
};
use crate::views::commands::copy_to_clipboard;

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

/// The line the drawer's selection marks in the pack's text: the selected diagnostic's
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
        Card { variant: CardVariant::Outlined, class: "review",
            div { class: "review__head",
                Icon { name: "rate_review", size: 24 }
                h2 { class: "review__title", "Review a pack" }
                span { class: "grow" }
                Switch {
                    checked: against(),
                    label: "Judge it inside {open.name}",
                    disabled: reviewing,
                    onchange: move |v: bool| against.set(v),
                }
                Button {
                    variant: ButtonVariant::Filled,
                    icon: "file_open",
                    disabled: reviewing,
                    onclick: {
                        let start = start.clone();
                        move |_| choose_and_review(handle, start.clone(), against())
                    },
                    "Choose a pack…"
                }
            }
            p { class: "review__lead",
                "satz review-pack holds a pack to the library's own bar: it parses, it is formatted, its header says what it is, its version has a changelog row, it declares no membership, it runs no legacy constraint beside its managed replacement, every type it emits has a prerequisite row, and it compiles. The pack is folded into an estate made of the documented example values, or into "
                code { "{open.name}" }
                " with the switch on — for a pack that estate uses. Each finding is in the drawer at its line."
            }
            if reviewing {
                LinearProgress {}
            }
            match review {
                None => rsx! {},
                Some(PackReviewState::Failed { pack, error }) => rsx! {
                    div { class: "review__failed", role: "alert",
                        Icon { name: "error", size: 20 }
                        div { class: "grow",
                            code { "{pack.display()}" }
                            p { "{error}" }
                        }
                        Button {
                            variant: ButtonVariant::Tonal,
                            icon: "refresh",
                            disabled: reviewing,
                            onclick: {
                                let pack = pack.clone();
                                move |_| handle.send(EstateAction::ReviewPack { pack: pack.clone(), against: against() })
                            },
                            "Review again"
                        }
                        Button { variant: ButtonVariant::Text, icon: "close", onclick: move |_| handle.send(EstateAction::CloseReview), "Close" }
                    }
                },
                Some(PackReviewState::Reviewed(reviewed)) => rsx! {
                    Reviewed { reviewed, presets: presets.clone(), reviewing }
                },
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
    let emits = if r.emits.is_empty() {
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
            Chip {
                kind: ChipKind::Assist,
                icon: if passed { "check_circle" } else { "error" },
                label: if passed { "clears the bar" } else { "does not clear the bar yet" },
                error: !passed,
            }
            Chip { kind: ChipKind::Assist, icon: "error", label: format!("{errors} errors"), error: errors > 0 }
            Chip { kind: ChipKind::Assist, icon: "warning", label: format!("{warnings} warnings") }
            Chip { kind: ChipKind::Assist, icon: "info", label: format!("{infos} notes") }
            Button {
                variant: ButtonVariant::Tonal,
                icon: "refresh",
                disabled: reviewing,
                onclick: {
                    let path = path.clone();
                    let against = reviewed.against;
                    move |_| handle.send(EstateAction::ReviewPack { pack: path.clone(), against })
                },
                "Review again"
            }
            Button { variant: ButtonVariant::Text, icon: "close", onclick: move |_| handle.send(EstateAction::CloseReview), "Close" }
        }
        p { class: "review__emits", "{folded} · emits {emits}" }
        div { class: "review__panes",
            List { class: "review__findings",
                for (i, d) in diagnostics.into_iter().enumerate() {
                    {
                        let icon = match d.severity {
                            satz_studio_core::diag::Severity::Error => "error",
                            satz_studio_core::diag::Severity::Warning => "warning",
                            satz_studio_core::diag::Severity::Info => "info",
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
                            ListItem {
                                key: "{i}",
                                headline,
                                supporting,
                                selected: is_selected,
                                leading: rsx! { Icon { name: icon, size: 20, class: "drawer__icon drawer__icon--{icon}" } },
                                onclick: move |_| select.set(Some(d.clone())),
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
        Card { variant: CardVariant::Filled, class: "review__destination",
            div { class: "review__destination-head",
                Icon { name: "publish", size: 24 }
                h3 { class: "review__destination-title", "Upstream" }
            }
            p { "The pack joins satz's library, for every estate. The hand-over is manual until satz ships contribute-pack: a pull request to the satz repository with" }
            ul { class: "review__steps",
                li {
                    Icon { name: "description", size: 18 }
                    match &name {
                        Ok(n) => rsx! { span { "the file as " code { "{n}" } ", or in the folder of its kind under " code { "presets/" } } },
                        Err(e) => rsx! { span { "{e}" } },
                    }
                }
                li { class: if !passed { "review__step--open" },
                    Icon { name: if passed { "check_circle" } else { "error" }, size: 18 }
                    span { if passed { "a clean review: this pack clears the bar" } else { "a clean review: every error above answered first" } }
                }
                li {
                    Icon { name: "history", size: 18 }
                    span { "a row for its version under " code { "## Changelog" } " in " code { "presets/README.md" } }
                }
                li {
                    Icon { name: "shield", size: 18 }
                    span { "no organisation in the file: the review reads every organisation and project id, e-mail address and domain that is not a documented example value as an error above, and each one becomes a param before the pack goes upstream." }
                }
            }
            div { class: "review__actions",
                Button {
                    variant: ButtonVariant::Tonal,
                    icon: "content_copy",
                    onclick: move |_| copy_to_clipboard(app, &path),
                    "Copy the path"
                }
                if let Some(folder) = folder {
                    Button {
                        variant: ButtonVariant::Text,
                        icon: "folder_open",
                        onclick: move |_| {
                            if let Err(e) = open::that(&folder) {
                                crate::state::toast(app, crate::state::ToastKind::Error, format!("{}: {e}", folder.display()));
                            }
                        },
                        "Open the folder"
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
        Card { variant: CardVariant::Filled, class: "review__destination",
            div { class: "review__destination-head",
                Icon { name: "lock", size: 24 }
                h3 { class: "review__destination-title", "Private" }
            }
            p { "The pack stays with this estate, in its library, under the " code { ".local.satz" } " name satz's updates never touch. A " code { "use" } " line in the estate is what switches it on." }
            p { class: "review__note", "A type satz's prerequisite table has no row for is an error either way: that row goes upstream, into satz's " code { "src/prerequisites.rs" } ", even for a pack that stays here." }
            match target {
                Err(e) => rsx! { p { class: "review__refused", role: "alert", "{e}" } },
                Ok(target) if placed => rsx! {
                    p { class: "review__placed",
                        Icon { name: "check_circle", size: 18 }
                        span { "This is the library's own " code { "{target.display()}" } "." }
                    }
                },
                Ok(target) => rsx! {
                    p { class: "review__target", "Goes to " code { "{target.display()}" } }
                    div { class: "review__actions",
                        Button {
                            variant: ButtonVariant::Filled,
                            icon: "library_add",
                            disabled: reviewing,
                            onclick: move |_| handle.send(EstateAction::PlacePrivate),
                            "Place in the library"
                        }
                    }
                },
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
        let about_pack = Diagnostic::error("m", DiagSource::Command("review-pack".to_string()))
            .at("/home/packs/team-access.satz", 17);
        let elsewhere = Diagnostic::error("m", DiagSource::Check).at("/e/yaml/C0example.satz", 17);
        assert_eq!(marked_line(Some(&about_pack), pack), Some(17));
        assert_eq!(marked_line(Some(&elsewhere), pack), None);
        assert_eq!(marked_line(None, pack), None);
    }
}
