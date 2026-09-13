//! The Commands view: the satz commands studio runs, each with its argument fields,
//! the streamed log of the one running, and — for `apply` and `bootstrap` — the command
//! line to copy or to open in the OS terminal.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use satz_studio_core::satz::CliLine;

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, Icon, LinearProgress, List, ListItem,
    Switch, TextField, Tooltip,
};
use crate::state::{
    AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, ToastKind, command_line, toast,
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
    /// runs in the OS terminal, never in the app
    pub external: bool,
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
        external: false,
    },
    CommandSpec {
        id: "questions",
        label: "questions",
        icon: "quiz",
        description: "What the estate's packs ask, joined with the answers its params carry.",
        head: &["questions"],
        estate: EstateArg::Positional,
        tail: &["--format", "json"],
        fields: &[Field::Flag {
            key: "unanswered",
            flag: "--unanswered",
            label: "Only the unanswered questions",
        }],
        external: false,
    },
    CommandSpec {
        id: "check-presets",
        label: "check-presets",
        icon: "difference",
        description: "Which packs are clean, behind upstream, or edited locally.",
        head: &["check-presets"],
        estate: EstateArg::Positional,
        tail: &["--format", "json"],
        fields: &[Field::Option {
            key: "pristine_dir",
            flag: "--pristine-dir",
            label: "Pristine directory",
            placeholder: "compare against this directory instead of downloading",
        }],
        external: false,
    },
    CommandSpec {
        id: "iac-roles",
        label: "iac-roles",
        icon: "admin_panel_settings",
        description: "The roles the IaC service account needs against the roles the estate grants it.",
        head: &["iac-roles"],
        estate: EstateArg::Positional,
        tail: &["--format", "json"],
        fields: &[Field::Flag {
            key: "execute",
            flag: "--execute",
            label: "Write the missing roles into the estate file",
        }],
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
            Field::Flag {
                key: "json",
                flag: "--format=json",
                label: "JSON output",
            },
        ],
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
                key: "report",
                flag: "--report",
                label: "Report file",
                placeholder: "evidence/<catalog>-latest.md by default",
            },
        ],
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
        external: true,
    },
];

/// The initial field values of a spec: positionals at their default, everything else empty.
pub fn defaults(spec: &CommandSpec) -> BTreeMap<String, String> {
    spec.fields
        .iter()
        .map(|f| match f {
            Field::Positional { key, default, .. } => ((*key).to_string(), (*default).to_string()),
            _ => (f.key().to_string(), String::new()),
        })
        .collect()
}

/// The argument vector after `satz --config <dir>`: head, positionals, the estate, the
/// tail, flags and options in field order, trailing words last. A flag is on when its
/// value is `"true"`. An empty positional is an error naming the field.
pub fn build_args(
    spec: &CommandSpec,
    estate: &str,
    values: &BTreeMap<String, String>,
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
    let built = build_args(&spec, &open.name, &values());
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

    fn spec(id: &str) -> &'static CommandSpec {
        PALETTE.iter().find(|s| s.id == id).unwrap()
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
        let args = build_args(spec("transpile-check"), "C0example.satz", &BTreeMap::new()).unwrap();
        assert_eq!(args, ["transpile", "C0example.satz", "--check"]);
    }

    #[test]
    fn flags_and_options_follow_the_fixed_words_in_field_order() {
        let mut v = defaults(spec("report-compliance"));
        v.insert("checkov".into(), "true".into());
        v.insert("prowler".into(), " /tmp/prowler.json ".into());
        let args = build_args(spec("report-compliance"), "C0example.satz", &v).unwrap();
        assert_eq!(
            args,
            [
                "report-compliance",
                "cis-gcp-5.0",
                "C0example.satz",
                "--checkov",
                "--prowler",
                "/tmp/prowler.json"
            ]
        );
    }

    #[test]
    fn an_empty_positional_is_refused_by_name() {
        let mut v = defaults(spec("require"));
        v.insert("framework".into(), "  ".into());
        assert_eq!(
            build_args(spec("require"), "x.satz", &v).unwrap_err(),
            "Catalog is required"
        );
    }

    #[test]
    fn the_estate_flag_and_trailing_words() {
        let mut v = defaults(spec("merge-presets"));
        v.insert("report_only".into(), "true".into());
        assert_eq!(
            build_args(spec("merge-presets"), "C0example.satz", &v).unwrap(),
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
            build_args(spec("plan"), "C0example.satz", &v).unwrap(),
            ["plan", "-target=google_folder.x", "-out=p.tfplan"]
        );
    }

    #[test]
    fn a_flag_that_is_off_or_an_empty_option_adds_nothing() {
        let v = defaults(spec("transpile"));
        assert_eq!(
            build_args(spec("transpile"), "C0example.satz", &v).unwrap(),
            ["transpile", "C0example.satz"]
        );
        let v = defaults(spec("update-schema"));
        assert_eq!(
            build_args(spec("update-schema"), "C0example.satz", &v).unwrap(),
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
}
