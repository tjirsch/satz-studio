//! The Import door of the Start screen: the form behind `satz import`, the run, and the
//! report it printed.
//!
//! Two things shape this form. `satz import` imports INTO a project, so a folder that is
//! not an estate yet takes `satz init` first and the form says which of the two it will
//! do before it runs. And the SOURCE decides the shape, so only the flags of the chosen
//! shape are shown: offering `--wrap-all` beside a state file, or `--on-collision`
//! beside a Terraform directory, would be offering satz something it does not take.
//!
//! The live shape reads a Google organisation with the Application Default Credentials
//! and prints what it derived — an organisation id, a directory id, a billing account,
//! an administrator's address. Those lines are shown while the window holds them and are
//! written into the estate satz created, nowhere else.

use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use satz_studio_core::satz::import::{ImportPlan, OnCollision};
use satz_studio_core::satz::{
    CliLine, ImportOptions, ImportReport, ImportShape, InitOptions, SatzError,
};

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, ChipList, Icon, LinearProgress,
    Segment, SegmentedButton, Switch, TextField,
};
use crate::state::{
    AppAction, AppStore, AppStoreStoreExt, CommandOutcome, EstateAction, ImportStoreStoreExt,
    ToastKind, View, run_line, toast,
};
use crate::views::commands::{one_click, spec_of};

/// The provider set `satz init --defaults` knows; it expands to `google` and
/// `google-beta` and fetches the schema of each with the Terraform tool.
const GOOGLE_SET: &str = "google";

/// What the source field asks for, and what it must be, per shape.
fn source_label(shape: ImportShape) -> &'static str {
    match shape {
        ImportShape::State => "State document",
        ImportShape::Live => "Scope",
        ImportShape::Hcl => "Terraform file or directory",
    }
}

fn source_supporting(shape: ImportShape) -> &'static str {
    match shape {
        ImportShape::State => {
            "The JSON `tofu show -json` writes — not a raw .tfstate, whatever it is named. Run `tofu show -json > state.json` first; satz tells the two apart by `values.root_module` and refuses the raw form."
        }
        ImportShape::Live => {
            "organizations/<number>, folders/<number> or projects/<id>. Left blank, satz takes the `root` of its import config. This reads Google with your Application Default Credentials."
        }
        ImportShape::Hcl => {
            "A .tf file, or the directory holding them. What satz cannot express as Satz it carries verbatim inside `hcl trust` and says why, per block."
        }
    }
}

fn shape_segments() -> Vec<Segment> {
    vec![
        Segment::new(ImportShape::State.as_str(), "State document"),
        Segment::new(ImportShape::Live.as_str(), "Live scope"),
        Segment::new(ImportShape::Hcl.as_str(), "Terraform HCL"),
    ]
}

/// Open the OS folder picker for the directory the import runs in.
fn pick_target(mut target: Signal<String>) {
    spawn(async move {
        if let Some(folder) = rfd::AsyncFileDialog::new()
            .set_title("Choose the folder the estate is imported into")
            .pick_folder()
            .await
        {
            target.set(folder.path().display().to_string());
        }
    });
}

/// Open the OS file picker for the source of a shape that reads one.
fn pick_source(mut options: Signal<ImportOptions>, shape: ImportShape) {
    spawn(async move {
        let dialog = rfd::AsyncFileDialog::new().set_title("Choose what to import from");
        let picked = match shape {
            ImportShape::State => {
                dialog
                    .add_filter("tofu show -json", &["json"])
                    .pick_file()
                    .await
            }
            // a Terraform source is a file or the directory holding them; the directory
            // is the usual one, so that is what the button opens
            ImportShape::Hcl => dialog.pick_folder().await,
            ImportShape::Live => None,
        };
        if let Some(file) = picked {
            options.write().source = file.path().display().to_string();
        }
    });
}

