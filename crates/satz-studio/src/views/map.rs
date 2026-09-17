//! The Packs destination: the pack rows — the map line first, then the gated lines under the
//! phase each can be adopted in, then the choices the file has no line for. A pack that
//! others wait on is a tree: its card, and the cards of the packs asked only when it is on
//! hung below it by right-angle connectors, whatever phase their lines stand under. A
//! toggle is an answer written by `satz_interview`; the one line the app writes itself is
//! the map's, which no question gates.

use dioxus::prelude::*;
use satz_studio_core::diag::{DiagSource, Diagnostic};
use satz_studio_core::model::{Choice, LineState, PackEdge, PackRow, PackRowKind};
use satz_studio_core::satz::reports::QuestionRow;

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, ConnectorBranch, ConnectorLine,
    ConnectorTree, Icon, Segment, SegmentedButton, Switch,
};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt};

/// The header of the section for rows the file has no line for.
pub const ABSENT_HEADER: &str = "Not in this file";
/// The header of the section for rows above which no phase comment stands.
pub const NO_PHASE_HEADER: &str = "No phase";

/// One card's worth of rows: a single gated line, or a `oneof` group's lines together.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Single(PackRow),
    Group {
        group: String,
        question: Option<QuestionRow>,
        rows: Vec<PackRow>,
    },
}

impl Entry {
    /// The gates the card switches: its row's, or every row's of the group.
    pub fn gates(&self) -> Vec<&str> {
        match self {
            Entry::Single(row) => row.gate.as_deref().into_iter().collect(),
            Entry::Group { rows, .. } => rows.iter().filter_map(|r| r.gate.as_deref()).collect(),
        }
    }

    /// Whether the card's pack is in the estate: a row's line is on and its gate is on —
    /// for a group, any option's.
    pub fn in_estate(&self) -> bool {
        match self {
            Entry::Single(row) => in_estate(row),
            Entry::Group { rows, .. } => rows.iter().any(in_estate),
        }
    }
}

/// A line that is on with its gate on: the pack is folded into the estate. A line that is
/// on while its gate is off emits nothing, and has its own note.
pub fn in_estate(row: &PackRow) -> bool {
    row.state == LineState::On
        && match row.choice {
            Choice::Bool { current, .. } => current == Some(true),
            Choice::OneofOption { selected, .. } => selected,
            Choice::Line => true,
        }
}

/// What a section's grid holds: a card in a cell, or a tree across the whole row.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// a card no other card waits on and that waits on none
    Cell(Entry),
    /// a card others wait on, with them below it
    Tree(Node),
}

/// A card and the branches that hang from it.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub entry: Entry,
    pub children: Vec<Branch>,
}

/// How a child hangs from its parent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// asked only while the parent's gate is on
    AskedWhen,
    /// asked only when, and on while the parent is until somebody answers it: its binding
    /// is the parent's gate by reference
    Follows,
}

/// One connector and the card it leads to.
#[derive(Debug, Clone, PartialEq)]
pub struct Branch {
    /// the gate the child waits on
    pub parent_gate: String,
    pub link: Link,
    /// the child's pack is in the estate while the parent's is not
    pub warn: bool,
    /// a later sibling's connector warns, so the trunk past this branch leads to it
    pub trunk_warn: bool,
    /// the phase the child's line stands under, when that is not its parent's: the
    /// section the child would be listed in on its own
    pub phase: Option<String>,
    pub node: Node,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub header: String,
    pub items: Vec<Item>,
}

/// The phase a comment block names: its first line.
pub fn phase_header(phase: &str) -> String {
    phase.lines().next().unwrap_or_default().trim().to_string()
}

