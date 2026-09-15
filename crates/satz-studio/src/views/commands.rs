//! The Commands view: the satz commands studio runs, each with its argument fields,
//! the streamed log of the one running, and — for `apply` and `bootstrap` — the command
//! line to copy or to open in the OS terminal.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use satz_studio_core::satz::CliLine;

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, Icon, LinearProgress, List, ListItem,
    Segment, SegmentedButton, Switch, TextField, Tooltip,
};
use crate::state::{
    AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, ToastKind, command_line,
    reports_dir, toast,
};

/// Where the open estate's file goes on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstateArg {
    /// the command takes no estate
    None,
    /// as a positional after the positional fields
    Positional,
    /// as `--estate <file>`
    Flag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// `flag` when on
    Flag {
        key: &'static str,
        flag: &'static str,
        label: &'static str,
    },
    /// `flag <one of options>`, always: the choice starts at `default` and an empty
    /// one is an error naming the field
    Choice {
        key: &'static str,
        flag: &'static str,
        label: &'static str,
        options: &'static [&'static str],
        default: &'static str,
    },
    /// `flag <value>` when the value is not empty
    Option {
        key: &'static str,
        flag: &'static str,
        label: &'static str,
        placeholder: &'static str,
    },
    /// a required positional, before the estate
    Positional {
        key: &'static str,
        label: &'static str,
        default: &'static str,
    },
    /// free words appended last, split on whitespace
    Trailing {
        key: &'static str,
        label: &'static str,
        placeholder: &'static str,
    },
}

impl Field {
    pub fn key(&self) -> &'static str {
        match self {
            Field::Flag { key, .. }
            | Field::Choice { key, .. }
            | Field::Option { key, .. }
            | Field::Positional { key, .. }
            | Field::Trailing { key, .. } => key,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    pub description: &'static str,
    /// the leading words, `["transpile"]`
    pub head: &'static [&'static str],
    pub estate: EstateArg,
    /// fixed words after the estate, `["--check"]`
    pub tail: &'static [&'static str],
    pub fields: &'static [Field],
    /// a reporting command: satz's ADR 0021 gives it one `--format` and one `--out`,
    /// both required, and it prints nothing. The destination is the file the spec's
    /// `--out` field names, else one of the app's own, which the log is filled from
    /// and which is removed.
    pub reports: bool,
    /// runs in the OS terminal, never in the app
    pub external: bool,
}

/// The file extension of a `--format` value: the destination says what it holds.
pub fn extension(format: &str) -> &'static str {
    match format {
        "markdown" => "md",
        "json" => "json",
        "pdf" => "pdf",
        "xlsx" => "xlsx",
        _ => "txt",
    }
}

/// The read-only session tools the view offers as one click: name and what it answers.
pub const SESSION_TOOLS: &[(&str, &str, &str)] = &[
    (
        "satz_whoami",
        "person_search",
        "both halves of the identity, with the live checks that decide whether the next call works",
    ),
    (
        "satz_transpile_check",
        "fact_check",
        "compile in memory over the session, write nothing",
    ),
    (
        "satz_questions",
        "quiz",
        "the questions the estate's packs declare, with their state",
    ),
];

