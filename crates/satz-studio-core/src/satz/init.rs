//! `satz init`: the one command that makes an estate where there was none.
//!
//! It runs in a working directory rather than against a `--config`
//! ([`super::SatzCli::run_in`]) and creates `config.toml`, `satz/`, `hcl/`, `schemas/`,
//! `.gitignore` and the estate file there. It is a LIVE command that runs signed in: what the
//! command line does not state it derives from the sign-in —
//! `customer_domain` and the first admin from the signed-in identity, `customer_id` and
//! `customer_organization_id` from an `organizations:search`, `billing_account_infra`
//! from `billingAccounts.list` — and says of each value where it came from.
//!
//! Those derived values are a customer's organization id, billing account and
//! administrator address. They belong in the estate satz writes and nowhere else: this
//! module renders a command line and reads a directory listing, and neither carries a
//! value back into settings, a transcript or a log file.

use std::path::{Path, PathBuf};

use crate::estate::{EstateDir, EstateError, ToolConfig};

/// The arguments of one `satz init` run, as the form holds them.
///
/// Every field is a flag `satz init` has, and nothing here is invented: an empty string
/// or an empty list is not passed at all, which is how the command is told to derive
/// the value or fall back to its own default. The flags satz has that this does NOT
/// carry are the ones the app has no business sending — `--interview`, which reads
/// stdin and would hang a child with no terminal; `--from-live`, which satz accepts and
/// ignores because deriving is already the default; and `--customer-organization-id`, `--customer-domain`,
/// `--iac-user`, `--infra-project-name` and `--infra-bucket-name`, which the
/// sign-in or the short name answer and `satz interview` edits afterwards.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InitOptions {
    /// `--customer-shortname`: the one value nothing on the platform can answer.
    /// `infra_project_name` and `infra_bucket_name` follow from it.
    pub customer_shortname: String,
    /// `--customer-id`: the Workspace/Cloud Identity directory id. Left empty it comes
    /// from `organizations:search`, and an init that cannot derive one writes no estate
    /// file at all.
    pub customer_id: String,
    /// `--billing-account-infra`. Left empty it comes from `billingAccounts.list`.
    pub billing_account_infra: String,
    /// `--default-region`. Left empty satz writes its own default.
    pub default_region: String,
    /// `--tf-tool`: `tofu` or `terraform`.
    pub tf_tool: String,
    /// `--defaults`: the provider sets to include — `google` is the set satz knows,
    /// and it expands to `google` and `google-beta`.
    pub defaults: Vec<String>,
    /// `--providers`: provider names to include beside the sets.
    pub providers: Vec<String>,
    /// `--from`: the interface folders an organization's estate wrote for a delegation — its own,
    /// `delegation-<name>`, and that of the delegated resource the estate works in. With any, satz
    /// writes a WORKLOAD estate from them and the organization's flags are not sent.
    pub from: Vec<String>,
    /// `--name`, with `--from`: the estate's name. Left empty it is the delegation's.
    pub name: String,
    /// `--force`: an estate file already in `yaml_dir` is rewritten — without it an
    /// organization's takes the stated values merged in and a workload's is refused.
    /// `config.toml` and `.gitignore` are kept either way ([`existing`]).
    pub force: bool,
}

impl InitOptions {
    /// The whole command line after the binary: `init` and the flags that carry a
    /// value. The order is fixed so the preview a form shows is the command that runs.
    pub fn argv(&self) -> Vec<String> {
        let mut argv = vec!["init".to_string()];
        let from: Vec<&str> = self
            .from
            .iter()
            .map(|f| f.trim())
            .filter(|f| !f.is_empty())
            .collect();
        if !from.is_empty() {
            argv.push("--from".to_string());
            argv.extend(from.iter().map(|f| f.to_string()));
        }
        let mut flag = |name: &str, value: &str| {
            let value = value.trim();
            if !value.is_empty() {
                argv.push(name.to_string());
                argv.push(value.to_string());
            }
        };
        if from.is_empty() {
            flag("--customer-shortname", &self.customer_shortname);
            flag("--customer-id", &self.customer_id);
            flag("--billing-account-infra", &self.billing_account_infra);
            flag("--default-region", &self.default_region);
        } else {
            flag("--name", &self.name);
        }
        flag("--tf-tool", &self.tf_tool);
        for (name, list) in [
            ("--defaults", &self.defaults),
            ("--providers", &self.providers),
        ] {
            let joined = list
                .iter()
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
                .collect::<Vec<_>>()
                .join(",");
            if !joined.is_empty() {
                argv.push(name.to_string());
                argv.push(joined);
            }
        }
        if self.force {
            argv.push("--force".to_string());
        }
        argv
    }
}

