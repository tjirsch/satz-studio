//! One diagnostic type for everything the app can point at a line: satz-core's parse
//! and pipeline errors, satz's own stderr, a tool refusal over MCP, the document
//! layer's own findings. Line granularity — that is what satz records (a node carries
//! its line and nothing finer).

use std::path::{Path, PathBuf};

use satz_core::pipeline::PipelineError;
use satz_core::satz::SatzError;

use crate::satz::reports::{Finding, FindingSeverity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Note,
}

/// Where a diagnostic came from, so the drawer can group and the reader can judge.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagSource {
    /// the lossless document layer could not read the file's structure
    Cst,
    /// `satz_core::satz::parse` refused the file
    Parse,
    /// the fragment pipeline refused the estate (unknown type, fold conflict, …)
    Compile,
    /// `satz transpile --check`, run by the app before a write lands
    Check,
    /// a satz command run from the Commands view, by name
    Command(String),
    /// a tool over MCP refused, by name
    Tool(String),
    /// the view model's own finding (a pack line active while its gate is false)
    Model,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Diagnostic {
    pub file: Option<PathBuf>,
    /// 1-based, as satz counts
    pub line: Option<u32>,
    pub severity: Severity,
    /// which of satz's checks spoke, kebab-case (`unadopted-pack`,
    /// `missing-required`, …); `None` for a diagnostic that is not one of its findings
    #[serde(default)]
    pub kind: Option<String>,
    pub message: String,
    pub source: DiagSource,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, source: DiagSource) -> Self {
        Self {
            file: None,
            line: None,
            severity: Severity::Error,
            kind: None,
            message: message.into(),
            source,
        }
    }

    pub fn at(mut self, file: impl Into<PathBuf>, line: u32) -> Self {
        self.file = Some(file.into());
        self.line = Some(line);
        self
    }

    /// `SatzError` carries a line and no file: the caller names the file it parsed.
    pub fn from_satz_error(file: &Path, e: &SatzError) -> Self {
        Self {
            file: Some(file.to_path_buf()),
            line: Some(e.line as u32),
            severity: Severity::Error,
            kind: None,
            message: e.msg.clone(),
            source: DiagSource::Parse,
        }
    }

    /// One of satz's own findings: the severity it carries, the kind it came from, and
    /// its file resolved against `base` — the estate's directory — when relative, since
    /// satz names a `use` path as the loader saw it. A finding that belongs to a group
    /// keeps that header in front of its message, the way the CLI prints the two
    /// together; the header ends in its own colon, which becomes the separator.
    pub fn from_finding(base: &Path, f: &Finding, source: DiagSource) -> Self {
        let file = f.file.as_ref().map(|f| {
            let path = Path::new(f);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                base.join(path)
            }
        });
        let message = match &f.group {
            Some(group) => format!(
                "{}: {}",
                group.strip_suffix(':').unwrap_or(group),
                f.message
            ),
            None => f.message.clone(),
        };
        Self {
            file,
            line: f.line,
            severity: match f.severity {
                FindingSeverity::Error => Severity::Error,
                FindingSeverity::Warning => Severity::Warning,
                FindingSeverity::Note => Severity::Note,
            },
            kind: Some(f.kind.clone()),
            message,
            source,
        }
    }

    /// `PipelineError` names the file as the loader was given it — relative paths are
    /// resolved against `base`, which is the estate's directory.
    pub fn from_pipeline_error(base: &Path, e: &PipelineError) -> Self {
        let file = Path::new(&e.file);
        let file = if file.is_absolute() {
            file.to_path_buf()
        } else {
            base.join(file)
        };
        Self {
            file: Some(file),
            line: Some(e.line as u32),
            severity: Severity::Error,
            kind: None,
            message: e.msg.clone(),
            source: DiagSource::Compile,
        }
    }

    /// Re-point a diagnostic that names a temp file at the real one (same lines).
    pub fn repoint(mut self, from: &Path, to: &Path) -> Self {
        if self.file.as_deref() == Some(from) {
            self.file = Some(to.to_path_buf());
        }
        self
    }
}

/// Parse what satz printed — its stderr, or the text of a refused tool call — into
/// diagnostics. The banner (`satz vX (built …)`) is dropped; `error: `, `warning: ` and
/// `note: ` set the severity; `transpile --check: ` is stripped; `file:line: msg` gives
/// the location and `satz: line N: msg` the line alone; an indented line continues the
/// diagnostic above it (satz prints a fold conflict as a header and its origins
/// indented under it). A line that fits none of that is a diagnostic without a
/// location, verbatim — nothing satz says is dropped.
pub fn parse_satz_output(text: &str, source: DiagSource) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() || is_banner(line) {
            continue;
        }
        let continuation = raw.starts_with(' ') || raw.starts_with('\t');
        if continuation && let Some(last) = out.last_mut() {
            last.message.push('\n');
            last.message.push_str(line.trim_start());
            continue;
        }
        let (severity, rest) = strip_severity(line.trim_start());
        let rest = rest.strip_prefix("transpile --check: ").unwrap_or(rest);
        let mut d = Diagnostic {
            file: None,
            line: None,
            severity,
            kind: None,
            message: rest.to_string(),
            source: source.clone(),
        };
        if let Some((n, msg)) = split_satz_line(rest) {
            d.line = Some(n);
            d.message = msg.to_string();
        } else if let Some((file, n, msg)) = split_location(rest) {
            d.file = Some(PathBuf::from(file));
            d.line = Some(n);
            d.message = msg.to_string();
        }
        out.push(d);
    }
    out
}