/// The palette, in the order the view lists it.
pub const PALETTE: &[CommandSpec] = &[
    CommandSpec {
        id: "transpile-check",
        label: "transpile --check",
        icon: "fact_check",
        description: "Compile in memory and write nothing: the estate either transpiles or the error says why.",
        head: &["transpile"],
        estate: EstateArg::Positional,
        tail: &["--check"],
        fields: &[],
        reports: false,
        external: false,
    },
    CommandSpec {
        id: "transpile",
        label: "transpile",
        icon: "build",
        description: "Compile the estate to HCL in hcl_dir.",
        head: &["transpile"],
        estate: EstateArg::Positional,
        tail: &[],
        fields: &[
            Field::Flag {
                key: "plan",
                flag: "--plan",
                label: "Run plan afterwards",
            },
            Field::Flag {
                key: "scan",
                flag: "--scan",
                label: "Run Checkov afterwards",
            },
            Field::Flag {
                key: "print_variables",
                flag: "--print-variables",
                label: "Print the resolved variables",
            },
        ],
        reports: false,
        external: false,
    },
    CommandSpec {
        id: "questions",
        label: "questions",
        icon: "quiz",
        description: "What the estate's packs ask, joined with the answers its params carry.",
        head: &["questions"],
        estate: EstateArg::Positional,
        tail: &[],
        fields: &[
            Field::Choice {
                key: "format",
                flag: "--format",
                label: "Format",
                options: &["text", "markdown", "json"],
                default: "json",
            },
            Field::Flag {
                key: "unanswered",
                flag: "--unanswered",
                label: "Only the unanswered questions",
            },
        ],
        reports: true,
        external: false,
    },
    CommandSpec {
        id: "check-presets",
        label: "check-presets",
        icon: "difference",
        description: "Which packs are clean, behind upstream, or edited locally.",
        head: &["check-presets"],
        estate: EstateArg::Positional,
        tail: &[],
        fields: &[
            Field::Choice {
                key: "format",
                flag: "--format",
                label: "Format",
                options: &["text", "json"],
                default: "json",
            },
            Field::Option {
                key: "pristine_dir",
                flag: "--pristine-dir",
                label: "Pristine directory",
                placeholder: "compare against this directory instead of downloading",
            },
        ],
        reports: true,
        external: false,
    },
    CommandSpec {
        id: "iac-roles",
        label: "iac-roles",
        icon: "admin_panel_settings",
        description: "The roles the IaC service account needs against the roles the estate grants it.",
        // one of the two commands satz's ADR 0021 leaves on the console: the exit code
        // is the answer and the text is the diagnosis, so it takes no `--out`
        head: &["iac-roles"],
        estate: EstateArg::Positional,
        tail: &["--format", "json"],
        fields: &[Field::Flag {
            key: "execute",
            flag: "--execute",
            label: "Write the missing roles into the estate file",
        }],
        reports: false,
        external: false,
    },
    CommandSpec {
        id: "update-schema",
        label: "update-schema",
        icon: "schema",
        description: "Fetch the provider schema into schema_dir.",
        head: &["update-schema"],
        estate: EstateArg::None,
        tail: &[],
        fields: &[
            Field::Option {
                key: "providers",
                flag: "--providers",
                label: "Providers",
                placeholder: "comma-separated; the config's by default",
            },
            Field::Option {
                key: "version",
                flag: "--version",
                label: "Provider version",
                placeholder: "the config's by default",
            },
        ],
        reports: false,
        external: false,
    },
    CommandSpec {
        id: "hcl-init",
        label: "hcl-init",
        icon: "play_circle",
        description: "Run the tool's init in hcl_dir.",
        head: &["hcl-init"],
        estate: EstateArg::None,
        tail: &[],
        fields: &[Field::Trailing {
            key: "args",
            label: "Extra arguments",
            placeholder: "-reconfigure, -migrate-state",
        }],
        reports: false,
        external: false,
    },
    CommandSpec {
        id: "plan",
        label: "plan",
        icon: "preview",
        description: "Run the tool's plan in hcl_dir.",
        head: &["plan"],
        estate: EstateArg::None,
        tail: &[],
        fields: &[Field::Trailing {
            key: "args",
            label: "Extra arguments",
            placeholder: "-target=…, -out=plan.tfplan",
        }],
        reports: false,
        external: false,
    },
    CommandSpec {
        id: "require",
        label: "require",
        icon: "rule",
        description: "Which controls of a catalog the declared estate satisfies.",
        head: &["require"],
        estate: EstateArg::Positional,
        tail: &[],
        fields: &[
            Field::Positional {
                key: "framework",
                label: "Catalog",
                default: "cis-gcp-5.0",
            },
            Field::Choice {
                key: "format",
                flag: "--format",
                label: "Format",
                options: &["text", "json"],
                default: "text",
            },
        ],
        reports: true,
        external: false,
    },
    CommandSpec {
        id: "report-compliance",
        label: "report-compliance",
        icon: "verified",
        description: "The goal view joined with live verification, written as the evidence report.",
        head: &["report-compliance"],
        estate: EstateArg::Positional,
        tail: &[],
        fields: &[
            Field::Positional {
                key: "framework",
                label: "Catalog",
                default: "cis-gcp-5.0",
            },
            Field::Choice {
                key: "format",
                flag: "--format",
                label: "Format",
                options: &["markdown", "json"],
                default: "markdown",
            },
            Field::Flag {
                key: "no_live",
                flag: "--no-live",
                label: "Declared estate only, no live verification",
            },
            Field::Flag {
                key: "checkov",
                flag: "--checkov",
                label: "Add the Checkov column",
            },
            Field::Option {
                key: "prowler",
                flag: "--prowler",
                label: "Prowler OCSF export",
                placeholder: "path to the json-ocsf file",
            },
            Field::Option {
                key: "out",
                flag: "--out",
                label: "Report file",
                placeholder: "a file of the estate's; empty writes one the log shows",
            },
        ],
        reports: true,
        external: false,
    },
    CommandSpec {
        id: "get-presets",
        label: "get-presets",
        icon: "download",
        description: "Fetch the upstream library: install what is missing, refresh what the estate does not use.",
        head: &["get-presets"],
        estate: EstateArg::None,
        tail: &[],
        fields: &[
            Field::Flag {
                key: "force",
                flag: "--force",
                label: "Overwrite packs the estate uses as well",
            },
            Field::Option {
                key: "pristine_dir",
                flag: "--pristine-dir",
                label: "Pristine directory",
                placeholder: "take the library from here instead of downloading",
            },
        ],
        reports: false,
        external: false,
    },
    CommandSpec {
        id: "merge-presets",
        label: "merge-presets",
        icon: "merge",
        description: "Reconcile the presets with upstream: new packs installed, unmodified ones upgraded, edited ones forked.",
        head: &["merge-presets"],
        estate: EstateArg::Flag,
        tail: &[],
        fields: &[
            Field::Flag {
                key: "report_only",
                flag: "--report-only",
                label: "Report only, write nothing",
            },
            Field::Option {
                key: "pristine_dir",
                flag: "--pristine-dir",
                label: "Pristine directory",
                placeholder: "compare against this directory instead of downloading",
            },
            Field::Option {
                key: "adopt",
                flag: "--adopt",
                label: "Adopt upstream in place",
                placeholder: "a pack stem, or all",
            },
        ],
        reports: false,
        external: false,
    },
    CommandSpec {
        id: "apply",
        label: "apply",
        icon: "rocket_launch",
        description: "Run the tool's apply in hcl_dir — in your terminal, where its approval prompt is.",
        head: &["apply"],
        estate: EstateArg::None,
        tail: &[],
        fields: &[Field::Trailing {
            key: "args",
            label: "Extra arguments",
            placeholder: "-target=…, a saved plan file",
        }],
        reports: false,
        external: true,
    },
    CommandSpec {
        id: "bootstrap",
        label: "bootstrap",
        icon: "foundation",
        description: "Day 0: folder, project, billing link, core APIs and the state bucket, after a permission pre-flight — in your terminal.",
        head: &["bootstrap"],
        estate: EstateArg::Positional,
        tail: &[],
        fields: &[
            Field::Flag {
                key: "dry_run",
                flag: "--dry-run",
                label: "Dry run: print the plan and run the pre-flight, create nothing",
            },
            Field::Flag {
                key: "greenfield",
                flag: "--greenfield",
                label: "Greenfield: materialise a not-yet-existing organisation",
            },
        ],
        reports: false,
        external: true,
    },
];

