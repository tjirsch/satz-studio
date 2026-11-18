//! The Create door of the Start screen: the form behind `satz init`, and the run.
//!
//! `init` is a live command. It reads the Application Default Credentials and derives
//! the customer's domain, directory id, organisation id, billing account and first
//! administrator from the platform, saying of each where it came from. The form
//! therefore asks only for what satz cannot answer and offers the derivable values as
//! overrides that are empty by default; what the run derives is shown in the log while
//! the window is open and is written into the estate satz created, nowhere else.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use dioxus_icons::lucide::{
    CircleAlert, CircleCheck, FolderOpen, HousePlus, Lock, Square, Terminal,
};
use satz_studio_core::satz::init::check_target;
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

    let running = app.create().running().cloned();
    let log = app.create().log().cloned();
    let last = app.create().command().cloned();
    let outcome = app.create().outcome().cloned();

    let dir = target().trim().to_string();
    // the same refusal the run would give, said while the folder is being chosen
    let problem = if dir.is_empty() {
        None
    } else {
        check_target(Path::new(&dir)).err().map(|e| e.to_string())
    };
    let ready = !dir.is_empty() && problem.is_none() && !running;
    let preview = run_line(Path::new(&dir), &options().argv());
    let google = options().defaults.iter().any(|d| d == GOOGLE_SET);
    let tool_pressed: HashSet<usize> = TF_TOOLS
        .iter()
        .position(|tool| *tool == options().tf_tool)
        .into_iter()
        .collect();

    rsx! {
        div { class: "create",
            Card { class: "create__form",
                CardHeader {
                    h2 { class: "create__heading", HousePlus { size: 22 } "New estate" }
                    CardDescription { class: "create__description",
                        "satz init writes config.toml, satz/, hcl/, schemas/, .gitignore and the estate file into the folder you choose. It reads your Application Default Credentials and derives the customer's domain, directory id, organisation id, billing account and first administrator from the platform, saying of each where it came from."
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
                    Field {
                        label: "Workload folder",
                        value: options().workload_folder_name,
                        placeholder: "Workloads",
                        disabled: running,
                        supporting: "The folder directly under the organisation that holds the customer's and the teams' folders. Left blank, they live at the organisation and no folder is created.",
                        oninput: move |v: String| options.write().workload_folder_name = v,
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

                    h3 { class: "create__subheading", "From your credentials, unless you state them" }
                    p { class: "create__note",
                        "Left blank, each of these is read from the credentials you are signed in with. State one to override what the platform answers."
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

                    code { class: "create__preview", "{preview}" }
                    div { class: "create__actions",
                        Button {
                            disabled: !ready,
                            onclick: move |_| {
                                handle.send(AppAction::CreateEstate {
                                    dir: PathBuf::from(&dir),
                                    options: options(),
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
                        if let Some(o) = &outcome {
                            Badge { variant: if o.ok { BadgeVariant::Outline } else { BadgeVariant::Destructive },
                                if o.ok { CircleCheck { size: 12 } } else { CircleAlert { size: 12 } }
                                "{o.text}"
                            }
                        }
                    }
                    if running {
                        Progress { value: None::<f64>, "aria-label": "working" }
                    }
                    p { class: "create__privacy",
                        Lock { size: 18 }
                        "What satz derives — an organisation id, a billing account, an administrator's address — is printed here and written into the estate satz creates. This app keeps none of it: not in its settings, not in a file of its own."
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
