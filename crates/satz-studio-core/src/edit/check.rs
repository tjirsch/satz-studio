//! The two checkers: `satz_transpile_check` over the estate's MCP session, and
//! `satz --config <dir> transpile <file> --check` through the CLI runner. The verdict
//! and the diagnostics are what they share; the summary is not — the CLI prints no
//! address list, so [`CliChecker`] returns an empty one.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{CheckFailure, CheckFuture, Checker};
use crate::diag::{DiagSource, Diagnostic, Severity, parse_satz_output};
use crate::satz::reports::CompileSummary;
use crate::satz::{CliLine, EstateSession, SatzCli, SatzError};

/// `satz_transpile_check {estate: <path>}` on the session's `satz mcp`. A refusal is
/// parsed from the tool's text (`transpile --check: file:line: msg`).
pub struct McpChecker {
    pub session: Arc<EstateSession>,
}

impl Checker for McpChecker {
    fn check<'a>(&'a self, estate: &'a Path) -> CheckFuture<'a> {
        Box::pin(async move {
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
                return Err(CheckFailure::Refused(parse_satz_output(
                    &outcome.text,
                    DiagSource::Check,
                )));
            }
            outcome
                .typed::<CompileSummary>("satz_transpile_check")
                .map_err(CheckFailure::Failed)
        })
    }
}

/// `satz --config <dir> transpile <path> --check`. Exit 0 is a pass with an empty
/// address list. A non-zero exit is a refusal whose diagnostics are decoded from the
/// final `Error: ` line satz prints — a `PipelineError { file, line, msg }` rendered
/// with `Debug` becomes one located diagnostic, any other payload one diagnostic
/// without a location — and, when no such line exists, from everything stderr said.
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
            if status.success() {
                return Ok(CompileSummary {
                    estate: estate.display().to_string(),
                    addresses: Vec::new(),
                    written: Vec::new(),
                });
            }
            let stderr: Vec<String> = lines
                .into_iter()
                .filter_map(|l| match l {
                    CliLine::Stderr(s) => Some(s),
                    CliLine::Stdout(_) => None,
                })
                .collect();
            Err(CheckFailure::Refused(refusal(&stderr)))
        })
    }
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

/// The diagnostics of a failed CLI check, from its stderr lines.
fn refusal(stderr: &[String]) -> Vec<Diagnostic> {
    let Some(payload) = stderr.iter().rev().find_map(|l| l.strip_prefix("Error: ")) else {
        return parse_satz_output(&stderr.join("\n"), DiagSource::Check);
    };
    if let Some(d) = pipeline_error(payload) {
        return vec![d];
    }
    let message = debug_string(payload)
        .filter(|(_, rest)| rest.is_empty())
        .map_or_else(|| payload.to_string(), |(s, _)| s);
    vec![Diagnostic::error(message, DiagSource::Check)]
}

/// `PipelineError { file: "…", line: N, msg: "…" }` as `Debug` renders it.
fn pipeline_error(payload: &str) -> Option<Diagnostic> {
    let rest = payload.strip_prefix("PipelineError { file: ")?;
    let (file, rest) = debug_string(rest)?;
    let rest = rest.strip_prefix(", line: ")?;
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let line: u32 = rest[..digits].parse().ok()?;
    let rest = rest[digits..].strip_prefix(", msg: ")?;
    let (message, rest) = debug_string(rest)?;
    if rest != " }" {
        return None;
    }
    Some(Diagnostic {
        file: Some(PathBuf::from(file)),
        line: Some(line),
        severity: Severity::Error,
        message,
        source: DiagSource::Check,
    })
}

/// A string as `Debug` prints it — `"…"` with `\"`, `\\`, `\n`, `\r`, `\t`, `\'`, `\0`
/// and `\u{…}` escapes — decoded, and what follows its closing quote.
fn debug_string(s: &str) -> Option<(String, &str)> {
    let body = s.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = body.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Some((out, &body[i + 1..])),
            '\\' => {
                let (_, e) = chars.next()?;
                match e {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    '0' => out.push('\0'),
                    '\\' | '"' | '\'' => out.push(e),
                    'u' => {
                        if chars.next()?.1 != '{' {
                            return None;
                        }
                        let mut hex = String::new();
                        loop {
                            let (_, h) = chars.next()?;
                            if h == '}' {
                                break;
                            }
                            hex.push(h);
                        }
                        out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                    }
                    _ => return None,
                }
            }
            c => out.push(c),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pipeline_error_rendered_with_debug_becomes_one_located_diagnostic() {
        let stderr = [
            "satz v0.56.1 (built 2026-09-13 13:56:42)".to_string(),
            "Loaded 45 resource types from schema file 'google.json'".to_string(),
            r#"Error: PipelineError { file: "/e/yaml/acme.studio-tmp.satz", line: 143, msg: "unknown param 'nobody'" }"#.to_string(),
        ];
        let d = refusal(&stderr);
        assert_eq!(d.len(), 1);
        assert_eq!(
            d[0].file.as_deref(),
            Some(Path::new("/e/yaml/acme.studio-tmp.satz"))
        );
        assert_eq!(d[0].line, Some(143));
        assert_eq!(d[0].message, "unknown param 'nobody'");
        assert_eq!(d[0].severity, Severity::Error);
        assert_eq!(d[0].source, DiagSource::Check);
    }

    #[test]
    fn debug_escapes_are_decoded() {
        let (s, rest) = debug_string(r#""say \"hi\" \\ \n \u{e9}" tail"#).unwrap();
        assert_eq!(s, "say \"hi\" \\ \n é");
        assert_eq!(rest, " tail");
        assert!(debug_string("\"open").is_none());
        assert!(debug_string("bare").is_none());
    }

    #[test]
    fn another_error_payload_is_one_diagnostic_without_a_location() {
        let d = refusal(&["Error: \"YAML-dialect estate: convert it first\"".to_string()]);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].message, "YAML-dialect estate: convert it first");
        assert_eq!(d[0].line, None);
        let d = refusal(&["Error: Os { code: 2, kind: NotFound }".to_string()]);
        assert_eq!(d[0].message, "Os { code: 2, kind: NotFound }");
    }

    #[test]
    fn without_an_error_line_everything_stderr_said_is_kept() {
        let d = refusal(&[
            "satz v0.56.1 (built now)".to_string(),
            "thread 'main' panicked at x".to_string(),
        ]);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].message, "thread 'main' panicked at x");
    }
}
