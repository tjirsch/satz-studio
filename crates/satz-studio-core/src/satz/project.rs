//! The interface plane through the CLI: `satz interfaces`, what the estate publishes to
//! the projects that read it, and `satz add-project`, satz's own writer of a project's
//! section or of an interface alone. Neither has an MCP tool, so both run as commands
//! (ADR 0023); the write still goes through the delegated-write discipline
//! ([`crate::edit::delegated_write`]), with [`add_project`] as its call.

use std::path::Path;

use super::reports::InterfacesReport;
use super::{SatzCli, SatzError, ToolOutcome};

/// The command as the diagnostics and the toasts name it.
pub const ADD_PROJECT: &str = "satz add-project";

/// The two names `interfaces/` reserves: `interfaces/core/` and `interfaces/common/` are
/// satz's (`satz_core::satz::CORE_INTERFACE`, `COMMON_LIBRARY`).
const RESERVED: [&str; 2] = ["core", "common"];

/// `satz --config <dir> interfaces <estate> --format json --out <file>`, typed. It
/// compiles the estate, so an estate satz refuses is an error carrying satz's stderr.
pub async fn interfaces(cli: &SatzCli, estate: &Path) -> Result<InterfacesReport, SatzError> {
    cli.json_report(&["interfaces".to_string(), estate.display().to_string()])
        .await
}

/// The arguments of `satz add-project`, as the wizard collects them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AddProjectArgs {
    /// the project's name, which is the interface's name and a folder name
    pub name: String,
    /// the group that reads the project, `<name>@<domain>`; not with `interface_only`
    pub owner_group: Option<String>,
    /// the interface block alone, for a workload that brings its own Google project
    pub interface_only: bool,
    /// the interfaces the new one also carries (`--use-interface`)
    pub uses: Vec<String>,
    /// other interfaces' exports written into the new one, `<interface>.<name>`
    /// (`--export`)
    pub exports: Vec<String>,
}

impl AddProjectArgs {
    /// `add-project <estate> --name <n> [--interface-only | --owner-group <g>]
    /// [--use-interface <i>]… [--export <i>.<n>]…`, what is set and nothing else.
    pub fn argv(&self, estate: &Path) -> Vec<String> {
        let mut argv = vec![
            "add-project".to_string(),
            estate.display().to_string(),
            "--name".to_string(),
            self.name.clone(),
        ];
        if self.interface_only {
            argv.push("--interface-only".to_string());
        } else if let Some(group) = &self.owner_group {
            argv.extend(["--owner-group".to_string(), group.clone()]);
        }
        for u in &self.uses {
            argv.extend(["--use-interface".to_string(), u.clone()]);
        }
        for x in &self.exports {
            argv.extend(["--export".to_string(), x.clone()]);
        }
        argv
    }

    /// What satz would refuse before it reads the estate, in satz's words, so the form
    /// can say it before anything runs: the name, the owner group of a project, and an
    /// interface-only block that would carry the core exports alone. A name the estate
    /// declares already is satz's to refuse, since only the file knows it.
    pub fn problem(&self) -> Option<String> {
        if let Err(e) = valid_name(&self.name) {
            return Some(e);
        }
        if self.interface_only {
            if self.owner_group.is_some() {
                return Some(
                    "an interface alone has no owner group — `--owner-group` onboards a project"
                        .to_string(),
                );
            }
            if self.uses.is_empty() && self.exports.is_empty() {
                return Some(format!(
                    "interface \"{}\" would carry the core exports alone, which `interfaces/common/core/` already is — pick an interface to use or an export to carry",
                    self.name
                ));
            }
            return None;
        }
        match &self.owner_group {
            None => Some("a project needs its owner group, `<name>@<domain>`".to_string()),
            Some(g) => owner_group_problem(g),
        }
    }
}

/// satz's rule for a project's name: an interface name and a folder name,
/// `[a-z][a-z0-9-]*`, and neither of the two folders `interfaces/` reserves.
pub fn valid_name(name: &str) -> Result<(), String> {
    let ok = name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !ok {
        return Err(format!(
            "`{name}`: a project's name is an interface name — lowercase letters, digits and `-`, starting with a letter"
        ));
    }
    if RESERVED.contains(&name) {
        return Err(format!(
            "`{name}` is reserved: interfaces/{name}/ is satz's"
        ));
    }
    Ok(())
}

/// satz's rule for `--owner-group`: the group's address, not an IAM member string.
fn owner_group_problem(group: &str) -> Option<String> {
    if !group.contains('@') || group.starts_with("group:") {
        return Some(format!(
            "`--owner-group {group}`: the group's address, `<name>@<domain>`"
        ));
    }
    None
}

