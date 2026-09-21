//! The notice dialog: what a pack asks to be run once it is switched on.
//!
//! satz returns a notice in the report of the call that opened it — an answer that
//! switched the pack on, or a merge that brought it in — and the window raises it here
//! rather than letting it pass. One notice at a time, over whatever destination the
//! operator is on, because the answer that opened it can be given in Decisions or in
//! Packs and the command it names is owed either way.
//!
//! Its three actions are the three things there are to do with it: **Run it** hands the
//! command to the app's log — over the Overview, where that log stands beside the
//! notice's own row — or to the OS terminal where the app runs that command (`apply`,
//! `bootstrap`, `migrate`); **I ran it** binds the notice's param `true`
//! through `satz_interview`, which is what an acknowledgement IS (satz's ADR 0033); and
//! **Later** lowers the dialog and leaves the notice standing — the Overview's card
//! counts it, the drawer carries satz's own sentence for it, and the estate's apply
//! stays refused while it holds one up.

use dioxus::prelude::*;
use satz_studio_core::satz::reports::NoticeRow;

use crate::components::{Button, ButtonVariant, Dialog};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, View};
use crate::views::commands::runs_in_terminal;

/// The placeholder satz writes where the estate file goes in a notice's command.
const ESTATE: &str = "<estate>";

/// The arguments after `satz` of the command a notice names, with `<estate>` filled in
/// — what the app would run. `Err` says why the app will not run this one, which the
/// dialog shows in place of the button: the command is the operator's to run either
/// way, and a command the app cannot parse is never guessed at.
pub fn notice_command(run: &str, estate: &str) -> Result<Vec<String>, String> {
    if run.contains('\'') || run.contains('"') {
        return Err("the command quotes a word, so the app cannot split it into arguments — run it yourself".to_string());
    }
    let mut words = run.split_whitespace().map(|w| {
        if w == ESTATE {
            estate.to_string()
        } else {
            w.to_string()
        }
    });
    if words.next().as_deref() != Some("satz") {
        return Err(
            "this is not a satz command, and the app runs satz — run it yourself".to_string(),
        );
    }
    let args: Vec<String> = words.collect();
    if let Some(left) = args.iter().find(|w| w.starts_with('<') && w.ends_with('>')) {
        return Err(format!(
            "the command carries `{left}`, which the app has no value for — run it yourself"
        ));
    }
    if args.is_empty() {
        return Err("the command is `satz` with nothing after it".to_string());
    }
    Ok(args)
}

/// The sentence under the command: what binding the param means, and what stays refused
/// until it is bound.
pub fn consequence(n: &NoticeRow) -> String {
    let bind = format!(
        "Running it is acknowledged by binding `{} = true` in this estate's params, which is what \"I ran it\" does.",
        n.param
    );
    if n.holds_up_apply() {
        format!("{bind} apply and bootstrap refuse this estate while the notice is open.")
    } else {
        bind
    }
}

