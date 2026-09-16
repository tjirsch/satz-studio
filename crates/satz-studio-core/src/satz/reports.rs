//! The JSON a reporting command writes with `--format json` and satz returns as
//! `structuredContent` over MCP, typed. Shapes mirror satz `src/questions.rs` and
//! `src/mcp.rs` at the pinned release: unknown fields are ignored (satz may add some),
//! missing required fields fail loudly (satz removed one, and the pin must move).

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuestionKind {
    Param,
    Oneof,
}

/// What changing the answer later costs: an edit, state surgery, or a recreate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reversal {
    Edit,
    StateSurgery,
    Recreate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Blast {
    None,
    Low,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum QuestionState {
    Answered,
    Unanswered,
    NotApplicable,
}

/// One question, joined with the answer the estate currently carries.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QuestionRow {
    /// the param it answers, or the group name for a choice
    pub subject: String,
    pub kind: QuestionKind,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    pub reversal: Reversal,
    pub blast: Blast,
    pub state: QuestionState,
    /// the estate's own value when answered
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<serde_json::Value>,
    /// the pack's default when unanswered and one exists — what an interview offers
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    /// unanswered and no usable default: a value has to be typed
    pub blocking: bool,
    /// the pack's own description — its header's first paragraph
    pub pack_description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommend: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<OptionRow>,
    /// the file that declared it
    pub from: String,
    pub pack: String,
}

impl QuestionRow {
    /// A recreate, or a high blast: the interview reads the `why` out before it.
    pub fn one_way_door(&self) -> bool {
        self.reversal == Reversal::Recreate || self.blast == Blast::High
    }
    /// The value the interview offers: the estate's own, else the pack's default.
    pub fn offered(&self) -> Option<&serde_json::Value> {
        self.current.as_ref().or(self.default.as_ref())
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OptionRow {
    pub param: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QuestionsReport {
    pub estate: String,
    pub questions: Vec<QuestionRow>,
    pub summary: QuestionsSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct QuestionsSummary {
    pub total: usize,
    pub answered: usize,
    pub unanswered: usize,
    pub not_applicable: usize,
    pub blocking: usize,
    pub one_way_doors: usize,
    /// THE GATE: every applicable question is answered
    pub complete: bool,
}

/// What `satz_interview` returns: the report, plus what the call did to the file.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InterviewReport {
    pub created: bool,
    pub written: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rename_to: Option<String>,
    #[serde(flatten)]
    pub report: QuestionsReport,
}

/// The arguments of `satz_interview`, as the app sends them.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct InterviewArgs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<InterviewFilter>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub create: bool,
    /// subject → value; for a `oneof`, the chosen option's PARAM NAME
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub answers: BTreeMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub accept_defaults: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InterviewFilter {
    Unanswered,
    All,
}

/// What `satz_update_prerequisites` returns. Only the lines it wrote are read: the
/// report beside them is what the command's own `--report-only` run prints into the
/// log, and the app does not render it twice.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PrerequisitesResult {
    /// the lines written into the estate; empty when nothing was missing
    pub written: Vec<String>,
}

/// What `satz_open` resolved — including the identity the estate's live tools run as.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OpenReport {
    pub config: String,
    pub estate: String,
    #[serde(default)]
    pub deployment_mode: Option<String>,
    #[serde(default)]
    pub runs_as: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EstateEntry {
    pub config: String,
    pub estate: String,
    #[serde(default)]
    pub deployment_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EstatesReport {
    pub root: String,
    pub estates: Vec<EstateEntry>,
}

/// How bad a finding is. `Error` refuses the compile; `Warning` and `Note` come back
/// with a summary that passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingSeverity {
    Error,
    Warning,
    Note,
}

/// One thing the compile found after the front end, at the file and line it names.
/// satz's own list (`src/findings.rs`), as it serialises it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Finding {
    pub severity: FindingSeverity,
    /// Which check spoke, kebab-case as satz writes it: `unadopted-pack`,
    /// `missing-required`, `written-reference`, `conflict`, `dry-run-conflict`,
    /// `suppression`, `emit`, `prerequisites`, `providers`, `action`, `hcl-passthrough`.
    /// A `String` rather than an enum, so a kind satz adds is carried through instead
    /// of failing the whole result.
    pub kind: String,
    /// The header the CLI prints once above the findings that share it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// The file as the loader saw it — a `use` path, or the estate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// 1-based, in that file
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub message: String,
}

/// What a compile produced: the emitted addresses, the files written (empty for a
/// check), and what the compile found and did not refuse on.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompileSummary {
    pub estate: String,
    pub addresses: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub written: Vec<String>,
    /// the warnings and notes the CLI prints; `CliChecker` synthesises a summary and
    /// has none
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<Finding>,
}

