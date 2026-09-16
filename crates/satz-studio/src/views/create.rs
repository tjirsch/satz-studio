//! The Create door of the Estates view: the form behind `satz init`, and the run.
//!
//! `init` is a live command. It reads the Application Default Credentials and derives
//! the customer's domain, directory id, organisation id, billing account and first
//! administrator from the platform, saying of each where it came from. The form
//! therefore asks only for what satz cannot answer and offers the derivable values as
//! overrides that are empty by default; what the run derives is shown in the log while
//! the window is open and is written into the estate satz created, nowhere else.

use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use satz_studio_core::satz::init::check_target;
use satz_studio_core::satz::{CliLine, InitOptions};

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, ChipList, Icon, LinearProgress,
    Segment, SegmentedButton, Switch, TextField,
};
use crate::state::{AppAction, AppStore, AppStoreStoreExt, CreateStoreStoreExt, run_line};

/// The provider set `satz init --defaults` knows; it expands to `google` and
/// `google-beta` and fetches the schema of each with the tf tool.
const GOOGLE_SET: &str = "google";

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

    rsx! {
        div { class: "create",
            Card { variant: CardVariant::Outlined, class: "create__form",
                h2 { class: "create__heading", Icon { name: "add_home", size: 22 } "New estate" }
                p { class: "create__description",
                    "satz init writes config.toml, yaml/, hcl/, schemas/, .gitignore and the estate file into the folder you choose. It reads your Application Default Credentials and derives the customer's domain, directory id, organisation id, billing account and first administrator from the platform, saying of each where it came from."
                }

                div { class: "create__folder",
                    TextField {
                        label: "Folder",
                        value: target(),
                        leading_icon: "folder",
                        class: "create__folder-field",
                        supporting: problem.clone().unwrap_or_else(|| "An empty folder: satz creates the estate inside it".to_string()),
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

                h3 { class: "create__subheading", "What satz cannot derive" }
                TextField {
                    label: "Customer shortname",
                    value: options().customer_shortname,
                    placeholder: "acme",
                    monospace: true,
                    disabled: running,
                    supporting: "Nothing on the platform names the customer. The infrastructure project and its state bucket follow from it as <shortname>-infra-001 and <shortname>-infra-001-state; left blank they are written empty.",
                    oninput: move |v: String| options.write().customer_shortname = v,
                }
                TextField {
                    label: "Default region",
                    value: options().default_region,
                    placeholder: "europe-west3",
                    monospace: true,
                    disabled: running,
                    supporting: "Left blank, satz writes europe-west3.",
                    oninput: move |v: String| options.write().default_region = v,
                }
                p { class: "create__label", "Terraform tool" }
                SegmentedButton {
                    options: vec![Segment::new("tofu", "tofu"), Segment::new("terraform", "terraform")],
                    selected: options().tf_tool,
                    onselect: move |v: String| options.write().tf_tool = v,
                }
                Switch {
                    label: "Fetch the Google provider schema now",
                    checked: google,
                    disabled: running,
                    onchange: move |on: bool| {
                        let mut o = options.write();
                        o.defaults = if on { vec![GOOGLE_SET.to_string()] } else { Vec::new() };
                    },
                }
                p { class: "create__note",
                    "--defaults google writes google and google-beta into config.toml and runs the Terraform tool for each provider's schema, so that tool has to be installed. Without it the estate still uses both providers, and the Commands view's update-schema fetches the schema later."
                }
                ChipList {
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
                TextField {
                    label: "Customer id",
                    value: options().customer_id,
                    monospace: true,
                    disabled: running,
                    supporting: "The directory id, from organizations:search. satz names the estate file after it, and a run that has none writes no estate file at all.",
                    oninput: move |v: String| options.write().customer_id = v,
                }
                TextField {
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
                        variant: ButtonVariant::Filled,
                        icon: "add_home",
                        disabled: !ready,
                        onclick: move |_| {
                            handle.send(AppAction::CreateEstate {
                                dir: PathBuf::from(&dir),
                                options: options(),
                            });
                        },
                        "Create"
                    }
                    Button {
                        variant: ButtonVariant::Outlined,
                        icon: "stop",
                        disabled: !running,
                        onclick: move |_| handle.send(AppAction::CancelCreate),
                        "Cancel"
                    }
                }
            }

            Card { variant: CardVariant::Filled, class: "create__log-card",
                div { class: "create__log-header",
                    Icon { name: "terminal", size: 20 }
                    code { class: "create__log-title", {last.unwrap_or_else(|| "nothing created yet".to_string())} }
                    span { class: "grow" }
                    if let Some(o) = &outcome {
                        Chip { kind: ChipKind::Assist, icon: if o.ok { "check_circle" } else { "error" }, label: o.text.clone(), error: !o.ok }
                    }
                }
                if running {
                    LinearProgress {}
                }
                p { class: "create__privacy",
                    Icon { name: "lock", size: 18 }
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