/// The initial field values of a spec: positionals and choices at their default,
/// everything else empty.
pub fn defaults(spec: &CommandSpec) -> BTreeMap<String, String> {
    spec.fields
        .iter()
        .map(|f| match f {
            Field::Positional { key, default, .. } | Field::Choice { key, default, .. } => {
                ((*key).to_string(), (*default).to_string())
            }
            _ => (f.key().to_string(), String::new()),
        })
        .collect()
}

/// The `--format` a spec's choice field currently holds, `"text"` for a spec without
/// one.
fn format_of(spec: &CommandSpec, values: &BTreeMap<String, String>) -> String {
    spec.fields
        .iter()
        .find_map(|f| match f {
            Field::Choice {
                key,
                flag: "--format",
                ..
            } => values.get(*key).map(|v| v.trim().to_string()),
            _ => None,
        })
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "text".to_string())
}

/// Where a reporting command writes when no field of its own names a file: the app's
/// own file, one per command, named for the format it holds. The run reads it into the
/// log and removes it.
pub fn report_path(spec: &CommandSpec, values: &BTreeMap<String, String>) -> PathBuf {
    let format = format_of(spec, values);
    reports_dir().join(format!("{}.{}", spec.id, extension(&format)))
}

/// The argument vector after `satz --config <dir>`: head, positionals, the estate, the
/// tail, flags, choices and options in field order, the destination of a reporting
/// command, trailing words last. A flag is on when its value is `"true"`. An empty
/// positional or choice is an error naming the field.
///
/// A reporting command writes one file and prints nothing (satz's ADR 0021), so it
/// always carries `--out`: the file its own field names, else `report_to`.
pub fn build_args(
    spec: &CommandSpec,
    estate: &str,
    values: &BTreeMap<String, String>,
    report_to: &Path,
) -> Result<Vec<String>, String> {
    let value = |key: &str| {
        values
            .get(key)
            .map(String::as_str)
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let mut args: Vec<String> = spec.head.iter().map(|s| (*s).to_string()).collect();
    for f in spec.fields {
        if let Field::Positional { key, label, .. } = f {
            let v = value(key);
            if v.is_empty() {
                return Err(format!("{label} is required"));
            }
            args.push(v);
        }
    }
    match spec.estate {
        EstateArg::None => {}
        EstateArg::Positional => args.push(estate.to_string()),
        EstateArg::Flag => {
            args.push("--estate".to_string());
            args.push(estate.to_string());
        }
    }
    args.extend(spec.tail.iter().map(|s| (*s).to_string()));
    for f in spec.fields {
        match f {
            Field::Flag { key, flag, .. } => {
                if value(key) == "true" {
                    args.push((*flag).to_string());
                }
            }
            Field::Choice {
                key, flag, label, ..
            } => {
                let v = value(key);
                if v.is_empty() {
                    return Err(format!("{label} is required"));
                }
                args.push((*flag).to_string());
                args.push(v);
            }
            Field::Option { key, flag, .. } => {
                let v = value(key);
                if !v.is_empty() {
                    args.push((*flag).to_string());
                    args.push(v);
                }
            }
            Field::Positional { .. } => {}
            Field::Trailing { .. } => {}
        }
    }
    if spec.reports && !args.iter().any(|a| a == "--out") {
        args.push("--out".to_string());
        args.push(report_to.display().to_string());
    }
    for f in spec.fields {
        if let Field::Trailing { key, .. } = f {
            args.extend(value(key).split_whitespace().map(str::to_string));
        }
    }
    Ok(args)
}

#[component]
pub fn CommandsView() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let mut selected = use_signal(|| 0usize);
    let mut values = use_signal(|| defaults(&PALETTE[0]));
    let spec = PALETTE[selected()];
    let open = app.open().cloned();
    let Some(open) = open else {
        return rsx! {};
    };
    let running = app.estate().running().cloned();
    let log = app.estate().command_log().cloned();
    let last = app.estate().last_command().cloned();
    let outcome = app.estate().outcome().cloned();
    let built = build_args(&spec, &open.name, &values(), &report_path(&spec, &values()));
    let preview = built
        .as_ref()
        .map(|args| command_line(&open.dir, args))
        .unwrap_or_else(|e| e.clone());

    rsx! {
        div { class: "view commands",
            h1 { class: "view__title", "Commands" }
            div { class: "commands__layout",
                Card { variant: CardVariant::Filled, class: "commands__palette",
                    List {
                        for (i, s) in PALETTE.iter().enumerate() {
                            ListItem {
                                key: "{s.id}",
                                headline: s.label.to_string(),
                                supporting: if s.external { "in your terminal".to_string() } else { String::new() },
                                selected: i == selected(),
                                leading: rsx! { Icon { name: s.icon.to_string(), size: 20 } },
                                onclick: move |_| {
                                    selected.set(i);
                                    values.set(defaults(&PALETTE[i]));
                                },
                            }
                        }
                    }
                }
                div { class: "commands__work",
                    Card { variant: CardVariant::Outlined, class: "commands__form",
                        h2 { class: "commands__heading", Icon { name: spec.icon.to_string(), size: 22 } "{spec.label}" }
                        p { class: "commands__description", "{spec.description}" }
                        for f in spec.fields {
                            {
                                let key = f.key().to_string();
                                let current = values().get(&key).cloned().unwrap_or_default();
                                match *f {
                                    Field::Flag { label, .. } => rsx! {
                                        Switch { key: "{key}", label: label.to_string(), checked: current == "true", onchange: move |v: bool| { values.write().insert(key.clone(), v.to_string()); } }
                                    },
                                    Field::Choice { label, options, .. } => rsx! {
                                        p { key: "{key}", class: "commands__label", "{label}" }
                                        SegmentedButton {
                                            options: options.iter().map(|o| Segment::new(*o, *o)).collect::<Vec<_>>(),
                                            selected: current,
                                            onselect: move |v: String| { values.write().insert(key.clone(), v); },
                                        }
                                    },
                                    Field::Option { label, placeholder, .. } => rsx! {
                                        TextField { key: "{key}", label: label.to_string(), value: current, placeholder: placeholder.to_string(), monospace: true, oninput: move |v: String| { values.write().insert(key.clone(), v); } }
                                    },
                                    Field::Positional { label, .. } => rsx! {
                                        TextField { key: "{key}", label: label.to_string(), value: current, monospace: true, error: values().get(&key).map(|v| v.trim().is_empty()).unwrap_or(true), oninput: move |v: String| { values.write().insert(key.clone(), v); } }
                                    },
                                    Field::Trailing { label, placeholder, .. } => rsx! {
                                        TextField { key: "{key}", label: label.to_string(), value: current, placeholder: placeholder.to_string(), monospace: true, oninput: move |v: String| { values.write().insert(key.clone(), v); } }
                                    },
                                }
                            }
                        }
                        code { class: "commands__preview", "{preview}" }
                        div { class: "commands__actions",
                            if spec.external {
                                Button {
                                    variant: ButtonVariant::Tonal,
                                    icon: "content_copy",
                                    disabled: built.is_err(),
                                    onclick: {
                                        let preview = preview.clone();
                                        move |_| copy_to_clipboard(app, &preview)
                                    },
                                    "Copy"
                                }
                                Button {
                                    variant: ButtonVariant::Filled,
                                    icon: "open_in_new",
                                    disabled: built.is_err(),
                                    onclick: {
                                        let built = built.clone();
                                        move |_| {
                                            if let Ok(args) = &built {
                                                handle.send(EstateAction::OpenInTerminal(args.clone()));
                                            }
                                        }
                                    },
                                    "Open in terminal"
                                }
                            } else {
                                Button {
                                    variant: ButtonVariant::Filled,
                                    icon: "play_arrow",
                                    disabled: running || built.is_err(),
                                    onclick: {
                                        let built = built.clone();
                                        move |_| {
                                            if let Ok(args) = &built {
                                                handle.send(EstateAction::RunCommand(args.clone()));
                                            }
                                        }
                                    },
                                    "Run"
                                }
                                Button { variant: ButtonVariant::Outlined, icon: "stop", disabled: !running, onclick: move |_| handle.send(EstateAction::CancelCommand), "Cancel" }
                            }
                        }
                    }
                    Card { variant: CardVariant::Outlined, class: "commands__tools",
                        h2 { class: "commands__heading", Icon { name: "handyman", size: 22 } "Session tools" }
                        p { class: "commands__description", "One call on this estate's satz mcp session, as the agent would make it; the result lands in the log." }
                        div { class: "commands__actions commands__actions--start",
                            for (name, icon, description) in SESSION_TOOLS {
                                Tooltip { key: "{name}", text: description.to_string(),
                                    Button {
                                        variant: ButtonVariant::Tonal,
                                        icon: icon.to_string(),
                                        disabled: running,
                                        onclick: move |_| handle.send(EstateAction::RunTool { name: name.to_string(), args: serde_json::Map::new() }),
                                        "{name}"
                                    }
                                }
                            }
                        }
                    }
                    Card { variant: CardVariant::Filled, class: "commands__log-card",
                        div { class: "commands__log-header",
                            Icon { name: "terminal", size: 20 }
                            code { class: "commands__log-title", {last.unwrap_or_else(|| "no command run yet".to_string())} }
                            span { class: "grow" }
                            if let Some(o) = &outcome {
                                Chip { kind: ChipKind::Assist, icon: if o.ok { "check_circle" } else { "error" }, label: o.text.clone(), error: !o.ok }
                            }
                        }
                        if running {
                            LinearProgress {}
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
}

fn copy_to_clipboard(app: Store<AppStore>, text: &str) {
    let literal = serde_json::to_string(text).unwrap_or_default();
    let _ = document::eval(&format!("navigator.clipboard.writeText({literal})"));
    toast(app, ToastKind::Info, "Copied");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The destination the app would name; `build_args` takes it, so a test reads the
    /// same command line the preview shows.
    const OUT: &str = "/tmp/satz-studio-reports/report";

    fn spec(id: &str) -> &'static CommandSpec {
        PALETTE.iter().find(|s| s.id == id).unwrap()
    }

    fn built(id: &str, estate: &str, values: &BTreeMap<String, String>) -> Vec<String> {
        build_args(spec(id), estate, values, Path::new(OUT)).unwrap()
    }

    #[test]
    fn every_id_is_unique_and_every_field_key_is_unique_within_its_spec() {
        let mut ids: Vec<&str> = PALETTE.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PALETTE.len());
        for s in PALETTE {
            let mut keys: Vec<&str> = s.fields.iter().map(Field::key).collect();
            keys.sort_unstable();
            keys.dedup();
            assert_eq!(keys.len(), s.fields.len(), "{}", s.id);
        }
    }

    #[test]
    fn a_check_puts_the_estate_before_the_tail() {
        let args = built("transpile-check", "C0example.satz", &BTreeMap::new());
        assert_eq!(args, ["transpile", "C0example.satz", "--check"]);
    }

    #[test]
    fn flags_and_options_follow_the_fixed_words_in_field_order() {
        let mut v = defaults(spec("report-compliance"));
        v.insert("checkov".into(), "true".into());
        v.insert("prowler".into(), " /tmp/prowler.json ".into());
        assert_eq!(
            built("report-compliance", "C0example.satz", &v),
            [
                "report-compliance",
                "cis-gcp-5.0",
                "C0example.satz",
                "--format",
                "markdown",
                "--checkov",
                "--prowler",
                "/tmp/prowler.json",
                "--out",
                OUT
            ]
        );
    }

    #[test]
    fn an_empty_positional_is_refused_by_name() {
        let mut v = defaults(spec("require"));
        v.insert("framework".into(), "  ".into());
        assert_eq!(
            build_args(spec("require"), "x.satz", &v, Path::new(OUT)).unwrap_err(),
            "Catalog is required"
        );
    }

    #[test]
    fn an_empty_choice_is_refused_by_name() {
        let mut v = defaults(spec("questions"));
        v.insert("format".into(), String::new());
        assert_eq!(
            build_args(spec("questions"), "x.satz", &v, Path::new(OUT)).unwrap_err(),
            "Format is required"
        );
    }

    #[test]
    fn the_estate_flag_and_trailing_words() {
        let mut v = defaults(spec("merge-presets"));
        v.insert("report_only".into(), "true".into());
        assert_eq!(
            built("merge-presets", "C0example.satz", &v),
            [
                "merge-presets",
                "--estate",
                "C0example.satz",
                "--report-only"
            ]
        );
        let mut v = defaults(spec("plan"));
        v.insert(
            "args".into(),
            "-target=google_folder.x  -out=p.tfplan".into(),
        );
        assert_eq!(
            built("plan", "C0example.satz", &v),
            ["plan", "-target=google_folder.x", "-out=p.tfplan"]
        );
    }

    #[test]
    fn a_flag_that_is_off_or_an_empty_option_adds_nothing() {
        let v = defaults(spec("transpile"));
        assert_eq!(
            built("transpile", "C0example.satz", &v),
            ["transpile", "C0example.satz"]
        );
        let v = defaults(spec("update-schema"));
        assert_eq!(
            built("update-schema", "C0example.satz", &v),
            ["update-schema"]
        );
    }

    #[test]
    fn only_apply_and_bootstrap_run_outside_the_app() {
        let external: Vec<&str> = PALETTE
            .iter()
            .filter(|s| s.external)
            .map(|s| s.id)
            .collect();
        assert_eq!(external, ["apply", "bootstrap"]);
    }

    /// satz's ADR 0021: a reporting command takes one `--format` and one `--out`, both
    /// required. `iac-roles` is the one entry the ADR leaves on the console.
    #[test]
    fn every_reporting_command_names_a_format_and_a_destination() {
        let reporting: Vec<&str> = PALETTE.iter().filter(|s| s.reports).map(|s| s.id).collect();
        assert_eq!(
            reporting,
            ["questions", "check-presets", "require", "report-compliance"]
        );
        for s in PALETTE.iter().filter(|s| s.reports) {
            let args = built(s.id, "C0example.satz", &defaults(s));
            assert!(args.iter().any(|a| a == "--format"), "{}: {args:?}", s.id);
            assert_eq!(
                args.iter().filter(|a| *a == "--out").count(),
                1,
                "{}: {args:?}",
                s.id
            );
        }
        let iac = spec("iac-roles");
        assert!(!iac.reports);
        assert_eq!(iac.tail, ["--format", "json"]);
        assert!(
            !built("iac-roles", "C0example.satz", &defaults(iac))
                .iter()
                .any(|a| a == "--out")
        );
    }

    #[test]
    fn a_reporting_command_takes_the_app_s_file_and_a_named_one_wins() {
        let v = defaults(spec("questions"));
        assert_eq!(
            built("questions", "C0example.satz", &v),
            [
                "questions",
                "C0example.satz",
                "--format",
                "json",
                "--out",
                OUT
            ]
        );
        let mut v = defaults(spec("report-compliance"));
        v.insert("out".into(), "evidence/cis.md".into());
        let args = built("report-compliance", "C0example.satz", &v);
        assert_eq!(args.iter().filter(|a| *a == "--out").count(), 1, "{args:?}");
        assert!(args.ends_with(&["--out".to_string(), "evidence/cis.md".to_string()]));
    }

    #[test]
    fn the_app_s_file_is_named_for_the_format_it_holds() {
        let s = spec("questions");
        let mut v = defaults(s);
        assert_eq!(
            report_path(s, &v).file_name().unwrap(),
            std::ffi::OsStr::new("questions.json")
        );
        v.insert("format".into(), "markdown".into());
        assert_eq!(
            report_path(s, &v).file_name().unwrap(),
            std::ffi::OsStr::new("questions.md")
        );
        v.insert("format".into(), "text".into());
        assert_eq!(
            report_path(s, &v).file_name().unwrap(),
            std::ffi::OsStr::new("questions.txt")
        );
        assert!(report_path(s, &v).starts_with(reports_dir()));
    }
}
