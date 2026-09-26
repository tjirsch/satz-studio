//! The JSON a reporting command writes with `--format json` and satz returns as
//! `structuredContent` over MCP, typed. Shapes mirror satz `src/questions.rs`,
//! `src/review_pack.rs`, `src/interface_report.rs` and `src/mcp.rs` at the pinned release: unknown fields are ignored (satz may add some),
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

/// The shape the pack declares a param with, and so the shape an answer must have.
/// satz refuses an answer that contradicts it, so the app types its field in this
/// shape and never derives one of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    String,
    Number,
    Bool,
    List,
    Map,
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
    /// the shape the pack declares the param with. Absent for a choice, which is
    /// answered by an option's name, and for a param whose declaration names no
    /// shape satz can read off
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<Shape>,
    /// the pack's own description — its header's first paragraph
    pub pack_description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommend: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<OptionRow>,
    /// a choice only: exactly one option must be set. A choice that is not required is
    /// also answered [`NO_BRANCH`] — every option `false`
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
    /// what an empty answer means, where the question says (`empty = "…"`): `""` is then
    /// an answer — offered, accepted and counted — rather than one still to give
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty: Option<String>,
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
    /// A choice that is not required, which [`NO_BRANCH`] answers besides its options.
    pub fn offers_none(&self) -> bool {
        self.kind == QuestionKind::Oneof && !self.required
    }
    /// The answer a choice carries: the option the estate sets, or [`NO_BRANCH`] for an
    /// answered choice that is not required and sets none of them.
    pub fn bound_option(&self) -> Option<&str> {
        self.options
            .iter()
            .find(|o| o.selected)
            .map(|o| o.param.as_str())
            .or_else(|| {
                (self.offers_none() && self.state == QuestionState::Answered).then_some(NO_BRANCH)
            })
    }
    /// The label of the answer a choice carries: its bound option's, or `None` for
    /// [`NO_BRANCH`].
    pub fn bound_label(&self) -> Option<String> {
        let b = self.bound_option()?;
        Some(
            self.options
                .iter()
                .find(|o| o.param == b)
                .map_or_else(|| "None".to_string(), |o| o.label.clone()),
        )
    }
    /// A value as satz shows it: a string is itself, and an empty answer to a question
    /// that says what `""` means carries that meaning beside it.
    pub fn shown(&self, v: &serde_json::Value) -> String {
        match (&self.empty, v) {
            (Some(meaning), serde_json::Value::String(s)) if s.is_empty() => {
                format!("\"\" ({meaning})")
            }
            (_, serde_json::Value::String(s)) => s.clone(),
            (_, other) => other.to_string(),
        }
    }
}

/// The answer to a choice that is not `required` which sets none of its options: every
/// option is bound `false`. satz's `NO_BRANCH` (`vendor/satz/src/questions.rs`).
pub const NO_BRANCH: &str = "none";

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

/// One notice of a pack the estate uses: what a pack asks to be run once it is
/// switched on, and the param the estate binds `true` to say it has been. satz's own
/// `NoticeRow` (`vendor/satz/src/notices.rs`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NoticeRow {
    /// the param the estate binds `true` to acknowledge it
    pub param: String,
    /// the file that declares it, as the `use` that reached it names it
    pub pack: String,
    /// what to do and why
    pub text: String,
    /// the command to run, with `<estate>` where the estate file goes
    pub run: String,
    /// what the pack declared: `error` — every command that writes to the organisation
    /// refuses while it is open — `warning`, or `info`. Required: a notice without one
    /// fails the report rather than reading as one that holds nothing up.
    pub severity: FindingSeverity,
    /// the estate binds the param `true`
    pub acknowledged: bool,
}

impl NoticeRow {
    /// Every command that writes to the organisation — apply and bootstrap among them —
    /// refuses while this one is open.
    pub fn holds_up_apply(&self) -> bool {
        self.severity == FindingSeverity::Error
    }
}

/// What `satz_interview` returns: the report, plus what the call did to the file and
/// the notices its answers opened.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InterviewReport {
    pub created: bool,
    pub written: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rename_to: Option<String>,
    /// the notices this call opened by switching a pack on — shown once, when they
    /// open; a later call returns only what it opens
    pub notices: Vec<NoticeRow>,
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

/// How bad a finding is. `Error` refuses the compile; `Warning` and `Info` come back
/// with a summary that passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingSeverity {
    Error,
    Warning,
    Info,
}

/// One thing the compile found after the front end, at the file and line it names.
/// satz's own list (`src/findings.rs`), as it serialises it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Finding {
    pub severity: FindingSeverity,
    /// Which check spoke, kebab-case as satz writes it: `unadopted-pack`,
    /// `missing-required`, `written-reference`, `conflict`, `dry-run-conflict`,
    /// `suppression`, `emit`, `prerequisites`, `providers`, `action`, `hcl-passthrough`,
    /// `workload-folder`.
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
    /// What the finding is about, and with `kind` its identity: the pack a pack finding
    /// judges, the param a notice is acknowledged by.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    pub message: String,
    /// The command that answers the finding, as it is typed; absent where no one
    /// command does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

