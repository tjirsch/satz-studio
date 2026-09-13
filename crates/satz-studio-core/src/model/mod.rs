//! The view model the app binds to, built pure from the document tree, the questions
//! report, the resolved params and the provider schema, and rebuilt after every commit.
//! Nothing here is guessed: a key the schema does not know is `Unknown` and read-only,
//! a gate without a line is `Absent`, a computed attribute is locked.

use std::path::PathBuf;

use crate::cst::{Cst, NodeId};
use crate::diag::Diagnostic;
use crate::satz::reports::{QuestionRow, QuestionsReport};
use crate::schema::{AttrType, ResourceRegistry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    /// `terraform { }`, `providers { }`
    Config,
    /// `google_folder { … }` — a map of named resources of one type
    ResourceMap,
    /// `infra { … }` under a map — one resource
    Resource,
    /// a nested block the schema declares (`versioning { }`)
    NestedBlock,
    /// `"group:…" = [roles]` inside a grant type
    MemberGrant,
    /// not in the provider schema — shown, never edited
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResourceNode {
    pub id: NodeId,
    pub kind: ResourceKind,
    pub key: String,
    pub label: Option<String>,
    pub line: u32,
    pub attrs: Vec<AttrRow>,
    pub children: Vec<ResourceNode>,
    /// the `use` lines inside this block
    pub uses: Vec<crate::cst::UseLine>,
}

/// A value as the file has it, decoded for display.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceValue {
    Str { raw: String, parts: Vec<StrPart> },
    Num(String),
    Bool(bool),
    /// a bare param reference and what it resolves to
    Ref { param: String, resolved: Option<serde_json::Value> },
    List(Vec<SourceValue>),
    Obj,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StrPart {
    Lit(String),
    /// `{param}` and its resolved value
    Param { name: String, resolved: Option<serde_json::Value> },
    /// `${…}` — a Terraform reference
    TfRef(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    /// a plain value; braces are refused
    Value,
    /// raw Satz source between the quotes; interpolations and references live here
    Source,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttrRow {
    pub id: NodeId,
    pub key: String,
    pub value: SourceValue,
    pub typed: AttrType,
    pub required: bool,
    pub optional: bool,
    pub computed: bool,
    /// false for computed-only attributes, `"import-id"`, and unknown keys
    pub editable: bool,
    pub mode: EditMode,
    pub line: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    Bool,
    Number,
    List,
    String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParamRow {
    pub id: NodeId,
    pub name: String,
    pub value: SourceValue,
    pub kind: ParamKind,
    pub question: Option<QuestionRow>,
    pub one_way_door: bool,
    pub mode: EditMode,
    pub line: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineState {
    On,
    Off,
    /// the gate is declared but the estate has no line for it: run merge-presets
    Absent,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Choice {
    Bool { current: Option<bool>, default: Option<bool> },
    /// one option of a `oneof` group
    OneofOption { group: String, selected: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PackRow {
    pub gate: String,
    pub path: Option<String>,
    pub state: LineState,
    pub choice: Choice,
    pub question: QuestionRow,
    pub phase: Option<String>,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SchemaStatus {
    Loaded { provider: String, version: String, resources: usize },
    /// `schema_dir` is empty: the Resources view is read-only until `satz update-schema`
    Missing(PathBuf),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EstateModel {
    pub main: PathBuf,
    pub outline: Vec<ResourceNode>,
    pub params: Vec<ParamRow>,
    pub packs: Vec<PackRow>,
    pub diagnostics: Vec<Diagnostic>,
    pub schema: SchemaStatus,
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error(transparent)]
    Cst(#[from] crate::cst::CstError),
    #[error(transparent)]
    Unimplemented(#[from] crate::Unimplemented),
}

impl EstateModel {
    pub fn build(
        main: &std::path::Path,
        cst: &Cst,
        registry: Option<&ResourceRegistry>,
        env: &satz_core::pipeline::Env,
        questions: &QuestionsReport,
        diagnostics: Vec<Diagnostic>,
    ) -> Result<EstateModel, ModelError> {
        let _ = (main, cst, registry, env, questions, diagnostics);
        Err(crate::Unimplemented::new("EstateModel::build", "U5").into())
    }
}