/// The choice rows in sections, with the packs that wait on another hung below it.
///
/// First the flat sections: a row with a phase comment opens the section that comment's
/// first line names, a row without one joins the section open at that point (or
/// [`NO_PHASE_HEADER`] when none is), and every `Absent` row goes last under
/// [`ABSENT_HEADER`]; within a section the rows of one `oneof` group share an entry.
///
/// Then the edges: an entry holding one of an edge's gates hangs below the first entry
/// holding the edge's parent gate, wherever that entry stands, and leaves its own section.
/// An entry that nothing hangs from and that hangs from nothing stays a [`Item::Cell`]; one
/// with children is an [`Item::Tree`] in its own section, its children in document order.
/// A section left with nothing is dropped. The model's edges are a forest; should a
/// grouping of rows still close a loop, the entry where it closes stays at the top.
pub fn sections(rows: &[PackRow], edges: &[PackEdge]) -> Vec<Section> {
    let flat = flat_sections(rows);
    let entries: Vec<(usize, &Entry)> = flat
        .iter()
        .enumerate()
        .flat_map(|(s, (_, es))| es.iter().map(move |e| (s, e)))
        .collect();
    let holder = |gate: &str| entries.iter().position(|(_, e)| e.gates().contains(&gate));
    let mut parent: Vec<Option<(usize, &PackEdge)>> = entries
        .iter()
        .map(|(_, e)| {
            let gates = e.gates();
            edges
                .iter()
                .find(|edge| edge.gates.iter().any(|g| gates.contains(&g.as_str())))
                .and_then(|edge| holder(&edge.parent).map(|p| (p, edge)))
        })
        .collect();
    for i in 0..entries.len() {
        let mut cur = parent[i].map(|(p, _)| p);
        for _ in 0..entries.len() {
            match cur {
                Some(p) if p == i => {
                    parent[i] = None;
                    break;
                }
                Some(p) => cur = parent[p].map(|(q, _)| q),
                None => break,
            }
        }
    }

    let node = |i: usize| build_node(i, &entries, &parent, &flat, rows);
    flat.iter()
        .enumerate()
        .map(|(s, (header, _))| Section {
            header: header.clone(),
            items: entries
                .iter()
                .enumerate()
                .filter(|(i, (es, _))| *es == s && parent[*i].is_none())
                .map(|(i, (_, e))| {
                    let n = node(i);
                    if n.children.is_empty() {
                        Item::Cell((*e).clone())
                    } else {
                        Item::Tree(n)
                    }
                })
                .collect(),
        })
        .filter(|s| !s.items.is_empty())
        .collect()
}

fn build_node(
    i: usize,
    entries: &[(usize, &Entry)],
    parent: &[Option<(usize, &PackEdge)>],
    flat: &[(String, Vec<Entry>)],
    rows: &[PackRow],
) -> Node {
    let mut children: Vec<Branch> = parent
        .iter()
        .enumerate()
        .filter_map(|(c, p)| p.filter(|(p, _)| *p == i).map(|(_, edge)| (c, edge)))
        .map(|(c, edge)| {
            let (child_section, child) = entries[c];
            let parent_on = rows
                .iter()
                .filter(|r| r.gate.as_deref() == Some(edge.parent.as_str()))
                .any(in_estate);
            let header = &flat[child_section].0;
            Branch {
                parent_gate: edge.parent.clone(),
                link: if edge.follows {
                    Link::Follows
                } else {
                    Link::AskedWhen
                },
                warn: child.in_estate() && !parent_on,
                trunk_warn: false,
                phase: (child_section != entries[i].0 && header != ABSENT_HEADER)
                    .then(|| header.clone()),
                node: build_node(c, entries, parent, flat, rows),
            }
        })
        .collect();
    for j in 0..children.len() {
        children[j].trunk_warn = children[j + 1..].iter().any(|b| b.warn);
    }
    Node {
        entry: entries[i].1.clone(),
        children,
    }
}

/// The sections before any card is hung below another: a header and its entries.
fn flat_sections(rows: &[PackRow]) -> Vec<(String, Vec<Entry>)> {
    let mut out: Vec<(String, Vec<Entry>)> = Vec::new();
    let mut absent: Vec<PackRow> = Vec::new();
    for row in rows
        .iter()
        .filter(|r| matches!(r.kind, PackRowKind::Choice | PackRowKind::Plain))
    {
        if row.state == LineState::Absent {
            absent.push(row.clone());
            continue;
        }
        if let Some(phase) = &row.phase {
            out.push((phase_header(phase), Vec::new()));
        } else if out.is_empty() {
            out.push((NO_PHASE_HEADER.to_string(), Vec::new()));
        }
        let (_, entries) = out.last_mut().expect("a section was opened above");
        push_entry(entries, row.clone());
    }
    if !absent.is_empty() {
        let mut entries = Vec::new();
        for row in absent {
            push_entry(&mut entries, row);
        }
        out.push((ABSENT_HEADER.to_string(), entries));
    }
    out
}

