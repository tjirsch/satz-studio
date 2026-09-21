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
    Info,
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
    /// a satz command run from a command deck, by name
    Command(String),
    /// a tool over MCP refused, by name
    Tool(String),
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

    /// The file is stored as [`plain`] gives it, whichever side named it: satz prints a
    /// plain path and the app knows the same file through `canonicalize`, so two
    /// diagnostics about one file are one file.
    pub fn at(mut self, file: impl Into<PathBuf>, line: u32) -> Self {
        self.file = Some(plain(&file.into()));
        self.line = Some(line);
        self
    }

    /// `SatzError` carries a line and no file: the caller names the file it parsed.
    pub fn from_satz_error(file: &Path, e: &SatzError) -> Self {
        Self {
            file: Some(plain(file)),
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
            let path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                base.join(path)
            };
            plain(&path)
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
                FindingSeverity::Info => Severity::Info,
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
            file: Some(plain(&file)),
            line: Some(e.line as u32),
            severity: Severity::Error,
            kind: None,
            message: e.msg.clone(),
            source: DiagSource::Compile,
        }
    }

    /// Re-point a diagnostic that names a temp file at the real one (same lines). The
    /// comparison is on [`plain`]: satz names the file it was given, and the app knows
    /// that file through `canonicalize`, which on Windows is the same path in another
    /// form.
    pub fn repoint(mut self, from: &Path, to: &Path) -> Self {
        if self.file.as_deref().map(plain) == Some(plain(from)) {
            self.file = Some(to.to_path_buf());
        }
        self
    }
}

/// One form for one file, so two paths to it compare equal.
///
/// On Windows `std::fs::canonicalize` returns an extended-length path — `\\?\D:\estate`,
/// or `\\?\UNC\server\share` for a network path — while satz prints the plain one. The two
/// name the same file and are not equal as strings, which is how a refusal on a temp file
/// kept the temp file's name: nothing matched, so nothing was re-pointed. Everywhere else
/// this is the path itself.
pub fn plain(path: &Path) -> PathBuf {
    let Some(rest) = path.to_str().and_then(|s| s.strip_prefix(r"\\?\")) else {
        return path.to_path_buf();
    };
    match rest.strip_prefix(r"UNC\") {
        Some(share) => PathBuf::from(format!(r"\\{share}")),
        None => PathBuf::from(rest),
    }
}

/// Parse what satz printed — its stderr, or the text of a refused tool call — into
/// diagnostics.
///
/// satz lays its findings out for a reader (its `src/findings.rs`, `lay_out`): a group's
/// title, `<title> (<count>)`; per finding a first line of columns — severity, kind,
/// `file:line`, subject — with the message indented under it and `fix: <command>` last;
/// findings of one group that say the same thing as ONE block, their first lines as a
/// table and the message once, which belongs to every row; and a footer of counts. Each
/// row is a diagnostic at its own location, its message the group's title and the text
/// under the block — the same message [`Diagnostic::from_finding`] builds from the JSON.
///
/// Around the findings satz prints other lines, read as before: the banner
/// (`satz vX (built …)`) is dropped; `error: `, `warning: ` and `info: ` set the
/// severity; `transpile --check: ` is stripped; `file:line: msg` gives the location and
/// `satz: line N: msg` the line alone; an indented line continues the diagnostic above
/// it. A line that fits none of that is a diagnostic without a location, verbatim —
/// nothing satz says is dropped, bar the title and the footer, which count what the rows
/// already carry.
pub fn parse_satz_output(text: &str, source: DiagSource) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = Vec::new();
    // per diagnostic: the group title it stands under, and the subject its row named
    let mut titles: Vec<Option<String>> = Vec::new();
    let mut subjects: Vec<String> = Vec::new();
    let mut title: Option<String> = None;
    // the first diagnostic an indented line belongs to — a block's rows share the text
    // under them — and whether the line before was a row, so the next row joins it
    let mut body_from = 0;
    let mut in_rows = false;
    for raw in text.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() {
            in_rows = false;
            continue;
        }
        if is_banner(line) {
            continue;
        }
        if raw.starts_with(' ') || raw.starts_with('\t') {
            for d in out.iter_mut().skip(body_from) {
                if !d.message.is_empty() {
                    d.message.push('\n');
                }
                d.message.push_str(line.trim_start());
            }
            in_rows = false;
            continue;
        }
        if let Some(t) = group_title(line) {
            title = Some(t.to_string());
            in_rows = false;
            continue;
        }
        if is_footer(line) {
            continue;
        }
        if let Some(row) = finding_row(line) {
            if !in_rows {
                body_from = out.len();
            }
            in_rows = true;
            let (file, line) = match row.location.map(split_at) {
                Some((file, line)) => (Some(PathBuf::from(file)), line),
                None => (None, None),
            };
            out.push(Diagnostic {
                file,
                line,
                severity: row.severity,
                kind: Some(row.kind.to_string()),
                message: String::new(),
                source: source.clone(),
            });
            titles.push(title.clone());
            subjects.push(row.subject.to_string());
            continue;
        }
        in_rows = false;
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
        body_from = out.len();
        out.push(d);
        titles.push(None);
        subjects.push(String::new());
    }
    for ((d, title), subject) in out.iter_mut().zip(titles).zip(subjects) {
        if d.message.is_empty() {
            d.message = subject;
        }
        if let Some(title) = title {
            d.message = format!(
                "{}: {}",
                title.strip_suffix(':').unwrap_or(&title),
                d.message
            );
        }
    }
    out
}

