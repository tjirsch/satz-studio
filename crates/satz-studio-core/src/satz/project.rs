//! The interface plane through the CLI: `satz interfaces`, what the estate publishes to
//! the projects that read it, which has no MCP tool and runs as a command (ADR 0023) —
//! and how a satz command that writes the estate file ends, as the call of a delegated
//! write reads it ([`outcome`], [`crate::edit::delegated_write`]).

use std::path::Path;
use std::process::ExitStatus;

use super::reports::InterfacesReport;
use super::{SatzCli, SatzError, ToolOutcome};

/// `satz --config <dir> interfaces <estate> --format json --out <file>`, typed. It
/// compiles the estate, so an estate satz refuses is an error carrying satz's stderr.
pub async fn interfaces(cli: &SatzCli, estate: &Path) -> Result<InterfacesReport, SatzError> {
    cli.json_report(&["interfaces".to_string(), estate.display().to_string()])
        .await
}

/// The process result of a satz command that writes the estate file, as the call of a
/// delegated write reads it: a zero exit is a write that landed, its text what satz
/// printed on stdout, trimmed; a non-zero exit is satz's refusal, `is_error` with satz's
/// sentence — stderr without the version banner and the `error: ` in front — or, when
/// satz said nothing, the exit status under `command`'s name.
pub fn outcome(command: &str, status: ExitStatus, stdout: &str, stderr: &str) -> ToolOutcome {
    if status.success() {
        return ToolOutcome {
            structured: None,
            text: stdout.trim().to_string(),
            is_error: false,
        };
    }
    let said = sentence(stderr);
    ToolOutcome {
        structured: None,
        text: if said.is_empty() {
            format!("{command} exited with {status} and said nothing")
        } else {
            said
        },
        is_error: true,
    }
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

    /// An exit status of 1, as the platform encodes one.
    fn failed() -> ExitStatus {
        #[cfg(unix)]
        {
            std::os::unix::process::ExitStatusExt::from_raw(1 << 8)
        }
        #[cfg(windows)]
        {
            std::os::windows::process::ExitStatusExt::from_raw(1)
        }
    }

    #[test]
    fn a_zero_exit_is_what_satz_printed_and_a_non_zero_one_is_its_sentence_or_the_status() {
        let landed = outcome(
            "satz adopt",
            ExitStatus::default(),
            "adopt: acknowledged the notice\nadopt: 2 \"import-id\"(s) written.\n",
            "satz v0.86.5 (built 2026-09-26 17:31:04)\n",
        );
        assert!(!landed.is_error);
        assert_eq!(
            landed.text,
            "adopt: acknowledged the notice\nadopt: 2 \"import-id\"(s) written."
        );
        let refused = outcome(
            "satz adopt",
            failed(),
            "",
            "satz v0.86.5 (built 2026-09-26 17:31:04)\nerror: 2 candidates: a, b — pin \"import-id\" by hand\n",
        );
        assert!(refused.is_error);
        assert_eq!(
            refused.text,
            "2 candidates: a, b — pin \"import-id\" by hand"
        );
        let silent = outcome("satz adopt", failed(), "", "");
        assert!(silent.is_error);
        assert!(
            silent.text.starts_with("satz adopt exited with "),
            "{}",
            silent.text
        );
        assert!(silent.text.ends_with("and said nothing"), "{}", silent.text);
    }

    #[test]
    fn a_refusal_reads_as_satzs_sentence() {
        let stderr = "satz v0.90.1 (built 2026-09-30 21:00:00)\nerror: acme.satz: nothing changed.\n\nline 12 declares `interface \"archive\"` already\n";
        assert_eq!(
            sentence(stderr),
            "acme.satz: nothing changed.\n\nline 12 declares `interface \"archive\"` already"
        );
    }
}