fn push_entry(entries: &mut Vec<Entry>, row: PackRow) {
    if let Choice::OneofOption { group, .. } = &row.choice {
        let existing = entries.iter_mut().find_map(|e| match e {
            Entry::Group { group: g, rows, .. } if g == group => Some(rows),
            _ => None,
        });
        match existing {
            Some(rows) => rows.push(row),
            None => entries.push(Entry::Group {
                group: group.clone(),
                question: row.question.clone(),
                rows: vec![row],
            }),
        }
    } else {
        entries.push(Entry::Single(row));
    }
}

#[component]
pub fn PacksView() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let model = app.estate().model().cloned();
    let loading = app.estate().loading().cloned();
    let notes: Vec<Diagnostic> = app
        .estate()
        .diagnostics()
        .read()
        .iter()
        .filter(|d| d.source == DiagSource::Model)
        .cloned()
        .collect();
    let Some(model) = model else {
        return rsx! {
            div { class: "view packs",
                h1 { class: "view__title", "Packs" }
                Card { variant: CardVariant::Filled, class: "map__empty",
                    Icon { name: "map", size: 48, class: "placeholder__icon" }
                    p { "The estate model is not available — the drawer says why." }
                }
            }
        };
    };
    let map_row = model
        .packs
        .iter()
        .find(|r| r.kind == PackRowKind::Map)
        .cloned();
    let sections = sections(&model.packs, &model.pack_edges);

    rsx! {
        div { class: "view packs",
            h1 { class: "view__title", "Packs" }
            if let Some(map) = map_row {
                match map.state {
                    LineState::Off => rsx! {
                        Card { variant: CardVariant::Filled, class: "map__map-card",
                            Icon { name: "map", size: 32, class: "placeholder__icon" }
                            div { class: "grow",
                                h2 { class: "map__map-title", "The map declares the choices; enable it to answer them." }
                                p { code { "// use \"{map.path.clone().unwrap_or_default()}\"" } " is commented out" if let Some(l) = map.line { " on line {l}" } ". While it is, no pack question is asked and every switch below is off." }
                            }
                            Button { variant: ButtonVariant::Filled, icon: "toggle_on", disabled: loading, onclick: move |_| handle.send(EstateAction::EnableMap), "Enable the map" }
                        }
                    },
                    LineState::Absent => rsx! {
                        Card { variant: CardVariant::Filled, class: "map__map-card",
                            Icon { name: "map", size: 32, class: "placeholder__icon" }
                            div { class: "grow",
                                h2 { class: "map__map-title", "This estate has no map line." }
                                p { code { "use \"{map.path.clone().unwrap_or_default()}\"" } " is neither active nor commented in the file; " code { "satz merge-presets" } " writes the lines the library declares." }
                            }
                            Button { variant: ButtonVariant::Tonal, icon: "merge", disabled: loading, onclick: move |_| handle.send(EstateAction::MergePresets), "Run merge-presets" }
                        }
                    },
                    LineState::On => rsx! {
                        div { class: "map__map-on",
                            Chip { kind: ChipKind::Assist, icon: "check_circle", label: "map on" }
                            span { "{map.path.clone().unwrap_or_default()}" if let Some(l) = map.line { ", line {l}" } }
                            span { class: "grow" }
                            Button {
                                variant: ButtonVariant::Tonal,
                                icon: "merge",
                                disabled: loading,
                                onclick: move |_| handle.send(EstateAction::MergePresets),
                                "Run merge-presets"
                            }
                        }
                    },
                }
            }
            p { class: "view__lead", "merge-presets reconciles this estate's library with upstream: a pack that is missing is installed and gets its commented line here, an unmodified one is upgraded, an edited one is forked rather than overwritten. Its flags — a report-only run, a pristine directory, adopting one pack in place — are in the commands palette." }
            for section in sections {
                section { key: "{section.header}", class: "map__section",
                    h2 { class: "map__section-title", "{section.header}" }
                    div { class: "map__cards",
                        for (i, item) in section.items.into_iter().enumerate() {
                            match item {
                                Item::Cell(entry) => rsx! {
                                    PackCard { key: "{i}-{entry_key(&entry)}", entry, notes: notes.clone(), loading }
                                },
                                Item::Tree(node) => rsx! {
                                    PackTree { key: "{i}-tree-{entry_key(&node.entry)}", node, notes: notes.clone(), loading }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The key a card is rendered under.
fn entry_key(entry: &Entry) -> String {
    match entry {
        Entry::Single(r) => format!(
            "{}-{}",
            r.gate.clone().unwrap_or_default(),
            r.path.clone().unwrap_or_default()
        ),
        Entry::Group { group, .. } => format!("group-{group}"),
    }
}

/// A card others wait on, across the whole row, with the cards that wait on it below.
#[component]
fn PackTree(node: Node, notes: Vec<Diagnostic>, loading: bool) -> Element {
    let Node { entry, children } = node;
    rsx! {
        ConnectorTree {
            class: "pack-tree",
            root: rsx! { PackCard { entry, notes: notes.clone(), loading } },
            for branch in children {
                PackBranch { key: "{entry_key(&branch.node.entry)}", branch, notes: notes.clone(), loading }
            }
        }
    }
}

/// One connector, the card it leads to, and what hangs from that card in turn.
#[component]
fn PackBranch(branch: Branch, notes: Vec<Diagnostic>, loading: bool) -> Element {
    let Branch {
        parent_gate,
        link,
        warn,
        trunk_warn,
        phase,
        node: Node { entry, children },
    } = branch;
    let labelled = warn || link == Link::Follows || phase.is_some();
    let label = labelled.then(|| {
        rsx! {
            if warn {
                span { class: "pack-tree__warn",
                    Icon { name: "error", size: 16 }
                    "on while " code { "{parent_gate}" } " is off"
                }
            }
            if link == Link::Follows {
                span {
                    class: "pack-tree__follows",
                    title: "Its default is {parent_gate} by reference: on while {parent_gate} is on, until answered.",
                    Icon { name: "link", size: 16 }
                    "follows " code { "{parent_gate}" }
                }
            }
            if let Some(phase) = &phase {
                span { class: "pack-tree__phase", title: "{phase}", "{phase}" }
            }
        }
    });
    let branches = (!children.is_empty()).then(|| {
        rsx! {
            for child in children {
                PackBranch { key: "{entry_key(&child.node.entry)}", branch: child, notes: notes.clone(), loading }
            }
        }
    });
    rsx! {
        ConnectorBranch {
            line: if link == Link::Follows { ConnectorLine::Dashed } else { ConnectorLine::Solid },
            error: warn,
            trunk_error: trunk_warn,
            label,
            branches,
            node: rsx! { PackCard { entry, notes: notes.clone(), loading } },
        }
    }
}

fn state_chip(state: LineState) -> Element {
    match state {
        LineState::On => {
            rsx! { Chip { kind: ChipKind::Assist, icon: "check_circle", label: "on" } }
        }
        LineState::Off => {
            rsx! { Chip { kind: ChipKind::Assist, icon: "radio_button_unchecked", label: "off" } }
        }
        LineState::Absent => {
            rsx! { Chip { kind: ChipKind::Assist, icon: "error", label: "absent", error: true } }
        }
    }
}

/// The notes the model raised on this row's line.
fn notes_at(notes: &[Diagnostic], line: Option<u32>) -> Vec<Diagnostic> {
    notes
        .iter()
        .filter(|d| d.line.is_some() && d.line == line)
        .cloned()
        .collect()
}

#[component]
fn PackCard(entry: Entry, notes: Vec<Diagnostic>, loading: bool) -> Element {
    let handle = use_coroutine_handle::<EstateAction>();
    match entry {
        Entry::Single(row) => {
            let gate = row.gate.clone().unwrap_or_default();
            let path = row.path.clone();
            let (current, default) = match row.choice {
                Choice::Bool { current, default } => (current, default),
                _ => (None, None),
            };
            let checked = current.or(default).unwrap_or(false);
            let asked = row.question.is_some();
            let row_notes = notes_at(&notes, row.line);
            let toggle_gate = gate.clone();
            rsx! {
                Card { variant: CardVariant::Outlined, class: "pack-card",
                    div { class: "pack-card__head",
                        Icon { name: "extension", size: 20 }
                        code { class: "pack-card__path", {path.clone().unwrap_or_else(|| format!("no line for {gate}"))} }
                        span { class: "grow" }
                        {state_chip(row.state)}
                    }
                    if row.kind == PackRowKind::Plain {
                        p { class: "pack-card__why",
                            "No gate: this line carries no " code { "when" } ", so the file decides the pack and no question does. Write a "
                            code { "when <param>" } " on it to make it a choice, or comment the line out to take the pack off."
                        }
                    } else {
                        match &row.question {
                            Some(q) => rsx! {
                                p { class: "pack-card__prompt", "{q.prompt}" }
                                if let Some(why) = &q.why {
                                    p { class: "pack-card__why", "{why}" }
                                }
                            },
                            None => rsx! {
                                p { class: "pack-card__why", "No question: the map does not declare one for " code { "{gate}" } ", or the map is not in." }
                            },
                        }
                    }
                    if row.kind == PackRowKind::Plain {
                        // no switch: there is no param to write, and the app never
                        // comments or uncomments a line the operator wrote by hand
                    } else if row.state == LineState::Absent {
                        div { class: "pack-card__remedy",
                            Icon { name: "info", size: 20 }
                            span { "The estate has no line for this pack: " code { "satz merge-presets" } " writes it under its phase." }
                            span { class: "grow" }
                            Button { variant: ButtonVariant::Tonal, icon: "merge", disabled: loading, onclick: move |_| handle.send(EstateAction::MergePresets), "Run merge-presets" }
                        }
                    } else {
                        div { class: "pack-card__control",
                            Switch {
                                label: "{gate}",
                                checked,
                                disabled: loading || !asked,
                                onchange: move |v: bool| handle.send(EstateAction::Answer { subject: toggle_gate.clone(), value: serde_json::Value::Bool(v) }),
                            }
                            span { class: "pack-card__note",
                                if checked {
                                    "On: satz uncomments the line when the answer lands."
                                } else {
                                    "Off keeps the commented line where it is; satz never re-comments one."
                                }
                            }
                        }
                    }
                    for (i, n) in row_notes.iter().enumerate() {
                        div { key: "{i}", class: "pack-card__diag",
                            Icon { name: "info", size: 18 }
                            span { "{n.message}" }
                        }
                    }
                }
            }
        }
        Entry::Group {
            group,
            question,
            rows,
        } => {
            let options: Vec<Segment> = question
                .as_ref()
                .map(|q| {
                    q.options
                        .iter()
                        .map(|o| Segment::new(o.param.clone(), o.label.clone()))
                        .collect()
                })
                .unwrap_or_default();
            let selected = rows
                .iter()
                .find(|r| matches!(r.choice, Choice::OneofOption { selected: true, .. }))
                .and_then(|r| r.gate.clone())
                .unwrap_or_default();
            let subject = group.clone();
            rsx! {
                Card { variant: CardVariant::Outlined, class: "pack-card pack-card--group",
                    div { class: "pack-card__head",
                        Icon { name: "alt_route", size: 20 }
                        code { class: "pack-card__path", "{group}" }
                        span { class: "grow" }
                        Chip { kind: ChipKind::Assist, icon: "rule", label: "one of" }
                    }
                    match &question {
                        Some(q) => rsx! {
                            p { class: "pack-card__prompt", "{q.prompt}" }
                            if let Some(why) = &q.why {
                                p { class: "pack-card__why", "{why}" }
                            }
                            SegmentedButton {
                                options,
                                selected,
                                onselect: move |v: String| handle.send(EstateAction::Answer { subject: subject.clone(), value: serde_json::Value::String(v) }),
                            }
                        },
                        None => rsx! {
                            p { class: "pack-card__why", "No question: the map is not in, so the choice cannot be made here." }
                        },
                    }
                    ul { class: "pack-card__lines",
                        for r in rows.iter() {
                            li { key: "{r.gate.clone().unwrap_or_default()}", class: "pack-card__line",
                                code { class: "pack-card__path", {r.path.clone().unwrap_or_else(|| format!("no line for {}", r.gate.clone().unwrap_or_default()))} }
                                if let Some(q) = &question {
                                    if let Some(o) = q.options.iter().find(|o| Some(&o.param) == r.gate.as_ref()) {
                                        span { class: "pack-card__option-label", "{o.label}" }
                                    }
                                }
                                span { class: "grow" }
                                {state_chip(r.state)}
                                if r.state == LineState::Absent {
                                    Button { variant: ButtonVariant::Text, icon: "merge", disabled: loading, onclick: move |_| handle.send(EstateAction::MergePresets), "merge-presets" }
                                }
                                for (i, n) in notes_at(&notes, r.line).iter().enumerate() {
                                    span { key: "{i}", class: "pack-card__diag", Icon { name: "info", size: 18 } "{n.message}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(path: Option<&str>, gate: &str, state: LineState, phase: Option<&str>) -> PackRow {
        PackRow {
            kind: PackRowKind::Choice,
            gate: Some(gate.to_string()),
            path: path.map(str::to_string),
            state,
            choice: Choice::Bool {
                current: None,
                default: None,
            },
            question: None,
            phase: phase.map(str::to_string),
            line: Some(1),
        }
    }

    fn oneof(path: &str, gate: &str, group: &str) -> PackRow {
        PackRow {
            choice: Choice::OneofOption {
                group: group.to_string(),
                selected: false,
            },
            ..row(Some(path), gate, LineState::Off, None)
        }
    }

    /// A line in the file, with its gate bound `gate_on`.
    fn line(gate: &str, state: LineState, gate_on: bool, phase: Option<&str>) -> PackRow {
        PackRow {
            choice: Choice::Bool {
                current: Some(gate_on),
                default: None,
            },
            ..row(Some(&format!("presets/{gate}.satz")), gate, state, phase)
        }
    }

    fn edge(parent: &str, child: &str) -> PackEdge {
        PackEdge {
            parent: parent.to_string(),
            child: child.to_string(),
            gates: vec![child.to_string()],
            follows: false,
            file: "presets/estate-map.satz".to_string(),
            line: 1,
        }
    }

    fn name(entry: &Entry) -> String {
        match entry {
            Entry::Single(r) => r.gate.clone().unwrap(),
            Entry::Group { group, rows, .. } => format!("{group}:{}", rows.len()),
        }
    }

    /// `a[b[d, e], c]` for a tree, `a` for a cell.
    fn outline(item: &Item) -> String {
        fn node(n: &Node) -> String {
            if n.children.is_empty() {
                return name(&n.entry);
            }
            let children: Vec<String> = n.children.iter().map(|b| node(&b.node)).collect();
            format!("{}[{}]", name(&n.entry), children.join(", "))
        }
        match item {
            Item::Cell(e) => name(e),
            Item::Tree(n) => node(n),
        }
    }

    fn shape(sections: &[Section]) -> Vec<(String, Vec<String>)> {
        sections
            .iter()
            .map(|s| (s.header.clone(), s.items.iter().map(outline).collect()))
            .collect()
    }

    fn tree<'a>(sections: &'a [Section], root: &str) -> &'a Node {
        sections
            .iter()
            .flat_map(|s| s.items.iter())
            .find_map(|i| match i {
                Item::Tree(n) if name(&n.entry) == root => Some(n),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no tree rooted at {root}: {:?}", shape(sections)))
    }

    fn owned(v: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
        v.iter()
            .map(|(h, items)| (h.to_string(), items.iter().map(|i| i.to_string()).collect()))
            .collect()
    }

    #[test]
    fn the_header_is_the_phase_comment_s_first_line() {
        assert_eq!(
            phase_header(
                "once the estate runs as the service account — these three stand alone\nmore"
            ),
            "once the estate runs as the service account — these three stand alone"
        );
        assert_eq!(phase_header(""), "");
    }

    #[test]
    fn a_phase_opens_a_section_a_bare_row_joins_it_and_absent_rows_go_last() {
        let rows = vec![
            PackRow {
                kind: PackRowKind::Map,
                gate: None,
                path: Some("presets/estate-map.satz".into()),
                state: LineState::On,
                choice: Choice::Line,
                question: None,
                phase: Some("the map".into()),
                line: Some(1),
            },
            row(
                Some("a.satz"),
                "use_a",
                LineState::Off,
                Some("once A\ndetail"),
            ),
            row(Some("b.satz"), "use_b", LineState::On, None),
            row(None, "use_z", LineState::Absent, None),
            oneof("s1.satz", "model_s1", "model"),
            oneof("s2.satz", "model_s2", "model"),
            row(Some("c.satz"), "use_c", LineState::Off, Some("once C")),
        ];
        assert_eq!(
            shape(&sections(&rows, &[])),
            owned(&[
                ("once A", &["use_a", "use_b", "model:2"]),
                ("once C", &["use_c"]),
                (ABSENT_HEADER, &["use_z"]),
            ])
        );
    }

    #[test]
    fn a_row_before_any_phase_opens_the_no_phase_section() {
        let rows = vec![row(Some("a.satz"), "use_a", LineState::Off, None)];
        assert_eq!(sections(&rows, &[])[0].header, NO_PHASE_HEADER);
    }

    /// The SCC shape: enablement in an early phase, notifications and export in a later
    /// one, the mail and the SIEM each in a phase of its own — three levels.
    fn scc_rows() -> Vec<PackRow> {
        vec![
            line("use_budget", LineState::Off, false, Some("once alone")),
            line("use_scc", LineState::Off, false, None),
            line("use_audit", LineState::Off, false, None),
            line("use_notify", LineState::Off, false, Some("once SCC is on")),
            line("use_export", LineState::Off, false, None),
            line(
                "use_siem",
                LineState::Off,
                false,
                Some("once the topic exists"),
            ),
            line(
                "use_mail",
                LineState::Off,
                false,
                Some("once the alerts are in"),
            ),
            line("use_runner", LineState::Off, false, Some("once the runner")),
        ]
    }

    fn scc_edges() -> Vec<PackEdge> {
        vec![
            edge("use_scc", "use_notify"),
            edge("use_notify", "use_mail"),
            edge("use_notify", "use_siem"),
            edge("use_scc", "use_export"),
        ]
    }

    #[test]
    fn a_pack_others_wait_on_is_a_tree_the_rest_stay_cells_and_children_leave_their_phase() {
        let s = sections(&scc_rows(), &scc_edges());
        assert_eq!(
            shape(&s),
            owned(&[
                (
                    "once alone",
                    &[
                        "use_budget",
                        "use_scc[use_notify[use_siem, use_mail], use_export]",
                        "use_audit",
                    ]
                ),
                ("once the runner", &["use_runner"]),
            ]),
            "children in document order, not in edge order; the emptied phases are gone"
        );
    }

    #[test]
    fn a_child_carries_the_phase_it_left_and_one_in_its_parent_s_phase_carries_none() {
        let s = sections(&scc_rows(), &scc_edges());
        let scc = tree(&s, "use_scc");
        let notify = &scc.children[0];
        let export = &scc.children[1];
        assert_eq!(notify.phase.as_deref(), Some("once SCC is on"));
        assert_eq!(export.phase.as_deref(), Some("once SCC is on"));
        assert_eq!(
            notify.node.children[0].phase.as_deref(),
            Some("once the topic exists")
        );
        assert_eq!(
            notify.node.children[1].phase.as_deref(),
            Some("once the alerts are in")
        );

        let same = vec![
            line("use_a", LineState::Off, false, Some("once A")),
            line("use_b", LineState::Off, false, None),
        ];
        let s = sections(&same, &[edge("use_a", "use_b")]);
        assert_eq!(tree(&s, "use_a").children[0].phase, None);
    }

    #[test]
    fn a_child_that_defaults_to_its_parent_by_reference_follows_it() {
        let rows = vec![
            line(
                "use_sentinel",
                LineState::Off,
                false,
                Some("once the archive"),
            ),
            line("use_logs", LineState::Off, false, None),
            line("use_net", LineState::Off, false, None),
        ];
        let mut logs = edge("use_sentinel", "use_logs");
        logs.follows = true;
        let s = sections(&rows, &[logs, edge("use_sentinel", "use_net")]);
        let links: Vec<(Link, &str)> = tree(&s, "use_sentinel")
            .children
            .iter()
            .map(|b| (b.link, b.parent_gate.as_str()))
            .collect();
        assert_eq!(
            links,
            [
                (Link::Follows, "use_sentinel"),
                (Link::AskedWhen, "use_sentinel")
            ]
        );
    }

    #[test]
    fn a_child_in_the_estate_under_a_parent_that_is_not_warns_and_the_trunk_leads_to_it() {
        let rows = vec![
            // the parent's line is on and its gate was answered off
            line("use_scc", LineState::On, false, Some("once alone")),
            // on, but its gate is off: nothing of it is emitted
            line("use_notify", LineState::On, false, None),
            // off
            line("use_export", LineState::Off, true, None),
            // in: line on, gate on
            line("use_extra", LineState::On, true, None),
        ];
        let edges = [
            edge("use_scc", "use_notify"),
            edge("use_scc", "use_export"),
            edge("use_scc", "use_extra"),
        ];
        let s = sections(&rows, &edges);
        let warns: Vec<(bool, bool)> = tree(&s, "use_scc")
            .children
            .iter()
            .map(|b| (b.warn, b.trunk_warn))
            .collect();
        assert_eq!(warns, [(false, true), (false, true), (true, false)]);

        // the parent in as well: nothing warns
        let mut on = rows.clone();
        on[0] = line("use_scc", LineState::On, true, Some("once alone"));
        let s = sections(&on, &edges);
        assert!(
            tree(&s, "use_scc")
                .children
                .iter()
                .all(|b| !b.warn && !b.trunk_warn)
        );
    }

    #[test]
    fn a_oneof_under_a_gate_is_one_child_and_a_group_holding_the_parent_gate_is_the_parent() {
        let mut default = oneof("presets/role-default.satz", "access_default", "access");
        default.state = LineState::On;
        default.choice = Choice::OneofOption {
            group: "access".to_string(),
            selected: true,
        };
        let rows = vec![
            line("use_plan", LineState::Off, false, Some("once Defender")),
            default,
            oneof("presets/role-least.satz", "access_least", "access"),
            line("use_audit_role", LineState::Off, false, None),
        ];
        let mut access = edge("use_plan", "access");
        access.gates = vec!["access_default".to_string(), "access_least".to_string()];
        let s = sections(&rows, &[access, edge("access_least", "use_audit_role")]);
        assert_eq!(
            shape(&s),
            owned(&[("once Defender", &["use_plan[access:2[use_audit_role]]"])])
        );
        let branch = &tree(&s, "use_plan").children[0];
        assert!(branch.warn, "an option is in while the plan is not");
    }

    #[test]
    fn a_loop_that_grouping_closes_stays_at_the_top_where_it_closes() {
        let rows = vec![
            line("use_a", LineState::Off, false, Some("once A")),
            oneof("x.satz", "pick_x", "pick"),
            oneof("y.satz", "pick_y", "pick"),
        ];
        let mut pick = edge("use_a", "pick");
        pick.gates = vec!["pick_x".to_string(), "pick_y".to_string()];
        let s = sections(&rows, &[pick, edge("pick_y", "use_a")]);
        assert_eq!(shape(&s), owned(&[("once A", &["use_a[pick:2]"])]));
    }

    #[test]
    fn an_absent_child_hangs_under_its_parent_without_a_phase_caption() {
        let rows = vec![
            line("use_scc", LineState::Off, false, Some("once alone")),
            row(None, "use_notify", LineState::Absent, None),
        ];
        let s = sections(&rows, &[edge("use_scc", "use_notify")]);
        assert_eq!(
            shape(&s),
            owned(&[("once alone", &["use_scc[use_notify]"])])
        );
        assert_eq!(tree(&s, "use_scc").children[0].phase, None);
    }
}
