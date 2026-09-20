//! The Overview: which estate this is, and what it still has to do.
//!
//! The first card answers "am I in the right estate": the answers the estate gave to its
//! OWN questions — the ones the `estate_core` pack declares, the customer and the
//! organisation it stands for, the infrastructure it names, the identity it runs as —
//! read from the questions report, never stored. [`identity`] is that derivation, pure
//! over the report, and a question the report does not carry is simply not a row.
//!
//! The card of owed items is DERIVED from the estate's own state on every render — the
//! questions report, the pack rows, the schema, the compile's findings, the generated
//! HCL directory — so it cannot drift from the estate, and it disappears when there is
//! nothing left in it. Nothing is saved: there is no workflow to resume, no step to be
//! trapped in, and no record of how the estate reached the app. An estate that was
//! created, one that was imported and one that was opened show the same list, because
//! the same facts are true of them — whether git holds the estate in a repository
//! included, which is a fact of the directory and not of the door it came through.
//!
//! [`owed`] is the whole derivation, pure over [`Facts`], so what the card says is
//! testable without a window.

use dioxus::prelude::*;
use satz_studio_core::diag::Diagnostic;
use satz_studio_core::estate::HclState;
use satz_studio_core::git::WorkTree;
use satz_studio_core::model::{EstateModel, LineState, PackRowKind, SchemaStatus};
use satz_studio_core::satz::reports::{QuestionRow, QuestionState, QuestionsReport};

use crate::components::{Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, Icon};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, View};
use crate::views::commands::CommandLog;

/// What the estate is, as the Overview reads it. Borrowed rather than cloned: this is
/// built on every render.
pub struct Facts<'a> {
    /// the main file's name, as a satz command takes it
    pub estate: &'a str,
    pub deployment_mode: Option<&'a str>,
    pub hcl: HclState,
    /// whether git holds the estate file's directory in a work tree; `None` until the
    /// first reload has asked
    pub work_tree: Option<&'a WorkTree>,
    pub questions: Option<&'a QuestionsReport>,
    pub model: Option<&'a EstateModel>,
    pub diagnostics: &'a [Diagnostic],
}

/// What a row offers to do about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Remedy {
    /// the destination where the work is done
    Go(View),
    /// a satz command, run in the app and streamed into the log below
    Run {
        label: &'static str,
        icon: &'static str,
        args: Vec<String>,
    },
    /// a satz command handed to the OS terminal
    Terminal {
        label: &'static str,
        icon: &'static str,
        args: Vec<String>,
    },
    /// `satz merge-presets`, under the estate's write lock
    Merge,
    /// `git init -b main`, `git add -A` and one commit in the estate directory, run when
    /// the operator presses it and never on the app's own account
    InitRepository,
}

/// One thing the estate still owes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owed {
    /// stable, so a test names a row without matching its prose
    pub id: &'static str,
    pub icon: &'static str,
    pub title: String,
    pub detail: String,
    pub remedies: Vec<Remedy>,
}