/// What `satz review-pack <pack> --format json` writes and `satz_review_pack` returns:
/// one pack judged against the preset library's own bar. satz's `Review`
/// (`vendor/satz/src/review_pack.rs`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PackReview {
    /// the pack, as an absolute path
    pub pack: String,
    /// the estate the pack was folded into: `synthetic`, the path `--against` named, or
    /// empty when the pack does not parse and nothing was folded
    pub folded_into: String,
    /// the resource addresses the pack contributes to that estate
    pub emits: Vec<String>,
    /// in the order satz checked them; every one of them names the pack as its `file`
    pub findings: Vec<Finding>,
}

impl PackReview {
    /// satz's verdict: the pack clears the bar when no finding is an error. Warnings are
    /// the author's to weigh. `satz review-pack` exits non-zero exactly when this is false.
    pub fn passed(&self) -> bool {
        !self
            .findings
            .iter()
            .any(|f| f.severity == FindingSeverity::Error)
    }

    /// How many findings carry `severity`.
    pub fn count(&self, severity: FindingSeverity) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == severity)
            .count()
    }
}

/// What a pack is to the pack graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackRole {
    /// `presets/estate-core.satz`, the day-0 pack every estate starts with: never switched
    Core,
    /// `presets/estate-map.satz`, which declares the gates of the menu packs
    Map,
    Pack,
}

/// Where a pack's `use` line stands in the estate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackLine {
    /// active, gated on its gate, where the graph places it
    Active,
    /// active without `when <gate>`: a no to the gate does not switch it off
    Ungated,
    Commented,
    /// the estate has no line for it, active or commented
    Absent,
    /// active, naming the pack's `.local` fork
    Forked,
    /// active outside the block the graph places it in
    Misplaced,
}

/// Why one pack needs another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RequirementKind {
    /// the map declares it
    Requires,
    /// the pack reads a param the other declares
    Data,
    /// the other declares the param the pack is gated on
    Gate,
}

/// One requirement of a pack: any one of `any_of` meets it. satz's `Requirement`
/// (`vendor/satz/src/packs.rs`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Requirement {
    pub kind: RequirementKind,
    /// the packs, by path, any one of which meets it
    pub any_of: Vec<String>,
    /// the params that make a `data` or `gate` requirement
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<String>,
    /// one of `any_of` is on — or, for `data`, the estate binds what the pack would
    /// otherwise read from it
    pub met: bool,
}

/// One pack the pack graph offers, as this estate has it. satz's `PackRow`
/// (`vendor/satz/src/packs.rs`), the row the Packs view draws.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PackRow {
    /// as a `use` line names it: `presets/…`
    pub path: String,
    pub role: PackRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<String>,
    /// the file that declares the gate
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_declared_in: Option<String>,
    /// the estate's own binding of the gate, as written
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    /// the gate's default in the file that declares it, as written
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// what the gate is in this estate: the answer, else the default while the declaring
    /// file is used
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<bool>,
    pub line: PackLine,
    /// the line's number, 1-based
    #[serde(default, rename = "at", skip_serializing_if = "Option::is_none")]
    pub at_line: Option<u32>,
    /// the path the line names when it is not the pack's own — its fork
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written: Option<String>,
    /// the param an active line is gated on when it is not the pack's gate
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gated_on: Option<String>,
    /// the pack emits: its line is active and its `when` holds
    pub deploys: bool,
    pub requires: Vec<Requirement>,
    /// the packs with a requirement this one meets
    pub required_by: Vec<String>,
    pub excludes: Vec<String>,
    /// the line is written by hand, never by satz; why
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_hand: Option<String>,
    /// what the pack asks to be run once it is on
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<NoticeRow>,
    /// what the pack adds to another pack's list params while it deploys, beside the
    /// estate's own entries
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contributes: Vec<Contributed>,
    /// the compile's findings about this pack, as sentences
    pub findings: Vec<String>,
}

/// One pack's entries in another pack's list param.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Contributed {
    /// the list param the entries are added to
    pub param: String,
    /// the entries, as the pack wrote them
    pub values: Vec<String>,
}

/// A `use` of a file the pack graph does not know: an estate's own file, or the
/// interface file of a central estate.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Unmanaged {
    pub path: String,
    /// the line's number, 1-based
    #[serde(rename = "at")]
    pub at_line: u32,
}

/// What `satz_packs` returns: every pack the pack graph offers, as this estate has it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PacksReport {
    pub estate: String,
    /// why the report has no packs: the presets carry no pack graph
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// every node of the graph, in the graph's order
    pub packs: Vec<PackRow>,
    pub unmanaged: Vec<Unmanaged>,
    /// the `use` lines of generated interface files (`interface "<name>"` as their
    /// header) — central estates' interfaces a project estate reads, no pack
    pub interfaces: Vec<Unmanaged>,
    /// the compile's pack findings, each with its pack as `subject`
    pub findings: Vec<Finding>,
}

