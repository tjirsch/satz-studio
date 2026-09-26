//! The Packs destination: satz's pack report (`satz_packs`) as cards. The map first; then
//! every pack whose line the file carries, under the phase that line stands under; then
//! the packs the file has no line for; then the `use` lines the pack graph does not know.
//! A pack that another needs — and that meets that need alone — is a tree: its card, and
//! the cards of the packs that need it hung below by right-angle connectors, whatever
//! phase their lines stand under. A switch is `satz_add_pack` or `satz_remove_pack`: what
//! a pack needs, what it deploys and what a switch does are satz's pack graph, and the
//! view derives none of it. Above the sections stands the pack review
//! ([`crate::views::review`]), for a pack the operator writes rather than switches.

use std::collections::BTreeMap;

use dioxus::prelude::*;
use satz_studio_core::satz::reports::{
    AddPackArgs, Finding, PackLine, PackRole, PackRow, PacksReport, QuestionsReport,
    RemovePackArgs, Requirement, RequirementKind,
};

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, ConnectorBranch, ConnectorLine,
    ConnectorTree, Icon, Switch,
};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt};
use crate::views::review::PackReviewCard;

/// The header of the section for packs the file has no line for.
pub const ABSENT_HEADER: &str = "Not in this file";
/// The header of the section for lines above which no phase comment stands.
pub const NO_PHASE_HEADER: &str = "No phase";

/// What a section's grid holds: a card in a cell, or a tree across the whole row.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// a pack no other card hangs from and that hangs from none
    Cell(PackRow),
    /// a pack others need, with them below it
    Tree(Node),
}

/// A card and the branches that hang from it.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub row: PackRow,
    pub children: Vec<Branch>,
}

/// One connector and the card it leads to.
#[derive(Debug, Clone, PartialEq)]
pub struct Branch {
    /// the child's requirement the parent meets: satz's own, `met` included
    pub requirement: Requirement,
    /// the child deploys while its requirement is not met
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

/// The pack `row` hangs from and the requirement that makes it: the first of its
/// requirements, in satz's order, that exactly one pack meets, where that pack is a menu
/// pack. A requirement several packs can meet is the operator's choice between them and
/// hangs the card under none; the map and the core pack stand at the head of the page,
/// and every menu pack needs the map.
pub fn parent_of<'a>(row: &'a PackRow, rows: &[PackRow]) -> Option<(&'a str, &'a Requirement)> {
    row.requires.iter().find_map(|r| match r.any_of.as_slice() {
        [one]
            if rows
                .iter()
                .any(|p| p.path == *one && p.role == PackRole::Pack) =>
        {
            Some((one.as_str(), r))
        }
        _ => None,
    })
}