fn is_banner(line: &str) -> bool {
    line.starts_with("satz v") && line.contains("(built ")
}

fn strip_severity(line: &str) -> (Severity, &str) {
    if let Some(r) = line.strip_prefix("error: ") {
        (Severity::Error, r)
    } else if let Some(r) = line.strip_prefix("warning: ") {
        (Severity::Warning, r)
    } else if let Some(r) = line.strip_prefix("note: ") {
        (Severity::Note, r)
    } else {
        (Severity::Error, line)
    }
}

/// `satz: line 12: msg` → (12, msg)
fn split_satz_line(s: &str) -> Option<(u32, &str)> {
    let rest = s.strip_prefix("satz: line ")?;
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let n: u32 = rest[..digits].parse().ok()?;
    let msg = rest[digits..].strip_prefix(": ")?;
    Some((n, msg))
}

/// `path/to/file.satz:12: msg` → (path, 12, msg). The first `:<digits>: ` wins, so a
/// Windows drive letter (`C:\…`) is not mistaken for the separator.
fn split_location(s: &str) -> Option<(&str, u32, &str)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1 && s[j..].starts_with(": ") && i > 0 {
                let n: u32 = s[i + 1..j].parse().ok()?;
                return Some((&s[..i], n, &s[j + 2..]));
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pipeline_line_has_file_and_line() {
        let d = parse_satz_output(
            "satz v0.56.1 (built 2026-09-13 13:56:42)\nerror: transpile --check: yaml/smoke.satz:12: unknown param 'x'\n",
            DiagSource::Check,
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].file.as_deref(), Some(Path::new("yaml/smoke.satz")));
        assert_eq!(d[0].line, Some(12));
        assert_eq!(d[0].message, "unknown param 'x'");
        assert_eq!(d[0].severity, Severity::Error);
    }

    #[test]
    fn a_parse_error_has_a_line_and_no_file() {
        let d = parse_satz_output("satz: line 3: expected `{`", DiagSource::Parse);
        assert_eq!(d[0].line, Some(3));
        assert_eq!(d[0].file, None);
        assert_eq!(d[0].message, "expected `{`");
    }

    #[test]
    fn an_indented_line_continues_the_one_above() {
        let d = parse_satz_output(
            "composition conflict at google_folder.x\n  - a.satz:4\n  - b.satz:9\nwarning: something else",
            DiagSource::Compile,
        );
        assert_eq!(d.len(), 2);
        assert_eq!(
            d[0].message,
            "composition conflict at google_folder.x\n- a.satz:4\n- b.satz:9"
        );
        assert_eq!(d[1].severity, Severity::Warning);
    }

    #[test]
    fn a_windows_path_keeps_its_drive_letter() {
        let (f, n, m) = split_location(r"C:\estates\acme\yaml\a.satz:7: msg").unwrap();
        assert_eq!(f, r"C:\estates\acme\yaml\a.satz");
        assert_eq!((n, m), (7, "msg"));
    }

    fn finding(severity: FindingSeverity, kind: &str) -> Finding {
        Finding {
            severity,
            kind: kind.to_string(),
            group: None,
            file: None,
            line: None,
            message: "the provider requires location".to_string(),
        }
    }

    #[test]
    fn a_finding_keeps_its_kind_and_resolves_its_file_against_the_estate() {
        let mut f = finding(FindingSeverity::Warning, "missing-required");
        f.file = Some("presets/organization-budget.satz".to_string());
        f.line = Some(12);
        let d = Diagnostic::from_finding(Path::new("/e/yaml"), &f, DiagSource::Check);
        assert_eq!(
            d.file.as_deref(),
            Some(Path::new("/e/yaml/presets/organization-budget.satz"))
        );
        assert_eq!(d.line, Some(12));
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.kind.as_deref(), Some("missing-required"));
        assert_eq!(d.message, "the provider requires location");

        f.file = Some("/other/a.satz".to_string());
        let d = Diagnostic::from_finding(Path::new("/e/yaml"), &f, DiagSource::Check);
        assert_eq!(d.file.as_deref(), Some(Path::new("/other/a.satz")));
    }

    #[test]
    fn a_group_heads_the_message_and_keeps_one_colon() {
        let mut f = finding(FindingSeverity::Error, "missing-required");
        f.group = Some("required arguments missing:".to_string());
        let d = Diagnostic::from_finding(Path::new("/e/yaml"), &f, DiagSource::Check);
        assert_eq!(
            d.message,
            "required arguments missing: the provider requires location"
        );
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.file, None);
        assert_eq!(d.line, None);
    }

    #[test]
    fn a_note_without_a_group_keeps_its_message_verbatim() {
        let d = Diagnostic::from_finding(
            Path::new("/e/yaml"),
            &finding(FindingSeverity::Note, "prerequisites"),
            DiagSource::Tool("satz_transpile_check".to_string()),
        );
        assert_eq!(d.severity, Severity::Note);
        assert_eq!(d.message, "the provider requires location");
        assert_eq!(
            d.source,
            DiagSource::Tool("satz_transpile_check".to_string())
        );
    }

    #[test]
    fn repoint_moves_only_the_named_file() {
        let d = Diagnostic::error("m", DiagSource::Check).at("/e/yaml/a.satz.studio-tmp", 2);
        let d = d.repoint(
            Path::new("/e/yaml/a.satz.studio-tmp"),
            Path::new("/e/yaml/a.satz"),
        );
        assert_eq!(d.file.as_deref(), Some(Path::new("/e/yaml/a.satz")));
    }
}
