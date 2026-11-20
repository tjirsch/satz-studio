//! The Create door of the Start screen: the form behind `satz init`, and the run.
//!
//! `init` is a live command. It reads the sign-in and derives
//! the customer's domain, directory id, organization id, billing account and first
//! administrator from the platform, saying of each where it came from. The form
//! therefore asks only for what satz cannot answer and offers the derivable values as
//! overrides that are empty by default; what the run derives is shown in the log while
//! the window is open and is written into the estate satz created, nowhere else.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use dioxus_icons::lucide::{
    CircleAlert, CircleCheck, FolderOpen, HousePlus, Lock, Square, Terminal, TriangleAlert,
};
use satz_studio_core::satz::init::{check_target, existing};
use satz_studio_core::satz::{CliLine, InitOptions};

use crate::components::badge::{Badge, BadgeVariant};
use crate::components::button::{Button, ButtonVariant};
use crate::components::card::{Card, CardContent, CardDescription, CardHeader};
use crate::components::progress::Progress;
use crate::components::toggle_group::{ToggleGroup, ToggleItem};
use crate::state::{AppAction, AppStore, AppStoreStoreExt, CreateStoreStoreExt, run_line};
use crate::widgets::{Field, SwitchRow, TagList};

/// The provider set `satz init --defaults` knows; it expands to `google` and
/// `google-beta` and fetches the schema of each with the tf tool.
const GOOGLE_SET: &str = "google";

/// The Terraform tools `satz init --tf-tool` takes.
const TF_TOOLS: [&str; 2] = ["tofu", "terraform"];

/// The OS dialog for the interface folders a workload estate is written from: one or more
/// folders, added to the ones already chosen.
fn pick_interface_folders(mut options: Signal<InitOptions>) {
    spawn(async move {
        let picked = rfd::AsyncFileDialog::new()
            .set_title("Choose the interface folders")
            .pick_folders()
            .await;
        if let Some(folders) = picked {
            let mut o = options.write();
            for f in folders {
                let path = f.path().display().to_string();
                if !o.from.contains(&path) {
                    o.from.push(path);
                }
            }
        }
    });
}

/// Open the OS folder picker for the directory the new estate is created in.
fn pick_target(mut target: Signal<String>) {
    spawn(async move {
        if let Some(folder) = rfd::AsyncFileDialog::new()
            .set_title("Choose the folder the new estate is created in")
            .pick_folder()
            .await
        {
            target.set(folder.path().display().to_string());
        }
    });
}

/// The one warning line over a folder that already holds an estate's files: what is
/// there, and what satz does with each kind — it keeps `config.toml`, `.gitignore` and the
/// schemas, rewrites the estate file it writes only under `--force`, replaces an interface
/// folder's earlier copy, and writes nothing when it refuses (satz v0.107.0).
fn existing_warning(found: &[String]) -> String {
    let copies = found
        .iter()
        .any(|f| Path::new(f).starts_with(Path::new("interfaces").join("incoming")));
    let mut line = format!(
        "This folder already holds an estate's files: {}. satz keeps config.toml, .gitignore and the schemas as they are, and rewrites the estate file it writes only with Rewrite the estate file on — without it, an organization's estate file takes the values stated here and a workload's is refused. A refused run writes nothing.",
        found.join(", ")
    );
    if copies {
        line.push_str(" An interface folder's earlier copy is replaced.");
    }
    line
}