/// Everything the estate owes, in the order the work happens. Each row rests on a fact
/// that is read, never on a mode the app remembers.
pub fn owed(f: &Facts) -> Vec<Owed> {
    let mut out = Vec::new();

    // The undo every later preset update needs. `satz merge-presets` edits the estate file
    // in place and asks git whether that edit can be taken back; outside a work tree, or
    // with no git to ask, it refuses. `satz init` makes no repository, so this is owed
    // from the first minute and would otherwise surface at the first preset update.
    match f.work_tree {
        Some(WorkTree::Outside(said)) => out.push(Owed {
            id: "repository",
            icon: "commit",
            title: "The estate is not in a git repository".to_string(),
            detail: format!(
                "`satz merge-presets` edits the estate in place and uses git as the undo of that edit, so outside a repository it refuses and the estate cannot take preset updates. git says: {said}\nThe button runs `git init -b main`, `git add -A` and one commit in the estate directory, with the identity git is configured with. `git add -A` commits everything the directory's `.gitignore` does not exclude, which is why it refuses a directory without one."
            ),
            remedies: vec![Remedy::InitRepository],
        }),
        Some(WorkTree::NoGit(error)) => out.push(Owed {
            id: "repository",
            icon: "commit",
            title: "git is not available".to_string(),
            detail: format!(
                "`satz merge-presets` uses git as the undo of its edit to the estate and refuses without it, so the estate cannot take preset updates until git is installed and on the PATH. {error}"
            ),
            remedies: Vec::new(),
        }),
        Some(WorkTree::Inside) | None => {}
    }

    // Day 0. The app cannot see a live organisation without calling one, so this rests
    // on what is here: a cloud estate keeps its state in the bucket `bootstrap`
    // creates, and an HCL directory that was never initialised has never reached it.
    if f.deployment_mode == Some("cloud") && !f.hcl.initialised {
        out.push(Owed {
            id: "bootstrap",
            icon: "foundation",
            title: "Day 0 is not confirmed".to_string(),
            detail: format!(
                "This estate keeps its state in a bucket that `satz bootstrap` creates, together with the folder, the infrastructure project and the billing link. Nothing has been initialised against that bucket from this checkout, so day 0 is either still owed or was done elsewhere. The dry run answers it: it prints the plan, resolves the identity your credentials give and runs the permission pre-flight, and creates nothing. The real run creates that infrastructure as you — the IaC service account does not exist yet — so it stays in your terminal.\nsatz bootstrap {} --dry-run",
                f.estate
            ),
            remedies: vec![
                Remedy::Run {
                    label: "Check day 0",
                    icon: "fact_check",
                    args: vec![
                        "bootstrap".to_string(),
                        f.estate.to_string(),
                        "--dry-run".to_string(),
                    ],
                },
                Remedy::Terminal {
                    label: "Bootstrap in your terminal",
                    icon: "open_in_new",
                    args: vec!["bootstrap".to_string(), f.estate.to_string()],
                },
            ],
        });
    }

    if let Some(q) = f.questions
        && q.summary.unanswered > 0
    {
        let blocking = q.summary.blocking;
        out.push(Owed {
            id: "questions",
            icon: "quiz",
            title: match q.summary.unanswered {
                1 => "1 question is unanswered".to_string(),
                n => format!("{n} questions are unanswered"),
            },
            detail: match blocking {
                0 => "Every one of them offers a default, so they can be bound in one pass.".to_string(),
                n => format!(
                    "{n} of them have no usable default: a value has to be typed before the estate compiles.",
                ),
            },
            remedies: vec![Remedy::Go(View::Decisions)],
        });
    }

    if let Some(model) = f.model {
        let map = model
            .packs
            .iter()
            .find(|r| r.kind == PackRowKind::Map)
            .map(|r| r.state);
        match map {
            Some(LineState::Off) => out.push(Owed {
                id: "map-off",
                icon: "inventory_2",
                title: "The pack map is switched off".to_string(),
                detail: "`use \"presets/estate-map.satz\"` is commented out, so no pack asks this estate anything. Switching it on is what opens the questions the library declares.".to_string(),
                remedies: vec![Remedy::Go(View::Packs)],
            }),
            Some(LineState::Absent) => out.push(Owed {
                id: "map-absent",
                icon: "inventory_2",
                title: "The estate has no pack map".to_string(),
                detail: "The file carries no line for `presets/estate-map.satz`, the map of pack choices every other question hangs from. `satz merge-presets` writes the lines the library has and this estate does not.".to_string(),
                remedies: vec![Remedy::Merge, Remedy::Go(View::Packs)],
            }),
            _ => {}
        }

        let absent = model
            .packs
            .iter()
            .filter(|r| r.kind == PackRowKind::Choice && r.state == LineState::Absent)
            .count();
        if absent > 0 {
            out.push(Owed {
                id: "packs-absent",
                icon: "playlist_add",
                title: match absent {
                    1 => "1 pack choice has no line in this estate".to_string(),
                    n => format!("{n} pack choices have no line in this estate"),
                },
                detail: "The map asks for them and the file has nothing to switch: the library gained a pack after this estate was written. `satz merge-presets` installs what is missing and writes the commented `use` line for each.".to_string(),
                remedies: vec![Remedy::Merge, Remedy::Go(View::Packs)],
            });
        }

        if let SchemaStatus::Missing(dir) = &model.schema {
            out.push(Owed {
                id: "schema",
                icon: "schema",
                title: "There is no provider schema".to_string(),
                detail: format!(
                    "{} holds none, so every attribute is untyped and locked: the app cannot tell a required argument from a typo. `satz update-schema` fetches the provider's own.",
                    dir.display()
                ),
                remedies: vec![Remedy::Run {
                    label: "Run update-schema",
                    icon: "download",
                    args: vec!["update-schema".to_string()],
                }],
            });
        }

        let untrusted = model.hcl.iter().filter(|b| !b.trusted).count();
        if untrusted > 0 {
            out.push(Owed {
                id: "hcl-trust",
                icon: "code_off",
                title: match untrusted {
                    1 => "1 raw HCL block has not been reviewed".to_string(),
                    n => format!("{n} raw HCL blocks have not been reviewed"),
                },
                detail: "An `hcl { … }` block is emitted verbatim and is opaque to the compliance plane: no claim can cover it, and the compile warns on every transpile. Reading it and writing `hcl trust \"<reason>\" { … }` around it settles the debt — the warning becomes a note carrying the reason. Counted here are this estate file's own blocks; the drawer carries the compile's finding for every one in the estate, packs included.".to_string(),
                remedies: vec![Remedy::Go(View::Estate)],
            });
        }
    }

    if f.diagnostics
        .iter()
        .any(|d| d.kind.as_deref() == Some("prerequisites"))
    {
        out.push(Owed {
            id: "prerequisites",
            icon: "admin_panel_settings",
            title: "The prerequisites are not declared".to_string(),
            detail: "The compile found roles this estate's own resource types need and its IaC service account is not granted, or APIs its infrastructure project does not enable. The drawer carries each of them; `satz update-prerequisites` writes them into the file.".to_string(),
            remedies: vec![Remedy::Go(View::Checks)],
        });
    }

    // What proves a plan: a plan needs an initialised directory, so a directory that
    // was never initialised has never been planned. The other way round proves
    // nothing — a plan leaves no trace — so the row goes once the init is there.
    if !f.hcl.transpiled {
        out.push(Owed {
            id: "not-transpiled",
            icon: "build",
            title: "The estate has not been compiled here".to_string(),
            detail: "There is no `main.tf` in the HCL directory: nothing has been emitted from this checkout, so nothing has been planned against it either. `satz transpile` writes it, `hcl-init` prepares the directory and `plan` is what reads it back.".to_string(),
            remedies: vec![Remedy::Go(View::Deploy)],
        });
    } else if !f.hcl.initialised {
        out.push(Owed {
            id: "not-planned",
            icon: "preview",
            title: "Not verified by a plan".to_string(),
            detail: "The HCL is written and the directory is not initialised, and a plan needs an initialised directory — so nothing here has been through one. Whatever the estate says about the live organisation is still a claim. `hcl-init`, then `plan`.".to_string(),
            remedies: vec![Remedy::Go(View::Deploy)],
        });
    }

    out
}

