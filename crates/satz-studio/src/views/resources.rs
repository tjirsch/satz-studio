//! The Resources view: the outline as a tree, and the selected node's attributes as
//! typed fields. A commit is one `Edit::ReplaceValue` through the app's own writer.
//! Without a provider schema every row is untyped and locked until `update-schema`.

use dioxus::prelude::*;
use satz_studio_core::cst::{NodeId, TypedValue};
use satz_studio_core::edit::Edit;
use satz_studio_core::model::{AttrRow, EditMode, ResourceKind, ResourceNode, SchemaStatus};

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, Draft, FieldKind, Icon, IconButton,
    SourceChips, TextField, Tree, TreeItem, TypedField,
};
use crate::state::{
    AppStore, AppStoreStoreExt, DiagnosticSelection, EstateAction, EstateStoreStoreExt,
};
use crate::views::{line_text, value_source};

/// Branches deeper than this start collapsed.
const OPEN_DEPTH: usize = 2;

/// The node a line belongs to: the node one of whose attributes is on that line, else
/// the last node in document order that starts on or before it — the innermost block
/// open at that line, since a child starts after its parent.
pub fn node_at_line(outline: &[ResourceNode], line: u32) -> Option<NodeId> {
    let mut best: Option<(u32, NodeId)> = None;
    fn walk(nodes: &[ResourceNode], line: u32, best: &mut Option<(u32, NodeId)>) -> Option<NodeId> {
        for n in nodes {
            if n.attrs.iter().any(|a| a.line == line) {
                return Some(n.id);
            }
            if n.line <= line && best.is_none_or(|(l, _)| n.line >= l) {
                *best = Some((n.line, n.id));
            }
            if let Some(id) = walk(&n.children, line, best) {
                return Some(id);
            }
        }
        None
    }
    walk(outline, line, &mut best).or(best.map(|(_, id)| id))
}

/// The node with this id, anywhere in the outline.
pub fn find_node(outline: &[ResourceNode], id: NodeId) -> Option<&ResourceNode> {
    for n in outline {
        if n.id == id {
            return Some(n);
        }
        if let Some(found) = find_node(&n.children, id) {
            return Some(found);
        }
    }
    None
}

fn kind_icon(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Config => "settings",
        ResourceKind::ResourceMap => "category",
        ResourceKind::Resource => "deployed_code",
        ResourceKind::NestedBlock => "data_object",
        ResourceKind::MemberGrant => "person",
        ResourceKind::Unknown => "help",
    }
}

fn kind_label(kind: ResourceKind) -> &'static str {
    match kind {
        ResourceKind::Config => "configuration",
        ResourceKind::ResourceMap => "resource map",
        ResourceKind::Resource => "resource",
        ResourceKind::NestedBlock => "nested block",
        ResourceKind::MemberGrant => "member grant",
        ResourceKind::Unknown => "not in the schema",
    }
}

/// The tree row's label: a resource's name, everything else its key.
fn label_of(node: &ResourceNode) -> String {
    node.name().unwrap_or(&node.key).to_string()
}

fn supporting_of(node: &ResourceNode) -> String {
    match node.kind {
        ResourceKind::Resource | ResourceKind::MemberGrant => {
            node.tf_type.clone().unwrap_or_default()
        }
        ResourceKind::ResourceMap => match node.children.len() {
            1 => "1 resource".to_string(),
            n => format!("{n} resources"),
        },
        ResourceKind::Config | ResourceKind::NestedBlock | ResourceKind::Unknown => String::new(),
    }
}

/// Why a row is locked, for the reader.
fn lock_reason(row: &AttrRow, block_editable: bool) -> Option<&'static str> {
    if row.editable {
        return None;
    }
    Some(if row.key == "import-id" {
        "written by satz adopt"
    } else if row.computed && !row.optional && !row.required {
        "computed by the provider"
    } else if !block_editable {
        "not in the schema"
    } else {
        "read-only"
    })
}