#[component]
pub fn ImportEstate() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<AppAction>();
    let mut target = use_signal(String::new);
    let mut options = use_signal(ImportOptions::default);
    // what the `satz init` half runs with when the folder is not an estate yet; tofu is
    // satz's own default and the form states it, so the preview is the command that runs
    let mut init = use_signal(|| InitOptions {
        tf_tool: "tofu".to_string(),
        defaults: vec![GOOGLE_SET.to_string()],
        ..Default::default()
    });

    let running = app.import().running().cloned();
    let log = app.import().log().cloned();
    let last = app.import().command().cloned();
    let outcome = app.import().outcome().cloned();
    let report = app.import().report().cloned();

    let dir = target().trim().to_string();
    let plan = if dir.is_empty() {
        None
    } else {
        Some(satz_studio_core::satz::import::plan(Path::new(&dir)))
    };
    let two_step = matches!(plan, Some(Ok(ImportPlan::InitThenImport)));
    // the same refusal the run would give, said while the form is being filled in
    let problem = match &plan {
        Some(Err(e)) => Some(e.to_string()),
        _ => None,
    };
    let shape = options().shape;
    // the CHEAP half of the check, because this runs on every keystroke: that the source
    // is there, and that a live scope is one. Whether a state document is what
    // `tofu show -json` writes means parsing it, which the runner does once — the field
    // says the requirement instead. A path is only judged once there is a directory to
    // resolve it against; a scope is judged on its own.
    let source_problem = match (&plan, options().check_source_exists(Path::new(&dir))) {
        (_, Err(e @ SatzError::NotAScope(_))) => Some(e.to_string()),
        (Some(Ok(_)), Err(e)) if !options().source.trim().is_empty() => Some(e.to_string()),
        _ => None,
    };
    let needs_source = shape != ImportShape::Live;
    let ready = !dir.is_empty()
        && problem.is_none()
        && source_problem.is_none()
        && !(needs_source && options().source.trim().is_empty())
        && !running;

    let import_line = run_line(Path::new(&dir), &options().argv());
    let init_line = run_line(Path::new(&dir), &init().argv());
    let google = init().defaults.iter().any(|d| d == GOOGLE_SET);

    rsx! {
        div { class: "import",
            Card { variant: CardVariant::Outlined, class: "import__form",
                h2 { class: "import__heading", Icon { name: "move_to_inbox", size: 22 } "Import an estate" }
                p { class: "import__description",
                    "satz import writes a Satz estate out of infrastructure that already exists. It imports INTO a project: a folder that is not an estate yet gets satz init first, and this form says which of the two it will do."
                }

                div { class: "import__folder",
                    TextField {
                        label: "Folder",
                        value: target(),
                        leading_icon: "folder",
                        class: "import__folder-field",
                        supporting: problem.clone().unwrap_or_else(|| "The folder the import runs in: an estate, or an empty folder that becomes one.".to_string()),
                        error: problem.is_some(),
                        disabled: running,
                        oninput: move |v| target.set(v),
                    }
                    Button {
                        icon: "folder_open",
                        variant: ButtonVariant::Tonal,
                        disabled: running,
                        onclick: move |_| pick_target(target),
                        "Choose folder"
                    }
                }
                match &plan {
                    Some(Ok(ImportPlan::Import)) => rsx! {
                        p { class: "import__plan",
                            Icon { name: "check_circle", size: 18 }
                            "This folder holds a config.toml: satz import runs in it, and the estate it writes joins the ones already there."
                        }
                    },
                    Some(Ok(ImportPlan::InitThenImport)) => rsx! {
                        p { class: "import__plan",
                            Icon { name: "info", size: 18 }
                            "This folder holds no config.toml, so satz import would refuse it. satz init runs first and writes config.toml, yaml/, hcl/, schemas/ and .gitignore; the import then fills them. With credentials that name an organisation, init also writes an estate file of its own — the one the import writes is the one that opens."
                        }
                    },
                    _ => rsx! {},
                }

                h3 { class: "import__subheading", "What to import from" }
                p { class: "import__label", "Source" }
                SegmentedButton {
                    options: shape_segments(),
                    selected: shape.as_str().to_string(),
                    onselect: move |v: String| {
                        // the segments are ImportShape::ALL, so nothing else arrives
                        if let Some(shape) = ImportShape::parse(&v) {
                            let mut o = options.write();
                            o.shape = shape;
                            o.source = String::new();
                        }
                    },
                }
                div { class: "import__source",
                    TextField {
                        label: source_label(shape),
                        value: options().source,
                        monospace: true,
                        class: "import__source-field",
                        supporting: source_problem.clone().unwrap_or_else(|| source_supporting(shape).to_string()),
                        error: source_problem.is_some(),
                        disabled: running,
                        oninput: move |v: String| options.write().source = v,
                    }
                    if needs_source {
                        Button {
                            icon: "attach_file",
                            variant: ButtonVariant::Tonal,
                            disabled: running,
                            onclick: move |_| pick_source(options, shape),
                            "Choose"
                        }
                    }
                }

                ShapeOptions { options, running }

                if two_step {
                    h3 { class: "import__subheading", "The folder is not an estate yet" }
                    p { class: "import__note",
                        "These two belong to the satz init that runs first. Everything else about that estate — the customer, the billing account, the region — satz derives from your credentials or the Create door asks for."
                    }
                    p { class: "import__label", "Terraform tool" }
                    SegmentedButton {
                        options: vec![Segment::new("tofu", "tofu"), Segment::new("terraform", "terraform")],
                        selected: init().tf_tool,
                        onselect: move |v: String| init.write().tf_tool = v,
                    }
                    Switch {
                        label: "Fetch the Google provider schema now",
                        checked: google,
                        disabled: running,
                        onchange: move |on: bool| {
                            let mut o = init.write();
                            o.defaults = if on { vec![GOOGLE_SET.to_string()] } else { Vec::new() };
                        },
                    }
                    p { class: "import__note",
                        "--defaults google runs the Terraform tool for each provider's schema, so that tool has to be installed. The Terraform HCL shape needs that schema to translate anything: without it every block is carried verbatim inside `hcl trust`, and the import says so, per block."
                    }
                }

                code { class: "import__preview",
                    if two_step {
                        "{init_line}\n{import_line}"
                    } else {
                        "{import_line}"
                    }
                }
                div { class: "import__actions",
                    Button {
                        variant: ButtonVariant::Filled,
                        icon: "move_to_inbox",
                        disabled: !ready,
                        onclick: move |_| {
                            handle.send(AppAction::ImportEstate {
                                dir: PathBuf::from(&dir),
                                options: options(),
                                init: init(),
                            });
                        },
                        "Import"
                    }
                    Button {
                        variant: ButtonVariant::Outlined,
                        icon: "stop",
                        disabled: !running,
                        onclick: move |_| handle.send(AppAction::CancelImport),
                        "Cancel"
                    }
                }
            }

            div { class: "import__result",
                if !report.is_empty() || outcome.is_some() {
                    ImportReportCard { report: report.clone(), outcome: outcome.clone() }
                }
                Card { variant: CardVariant::Filled, class: "import__log-card",
                    div { class: "import__log-header",
                        Icon { name: "terminal", size: 20 }
                        code { class: "import__log-title", {last.unwrap_or_else(|| "nothing imported yet".to_string())} }
                        span { class: "grow" }
                        if let Some(o) = &outcome {
                            Chip { kind: ChipKind::Assist, icon: if o.ok { "check_circle" } else { "error" }, label: o.text.clone(), error: !o.ok }
                        }
                    }
                    if running {
                        LinearProgress {}
                    }
                    p { class: "import__privacy",
                        Icon { name: "lock", size: 18 }
                        "A live import prints what it read from your credentials — an organisation id, a billing account, an administrator's address. It is written into the estate satz created; this app keeps none of it, not in its settings and not in a file of its own."
                    }
                    pre { class: "log",
                        for (i, line) in log.iter().enumerate() {
                            {
                                let (class, text) = match line {
                                    CliLine::Stdout(s) => ("log__line", s),
                                    CliLine::Stderr(s) => ("log__line log__line--stderr", s),
                                };
                                rsx! { span { key: "{i}", class: "{class}", "{text}\n" } }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The flags of the chosen shape, and no others.
#[component]
fn ShapeOptions(options: Signal<ImportOptions>, running: bool) -> Element {
    let current = options();
    match current.shape {
        ImportShape::State | ImportShape::Live => rsx! {
            h3 { class: "import__subheading", "What to take, and what to call it" }
            ChipList {
                label: "Only these types",
                items: current.only.clone(),
                disabled: running,
                supporting: "--only, over the types the import config marks import: true",
                onchange: move |v: Vec<String>| options.write().only = v,
            }
            ChipList {
                label: "Leave these types out",
                items: current.exclude.clone(),
                disabled: running,
                supporting: "--exclude; a * wildcard is allowed, as in google_*_iam_member",
                onchange: move |v: Vec<String>| options.write().exclude = v,
            }
            Switch {
                label: "Every type the source can deliver",
                checked: current.all,
                disabled: running,
                onchange: move |on: bool| options.write().all = on,
            }
            p { class: "import__note",
                "--all takes every type instead of the rows marked import: true — from a state document every row, live every row with a Cloud Asset Inventory name."
            }
            p { class: "import__label", "One principal, a grant on two containers" }
            SegmentedButton {
                options: vec![
                    Segment::new(OnCollision::Error.as_arg(), "refuse, naming them"),
                    Segment::new(OnCollision::Counter.as_arg(), "number them"),
                ],
                selected: current.on_collision.as_arg().to_string(),
                onselect: move |v: String| {
                    if let Some(on_collision) = OnCollision::parse(&v) {
                        options.write().on_collision = on_collision;
                    }
                },
            }
            p { class: "import__note",
                "--on-collision. The map form emits one address per member and role, so a grant one principal holds on two folders or two projects collides; counter writes the second and later as labelled resources with a running number, and says which."
            }
            TextField {
                label: "Customer shortname",
                value: current.customer_shortname.clone(),
                placeholder: "acme",
                monospace: true,
                disabled: running,
                supporting: "--customer-shortname, which no platform fact carries. Left blank, satz infers it from the leading token of the project and bucket names and reports it as not derivable when nothing repeats.",
                oninput: move |v: String| options.write().customer_shortname = v,
            }
            TextField {
                label: "Output file",
                value: current.output.clone(),
                placeholder: "discovered.satz",
                monospace: true,
                disabled: running,
                supporting: "--output, inside yaml_dir. Left blank, satz writes discovered.satz.",
                oninput: move |v: String| options.write().output = v,
            }
            Switch {
                label: "List every skipped resource",
                checked: current.verbose,
                disabled: running,
                onchange: move |on: bool| options.write().verbose = on,
            }
            p { class: "import__note",
                "--verbose. The import always reports how many resources it skipped and why; this adds one line per resource, which is what satz points at in that summary."
            }
        },
        ImportShape::Hcl => rsx! {
            h3 { class: "import__subheading", "How much to translate" }
            Switch {
                label: "Carry every block verbatim",
                checked: current.wrap_all,
                disabled: running,
                onchange: move |on: bool| options.write().wrap_all = on,
            }
            p { class: "import__note",
                "--wrap-all is the zero-risk form: every block goes inside `hcl trust` and the estate deploys exactly as the source did. Nothing is translated, so the compliance plane cannot see into any of it. Without it satz translates what it can and wraps the rest, saying why per block."
            }
        },
    }
}

/// satz's own import report: what it wrote, what it could not derive, what it left out.
///
/// The sections are satz's sentences, never the app's summary of them, and the run log
/// below carries every line either way.
#[component]
fn ImportReportCard(report: ImportReport, outcome: Option<CommandOutcome>) -> Element {
    let app = use_context::<Store<AppStore>>();
    // the estate's coroutine exists only while one is open, and the import opens it
    let estate = try_use_context::<Coroutine<EstateAction>>();
    let ok = outcome.as_ref().is_some_and(|o| o.ok);
    rsx! {
        Card { variant: CardVariant::Outlined, class: "import__report",
            h3 { class: "import__heading", Icon { name: "summarize", size: 22 } "What satz reported" }
            if report.is_empty() {
                p { class: "import__note", "satz printed no import report. The log below is what it said." }
            }
            ReportSection {
                title: "Wrote",
                icon: "description",
                lines: report.wrote.clone(),
                kind: "wrote",
            }
            ReportSection {
                title: "Skipped",
                icon: "filter_alt_off",
                lines: report.skipped.clone(),
                kind: "skipped",
            }
            ReportSection {
                title: "Params satz could not derive",
                icon: "help",
                lines: report.not_derivable.clone(),
                kind: "not-derivable",
            }
            ReportSection {
                title: "Warnings",
                icon: "warning",
                lines: report.warnings.clone(),
                kind: "warning",
            }
            ReportSection {
                title: "The rest of the report",
                icon: "notes",
                lines: report.rest.clone(),
                kind: "rest",
            }
            if let (true, Some(handle)) = (ok, estate) {
                div { class: "import__next",
                    p { class: "import__note",
                        "satz says it itself: review the file, then transpile and plan. The check is one click and its output is in Checks, where the compile and the catalogs live; the plan is in Deploy."
                    }
                    Button {
                        variant: ButtonVariant::Filled,
                        icon: "fact_check",
                        onclick: move |_| {
                            // the estate the import opened: `transpile --check` on it, as
                            // the Checks deck runs it, with satz's text in the log
                            let Some(open) = app.open().cloned() else {
                                toast(app, ToastKind::Error, "no estate is open to check");
                                return;
                            };
                            match spec_of("transpile-check").ok_or_else(|| "the palette has no transpile-check".to_string()).and_then(|s| one_click(s, &open.name)) {
                                Ok(args) => handle.send(EstateAction::RunCommand(args)),
                                Err(e) => toast(app, ToastKind::Error, e),
                            }
                            app.nav().set(View::Checks);
                        },
                        "Check it compiles"
                    }
                }
            }
        }
    }
}

/// One section of the report, absent when satz printed nothing for it.
#[component]
fn ReportSection(title: String, icon: String, lines: Vec<String>, kind: String) -> Element {
    if lines.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "import__section import__section--{kind}",
            h4 { class: "import__section-title", Icon { name: icon, size: 18 } "{title}" }
            pre { class: "import__section-lines",
                for (i, line) in lines.iter().enumerate() {
                    span { key: "{i}", class: "log__line", "{line}\n" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The preview is the command line the run makes: the directory first, because the
    /// working directory is the whole of the address and there is no `--config`.
    #[test]
    fn the_preview_names_the_directory_and_carries_no_config() {
        let options = ImportOptions {
            shape: ImportShape::Hcl,
            source: "src".to_string(),
            ..Default::default()
        };
        let line = run_line(Path::new("/tmp/acme"), &options.argv());
        assert_eq!(line, "cd /tmp/acme && satz import src --from hcl");
        assert!(!line.contains("--config"));
    }

    /// A folder that is not an estate yet runs two commands, and the preview shows both
    /// in the order they run — the init that makes the project, then the import that
    /// fills it.
    #[test]
    fn the_two_step_preview_shows_the_init_before_the_import() {
        let dir = Path::new("/tmp/acme");
        let init = InitOptions {
            tf_tool: "tofu".to_string(),
            defaults: vec![GOOGLE_SET.to_string()],
            ..Default::default()
        };
        let options = ImportOptions {
            shape: ImportShape::State,
            source: "state.json".to_string(),
            ..Default::default()
        };
        assert_eq!(
            run_line(dir, &init.argv()),
            "cd /tmp/acme && satz init --tf-tool tofu --defaults google"
        );
        assert_eq!(
            run_line(dir, &options.argv()),
            "cd /tmp/acme && satz import state.json --from state --on-collision error"
        );
    }

    /// The row of shapes is every shape satz has, by the value the options parse back,
    /// and each one asks for its own source in its own words.
    #[test]
    fn the_row_offers_every_shape_and_each_asks_for_its_own_source() {
        let segments = shape_segments();
        assert_eq!(
            segments
                .iter()
                .map(|s| s.value.as_str())
                .collect::<Vec<_>>(),
            ImportShape::ALL
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
        );
        for segment in &segments {
            assert!(ImportShape::parse(&segment.value).is_some(), "{segment:?}");
        }
        let labels: Vec<&str> = ImportShape::ALL.iter().map(|s| source_label(*s)).collect();
        assert_eq!(labels.len(), 3);
        assert!(labels.iter().all(|l| !l.is_empty()));
    }

    /// The state shape's refusal is stated in the form, before a run: satz reads what
    /// `tofu show -json` writes, and a raw `.tfstate` is not it.
    #[test]
    fn the_state_shape_says_what_it_reads_before_anything_runs() {
        let said = source_supporting(ImportShape::State);
        assert!(said.contains("tofu show -json > state.json"), "{said}");
        assert!(said.contains(".tfstate"), "{said}");
    }
}