/// The pack that declares the estate's own questions: `satz init` writes its `use` line
/// into every skeleton, and its answers are what one estate IS rather than what it does.
const CORE_PACK: &str = "estate_core";

/// The estate's own answers, in reading order — the customer first, because the reason to
/// look at this card is "is this the right one". `deployment_mode` is not here: the card
/// states it below in its own words, and one fact in two places is one fact to keep in
/// step. A core subject this list does not name still shows, under a label made from its
/// own name, after the named ones: a question satz adds is a row, never a silence.
const CORE_ORDER: [(&str, &str); 15] = [
    ("customer_longname", "Customer"),
    ("customer_shortname", "Short name"),
    ("customer_id", "Customer ID"),
    ("customer_organization_id", "Organisation ID"),
    ("customer_domain", "Domain"),
    ("first_admin", "First admin"),
    ("billing_account_infra", "Billing account"),
    ("infra_folder_name", "Infrastructure folder"),
    ("infra_project_name", "Infrastructure project"),
    ("infra_bucket_name", "State bucket"),
    ("svc_iac_account", "IaC service account"),
    ("svc_iac_users_group", "IaC users group"),
    ("deployment_engine", "Engine"),
    ("default_region", "Region"),
    ("default_zone", "Zone"),
];

/// One answer the estate gave to a question of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    pub label: String,
    /// what the estate carries; `None` where the question is unanswered, which the card
    /// says rather than filling in the pack's default — a default is what an interview
    /// OFFERS, not what this estate states
    pub value: Option<String>,
}

/// The estate's own answers, read from the questions report. Empty until the first report
/// has arrived, and empty for an estate whose packs do not include the core one.
pub fn identity(questions: Option<&QuestionsReport>) -> Vec<Fact> {
    let Some(report) = questions else {
        return Vec::new();
    };
    let core: Vec<&QuestionRow> = report
        .questions
        .iter()
        .filter(|q| q.pack == CORE_PACK && q.subject != "deployment_mode")
        .collect();
    let mut out: Vec<Fact> = CORE_ORDER
        .iter()
        .filter_map(|(subject, label)| {
            let q = core.iter().find(|q| q.subject == *subject)?;
            Some(Fact {
                label: (*label).to_string(),
                value: answer(q),
            })
        })
        .collect();
    out.extend(
        core.iter()
            .filter(|q| !CORE_ORDER.iter().any(|(s, _)| *s == q.subject))
            .map(|q| Fact {
                label: label_for(&q.subject),
                value: answer(q),
            }),
    );
    out
}

/// The answer as one line. A choice is answered by an option's name, which the report
/// carries as the current value like any other.
fn answer(q: &QuestionRow) -> Option<String> {
    match q.state {
        QuestionState::Answered => Some(q.current.as_ref().map_or_else(
            || "answered".to_string(),
            |v| {
                match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Bool(b) => if *b { "yes" } else { "no" }.to_string(),
                    serde_json::Value::Array(items) => items
                        .iter()
                        .map(|i| match i {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                    other => other.to_string(),
                }
            },
        )),
        QuestionState::Unanswered | QuestionState::NotApplicable => None,
    }
}

/// A label for a core subject the list above does not name: `svc_iac_users_group` reads
/// "Svc iac users group", which is worse than a written label and better than nothing.
fn label_for(subject: &str) -> String {
    let spaced = subject.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => spaced,
    }
}

/// Whom the estate's live calls run as, as `satz whoami` tells the cases apart — from
/// what `satz_open` reported, which is enough to tell the ones an open estate can be in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunsAs<'a> {
    /// cloud mode: the IaC service account the estate declares, impersonated by the ADC
    /// identity
    Impersonated(&'a str),
    /// the ADC identity itself, and why nothing is impersonated
    Credentials(String),
}

