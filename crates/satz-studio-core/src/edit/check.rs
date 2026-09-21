//! The two checkers: `satz_transpile_check` over the estate's MCP session, and
//! `satz --config <dir> transpile <file> --check --format json` through the CLI runner.
//! Both answer with satz's `CompileSummary` — the addresses the estate emits and the
//! findings the compile reports as data — so the two read one shape and agree by
//! construction. A refusal is that summary carrying the findings that refused it. A
//! failure that never reached the compile (a file that is not there) carries none, and
//! then its text is what satz said.
//!
//! A finding's `file` is relative to the estate's directory — the one `config.toml` is in,
//! which is what `--config` names — so that directory is what it resolves against, not the
//! directory of the `.satz` file being checked.

use std::path::Path;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{CheckFailure, CheckFuture, Checker};
use crate::diag::{DiagSource, Diagnostic, parse_satz_output};
use crate::satz::reports::{CompileSummary, Finding, Refusal};
use crate::satz::{CliLine, EstateSession, SatzCli, SatzError, ToolOutcome};

/// `satz_transpile_check {estate: <path>}` on the session's `satz mcp`. A refusal's
/// findings are its `structuredContent`; a refusal that carries none is read from the
/// tool's text. A pass returns the summary, the compile's warnings and infos included.
pub struct McpChecker {
    pub session: Arc<EstateSession>,
}

impl Checker for McpChecker {
    fn check<'a>(&'a self, estate: &'a Path) -> CheckFuture<'a> {
        Box::pin(async move {
            let base = self.session.cli.config_dir.clone();
            let estate = absolute_utf8(estate, "satz_transpile_check")?;
            let mut args = serde_json::Map::new();
            args.insert(
                "estate".to_string(),
                serde_json::Value::String(estate.to_string()),
            );
            let outcome = self
                .session
                .tool("satz_transpile_check", args)
                .await
                .map_err(CheckFailure::Failed)?;
            if outcome.is_error {
                return Err(match mcp_refusal(&base, &outcome) {
                    Ok(diags) => CheckFailure::Refused(diags),
                    Err(e) => CheckFailure::Failed(e),
                });
            }
            outcome
                .typed::<CompileSummary>("satz_transpile_check")
                .map_err(CheckFailure::Failed)
        })
    }
}

/// The diagnostics of a refused `satz_transpile_check`: the findings its structured
/// content carries, each at its own line, else what its text said. A structured
/// payload of another shape is an error — satz changed, and guessing would hide it.
fn mcp_refusal(base: &Path, outcome: &ToolOutcome) -> Result<Vec<Diagnostic>, SatzError> {
    let findings = match &outcome.structured {
        Some(value) => {
            serde_json::from_value::<Refusal>(value.clone())
                .map_err(|e| SatzError::Json {
                    command: "satz_transpile_check".to_string(),
                    source: e,
                })?
                .findings
        }
        None => Vec::new(),
    };
    Ok(diagnostics(base, &findings, &outcome.text))
}

/// `satz --config <dir> transpile <path> --check --format json`: the same
/// `CompileSummary` on stdout that the MCP tool returns, exit 0 for a pass and 1 for a
/// refusal. A failure that is no verdict on the estate prints `error: …` on stderr and
/// nothing on stdout.
pub struct CliChecker {
    pub cli: SatzCli,
}

impl Checker for CliChecker {
    fn check<'a>(&'a self, estate: &'a Path) -> CheckFuture<'a> {
        Box::pin(async move {
            let estate_str = absolute_utf8(estate, "transpile --check")?;
            let args = vec![
                "transpile".to_string(),
                estate_str.to_string(),
                "--check".to_string(),
                "--format".to_string(),
                "json".to_string(),
            ];
            let (tx, mut rx) = mpsc::channel::<CliLine>(256);
            let collect = tokio::spawn(async move {
                let mut lines = Vec::new();
                while let Some(line) = rx.recv().await {
                    lines.push(line);
                }
                lines
            });
            let status = self
                .cli
                .run(&args, tx, CancellationToken::new())
                .await
                .map_err(CheckFailure::Failed)?;
            let lines = collect.await.map_err(|e| {
                CheckFailure::Failed(SatzError::Io {
                    context: "collecting the output of `satz transpile --check`".to_string(),
                    source: std::io::Error::other(e),
                })
            })?;
            let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
            for line in lines {
                match line {
                    CliLine::Stdout(s) => stdout.push(s),
                    CliLine::Stderr(s) => stderr.push(s),
                }
            }
            cli_verdict(
                status.success(),
                &stdout.join("\n"),
                &stderr.join("\n"),
                &self.cli.config_dir,
            )
        })
    }
}

/// The verdict of `transpile --check --format json` from its exit and its two streams.
/// A pass is always data: a zero exit whose stdout is not a summary is satz changing
/// shape, and is reported as that rather than read as a pass.
fn cli_verdict(
    success: bool,
    stdout: &str,
    stderr: &str,
    base: &Path,
) -> Result<CompileSummary, CheckFailure> {
    let summary = serde_json::from_str::<CompileSummary>(stdout);
    match (success, summary) {
        (true, Ok(summary)) => Ok(summary),
        (true, Err(e)) => Err(CheckFailure::Failed(SatzError::Json {
            command: "transpile --check --format json".to_string(),
            source: e,
        })),
        (false, Ok(summary)) => Err(CheckFailure::Refused(diagnostics(
            base,
            &summary.findings,
            stderr,
        ))),
        (false, Err(_)) => Err(CheckFailure::Refused(diagnostics(base, &[], stderr))),
    }
}