fn is_banner(line: &str) -> bool {
    line.starts_with("satz v") && line.contains("(built ")
}

/// A finding's first line, in satz's columns: severity, kind, where, what.
struct Row<'a> {
    severity: Severity,
    kind: &'a str,
    location: Option<&'a str>,
    subject: &'a str,
}

/// `error    unadopted-pack  yaml/acme.satz:12  use_budget` — the severity word padded
/// into its column, so it is followed by two spaces or more, never by `: `. The columns
/// are separated by two spaces or more; `file:line` stands before the subject, and a
/// finding with no location leaves its column blank.
fn finding_row(line: &str) -> Option<Row<'_>> {
    let (severity, rest) = [
        ("error", Severity::Error),
        ("warning", Severity::Warning),
        ("info", Severity::Info),
    ]
    .into_iter()
    .find_map(|(word, s)| {
        line.strip_prefix(word)
            .filter(|r| r.starts_with("  "))
            .map(|r| (s, r))
    })?;
    let mut columns = rest.split("  ").map(str::trim).filter(|c| !c.is_empty());
    let kind = columns.next()?;
    if !kind.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
        return None;
    }
    let rest: Vec<&str> = columns.collect();
    let (location, subject) = match rest.split_first() {
        None => (None, ""),
        Some((first, more)) if is_location(first) => {
            (Some(*first), more.first().copied().unwrap_or_default())
        }
        Some((first, _)) => (None, *first),
    };
    // a subject with two spaces inside it is cut at them; its first part is kept, and the
    // message under the row says the whole of it
    Some(Row {
        severity,
        kind,
        location,
        subject,
    })
}

/// `file:line`, or a file alone when the whole file is the subject.
fn is_location(s: &str) -> bool {
    split_at(s).1.is_some() || s.ends_with(".satz") || s.ends_with(".tf")
}

/// `file:line` → (file, Some(line)); anything else is a file with no line. The LAST `:`
/// is the separator, so a Windows drive letter stays with its path.
fn split_at(s: &str) -> (&str, Option<u32>) {
    match s.rsplit_once(':') {
        Some((file, n))
            if !file.is_empty() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) =>
        {
            (file, n.parse().ok())
        }
        _ => (s, None),
    }
}

/// `<title> (<count>)` or `<title> (<shown> of <all>, <n> silenced)`: the line a group of
/// findings stands under.
fn group_title(line: &str) -> Option<&str> {
    let body = line.strip_suffix(')')?;
    let (title, count) = body.rsplit_once(" (")?;
    let first = count.split(' ').next()?;
    (first.parse::<usize>().is_ok() && (count == first || count.ends_with(" silenced")))
        .then_some(title)
}

/// The last line of a run: `1 error, 10 warnings`, `…; 3 silenced (2 estate, 1 run) — …`,
/// or the silenced part alone.
fn is_footer(line: &str) -> bool {
    let counted = |part: &str| {
        let mut words = part.splitn(2, ' ');
        words.next().is_some_and(|n| n.parse::<usize>().is_ok())
            && words.next().is_some_and(|w| {
                matches!(
                    w,
                    "error" | "errors" | "warning" | "warnings" | "info" | "infos"
                )
            })
    };
    let head = line.split(';').next().unwrap_or(line);
    head.split(", ").all(counted)
        || (line
            .split(' ')
            .next()
            .is_some_and(|n| n.parse::<usize>().is_ok())
            && line.contains(" silenced ("))
}

