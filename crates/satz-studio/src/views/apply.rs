//! The Apply destination: the estate's way to the organisation, in the order it is
//! walked. Who runs it — the sign-in check, `satz_whoami` — then what state it is in,
//! then the steps: the checks that go further than the compile every reload runs, day 0,
//! the generated code, its directory, the plan and the apply
//! ([ADR 0028](../../../../docs/adr/0028-apply-is-one-destination-and-starts-with-who-is-signed-in.md)).
//!
//! `transpile`, `hcl-init` and `plan` run in the app. The three that change a live
//! organisation run in your terminal: `apply` because the tool's approval prompt is the
//! safety step, `bootstrap` because it creates day 0 as you, and `migrate` because it
//! rewrites the estate and copies the state to the other backend without asking
//! ([ADR 0006](../../../../docs/adr/0006-apply-and-bootstrap-run-in-the-users-terminal.md),
//! [ADR 0012](../../../../docs/adr/0012-migrate-hands-off-to-the-terminal.md)).
//!
//! `update-prerequisites` is the one entry here that can WRITE. The deck runs it
//! `--report-only`, because a command cannot rewrite the estate under the window; the
//! writing run is its own button, which goes through satz's own writer under the
//! estate's write lock and is followed by a reload, like every other write.

use dioxus::prelude::*;
use satz_studio_core::diag::Severity;
use satz_studio_core::satz::reports::WhoamiReport;

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, Icon, LinearProgress,
};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, SignIn, View};
use crate::views::commands::{APPLY, CommandDeck};
use crate::views::path::NextStep;

/// The command that signs in, as satz's own refusal names it.
const SIGN_IN: &str = "gcloud auth application-default login";

/// What the sign-in card says in its first line, and whether it is an error.
pub fn sign_in_headline(sign_in: Option<&SignIn>) -> (String, bool) {
    match sign_in {
        None | Some(SignIn::Checking) => ("Checking who is signed in…".to_string(), false),
        Some(SignIn::Refused(_)) => ("Not signed in".to_string(), true),
        Some(SignIn::Known(report)) => match (report.ready(), report.adc.account.as_deref()) {
            (Some(false), _) => ("Signed in, and the estate cannot run".to_string(), true),
            (_, Some(account)) => (format!("Signed in as {account}"), false),
            (_, None) => ("Signed in".to_string(), false),
        },
    }
}

/// The lines under the headline: what the estate runs as, the project its calls are
/// billed to, and the permission test — each with whether it is in order.
pub fn sign_in_lines(report: &WhoamiReport) -> Vec<(String, bool)> {
    let mut lines = Vec::new();
    if let Some(estate) = &report.estate {
        match (&estate.service_account, estate.impersonated) {
            (Some(account), true) => lines.push((
                match estate.may_impersonate {
                    Some(false) => format!(
                        "The estate runs as {account}, and these credentials may not act as it"
                    ),
                    _ => format!("The estate runs as {account}"),
                },
                estate.may_impersonate != Some(false),
            )),
            _ => lines.push((
                format!(
                    "The estate runs as the signed-in identity itself ({} mode)",
                    estate.deployment_mode
                ),
                true,
            )),
        }
        if let Some(error) = &estate.error {
            lines.push((error.clone(), false));
        }
    }
    if let Some(quota) = &report.quota_project {
        lines.push((
            match quota.reachable {
                Some(false) => format!(
                    "Calls are billed to {}, which these credentials cannot reach",
                    quota.id
                ),
                _ => format!("Calls are billed to {}", quota.id),
            },
            quota.reachable != Some(false),
        ));
        if let Some(error) = &quota.error {
            lines.push((error.clone(), false));
        }
    }
    if let Some(p) = &report.permissions {
        lines.push((
            match p.missing.len() {
                0 => format!("{} permissions tested, none missing", p.tested),
                n => format!(
                    "{} permissions tested, {n} missing — the Diagnostics bar names each",
                    p.tested
                ),
            },
            p.missing.is_empty(),
        ));
    }
    lines
}

