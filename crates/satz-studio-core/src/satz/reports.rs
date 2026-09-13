//! The JSON satz prints with `--format json` and returns as `structuredContent` over
//! MCP, typed. Shapes mirror satz `src/questions.rs` and `src/mcp.rs` at the pinned
//! release: unknown fields are ignored (satz may add some), missing required fields
//! fail loudly (satz removed one, and the pin must move).

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

/// What a compile produced: the emitted addresses, and the files written (empty for a check).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompileSummary {
    pub estate: String,
    pub addresses: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub written: Vec<String>,
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