#[component]
pub fn CreateEstate() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<AppAction>();
    let mut target = use_signal(String::new);
    // tofu is satz's own default; the form states it rather than leaving the choice
    // implicit, so the preview is the command that runs
    let mut options = use_signal(|| InitOptions {
        tf_tool: "tofu".to_string(),
        ..Default::default()
    });

    // an organization's estate, or a workload estate from the interface folders it was given
    let mut team = use_signal(|| false);
    let running = app.create().running().cloned();
    let log = app.create().log().cloned();
    let last = app.create().command().cloned();
    let outcome = app.create().outcome().cloned();

    let dir = target().trim().to_string();
    // the same refusal the run would give, said while the folder is being chosen, and
    // what the folder already holds that the run would meet
    let (problem, found) = if dir.is_empty() {
        (None, Vec::new())
    } else {
        match check_target(Path::new(&dir)) {
            Err(e) => (Some(e.to_string()), Vec::new()),
            Ok(()) => match existing(Path::new(&dir), &options()) {
                Ok(found) => (None, found),
                Err(e) => (Some(e.to_string()), Vec::new()),
            },
        }
    };
    // the switch belongs to a folder that holds something to write over: over an empty
    // one it is not drawn, and the run does not carry it
    let mut run = options();
    run.force &= !found.is_empty();
    let ready =
        !dir.is_empty() && problem.is_none() && !running && (!team() || !options().from.is_empty());
    let preview = run_line(Path::new(&dir), &run.argv());
    let google = options().defaults.iter().any(|d| d == GOOGLE_SET);

    let mode_pressed: HashSet<usize> = [usize::from(team())].into_iter().collect();
    let tool_index = TF_TOOLS.iter().position(|t| *t == options().tf_tool);
    let tool_pressed: HashSet<usize> = tool_index.into_iter().collect();

    rsx! {
        div { class: "create",
            Card { class: "create__form",
                CardHeader {
                    h2 { class: "create__heading", HousePlus { size: 22 } "New estate" }
                    CardDescription {
                        "satz init writes config.toml, satz/, hcl/, schemas/, .gitignore and the estate file into the folder you choose. It reads your sign-in and derives the customer's domain, directory id, organization id, billing account and first administrator from the platform, saying of each where it came from."
                    }
                }
                CardContent {
                    div { class: "create__folder",
                        Field {
                            label: "Folder",
                            value: target(),
                            class: "create__folder-field",
                            supporting: problem.clone().unwrap_or_else(|| "An empty folder: satz creates the estate inside it".to_string()),
                            error: problem.is_some(),
                            disabled: running,
                            oninput: move |v| target.set(v),
                        }
                        Button {
                            variant: ButtonVariant::Secondary,
                            disabled: running,
                            onclick: move |_| pick_target(target),
                            FolderOpen { size: 16 }
                            "Choose folder"
                        }
                    }
                    if !found.is_empty() {
                        p { class: "create__warning",
                            TriangleAlert { size: 18 }
                            {existing_warning(&found)}
                        }
                        SwitchRow {
                            label: "Rewrite the estate file",
                            checked: options().force,
                            disabled: running,
                            onchange: move |on: bool| options.write().force = on,
                        }
                    }

                    p { class: "create__label", "What the estate is for" }
                    ToggleGroup {
                        horizontal: true,
                        pressed: Some(mode_pressed.clone()),
                        on_pressed_change: move |now: HashSet<usize>| {
                            // a click on the pressed one leaves it pressed
                            if let Some(&i) = now.difference(&mode_pressed).next() {
                                team.set(i == 1);
                                if i == 0 {
                                    let mut o = options.write();
                                    o.from.clear();
                                    o.name.clear();
                                }
                            }
                        },
                        ToggleItem { index: 0usize, "An organization" }
                        ToggleItem { index: 1usize, "A workload, from interface folders" }
                    }
                    if team() {
                        p { class: "create__note",
                            "A workload estate is written from the folders an organization's estate wrote for a delegation, under its interfaces/outgoing/: the delegation's own, delegation-<name>, and that of each delegated resource it was given. satz copies them into interfaces/incoming/ here and writes an estate that runs as the delegation's account."
                        }
                        TagList {
                            label: "Interface folders",
                            items: options().from,
                            disabled: running,
                            supporting: "--from: the delegation's folder and one place",
                            onchange: move |v: Vec<String>| options.write().from = v,
                        }
                        div { class: "create__actions",
                            Button {
                                variant: ButtonVariant::Secondary,
                                disabled: running,
                                onclick: move |_| pick_interface_folders(options),
                                FolderOpen { size: 16 }
                                "Add folders…"
                            }
                        }
                        Field {
                            label: "Name",
                            value: options().name,
                            monospace: true,
                            disabled: running,
                            supporting: "--name: the estate's name. Left blank, the delegation's.",
                            oninput: move |v: String| options.write().name = v,
                        }
                    }
                    if !team() {
                        h3 { class: "create__subheading", "What satz cannot derive" }
                        Field {
                            label: "Customer shortname",
                            value: options().customer_shortname,
                            placeholder: "acme",
                            monospace: true,
                            disabled: running,
                            supporting: "Nothing on the platform names the customer. The infrastructure project and its state bucket follow from it as <shortname>-infra-001 and <shortname>-infra-001-state; left blank they are written empty.",
                            oninput: move |v: String| options.write().customer_shortname = v,
                        }
                        Field {
                            label: "Default region",
                            value: options().default_region,
                            placeholder: "europe-west3",
                            monospace: true,
                            disabled: running,
                            supporting: "Left blank, satz writes europe-west3.",
                            oninput: move |v: String| options.write().default_region = v,
                        }
                    }
                    p { class: "create__label", "Terraform tool" }
                    ToggleGroup {
                        horizontal: true,
                        pressed: Some(tool_pressed.clone()),
                        on_pressed_change: move |now: HashSet<usize>| {
                            // a click on the pressed one leaves it pressed
                            if let Some(tool) = now.difference(&tool_pressed).next().and_then(|i| TF_TOOLS.get(*i)) {
                                options.write().tf_tool = (*tool).to_string();
                            }
                        },
                        for (i, tool) in TF_TOOLS.iter().enumerate() {
                            ToggleItem { key: "{tool}", index: i, "{tool}" }
                        }
                    }
                    SwitchRow {
                        label: "Fetch the Google provider schema now",
                        checked: google,
                        disabled: running,
                        onchange: move |on: bool| {
                            let mut o = options.write();
                            o.defaults = if on { vec![GOOGLE_SET.to_string()] } else { Vec::new() };
                        },
                    }
                    p { class: "create__note",
                        "--defaults google writes google and google-beta into config.toml and runs the Terraform tool for each provider's schema, so that tool has to be installed. Without it the estate still uses both providers, and the update-schema command fetches the schema later."
                    }
                    TagList {
                        label: "Extra providers",
                        items: options().providers,
                        disabled: running,
                        supporting: "--providers, beside the set above",
                        onchange: move |v: Vec<String>| options.write().providers = v,
                    }

                    if !team() {
                        h3 { class: "create__subheading", "From your sign-in, unless you state them" }
                        p { class: "create__note",
                            "Left blank, each of these is read from the account you are signed in with. State one to override what the platform answers."
                        }
                        Field {
                            label: "Customer id",
                            value: options().customer_id,
                            monospace: true,
                            disabled: running,
                            supporting: "The directory id, from organizations:search. satz names the estate file after it, and a run that has none writes no estate file at all.",
                            oninput: move |v: String| options.write().customer_id = v,
                        }
                        Field {
                            label: "Billing account",
                            value: options().billing_account_infra,
                            monospace: true,
                            disabled: running,
                            supporting: "From billingAccounts.list, when exactly one account is open to you.",
                            oninput: move |v: String| options.write().billing_account_infra = v,
                        }
                    }

                    code { class: "create__preview", "{preview}" }
                    div { class: "create__actions",
                        Button {
                            disabled: !ready,
                            onclick: move |_| {
                                handle.send(AppAction::CreateEstate {
                                    dir: PathBuf::from(&dir),
                                    options: run.clone(),
                                });
                            },
                            HousePlus { size: 16 }
                            "Create"
                        }
                        Button {
                            variant: ButtonVariant::Outline,
                            disabled: !running,
                            onclick: move |_| handle.send(AppAction::CancelCreate),
                            Square { size: 16 }
                            "Cancel"
                        }
                    }
                }
            }

            Card { class: "create__log-card",
                CardContent {
                    div { class: "create__log-header",
                        Terminal { size: 20 }
                        code { class: "create__log-title", {last.unwrap_or_else(|| "nothing created yet".to_string())} }
                        span { class: "grow" }
                        if let Some(o) = outcome.as_ref().filter(|o| o.ok) {
                            Badge { variant: BadgeVariant::Outline,
                                CircleCheck { size: 12 }
                                "{o.text}"
                            }
                        }
                    }
                    // a refusal is satz's whole sentence, wrapped, never cut to fit a badge
                    if let Some(o) = outcome.as_ref().filter(|o| !o.ok) {
                        p { class: "create__refusal",
                            CircleAlert { size: 18 }
                            "{o.text}"
                        }
                    }
                    if running {
                        Progress { value: None::<f64>, "aria-label": "working" }
                    }
                    p { class: "create__privacy",
                        Lock { size: 18 }
                        "What satz derives — an organization id, a billing account, an administrator's address — is printed here and written into the estate satz creates. This app keeps none of it: not in its settings, not in a file of its own."
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The preview is the command line the run makes: the directory first, because the
    /// working directory is the whole of the address and there is no `--config`. Before
    /// a folder is chosen there is no directory to name and the command stands alone.
    #[test]
    fn the_preview_names_the_directory_and_carries_no_config() {
        let options = InitOptions {
            customer_shortname: "acme".to_string(),
            ..Default::default()
        };
        let line = run_line(Path::new("/tmp/acme"), &options.argv());
        assert_eq!(line, "cd /tmp/acme && satz init --customer-shortname acme");
        assert!(!line.contains("--config"));
        assert_eq!(
            run_line(Path::new(""), &options.argv()),
            "satz init --customer-shortname acme"
        );
    }

    #[test]
    fn the_warning_names_every_file_found() {
        let line = existing_warning(&["config.toml".to_string(), ".gitignore".to_string()]);
        assert!(
            line.starts_with(
                "This folder already holds an estate's files: config.toml, .gitignore."
            )
        );
        assert!(line.contains("Rewrite the estate file"));
        assert!(line.contains("A refused run writes nothing."));
        assert!(!line.contains("interface folder"));
        let copy = Path::new("interfaces")
            .join("incoming")
            .join("delegation-shop")
            .to_string_lossy()
            .into_owned();
        assert!(
            existing_warning(&[copy]).ends_with("An interface folder's earlier copy is replaced.")
        );
    }

    /// The switch is the only writer of `defaults`, and `--defaults google` is what it
    /// means.
    #[test]
    fn the_google_switch_renders_the_defaults_flag() {
        let on = InitOptions {
            defaults: vec![GOOGLE_SET.to_string()],
            ..Default::default()
        };
        assert_eq!(on.argv(), ["init", "--defaults", "google"]);
        assert_eq!(InitOptions::default().argv(), ["init"]);
    }
}
