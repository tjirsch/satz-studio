//! The two checkers: `satz_transpile_check` over the estate's MCP session, and
//! `satz --config <dir> transpile <file> --check` through the CLI runner. The verdict
//! and the diagnostics are what they share; the summary is not — the CLI prints no
//! address list and no findings, so [`CliChecker`] returns empty ones.
//!
//! What the compile finds after the front end is a list satz reports as data, and both
//! checkers read that list rather than the sentences it renders to: the MCP session
//! gets it as JSON, the CLI as the `Debug` of the `CompileRefusal` it exits on. A
//! refusal that never reached the compile — a missing file — has no findings, and its
//! text is read as satz's output.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::{CheckFailure, CheckFuture, Checker};
use crate::diag::{DiagSource, Diagnostic, Severity, parse_satz_output};
use crate::satz::reports::{CompileSummary, Finding, FindingSeverity, Refusal};
use crate::satz::{CliLine, EstateSession, SatzCli, SatzError, ToolOutcome};

/// `satz_transpile_check {estate: <path>}` on the session's `satz mcp`. A refusal's
/// findings are its `structuredContent`; a refusal that carries none is read from the
/// tool's text (`transpile --check: file:line: msg`). A pass returns the summary, the
/// warnings and notes of the compile included.
pub struct McpChecker {
    pub session: Arc<EstateSession>,
}

impl Checker for McpChecker {
    fn check<'a>(&'a self, estate: &'a Path) -> CheckFuture<'a> {
        Box::pin(async move {
            let base = parent_of(estate);
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
    if findings.is_empty() {
        return Ok(parse_satz_output(&outcome.text, DiagSource::Check));
    }
    Ok(findings
        .iter()
        .map(|f| Diagnostic::from_finding(base, f, DiagSource::Check))
        .collect())
}

/// `satz --config <dir> transpile <path> --check`. Exit 0 is a pass with an empty
/// address list and no findings — the CLI prints neither as data. A non-zero exit is a
/// refusal whose diagnostics are decoded from the final `Error: ` line satz prints —
/// a `CompileRefusal { message, findings }` rendered with `Debug` becomes one
/// diagnostic per finding, a `PipelineError { file, line, msg }` one located
/// diagnostic, any other payload one diagnostic without a location — and, when no such
/// line exists, from everything stderr said.
pub struct CliChecker {
    pub cli: SatzCli,
}

impl Checker for CliChecker {
    fn check<'a>(&'a self, estate: &'a Path) -> CheckFuture<'a> {
        Box::pin(async move {
            let base = parent_of(estate);
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
                    findings: Vec::new(),
                });
            }
            let stderr: Vec<String> = lines
                .into_iter()
                .filter_map(|l| match l {
                    CliLine::Stderr(s) => Some(s),
                    CliLine::Stdout(_) => None,
                })
                .collect();
            Err(CheckFailure::Refused(refusal(&stderr, &base)))
        })
    }
}

