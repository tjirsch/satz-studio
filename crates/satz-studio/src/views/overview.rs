//! The Overview: which estate this is, and what it still has to do.
//!
//! The first card answers "am I in the right estate": first whose estate it is — the
//! short name, the customer, the customer id and the organisation id, read from the
//! file's own `params { }` block whether or not it uses the `estate_core` pack — then the
//! answers the estate gave to the rest of its OWN questions, the ones that pack declares
//! (the infrastructure it names, the identity it runs as), read from the questions
//! report. Nothing is stored. [`identity`] is that derivation, pure over the params and
//! the report; a param the file does not set says so, and a question the report does
//! not carry is simply not a row.
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
use satz_studio_core::model::{EstateModel, ParamRow, SchemaStatus, SourceValue, StrPart};
use satz_studio_core::satz::reports::{PackLine, QuestionRow, QuestionState, QuestionsReport};

use crate::components::{Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, Icon};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, View};
use crate::views::commands::CommandLog;
use crate::views::export::{ExportCard, Moment};

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
    /// how many of the open notices the window is holding — the ones a write of this
    /// session opened, which its dialog can raise again
    pub held_notices: usize,
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
    /// `git init -b main`, `git add -A` and one commit in the estate directory, run when
    /// the operator presses it and never on the app's own account
    InitRepository,
    /// raise the notice dialog again, for the notices this session is holding
    ShowNotices,
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
        match model.packs.map().map(|m| m.line) {
            Some(PackLine::Commented) => out.push(Owed {
                id: "map-off",
                icon: "inventory_2",
                title: "The pack map is switched off".to_string(),
                detail: "`use \"presets/estate-map.satz\"` is commented out, so no pack asks this estate anything. Switching it on is what opens the questions the library declares.".to_string(),
                remedies: vec![Remedy::Go(View::Packs)],
            }),
            Some(PackLine::Absent) => out.push(Owed {
                id: "map-absent",
                icon: "inventory_2",
                title: "The estate has no pack map".to_string(),
                detail: "The file carries no line for `presets/estate-map.satz`, the map of pack choices every other question hangs from. Switching it on in Packs writes the line where the pack graph places it.".to_string(),
                remedies: vec![Remedy::Go(View::Packs)],
            }),
            _ => {}
        }

        let findings = &model.packs.findings;
        if let Some(first) = findings.first() {
            out.push(Owed {
                id: "pack-findings",
                icon: "account_tree",
                title: match findings.len() {
                    1 => "The pack graph has 1 finding".to_string(),
                    n => format!("The pack graph has {n} findings"),
                },
                detail: first.message.clone(),
                remedies: vec![Remedy::Go(View::Packs)],
            });
        }

        // A `use` line without its `when <gate>` deploys its pack whatever the estate
        // answers. satz reports that as `ungated-pack` only while the file declaring the
        // gate deploys, so with the map off the findings row above says nothing about it;
        // a row satz does speak about is left to satz's own sentence, in Packs.
        let ungated = model
            .packs
            .packs
            .iter()
            .filter(|p| p.line == PackLine::Ungated && p.findings.is_empty())
            .count();
        if ungated > 0 {
            out.push(Owed {
                id: "ungated-lines",
                icon: "link_off",
                title: match ungated {
                    1 => "1 pack line has no gate".to_string(),
                    n => format!("{n} pack lines have no gate"),
                },
                detail: "A `use` line without its `when <gate>` deploys the pack whatever this estate answers, so no question switches it off. Packs names the gate each of them needs.".to_string(),
                remedies: vec![Remedy::Go(View::Packs)],
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

    // What a pack asks to be run once it is switched on. The compile raises one finding
    // per open notice, so the count is satz's, not the app's; the window holds the ones
    // a write of this session opened and can raise their dialog again.
    let notices = f
        .diagnostics
        .iter()
        .filter(|d| d.kind.as_deref() == Some("notice"))
        .count();
    if notices > 0 {
        out.push(Owed {
            id: "notices",
            icon: "assignment_late",
            title: match notices {
                1 => "1 pack asks for a command to be run".to_string(),
                n => format!("{n} packs ask for a command to be run"),
            },
            detail: "A pack that goes into an estate can name the command to run once it is on — `satz adopt` for the CIS org-policy packs, so every policy that is already live is in the state before the apply. The estate acknowledges each by binding the notice's param, and apply and bootstrap refuse while one that holds them up is open. The drawer carries each notice with its command and its param."
                .to_string(),
            remedies: if f.held_notices > 0 {
                vec![Remedy::ShowNotices]
            } else {
                Vec::new()
            },
        });
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

/// Whose estate this is, read from the file's own `params { }` block rather than from a
/// pack's questions, so an estate that sets these without `use`-ing the core pack still
/// says who it stands for. The card opens with them, in this order, because the reason to
/// look at it is "is this the right one".
const FILE_ORDER: [(&str, &str); 4] = [
    ("customer_shortname", "Short name"),
    ("customer_longname", "Customer"),
    ("customer_id", "Customer ID"),
    ("customer_organization_id", "Organisation ID"),
];

/// The rest of the estate's own answers, in reading order. The subjects of [`FILE_ORDER`]
/// are never a question row, so each shows once. `deployment_mode` is not here either:
/// the card states it below in its own words, and one fact in two places is one fact to
/// keep in step. A core subject this list does not name still shows, under a label made
/// from its own name, after the named ones: a question satz adds is a row, never a
/// silence.
const CORE_ORDER: [(&str, &str); 11] = [
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

/// What one row of the card says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reading {
    /// what the estate carries, as one line
    Value(String),
    /// a core question the report has unanswered. The pack's default is what an
    /// interview OFFERS, not what this estate states, so it is never shown here
    NotAnswered,
    /// a param of [`FILE_ORDER`] the file's `params { }` block does not set; a pack's
    /// default is not shown for it either
    NotSet,
    /// a param of [`FILE_ORDER`] the file sets to `""`, as `satz init` writes the ones it
    /// could not derive
    Empty,
    /// a param of [`FILE_ORDER`] before the model is built: whether the file sets it is
    /// not known yet, so the row states neither a value nor "not set"
    NotRead,
}

/// One row of the card: a label, and what the estate says under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    pub label: String,
    pub reading: Reading,
}

/// The card's rows: the four of [`FILE_ORDER`] from the file's params (`params` is the
/// model's, `None` until the model is built), then the other core answers from the
/// questions report — none until the first report has arrived, and none for an estate
/// whose packs do not include the core one.
pub fn identity(params: Option<&[ParamRow]>, questions: Option<&QuestionsReport>) -> Vec<Fact> {
    let mut out: Vec<Fact> = FILE_ORDER
        .iter()
        .map(|(name, label)| Fact {
            label: (*label).to_string(),
            reading: match params {
                None => Reading::NotRead,
                Some(params) => match params.iter().find(|p| p.name == *name) {
                    None => Reading::NotSet,
                    Some(p) => match source_line(&p.value) {
                        line if line.is_empty() => Reading::Empty,
                        line => Reading::Value(line),
                    },
                },
            },
        })
        .collect();
    let Some(report) = questions else {
        return out;
    };
    let core: Vec<&QuestionRow> = report
        .questions
        .iter()
        .filter(|q| {
            q.pack == CORE_PACK
                && q.subject != "deployment_mode"
                && !FILE_ORDER.iter().any(|(s, _)| *s == q.subject)
        })
        .collect();
    out.extend(CORE_ORDER.iter().filter_map(|(subject, label)| {
        let q = core.iter().find(|q| q.subject == *subject)?;
        Some(Fact {
            label: (*label).to_string(),
            reading: answer(q),
        })
    }));
    out.extend(
        core.iter()
            .filter(|q| !CORE_ORDER.iter().any(|(s, _)| *s == q.subject))
            .map(|q| Fact {
                label: label_for(&q.subject),
                reading: answer(q),
            }),
    );
    out
}

/// A value as the file has it, as one line: a string as satz reads it, each `{param}` in
/// it replaced by what the param resolves to; a reference by its resolved value; a list
/// joined with commas.
fn source_line(value: &SourceValue) -> String {
    match value {
        SourceValue::Str { parts, .. } => parts
            .iter()
            .map(|part| match part {
                StrPart::Lit(text) => text.clone(),
                StrPart::TfRef(target) => format!("${{{target}}}"),
                StrPart::Param {
                    resolved: Some(v), ..
                } => json_line(v),
                StrPart::Param {
                    name,
                    resolved: None,
                } => format!("{{{name}}}"),
            })
            .collect(),
        SourceValue::Num(n) => n.clone(),
        SourceValue::Bool(b) => if *b { "yes" } else { "no" }.to_string(),
        SourceValue::Ref {
            resolved: Some(v), ..
        } => json_line(v),
        SourceValue::Ref {
            param,
            resolved: None,
        } => param.clone(),
        SourceValue::List(items) => items.iter().map(source_line).collect::<Vec<_>>().join(", "),
        SourceValue::Obj => "an object".to_string(),
    }
}

/// The answer as one line. A choice is answered by an option's name, which the report
/// carries as the current value like any other.
fn answer(q: &QuestionRow) -> Reading {
    match q.state {
        QuestionState::Answered => Reading::Value(
            q.current
                .as_ref()
                .map_or_else(|| "answered".to_string(), json_line),
        ),
        QuestionState::Unanswered | QuestionState::NotApplicable => Reading::NotAnswered,
    }
}

/// A JSON value as one line: a string as itself, a boolean as a word, a list joined with
/// commas.
fn json_line(v: &serde_json::Value) -> String {
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
        held_notices: app.estate().notices().read().len(),
    });

    let facts = identity(
        model.as_deref().map(|m| m.params.as_slice()),
        questions.as_ref(),
    );

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
                                            Remedy::ShowNotices => rsx! {
                                                Button {
                                                    key: "{i}",
                                                    variant: ButtonVariant::Filled,
                                                    icon: "assignment_late",
                                                    onclick: move |_| app.estate().notices_open().set(true),
                                                    "Show the notices"
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

            ExportCard { moment: Moment::Handover }

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
    let missing = facts
        .iter()
        .filter(|f| {
            matches!(
                f.reading,
                Reading::NotAnswered | Reading::NotSet | Reading::Empty
            )
        })
        .count();

    rsx! {
        Card { variant: CardVariant::Outlined, class: "overview__identity",
            header { class: "overview__owed-head",
                Icon { name: "description", size: 22 }
                h2 { class: "overview__owed-title", "{open.name}" }
                span { class: "grow" }
                if missing > 0 {
                    Chip {
                        kind: ChipKind::Assist,
                        label: match missing {
                            1 => "1 without a value".to_string(),
                            n => format!("{n} without a value"),
                        },
                    }
                }
            }
            dl { class: "overview__facts",
                for fact in facts {
                    dt { key: "{fact.label}", "{fact.label}" }
                    dd {
                        match fact.reading {
                            Reading::Value(value) => rsx! { "{value}" },
                            Reading::NotAnswered => rsx! { span { class: "overview__unanswered", "not answered" } },
                            Reading::NotSet => rsx! { span { class: "overview__unanswered", "not set" } },
                            Reading::Empty => rsx! { span { class: "overview__unanswered", "empty" } },
                            Reading::NotRead => rsx! { span { class: "overview__unanswered", "not read yet" } },
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
    use satz_studio_core::satz::reports::QuestionsSummary;
    use satz_studio_core::satz::reports::{
        Finding, FindingSeverity, PackRole, PackRow, PacksReport,
    };
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

    /// The rows that follow the four the file answers: the questions report's.
    fn answered_rows(facts: &[Fact]) -> &[Fact] {
        &facts[FILE_ORDER.len()..]
    }

    fn value(text: &str) -> Reading {
        Reading::Value(text.to_string())
    }

    /// One row of the file's `params { }` block holding a plain string.
    fn param(name: &str, text: &str) -> ParamRow {
        ParamRow {
            id: 0,
            name: name.to_string(),
            value: SourceValue::Str {
                raw: text.to_string(),
                parts: vec![StrPart::Lit(text.to_string())],
            },
            kind: satz_studio_core::model::ParamKind::String,
            question: None,
            one_way_door: false,
            mode: satz_studio_core::model::EditMode::Value,
            line: 1,
        }
    }

    /// The four params that say whose estate this is, as an estate that does not use the
    /// core pack sets them; the organisation id is a number.
    fn whose() -> Vec<ParamRow> {
        let mut org = param("customer_organization_id", "");
        org.value = SourceValue::Num("123456789012".to_string());
        vec![
            param("customer_id", "C0example"),
            param("customer_longname", "Acme Corp."),
            param("default_region", "europe-west3"),
            org,
            param("customer_shortname", "acme"),
        ]
    }

    /// The card opens with whose estate this is, read from the file itself: with no
    /// questions report at all, the four params the file sets are exactly the four rows,
    /// short name first, and another param of the file is not one of them.
    #[test]
    fn the_file_s_own_params_say_whose_estate_it_is_without_a_questions_report() {
        let facts = identity(Some(&whose()), None);
        assert_eq!(
            facts,
            [
                Fact {
                    label: "Short name".to_string(),
                    reading: value("acme")
                },
                Fact {
                    label: "Customer".to_string(),
                    reading: value("Acme Corp.")
                },
                Fact {
                    label: "Customer ID".to_string(),
                    reading: value("C0example")
                },
                Fact {
                    label: "Organisation ID".to_string(),
                    reading: value("123456789012")
                },
            ]
        );
    }

    /// An estate that uses the core pack carries the same four subjects as questions too;
    /// each is still one row, read from the file, and the other core answers follow.
    #[test]
    fn with_the_core_report_each_subject_still_shows_once() {
        let r = report(vec![
            row("default_region", CORE_PACK, Some(json!("europe-west3"))),
            row("customer_id", CORE_PACK, Some(json!("C0example"))),
            row("customer_longname", CORE_PACK, Some(json!("Acme Corp."))),
            row("customer_shortname", CORE_PACK, Some(json!("acme"))),
            row(
                "customer_organization_id",
                CORE_PACK,
                Some(json!("123456789012")),
            ),
        ]);
        let facts = identity(Some(&whose()), Some(&r));
        assert_eq!(
            labels(&facts),
            [
                "Short name",
                "Customer",
                "Customer ID",
                "Organisation ID",
                "Region"
            ]
        );
        assert_eq!(facts[0].reading, value("acme"));
        assert_eq!(facts[4].reading, value("europe-west3"));
    }

    /// A param the file does not set says so, even where the core pack's question offers a
    /// default, and one it sets to `""` says it is empty; before the model is built the
    /// file has not been read, and the row says that instead of claiming the param is not
    /// set.
    #[test]
    fn a_param_the_file_does_not_set_reads_not_set_and_an_unread_file_says_so() {
        let mut offered = row("customer_longname", CORE_PACK, None);
        offered.default = Some(json!("Acme Corp."));
        let r = report(vec![offered]);
        let facts = identity(
            Some(&[
                param("customer_shortname", "acme"),
                param("customer_id", ""),
            ]),
            Some(&r),
        );
        assert_eq!(
            facts.iter().map(|f| &f.reading).collect::<Vec<_>>(),
            [
                &value("acme"),
                &Reading::NotSet,
                &Reading::Empty,
                &Reading::NotSet
            ]
        );
        assert!(
            identity(None, Some(&r))
                .iter()
                .all(|f| f.reading == Reading::NotRead)
        );
    }

    /// A string reads as satz reads it — an interpolated param by its value, a Terraform
    /// reference as written — and a list joined with commas.
    #[test]
    fn a_file_value_reads_as_one_line() {
        let mut long = param("customer_longname", "");
        long.value = SourceValue::Str {
            raw: "{customer_shortname} at ${var.site}".to_string(),
            parts: vec![
                StrPart::Param {
                    name: "customer_shortname".to_string(),
                    resolved: Some(json!("acme")),
                },
                StrPart::Lit(" at ".to_string()),
                StrPart::TfRef("var.site".to_string()),
            ],
        };
        let mut id = param("customer_id", "");
        id.value = SourceValue::List(vec![
            SourceValue::Str {
                raw: "C0example".to_string(),
                parts: vec![StrPart::Lit("C0example".to_string())],
            },
            SourceValue::Bool(true),
        ]);
        let facts = identity(Some(&[long, id]), None);
        assert_eq!(facts[1].reading, value("acme at ${var.site}"));
        assert_eq!(facts[2].reading, value("C0example, yes"));
    }

    /// An unanswered question is SAID to be unanswered. The pack's default is what an
    /// interview would offer, and printing it here would state something the estate does
    /// not say.
    #[test]
    fn an_unanswered_question_carries_no_value() {
        let r = report(vec![row("customer_domain", CORE_PACK, None)]);
        let facts = identity(Some(&[]), Some(&r));
        assert_eq!(labels(answered_rows(&facts)), ["Domain"]);
        assert_eq!(answered_rows(&facts)[0].reading, Reading::NotAnswered);
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
        assert_eq!(
            labels(answered_rows(&identity(Some(&[]), Some(&r)))),
            ["Domain"]
        );
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
        let facts = identity(Some(&[]), Some(&r));
        assert_eq!(
            labels(answered_rows(&facts)),
            ["Zone", "Customer second domain"]
        );
    }

    /// Before the first report, and for an estate whose packs do not include the core one,
    /// the card carries the four rows the file answers and nothing else.
    #[test]
    fn no_report_and_no_core_pack_leave_only_the_file_s_rows() {
        assert!(answered_rows(&identity(Some(&[]), None)).is_empty());
        let r = report(vec![row("use_budget", "organization-budget", None)]);
        assert!(answered_rows(&identity(Some(&[]), Some(&r))).is_empty());
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
        let facts = identity(Some(&[]), Some(&r));
        let rows = answered_rows(&facts);
        assert_eq!(labels(rows), ["Domain", "First admin"]);
        assert_eq!(rows[0].reading, value("yes"));
        assert_eq!(rows[1].reading, value("a@example.com, b@example.com"));
    }

    fn model(packs: PacksReport, schema: SchemaStatus) -> EstateModel {
        EstateModel {
            main: PathBuf::from("/estates/acme/C0example.satz"),
            outline: Vec::new(),
            params: Vec::new(),
            packs,
            phases: Default::default(),
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

    /// A report whose map line stands as `map` and that carries `findings` pack findings.
    fn packs(map: PackLine, findings: usize) -> PacksReport {
        PacksReport {
            estate: "C0example.satz".to_string(),
            note: None,
            packs: vec![PackRow {
                path: "presets/estate-map.satz".to_string(),
                role: PackRole::Map,
                gate: None,
                gate_declared_in: None,
                answer: None,
                default: None,
                value: None,
                line: map,
                at_line: (map != PackLine::Absent).then_some(4),
                written: None,
                gated_on: None,
                deploys: map == PackLine::Active,
                requires: Vec::new(),
                required_by: Vec::new(),
                excludes: Vec::new(),
                by_hand: None,
                notices: Vec::new(),
                findings: Vec::new(),
            }],
            unmanaged: Vec::new(),
            findings: (0..findings)
                .map(|i| Finding {
                    severity: FindingSeverity::Warning,
                    kind: "unadopted-pack".to_string(),
                    group: None,
                    file: None,
                    line: None,
                    subject: Some("presets/organization-budget.satz".to_string()),
                    message: format!("finding {i}"),
                    fix: None,
                })
                .collect(),
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
            held_notices: 0,
        }
    }

    fn ids(rows: &[Owed]) -> Vec<&'static str> {
        rows.iter().map(|r| r.id).collect()
    }

    #[test]
    fn an_estate_that_owes_nothing_has_no_rows() {
        let m = model(packs(PackLine::Active, 0), loaded());
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
        let m = model(packs(PackLine::Active, 0), loaded());
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
        let m = model(packs(PackLine::Active, 0), loaded());
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
        let m = model(packs(PackLine::Active, 0), loaded());
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
    fn a_map_that_is_off_one_that_is_absent_and_the_pack_findings_are_three_rows() {
        let q = questions(0, 0);
        let hcl = HclState {
            transpiled: true,
            initialised: true,
        };
        let m = model(packs(PackLine::Commented, 0), loaded());
        assert_eq!(
            ids(&owed(&facts(None, hcl, Some(&q), Some(&m), &[]))),
            ["map-off"]
        );
        let m = model(packs(PackLine::Absent, 0), loaded());
        assert_eq!(
            ids(&owed(&facts(None, hcl, Some(&q), Some(&m), &[]))),
            ["map-absent"]
        );
        let m = model(packs(PackLine::Active, 2), loaded());
        let rows = owed(&facts(None, hcl, Some(&q), Some(&m), &[]));
        assert_eq!(ids(&rows), ["pack-findings"]);
        assert_eq!(rows[0].title, "The pack graph has 2 findings");
        assert_eq!(rows[0].detail, "finding 0");
        assert_eq!(rows[0].remedies, [Remedy::Go(View::Packs)]);
    }

    /// satz reports `ungated-pack` only while the file declaring the gate deploys, so the
    /// findings row is silent about a line no answer switches off in an estate with the
    /// map off. This row is not.
    #[test]
    fn a_line_without_its_gate_is_a_row_of_its_own_until_satz_says_it() {
        let q = questions(0, 0);
        let hcl = HclState {
            transpiled: true,
            initialised: true,
        };
        let mut report = packs(PackLine::Commented, 0);
        report.packs.push(PackRow {
            path: "presets/organization-budget.satz".to_string(),
            role: PackRole::Pack,
            gate: Some("use_budget".to_string()),
            gate_declared_in: Some("presets/estate-map.satz".to_string()),
            answer: None,
            default: None,
            value: None,
            line: PackLine::Ungated,
            at_line: Some(12),
            written: None,
            gated_on: None,
            deploys: true,
            requires: Vec::new(),
            required_by: Vec::new(),
            excludes: Vec::new(),
            by_hand: None,
            notices: Vec::new(),
            findings: Vec::new(),
        });
        let m = model(report.clone(), loaded());
        let rows = owed(&facts(None, hcl, Some(&q), Some(&m), &[]));
        assert_eq!(ids(&rows), ["map-off", "ungated-lines"]);
        assert_eq!(rows[1].title, "1 pack line has no gate");
        assert_eq!(rows[1].remedies, [Remedy::Go(View::Packs)]);

        // satz's own sentence on the row takes the line over
        report.packs[1].findings = vec!["is used without `when use_budget`".to_string()];
        let m = model(report, loaded());
        assert_eq!(
            ids(&owed(&facts(None, hcl, Some(&q), Some(&m), &[]))),
            ["map-off"]
        );
    }

    #[test]
    fn a_missing_schema_names_the_directory_and_offers_the_command_that_fills_it() {
        let q = questions(0, 0);
        let m = model(
            packs(PackLine::Active, 0),
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
        let mut m = model(packs(PackLine::Active, 0), loaded());
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

    /// The count is the compile's, so a notice acknowledged outside the window stops
    /// being a row at the next reload; the button appears only where this session is
    /// holding the notice and can raise its dialog again.
    #[test]
    fn the_notices_row_counts_the_compiles_own_findings_and_offers_the_dialog_it_holds() {
        let q = questions(0, 0);
        let m = model(packs(PackLine::Active, 0), loaded());
        let hcl = HclState {
            transpiled: true,
            initialised: true,
        };
        let notice = |message: &str| {
            let mut d = Diagnostic::error(message, DiagSource::Check);
            d.kind = Some("notice".to_string());
            d
        };
        let two = [
            notice("the baseline asks for adopt"),
            notice("so does the SSH pack"),
        ];
        let rows = owed(&facts(None, hcl, Some(&q), Some(&m), &two));
        assert_eq!(ids(&rows), ["notices"]);
        assert_eq!(rows[0].title, "2 packs ask for a command to be run");
        assert_eq!(rows[0].remedies, [], "this session opened neither of them");
        let mut held = facts(None, hcl, Some(&q), Some(&m), &two);
        held.held_notices = 1;
        assert_eq!(owed(&held)[0].remedies, [Remedy::ShowNotices]);
        let none: [Diagnostic; 0] = [];
        assert!(owed(&facts(None, hcl, Some(&q), Some(&m), &none)).is_empty());
    }

    #[test]
    fn the_prerequisites_row_comes_from_the_compiles_own_finding() {
        let q = questions(0, 0);
        let m = model(packs(PackLine::Active, 0), loaded());
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
        let m = model(packs(PackLine::Active, 0), loaded());
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
        let m = model(packs(PackLine::Active, 0), loaded());
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
        let m = model(packs(PackLine::Active, 0), loaded());
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