/// Whether `dir` is a directory `init` may be run in: it must exist — the child is
/// spawned with `dir` as its working directory, and a directory that is not there is an
/// unreadable spawn failure. What the directory already holds is [`existing`]'s answer.
pub fn check_target(dir: &Path) -> Result<(), super::SatzError> {
    if !dir.is_dir() {
        return Err(super::SatzError::TargetMissing(dir.to_path_buf()));
    }
    Ok(())
}

/// The files and folders in `dir` that this `init` run would meet, relative to `dir`:
/// `config.toml` and `.gitignore`, which satz keeps as they are; every estate file in the
/// `yaml_dir` the run writes into — `config.toml`'s own when there is one, satz's default
/// otherwise — which `--force` rewrites; and, for each `--from` folder, the copy under
/// `interfaces/incoming/` that satz replaces, unless that copy IS the folder named.
///
/// The estate file's name is not knowable before the run (it follows the customer id the
/// sign-in answers, or the delegation's name), so every estate file of `yaml_dir` is
/// named. A `config.toml` that does not parse is an error, not an empty answer: satz
/// would read that file before anything else.
pub fn existing(dir: &Path, options: &InitOptions) -> Result<Vec<String>, EstateError> {
    let config_path = dir.join("config.toml");
    let mut found = Vec::new();
    let estate_dir = if config_path.is_file() {
        found.push("config.toml".to_string());
        EstateDir::open(dir)?
    } else {
        let tool: ToolConfig = toml::from_str("").map_err(|e| EstateError::Parse {
            path: config_path.clone(),
            source: e,
        })?;
        EstateDir {
            runtime: tool.resolved(dir),
            tool,
            config_path,
            dir: dir.to_path_buf(),
        }
    };
    if dir.join(".gitignore").is_file() {
        found.push(".gitignore".to_string());
    }
    let relative = |p: &Path| {
        p.strip_prefix(dir)
            .unwrap_or(p)
            .to_string_lossy()
            .into_owned()
    };
    if Path::new(&estate_dir.runtime.yaml_dir).is_dir() {
        found.extend(estate_dir.estates()?.iter().map(|p| relative(p)));
    }
    let incoming = Path::new(&estate_dir.runtime.interfaces_dir).join("incoming");
    for folder in options
        .from
        .iter()
        .map(|f| f.trim())
        .filter(|f| !f.is_empty())
    {
        let folder = Path::new(folder)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(folder));
        let Some(name) = folder.file_name() else {
            continue;
        };
        let copy = incoming.join(name);
        let in_place = copy.canonicalize().is_ok_and(|c| c == folder);
        if copy.exists() && !in_place {
            found.push(relative(&copy));
        }
    }
    Ok(found)
}

/// The sentence a refused run is summed up by: satz's own `error: …` line, whole —
/// the first one, since what follows it is consequence — or, when satz printed none,
/// the last line of stderr. `None` when stderr was empty.
pub fn refusal(stderr: &[String]) -> Option<String> {
    let lines: Vec<&str> = stderr
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    lines
        .iter()
        .find(|l| l.starts_with("error:"))
        .or(lines.last())
        .map(|l| l.to_string())
}