/// The packs in sections, with the packs that need another hung below it.
///
/// First the flat sections: the packs whose line the file carries, in the file's order —
/// a line with a phase comment opens the section that comment's first line names, a line
/// without one joins the section open at that point (or [`NO_PHASE_HEADER`] when none
/// is) — then every pack the file has no line for, in the graph's order, under
/// [`ABSENT_HEADER`]. The map is the page's head and in no section.
///
/// Then the edges: a pack hangs below the pack [`parent_of`] names, wherever that one
/// stands, and leaves its own section. A pack nothing hangs from and that hangs from
/// nothing stays an [`Item::Cell`]; one with children is an [`Item::Tree`] in its own
/// section, its children in section order. A section left with nothing is dropped. The
/// graph's requirements do not loop; should they, the pack where the loop closes stays at
/// the top.
pub fn sections(report: &PacksReport, phases: &BTreeMap<u32, String>) -> Vec<Section> {
    let flat = flat_sections(report, phases);
    let entries: Vec<(usize, &PackRow)> = flat
        .iter()
        .enumerate()
        .flat_map(|(s, (_, rows))| rows.iter().map(move |r| (s, r)))
        .collect();
    let mut parent: Vec<Option<(usize, &Requirement)>> = entries
        .iter()
        .map(|(_, row)| {
            parent_of(row, &report.packs).and_then(|(path, req)| {
                entries
                    .iter()
                    .position(|(_, e)| e.path == path)
                    .map(|p| (p, req))
            })
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

    flat.iter()
        .enumerate()
        .map(|(s, (header, _))| Section {
            header: header.clone(),
            items: entries
                .iter()
                .enumerate()
                .filter(|(i, (es, _))| *es == s && parent[*i].is_none())
                .map(|(i, (_, row))| {
                    let n = build_node(i, &entries, &parent, &flat);
                    if n.children.is_empty() {
                        Item::Cell((*row).clone())
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
    entries: &[(usize, &PackRow)],
    parent: &[Option<(usize, &Requirement)>],
    flat: &[(String, Vec<PackRow>)],
) -> Node {
    let mut children: Vec<Branch> = parent
        .iter()
        .enumerate()
        .filter_map(|(c, p)| p.filter(|(p, _)| *p == i).map(|(_, req)| (c, req)))
        .map(|(c, req)| {
            let (child_section, child) = entries[c];
            let header = &flat[child_section].0;
            Branch {
                requirement: req.clone(),
                warn: child.deploys && !req.met,
                trunk_warn: false,
                phase: (child_section != entries[i].0 && header != ABSENT_HEADER)
                    .then(|| header.clone()),
                node: build_node(c, entries, parent, flat),
            }
        })
        .collect();
    for j in 0..children.len() {
        children[j].trunk_warn = children[j + 1..].iter().any(|b| b.warn);
    }
    Node {
        row: entries[i].1.clone(),
        children,
    }
}

/// The sections before any card is hung below another: a header and its packs.
fn flat_sections(
    report: &PacksReport,
    phases: &BTreeMap<u32, String>,
) -> Vec<(String, Vec<PackRow>)> {
    let mut lined: Vec<&PackRow> = report
        .packs
        .iter()
        .filter(|r| r.role != PackRole::Map && r.at_line.is_some())
        .collect();
    lined.sort_by_key(|r| r.at_line);
    let mut out: Vec<(String, Vec<PackRow>)> = Vec::new();
    for row in lined {
        if let Some(phase) = row.at_line.and_then(|l| phases.get(&l)) {
            out.push((phase_header(phase), Vec::new()));
        } else if out.is_empty() {
            out.push((NO_PHASE_HEADER.to_string(), Vec::new()));
        }
        let (_, rows) = out.last_mut().expect("a section was opened above");
        rows.push(row.clone());
    }
    let absent: Vec<PackRow> = report
        .packs
        .iter()
        .filter(|r| r.role != PackRole::Map && r.at_line.is_none())
        .cloned()
        .collect();
    if !absent.is_empty() {
        out.push((ABSENT_HEADER.to_string(), absent));
    }
    out
}

/// Each of the compile's findings about `row`, with the command that answers it where
/// satz names one: the sentences are the row's, the commands the report's findings whose
/// subject is the pack.
pub fn row_findings(report: &PacksReport, row: &PackRow) -> Vec<(String, Option<String>)> {
    row.findings
        .iter()
        .map(|message| {
            let fix = report
                .findings
                .iter()
                .find(|f| f.subject.as_deref() == Some(row.path.as_str()) && f.message == *message)
                .and_then(|f| f.fix.clone());
            (message.clone(), fix)
        })
        .collect()
}

/// The report's findings about no pack it has a row for: they have no card to stand on.
pub fn loose_findings(report: &PacksReport) -> Vec<Finding> {
    report
        .findings
        .iter()
        .filter(|f| f.subject.as_deref().is_none_or(|s| report.row(s).is_none()))
        .cloned()
        .collect()
}

/// The question that asks the gate, as the interview puts it: its prompt, and the option's
/// or the question's `why`. `None` while no pack the estate uses asks it.
pub fn prompt_of(questions: &QuestionsReport, gate: &str) -> Option<(String, Option<String>)> {
    questions.questions.iter().find_map(|q| {
        if q.subject == gate {
            return Some((q.prompt.clone(), q.why.clone()));
        }
        q.options.iter().find(|o| o.param == gate).map(|o| {
            (
                format!("{} — {}", q.prompt, o.label),
                o.why.clone().or(q.why.clone()),
            )
        })
    })
}

/// The requirements a card lists: every one that is not met, and the met ones the tree
/// does not already draw — neither its parent's connector nor the map every menu pack
/// needs.
pub fn listed_requirements<'a>(row: &'a PackRow, report: &PacksReport) -> Vec<&'a Requirement> {
    let drawn = parent_of(row, &report.packs).map(|(_, r)| r);
    let map = report.map().map(|m| m.path.as_str());
    row.requires
        .iter()
        .filter(|r| {
            !r.met
                || !(drawn == Some(*r)
                    || (r.any_of.len() == 1 && Some(r.any_of[0].as_str()) == map))
        })
        .collect()
}

#[component]
pub fn PacksView() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let model = app.estate().model().cloned();
    let loading = app.estate().loading().cloned();
    let questions = app.estate().questions().cloned();
    let Some(model) = model else {
        return rsx! {
            div { class: "view packs",
                h1 { class: "view__title", "Packs" }
                Card { variant: CardVariant::Filled, class: "map__empty",
                    Icon { name: "map", size: 48, class: "placeholder__icon" }
                    p { "The estate model is not available — the drawer says why." }
                }
                PackReviewCard {}
            }
        };
    };
    let report = model.packs.clone();
    let map_row = report.map().cloned();
    let sections = sections(&report, &model.phases);
    let loose = loose_findings(&report);
    let unmanaged = report.unmanaged.clone();

    rsx! {
        div { class: "view packs",
            h1 { class: "view__title", "Packs" }
            if let Some(note) = &report.note {
                Card { variant: CardVariant::Filled, class: "map__map-card",
                    Icon { name: "map", size: 32, class: "placeholder__icon" }
                    p { class: "grow", "{note}" }
                }
            }
            if let Some(map) = map_row {
                if map.deploys {
                    div { class: "map__map-on",
                        Chip { kind: ChipKind::Assist, icon: "check_circle", label: "map on" }
                        span { "{map.path}" if let Some(l) = map.at_line { ", line {l}" } }
                        span { class: "grow" }
                        Button {
                            variant: ButtonVariant::Tonal,
                            icon: "merge",
                            disabled: loading,
                            onclick: move |_| handle.send(EstateAction::MergePresets),
                            "Run merge-presets"
                        }
                    }
                } else {
                    Card { variant: CardVariant::Filled, class: "map__map-card",
                        Icon { name: "map", size: 32, class: "placeholder__icon" }
                        div { class: "grow",
                            if map.line == PackLine::Absent {
                                h2 { class: "map__map-title", "This estate has no map line." }
                                p { "The map declares the choices every other pack hangs from. Switching it on writes " code { "use \"{map.path}\"" } " where the pack graph places it." }
                            } else {
                                h2 { class: "map__map-title", "The map declares the choices; switch it on to answer them." }
                                p { code { "use \"{map.path}\"" } " is {map.line.word()}" if let Some(l) = map.at_line { " on line {l}" } ". While it is, no pack question is asked and no pack the map gates deploys." }
                            }
                        }
                        Button {
                            variant: ButtonVariant::Filled,
                            icon: "toggle_on",
                            disabled: loading,
                            onclick: {
                                let path = map.path.clone();
                                move |_| handle.send(EstateAction::AddPack(AddPackArgs { pack: path.clone(), with_requirements: false }))
                            },
                            "Switch the map on"
                        }
                    }
                }
            }
            p { class: "view__lead", "A switch is satz's add-pack or remove-pack: satz binds the gate, writes the line where the pack graph places it, and refuses a switch while a pack it needs is off or a pack that needs it is on. merge-presets reconciles this estate's library with upstream: a pack that is missing is installed, an unmodified one is upgraded, an edited one is forked rather than overwritten. Its flags are in the commands palette." }
            PackReviewCard {}
            for (i, f) in loose.iter().enumerate() {
                div { key: "loose-{i}", class: "pack-card__diag",
                    Icon { name: "error", size: 18 }
                    span { "{f.message}" }
                }
            }
            for (s, section) in sections.into_iter().enumerate() {
                section { key: "{s}-{section.header}", class: "map__section",
                    h2 { class: "map__section-title", "{section.header}" }
                    div { class: "map__cards",
                        for item in section.items {
                            match item {
                                Item::Cell(row) => rsx! {
                                    PackCard { key: "{row.path}", row: row.clone(), report: report.clone(), questions: questions.clone(), loading }
                                },
                                Item::Tree(node) => rsx! {
                                    PackTree { key: "tree-{node.row.path}", node, report: report.clone(), questions: questions.clone(), loading }
                                },
                            }
                        }
                    }
                }
            }
            if !unmanaged.is_empty() {
                section { class: "map__section",
                    h2 { class: "map__section-title", "Not in the pack graph" }
                    div { class: "map__cards",
                        for u in unmanaged {
                            Card { key: "{u.path}-{u.at_line}", variant: CardVariant::Outlined, class: "pack-card",
                                div { class: "pack-card__head",
                                    Icon { name: "extension_off", size: 20 }
                                    code { class: "pack-card__path", "{u.path}" }
                                    span { class: "grow" }
                                    Chip { kind: ChipKind::Assist, icon: "check_circle", label: "line {u.at_line}" }
                                }
                                p { class: "pack-card__why", "The pack graph does not know this file, so no switch here changes its line: it is the estate's own." }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A pack others need, across the whole row, with the packs that need it below.
#[component]
fn PackTree(
    node: Node,
    report: PacksReport,
    questions: Option<QuestionsReport>,
    loading: bool,
) -> Element {
    let Node { row, children } = node;
    rsx! {
        ConnectorTree {
            class: "pack-tree",
            root: rsx! { PackCard { row, report: report.clone(), questions: questions.clone(), loading } },
            for branch in children {
                PackBranch { key: "{branch.node.row.path}", branch, report: report.clone(), questions: questions.clone(), loading }
            }
        }
    }
}

/// One connector, the card it leads to, and what hangs from that card in turn. A data
/// requirement — the child reads params the parent declares — is dashed and names them.
#[component]
fn PackBranch(
    branch: Branch,
    report: PacksReport,
    questions: Option<QuestionsReport>,
    loading: bool,
) -> Element {
    let Branch {
        requirement,
        warn,
        trunk_warn,
        phase,
        node: Node { row, children },
    } = branch;
    let reads = (requirement.kind == RequirementKind::Data && !requirement.params.is_empty())
        .then(|| requirement.params.join(", "));
    let parent = requirement.any_of.first().cloned().unwrap_or_default();
    let labelled = warn || reads.is_some() || phase.is_some();
    let label = labelled.then(|| {
        rsx! {
            if warn {
                span { class: "pack-tree__warn",
                    Icon { name: "error", size: 16 }
                    "deploys while " code { "{parent}" } " is off"
                }
            }
            if let Some(reads) = &reads {
                span {
                    class: "pack-tree__reads",
                    title: "It reads params the pack above declares.",
                    Icon { name: "link", size: 16 }
                    "reads " code { "{reads}" }
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
                PackBranch { key: "{child.node.row.path}", branch: child, report: report.clone(), questions: questions.clone(), loading }
            }
        }
    });
    rsx! {
        ConnectorBranch {
            line: if requirement.kind == RequirementKind::Data { ConnectorLine::Dashed } else { ConnectorLine::Solid },
            error: warn,
            trunk_error: trunk_warn,
            label,
            branches,
            node: rsx! { PackCard { row, report: report.clone(), questions: questions.clone(), loading } },
        }
    }
}

/// The word a line state reads as on a chip and in a sentence.
trait Word {
    fn word(self) -> &'static str;
}

impl Word for PackLine {
    fn word(self) -> &'static str {
        match self {
            PackLine::Active => "active",
            PackLine::Ungated => "ungated",
            PackLine::Commented => "commented out",
            PackLine::Absent => "absent",
            PackLine::Forked => "forked",
            PackLine::Misplaced => "misplaced",
        }
    }
}

fn line_chip(row: &PackRow) -> Element {
    let icon = if row.deploys {
        "check_circle"
    } else {
        "radio_button_unchecked"
    };
    let error = matches!(row.line, PackLine::Ungated | PackLine::Misplaced);
    let label = match row.at_line {
        Some(at) => format!("{} · line {at}", row.line.word()),
        None => row.line.word().to_string(),
    };
    rsx! { Chip { kind: ChipKind::Assist, icon, label, error } }
}

/// What a line state the chip marks in the error colour means, and the edit that answers
/// it: a sentence for `ungated` and for `misplaced`, `None` for every other state, which
/// the chip says in full.
///
/// satz says this itself where it can — the `ungated-pack` finding, which the card shows
/// with satz's own `fix:` — and it can only where the file declaring the gate deploys, so
/// an estate with the map off hears nothing about a line no answer switches off. This is
/// what the card says then: a row that carries a finding of its own has none, because
/// satz's wording wins wherever satz speaks.
fn line_note(row: &PackRow) -> Option<String> {
    if !row.findings.is_empty() {
        return None;
    }
    match (row.line, row.gate.as_deref()) {
        (PackLine::Ungated, Some(gate)) => Some(format!(
            "No gate: this line carries no `when {gate}`, so a no to `{gate}` does not switch \
             the pack off. Write `when {gate}` on it so the choice decides the pack, or comment \
             the line out to take the pack off."
        )),
        (PackLine::Ungated, None) => Some(
            "No gate: this line carries no `when`, so the file decides the pack and no question \
             does. Write a `when <param>` on it to make it a choice, or comment the line out to \
             take the pack off."
                .to_string(),
        ),
        (PackLine::Misplaced, _) => Some(
            "Out of place: this line stands outside the block the pack graph places the pack in. \
             Move it into that block, or take the line out and switch the pack on here — satz \
             then writes it where the graph places it."
                .to_string(),
        ),
        _ => None,
    }
}

/// What the gate is in this estate, as satz reports it: the value, and whether the estate
/// answered it or the library's default gives it.
fn gate_text(row: &PackRow, gate: &str) -> String {
    let value = row
        .value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "unset".to_string());
    match (&row.answer, &row.default) {
        (Some(a), _) => format!("{gate} = {value} (answered {a})"),
        (None, Some(d)) => format!("{gate} = {value} (default {d})"),
        (None, None) => format!("{gate} = {value}"),
    }
}

#[component]
fn PackCard(
    row: PackRow,
    report: PacksReport,
    questions: Option<QuestionsReport>,
    loading: bool,
) -> Element {
    let handle = use_coroutine_handle::<EstateAction>();
    let prompt = row
        .gate
        .as_deref()
        .zip(questions.as_ref())
        .and_then(|(g, q)| prompt_of(q, g));
    // (met, the packs any of which meets it, the params a data requirement reads, the
    // one pack whose switch meets it)
    let requirements: Vec<(bool, String, Option<String>, Option<String>)> =
        listed_requirements(&row, &report)
            .into_iter()
            .map(|r| {
                (
                    r.met,
                    r.any_of.join(" or "),
                    (r.kind == RequirementKind::Data && !r.params.is_empty())
                        .then(|| r.params.join(", ")),
                    match (r.met, r.any_of.as_slice()) {
                        (false, [one]) => Some(one.clone()),
                        _ => None,
                    },
                )
            })
            .collect();
    let gate = row.gate.as_deref().map(|g| gate_text(&row, g));
    let label = row.gate.clone().unwrap_or_else(|| row.path.clone());
    let required_by = row.required_by.join(", ");
    let excludes = row.excludes.join(", ");
    let findings = row_findings(&report, &row);
    let note = line_note(&row);
    let switchable = row.role == PackRole::Pack && row.by_hand.is_none();
    let path = row.path.clone();
    rsx! {
        Card { variant: CardVariant::Outlined, class: "pack-card",
            div { class: "pack-card__head",
                Icon { name: "extension", size: 20 }
                code { class: "pack-card__path", "{row.path}" }
                span { class: "grow" }
                {line_chip(&row)}
            }
            if let Some((prompt, why)) = &prompt {
                p { class: "pack-card__prompt", "{prompt}" }
                if let Some(why) = why {
                    p { class: "pack-card__why", "{why}" }
                }
            }
            if let Some(gate) = &gate {
                p { class: "pack-card__note", code { "{gate}" } }
            }
            if let Some(written) = &row.written {
                p { class: "pack-card__note", "The line names " code { "{written}" } "." }
            }
            if let Some(on) = &row.gated_on {
                p { class: "pack-card__note", "The line is gated on " code { "{on}" } ", not on the pack's own gate." }
            }
            if row.role == PackRole::Core {
                p { class: "pack-card__why", "The day-0 pack every estate starts with: satz does not switch it." }
            }
            if let Some(why) = &row.by_hand {
                p { class: "pack-card__why", "Its line is written by hand, never by satz: {why}." }
            }
            if switchable {
                div { class: "pack-card__control",
                    Switch {
                        label,
                        checked: row.deploys,
                        disabled: loading,
                        onchange: move |on: bool| {
                            let pack = path.clone();
                            handle.send(if on {
                                EstateAction::AddPack(AddPackArgs { pack, with_requirements: false })
                            } else {
                                EstateAction::RemovePack(RemovePackArgs { pack, cascade: false })
                            })
                        },
                    }
                    span { class: "pack-card__note",
                        if row.deploys {
                            "On. Off binds the gate false and leaves the line: a gated line with a false gate deploys nothing."
                        } else {
                            "Off. On binds the gate true and makes the line active where the pack graph places it."
                        }
                    }
                }
            }
            if !requirements.is_empty() {
                ul { class: "pack-card__lines",
                    for (i, (met, any_of, reads, one)) in requirements.into_iter().enumerate() {
                        li { key: "{i}", class: "pack-card__line",
                            Icon { name: if met { "check_circle" } else { "error" }, size: 18 }
                            span { class: "grow",
                                "needs " code { "{any_of}" }
                                if let Some(reads) = reads {
                                    " (reads " code { "{reads}" } ")"
                                }
                            }
                            if let Some(one) = one {
                                Button {
                                    variant: ButtonVariant::Text,
                                    icon: "toggle_on",
                                    disabled: loading,
                                    onclick: move |_| handle.send(EstateAction::AddPack(AddPackArgs { pack: one.clone(), with_requirements: false })),
                                    "Switch on"
                                }
                            }
                        }
                    }
                }
            }
            if row.role == PackRole::Pack && !row.required_by.is_empty() {
                p { class: "pack-card__note", "Needed by " code { "{required_by}" } "." }
            }
            if !row.excludes.is_empty() {
                p { class: "pack-card__note", "Excludes " code { "{excludes}" } "." }
            }
            for n in row.notices.iter() {
                div { key: "{n.param}", class: "pack-card__remedy",
                    Icon { name: if n.acknowledged { "task_alt" } else { "assignment_late" }, size: 20 }
                    span {
                        if n.acknowledged { "Ran " } else if row.deploys { "Run " } else { "Once on, run " }
                        code { "{n.run}" } ", then bind " code { "{n.param} = true" } "."
                    }
                }
            }
            if let Some(note) = &note {
                p { class: "pack-card__why", "{note}" }
            }
            for (i, (message, fix)) in findings.iter().enumerate() {
                div { key: "{i}", class: "pack-card__diag",
                    Icon { name: "error", size: 18 }
                    span {
                        "{message}"
                        if let Some(fix) = fix {
                            br {}
                            code { "{fix}" }
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
    use satz_studio_core::satz::reports::FindingSeverity;

    fn row(path: &str, at_line: Option<u32>) -> PackRow {
        PackRow {
            path: path.to_string(),
            role: PackRole::Pack,
            gate: Some(format!("use_{}", path.trim_end_matches(".satz"))),
            gate_declared_in: Some("presets/estate-map.satz".to_string()),
            answer: None,
            default: None,
            value: None,
            line: if at_line.is_some() {
                PackLine::Commented
            } else {
                PackLine::Absent
            },
            at_line,
            written: None,
            gated_on: None,
            deploys: false,
            requires: vec![need(
                RequirementKind::Gate,
                &["presets/estate-map.satz"],
                true,
            )],
            required_by: Vec::new(),
            excludes: Vec::new(),
            by_hand: None,
            notices: Vec::new(),
            contributes: Vec::new(),
            findings: Vec::new(),
        }
    }

    fn need(kind: RequirementKind, any_of: &[&str], met: bool) -> Requirement {
        Requirement {
            kind,
            any_of: any_of.iter().map(|p| p.to_string()).collect(),
            params: Vec::new(),
            met,
        }
    }

    /// `child` needs `parent`, and whether that is met.
    fn needs(mut child: PackRow, parent: &str, met: bool) -> PackRow {
        child
            .requires
            .push(need(RequirementKind::Data, &[parent], met));
        child
    }

    fn report(mut packs: Vec<PackRow>) -> PacksReport {
        let mut map = row("presets/estate-map.satz", Some(1));
        map.role = PackRole::Map;
        map.gate = None;
        map.requires = Vec::new();
        map.line = PackLine::Active;
        map.deploys = true;
        packs.insert(0, map);
        PacksReport {
            estate: "C0example.satz".to_string(),
            note: None,
            packs,
            unmanaged: Vec::new(),
            interfaces: Vec::new(),
            findings: Vec::new(),
        }
    }

    fn phases(v: &[(u32, &str)]) -> BTreeMap<u32, String> {
        v.iter().map(|(l, p)| (*l, p.to_string())).collect()
    }

    /// `a[b[d, e], c]` for a tree, `a` for a cell.
    fn outline(item: &Item) -> String {
        fn node(n: &Node) -> String {
            if n.children.is_empty() {
                return n.row.path.clone();
            }
            let children: Vec<String> = n.children.iter().map(|b| node(&b.node)).collect();
            format!("{}[{}]", n.row.path, children.join(", "))
        }
        match item {
            Item::Cell(r) => r.path.clone(),
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
                Item::Tree(n) if n.row.path == root => Some(n),
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
    fn lines_go_in_file_order_under_their_phase_and_packs_without_one_go_last() {
        // the graph's order is not the file's: the file decides the sections
        let r = report(vec![
            row("c.satz", Some(30)),
            row("z.satz", None),
            row("a.satz", Some(10)),
            row("b.satz", Some(20)),
            row("y.satz", None),
        ]);
        let p = phases(&[(10, "once A\ndetail"), (30, "once C")]);
        assert_eq!(
            shape(&sections(&r, &p)),
            owned(&[
                ("once A", &["a.satz", "b.satz"]),
                ("once C", &["c.satz"]),
                (ABSENT_HEADER, &["z.satz", "y.satz"]),
            ])
        );
    }

    #[test]
    fn a_line_before_any_phase_opens_the_no_phase_section() {
        let r = report(vec![row("a.satz", Some(3))]);
        assert_eq!(sections(&r, &BTreeMap::new())[0].header, NO_PHASE_HEADER);
    }

    /// The SCC and archive shape: the mail needs the central alerts, which need the
    /// archive; the SIEM needs the notifications — three levels, children leaving their
    /// phase.
    fn scc() -> (PacksReport, BTreeMap<u32, String>) {
        let r = report(vec![
            row("budget.satz", Some(10)),
            row("archive.satz", Some(11)),
            needs(row("alerts.satz", Some(12)), "archive.satz", false),
            row("notify.satz", Some(20)),
            needs(row("siem.satz", Some(30)), "notify.satz", false),
            needs(
                needs(row("mail.satz", Some(40)), "alerts.satz", false),
                "notify.satz",
                false,
            ),
            needs(row("sentinel.satz", Some(50)), "archive.satz", false),
        ]);
        let p = phases(&[
            (10, "once alone"),
            (20, "once SCC is on"),
            (30, "once the topic exists"),
            (40, "once the alerts are in"),
            (50, "once the archive"),
        ]);
        (r, p)
    }

    #[test]
    fn a_pack_another_needs_is_a_tree_the_rest_stay_cells_and_children_leave_their_phase() {
        let (r, p) = scc();
        let s = sections(&r, &p);
        assert_eq!(
            shape(&s),
            owned(&[
                (
                    "once alone",
                    &[
                        "budget.satz",
                        "archive.satz[alerts.satz[mail.satz], sentinel.satz]",
                    ]
                ),
                ("once SCC is on", &["notify.satz[siem.satz]"]),
            ]),
            "the mail hangs from its first requirement; the emptied phases are gone"
        );
        let archive = tree(&s, "archive.satz");
        assert_eq!(
            archive.children[0].phase, None,
            "the alerts share its phase"
        );
        assert_eq!(
            archive.children[1].phase.as_deref(),
            Some("once the archive")
        );
        assert_eq!(
            archive.children[0].node.children[0].phase.as_deref(),
            Some("once the alerts are in")
        );
        assert_eq!(
            archive.children[0].requirement.kind,
            RequirementKind::Data,
            "the branch carries satz's requirement"
        );
    }

    #[test]
    fn a_requirement_several_packs_meet_and_the_map_s_hang_a_card_nowhere() {
        let mut billing = row("billing.satz", Some(12));
        billing.requires.insert(
            0,
            need(RequirementKind::Requires, &["s1.satz", "s2.satz"], true),
        );
        let r = report(vec![
            row("s1.satz", Some(10)),
            row("s2.satz", Some(11)),
            billing,
        ]);
        assert_eq!(
            shape(&sections(&r, &BTreeMap::new())),
            owned(&[(NO_PHASE_HEADER, &["s1.satz", "s2.satz", "billing.satz"])])
        );
    }

    #[test]
    fn a_child_that_deploys_while_its_requirement_is_off_warns_and_the_trunk_leads_to_it() {
        let mut deploys = needs(row("extra.satz", Some(13)), "scc.satz", false);
        deploys.deploys = true;
        let mut met = needs(row("export.satz", Some(12)), "scc.satz", true);
        met.deploys = true;
        let r = report(vec![
            row("scc.satz", Some(10)),
            needs(row("notify.satz", Some(11)), "scc.satz", false),
            met,
            deploys,
        ]);
        let s = sections(&r, &BTreeMap::new());
        let warns: Vec<(bool, bool)> = tree(&s, "scc.satz")
            .children
            .iter()
            .map(|b| (b.warn, b.trunk_warn))
            .collect();
        assert_eq!(warns, [(false, true), (false, true), (true, false)]);
    }

    #[test]
    fn packs_that_need_each_other_stay_at_the_top_where_the_loop_closes() {
        let r = report(vec![
            needs(row("a.satz", Some(10)), "b.satz", false),
            needs(row("b.satz", Some(11)), "a.satz", false),
        ]);
        assert_eq!(
            shape(&sections(&r, &BTreeMap::new())),
            owned(&[(NO_PHASE_HEADER, &["a.satz[b.satz]"])])
        );
    }

    #[test]
    fn an_absent_child_hangs_under_its_parent_without_a_phase_caption() {
        let r = report(vec![
            row("scc.satz", Some(10)),
            needs(row("notify.satz", None), "scc.satz", false),
        ]);
        let s = sections(&r, &phases(&[(10, "once alone")]));
        assert_eq!(
            shape(&s),
            owned(&[("once alone", &["scc.satz[notify.satz]"])])
        );
        assert_eq!(tree(&s, "scc.satz").children[0].phase, None);
    }

    #[test]
    fn a_card_lists_what_is_off_and_what_the_tree_does_not_draw() {
        let mut mail = needs(row("mail.satz", Some(12)), "alerts.satz", true);
        mail.requires
            .push(need(RequirementKind::Data, &["notify.satz"], false));
        mail.requires.push(need(
            RequirementKind::Requires,
            &["s1.satz", "s2.satz"],
            true,
        ));
        let r = report(vec![row("alerts.satz", Some(10)), mail.clone()]);
        let listed: Vec<Vec<String>> = listed_requirements(&mail, &r)
            .into_iter()
            .map(|q| q.any_of.clone())
            .collect();
        assert_eq!(
            listed,
            [
                vec!["notify.satz".to_string()],
                vec!["s1.satz".to_string(), "s2.satz".to_string()]
            ],
            "the map and the drawn parent are not listed while they are met"
        );
        let mut off = mail;
        off.requires[0].met = false;
        assert_eq!(
            listed_requirements(&off, &r).len(),
            3,
            "the map is listed once off"
        );
    }

    /// satz's own report for its smoke estate, recorded from the release the app is
    /// tested against: the dependencies no `ask_when` declares are trees all the same.
    #[test]
    fn the_recorded_report_hangs_every_pack_under_the_one_it_needs() {
        let r: PacksReport = serde_json::from_str(include_str!(
            "../../../satz-studio-core/tests/fixtures/packs-smoke.json"
        ))
        .unwrap();
        let s = sections(&r, &BTreeMap::new());
        let items: Vec<String> = s.iter().flat_map(|s| s.items.iter().map(outline)).collect();
        let hangs = |parent: &str, child: &str| {
            let (parent, child) = (format!("presets/{parent}"), format!("presets/{child}"));
            items
                .iter()
                .any(|i| i.contains(&format!("{parent}[")) && i.contains(&child))
        };
        assert!(
            hangs(
                "monitoring/organization-audit-logsink.satz",
                "monitoring/organization-cis-log-alerts-central.satz"
            ),
            "{items:#?}"
        );
        assert!(hangs(
            "monitoring/organization-cis-log-alerts-central.satz",
            "scc/scc-findings-mail.satz"
        ));
        assert!(hangs(
            "monitoring/organization-audit-logsink.satz",
            "integrations/microsoft-sentinel.satz"
        ));
        assert!(hangs(
            "ci/verification-runner.satz",
            "ci/verification-runner-grant.satz"
        ));
        // billing needs one of the two security-group models: a choice, not a parent
        let billing = r.row("presets/billing-account-permissions.satz").unwrap();
        assert!(parent_of(billing, &r.packs).is_none());
        assert!(
            listed_requirements(billing, &r)
                .iter()
                .any(|q| q.any_of.len() == 2 && !q.met)
        );
        assert!(
            s.iter().all(|s| s.items.iter().all(|i| match i {
                Item::Cell(row) => row.role != PackRole::Map,
                Item::Tree(n) => n.row.role != PackRole::Map,
            })),
            "the map is the page's head"
        );
    }

    /// satz reports `ungated-pack` only where the file declaring the gate deploys, so an
    /// estate with the map off hears it from the card alone.
    #[test]
    fn an_ungated_line_says_what_it_means_and_names_its_gate() {
        let mut budget = row("presets/organization-budget.satz", Some(12));
        budget.line = PackLine::Ungated;
        let gate = budget.gate.clone().unwrap();
        let note = line_note(&budget).expect("an ungated line carries a note");
        assert!(note.contains(&format!("when {gate}")), "{note}");
        assert!(note.contains("comment the line out"), "{note}");

        let misplaced = PackRow {
            line: PackLine::Misplaced,
            ..budget.clone()
        };
        assert!(
            line_note(&misplaced)
                .is_some_and(|n| n.contains("outside the block the pack graph places")),
            "a misplaced line carries a note"
        );

        // every other state is what the chip says
        budget.line = PackLine::Active;
        assert_eq!(line_note(&budget), None);

        // and satz's own sentence wins wherever satz speaks
        budget.line = PackLine::Ungated;
        budget.findings = vec![
            "`presets/organization-budget.satz` is used without `when use_budget`".to_string(),
        ];
        assert_eq!(line_note(&budget), None);
    }

    #[test]
    fn a_finding_carries_the_command_satz_names_for_it() {
        let mut logsink = row(
            "presets/monitoring/organization-audit-logsink.satz",
            Some(9),
        );
        logsink.findings = vec!["needs the map".to_string(), "another".to_string()];
        let mut r = report(vec![logsink.clone()]);
        let finding = |subject: &str, message: &str, fix: Option<&str>| Finding {
            severity: FindingSeverity::Warning,
            kind: "pack-requirement".to_string(),
            group: None,
            file: None,
            line: Some(9),
            subject: Some(subject.to_string()),
            message: message.to_string(),
            fix: fix.map(str::to_string),
        };
        r.findings = vec![
            finding(
                &logsink.path,
                "needs the map",
                Some("satz add-pack C0example.satz presets/estate-map.satz"),
            ),
            finding("presets/gone.satz", "a pack the graph has no row for", None),
        ];
        assert_eq!(
            row_findings(&r, &logsink),
            [
                (
                    "needs the map".to_string(),
                    Some("satz add-pack C0example.satz presets/estate-map.satz".to_string())
                ),
                ("another".to_string(), None),
            ]
        );
        let loose = loose_findings(&r);
        assert_eq!(loose.len(), 1);
        assert_eq!(loose[0].message, "a pack the graph has no row for");
    }
}