/// `runs_as` is `satz_open`'s answer, null whenever the calls impersonate nothing; the
/// mode is the estate's `deployment_mode`, `local` when it binds none — as the emitter,
/// `satz whoami` and `satz migrate` read it. In cloud mode a null `runs_as` means the
/// estate declares no account to impersonate.
pub fn runs_as_row<'a>(runs_as: Option<&'a str>, deployment_mode: Option<&str>) -> RunsAs<'a> {
    match (runs_as, deployment_mode.unwrap_or("local")) {
        (Some(account), _) => RunsAs::Impersonated(account),
        (None, "cloud") => RunsAs::Credentials(
            "the estate declares no IaC service account (svc_iac_account, infra_project_name), \
             so nothing is impersonated"
                .to_string(),
        ),
        (None, mode) => RunsAs::Credentials(format!(
            "{mode} mode; `satz migrate --mode cloud` makes every run impersonate the IaC \
             service account the estate declares"
        )),
    }
}

#[component]
pub fn OverviewView() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let open = app.open().cloned();
    let Some(open) = open else {
        return rsx! {};
    };
    let model = app.estate().model().cloned();
    let questions = app.estate().questions().cloned();
    let diagnostics = app.estate().diagnostics().cloned();
    let hcl = app.estate().hcl().cloned();
    let work_tree = app.estate().work_tree().cloned();
    let last_command = app.estate().last_command().cloned();
    let owed = owed(&Facts {
        estate: &open.name,
        deployment_mode: open.deployment_mode.as_deref(),
        hcl,
        work_tree: work_tree.as_ref(),
        questions: questions.as_ref(),
        model: model.as_deref(),
        diagnostics: &diagnostics,
    });

    let facts = identity(questions.as_ref());

    rsx! {
        div { class: "view overview",
            h1 { class: "view__title", "Overview" }
            IdentityCard { facts }
            if owed.is_empty() {
                Card { variant: CardVariant::Filled, class: "overview__clear",
                    Icon { name: "task_alt", size: 32, filled: true, class: "overview__clear-icon" }
                    div {
                        h2 { class: "overview__clear-title", "Nothing left to do" }
                        p { "The estate is in a git repository, every question is answered, every pack the map asks for has a line, the schema is there and the HCL directory has been through an init. What is left is the work you came for." }
                    }
                }
            } else {
                Card { variant: CardVariant::Outlined, class: "overview__owed",
                    header { class: "overview__owed-head",
                        Icon { name: "assignment_late", size: 22 }
                        h2 { class: "overview__owed-title", "Still to do" }
                        span { class: "grow" }
                        Chip { kind: ChipKind::Assist, label: match owed.len() { 1 => "1 item".to_string(), n => format!("{n} items") } }
                    }
                    for row in owed {
                        div { key: "{row.id}", class: "overview__row",
                            Icon { name: row.icon, size: 22, class: "overview__row-icon" }
                            div { class: "overview__row-text",
                                h3 { class: "overview__row-title", "{row.title}" }
                                for line in row.detail.lines() {
                                    p { key: "{line}", class: "overview__row-detail", "{line}" }
                                }
                            }
                            div { class: "overview__row-actions",
                                for (i, remedy) in row.remedies.into_iter().enumerate() {
                                    {
                                        match remedy {
                                            Remedy::Go(view) => rsx! {
                                                Button {
                                                    key: "{i}",
                                                    variant: if i == 0 { ButtonVariant::Filled } else { ButtonVariant::Tonal },
                                                    icon: view.icon(),
                                                    onclick: move |_| app.nav().set(view),
                                                    "{view.label()}"
                                                }
                                            },
                                            Remedy::Run { label, icon, args } => rsx! {
                                                Button {
                                                    key: "{i}",
                                                    variant: if i == 0 { ButtonVariant::Filled } else { ButtonVariant::Tonal },
                                                    icon,
                                                    onclick: move |_| handle.send(EstateAction::RunCommand(args.clone())),
                                                    "{label}"
                                                }
                                            },
                                            Remedy::Terminal { label, icon, args } => rsx! {
                                                Button {
                                                    key: "{i}",
                                                    variant: ButtonVariant::Tonal,
                                                    icon,
                                                    onclick: move |_| handle.send(EstateAction::OpenInTerminal(args.clone())),
                                                    "{label}"
                                                }
                                            },
                                            Remedy::InitRepository => rsx! {
                                                Button {
                                                    key: "{i}",
                                                    variant: ButtonVariant::Filled,
                                                    icon: "commit",
                                                    onclick: move |_| handle.send(EstateAction::InitRepository),
                                                    "Create the repository"
                                                }
                                            },
                                            Remedy::Merge => rsx! {
                                                Button {
                                                    key: "{i}",
                                                    variant: ButtonVariant::Filled,
                                                    icon: "merge",
                                                    onclick: move |_| handle.send(EstateAction::MergePresets),
                                                    "Run merge-presets"
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if last_command.is_some() {
                CommandLog {}
            }
        }
    }
}

/// The card that says which estate this is: its own answers first, then where it lives
/// and what it is built against.
#[component]
fn IdentityCard(facts: Vec<Fact>) -> Element {
    let app = use_context::<Store<AppStore>>();
    let open = app.open().cloned();
    let model = app.estate().model().cloned();
    let hcl = app.estate().hcl().cloned();
    let Some(open) = open else {
        return rsx! {};
    };
    let unanswered = facts.iter().filter(|f| f.value.is_none()).count();

    rsx! {
        Card { variant: CardVariant::Outlined, class: "overview__identity",
            header { class: "overview__owed-head",
                Icon { name: "description", size: 22 }
                h2 { class: "overview__owed-title", "{open.name}" }
                span { class: "grow" }
                if unanswered > 0 {
                    Chip {
                        kind: ChipKind::Assist,
                        label: match unanswered {
                            1 => "1 unanswered".to_string(),
                            n => format!("{n} unanswered"),
                        },
                    }
                }
            }
            dl { class: "overview__facts",
                for fact in facts {
                    dt { key: "{fact.label}", "{fact.label}" }
                    dd {
                        match fact.value {
                            Some(value) => rsx! { "{value}" },
                            None => rsx! { span { class: "overview__unanswered", "not answered" } },
                        }
                    }
                }
                dt { "Runs as" }
                    dd {
                        match runs_as_row(open.runs_as.as_deref(), open.deployment_mode.as_deref()) {
                            RunsAs::Impersonated(account) => rsx! { code { "{account}" } " — impersonated by the ADC identity" },
                            RunsAs::Credentials(why) => rsx! { "the ADC identity — {why}" },
                        }
                    }
                    dt { "File" }
                    dd { code { "{open.main.display()}" } }
                    dt { "Directory" }
                    dd { code { "{open.dir.display()}" } }
                    dt { "State" }
                    dd {
                        match open.deployment_mode.as_deref() {
                            Some("cloud") => "cloud — the state lives in the bucket bootstrap creates, and every apply runs as the IaC service account",
                            Some("local") => "local — the state is a file beside the generated HCL",
                            Some(other) => other,
                            None => "not set — the estate binds no deployment_mode, and the emitter takes local",
                        }
                    }
                    dt { "Schema" }
                    dd {
                        match model.as_deref().map(|m| &m.schema) {
                            Some(SchemaStatus::Loaded { providers, resources }) => format!("{} — {resources} resource types", providers.join(", ")),
                            Some(SchemaStatus::Missing(dir)) => format!("none in {}", dir.display()),
                            None => "not read".to_string(),
                        }
                    }
                    dt { "HCL" }
                    dd {
                        {
                            let state = match (hcl.transpiled, hcl.initialised) {
                                (false, _) => "nothing emitted yet",
                                (true, false) => "emitted, not initialised",
                                (true, true) => "emitted and initialised",
                            };
                            rsx! { "{state}" }
                        }
                    }
                }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use satz_studio_core::diag::DiagSource;
    use satz_studio_core::model::{Choice, PackRow};
    use satz_studio_core::satz::reports::QuestionsSummary;
    use serde_json::json;
    use std::path::PathBuf;

    /// One report row, as satz writes it: `current` is the estate's own answer and is
    /// absent while the question is unanswered.
    fn row(subject: &str, pack: &str, current: Option<serde_json::Value>) -> QuestionRow {
        let mut value = json!({
            "subject": subject, "kind": "param", "prompt": "p", "reversal": "edit",
            "blast": "low", "state": if current.is_some() { "answered" } else { "unanswered" },
            "blocking": false, "pack_description": "d", "from": "presets/estate-core.satz",
            "pack": pack
        });
        if let Some(current) = current {
            value["current"] = current;
        }
        serde_json::from_value(value).unwrap()
    }

    fn report(questions: Vec<QuestionRow>) -> QuestionsReport {
        QuestionsReport {
            estate: "C0example.satz".to_string(),
            questions,
            summary: QuestionsSummary::default(),
        }
    }

    fn labels(facts: &[Fact]) -> Vec<&str> {
        facts.iter().map(|f| f.label.as_str()).collect()
    }

    /// The customer comes first, because the reason to read this card is "is this the
    /// right estate", and every row carries the estate's own answer.
    #[test]
    fn the_estate_own_answers_read_customer_first() {
        let r = report(vec![
            row("default_region", CORE_PACK, Some(json!("europe-west3"))),
            row("customer_id", CORE_PACK, Some(json!("C0example"))),
            row("customer_longname", CORE_PACK, Some(json!("Acme Corp."))),
            row("customer_shortname", CORE_PACK, Some(json!("acme"))),
        ]);
        let facts = identity(Some(&r));
        assert_eq!(
            labels(&facts),
            ["Customer", "Short name", "Customer ID", "Region"]
        );
        assert_eq!(facts[0].value.as_deref(), Some("Acme Corp."));
        assert_eq!(facts[3].value.as_deref(), Some("europe-west3"));
    }

    /// An unanswered question is SAID to be unanswered. The pack's default is what an
    /// interview would offer, and printing it here would state something the estate does
    /// not say.
    #[test]
    fn an_unanswered_question_carries_no_value() {
        let r = report(vec![row("customer_domain", CORE_PACK, None)]);
        let facts = identity(Some(&r));
        assert_eq!(labels(&facts), ["Domain"]);
        assert_eq!(facts[0].value, None);
    }

    /// The card is the estate's own identity: a pack's question belongs to Decisions, and
    /// `deployment_mode` is the one core answer the card states below in its own words.
    #[test]
    fn pack_questions_and_deployment_mode_are_not_rows() {
        let r = report(vec![
            row("use_budget", "organization-budget", Some(json!(true))),
            row("deployment_mode", CORE_PACK, Some(json!("cloud"))),
            row("customer_domain", CORE_PACK, Some(json!("example.com"))),
        ]);
        assert_eq!(labels(&identity(Some(&r))), ["Domain"]);
    }

    /// A core question satz adds after this file was written is a row under its own name,
    /// after the ones with a written label — never a silence.
    #[test]
    fn a_core_question_without_a_written_label_still_shows() {
        let r = report(vec![
            row("default_zone", CORE_PACK, Some(json!("europe-west3-a"))),
            row(
                "customer_second_domain",
                CORE_PACK,
                Some(json!("example.net")),
            ),
        ]);
        let facts = identity(Some(&r));
        assert_eq!(labels(&facts), ["Zone", "Customer second domain"]);
    }

    /// Before the first report there is no card content, and an estate whose packs do not
    /// include the core one has none either.
    #[test]
    fn no_report_and_no_core_pack_are_both_empty() {
        assert!(identity(None).is_empty());
        let r = report(vec![row("use_budget", "organization-budget", None)]);
        assert!(identity(Some(&r)).is_empty());
    }

    /// A list answer reads as a list, and a boolean as a word.
    #[test]
    fn a_value_reads_as_one_line() {
        let r = report(vec![
            row(
                "first_admin",
                CORE_PACK,
                Some(json!(["a@example.com", "b@example.com"])),
            ),
            row("customer_domain", CORE_PACK, Some(json!(true))),
        ]);
        let facts = identity(Some(&r));
        assert_eq!(labels(&facts), ["Domain", "First admin"]);
        assert_eq!(facts[0].value.as_deref(), Some("yes"));
        assert_eq!(
            facts[1].value.as_deref(),
            Some("a@example.com, b@example.com")
        );
    }

    fn model(packs: Vec<PackRow>, schema: SchemaStatus) -> EstateModel {
        EstateModel {
            main: PathBuf::from("/estates/acme/C0example.satz"),
            outline: Vec::new(),
            params: Vec::new(),
            packs,
            pack_edges: Vec::new(),
            uses: Vec::new(),
            hcl: Vec::new(),
            diagnostics: Vec::new(),
            schema,
        }
    }

    fn loaded() -> SchemaStatus {
        SchemaStatus::Loaded {
            providers: vec!["google".to_string()],
            resources: 45,
        }
    }

    fn pack(kind: PackRowKind, state: LineState) -> PackRow {
        PackRow {
            kind,
            gate: Some("use_budget".to_string()),
            path: Some("presets/organization-budget.satz".to_string()),
            state,
            choice: Choice::Line,
            question: None,
            phase: None,
            line: Some(4),
        }
    }

    fn questions(unanswered: usize, blocking: usize) -> QuestionsReport {
        QuestionsReport {
            estate: "C0example.satz".to_string(),
            questions: Vec::new(),
            summary: QuestionsSummary {
                total: 16,
                answered: 16 - unanswered,
                unanswered,
                not_applicable: 0,
                blocking,
                one_way_doors: 0,
                complete: unanswered == 0,
            },
        }
    }

    fn facts<'a>(
        mode: Option<&'a str>,
        hcl: HclState,
        q: Option<&'a QuestionsReport>,
        m: Option<&'a EstateModel>,
        d: &'a [Diagnostic],
    ) -> Facts<'a> {
        Facts {
            estate: "C0example.satz",
            deployment_mode: mode,
            hcl,
            work_tree: Some(&WorkTree::Inside),
            questions: q,
            model: m,
            diagnostics: d,
        }
    }

    fn ids(rows: &[Owed]) -> Vec<&'static str> {
        rows.iter().map(|r| r.id).collect()
    }

    #[test]
    fn an_estate_that_owes_nothing_has_no_rows() {
        let m = model(vec![pack(PackRowKind::Map, LineState::On)], loaded());
        let q = questions(0, 0);
        let rows = owed(&facts(
            Some("cloud"),
            HclState {
                transpiled: true,
                initialised: true,
            },
            Some(&q),
            Some(&m),
            &[],
        ));
        assert!(rows.is_empty(), "{:?}", ids(&rows));
    }

    #[test]
    fn day_zero_is_owed_by_a_cloud_estate_whose_hcl_was_never_initialised_and_by_no_local_one() {
        let m = model(vec![pack(PackRowKind::Map, LineState::On)], loaded());
        let q = questions(0, 0);
        let fresh = HclState::default();
        let rows = owed(&facts(Some("cloud"), fresh, Some(&q), Some(&m), &[]));
        assert_eq!(ids(&rows), ["bootstrap", "not-transpiled"]);
        let day_zero = &rows[0];
        assert_eq!(day_zero.remedies.len(), 2, "a check and a hand-off");
        assert!(matches!(day_zero.remedies[0], Remedy::Run { .. }));
        assert!(matches!(day_zero.remedies[1], Remedy::Terminal { .. }));
        // a local estate has no day 0: there is no bucket to create
        let rows = owed(&facts(Some("local"), fresh, Some(&q), Some(&m), &[]));
        assert_eq!(ids(&rows), ["not-transpiled"]);
    }

    #[test]
    fn a_directory_that_was_never_initialised_is_the_only_proof_that_no_plan_has_run() {
        let m = model(vec![pack(PackRowKind::Map, LineState::On)], loaded());
        let q = questions(0, 0);
        let emitted = HclState {
            transpiled: true,
            initialised: false,
        };
        assert_eq!(
            ids(&owed(&facts(
                Some("local"),
                emitted,
                Some(&q),
                Some(&m),
                &[]
            ))),
            ["not-planned"]
        );
        let initialised = HclState {
            transpiled: true,
            initialised: true,
        };
        assert!(owed(&facts(Some("local"), initialised, Some(&q), Some(&m), &[])).is_empty());
    }

    #[test]
    fn the_questions_row_counts_the_unanswered_and_names_the_blocking() {
        let m = model(vec![pack(PackRowKind::Map, LineState::On)], loaded());
        let q = questions(4, 2);
        let rows = owed(&facts(
            Some("local"),
            HclState {
                transpiled: true,
                initialised: true,
            },
            Some(&q),
            Some(&m),
            &[],
        ));
        assert_eq!(ids(&rows), ["questions"]);
        assert_eq!(rows[0].title, "4 questions are unanswered");
        assert!(rows[0].detail.starts_with("2 of them"));
        assert_eq!(rows[0].remedies, [Remedy::Go(View::Decisions)]);
    }

    #[test]
    fn a_map_that_is_off_and_a_pack_without_a_line_are_two_different_rows() {
        let q = questions(0, 0);
        let hcl = HclState {
            transpiled: true,
            initialised: true,
        };
        let m = model(vec![pack(PackRowKind::Map, LineState::Off)], loaded());
        assert_eq!(
            ids(&owed(&facts(None, hcl, Some(&q), Some(&m), &[]))),
            ["map-off"]
        );
        let m = model(
            vec![
                pack(PackRowKind::Map, LineState::On),
                pack(PackRowKind::Choice, LineState::Absent),
                pack(PackRowKind::Choice, LineState::Absent),
            ],
            loaded(),
        );
        let rows = owed(&facts(None, hcl, Some(&q), Some(&m), &[]));
        assert_eq!(ids(&rows), ["packs-absent"]);
        assert_eq!(rows[0].title, "2 pack choices have no line in this estate");
        assert_eq!(rows[0].remedies[0], Remedy::Merge);
    }

    #[test]
    fn a_missing_schema_names_the_directory_and_offers_the_command_that_fills_it() {
        let q = questions(0, 0);
        let m = model(
            vec![pack(PackRowKind::Map, LineState::On)],
            SchemaStatus::Missing(PathBuf::from("/estates/acme/schema")),
        );
        let rows = owed(&facts(
            None,
            HclState {
                transpiled: true,
                initialised: true,
            },
            Some(&q),
            Some(&m),
            &[],
        ));
        assert_eq!(ids(&rows), ["schema"]);
        assert!(rows[0].detail.contains("/estates/acme/schema"));
        assert!(matches!(
            &rows[0].remedies[0],
            Remedy::Run { args, .. } if args == &["update-schema".to_string()]
        ));
    }

    #[test]
    fn raw_hcl_is_owed_until_it_carries_a_reason() {
        use satz_studio_core::model::HclBlock;
        let q = questions(0, 0);
        let hcl = HclState {
            transpiled: true,
            initialised: true,
        };
        let mut m = model(vec![pack(PackRowKind::Map, LineState::On)], loaded());
        m.hcl = vec![
            HclBlock {
                line: 12,
                trusted: false,
            },
            HclBlock {
                line: 40,
                trusted: true,
            },
        ];
        let rows = owed(&facts(None, hcl, Some(&q), Some(&m), &[]));
        assert_eq!(ids(&rows), ["hcl-trust"]);
        assert_eq!(rows[0].title, "1 raw HCL block has not been reviewed");
        m.hcl = vec![HclBlock {
            line: 12,
            trusted: true,
        }];
        assert!(owed(&facts(None, hcl, Some(&q), Some(&m), &[])).is_empty());
    }

    #[test]
    fn the_prerequisites_row_comes_from_the_compiles_own_finding() {
        let q = questions(0, 0);
        let m = model(vec![pack(PackRowKind::Map, LineState::On)], loaded());
        let hcl = HclState {
            transpiled: true,
            initialised: true,
        };
        let mut d = Diagnostic::error("the IaC service account lacks roles", DiagSource::Check);
        d.kind = Some("prerequisites".to_string());
        let rows = owed(&facts(
            None,
            hcl,
            Some(&q),
            Some(&m),
            std::slice::from_ref(&d),
        ));
        assert_eq!(ids(&rows), ["prerequisites"]);
        assert_eq!(rows[0].remedies, [Remedy::Go(View::Checks)]);
        d.kind = Some("missing-required".to_string());
        assert!(
            owed(&facts(
                None,
                hcl,
                Some(&q),
                Some(&m),
                std::slice::from_ref(&d)
            ))
            .is_empty()
        );
    }

    #[test]
    fn an_estate_outside_a_repository_owes_one_with_the_fix_offered_and_git_s_own_words() {
        let q = questions(0, 0);
        let m = model(vec![pack(PackRowKind::Map, LineState::On)], loaded());
        let hcl = HclState {
            transpiled: true,
            initialised: true,
        };
        let outside = WorkTree::Outside(
            "fatal: not a git repository (or any of the parent directories): .git".to_string(),
        );
        let mut f = facts(None, hcl, Some(&q), Some(&m), &[]);
        f.work_tree = Some(&outside);
        let rows = owed(&f);
        assert_eq!(ids(&rows), ["repository"]);
        assert_eq!(rows[0].remedies, [Remedy::InitRepository]);
        assert!(rows[0].detail.contains("fatal: not a git repository"));
        assert!(rows[0].detail.contains("merge-presets"));
    }

    /// Without git the directory is neither in a repository nor out of one: the row says
    /// git is missing and offers no button, because the button would run git.
    #[test]
    fn without_git_the_row_names_git_and_offers_nothing_to_press() {
        let q = questions(0, 0);
        let m = model(vec![pack(PackRowKind::Map, LineState::On)], loaded());
        let hcl = HclState {
            transpiled: true,
            initialised: true,
        };
        let missing = WorkTree::NoGit("running git: No such file or directory".to_string());
        let mut f = facts(None, hcl, Some(&q), Some(&m), &[]);
        f.work_tree = Some(&missing);
        let rows = owed(&f);
        assert_eq!(ids(&rows), ["repository"]);
        assert_eq!(rows[0].title, "git is not available");
        assert!(rows[0].remedies.is_empty());
        assert!(rows[0].detail.contains("No such file or directory"));
    }

    /// Inside a work tree, its own or one above it, and before the first reload has
    /// asked, nothing is owed for it.
    #[test]
    fn a_repository_or_a_fact_not_yet_read_owes_nothing() {
        let q = questions(0, 0);
        let m = model(vec![pack(PackRowKind::Map, LineState::On)], loaded());
        let hcl = HclState {
            transpiled: true,
            initialised: true,
        };
        let mut f = facts(None, hcl, Some(&q), Some(&m), &[]);
        assert!(owed(&f).is_empty());
        f.work_tree = None;
        assert!(owed(&f).is_empty());
    }

    const ACCOUNT: &str = "svc-iac-001@acme-infra-001.iam.gserviceaccount.com";

    /// Cloud mode with a declared account: that account, and who becomes it.
    #[test]
    fn an_account_satz_open_names_is_impersonated() {
        assert_eq!(
            runs_as_row(Some(ACCOUNT), Some("cloud")),
            RunsAs::Impersonated(ACCOUNT)
        );
    }

    /// Local mode runs as the credentials whatever the estate declares, and the row
    /// names the switch rather than claiming the estate declares no account — the card
    /// above it lists the account it does declare.
    #[test]
    fn local_mode_runs_as_the_credentials_and_names_the_switch() {
        for mode in [Some("local"), None] {
            let RunsAs::Credentials(why) = runs_as_row(None, mode) else {
                panic!("local mode impersonates nothing");
            };
            assert!(why.starts_with("local mode;"), "{why}");
            assert!(why.contains("`satz migrate --mode cloud`"), "{why}");
            assert!(!why.contains("declares no"), "{why}");
        }
    }

    /// Cloud mode with nothing to impersonate is the estate declaring no account.
    #[test]
    fn cloud_mode_without_an_account_says_the_estate_declares_none() {
        let RunsAs::Credentials(why) = runs_as_row(None, Some("cloud")) else {
            panic!("nothing is impersonated");
        };
        assert!(why.contains("declares no IaC service account"), "{why}");
        assert!(!why.contains("migrate"), "{why}");
    }
}