/// The directory a finding's relative file resolves against: the estate's own.
fn parent_of(estate: &Path) -> PathBuf {
    estate.parent().unwrap_or(Path::new(".")).to_path_buf()
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
fn refusal(stderr: &[String], base: &Path) -> Vec<Diagnostic> {
    let Some(payload) = stderr.iter().rev().find_map(|l| l.strip_prefix("Error: ")) else {
        return parse_satz_output(&stderr.join("\n"), DiagSource::Check);
    };
    if let Some(findings) = compile_refusal(payload)
        && !findings.is_empty()
    {
        return findings
            .iter()
            .map(|f| Diagnostic::from_finding(base, f, DiagSource::Check))
            .collect();
    }
    if let Some(d) = pipeline_error(payload) {
        return vec![d];
    }
    let message = debug_string(payload)
        .filter(|(_, rest)| rest.is_empty())
        .map_or_else(|| payload.to_string(), |(s, _)| s);
    // A front-end refusal satz renders itself — one that carries a hint beside the
    // parser's sentence — is a plain `<file>:<line>: message`. Read that location, or
    // the drawer cannot point at the line the estate is wrong on.
    let mut diags = parse_satz_output(&message, DiagSource::Check);
    if diags.is_empty() {
        return vec![Diagnostic::error(message, DiagSource::Check)];
    }
    for d in &mut diags {
        if let Some(file) = &d.file
            && file.is_relative()
        {
            d.file = Some(base.join(file));
        }
    }
    diags
}

/// `CompileRefusal { message: "…", findings: [Finding { … }, …] }` as `Debug` renders
/// it: the same findings the MCP session gets as JSON, which is why the message — the
/// text those findings render to — is read past.
fn compile_refusal(payload: &str) -> Option<Vec<Finding>> {
    let rest = payload.strip_prefix("CompileRefusal { message: ")?;
    let (_, rest) = debug_string(rest)?;
    let mut rest = rest.strip_prefix(", findings: [")?;
    let mut findings = Vec::new();
    loop {
        if let Some(end) = rest.strip_prefix("] }") {
            return end.is_empty().then_some(findings);
        }
        if !findings.is_empty() {
            rest = rest.strip_prefix(", ")?;
        }
        let (finding, after) = debug_finding(rest)?;
        findings.push(finding);
        rest = after;
    }
}

/// One `Finding { severity: Error, kind: UnadoptedPack, group: Some("…"), file: None,
/// line: Some(12), message: "…" }`, and what follows its closing brace. The fields are
/// in declaration order, which is the order `Debug` prints them.
fn debug_finding(s: &str) -> Option<(Finding, &str)> {
    let rest = s.strip_prefix("Finding { severity: ")?;
    let (severity, rest) = debug_word(rest, ", kind: ")?;
    let severity = match severity {
        "Error" => FindingSeverity::Error,
        "Warning" => FindingSeverity::Warning,
        "Note" => FindingSeverity::Note,
        _ => return None,
    };
    let (kind, rest) = debug_word(rest, ", group: ")?;
    let (group, rest) = debug_option_string(rest, ", file: ")?;
    let (file, rest) = debug_option_string(rest, ", line: ")?;
    let (line, rest) = debug_option_line(rest, ", message: ")?;
    let (message, rest) = debug_string(rest)?;
    let rest = rest.strip_prefix(" }")?;
    Some((
        Finding {
            severity,
            kind: kebab_case(kind),
            group,
            file,
            line,
            message,
        },
        rest,
    ))
}

/// The bare word `s` starts with, followed by `until` — a `Debug`-printed enum variant.
fn debug_word<'a>(s: &'a str, until: &str) -> Option<(&'a str, &'a str)> {
    let end = s
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    Some((&s[..end], s[end..].strip_prefix(until)?))
}

/// `Some("…")` or `None`, up to `until`.
fn debug_option_string<'a>(s: &'a str, until: &str) -> Option<(Option<String>, &'a str)> {
    if let Some(rest) = s.strip_prefix("None") {
        return Some((None, rest.strip_prefix(until)?));
    }
    let (value, rest) = debug_string(s.strip_prefix("Some(")?)?;
    Some((Some(value), rest.strip_prefix(')')?.strip_prefix(until)?))
}

/// `Some(12)` or `None`, up to `until`.
fn debug_option_line<'a>(s: &'a str, until: &str) -> Option<(Option<u32>, &'a str)> {
    if let Some(rest) = s.strip_prefix("None") {
        return Some((None, rest.strip_prefix(until)?));
    }
    let rest = s.strip_prefix("Some(")?;
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let line: u32 = rest[..digits].parse().ok()?;
    Some((
        Some(line),
        rest[digits..].strip_prefix(')')?.strip_prefix(until)?,
    ))
}