/// `satz --config <dir> add-project …` run to its end, as the call of a delegated write:
/// a zero exit is a write that landed, its text the line satz ends on; a non-zero exit is
/// satz's refusal, `is_error` with satz's sentence — stderr without the version banner
/// and the `error: ` in front. The write is satz's on the real file; the discipline
/// around it is the caller's.
pub async fn add_project(
    cli: &SatzCli,
    estate: &Path,
    args: &AddProjectArgs,
) -> Result<ToolOutcome, SatzError> {
    let (status, output) = cli.finished(&args.argv(estate)).await?;
    Ok(if status.success() {
        ToolOutcome {
            structured: None,
            text: output.stdout.trim().to_string(),
            is_error: false,
        }
    } else {
        let said = sentence(&output.stderr);
        ToolOutcome {
            structured: None,
            text: if said.is_empty() {
                format!("satz add-project exited with {status} and said nothing")
            } else {
                said
            },
            is_error: true,
        }
    })
}

/// What a failed `satz interfaces` says to the operator: satz's own stderr as
/// [`sentence`] reads it when satz ran and refused, the error itself otherwise.
pub fn said(e: &SatzError) -> String {
    match e {
        SatzError::Exit { stderr, .. } if !sentence(stderr).is_empty() => sentence(stderr),
        other => other.to_string(),
    }
}

/// satz's stderr as the operator reads it: the banner dropped, `error: ` taken off.
pub fn sentence(stderr: &str) -> String {
    let text = stderr
        .lines()
        .filter(|l| !crate::diag::is_banner(l))
        .collect::<Vec<_>>()
        .join("\n");
    let text = text.trim();
    text.strip_prefix("error: ").unwrap_or(text).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_renders_its_owner_group_and_the_choices_in_order() {
        let args = AddProjectArgs {
            name: "payments".to_string(),
            owner_group: Some("payments-owners@example.com".to_string()),
            uses: vec!["audit".to_string()],
            exports: vec!["archive.archive_project_id".to_string()],
            ..Default::default()
        };
        assert_eq!(
            args.argv(Path::new("satz/acme.satz")),
            [
                "add-project",
                "satz/acme.satz",
                "--name",
                "payments",
                "--owner-group",
                "payments-owners@example.com",
                "--use-interface",
                "audit",
                "--export",
                "archive.archive_project_id",
            ]
        );
        assert_eq!(args.problem(), None);
    }

    #[test]
    fn an_interface_alone_carries_no_owner_group() {
        let args = AddProjectArgs {
            name: "reports".to_string(),
            interface_only: true,
            uses: vec!["audit".to_string()],
            ..Default::default()
        };
        assert_eq!(
            args.argv(Path::new("e.satz")),
            [
                "add-project",
                "e.satz",
                "--name",
                "reports",
                "--interface-only",
                "--use-interface",
                "audit"
            ]
        );
        assert_eq!(args.problem(), None);
        let with_group = AddProjectArgs {
            owner_group: Some("g@example.com".to_string()),
            ..args
        };
        assert!(with_group.problem().unwrap().contains("no owner group"));
    }

    #[test]
    fn an_interface_alone_needs_something_to_carry() {
        let args = AddProjectArgs {
            name: "reports".to_string(),
            interface_only: true,
            ..Default::default()
        };
        assert!(args.problem().unwrap().contains("core exports alone"));
        let export = AddProjectArgs {
            exports: vec!["archive.archive_project_number".to_string()],
            ..args
        };
        assert_eq!(export.problem(), None);
    }

    #[test]
    fn a_project_needs_a_group_address() {
        let mut args = AddProjectArgs {
            name: "payments".to_string(),
            ..Default::default()
        };
        assert!(args.problem().unwrap().contains("owner group"));
        for bad in ["payments-owners", "group:payments-owners@example.com"] {
            args.owner_group = Some(bad.to_string());
            assert!(
                args.problem().unwrap().contains("the group's address"),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_name_is_an_interface_name_and_not_a_reserved_folder() {
        for good in ["payments", "a", "team-2", "x1-y"] {
            assert_eq!(valid_name(good), Ok(()), "{good}");
        }
        for bad in ["", "Payments", "2team", "-x", "team_2", "pay ments", "é"] {
            assert!(
                valid_name(bad).unwrap_err().contains("lowercase letters"),
                "{bad}"
            );
        }
        for reserved in ["core", "common"] {
            assert!(valid_name(reserved).unwrap_err().contains("reserved"));
        }
    }

    #[test]
    fn a_refusal_reads_as_satzs_sentence() {
        let stderr = "satz v0.86.1 (built 2026-09-26 11:53:21)\nerror: add-project archive: nothing changed.\n\nline 265 declares `interface \"archive\"` already — one project is one interface\n";
        assert_eq!(
            sentence(stderr),
            "add-project archive: nothing changed.\n\nline 265 declares `interface \"archive\"` already — one project is one interface"
        );
    }
}