/// One diagnostic per finding, each at its own file and line; without findings, what
/// satz said in `text`, and when that is empty too, one diagnostic saying so — a refusal
/// is never an empty list.
fn diagnostics(base: &Path, findings: &[Finding], text: &str) -> Vec<Diagnostic> {
    if !findings.is_empty() {
        return findings
            .iter()
            .map(|f| Diagnostic::from_finding(base, f, DiagSource::Check))
            .collect();
    }
    let diags = parse_satz_output(text, DiagSource::Check);
    if diags.is_empty() {
        return vec![Diagnostic::error(
            "satz refused the estate and said nothing about why",
            DiagSource::Check,
        )];
    }
    diags
}

/// The path as the check takes it: absolute, since satz resolves a relative name inside
/// `yaml_dir`, and UTF-8, since the argument is a string.
fn absolute_utf8<'a>(estate: &'a Path, what: &str) -> Result<&'a str, CheckFailure> {
    let refuse = |reason: &str| {
        CheckFailure::Failed(SatzError::Io {
            context: format!("{what} on {}", estate.display()),
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, reason),
        })
    };
    if !estate.is_absolute() {
        return Err(refuse(
            "needs an absolute path — a relative name resolves inside yaml_dir",
        ));
    }
    estate
        .to_str()
        .ok_or_else(|| refuse("not a UTF-8 path, and the argument is a string"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Severity;

    /// the estate's directory — where `config.toml` is, and what `file` is relative to
    const BASE: &str = "/e";

    fn refused(r: Result<CompileSummary, CheckFailure>) -> Vec<Diagnostic> {
        match r {
            Err(CheckFailure::Refused(d)) => d,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    /// recorded from `satz transpile --check --format json` at the pinned release
    const REFUSED: &str = r#"{
      "estate": "/e/yaml/acme.satz",
      "addresses": [],
      "written": [],
      "findings": [
        {"severity": "error", "kind": "missing-required", "group": "required arguments missing:",
         "file": "yaml/acme.satz", "line": 110,
         "message": "google_storage_bucket.state: the provider requires \"location\"",
         "fix": null, "silenced": false, "shared": false},
        {"severity": "info", "kind": "unadopted-pack",
         "message": "`use_budget` is true and this estate has no line for it"}
      ]
    }"#;

    #[test]
    fn a_refusal_is_one_diagnostic_per_finding_resolved_against_the_estate_directory() {
        let d = refused(cli_verdict(false, REFUSED, "", Path::new(BASE)));
        assert_eq!(d.len(), 2, "{d:?}");
        assert_eq!(d[0].severity, Severity::Error);
        assert_eq!(d[0].kind.as_deref(), Some("missing-required"));
        assert_eq!(d[0].file.as_deref(), Some(Path::new("/e/yaml/acme.satz")));
        assert_eq!(d[0].line, Some(110));
        assert_eq!(
            d[0].message,
            "required arguments missing: google_storage_bucket.state: the provider requires \"location\""
        );
        assert_eq!(d[1].severity, Severity::Info);
        assert_eq!((d[1].file.as_deref(), d[1].line), (None, None));
    }

    #[test]
    fn a_pass_is_the_summary_and_a_pass_that_is_not_data_is_a_failure() {
        let ok = r#"{"estate": "/e/yaml/acme.satz", "addresses": ["google_folder.a"], "written": [], "findings": []}"#;
        let s = cli_verdict(true, ok, "", Path::new(BASE)).unwrap();
        assert_eq!(s.addresses, vec!["google_folder.a".to_string()]);
        assert!(matches!(
            cli_verdict(true, "transpile --check: OK", "", Path::new(BASE)),
            Err(CheckFailure::Failed(SatzError::Json { .. }))
        ));
    }

    #[test]
    fn a_failure_before_the_compile_is_what_stderr_said() {
        let stderr =
            "satz v0.73.0 (built now)\nerror: /e/yaml/acme.satz: no such file or directory";
        let d = refused(cli_verdict(false, "", stderr, Path::new(BASE)));
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].severity, Severity::Error);
        assert!(d[0].message.contains("no such file"), "{d:?}");

        let d = refused(cli_verdict(false, "", "", Path::new(BASE)));
        assert_eq!(d.len(), 1, "a refusal is never an empty list");
    }

    #[test]
    fn the_mcp_refusal_reads_its_structured_findings_and_falls_to_the_text_without_them() {
        let outcome = |structured: Option<serde_json::Value>| ToolOutcome {
            structured,
            text: "transpile --check: /e/yaml/acme.satz: file not found".to_string(),
            is_error: true,
        };
        let structured: serde_json::Value = serde_json::from_str(REFUSED).unwrap();
        let d = mcp_refusal(Path::new(BASE), &outcome(Some(structured))).unwrap();
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].file.as_deref(), Some(Path::new("/e/yaml/acme.satz")));
        assert_eq!(d[0].kind.as_deref(), Some("missing-required"));

        // a refusal before the compile carries none: the text is what satz said
        let d = mcp_refusal(Path::new(BASE), &outcome(None)).unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].message, "/e/yaml/acme.satz: file not found");
        assert_eq!(d[0].kind, None);

        // a structured payload of another shape is an error, never a guess
        let bad =
            serde_json::json!({"findings": [{"severity": "loud", "kind": "k", "message": "m"}]});
        assert!(mcp_refusal(Path::new(BASE), &outcome(Some(bad))).is_err());
    }
}
