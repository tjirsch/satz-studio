//! The lossless document layer: a concrete syntax tree of one `.satz` file with byte
//! spans and every comment kept, over the tree-sitter grammar of Satz
//! (`vendor/satz-tree-sitter`, the grammar maintained beside satz — see
//! `docs/adr/0003-*.md`). `text()` IS the file; a node's span indexes into it, so
//! reproducing the file is the identity and not a re-emission. Semantics stay with
//! satz-core: [`Cst::lower`] parses the same text with `satz_core::satz::parse`.
//!
//! The pack lines an estate carries (`use "…" when <gate>`, commented out until a
//! question is answered yes) are load-bearing comments; [`scan_uses`] recognises them.

pub mod grammar;

use satz_core::satz::{File, SatzError};

pub type NodeId = usize;

/// Byte offsets into [`Cst::text`], end exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn len(&self) -> usize {
        self.end - self.start
    }
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Str,
    Num,
    Bool,
    /// a bare identifier: a param reference
    Ref,
    List,
    Obj,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// `estate NAME` or `pack NAME version "…"`
    Header { keyword: String, name: String },
    /// the `params { … }` block
    Params,
    /// `name = value` inside `params`
    ParamEntry { name: Span, eq: Span, value: NodeId },
    /// `use "path" [as key] [when gate]`, active
    Use { path: Span, as_key: Option<Span>, when: Option<Span> },
    /// `KEY [NAME] { … }` — a resource map, a named entry, a nested mapping
    Block { key: Span, name: Option<Span>, open: Span, close: Span },
    /// `key = value` inside a block
    Attr { key: Span, eq: Span, value: NodeId },
    Value(ValueKind),
    /// `claim`, `question`, `action`, `suppress`, `hcl` — kept whole in V1
    Opaque { statement: String },
    /// the grammar could not read this range; the file still shows
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub kind: NodeKind,
    pub span: Span,
    /// 1-based, as satz counts
    pub line: u32,
    pub children: Vec<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cst {
    text: String,
    nodes: Vec<Node>,
    root: NodeId,
}

#[derive(Debug, thiserror::Error)]
pub enum CstError {
    #[error("line {line}: {message}")]
    Parse { line: u32, message: String },
    #[error(transparent)]
    Unimplemented(#[from] crate::Unimplemented),
}

impl Cst {
    /// Parse a file. Never fails on text satz accepts; text satz rejects still yields a
    /// tree with [`NodeKind::Error`] nodes, so the editor can show the file.
    pub fn parse(text: &str) -> Result<Cst, CstError> {
        let _ = text;
        Err(crate::Unimplemented::new("Cst::parse", "U2").into())
    }

    /// The file, byte for byte.
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn root(&self) -> NodeId {
        self.root
    }
    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }
    pub fn nodes(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        self.nodes.iter().enumerate()
    }
    pub fn slice(&self, span: Span) -> &str {
        &self.text[span.start..span.end]
    }
    /// Every node whose span starts on this line, outermost first.
    pub fn nodes_at_line(&self, line: u32) -> Vec<NodeId> {
        self.nodes.iter().enumerate().filter(|(_, n)| n.line == line).map(|(i, _)| i).collect()
    }
    /// The `params { … }` block, if the file has one.
    pub fn params(&self) -> Option<NodeId> {
        self.nodes.iter().position(|n| n.kind == NodeKind::Params)
    }
    /// The entry binding `name` inside `params { … }`.
    pub fn param(&self, name: &str) -> Option<NodeId> {
        let params = self.params()?;
        self.node(params).children.iter().copied().find(|&id| match &self.node(id).kind {
            NodeKind::ParamEntry { name: n, .. } => self.slice(*n) == name,
            _ => false,
        })
    }

    /// The same text through satz-core's parser — the authority on what it means.
    pub fn lower(&self) -> Result<File, SatzError> {
        satz_core::satz::parse(&self.text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseState {
    Active,
    /// `// use "…" when gate` — the OFF state of a pack line, as satz writes it
    Commented,
}

/// One `use` line, active or commented, with the phase comment above it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseLine {
    pub path: String,
    pub gate: Option<String>,
    pub as_key: Option<String>,
    pub state: UseState,
    /// the whole line without its newline
    pub span: Span,
    pub line: u32,
    /// the contiguous comment block directly above, minus the `// ` prefixes
    pub phase_comment: Option<String>,
}

/// Every `use` line of the file, in order: the active ones from the tree, the commented
/// ones from the comment nodes that have the exact shape satz's `pack_line` writes and
/// its `uncomment_pack` matches (`// use "` … ` when <gate>`).
pub fn scan_uses(cst: &Cst) -> Vec<UseLine> {
    let _ = cst;
    Vec::new()
}

/// A value the editor writes, typed.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedValue {
    /// a plain string: quoted and escaped on write (`\"`, `\\`, `{{`)
    Str(String),
    Num(String),
    Bool(bool),
    /// a bare param reference
    Ref(String),
    List(Vec<TypedValue>),
    /// raw Satz source for a value, written as is (source mode)
    Raw(String),
}

/// The style of the neighbourhood a value is written into.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StyleCtx {
    /// the indentation of the line the value sits on
    pub indent: String,
    /// the old list had one item per line
    pub list_multiline: bool,
    /// the old list ended its last item with a comma
    pub trailing_comma: bool,
}

/// Render a value as Satz source in the given style, escaping as satz's own writer does.
pub fn render_value(value: &TypedValue, ctx: &StyleCtx) -> String {
    let _ = (value, ctx);
    String::new()
}