impl PacksReport {
    /// The map row, when the graph has one.
    pub fn map(&self) -> Option<&PackRow> {
        self.packs.iter().find(|p| p.role == PackRole::Map)
    }

    /// The row of the pack at `path`.
    pub fn row(&self, path: &str) -> Option<&PackRow> {
        self.packs.iter().find(|p| p.path == path)
    }
}

/// The arguments of `satz_add_pack`, as the app sends them.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AddPackArgs {
    /// the pack's path (`presets/…`) or its gate
    pub pack: String,
    /// switch on what the pack needs too, where the graph names one pack for it
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub with_requirements: bool,
}

/// The arguments of `satz_remove_pack`, as the app sends them.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RemovePackArgs {
    /// the pack's path (`presets/…`) or its gate
    pub pack: String,
    /// switch off the packs that need it too
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cascade: bool,
}

/// A gate a switch bound.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Bound {
    pub param: String,
    pub value: bool,
}

/// What a switch did to one line.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LineEdit {
    pub path: String,
    /// 1-based, in the file as written
    #[serde(rename = "at")]
    pub at_line: u32,
    /// `uncommented` or `written`
    pub edit: String,
}

/// What `satz_add_pack` and `satz_remove_pack` return.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PackChange {
    pub estate: String,
    /// `add` or `remove`
    pub action: String,
    /// the packs switched, requirements and dependents included
    pub switched: Vec<String>,
    pub bound: Vec<Bound>,
    pub lines: Vec<LineEdit>,
    /// what the switch left as it is, and why
    pub left: Vec<String>,
    /// the questions the switch opened
    pub opened: Vec<String>,
    /// the notices the switch opened
    pub notices: Vec<NoticeRow>,
}

/// What `satz_merge_presets` returns: the run as events in walk order, the count of
/// each outcome, whether a human has to act, and the notices the merge opened.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MergeReport {
    /// nothing was written
    pub report_only: bool,
    pub events: Vec<MergeEvent>,
    pub counts: MergeCounts,
    /// something needs a human: the CLI exits non-zero on it
    pub attention: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<NoticeRow>,
}

/// One line of a merge run, as satz tags it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum MergeEvent {
    Pack {
        /// the path under `presets_dir`
        file: String,
        /// what the run did to it, kebab-case as satz writes it (`installed`,
        /// `forked-and-repointed`, …); a `String`, so an outcome satz adds is carried
        action: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fork: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        diff: Option<String>,
        /// `2.7 -> 2.8`, where both versions are known
        #[serde(default, skip_serializing_if = "Option::is_none")]
        versions: Option<String>,
        /// why, for a refusal
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// upstream changed a pack's content without moving its version
    Warning { file: String, text: String },
    /// something the run said that is not about one pack
    Note { text: String },
    /// what an adoption changes in the emission
    EmissionDelta { lines: Vec<String> },
    /// the roles and APIs the estate's resource types need and it did not declare
    Prerequisites {
        wrote: Vec<String>,
        missing: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        refused: Option<String>,
    },
}

/// How many packs each outcome took; a report-only run counts what it would do.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MergeCounts {
    pub installed: usize,
    pub current: usize,
    pub artifacts_updated: usize,
    pub doc_only: usize,
    pub unused_overwritten: usize,
    pub adopted_in_place: usize,
    pub forked_and_repointed: usize,
    pub fork_diffs_refreshed: usize,
    pub deferred: usize,
    pub refused: usize,
    pub skipped_edited: usize,
}