#[component]
pub fn NoticeDialog() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let notices = app.estate().notices().cloned();
    let open = app.estate().notices_open().cloned();
    let estate = app.open().read().as_ref().map(|o| o.name.clone());
    let (Some(notice), Some(estate)) = (notices.first().cloned(), estate) else {
        return rsx! {};
    };
    let command = notice_command(&notice.run, &estate);
    let of = if notices.len() > 1 {
        format!(" · 1 of {}", notices.len())
    } else {
        String::new()
    };
    let param = notice.param.clone();
    let title = format!("{} asks for a command{of}", short_pack(&notice.pack));

    rsx! {
        Dialog {
            open,
            title,
            icon: "assignment_late",
            class: "notice-dialog",
            ondismiss: move |_| app.estate().notices_open().set(false),
            actions: rsx! {
                Button {
                    variant: ButtonVariant::Text,
                    onclick: move |_| app.estate().notices_open().set(false),
                    "Later"
                }
                if let Ok(args) = command.clone() {
                    Button {
                        variant: ButtonVariant::Tonal,
                        icon: if runs_in_terminal(&args) { "open_in_new" } else { "play_arrow" },
                        onclick: move |_| {
                            app.estate().notices_open().set(false);
                            if runs_in_terminal(&args) {
                                handle.send(EstateAction::OpenInTerminal(args.clone()));
                            } else {
                                // the command streams into the estate's log, and the
                                // Overview is where that log stands beside the notice's
                                // own row: a live run nobody can watch is worse than a
                                // destination the operator did not ask for
                                app.nav().set(View::Overview);
                                handle.send(EstateAction::RunNoticeCommand(args.clone()));
                            }
                        },
                        "Run it"
                    }
                }
                Button {
                    variant: ButtonVariant::Filled,
                    icon: "check",
                    onclick: move |_| {
                        handle.send(EstateAction::Answer {
                            subject: param.clone(),
                            value: serde_json::Value::Bool(true),
                        });
                    },
                    "I ran it"
                }
            },
            p { class: "notice-dialog__pack", code { "{notice.pack}" } }
            p { "{notice.text}" }
            pre { class: "notice-dialog__command", code { "{notice.run}" } }
            if let Err(why) = &command {
                p { class: "notice-dialog__refusal", "{why}" }
            }
            p { class: "notice-dialog__consequence", {consequence(&notice)} }
        }
    }
}

/// The pack as the dialog's headline names it: the file name without its directories
/// and its extension, which is the pack an operator switched on.
fn short_pack(pack: &str) -> String {
    pack.rsplit('/')
        .next()
        .unwrap_or(pack)
        .trim_end_matches(".satz")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use satz_studio_core::satz::reports::FindingSeverity;

    fn notice() -> NoticeRow {
        NoticeRow {
            param: "cis_baseline_adopted".to_string(),
            pack: "presets/cis/CIS-GCP-Foundation-4.0.satz".to_string(),
            text: "Import what is live first.".to_string(),
            run: "satz adopt <estate> --execute --import".to_string(),
            severity: FindingSeverity::Error,
            acknowledged: false,
        }
    }

    #[test]
    fn the_estate_takes_the_place_of_the_placeholder_and_satz_is_dropped() {
        assert_eq!(
            notice_command(&notice().run, "C0example.satz").unwrap(),
            ["adopt", "C0example.satz", "--execute", "--import"]
        );
    }

    #[test]
    fn a_command_the_app_will_not_run_says_why_instead_of_being_guessed_at() {
        for (run, word) in [
            ("gcloud org-policies list", "not a satz command"),
            ("satz adopt <estate> --file 'a b'", "quotes a word"),
            ("satz adopt <org> --execute", "<org>"),
            ("satz", "nothing after it"),
        ] {
            let e = notice_command(run, "C0example.satz").unwrap_err();
            assert!(e.contains(word), "`{run}` said `{e}`");
        }
    }

    #[test]
    fn the_command_a_notice_names_goes_where_the_app_runs_that_command() {
        let adopt = notice_command(&notice().run, "C0example.satz").unwrap();
        assert!(!runs_in_terminal(&adopt), "adopt runs in the app's log");
        let bootstrap = notice_command("satz bootstrap <estate>", "C0example.satz").unwrap();
        assert!(
            runs_in_terminal(&bootstrap),
            "bootstrap is the operator's terminal (ADR 0006)"
        );
    }

    #[test]
    fn the_consequence_names_the_param_and_what_stays_refused() {
        let said = consequence(&notice());
        assert!(said.contains("cis_baseline_adopted = true"), "{said}");
        assert!(said.contains("apply and bootstrap refuse"), "{said}");
        let mut soft = notice();
        soft.severity = FindingSeverity::Warning;
        assert!(!consequence(&soft).contains("refuse"));
    }

    #[test]
    fn the_headline_names_the_pack_and_not_its_path() {
        assert_eq!(
            short_pack("presets/cis/CIS-GCP-Foundation-4.0.satz"),
            "CIS-GCP-Foundation-4.0"
        );
    }
}