/// The `structuredContent` of a refused compile tool: what the compile found, each
/// finding at its own line. satz sends the whole `CompileSummary` shape with nothing
/// emitted; the findings are the half a caller can point at lines. A refusal that
/// never reached the compile — a missing file, an estate outside the root — carries no
/// structured content at all.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Refusal {
    #[serde(default)]
    pub findings: Vec<Finding>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMOKE: &str = include_str!("../../tests/fixtures/questions-smoke.json");

    #[test]
    fn the_recorded_questions_report_round_trips() {
        let report: QuestionsReport = serde_json::from_str(SMOKE).unwrap();
        assert_eq!(report.summary.total, report.questions.len());
        let again: serde_json::Value = serde_json::to_value(&report).unwrap();
        let original: serde_json::Value = serde_json::from_str(SMOKE).unwrap();
        assert_eq!(again, original);
    }

    #[test]
    fn enums_read_the_words_satz_prints() {
        let q: QuestionRow = serde_json::from_value(serde_json::json!({
            "subject": "security_model", "kind": "oneof", "prompt": "p", "reversal": "state_surgery",
            "blast": "low", "state": "not-applicable", "blocking": false, "pack_description": "d",
            "options": [{"param": "s1", "label": "S1", "selected": true}], "from": "f", "pack": "p"
        }))
        .unwrap();
        assert_eq!(q.kind, QuestionKind::Oneof);
        assert_eq!(q.reversal, Reversal::StateSurgery);
        assert_eq!(q.state, QuestionState::NotApplicable);
        assert!(!q.one_way_door());
    }

    #[test]
    fn an_interview_report_flattens_the_questions_report() {
        let r: InterviewReport = serde_json::from_value(serde_json::json!({
            "created": true, "written": 2, "rename_to": "C0example.satz",
            "estate": "x.satz", "questions": [], "summary": {"total": 0, "answered": 0, "unanswered": 0, "not_applicable": 0, "blocking": 0, "one_way_doors": 0, "complete": true}
        }))
        .unwrap();
        assert_eq!(r.rename_to.as_deref(), Some("C0example.satz"));
        assert!(r.report.summary.complete);
    }

    /// The `structuredContent` of a refused `satz_transpile_check`, recorded from the
    /// pinned release.
    const REFUSED: &str = r#"{
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
    }"#;

    #[test]
    fn a_refusals_structured_content_reads_as_findings() {
        let refusal: Refusal = serde_json::from_str(REFUSED).unwrap();
        assert_eq!(refusal.findings.len(), 1);
        let f = &refusal.findings[0];
        assert_eq!(f.severity, FindingSeverity::Error);
        assert_eq!(f.kind, "missing-required");
        assert_eq!(f.line, Some(109));
        assert_eq!(f.group.as_deref(), Some("required arguments missing:"));
        // the same payload is a summary: a refusal emits nothing
        let summary: CompileSummary = serde_json::from_str(REFUSED).unwrap();
        assert!(summary.addresses.is_empty());
        assert_eq!(summary.findings, refusal.findings);
    }

    #[test]
    fn a_summary_without_findings_reads_and_a_kind_satz_adds_is_carried() {
        let s: CompileSummary = serde_json::from_value(
            serde_json::json!({"estate": "e", "addresses": ["google_folder.x"]}),
        )
        .unwrap();
        assert!(s.findings.is_empty());
        let f: Finding = serde_json::from_value(serde_json::json!({
            "severity": "note", "kind": "a-kind-satz-grew", "message": "m"
        }))
        .unwrap();
        assert_eq!(f.kind, "a-kind-satz-grew");
        assert_eq!(f.severity, FindingSeverity::Note);
        assert_eq!((f.file, f.line, f.group), (None, None, None));
    }

    #[test]
    fn interview_args_send_only_what_is_set() {
        let a = InterviewArgs {
            answers: BTreeMap::from([("x".to_string(), serde_json::json!(true))]),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&a).unwrap(),
            serde_json::json!({"answers": {"x": true}})
        );
    }
}