impl MergeReport {
    /// The run as the command log shows it: one sentence per pack the run changed or
    /// would change, satz's warnings, notes and prerequisites, the counts, and each
    /// notice the merge opened with the command it names. A pack that is current says
    /// nothing; the counts carry it.
    pub fn lines(&self) -> Vec<String> {
        let dry = self.report_only;
        let verb = |would: &str, did: &str| if dry { would } else { did }.to_string();
        let mut out = Vec::new();
        for e in &self.events {
            match e {
                MergeEvent::Pack {
                    file,
                    action,
                    fork,
                    diff,
                    versions,
                    reason,
                } => {
                    let fork = fork.as_deref().unwrap_or("its fork");
                    let diff = diff.as_deref().unwrap_or("the delta file");
                    let versions = versions
                        .as_deref()
                        .map(|v| format!(" ({v})"))
                        .unwrap_or_default();
                    let line = match action.as_str() {
                        "current" => continue,
                        "installed" => format!("{} {file}", verb("would install", "installed")),
                        "artifact-updated" => {
                            format!("{} {file}", verb("would update", "updated"))
                        }
                        "doc-only" => format!(
                            "{} {file}: comments and layout only",
                            verb("would update", "updated")
                        ),
                        "unused-overwritten" => format!(
                            "{} {file}, which the estate does not use",
                            verb("would overwrite", "overwrote")
                        ),
                        "adopted-in-place" => format!(
                            "{} {file} in place{versions}; the estate keeps its name",
                            verb("would adopt", "adopted")
                        ),
                        "forked-and-repointed" => format!(
                            "{} {file} to {fork} and {} the estate at it{versions}; the adoption delta is {diff}",
                            verb("would fork", "forked"),
                            verb("point", "pointed")
                        ),
                        "fork-diff-refreshed" => format!(
                            "{file} moved upstream{versions}: {} the delta in {diff} beside {fork}",
                            verb("would refresh", "refreshed")
                        ),
                        "deferred" => format!(
                            "{file} is deferred: it needs a fork, which cannot share a run with --adopt; run merge-presets without it"
                        ),
                        "refused" => format!(
                            "{file} is refused: {}",
                            reason.as_deref().unwrap_or("satz gave no reason")
                        ),
                        "skipped-edited" => format!(
                            "{file} is skipped: it has upstream's version and other content, which is a local edit; name it to overwrite it"
                        ),
                        other => format!("{file}: {}", other.replace('-', " ")),
                    };
                    out.push(line);
                }
                MergeEvent::Warning { file, text } => out.push(format!("warning: {file}: {text}")),
                MergeEvent::Note { text } => out.push(text.clone()),
                MergeEvent::EmissionDelta { lines } => {
                    out.push("What the adoption changes in the emission:".to_string());
                    out.extend(lines.iter().map(|l| format!("  {l}")));
                    out.push(
                        "hcl/ is not regenerated by the merge: transpile, then plan, before applying."
                            .to_string(),
                    );
                }
                MergeEvent::Prerequisites {
                    wrote,
                    missing,
                    refused,
                } => {
                    out.extend(wrote.iter().map(|w| format!("prerequisite written: {w}")));
                    if let Some(why) = refused {
                        out.push(format!("prerequisites not written: {why}"));
                    }
                    let lead = verb("would declare", "still missing:");
                    out.extend(missing.iter().map(|m| format!("{lead} {m}")));
                }
            }
        }
        out.push(self.summary());
        for n in &self.notices {
            out.push(format!("notice from {}: {}", n.pack, n.text));
            out.push(format!("  run: {}", n.run));
        }
        out
    }

    /// One sentence: whether anything was written, and every outcome that took a pack.
    fn summary(&self) -> String {
        let c = &self.counts;
        let parts: Vec<String> = [
            (c.installed, "installed"),
            (c.artifacts_updated, "artifacts updated"),
            (c.doc_only, "comments and layout only"),
            (c.unused_overwritten, "unused and overwritten"),
            (c.adopted_in_place, "adopted in place"),
            (c.forked_and_repointed, "forked and repointed"),
            (c.fork_diffs_refreshed, "fork deltas refreshed"),
            (c.deferred, "deferred"),
            (c.refused, "refused"),
            (c.skipped_edited, "skipped as local edits"),
            (c.current, "current"),
        ]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, what)| format!("{n} {what}"))
        .collect();
        let counted = if parts.is_empty() {
            "no pack in the library".to_string()
        } else {
            parts.join(", ")
        };
        let lead = if self.report_only {
            "Report only, nothing written"
        } else {
            "Merged"
        };
        let attention = if self.attention {
            " Something above needs you."
        } else {
            ""
        };
        format!("{lead}: {counted}.{attention}")
    }
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

/// What `satz interfaces <estate> --format json` writes: what the estate publishes to the
/// projects that read it — every export with the interface it stands in, and every
/// interface with what it uses. satz's `InterfacesReport`
/// (`vendor/satz/src/interface_report.rs`), read off the same compile `transpile` runs.
/// No MCP tool serves it; the app reads it through the CLI (ADR 0023).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InterfacesReport {
    pub estate: String,
    /// every export, the core ones first, then each interface's in declaration order
    pub exports: Vec<ExportRow>,
    /// every declared interface; `core` is none of them — the core exports are the rows
    /// with no `interface`
    pub interfaces: Vec<InterfaceRow>,
    /// the lists a project may add entries to (`request`), and the shape of an entry
    pub requests: Vec<RequestRow>,
}

impl InterfacesReport {
    /// The core exports: the ones every interface carries.
    pub fn core(&self) -> impl Iterator<Item = &ExportRow> {
        self.exports.iter().filter(|e| e.interface.is_none())
    }

    /// The exports interface `name` declares itself.
    pub fn of<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a ExportRow> {
        self.exports
            .iter()
            .filter(move |e| e.interface.as_deref() == Some(name))
    }
}

/// How a project's module holds an export's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportHow {
    /// a literal
    Static,
    /// a data source the project's plan reads
    Lookup,
    /// `all <type>`: a map keyed by label
    Map,
}

