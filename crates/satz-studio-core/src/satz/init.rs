//! `satz init`: the one command that makes an estate where there was none.
//!
//! It runs in a working directory rather than against a `--config`
//! ([`super::SatzCli::run_in`]) and creates `config.toml`, `satz/`, `hcl/`, `schemas/`,
//! `.gitignore` and the estate file there. It is a LIVE, credentialed command: what the
//! command line does not state it derives from the Application Default Credentials —
//! `customer_domain` and the first admin from the ADC identity, `customer_id` and
//! `customer_organization_id` from an `organizations:search`, `billing_account_infra`
//! from `billingAccounts.list` — and says of each value where it came from.
//!
//! Those derived values are a customer's organisation id, billing account and
//! administrator address. They belong in the estate satz writes and nowhere else: this
//! module renders a command line and reads a directory listing, and neither carries a
//! value back into settings, a transcript or a log file.

use std::path::{Path, PathBuf};

use crate::estate::{EstateDir, EstateError};

/// The arguments of one `satz init` run, as the form holds them.
///
/// Every field is a flag `satz init` has, and nothing here is invented: an empty string
/// or an empty list is not passed at all, which is how the command is told to derive
/// the value or fall back to its own default. The flags satz has that this does NOT
/// carry are the ones the app has no business sending — `--interview`, which reads
/// stdin and would hang a child with no terminal; `--from-live`, which satz accepts and
/// ignores because deriving is already the default; `--force`, because Create refuses a
/// directory that already holds a `config.toml` rather than writing over one
/// ([`check_target`]); and `--customer-organization-id`, `--customer-domain`,
/// `--iac-user`, `--infra-project-name` and `--infra-bucket-name`, which the
/// credentials or the short name answer and `satz interview` edits afterwards.
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
    /// `--workload-folder-name`: the display name of the folder directly under the
    /// organisation that holds the customer's and the teams' folders. Left empty satz
    /// writes `workload_folder_name = ""` — the organisation itself, for which nothing is
    /// created.
    pub workload_folder_name: String,
    /// `--tf-tool`: `tofu` or `terraform`.
    pub tf_tool: String,
    /// `--defaults`: the provider sets to include — `google` is the set satz knows,
    /// and it expands to `google` and `google-beta`.
    pub defaults: Vec<String>,
    /// `--providers`: provider names to include beside the sets.
    pub providers: Vec<String>,
}

impl InitOptions {
    /// The whole command line after the binary: `init` and the flags that carry a
    /// value. The order is fixed so the preview a form shows is the command that runs.
    pub fn argv(&self) -> Vec<String> {
        let mut argv = vec!["init".to_string()];
        let mut flag = |name: &str, value: &str| {
            let value = value.trim();
            if !value.is_empty() {
                argv.push(name.to_string());
                argv.push(value.to_string());
            }
        };
        flag("--customer-shortname", &self.customer_shortname);
        flag("--customer-id", &self.customer_id);
        flag("--billing-account-infra", &self.billing_account_infra);
        flag("--default-region", &self.default_region);
        flag("--workload-folder-name", &self.workload_folder_name);
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
        argv
    }
}

/// Whether `dir` is a directory `init` may be run in.
///
/// It must exist — the child is spawned with `dir` as its working directory, and a
/// directory that is not there is an unreadable spawn failure — and it must hold no
/// `config.toml`. `satz init` MERGES the params it is given into an estate that is
/// already there, which is a different act from making one; that door is Open, and
/// this one refuses rather than writing into somebody's estate.
pub fn check_target(dir: &Path) -> Result<(), super::SatzError> {
    if !dir.is_dir() {
        return Err(super::SatzError::TargetMissing(dir.to_path_buf()));
    }
    if dir.join("config.toml").is_file() {
        return Err(super::SatzError::AlreadyAnEstate(dir.to_path_buf()));
    }
    Ok(())
}

/// What an `init` run left in `dir`: the estate directory it wrote, and the estate
/// files in that directory's `yaml_dir`.
///
/// The estate's file NAME is not knowable before the run — `init` names it after the
/// customer id, which it reads from the credentials when the command line states none —
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
            workload_folder_name: "Workloads".to_string(),
            tf_tool: "tofu".to_string(),
            defaults: vec!["google".to_string()],
            providers: vec!["google-beta".to_string(), "random".to_string()],
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
                "--workload-folder-name",
                "Workloads",
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
    /// `--customer-id` asks the credentials for one.
    #[test]
    fn a_blank_or_whitespace_field_is_not_passed_at_all() {
        let options = InitOptions {
            customer_shortname: "  acme  ".to_string(),
            customer_id: "   ".to_string(),
            billing_account_infra: String::new(),
            default_region: String::new(),
            workload_folder_name: " ".to_string(),
            tf_tool: String::new(),
            defaults: vec![String::new(), "  ".to_string()],
            providers: Vec::new(),
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

    #[test]
    fn a_directory_that_already_holds_a_config_is_refused_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("config.toml"), "yaml_dir = \"yaml\"\n").unwrap();
        let err = check_target(tmp.path()).unwrap_err();
        assert!(
            matches!(err, super::super::SatzError::AlreadyAnEstate(_)),
            "{err:?}"
        );
        assert!(err.to_string().contains("open that estate"));
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
}