fn strip_severity(line: &str) -> (Severity, &str) {
    if let Some(r) = line.strip_prefix("error: ") {
        (Severity::Error, r)
    } else if let Some(r) = line.strip_prefix("warning: ") {
        (Severity::Warning, r)
    } else if let Some(r) = line.strip_prefix("info: ") {
        (Severity::Info, r)
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

    /// satz's own layout (`lay_out`): a title, a block of two rows sharing one message
    /// and one command, a second group, and the footer.
    const LAID_OUT: &str = "satz v0.73.0 (built 2026-09-21 07:30:15)
packs on while a pack they need is off (2)

warning  pack-requirement  yaml/smoke.satz:139  presets/monitoring/organization-audit-logsink.satz
warning  pack-requirement  yaml/smoke.satz:140  presets/monitoring/organization-cis-log-alerts-central.satz
    needs `presets/estate-map.satz`, which is off
    fix: satz add-pack smoke.satz presets/estate-map.satz

notices open — what a pack asks to be run once it is on (1)

error    notice            yaml/new.satz:79     cis_baseline_adopted
    Run satz adopt first.
    fix: satz adopt new.satz --execute --import

info     unadopted-pack                         use_budget

1 error, 2 warnings, 1 info
";

    #[test]
    fn the_laid_out_findings_are_one_diagnostic_per_row() {
        let d = parse_satz_output(LAID_OUT, DiagSource::Check);
        assert_eq!(
            d.len(),
            4,
            "the title and the footer are no findings: {d:?}"
        );

        // a block: each row at its own line, the one message under it said by both
        for (i, line) in [(0, 139), (1, 140)] {
            assert_eq!(d[i].severity, Severity::Warning);
            assert_eq!(d[i].kind.as_deref(), Some("pack-requirement"));
            assert_eq!(d[i].file.as_deref(), Some(Path::new("yaml/smoke.satz")));
            assert_eq!(d[i].line, Some(line));
            assert_eq!(
                d[i].message,
                "packs on while a pack they need is off: needs `presets/estate-map.satz`, which is off\nfix: satz add-pack smoke.satz presets/estate-map.satz"
            );
        }

        assert_eq!(d[2].severity, Severity::Error);
        assert_eq!(d[2].kind.as_deref(), Some("notice"));
        assert_eq!(
            (d[2].file.as_deref(), d[2].line),
            (Some(Path::new("yaml/new.satz")), Some(79))
        );
        assert!(
            d[2].message.starts_with(
                "notices open — what a pack asks to be run once it is on: Run satz adopt first."
            ),
            "{}",
            d[2].message
        );

        // no location, no text under it: the subject is what it says
        assert_eq!(d[3].severity, Severity::Info);
        assert_eq!((d[3].file.as_deref(), d[3].line), (None, None));
        assert!(d[3].message.ends_with(": use_budget"), "{}", d[3].message);
    }

    #[test]
    fn a_line_that_only_starts_with_a_severity_word_is_no_row() {
        let d = parse_satz_output(
            "error: transpile --check: yaml/a.satz:3: bad",
            DiagSource::Check,
        );
        assert_eq!(d[0].kind, None);
        assert_eq!((d[0].line, d[0].message.as_str()), (Some(3), "bad"));
        let d = parse_satz_output("errors happen", DiagSource::Check);
        assert_eq!(d[0].message, "errors happen");
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
            subject: None,
            message: "the provider requires location".to_string(),
            fix: None,
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

    /// The Windows forms, checked on every platform because the string rule is the same
    /// everywhere and the bug they caused — a refusal that kept naming the temp file —
    /// only ever showed on the runner.
    #[test]
    fn an_extended_length_path_and_a_plain_one_are_one_file() {
        assert_eq!(
            plain(Path::new(r"\\?\D:\estate\yaml\a.satz")),
            PathBuf::from(r"D:\estate\yaml\a.satz")
        );
        assert_eq!(
            plain(Path::new(r"\\?\UNC\server\share\a.satz")),
            PathBuf::from(r"\\server\share\a.satz")
        );
        // what every other platform carries, and Windows too once satz has printed it
        assert_eq!(
            plain(Path::new("/estates/acme/yaml/a.satz")),
            PathBuf::from("/estates/acme/yaml/a.satz")
        );
        assert_eq!(
            plain(Path::new(r"D:\estate\yaml\a.satz")),
            PathBuf::from(r"D:\estate\yaml\a.satz")
        );
    }

    /// The two halves of the fix: what satz printed and what the app canonicalised name
    /// the same file, so a diagnostic about the temp file re-points to the real one.
    #[test]
    fn a_diagnostic_repoints_across_the_two_forms() {
        let printed = Path::new(r"D:\estate\yaml\a.studio-tmp.satz");
        let canonical = Path::new(r"\\?\D:\estate\yaml\a.studio-tmp.satz");
        let real = Path::new(r"\\?\D:\estate\yaml\a.satz");
        let d = Diagnostic::error("unknown param", DiagSource::Check)
            .at(printed, 20)
            .repoint(canonical, real);
        assert_eq!(d.file.as_deref(), Some(real));
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
            &finding(FindingSeverity::Info, "prerequisites"),
            DiagSource::Tool("satz_transpile_check".to_string()),
        );
        assert_eq!(d.severity, Severity::Info);
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
