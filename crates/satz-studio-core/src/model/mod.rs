//! The view model the app binds to, built pure from the document tree, the questions
//! report, the resolved params and the provider schema, and rebuilt after every commit.
//! Nothing here is guessed: a key the schema does not know is `Unknown` and read-only,
//! a gate without a line is `Absent`, a computed attribute is locked.
//!
//! Three walks over one file, each in its own module: [`outline`] classifies the blocks
//! as satz's `EstateResolver` and `split_body` do, [`params`] reads the `params { }`
//! block beside the questions report, and [`packs`] derives the pack rows from the
//! `use` lines, the report and the resolved params — no copy of satz's `PACK_LINES`.
//! [`value`] decodes a value the way satz's lexer reads it.

mod outline;
mod packs;
mod params;
mod value;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use satz_core::pipeline::Env;

use crate::cst::{Cst, NodeId, UseLine, scan_uses};
use crate::diag::Diagnostic;
use crate::satz::reports::{QuestionRow, QuestionsReport};
use crate::schema::{AttrType, ResourceRegistry};

pub use value::{decode_string, truthy};

/// The map pack: the `use` line every other pack line's question is declared behind.
pub const MAP_PATH: &str = "presets/estate-map.satz";
/// The map pack's declared name, as the questions report's `pack` column carries it.
pub const MAP_PACK: &str = "estate_map";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    /// `terraform { }`, `providers { }`, and every block inside them
    Config,
    /// `google_folder { … }` — a map of named resources of one type
    ResourceMap,
    /// `infra { … }` under a map, or `google_folder infra { … }` — one resource
    Resource,
    /// a nested block the schema declares (`versioning { }`), an attribute written in
    /// block form (`labels { }`), or Terraform's `lifecycle { }`
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
    /// the block's key as written, quotes stripped: the type of a `ResourceMap`, the
    /// name of a `Resource` under a map, the key of a `NestedBlock`, the member of a
    /// `MemberGrant`
    pub key: String,
    /// `key` decoded: one `Lit` for an identifier, the parts of a string key
    pub key_parts: Vec<StrPart>,
    /// the block's name when it has one (`TYPE NAME { }`), as written
    pub label: Option<String>,
    /// `label` decoded
    pub label_parts: Option<Vec<StrPart>>,
    /// the Terraform type of a `ResourceMap`, a `Resource` or a `MemberGrant`
    pub tf_type: Option<String>,
    pub line: u32,
    pub attrs: Vec<AttrRow>,
    pub children: Vec<ResourceNode>,
    /// the `use` lines inside this block and not inside a block below it
    pub uses: Vec<UseLine>,
    /// the schema's required attributes and required blocks (`min_items` ≥ 1) that have
    /// neither a row nor a child here, minus what satz derives from the position (a
    /// folder's `parent`, a project's `folder_id`, a group's `group_key`); sorted
    pub missing_required: Vec<String>,
}

impl ResourceNode {
    /// A `Resource`'s name: the block name of `TYPE NAME { }`, else its key under a map.
    pub fn name(&self) -> Option<&str> {
        match self.kind {
            ResourceKind::Resource => Some(self.label.as_deref().unwrap_or(&self.key)),
            _ => None,
        }
    }
}

/// A value as the file has it, decoded for display.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceValue {
    /// `raw` is the text between the quotes as written; `parts` is what satz reads
    Str {
        raw: String,
        parts: Vec<StrPart>,
    },
    Num(String),
    Bool(bool),
    /// a bare param reference and what it resolves to
    Ref {
        param: String,
        resolved: Option<serde_json::Value>,
    },
    List(Vec<SourceValue>),
    Obj,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StrPart {
    Lit(String),
    /// `{param}` and its resolved value
    Param {
        name: String,
        resolved: Option<serde_json::Value>,
    },
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
    /// the key as written, quotes stripped
    pub key: String,
    pub value: SourceValue,
    pub typed: AttrType,
    pub required: bool,
    pub optional: bool,
    pub computed: bool,
    /// false for computed-only attributes, `"import-id"`, and every row of an
    /// `Unknown` block
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
    Bool {
        current: Option<bool>,
        default: Option<bool>,
    },
    /// one option of a `oneof` group
    OneofOption { group: String, selected: bool },
    /// the map line: no param gates it, the line itself is the switch
    Line,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackRowKind {
    /// the map line, `presets/estate-map.satz`
    Map,
    /// a `use … when <gate>` line, or a gate the file has no line for
    Choice,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PackRow {
    pub kind: PackRowKind,
    /// the param that gates the line; `None` for the map row
    pub gate: Option<String>,
    /// the pack the line names; `None` for an `Absent` choice
    pub path: Option<String>,
    pub state: LineState,
    pub choice: Choice,
    /// the question whose subject is the gate, or the `oneof` one of whose options is;
    /// `None` when no pack the estate uses asks it
    pub question: Option<QuestionRow>,
    /// the comment block above the line: the phase it can be adopted in
    pub phase: Option<String>,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SchemaStatus {
    /// the providers the schema files came from, and how many resource types
    Loaded {
        providers: Vec<String>,
        resources: usize,
    },
    /// `schema_dir` holds no schema: the Resources view is read-only until
    /// `satz update-schema`
    Missing(PathBuf),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EstateModel {
    pub main: PathBuf,
    pub outline: Vec<ResourceNode>,
    pub params: Vec<ParamRow>,
    pub packs: Vec<PackRow>,
    /// the `use` lines outside every block; the ones inside a block are on its node
    pub uses: Vec<UseLine>,
    pub diagnostics: Vec<Diagnostic>,
    pub schema: SchemaStatus,
}

/// A value the grammar accepted and satz's lexer rules cannot read — a disagreement
/// between the vendored grammar and [`decode_string`], never a property of the text.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("line {line}: {message}")]
    Decode { line: u32, message: String },
}

impl EstateModel {
    /// The model of `main`, whose parsed text is `cst`. `schema` is the registry, or
    /// the directory it is missing from; `env` the resolved params
    /// (`EstateDir::params`); `questions` the report of `satz questions`; `diagnostics`
    /// what the parse and the compile said, which the model's own notes join.
    pub fn build(
        main: &Path,
        cst: &Cst,
        schema: Result<&ResourceRegistry, &Path>,
        env: &Env,
        questions: &QuestionsReport,
        mut diagnostics: Vec<Diagnostic>,
    ) -> Result<EstateModel, ModelError> {
        let uses = scan_uses(cst);
        let (outline, top_uses) = outline::build(cst, schema.ok(), env, &uses)?;
        let (packs, notes) = packs::build(main, env, questions, &uses);
        let gates: BTreeSet<&str> = packs
            .iter()
            .filter_map(|r| r.gate.as_deref())
            .chain(
                questions
                    .questions
                    .iter()
                    .flat_map(|q| q.options.iter().map(|o| o.param.as_str())),
            )
            .collect();
        let params = params::build(cst, env, questions, &gates)?;
        diagnostics.extend(notes);
        let schema = match schema {
            Ok(registry) => SchemaStatus::Loaded {
                providers: registry.providers(),
                resources: registry.resources.len(),
            },
            Err(dir) => SchemaStatus::Missing(dir.to_path_buf()),
        };
        Ok(EstateModel {
            main: main.to_path_buf(),
            outline,
            params,
            packs,
            uses: top_uses,
            diagnostics,
            schema,
        })
    }
}