#[component]
pub fn ResourcesView() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let selection = use_context::<DiagnosticSelection>();
    let model = app.estate().model().cloned();
    let cst = app.estate().cst().cloned();
    let loading = app.estate().loading().cloned();
    let mut selected = use_signal(|| None::<NodeId>);

    // The drawer's selection, when it names a line of the main file, selects the node
    // there. The model is peeked, not read: a reload must not move a selection the
    // reader made in the tree since.
    use_effect(move || {
        let picked = (selection.0)();
        let Some(d) = picked else { return };
        let Some(line) = d.line else { return };
        let Some(m) = app.estate().model().peek().clone() else {
            return;
        };
        if d.file.as_deref().is_some_and(|f| f != m.main.as_path()) {
            return;
        }
        if let Some(id) = node_at_line(&m.outline, line) {
            selected.set(Some(id));
        }
    });

    let (Some(model), Some(cst)) = (model, cst) else {
        return rsx! {
            div { class: "view resources",
                h1 { class: "view__title", "Resources" }
                Card { variant: CardVariant::Filled, class: "resources__empty",
                    Icon { name: "account_tree", size: 48, class: "placeholder__icon" }
                    p { "The estate model is not available — the drawer says why." }
                }
            }
        };
    };
    let missing_schema = match &model.schema {
        SchemaStatus::Missing(dir) => Some(dir.display().to_string()),
        SchemaStatus::Loaded { .. } => None,
    };
    let read_only = missing_schema.is_some();
    let node = selected().and_then(|id| find_node(&model.outline, id).cloned());
    let value_sources: Vec<(String, String)> = node
        .as_ref()
        .map(|n| {
            n.attrs
                .iter()
                .map(|a| {
                    (
                        value_source(&cst, a.id).unwrap_or_default(),
                        line_text(&cst, a.line),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    rsx! {
        div { class: "view resources",
            div { class: "resources__head",
                h1 { class: "view__title", "Resources" }
                span { class: "grow" }
                if let Some(dir) = &missing_schema {
                    div { class: "resources__no-schema", role: "alert",
                        Icon { name: "schema", size: 20 }
                        span { "No provider schema in " code { "{dir}" } ": the rows are untyped and locked. Run update-schema." }
                        Button { variant: ButtonVariant::Tonal, icon: "download", disabled: loading, onclick: move |_| handle.send(EstateAction::RunCommand(vec!["update-schema".to_string()])), "Run update-schema" }
                    }
                }
            }
            div { class: "resources__panes",
                Card { variant: CardVariant::Filled, class: "resources__tree",
                    if model.outline.is_empty() {
                        p { class: "resources__none", "The file declares no block." }
                    }
                    Tree {
                        for n in model.outline.iter().cloned() {
                            NodeItem { key: "{n.id}", node: n, depth: 0, selected: selected(), onselect: move |id: NodeId| selected.set(Some(id)) }
                        }
                    }
                }
                div { class: "resources__detail",
                    match node {
                        Some(n) => rsx! {
                            NodeDetail { key: "{n.id}:{n.line}", node: n, value_sources, read_only, loading }
                        },
                        None => rsx! {
                            Card { variant: CardVariant::Outlined, class: "resources__empty",
                                Icon { name: "account_tree", size: 48, class: "placeholder__icon" }
                                p { "Select a block in the tree to see its attributes." }
                            }
                        },
                    }
                }
            }
        }
    }
}

#[component]
fn NodeItem(
    node: ResourceNode,
    depth: usize,
    selected: Option<NodeId>,
    onselect: EventHandler<NodeId>,
) -> Element {
    let id = node.id;
    let is_branch = !node.children.is_empty() || !node.uses.is_empty();
    let missing = node.missing_required.len();
    let label = label_of(&node);
    let supporting = supporting_of(&node);
    let trailing = (missing > 0).then(|| {
        rsx! {
            Chip { kind: ChipKind::Assist, icon: "warning", label: format!("{missing} required missing"), error: true, class: "resources__missing-chip" }
        }
    });
    if is_branch {
        rsx! {
            TreeItem {
                label,
                icon: kind_icon(node.kind),
                supporting,
                open: depth < OPEN_DEPTH,
                selected: selected == Some(id),
                trailing,
                onclick: move |_| onselect.call(id),
                for u in node.uses.iter() {
                    TreeItem { key: "use-{u.line}", label: u.path.clone(), icon: "extension", supporting: u.gate.clone().map(|g| format!("when {g}")).unwrap_or_default() }
                }
                for c in node.children.iter().cloned() {
                    NodeItem { key: "{c.id}", node: c, depth: depth + 1, selected, onselect: move |id: NodeId| onselect.call(id) }
                }
            }
        }
    } else {
        rsx! {
            TreeItem {
                label,
                icon: kind_icon(node.kind),
                supporting,
                selected: selected == Some(id),
                trailing,
                onclick: move |_| onselect.call(id),
            }
        }
    }
}

/// The selected block: its kind, type and line, what it lacks, and one row per
/// attribute with the value's source text and the line it stands on.
#[component]
fn NodeDetail(
    node: ResourceNode,
    value_sources: Vec<(String, String)>,
    read_only: bool,
    loading: bool,
) -> Element {
    let block_editable = !matches!(node.kind, ResourceKind::Unknown);
    let title = label_of(&node);
    rsx! {
        Card { variant: CardVariant::Outlined, class: "node-detail",
            div { class: "node-detail__head",
                Icon { name: kind_icon(node.kind), size: 24, class: "placeholder__icon" }
                div { class: "node-detail__titles",
                    h2 { class: "node-detail__title", "{title}" }
                    span { class: "node-detail__subtitle",
                        "{kind_label(node.kind)}"
                        if let Some(t) = &node.tf_type { " · " code { "{t}" } }
                        " · line {node.line}"
                    }
                }
            }
            if !node.missing_required.is_empty() {
                div { class: "node-detail__missing", role: "alert",
                    Icon { name: "warning", size: 20 }
                    span {
                        "Required and not written: "
                        for (i, m) in node.missing_required.iter().enumerate() {
                            code { key: "{m}", "{m}" }
                            if i + 1 < node.missing_required.len() { ", " }
                        }
                        ". Adding a row is not part of this version; write it in the file."
                    }
                }
            }
            if node.attrs.is_empty() {
                p { class: "node-detail__none", "No attribute on this block; its blocks are in the tree." }
            }
            for (row, (source, line)) in node.attrs.iter().cloned().zip(value_sources.into_iter()) {
                AttrRowView { key: "{row.key}:{row.line}:{source}", row, source, line, block_editable, read_only, loading }
            }
        }
    }
}

#[component]
fn AttrRowView(
    row: AttrRow,
    source: String,
    line: String,
    block_editable: bool,
    read_only: bool,
    loading: bool,
) -> Element {
    let handle = use_coroutine_handle::<EstateAction>();
    let kind = FieldKind::of_attr(&row.typed);
    let value_draft = kind.and_then(|k| Draft::of_source(&row.value, k));
    let mut mode = use_signal(|| row.mode);
    let mut show_raw = use_signal(|| false);
    let mut raw = use_signal(|| source.clone());
    let locked = read_only || !row.editable;
    let reason = if read_only {
        Some("no schema")
    } else {
        lock_reason(&row, block_editable)
    };
    let in_source = mode() == EditMode::Source || value_draft.is_none();
    let raw_empty = raw().trim().is_empty();
    let node = row.id;
    let commit_raw = {
        let given = source.clone();
        move || {
            let text = raw();
            if text.trim().is_empty() || text == given {
                return;
            }
            handle.send(EstateAction::CommitEdit(Edit::ReplaceValue {
                node,
                value: TypedValue::Raw(text),
            }));
        }
    };
    let flags: Vec<&str> = [
        (row.required, "required"),
        (row.optional, "optional"),
        (row.computed, "computed"),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then_some(name))
    .collect();

    rsx! {
        div { class: "attr-row", class: if locked { "attr-row--locked" },
            div { class: "attr-row__name",
                code { "{row.key}" }
                span { class: "attr-row__line", "line {row.line}" }
                span { class: "attr-row__type", "{type_label(&row)}" }
                if !flags.is_empty() {
                    span { class: "attr-row__flags", {flags.join(" · ")} }
                }
            }
            div { class: "attr-row__field",
                if locked {
                    div { class: "attr-row__chips", SourceChips { value: row.value.clone() } }
                    if let Some(r) = reason {
                        span { class: "attr-row__reason", Icon { name: "lock", size: 16 } "{r}" }
                    }
                } else if in_source {
                    div { class: "attr-row__chips", SourceChips { value: row.value.clone() } }
                    TextField {
                        label: "Satz source",
                        value: raw(),
                        monospace: true,
                        disabled: loading,
                        error: raw_empty,
                        supporting: if raw_empty { "a value is needed".to_string() } else { "written as typed: a string keeps its quotes, a bare name is a param reference".to_string() },
                        oninput: move |v: String| raw.set(v),
                        onenter: {
                            let commit_raw = commit_raw.clone();
                            move |_| commit_raw()
                        },
                        onblur: move |_| commit_raw(),
                    }
                } else if let (Some(k), Some(draft)) = (kind, value_draft.clone()) {
                    TypedField {
                        kind: k,
                        draft,
                        label: row.key.clone(),
                        subject: row.key.clone(),
                        disabled: loading,
                        oncommit: move |d: Draft| {
                            handle.send(EstateAction::CommitEdit(Edit::ReplaceValue { node, value: d.to_typed(k) }));
                        },
                    }
                }
                if show_raw() {
                    pre { class: "attr-row__raw", "{row.line}  {line}" }
                }
            }
            div { class: "attr-row__tools",
                if !locked {
                    IconButton {
                        icon: "code",
                        label: if in_source { "Edit as a value" } else { "Edit the Satz source" },
                        selected: in_source,
                        disabled: value_draft.is_none(),
                        onclick: move |_| {
                            mode.set(if mode() == EditMode::Source { EditMode::Value } else { EditMode::Source });
                        },
                    }
                }
                IconButton {
                    icon: "subject",
                    label: "Show the line",
                    selected: show_raw(),
                    onclick: move |_| show_raw.toggle(),
                }
            }
        }
    }
}

/// The attribute's type as the schema declares it, in a word.
fn type_label(row: &AttrRow) -> String {
    use satz_studio_core::schema::AttrType;
    fn name(t: &AttrType) -> String {
        match t {
            AttrType::String => "string".into(),
            AttrType::Number => "number".into(),
            AttrType::Bool => "bool".into(),
            AttrType::ListOf(i) => format!("list of {}", name(i)),
            AttrType::SetOf(i) => format!("set of {}", name(i)),
            AttrType::MapOf(i) => format!("map of {}", name(i)),
            AttrType::Object(_) => "object".into(),
            AttrType::Unknown => "untyped".into(),
        }
    }
    name(&row.typed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use satz_studio_core::model::{SourceValue, StrPart};
    use satz_studio_core::schema::AttrType;

    fn attr(id: NodeId, key: &str, line: u32) -> AttrRow {
        AttrRow {
            id,
            key: key.to_string(),
            value: SourceValue::Str {
                raw: String::new(),
                parts: vec![StrPart::Lit(String::new())],
            },
            typed: AttrType::String,
            required: false,
            optional: true,
            computed: false,
            editable: true,
            mode: EditMode::Value,
            line,
        }
    }

    fn node(
        id: NodeId,
        key: &str,
        line: u32,
        attrs: Vec<AttrRow>,
        children: Vec<ResourceNode>,
    ) -> ResourceNode {
        ResourceNode {
            id,
            kind: ResourceKind::Resource,
            key: key.to_string(),
            key_parts: vec![StrPart::Lit(key.to_string())],
            label: None,
            label_parts: None,
            tf_type: Some("google_folder".to_string()),
            line,
            attrs,
            children,
            uses: Vec::new(),
            missing_required: Vec::new(),
        }
    }

    #[test]
    fn a_line_selects_the_node_whose_attribute_it_is_else_the_innermost_block_open_there() {
        // 1 google_folder {          (id 1)
        // 2   infra {                (id 2)
        // 3     display_name = "x"   (attr id 3)
        // 4     google_project {     (id 4)
        // 5       infra {            (id 5)
        // 6         name = "y"       (attr id 6)
        // 7       }
        // 8     }
        // 9   }
        // 10  logging {              (id 10)
        // 11    display_name = "z"   (attr id 11)
        // 12  }
        // 13 }
        let outline = vec![node(
            1,
            "google_folder",
            1,
            vec![],
            vec![
                node(
                    2,
                    "infra",
                    2,
                    vec![attr(3, "display_name", 3)],
                    vec![node(
                        4,
                        "google_project",
                        4,
                        vec![],
                        vec![node(5, "infra", 5, vec![attr(6, "name", 6)], vec![])],
                    )],
                ),
                node(
                    10,
                    "logging",
                    10,
                    vec![attr(11, "display_name", 11)],
                    vec![],
                ),
            ],
        )];
        assert_eq!(node_at_line(&outline, 3), Some(2));
        assert_eq!(node_at_line(&outline, 6), Some(5));
        assert_eq!(node_at_line(&outline, 11), Some(10));
        assert_eq!(node_at_line(&outline, 5), Some(5));
        assert_eq!(node_at_line(&outline, 7), Some(5));
        assert_eq!(node_at_line(&outline, 1), Some(1));
        assert_eq!(node_at_line(&outline, 13), Some(10));
        assert_eq!(node_at_line(&[], 3), None);
        assert_eq!(
            find_node(&outline, 5).map(|n| n.key.as_str()),
            Some("infra")
        );
        assert_eq!(find_node(&outline, 99), None);
    }

    #[test]
    fn a_lock_names_its_reason() {
        let mut r = attr(1, "import-id", 1);
        r.editable = false;
        assert_eq!(lock_reason(&r, true), Some("written by satz adopt"));
        let mut c = attr(1, "id", 1);
        c.editable = false;
        c.computed = true;
        c.optional = false;
        assert_eq!(lock_reason(&c, true), Some("computed by the provider"));
        let mut u = attr(1, "x", 1);
        u.editable = false;
        assert_eq!(lock_reason(&u, false), Some("not in the schema"));
        assert_eq!(lock_reason(&attr(1, "name", 1), true), None);
    }
}