impl ExportHow {
    pub fn as_str(self) -> &'static str {
        match self {
            ExportHow::Static => "static",
            ExportHow::Lookup => "lookup",
            ExportHow::Map => "map",
        }
    }
}

/// One export of the estate.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExportRow {
    pub name: String,
    /// the interface it stands in; absent for a core export, which every interface carries
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interface: Option<String>,
    pub how: ExportHow,
    /// what a project's module holds: the literal, or the data source expression
    pub value: String,
    /// the resource type of an `all` map
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub all: Option<String>,
    /// the estate's resources it names, by address
    pub targets: Vec<String>,
    /// the attachment types a project may create against it
    pub attach: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// where it is declared, as satz names the file
    pub file: String,
    pub line: usize,
}

impl ExportRow {
    /// The name `satz add-project --export` takes for it: `<interface>.<name>`; `None` for
    /// a core export, which satz refuses to copy.
    pub fn qualified(&self) -> Option<String> {
        self.interface
            .as_ref()
            .map(|i| format!("{i}.{}", self.name))
    }
}

/// One request point of `satz interfaces`: a list a project may add entries to.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RequestRow {
    /// the list param a project's `contributes_<param>` adds to
    pub param: String,
    /// the field that names an entry
    pub key: String,
    /// every field an entry may carry
    pub fields: Vec<String>,
    pub description: Option<String>,
    /// the entries the list holds now
    pub entries: usize,
    pub file: String,
    pub line: usize,
}

