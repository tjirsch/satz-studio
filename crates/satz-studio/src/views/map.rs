//! The Map view: the pack rows — the map line first, then the gated lines under the
//! phase each can be adopted in, then the choices the file has no line for. A toggle
//! is an answer written by `satz_interview`; the one line the app writes itself is the
//! map's, which no question gates.

use dioxus::prelude::*;
use satz_studio_core::diag::{DiagSource, Diagnostic};
use satz_studio_core::model::{Choice, LineState, PackRow, PackRowKind};
use satz_studio_core::satz::reports::QuestionRow;

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, Icon, Segment, SegmentedButton,
    Switch,
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

#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub header: String,
    pub entries: Vec<Entry>,
}

/// The phase a comment block names: its first line.
pub fn phase_header(phase: &str) -> String {
    phase.lines().next().unwrap_or_default().trim().to_string()
}

/// The choice rows in sections: a row with a phase comment opens the section that
/// comment's first line names, a row without one joins the section open at that point
/// (or [`NO_PHASE_HEADER`] when none is), and every `Absent` row goes last under
/// [`ABSENT_HEADER`]. Within a section the rows of one `oneof` group share an entry.
pub fn sections(rows: &[PackRow]) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut absent: Vec<PackRow> = Vec::new();
    for row in rows.iter().filter(|r| r.kind == PackRowKind::Choice) {
        if row.state == LineState::Absent {
            absent.push(row.clone());
            continue;
        }
        if let Some(phase) = &row.phase {
            out.push(Section {
                header: phase_header(phase),
                entries: Vec::new(),
            });
        } else if out.is_empty() {
            out.push(Section {
                header: NO_PHASE_HEADER.to_string(),
                entries: Vec::new(),
            });
        }
        let section = out.last_mut().expect("a section was opened above");
        push_entry(&mut section.entries, row.clone());
    }
    if !absent.is_empty() {
        let mut entries = Vec::new();
        for row in absent {
            push_entry(&mut entries, row);
        }
        out.push(Section {
            header: ABSENT_HEADER.to_string(),
            entries,
        });
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
pub fn MapView() -> Element {
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
            div { class: "view map",
                h1 { class: "view__title", "Map" }
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
    let sections = sections(&model.packs);

    rsx! {
        div { class: "view map",
            h1 { class: "view__title", "Map" }
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
                        }
                    },
                }
            }
            for section in sections {
                section { key: "{section.header}", class: "map__section",
                    h2 { class: "map__section-title", "{section.header}" }
                    div { class: "map__cards",
                        for (i, entry) in section.entries.into_iter().enumerate() {
                            {
                                let key = match &entry {
                                    Entry::Single(r) => format!("{}-{}", r.gate.clone().unwrap_or_default(), r.path.clone().unwrap_or_default()),
                                    Entry::Group { group, .. } => format!("group-{group}"),
                                };
                                rsx! {
                                    PackCard { key: "{i}-{key}", entry, notes: notes.clone(), loading }
                                }
                            }
                        }
                    }
                }
            }
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
                    if row.state == LineState::Absent {
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
        let s = sections(&rows);
        let shape: Vec<(String, Vec<String>)> = s
            .iter()
            .map(|sec| {
                (
                    sec.header.clone(),
                    sec.entries
                        .iter()
                        .map(|e| match e {
                            Entry::Single(r) => r.gate.clone().unwrap(),
                            Entry::Group { group, rows, .. } => format!("{group}:{}", rows.len()),
                        })
                        .collect(),
                )
            })
            .collect();
        assert_eq!(
            shape,
            vec![
                (
                    "once A".to_string(),
                    vec![
                        "use_a".to_string(),
                        "use_b".to_string(),
                        "model:2".to_string()
                    ]
                ),
                ("once C".to_string(), vec!["use_c".to_string()]),
                (ABSENT_HEADER.to_string(), vec!["use_z".to_string()]),
            ]
        );
    }

    #[test]
    fn a_row_before_any_phase_opens_the_no_phase_section() {
        let rows = vec![row(Some("a.satz"), "use_a", LineState::Off, None)];
        assert_eq!(sections(&rows)[0].header, NO_PHASE_HEADER);
    }
}
