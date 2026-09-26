//! The lossless document layer: a concrete syntax tree of one `.satz` file with byte
//! spans and every comment kept, over the tree-sitter grammar of Satz
//! (`vendor/satz-tree-sitter`, the grammar maintained beside satz — see
//! `docs/adr/0003-*.md`). `text()` IS the file; a node's span indexes into it, so
//! reproducing the file is the identity and not a re-emission. Semantics stay with
//! satz-core: [`Cst::lower`] parses the same text with `satz_core::satz::parse`.
//!
//! The pack lines an estate carries (`use "…" when <gate>`, commented out until a
//! question is answered yes) are load-bearing comments; [`scan_uses`] recognises them.

mod build;
pub mod grammar;
mod render;
mod uses;

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

/// What a node is. Ids are assigned in pre-order, so a parent's id is smaller than its
/// children's and the node vector is in document order. Keywords and punctuation are not
/// nodes; where a node needs one (`=`, `{`, `}`) it carries the span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// the root: the whole file
    Document,
    /// `estate NAME`, `pack NAME version "…"` or `interface "NAME"` (a generated
    /// interface file; `name` without the quotes); a `version` string is a `Value` child
    Header { keyword: String, name: String },
    /// the `params { … }` block
    Params,
    /// `name = value` inside `params`
    ParamEntry { name: Span, eq: Span, value: NodeId },
    /// `use "path" [as key] [when gate]`, active; `path` spans the quotes
    Use {
        path: Span,
        as_key: Option<Span>,
        when: Option<Span>,
    },
    /// `KEY [NAME] { … }` — a resource map, a named entry, a nested mapping; the body's
    /// entries are the children
    Block {
        key: Span,
        name: Option<Span>,
        open: Span,
        close: Span,
    },
    /// `key = value` inside a block
    Attr { key: Span, eq: Span, value: NodeId },
    /// a list's items and an object's entries are the children; the scalars are leaves
    Value(ValueKind),
    /// `claim`, `question`, `action`, `notice`, `offers`, `export`, `interface`,
    /// `suppress`, `private`, `request`, `hcl`, and an `each` entry — kept whole in V1; the comments
    /// inside are still children
    Opaque { statement: String },
    /// a `//`, `#` or `/* … */` comment, wherever it stands
    Comment,
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

/// A node kind the vendored grammar produces and this layer does not map — a
/// disagreement between the two, never a property of the text.
#[derive(Debug, thiserror::Error)]
pub enum CstError {
    #[error("line {line}: {message}")]
    Parse { line: u32, message: String },
}

impl Cst {
    /// Parse a file. Never fails on text satz accepts; text satz rejects still yields a
    /// tree with [`NodeKind::Error`] nodes, so the editor can show the file. The error
    /// case is a node kind the vendored grammar produces and this layer does not map.
    pub fn parse(text: &str) -> Result<Cst, CstError> {
        let tree = grammar::parser()
            .parse(text, None)
            .ok_or_else(|| CstError::Parse {
                line: 1,
                message: "tree-sitter produced no tree for this text".to_string(),
            })?;
        let nodes = build::build(text, tree.root_node())?;
        Ok(Cst {
            text: text.to_string(),
            nodes,
            root: 0,
        })
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
    /// Every node in document order, outermost first.
    pub fn nodes(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        self.nodes.iter().enumerate()
    }
    pub fn slice(&self, span: Span) -> &str {
        &self.text[span.start..span.end]
    }
    /// Every node whose span starts on this line, outermost first.
    pub fn nodes_at_line(&self, line: u32) -> Vec<NodeId> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.line == line)
            .map(|(i, _)| i)
            .collect()
    }
    /// The `params { … }` block, if the file has one.
    pub fn params(&self) -> Option<NodeId> {
        self.nodes.iter().position(|n| n.kind == NodeKind::Params)
    }
    /// The entry binding `name` inside `params { … }`.
    pub fn param(&self, name: &str) -> Option<NodeId> {
        let params = self.params()?;
        self.node(params)
            .children
            .iter()
            .copied()
            .find(|&id| match &self.node(id).kind {
                NodeKind::ParamEntry { name: n, .. } => self.slice(*n) == name,
                _ => false,
            })
    }

    /// The same text through satz-core's parser — the authority on what it means.
    pub fn lower(&self) -> Result<File, SatzError> {
        satz_core::satz::parse(&self.text)
    }
}

/// The byte offset where the line containing `at` starts.
fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |i| i + 1)
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
    /// the text between the quotes
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
/// its `uncomment_pack` matches (`// use "` … ` when <gate>`). A comment that carries
/// anything else after the path (an `as` key excepted) is not a pack line.
pub fn scan_uses(cst: &Cst) -> Vec<UseLine> {
    uses::scan(cst)
}

/// A value the editor writes, typed.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedValue {
    /// a plain string: quoted and escaped on write (`\"`, `\\`, `\n`, `{{`, `}}`)
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

/// Render a value as Satz source in the given style, escaping so that satz's lexer reads
/// back exactly the string given: `\` as `\\`, `"` as `\"`, a newline as `\n`, `{` as
/// `{{` and `}` as `}}` (the lexer reads a doubled brace as one).
pub fn render_value(value: &TypedValue, ctx: &StyleCtx) -> String {
    render::render(value, ctx)
}

/// The style of the line a node sits on: its indentation and, for a list (or an entry
/// whose value is a list), whether the list spans lines and whether its last item
/// carries a comma.
pub fn style_of(cst: &Cst, node: NodeId) -> StyleCtx {
    render::style_of(cst, node)
}