/// One declared interface.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InterfaceRow {
    pub name: String,
    /// in the library every project's folder carries: marked `common`, or declared in a pack
    pub common: bool,
    /// the interfaces its module also carries, directly or through another
    pub uses: Vec<String>,
    /// how many exports it declares itself
    pub exports: usize,
    pub file: String,
    pub line: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMOKE: &str = include_str!("../../tests/fixtures/questions-smoke.json");

    /// `satz_merge_presets {report_only: true}` of satz's smoke estate, recorded from the
    /// release the app is tested against, over a pristine library in which upstream
    /// added a file, moved a pack the estate uses to a new version and touched the
    /// comments of one it does not use. The diff path is written relative.
    const MERGE: &str = include_str!("../../tests/fixtures/merge-presets-smoke.json");

    #[test]
    fn the_recorded_merge_report_round_trips() {
        let report: MergeReport = serde_json::from_str(MERGE).unwrap();
        assert_eq!(report.counts.current, 98);
        let again: serde_json::Value = serde_json::to_value(&report).unwrap();
        let original: serde_json::Value = serde_json::from_str(MERGE).unwrap();
        assert_eq!(again, original);
    }

    /// The log reads the merge as sentences: what it would do to each pack it touches,
    /// the counts, and never a line of the JSON it came as.
    #[test]
    fn a_merge_reads_as_sentences_in_the_log() {
        let report: MergeReport = serde_json::from_str(MERGE).unwrap();
        let lines = report.lines();
        assert!(
            lines.iter().any(|l| l
                == "would fork essential-contacts-organization.satz to essential-contacts-organization.local.satz and point the estate at it (1.4 -> 1.5); the adoption delta is presets/essential-contacts-organization.diff.satz"),
            "{lines:#?}"
        );
        assert!(
            lines.iter().any(|l| l == "would install notes-new.md"),
            "{lines:#?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l == "would update cis/dns-logging.satz: comments and layout only"),
            "{lines:#?}"
        );
        assert_eq!(
            lines.last().unwrap(),
            "Report only, nothing written: 1 installed, 1 comments and layout only, 1 forked and repointed, 98 current. Something above needs you."
        );
        // a pack that is current says nothing: the counts carry it
        assert_eq!(lines.len(), 4, "{lines:#?}");
        for l in &lines {
            let t = l.trim_start();
            assert!(!t.starts_with('{') && !t.starts_with('['), "{l}");
        }
    }

    #[test]
    fn a_notice_a_merge_opened_names_the_command_to_run() {
        let mut report: MergeReport = serde_json::from_str(MERGE).unwrap();
        report.report_only = false;
        report.attention = false;
        report.notices.push(NoticeRow {
            param: "cis_baseline_adopted".into(),
            pack: "presets/cis/block-project-ssh-keys.satz".into(),
            text: "Import what is live first.".into(),
            run: "satz adopt <estate> --execute --import".into(),
            severity: FindingSeverity::Error,
            acknowledged: false,
        });
        let lines = report.lines();
        assert!(
            lines.iter().any(|l| l == "installed notes-new.md"),
            "{lines:#?}"
        );
        assert!(
            lines.iter().any(|l| l.starts_with("Merged: 1 installed")),
            "{lines:#?}"
        );
        assert!(
            lines.ends_with(&[
                "notice from presets/cis/block-project-ssh-keys.satz: Import what is live first."
                    .to_string(),
                "  run: satz adopt <estate> --execute --import".to_string(),
            ]),
            "{lines:#?}"
        );
    }

    #[test]
    fn a_merge_event_of_a_kind_the_app_does_not_read_fails_the_report() {
        let mut v: serde_json::Value = serde_json::from_str(MERGE).unwrap();
        v["events"][0]["kind"] = "rename".into();
        assert!(serde_json::from_value::<MergeReport>(v).is_err());
    }

    #[test]
    fn the_recorded_questions_report_round_trips() {
        let report: QuestionsReport = serde_json::from_str(SMOKE).unwrap();
        assert_eq!(report.summary.total, report.questions.len());
        let again: serde_json::Value = serde_json::to_value(&report).unwrap();
        let original: serde_json::Value = serde_json::from_str(SMOKE).unwrap();
        assert_eq!(again, original);
    }

    /// The recorded `satz questions --format json` of satz's showcase estate, which binds
    /// every option of its choice that is not required `false` and answers its
    /// `empty` question `""`.
    const SHOWCASE: &str = include_str!("../../tests/fixtures/questions-showcase.json");

    /// A choice that is not required is answered `none`, and a question whose `empty`
    /// says what `""` means takes `""` as an answer: both read out of what satz reports.
    #[test]
    fn a_choice_answered_none_and_an_empty_answer_read_as_satz_reports_them() {
        let report: QuestionsReport = serde_json::from_str(SHOWCASE).unwrap();
        let again: serde_json::Value = serde_json::to_value(&report).unwrap();
        let original: serde_json::Value = serde_json::from_str(SHOWCASE).unwrap();
        assert_eq!(again, original, "the showcase report round-trips");
        let by = |s: &str| {
            report
                .questions
                .iter()
                .find(|q| q.subject == s)
                .unwrap_or_else(|| panic!("the recorded report asks no `{s}`"))
        };
        let extras = by("optional_extras");
        assert_eq!(extras.state, QuestionState::Answered);
        assert!(!extras.required && extras.offers_none());
        assert_eq!(extras.bound_option(), Some(NO_BRANCH));
        assert_eq!(extras.bound_label().as_deref(), Some("None"));
        let model = by("group_model");
        assert!(model.required && !model.offers_none());
        assert_eq!(model.bound_option(), Some("group_model_flat"));
        let folder = by("team_folder_name");
        assert_eq!(folder.state, QuestionState::Answered);
        assert_eq!(folder.empty.as_deref(), Some("no team folder"));
        assert_eq!(folder.current, Some(serde_json::json!("")));
        assert_eq!(
            folder.shown(&serde_json::json!("")),
            "\"\" (no team folder)"
        );
        assert_eq!(folder.shown(&serde_json::json!("Team A")), "Team A");
    }

    #[test]
    fn an_answered_choice_that_sets_no_option_is_bound_to_none() {
        let mut q: QuestionRow = serde_json::from_value(serde_json::json!({
            "subject": "interface_notice", "kind": "oneof", "prompt": "p", "reversal": "edit",
            "blast": "none", "state": "answered", "blocking": false, "pack_description": "d",
            "options": [{"param": "interface_notice_pubsub", "label": "Pub/Sub", "selected": false}],
            "from": "f", "pack": "p"
        }))
        .unwrap();
        assert_eq!(q.bound_option(), Some(NO_BRANCH));
        assert_eq!(q.bound_label().as_deref(), Some("None"));
        q.options[0].selected = true;
        assert_eq!(q.bound_option(), Some("interface_notice_pubsub"));
        assert_eq!(q.bound_label().as_deref(), Some("Pub/Sub"));
        q.options[0].selected = false;
        q.required = true;
        assert_eq!(q.bound_option(), None, "a required choice has no none");
        q.required = false;
        q.state = QuestionState::Unanswered;
        assert_eq!(q.bound_option(), None, "an open choice binds nothing yet");
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
            "created": true, "written": 2, "rename_to": "C0example.satz", "notices": [],
            "estate": "x.satz", "questions": [], "summary": {"total": 0, "answered": 0, "unanswered": 0, "not_applicable": 0, "blocking": 0, "one_way_doors": 0, "complete": true}
        }))
        .unwrap();
        assert_eq!(r.rename_to.as_deref(), Some("C0example.satz"));
        assert!(r.report.summary.complete);
        assert!(r.notices.is_empty());
    }

    /// The notice a `satz_interview` call returned when the CIS pack went on, recorded
    /// from the pinned release.
    const OPENED: &str = r#"{
        "acknowledged": false,
        "pack": "presets/cis/block-project-ssh-keys.satz",
        "param": "cis_block_project_ssh_keys_adopted",
        "run": "satz adopt <estate> --execute --import",
        "severity": "error",
        "text": "Run satz adopt once the pack is on, so every live policy is in the state before the apply."
    }"#;

    #[test]
    fn a_notice_carries_the_command_to_run_and_the_param_that_acknowledges_it() {
        let n: NoticeRow = serde_json::from_str(OPENED).unwrap();
        assert_eq!(n.param, "cis_block_project_ssh_keys_adopted");
        assert_eq!(n.run, "satz adopt <estate> --execute --import");
        assert!(!n.acknowledged);
        assert!(n.holds_up_apply());
        assert_eq!(
            serde_json::to_value(&n).unwrap(),
            serde_json::from_str::<serde_json::Value>(OPENED).unwrap()
        );
    }

    /// A notice the app cannot read is a failed report, never a notice quietly dropped:
    /// the operator would never learn of the command the pack asks for.
    #[test]
    fn a_notice_field_the_app_cannot_read_fails_the_report() {
        let missing = serde_json::json!({
            "param": "p", "pack": "presets/p.satz", "text": "t", "acknowledged": false
        });
        assert!(
            serde_json::from_value::<NoticeRow>(missing).is_err(),
            "no `run`"
        );
        let unknown_before = serde_json::json!({
            "param": "p", "pack": "presets/p.satz", "text": "t", "run": "satz x",
            "before": "bootstrap", "acknowledged": false
        });
        assert!(serde_json::from_value::<NoticeRow>(unknown_before).is_err());
        let no_notices = serde_json::json!({
            "created": false, "written": 1,
            "estate": "x.satz", "questions": [], "summary": {"total": 0, "answered": 0, "unanswered": 0, "not_applicable": 0, "blocking": 0, "one_way_doors": 0, "complete": true}
        });
        assert!(
            serde_json::from_value::<InterviewReport>(no_notices).is_err(),
            "a satz that does not report notices is not one this app runs"
        );
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
            "severity": "info", "kind": "a-kind-satz-grew", "message": "m"
        }))
        .unwrap();
        assert_eq!(f.kind, "a-kind-satz-grew");
        assert_eq!(f.severity, FindingSeverity::Info);
        assert_eq!((f.file, f.line, f.group), (None, None, None));
        assert_eq!((f.subject, f.fix), (None, None));
    }

    /// `satz packs vendor/satz/tests/smoke/yaml/smoke.satz --format json`, recorded from
    /// the release the app is tested against over `tests/fixtures/smoke/config.toml`.
    const PACKS: &str = include_str!("../../tests/fixtures/packs-smoke.json");

    #[test]
    fn the_recorded_packs_report_round_trips() {
        let report: PacksReport = serde_json::from_str(PACKS).unwrap();
        assert!(report.note.is_none());
        assert_eq!(
            report.map().map(|m| m.path.as_str()),
            Some("presets/estate-map.satz")
        );
        assert!(report.packs.iter().any(|p| p.line == PackLine::Ungated));
        assert!(report.packs.iter().any(|p| !p.notices.is_empty()));
        assert!(report.findings.iter().all(|f| f.subject.is_some()));
        let again: serde_json::Value = serde_json::to_value(&report).unwrap();
        let original: serde_json::Value = serde_json::from_str(PACKS).unwrap();
        assert_eq!(again, original);
    }

    /// A field satz always sends and no longer does is a failed report, never a row read
    /// with a default the app made up.
    #[test]
    fn a_pack_row_without_a_field_satz_always_sends_fails() {
        let row = serde_json::json!({
            "path": "presets/organization-budget.satz", "role": "pack", "line": "absent",
            "deploys": false, "requires": [], "required_by": [], "excludes": [], "findings": []
        });
        assert!(serde_json::from_value::<PackRow>(row.clone()).is_ok());
        for field in [
            "line",
            "deploys",
            "requires",
            "required_by",
            "excludes",
            "findings",
        ] {
            let mut without = row.clone();
            without.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<PackRow>(without).is_err(),
                "a row without `{field}` was read"
            );
        }
        let mut unknown = row;
        unknown["line"] = serde_json::json!("sideways");
        assert!(serde_json::from_value::<PackRow>(unknown).is_err());
    }

    /// `satz review-pack <pack> --format json`, recorded from the release the app is tested
    /// against, over
    /// `tests/fixtures/smoke/config.toml`: a pack of satz's own library, which clears the
    /// bar, and `tests/fixtures/review/team-access.satz`, which does not. The absolute
    /// paths are written as `/e/…`.
    const REVIEW_CLEAN: &str = include_str!("../../tests/fixtures/review/organization-budget.json");
    const REVIEW_BROKEN: &str = include_str!("../../tests/fixtures/review/team-access.json");

    #[test]
    fn the_recorded_reviews_round_trip() {
        for recorded in [REVIEW_CLEAN, REVIEW_BROKEN] {
            let review: PackReview = serde_json::from_str(recorded).unwrap();
            let again: serde_json::Value = serde_json::to_value(&review).unwrap();
            let original: serde_json::Value = serde_json::from_str(recorded).unwrap();
            assert_eq!(again, original);
        }
    }

    #[test]
    fn a_review_passes_when_no_finding_is_an_error() {
        let clean: PackReview = serde_json::from_str(REVIEW_CLEAN).unwrap();
        assert!(clean.passed());
        assert_eq!(clean.folded_into, "synthetic");
        assert_eq!(clean.emits, ["google_billing_budget.global_budget"]);

        let broken: PackReview = serde_json::from_str(REVIEW_BROKEN).unwrap();
        assert!(!broken.passed());
        assert_eq!(broken.count(FindingSeverity::Error), 3);
        let membership = broken
            .findings
            .iter()
            .find(|f| f.message.starts_with("declares the membership"))
            .expect("the membership is a finding");
        assert_eq!(membership.severity, FindingSeverity::Error);
        assert_eq!(membership.kind, "pack");
        assert!(membership.line.is_some(), "anchored at its block");
        let unformatted = broken
            .findings
            .iter()
            .find(|f| f.message.starts_with("not formatted"))
            .expect("the layout is a finding");
        assert!(
            unformatted
                .fix
                .as_deref()
                .is_some_and(|f| f.starts_with("satz fmt ")),
            "{unformatted:?}"
        );
    }

    /// A review without a field satz always sends is a failed report, never a review read
    /// with nothing in it.
    #[test]
    fn a_review_without_a_field_satz_always_sends_fails() {
        let v: serde_json::Value = serde_json::from_str(REVIEW_BROKEN).unwrap();
        for field in ["pack", "folded_into", "emits", "findings"] {
            let mut without = v.clone();
            without.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<PackReview>(without).is_err(),
                "a review without `{field}` was read"
            );
        }
    }

    /// `satz interfaces showcase.satz --format json`, recorded from the release the app is
    /// tested against over `tests/fixtures/smoke/config.toml`: the smoke showcase's core
    /// exports, its common `audit` interface and `archive`, which uses it.
    const INTERFACES: &str = include_str!("../../tests/fixtures/interfaces-showcase.json");

    #[test]
    fn the_recorded_interfaces_report_round_trips() {
        let report: InterfacesReport = serde_json::from_str(INTERFACES).unwrap();
        assert!(report.core().any(|e| e.name == "workload_folder"));
        let audit = report
            .interfaces
            .iter()
            .find(|i| i.name == "audit")
            .unwrap();
        assert!(audit.common);
        assert_eq!(report.of("audit").count(), audit.exports);
        let archive = report
            .interfaces
            .iter()
            .find(|i| i.name == "archive")
            .unwrap();
        assert_eq!(archive.uses, ["audit"]);
        assert!(report.of("archive").any(|e| !e.attach.is_empty()));
        assert!(
            report
                .exports
                .iter()
                .any(|e| e.how == ExportHow::Map && e.all.is_some())
        );
        assert_eq!(
            report
                .of("archive")
                .find(|e| e.name == "archive_project_number")
                .and_then(ExportRow::qualified)
                .as_deref(),
            Some("archive.archive_project_number")
        );
        assert!(report.core().all(|e| e.qualified().is_none()));
        let again: serde_json::Value = serde_json::to_value(&report).unwrap();
        let original: serde_json::Value = serde_json::from_str(INTERFACES).unwrap();
        assert_eq!(again, original);
    }

    /// A field satz always sends is required, and a `how` satz adds fails the report.
    #[test]
    fn an_export_row_without_a_field_satz_always_sends_fails() {
        let row = serde_json::json!({
            "name": "x", "how": "static", "value": "\"v\"", "targets": [], "attach": [],
            "file": "e.satz", "line": 1
        });
        assert!(serde_json::from_value::<ExportRow>(row.clone()).is_ok());
        for field in ["name", "how", "value", "targets", "attach", "file", "line"] {
            let mut without = row.clone();
            without.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<ExportRow>(without).is_err(),
                "a row without `{field}` was read"
            );
        }
        let mut unknown = row;
        unknown["how"] = serde_json::json!("sideways");
        assert!(serde_json::from_value::<ExportRow>(unknown).is_err());
        let iface = serde_json::json!({
            "name": "audit", "common": true, "uses": [], "exports": 1, "file": "e.satz", "line": 2
        });
        for field in ["name", "common", "uses", "exports", "file", "line"] {
            let mut without = iface.clone();
            without.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<InterfaceRow>(without).is_err(),
                "an interface without `{field}` was read"
            );
        }
    }

    #[test]
    fn pack_switch_args_send_only_what_is_set() {
        let add = AddPackArgs {
            pack: "presets/organization-budget.satz".to_string(),
            with_requirements: false,
        };
        assert_eq!(
            serde_json::to_value(&add).unwrap(),
            serde_json::json!({"pack": "presets/organization-budget.satz"})
        );
        let remove = RemovePackArgs {
            pack: "use_budget".to_string(),
            cascade: true,
        };
        assert_eq!(
            serde_json::to_value(&remove).unwrap(),
            serde_json::json!({"pack": "use_budget", "cascade": true})
        );
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