/// What an `init` run left in `dir`: the estate directory it wrote, and the estate
/// files in that directory's `yaml_dir`.
///
/// The estate's file NAME is not knowable before the run — `init` names it after the
/// customer id, which it reads from the sign-in when the command line states none —
/// so it is read from the directory rather than predicted. An empty list is not a
/// failure of this function: `init` writes the directories, `config.toml` and
/// `.gitignore` whatever happens, and writes an estate file only when a customer id was
/// stated or could be derived, so an empty list is the answer "no estate, and the run
/// log says why".
pub fn created(dir: &Path) -> Result<(EstateDir, Vec<PathBuf>), EstateError> {
    let estate_dir = EstateDir::open(dir)?;
    let estates = estate_dir.estates()?;
    Ok((estate_dir, estates))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example customer of satz's `docs/examples.md`; no live value is ever written
    /// into a test here.
    const CUSTOMER_ID: &str = "C0example";

    #[test]
    fn an_empty_form_is_the_bare_command() {
        assert_eq!(InitOptions::default().argv(), ["init"]);
    }

    #[test]
    fn every_field_renders_its_own_flag_in_order() {
        let options = InitOptions {
            customer_shortname: "acme".to_string(),
            customer_id: CUSTOMER_ID.to_string(),
            billing_account_infra: "012345-6789AB-CDEF01".to_string(),
            default_region: "europe-west3".to_string(),
            tf_tool: "tofu".to_string(),
            defaults: vec!["google".to_string()],
            providers: vec!["google-beta".to_string(), "random".to_string()],
            from: Vec::new(),
            name: String::new(),
            force: false,
        };
        assert_eq!(
            options.argv(),
            [
                "init",
                "--customer-shortname",
                "acme",
                "--customer-id",
                CUSTOMER_ID,
                "--billing-account-infra",
                "012345-6789AB-CDEF01",
                "--default-region",
                "europe-west3",
                "--tf-tool",
                "tofu",
                "--defaults",
                "google",
                "--providers",
                "google-beta,random",
            ]
        );
    }

    /// A blank field is the instruction to derive, so it must not reach the command
    /// line as an empty value: `--customer-id ""` states an id, and an absent
    /// `--customer-id` asks the sign-in for one.
    #[test]
    fn a_blank_or_whitespace_field_is_not_passed_at_all() {
        let options = InitOptions {
            customer_shortname: "  acme  ".to_string(),
            customer_id: "   ".to_string(),
            billing_account_infra: String::new(),
            default_region: String::new(),
            tf_tool: String::new(),
            defaults: vec![String::new(), "  ".to_string()],
            providers: Vec::new(),
            from: Vec::new(),
            name: String::new(),
            force: false,
        };
        assert_eq!(options.argv(), ["init", "--customer-shortname", "acme"]);
    }

    #[test]
    fn a_directory_that_is_not_there_is_refused_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nope");
        let err = check_target(&missing).unwrap_err();
        assert!(
            matches!(err, super::super::SatzError::TargetMissing(ref p) if p == &missing),
            "{err:?}"
        );
    }

    /// A folder a first run left half-made is no refusal: the run goes ahead, and what
    /// it would meet is named — `config.toml`, `.gitignore`, the estate files in the
    /// `yaml_dir` that `config.toml` names.
    #[test]
    fn a_directory_that_already_holds_a_config_names_what_is_there() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("config.toml"), "yaml_dir = \"yaml\"\n").unwrap();
        std::fs::write(tmp.path().join(".gitignore"), "hcl/\n").unwrap();
        std::fs::create_dir_all(tmp.path().join("yaml")).unwrap();
        std::fs::write(
            tmp.path().join("yaml").join(format!("{CUSTOMER_ID}.satz")),
            "estate acme\n",
        )
        .unwrap();
        assert!(check_target(tmp.path()).is_ok());
        assert_eq!(
            existing(tmp.path(), &InitOptions::default()).unwrap(),
            [
                "config.toml".to_string(),
                ".gitignore".to_string(),
                Path::new("yaml")
                    .join(format!("{CUSTOMER_ID}.satz"))
                    .to_string_lossy()
                    .into_owned(),
            ]
        );
    }

    /// Without a `config.toml` the run writes into satz's default `satz/`, so an estate
    /// file there is named too.
    #[test]
    fn without_a_config_the_default_estate_directory_is_read() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("satz")).unwrap();
        std::fs::write(
            tmp.path().join("satz").join("shop_dev.satz"),
            "estate shop_dev\n",
        )
        .unwrap();
        assert_eq!(
            existing(tmp.path(), &InitOptions::default()).unwrap(),
            [Path::new("satz")
                .join("shop_dev.satz")
                .to_string_lossy()
                .into_owned()]
        );
    }

    /// A `--from` folder's earlier copy under `interfaces/incoming/` is named — satz
    /// replaces it — unless the folder named IS that copy.
    #[test]
    fn an_earlier_copy_of_an_interface_folder_is_named() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        let folder = source.path().join("delegation-shop");
        std::fs::create_dir_all(&folder).unwrap();
        let copy = tmp
            .path()
            .join("interfaces")
            .join("incoming")
            .join("delegation-shop");
        std::fs::create_dir_all(&copy).unwrap();
        let from = |f: &Path| InitOptions {
            from: vec![f.display().to_string()],
            ..Default::default()
        };
        assert_eq!(
            existing(tmp.path(), &from(&folder)).unwrap(),
            [Path::new("interfaces")
                .join("incoming")
                .join("delegation-shop")
                .to_string_lossy()
                .into_owned()]
        );
        assert!(existing(tmp.path(), &from(&copy)).unwrap().is_empty());
    }

    #[test]
    fn a_config_that_does_not_parse_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("config.toml"), "yaml_dir = [").unwrap();
        assert!(matches!(
            existing(tmp.path(), &InitOptions::default()),
            Err(EstateError::Parse { .. })
        ));
    }

    #[test]
    fn an_empty_directory_holds_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            existing(tmp.path(), &InitOptions::default())
                .unwrap()
                .is_empty()
        );
    }

    /// `--force` closes the command line, in both forms, and is absent when off.
    #[test]
    fn force_renders_its_flag_in_both_forms() {
        let organization = InitOptions {
            customer_shortname: "acme".to_string(),
            force: true,
            ..Default::default()
        };
        assert_eq!(
            organization.argv(),
            ["init", "--customer-shortname", "acme", "--force"]
        );
        let workload = InitOptions {
            from: vec!["/org/interfaces/outgoing/delegation-shop".to_string()],
            name: "shop-dev".to_string(),
            force: true,
            ..Default::default()
        };
        assert_eq!(
            workload.argv(),
            [
                "init",
                "--from",
                "/org/interfaces/outgoing/delegation-shop",
                "--name",
                "shop-dev",
                "--force"
            ]
        );
        let off = InitOptions {
            force: false,
            ..workload
        };
        assert!(!off.argv().contains(&"--force".to_string()));
        assert!(
            !InitOptions {
                force: false,
                ..organization
            }
            .argv()
            .contains(&"--force".to_string())
        );
    }

    /// satz's `error:` line stands whole for the run, even when lines follow it; without
    /// one, the last line of stderr does.
    #[test]
    fn the_refusal_is_satzs_first_error_line() {
        let stderr = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            refusal(&stderr(&[
                "satz v0.106.0 (built 2026-11-01)",
                "error: delegation-shop carries the names before the rename — copy it again",
                "",
                "hint: satz init --help",
            ]))
            .as_deref(),
            Some("error: delegation-shop carries the names before the rename — copy it again")
        );
        assert_eq!(
            refusal(&stderr(&["satz v0.106.0", "nothing could be derived", ""])).as_deref(),
            Some("nothing could be derived")
        );
        assert_eq!(refusal(&[]), None);
    }

    #[test]
    fn an_empty_directory_is_ready() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(check_target(tmp.path()).is_ok());
    }

    /// The shape `init` leaves behind, read back without the name being known in
    /// advance: the file is found because it declares an estate, not because the test
    /// guessed what it would be called.
    #[test]
    fn created_finds_the_estate_file_by_reading_the_directory() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("satz")).unwrap();
        std::fs::write(tmp.path().join("config.toml"), "yaml_dir = \"satz\"\n").unwrap();
        std::fs::write(
            tmp.path().join("satz").join(format!("{CUSTOMER_ID}.satz")),
            "estate acme\n",
        )
        .unwrap();
        let (dir, estates) = created(tmp.path()).unwrap();
        assert_eq!(dir.config_path, tmp.path().join("config.toml"));
        assert_eq!(estates.len(), 1);
        assert_eq!(
            estates[0].file_name().unwrap(),
            format!("{CUSTOMER_ID}.satz").as_str()
        );
    }

    /// `init` without a customer id — none stated and none derivable — writes the
    /// directories and the config and no estate file. That is a run that made no
    /// estate, not a broken directory.
    #[test]
    fn created_reports_no_estate_when_init_wrote_none() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("satz")).unwrap();
        // no `yaml_dir`: the estate directory is `satz/`, as satz reads it
        std::fs::write(tmp.path().join("config.toml"), "").unwrap();
        let (_, estates) = created(tmp.path()).unwrap();
        assert!(estates.is_empty());
    }

    #[test]
    fn created_over_a_directory_init_never_touched_is_no_config() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(matches!(created(tmp.path()), Err(EstateError::NoConfig(_))));
    }
    /// A workload estate: the interface folders after `--from`, the name, and none of the
    /// organization's flags — whatever the form still holds of them.
    #[test]
    fn a_teams_estate_is_written_from_its_interface_folders() {
        let options = InitOptions {
            customer_shortname: "acme".to_string(),
            tf_tool: "tofu".to_string(),
            from: vec![
                "/org/interfaces/outgoing/delegation-shop".to_string(),
                " /org/interfaces/outgoing/shop ".to_string(),
                String::new(),
            ],
            name: "shop-dev".to_string(),
            ..Default::default()
        };
        assert_eq!(
            options.argv(),
            [
                "init",
                "--from",
                "/org/interfaces/outgoing/delegation-shop",
                "/org/interfaces/outgoing/shop",
                "--name",
                "shop-dev",
                "--tf-tool",
                "tofu"
            ]
        );
        // without folders the name is not sent: it belongs to `--from`
        let organization = InitOptions {
            customer_shortname: "acme".to_string(),
            name: "x".to_string(),
            ..Default::default()
        };
        assert_eq!(
            organization.argv(),
            ["init", "--customer-shortname", "acme"]
        );
    }
}