/// A `Debug`-printed variant name as serde's `rename_all = "kebab-case"` writes it:
/// `UnadoptedPack` → `unadopted-pack`. The MCP session reads the same kinds off the
/// wire already spelled this way.
fn kebab_case(variant: &str) -> String {
    let mut out = String::with_capacity(variant.len() + 2);
    for (i, c) in variant.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
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
        kind: None,
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

    const BASE: &str = "/e/yaml";

    #[test]
    fn a_pipeline_error_rendered_with_debug_becomes_one_located_diagnostic() {
        let stderr = [
            "satz v0.56.1 (built 2026-09-13 13:56:42)".to_string(),
            "Loaded 45 resource types from schema file 'google.json'".to_string(),
            r#"Error: PipelineError { file: "/e/yaml/acme.studio-tmp.satz", line: 143, msg: "unknown param 'nobody'" }"#.to_string(),
        ];
        let d = refusal(&stderr, Path::new(BASE));
        assert_eq!(d.len(), 1);
        assert_eq!(
            d[0].file.as_deref(),
            Some(Path::new("/e/yaml/acme.studio-tmp.satz"))
        );
        assert_eq!(d[0].line, Some(143));
        assert_eq!(d[0].message, "unknown param 'nobody'");
        assert_eq!(d[0].severity, Severity::Error);
        assert_eq!(d[0].kind, None);
        assert_eq!(d[0].source, DiagSource::Check);
    }

    /// satz renders a front-end refusal itself where it has something to add to the
    /// parser's sentence, and then the error is a plain string: the location in front
    /// of it is what the drawer points at.
    #[test]
    fn a_front_end_refusal_satz_rendered_itself_keeps_its_file_and_line() {
        let stderr = [
            "satz v0.67.0 (built 2026-09-20 05:23:53)".to_string(),
            concat!(
                r#"Error: "/e/yaml/acme.satz:24: unknown param 'nobody' — the pack graph: "#,
                r#"`presets/cis/CIS-GCP-Foundation-4.0.satz` needs `presets/estate-map.satz`, which is off""#,
            )
            .to_string(),
        ];
        let d = refusal(&stderr, Path::new(BASE));
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].file.as_deref(), Some(Path::new("/e/yaml/acme.satz")));
        assert_eq!(d[0].line, Some(24));
        assert!(d[0].message.starts_with("unknown param 'nobody'"), "{d:?}");
        assert_eq!(d[0].severity, Severity::Error);
        assert_eq!(d[0].source, DiagSource::Check);
    }

    /// The same refusal with a relative file, as satz names one it loaded by a `use`
    /// path: resolved against the estate's own directory.
    #[test]
    fn a_relative_file_in_a_rendered_refusal_resolves_against_the_estate() {
        let stderr = [r#"Error: "acme.satz:7: unknown param 'nobody'""#.to_string()];
        let d = refusal(&stderr, Path::new(BASE));
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].file.as_deref(), Some(Path::new("/e/yaml/acme.satz")));
        assert_eq!(d[0].line, Some(7));
    }

    #[test]
    fn a_compile_refusal_rendered_with_debug_becomes_one_diagnostic_per_finding() {
        let stderr = [
            "satz v0.56.14 (built 2026-09-14 16:04:19)".to_string(),
            concat!(
                r#"Error: CompileRefusal { message: "whatever the CLI prints", findings: ["#,
                r#"Finding { severity: Error, kind: MissingRequired, group: Some("required arguments missing:"), "#,
                r#"file: Some("/e/yaml/acme.satz"), line: Some(110), message: "google_storage_bucket.state: the provider requires \"location\"" }, "#,
                r#"Finding { severity: Warning, kind: UnadoptedPack, group: None, file: None, line: None, "#,
                r#"message: "`use_budget` is true and this estate has no line for it" }] }"#,
            )
            .to_string(),
        ];
        let d = refusal(&stderr, Path::new(BASE));
        assert_eq!(d.len(), 2, "{d:?}");
        assert_eq!(d[0].severity, Severity::Error);
        assert_eq!(d[0].kind.as_deref(), Some("missing-required"));
        assert_eq!(d[0].file.as_deref(), Some(Path::new("/e/yaml/acme.satz")));
        assert_eq!(d[0].line, Some(110));
        assert_eq!(
            d[0].message,
            "required arguments missing: google_storage_bucket.state: the provider requires \"location\""
        );
        assert_eq!(d[1].severity, Severity::Warning);
        assert_eq!(d[1].kind.as_deref(), Some("unadopted-pack"));
        assert_eq!((d[1].file.as_deref(), d[1].line), (None, None));
        assert_eq!(
            d[1].message,
            "`use_budget` is true and this estate has no line for it"
        );
    }

    #[test]
    fn a_relative_file_in_a_finding_resolves_against_the_estates_directory() {
        let stderr = [concat!(
            r#"Error: CompileRefusal { message: "m", findings: [Finding { severity: Error, "#,
            r#"kind: DryRunConflict, group: None, file: Some("presets/cis-4.0.satz"), line: Some(4), message: "m" }] }"#,
        )
        .to_string()];
        let d = refusal(&stderr, Path::new(BASE));
        assert_eq!(
            d[0].file.as_deref(),
            Some(Path::new("/e/yaml/presets/cis-4.0.satz"))
        );
        assert_eq!(d[0].kind.as_deref(), Some("dry-run-conflict"));
    }

    #[test]
    fn a_refusal_shape_the_parser_does_not_know_is_one_diagnostic_with_the_payload() {
        let payload = "CompileRefusal { message: \"m\", findings: [Finding { severity: Loud }] }";
        assert!(compile_refusal(payload).is_none());
        let d = refusal(&[format!("Error: {payload}")], Path::new(BASE));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].message, payload);
        assert_eq!(d[0].kind, None);
    }

    #[test]
    fn the_mcp_refusal_reads_its_structured_findings_and_falls_to_the_text_without_them() {
        let outcome = |structured: Option<serde_json::Value>| ToolOutcome {
            structured,
            text: "transpile --check: /e/yaml/acme.satz: file not found".to_string(),
            is_error: true,
        };
        // recorded from `satz mcp` at the pinned release
        let structured = serde_json::json!({
            "addresses": [],
            "estate": "/e/yaml/acme.satz",
            "findings": [{
                "file": "/e/yaml/acme.satz",
                "group": "required arguments missing:",
                "kind": "missing-required",
                "line": 109,
                "message": "google_storage_bucket.state (/e/yaml/acme.satz:109): the provider requires location",
                "severity": "error"
            }]
        });
        let d = mcp_refusal(Path::new(BASE), &outcome(Some(structured))).unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].file.as_deref(), Some(Path::new("/e/yaml/acme.satz")));
        assert_eq!(d[0].line, Some(109));
        assert_eq!(d[0].kind.as_deref(), Some("missing-required"));
        assert_eq!(d[0].severity, Severity::Error);
        assert!(d[0].message.starts_with("required arguments missing: "));

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
        let d = refusal(
            &["Error: \"YAML-dialect estate: convert it first\"".to_string()],
            Path::new(BASE),
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].message, "YAML-dialect estate: convert it first");
        assert_eq!(d[0].line, None);
        let d = refusal(
            &["Error: Os { code: 2, kind: NotFound }".to_string()],
            Path::new(BASE),
        );
        assert_eq!(d[0].message, "Os { code: 2, kind: NotFound }");
    }

    #[test]
    fn without_an_error_line_everything_stderr_said_is_kept() {
        let d = refusal(
            &[
                "satz v0.56.1 (built now)".to_string(),
                "thread 'main' panicked at x".to_string(),
            ],
            Path::new(BASE),
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].message, "thread 'main' panicked at x");
    }
}