#[component]
pub fn ApplyView() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let open = app.open().cloned();
    let hcl = app.estate().hcl().cloned();
    let diagnostics = app.estate().diagnostics().cloned();
    let loading = app.estate().loading().cloned();
    let sign_in = app.estate().sign_in().cloned();
    // asked once per estate when the destination is entered, never at open: the check
    // mints a token, which is a network call
    use_effect(move || {
        if app.estate().sign_in().peek().is_none() {
            handle.send(EstateAction::CheckSignIn);
        }
    });
    let Some(open) = open else {
        return rsx! {};
    };
    let dir = open.session.dir.hcl_dir();
    let compiles = !diagnostics.iter().any(|d| d.severity == Severity::Error);
    let prerequisites: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.kind.as_deref() == Some("prerequisites") && d.severity != Severity::Info)
        .map(|d| d.message.clone())
        .collect();
    let (headline, failed) = sign_in_headline(sign_in.as_ref());

    rsx! {
        div { class: "view apply",
            h1 { class: "view__title", "Apply" }
            p { class: "view__lead", "The estate's way to the organisation: who runs it, what state it is in, and the steps in the order they are walked." }

            Card { variant: CardVariant::Outlined, class: "apply__signin",
                header { class: "apply__head",
                    Icon { name: if failed { "no_accounts" } else { "account_circle" }, size: 22 }
                    h2 { class: "apply__title", class: if failed { "apply__title--error" }, "{headline}" }
                    span { class: "grow" }
                    Button {
                        variant: ButtonVariant::Text,
                        icon: "refresh",
                        disabled: sign_in == Some(SignIn::Checking),
                        onclick: move |_| handle.send(EstateAction::CheckSignIn),
                        "Check again"
                    }
                }
                match &sign_in {
                    None | Some(SignIn::Checking) => rsx! { LinearProgress {} },
                    Some(SignIn::Refused(text)) => rsx! {
                        p { class: "apply__line apply__line--error", "{text}" }
                        p { class: "apply__note",
                            "The steps that call Google will fail until you are signed in. In a terminal: "
                            code { "{SIGN_IN}" }
                            " — then \"Check again\"."
                        }
                    },
                    Some(SignIn::Known(report)) => rsx! {
                        for (i, (line, ok)) in sign_in_lines(report).into_iter().enumerate() {
                            p { key: "{i}", class: "apply__line", class: if !ok { "apply__line--error" },
                                Icon { name: if ok { "check_circle" } else { "error" }, size: 18 }
                                "{line}"
                            }
                        }
                        if let Some(note) = &report.note {
                            p { class: "apply__note", "{note}" }
                        }
                    },
                }
            }

            Card { variant: CardVariant::Filled, class: "apply__state",
                Icon { name: "folder_special", size: 22 }
                code { class: "apply__dir", "{dir.display()}" }
                span { class: "grow" }
                Chip {
                    kind: ChipKind::Assist,
                    icon: if compiles { "check_circle" } else { "error" },
                    label: if compiles { "compiles" } else { "does not compile" },
                    error: !compiles,
                }
                Chip {
                    kind: ChipKind::Assist,
                    icon: if prerequisites.is_empty() { "check_circle" } else { "pending" },
                    label: if prerequisites.is_empty() { "rights declared" } else { "rights not declared" },
                }
                Chip {
                    kind: ChipKind::Assist,
                    icon: if hcl.transpiled { "check_circle" } else { "pending" },
                    label: if hcl.transpiled { "main.tf written" } else { "nothing emitted" },
                }
                Chip {
                    kind: ChipKind::Assist,
                    icon: if hcl.initialised { "check_circle" } else { "pending" },
                    label: if hcl.initialised { "initialised" } else { "not initialised" },
                }
            }

            if !prerequisites.is_empty() {
                Card { variant: CardVariant::Outlined, class: "apply__prerequisites",
                    header { class: "apply__head",
                        Icon { name: "admin_panel_settings", size: 22 }
                        h2 { class: "apply__title", "The prerequisites are not declared" }
                    }
                    for (i, message) in prerequisites.iter().enumerate() {
                        p { key: "{i}", class: "apply__finding", "{message}" }
                    }
                    p { class: "apply__note", "Writing them binds the roles on the estate's IaC service account and each API on the project its calls are billed to — the infrastructure project, or the project a resource sits inside — in the estate file; an API for a project a pack declares is named, not written. satz works both out offline, from the resource types this estate emits; the write goes through the same check and rollback as an answer." }
                    div { class: "apply__actions",
                        Button {
                            variant: ButtonVariant::Filled,
                            icon: "edit_note",
                            disabled: loading,
                            onclick: move |_| handle.send(EstateAction::WritePrerequisites),
                            "Write them into the estate"
                        }
                    }
                }
            }

            CommandDeck { ids: APPLY.to_vec() }
            NextStep { from: View::Apply }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use satz_studio_core::satz::reports::{
        WhoamiCredential, WhoamiEstate, WhoamiPermissions, WhoamiQuotaProject,
    };

    fn report() -> WhoamiReport {
        WhoamiReport {
            adc: WhoamiCredential {
                account: Some("operator@example.com".into()),
                kind: "user-adc".into(),
                file: None,
            },
            estate: Some(WhoamiEstate {
                path: "C0example.satz".into(),
                deployment_mode: "cloud".into(),
                service_account: Some("svc-iac@acme-infra-001.iam.gserviceaccount.com".into()),
                impersonated: true,
                may_impersonate: Some(true),
                error: None,
            }),
            quota_project: Some(WhoamiQuotaProject {
                id: "acme-infra-001".into(),
                reachable: Some(true),
                error: None,
            }),
            offline: false,
            note: None,
            permissions: Some(WhoamiPermissions {
                tested: 12,
                missing: vec![],
                not_tested: vec![],
            }),
        }
    }

    /// The card's first line says who is signed in, that nobody is, or that somebody is
    /// and the estate still cannot run.
    #[test]
    fn the_headline_tells_the_three_cases_apart() {
        assert_eq!(
            sign_in_headline(None),
            ("Checking who is signed in…".to_string(), false)
        );
        assert_eq!(
            sign_in_headline(Some(&SignIn::Refused("no token".into()))),
            ("Not signed in".to_string(), true)
        );
        let ok = SignIn::Known(Box::new(report()));
        assert_eq!(
            sign_in_headline(Some(&ok)),
            ("Signed in as operator@example.com".to_string(), false)
        );
        let mut blocked = report();
        blocked.estate.as_mut().unwrap().may_impersonate = Some(false);
        assert_eq!(
            sign_in_headline(Some(&SignIn::Known(Box::new(blocked)))),
            ("Signed in, and the estate cannot run".to_string(), true)
        );
    }

    /// Every line carries whether it is in order: the account, the billed project and
    /// the permission test.
    #[test]
    fn the_lines_name_the_account_the_billed_project_and_the_permissions() {
        let lines = sign_in_lines(&report());
        assert_eq!(
            lines,
            [
                (
                    "The estate runs as svc-iac@acme-infra-001.iam.gserviceaccount.com".to_string(),
                    true
                ),
                ("Calls are billed to acme-infra-001".to_string(), true),
                ("12 permissions tested, none missing".to_string(), true),
            ]
        );
        let mut broken = report();
        broken.quota_project.as_mut().unwrap().reachable = Some(false);
        broken.permissions.as_mut().unwrap().missing = vec![serde_json::json!({})];
        let lines = sign_in_lines(&broken);
        assert!(
            !lines[1].1 && lines[1].0.contains("cannot reach"),
            "{lines:?}"
        );
        assert!(!lines[2].1 && lines[2].0.contains("1 missing"), "{lines:?}");

        let mut local = report();
        let estate = local.estate.as_mut().unwrap();
        estate.impersonated = false;
        estate.deployment_mode = "local".into();
        assert_eq!(
            sign_in_lines(&local)[0],
            (
                "The estate runs as the signed-in identity itself (local mode)".to_string(),
                true
            )
        );
    }
}
