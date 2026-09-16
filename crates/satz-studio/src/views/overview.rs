//! The Overview: what this estate still owes, and what it is.
//!
//! The card of owed items is DERIVED from the estate's own state on every render — the
//! questions report, the pack rows, the schema, the compile's findings, the generated
//! HCL directory — so it cannot drift from the estate, and it disappears when there is
//! nothing left in it. Nothing is saved: there is no workflow to resume, no step to be
//! trapped in, and no record of how the estate reached the app. An estate that was
//! created, one that was imported and one that was opened show the same list, because
//! the same facts are true of them.
//!
//! [`owed`] is the whole derivation, pure over [`Facts`], so what the card says is
//! testable without a window.

use dioxus::prelude::*;
use satz_studio_core::diag::Diagnostic;
use satz_studio_core::estate::HclState;
use satz_studio_core::model::{EstateModel, LineState, PackRowKind, SchemaStatus};
use satz_studio_core::satz::reports::QuestionsReport;

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
    let last_command = app.estate().last_command().cloned();
    let owed = owed(&Facts {
        estate: &open.name,
        deployment_mode: open.deployment_mode.as_deref(),
        hcl,
        questions: questions.as_ref(),
        model: model.as_deref(),
        diagnostics: &diagnostics,
    });

    rsx! {
        div { class: "view overview",
            h1 { class: "view__title", "Overview" }
            if owed.is_empty() {
                Card { variant: CardVariant::Filled, class: "overview__clear",
                    Icon { name: "task_alt", size: 32, filled: true, class: "overview__clear-icon" }
                    div {
                        h2 { class: "overview__clear-title", "Nothing is owed" }
                        p { "Every question is answered, every pack the map asks for has a line, the schema is there and the HCL directory has been through an init. What is left is the work you came for." }
                    }
                }
            } else {
                Card { variant: CardVariant::Outlined, class: "overview__owed",
                    header { class: "overview__owed-head",
                        Icon { name: "assignment_late", size: 22 }
                        h2 { class: "overview__owed-title", "This estate still owes" }
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

            Card { variant: CardVariant::Filled, class: "overview__identity",
                header { class: "overview__owed-head",
                    Icon { name: "description", size: 22 }
                    h2 { class: "overview__owed-title", "{open.name}" }
                }
                dl { class: "overview__facts",
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

            if last_command.is_some() {
                CommandLog {}
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
    use std::path::PathBuf;

    fn model(packs: Vec<PackRow>, schema: SchemaStatus) -> EstateModel {
        EstateModel {
            main: PathBuf::from("/estates/acme/C0example.satz"),
            outline: Vec::new(),
            params: Vec::new(),
            packs,
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
}
