//! `satz scan <estate> --format json`: Checkov over the HCL code in the estate's
//! `hcl_dir`, each failed check with the Satz file and line that declared its resource.
//!
//! satz runs Checkov and joins its findings to the estate; the app starts the command
//! and reads the report. The scan reads the files a transpile wrote, so it says nothing
//! about an edit that has not been transpiled, and it reads the estate's own code only:
//! no sign-in, no catalog, nothing of the organization beyond what the estate declares
//! ([ADR 0035](../../../../docs/adr/0035-security-is-a-destination-of-hints-on-the-estates-own-resources.md)).

use std::path::Path;

use super::reports::ScanReport;
use super::{SatzCli, SatzError};

/// The command line after `satz`, without the format the reader appends.
pub fn args(estate: &Path) -> Vec<String> {
    vec!["scan".to_string(), estate.display().to_string()]
}

/// Run the scan and type its report. satz exits non-zero exactly when a check failed
/// and prints the report either way; an exit that disagrees with the report's own count
/// is an error, and so is a refusal — no Checkov on this machine, no HCL code written
/// yet — which carries satz's words.
pub async fn scan(cli: &SatzCli, estate: &Path) -> Result<ScanReport, SatzError> {
    let (report, status): (ScanReport, _) = cli.json_stdout(&args(estate)).await?;
    if status.success() == (report.failed == 0) {
        Ok(report)
    } else {
        Err(SatzError::Verdict {
            command: "scan".to_string(),
            status,
            report: format!("it counts {} failed checks", report.failed),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scan_names_the_estate_and_leaves_the_format_to_the_reader() {
        assert_eq!(
            args(Path::new("/e/C0example.satz")),
            ["scan", "/e/C0example.satz"]
        );
    }
}
