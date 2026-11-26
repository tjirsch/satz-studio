//! The Security destination: which security checks the estate's own resources fail, as
//! hints for whoever applies them or writes the pack that declares them
//! ([ADR 0035](../../../../docs/adr/0035-security-is-a-destination-of-hints-on-the-estates-own-resources.md)).
//!
//! "Scan" runs `satz scan <estate> --format json` ([`EstateAction::Scan`]): Checkov over
//! the HCL code the last transpile wrote, each failed check with the Satz file and line
//! that declared its resource. The checks and their words are Checkov's, the join to the
//! estate is satz's; the view lists them by resource and judges nothing. It reads the
//! estate's own code only, so it needs no sign-in and says nothing about the
//! organization, and it is not a review: the security review is an agent's skill, started
//! from Documents.

use dioxus::prelude::*;
use dioxus_icons::lucide::{ExternalLink, ScanSearch, ShieldAlert};
use satz_studio_core::satz::reports::ScanReport;

use crate::components::badge::{Badge, BadgeVariant};
use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::card::{
    Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle,
};
use crate::components::item::{
    Item, ItemContent, ItemDescription, ItemGroup, ItemMedia, ItemTitle, ItemVariant,
};
use crate::components::progress::Progress;
use crate::state::{
    AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, ToastKind, toast,
};

/// The card's line under its title: what the last scan counted, or what the scan is
/// before it has run.
pub fn summary(scan: Option<&ScanReport>, transpiled: bool) -> String {
    match scan {
        Some(r) => format!(
            "Checkov {} over {} resources: {} failed, {} passed, {} skipped.",
            r.checkov_version, r.resource_count, r.failed, r.passed, r.skipped
        ),
        None if transpiled => "Checkov's checks over the HCL code the last transpile wrote. Checkov runs from PATH, else through uvx, and takes a while.".to_string(),
        None => "Nothing is transpiled yet: the transpile on Apply writes the HCL code the scan reads.".to_string(),
    }
}

#[component]
pub fn SecurityView() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let scan = app.estate().scan().cloned();
    let scanning = app.estate().scanning().cloned();
    let transpiled = app.estate().hcl().cloned().transpiled;
    let report = scan.as_ref().and_then(|s| s.as_ref().ok());

    rsx! {
        div { class: "view security",
            h1 { class: "view__title", "Security" }
            p { class: "view__lead",
                "Hints on what this estate declares: the security checks its own resources fail, each at the line that declares it. A failed check is a place to look, to change, or to justify; it reads no organization and is not a review — the security review is an agent's, on Documents."
            }
            Card {
                CardHeader {
                    CardTitle { "Checkov scan" }
                    CardDescription { "{summary(report, transpiled)}" }
                    CardAction {
                        Button {
                            variant: if scan.is_some() { ButtonVariant::Secondary } else { ButtonVariant::Primary },
                            disabled: scanning || !transpiled,
                            onclick: move |_| handle.send(EstateAction::Scan),
                            ScanSearch { size: 16 }
                            if scan.is_some() { "Scan again" } else { "Scan" }
                        }
                    }
                }
                if scanning || scan.is_some() {
                    CardContent {
                        if scanning {
                            Progress { value: None::<f64>, "aria-label": "scanning" }
                        }
                        match &scan {
                            None => rsx! {},
                            Some(Err(e)) => rsx! { p { class: "security__error", "{e}" } },
                            Some(Ok(r)) if r.findings.is_empty() => rsx! {
                                p { class: "security__none", "No check failed." }
                            },
                            Some(Ok(r)) => rsx! {
                                ItemGroup { class: "security__list",
                                    for (resource, declared_at, checks) in r.by_resource() {
                                        Item { key: "{resource}", variant: ItemVariant::Outline,
                                            ItemMedia { ShieldAlert { size: 20 } }
                                            ItemContent {
                                                ItemTitle { code { "{resource}" } }
                                                ItemDescription {
                                                    // a named use's resource: the line serves every use of its pack
                                                    match (declared_at, checks.first().and_then(|f| f.named.as_deref())) {
                                                        (Some(at), Some(name)) => format!("declared at {at}, written by the named use {name}"),
                                                        (Some(at), None) => format!("declared at {at}"),
                                                        (None, _) => "not traced to a block of the estate".to_string(),
                                                    }
                                                }
                                                div { class: "security__checks",
                                                    for f in checks {
                                                        div { key: "{f.check_id}", class: "security__check",
                                                            Badge { variant: BadgeVariant::Outline, "{f.check_id}" }
                                                            span { "{f.check_name}" }
                                                            if let Some(url) = f.guideline.clone() {
                                                                Button {
                                                                    variant: ButtonVariant::Ghost,
                                                                    size: ButtonSize::Xs,
                                                                    onclick: move |_| {
                                                                        if let Err(e) = open::that(&url) {
                                                                            toast(app, ToastKind::Error, format!("{url}: {e}"));
                                                                        }
                                                                    },
                                                                    ExternalLink { size: 14 }
                                                                    "Guideline"
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            },
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

    fn report() -> ScanReport {
        ScanReport {
            estate: "/e/workload.satz".into(),
            hcl_dir: "hcl".into(),
            checkov_version: "3.4.6".into(),
            passed: 12,
            failed: 3,
            skipped: 0,
            resource_count: 6,
            findings: vec![],
        }
    }

    /// The line under the title counts the last scan, says what the scan reads before it
    /// has run, and sends to Apply's transpile while nothing is written.
    #[test]
    fn the_summary_counts_the_scan_or_says_what_it_needs() {
        assert_eq!(
            summary(Some(&report()), true),
            "Checkov 3.4.6 over 6 resources: 3 failed, 12 passed, 0 skipped."
        );
        assert!(summary(None, true).contains("the last transpile wrote"));
        assert!(summary(None, false).contains("Nothing is transpiled yet"));
    }
}
