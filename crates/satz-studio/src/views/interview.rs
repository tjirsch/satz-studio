//! The Decisions destination: the questions the estate's packs declare, one at a time, the
//! unanswered ones first. Every answer is one `satz_interview` call through the estate
//! coroutine, and the view re-renders from the reloaded report.
//!
//! The card has one filled button, and it reads Accept or Next. Accept writes the value
//! the card holds and moves the walk to the next question; Next moves it and writes
//! nothing. An unanswered question reads Accept whether or not its value was changed —
//! accepting the offered default writes it — and carries a text Skip, which moves on
//! without writing. An answered question reads Next while its field holds the written
//! value, or its option buttons the bound one, and Accept as soon as the value differs from it;
//! it has no Skip. Editing the field or picking an option writes nothing, and neither does
//! the field losing focus; Enter in a text or number field is the keyboard form of the
//! filled button. The walk follows its question by subject and remembers the questions
//! it moved away from, so Back returns to one whether it is answered by then or not; with
//! "Show answered" on, every question is listed beside the card. Back, Skip and the filled
//! button stand in a bar below the question, which scrolls above it. A typed answer's
//! field has the shape `answer_kind` reads off the report — the shape the pack declares
//! the param with — for a param every pack declares `none`, the shape satz reads from where
//! it is read (satz's ADR 0108) — else the offered value's, else text, so a list that
//! offers nothing is still a list.

use dioxus::prelude::*;
use dioxus_icons::lucide::{
    ArrowLeft, ArrowRight, Ban, Boxes, Check, CheckCheck, Circle, CircleCheck, CircleCheckBig,
    Code, DoorOpen, History, Lightbulb, MessageCircleQuestionMark, OctagonAlert, PencilLine,
    Sparkles,
};
use std::collections::BTreeMap;

use satz_studio_core::model::{SourceValue, answer_kind};
use satz_studio_core::satz::reports::{
    Blast, NO_BRANCH, QuestionKind, QuestionRow, QuestionState, Reversal,
};

use crate::components::badge::{Badge, BadgeVariant};
use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::card::{Card, CardContent};
use crate::components::item::{
    Item, ItemContent, ItemDescription, ItemGroup, ItemMedia, ItemSize, ItemTitle,
};
use crate::components::progress::Progress;
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, View};
use crate::views::value_source;
use crate::widgets::{Draft, FieldKind, SwitchRow, Tip, TypedField, value_in_words};

/// The questions in the order the view walks them: the unanswered ones as the report
/// lists them, then — when asked for — the answered ones and the ones not asked.
pub fn ordered(questions: &[QuestionRow], show_answered: bool) -> Vec<QuestionRow> {
    let mut out: Vec<QuestionRow> = questions
        .iter()
        .filter(|q| q.state == QuestionState::Unanswered)
        .cloned()
        .collect();
    if show_answered {
        out.extend(
            questions
                .iter()
                .filter(|q| q.state == QuestionState::Answered)
                .cloned(),
        );
        out.extend(
            questions
                .iter()
                .filter(|q| q.state == QuestionState::NotApplicable)
                .cloned(),
        );
    }
    out
}

/// Where the walk stands. The card follows its question by `subject`, because a reload
/// reorders the list: an answer moves a question into the answered block, or out of the
/// walk while answered questions are hidden. `index` is where the card was, for when its
/// question has left the list — the question that took its place is shown. `left` holds
/// the questions the walk moved away from, the latest last, for Back.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Walk {
    subject: Option<String>,
    index: usize,
    left: Vec<String>,
}

impl Walk {
    /// The position of the card's question in `list`.
    pub fn position(&self, list: &[QuestionRow]) -> usize {
        self.subject
            .as_ref()
            .and_then(|s| list.iter().position(|q| &q.subject == s))
            .unwrap_or(self.index)
            .min(list.len().saturating_sub(1))
    }

    fn leave(&mut self, list: &[QuestionRow]) {
        if let Some(q) = list.get(self.position(list))
            && self.left.last() != Some(&q.subject)
        {
            self.left.push(q.subject.clone());
        }
    }

    /// Open the question at `to` in the walk's list, remembering the one on the card.
    pub fn open(&mut self, questions: &[QuestionRow], show_answered: bool, to: usize) {
        let list = ordered(questions, show_answered);
        let Some(q) = list.get(to) else { return };
        if to == self.position(&list) {
            return;
        }
        self.leave(&list);
        self.subject = Some(q.subject.clone());
        self.index = to;
    }

    /// Skip, or Next: the question after the card's, the first one after the last. The
    /// only question in the walk stays on the card: there is nowhere to go.
    pub fn next(&mut self, questions: &[QuestionRow], show_answered: bool) {
        self.step(questions, show_answered, false);
    }

    /// Accept: the card's question is being written, and the walk moves on as Next does.
    /// It is called with the report as it stands BEFORE the write, and the card follows
    /// the question it moves to by subject, so the reload that takes the answered
    /// question out of the unanswered block neither skips the next one nor carries the
    /// card back. The only question in the walk is let go when answered questions are
    /// hidden — the write takes it out of the walk — and Back returns to it.
    pub fn accept(&mut self, questions: &[QuestionRow], show_answered: bool) {
        self.step(questions, show_answered, true);
    }

    fn step(&mut self, questions: &[QuestionRow], show_answered: bool, answering: bool) {
        let list = ordered(questions, show_answered);
        if list.is_empty() {
            return;
        }
        let from = self.position(&list);
        let to = (from + 1) % list.len();
        if to != from {
            self.open(questions, show_answered, to);
        } else if answering && !show_answered {
            self.leave(&list);
            self.subject = None;
            self.index = 0;
        }
    }

    pub fn can_go_back(&self) -> bool {
        !self.left.is_empty()
    }

    /// Back: the question the walk last moved away from. `None` when there is none;
    /// otherwise whether answered questions have to be shown to hold it, because an
    /// answered question is not in the walk while they are hidden.
    pub fn back(&mut self, questions: &[QuestionRow], show_answered: bool) -> Option<bool> {
        while let Some(subject) = self.left.pop() {
            // a question the report no longer carries (its pack is gone) is passed over
            let Some(q) = questions.iter().find(|q| q.subject == subject) else {
                continue;
            };
            let show = show_answered || q.state != QuestionState::Unanswered;
            self.index = ordered(questions, show)
                .iter()
                .position(|r| r.subject == subject)
                .unwrap_or(0);
            self.subject = Some(subject);
            return Some(show);
        }
        None
    }

    /// "Show answered" switched from `from` to `to`: the card keeps its question when
    /// the new list holds it, and starts from the first question when it does not.
    pub fn switched(&mut self, questions: &[QuestionRow], from: bool, to: bool) {
        let before = ordered(questions, from);
        self.subject = before
            .get(self.position(&before))
            .map(|q| q.subject.clone());
        let after = ordered(questions, to);
        match self
            .subject
            .as_ref()
            .and_then(|s| after.iter().position(|q| &q.subject == s))
        {
            Some(i) => self.index = i,
            None => {
                self.subject = None;
                self.index = 0;
            }
        }
    }
}

/// The option the view starts on: the one the estate binds, else the one the pack
/// offers as its default, else nothing chosen. A choice that is not required binds and
/// offers [`NO_BRANCH`] like an option.
pub fn initial_option(q: &QuestionRow) -> Option<String> {
    q.bound_option().map(str::to_string).or_else(|| {
        let default = q.default.as_ref()?.as_str()?;
        let known = q.options.iter().any(|o| o.param == default)
            || (q.offers_none() && default == NO_BRANCH);
        known.then(|| default.to_string())
    })
}

/// The option buttons of a choice: each option's param and label, then [`NO_BRANCH`] where the
/// choice is not required.
pub fn choices(q: &QuestionRow) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = q
        .options
        .iter()
        .map(|o| (o.param.clone(), o.label.clone()))
        .collect();
    if q.offers_none() {
        out.push((NO_BRANCH.to_string(), "None".to_string()));
    }
    out
}

/// What the card's filled button does: write the card's value and move on, or move on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimaryAction {
    Accept,
    Next,
}

/// The card's filled button: its words, what a click does and whether it can be clicked.
/// Its icon follows from what a click does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Primary {
    pub label: &'static str,
    pub action: PrimaryAction,
    pub enabled: bool,
}

impl Primary {
    /// The Lucide icon of the button, `size` pixels square.
    pub fn icon(self, size: u32) -> Element {
        match self.action {
            PrimaryAction::Accept => rsx! { Check { size } },
            PrimaryAction::Next => rsx! { ArrowRight { size } },
        }
    }
}

/// The filled button of a card with a field or option buttons. `changed` is the card's value
/// differing from the one the estate binds — for a question that is not answered it
/// does not matter; `valid` is the value being one that can be written: no problem in
/// the field, and not empty where the question offers nothing, or an option chosen.
///
/// A question not answered reads Accept, which writes what the card holds, the offered
/// default included. An answered one — or one not asked — reads Next while its value is
/// the bound one, and Accept once it differs. Accept is disabled on a value that cannot
/// be written and while a write is in flight; Next writes nothing and is never disabled.
pub fn primary(state: QuestionState, changed: bool, valid: bool, loading: bool) -> Primary {
    if state != QuestionState::Unanswered && !changed {
        Primary {
            label: "Next",
            action: PrimaryAction::Next,
            enabled: true,
        }
    } else {
        Primary {
            label: "Accept",
            action: PrimaryAction::Accept,
            enabled: valid && !loading,
        }
    }
}

/// The text button that moves on beside the filled one: Skip on a question not answered;
/// on any other only where the card has no filled button — a param with no field — and
/// there it reads Next. An answered question with a filled button has no Skip: its filled
/// button reads Next.
pub fn text_forward(state: QuestionState, has_primary: bool) -> Option<&'static str> {
    if state == QuestionState::Unanswered {
        Some("Skip")
    } else if has_primary {
        None
    } else {
        Some("Next")
    }
}

/// What the card holds, ready for its filled button: whether it differs from the bound
/// value, whether it can be written, and the value `satz_interview` is sent.
#[derive(Debug, Clone, PartialEq)]
pub struct Held {
    pub changed: bool,
    pub valid: bool,
    pub value: serde_json::Value,
}

/// A typed answer's field. `offered` is the report's value — the estate's own for an
/// answered question, else the pack's default — which is where the field starts.
/// `empty_answers` is the question saying what `""` means (`empty = "…"`), which makes
/// an empty field an answer.
pub fn held_param(
    kind: FieldKind,
    subject: &str,
    offered: Option<&serde_json::Value>,
    empty_answers: bool,
    draft: &Draft,
) -> Held {
    Held {
        changed: *draft != Draft::of_json(offered, kind),
        // nothing typed is no answer to a question that offers nothing, unless the
        // question says what nothing means
        valid: draft.problem(kind, subject).is_none()
            && (offered.is_some() || empty_answers || !draft.is_empty()),
        value: draft.to_json(),
    }
}

/// A `oneof`'s option buttons. `chosen` is the option picked on the card, `bound` the option the
/// estate binds.
pub fn held_oneof(chosen: Option<&str>, bound: Option<&str>) -> Held {
    Held {
        changed: chosen != bound,
        valid: chosen.is_some(),
        value: chosen.map_or(serde_json::Value::Null, |c| {
            serde_json::Value::String(c.to_string())
        }),
    }
}

/// A JSON value as the interview prints it beside the recommendation.
fn shown(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Whether the pack's recommendation is something other than what is offered.
pub fn recommends_otherwise(q: &QuestionRow) -> Option<&str> {
    let r = q.recommend.as_deref()?;
    match q.offered() {
        Some(offered) if shown(offered) == r => None,
        _ => Some(r),
    }
}

/// An answer the estate file writes as something only the file can express — a named
/// read, a reference to a param, a string that interpolates — which no typed field holds
/// without losing what it reads. The questions report carries such an answer resolved or
/// decoded (`${interface.rag-store.project_id}`, a referenced param's value), so the card
/// takes it from the estate file instead and shows it there, never in a field.
#[derive(Debug, Clone, PartialEq)]
pub struct InFile {
    /// the value as the estate file writes it: `"${{interface.rag-store.project_id}}"`
    pub source: String,
    /// what it reads, in words
    pub words: String,
}

/// The [`InFile`] answer of a question: an answered param question whose value in the
/// estate file — `value`, written as `source` — is not a literal. `None` for a literal,
/// which the field shows and edits, and for a question not answered.
pub fn in_file(q: &QuestionRow, value: &SourceValue, source: String) -> Option<InFile> {
    if q.state != QuestionState::Answered || q.kind != QuestionKind::Param {
        return None;
    }
    value_in_words(value).map(|words| InFile { source, words })
}

/// A question's state in words, as the card's badge and the list say it. A choice says
/// its bound option's label, or `none`; an answer only the estate file can express says
/// it as the file writes it.
pub fn state_of(q: &QuestionRow, in_file: Option<&InFile>) -> String {
    let answer = match (q.kind, in_file) {
        (QuestionKind::Oneof, _) => q.bound_label().unwrap_or_default(),
        (QuestionKind::Param, Some(f)) => f.source.clone(),
        (QuestionKind::Param, None) => q.current.as_ref().map(|v| q.shown(v)).unwrap_or_default(),
    };
    match q.state {
        QuestionState::Unanswered if q.blocking => "needs a value".to_string(),
        QuestionState::Unanswered => "open".to_string(),
        QuestionState::Answered => format!("answered: {answer}"),
        QuestionState::NotApplicable => "not asked: its ask_when is false".to_string(),
    }
}

/// The Lucide icon of a question's state, `size` pixels square, beside the words of
/// [`state_of`].
pub fn state_icon(q: &QuestionRow, size: u32) -> Element {
    match q.state {
        QuestionState::Unanswered if q.blocking => rsx! { OctagonAlert { size } },
        QuestionState::Unanswered => rsx! { Circle { size } },
        QuestionState::Answered => rsx! { CircleCheck { size } },
        QuestionState::NotApplicable => rsx! { Ban { size } },
    }
}

/// The list's supporting line for a question: its subject, then its state.
pub fn listed(q: &QuestionRow, in_file: Option<&InFile>) -> String {
    format!("{} · {}", q.subject, state_of(q, in_file))
}

/// The card's identity: its question, its field's shape and the value it starts on. A
/// reload that changes the value a card starts on — a default derived from the answer
/// just written, reaching the card the walk moved to while the write was in flight —
/// starts that card afresh, so it never holds a value the report no longer offers.
/// The field is disabled while a write is in flight, so nothing typed is lost.
fn card_key(q: &QuestionRow, field: Option<FieldKind>, in_file: Option<&InFile>) -> String {
    let offered = q.offered().map(shown).unwrap_or_default();
    let option = initial_option(q).unwrap_or_default();
    let written = in_file.map(|f| f.source.as_str()).unwrap_or_default();
    format!("{}|{field:?}|{offered}|{option}|{written}", q.subject)
}

#[component]
pub fn DecisionsView() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let report = app.estate().questions().cloned();
    let interview = app.estate().interview().cloned();
    let loading = app.estate().loading().cloned();
    let mut show_answered = use_signal(|| false);
    let mut walk = use_signal(Walk::default);

    let Some(report) = report else {
        return rsx! {
            div { class: "view decisions interview",
                h1 { class: "view__title", "Decisions" }
                Card {
                    CardContent { class: "interview__empty",
                        MessageCircleQuestionMark { size: 48, class: "placeholder__icon" }
                        p { "The questions report is not available — the drawer says why." }
                    }
                }
            }
        };
    };
    // The handlers read the report when they run: an answer reloads it between renders.
    let questions_now = move || {
        app.estate()
            .questions()
            .cloned()
            .map(|r| r.questions)
            .unwrap_or_default()
    };
    let mut go_back = move || {
        let show = walk.write().back(&questions_now(), show_answered());
        if let Some(show) = show
            && show != show_answered()
        {
            show_answered.set(show);
        }
    };

    // the answers only the estate file can express, by subject, read from the file itself
    let in_files: BTreeMap<String, InFile> =
        match (app.estate().model().cloned(), app.estate().cst().cloned()) {
            (Some(model), Some(cst)) => report
                .questions
                .iter()
                .filter_map(|q| {
                    let row = model.params.iter().find(|r| r.name == q.subject)?;
                    let source = value_source(&cst, row.id)?;
                    in_file(q, &row.value, source).map(|f| (q.subject.clone(), f))
                })
                .collect(),
            _ => BTreeMap::new(),
        };
    let s = report.summary.clone();
    let list = ordered(&report.questions, show_answered());
    let offered_defaults = report
        .questions
        .iter()
        .filter(|q| q.state == QuestionState::Unanswered && q.default.is_some())
        .count();
    let i = walk.read().position(&list);
    let can_back = walk.read().can_go_back();
    let current = list.get(i).cloned();
    // the field's shape, as the report declares it; `None` when there is no field to
    // type the answer in — a map
    let field = current
        .as_ref()
        .and_then(answer_kind)
        .map(FieldKind::of_param);
    let previous_group = i
        .checked_sub(1)
        .and_then(|p| list.get(p))
        .map(QuestionRow::group);
    let progress = if s.total == 0 {
        1.0
    } else {
        s.answered as f32 / s.total as f32
    };
    let rename_to = interview.as_ref().and_then(|r| r.rename_to.clone());

    rsx! {
        div { class: "view decisions interview",
            div { class: "interview__head",
                h1 { class: "view__title", "Decisions" }
                span { class: "grow" }
                SwitchRow {
                    label: "Show answered",
                    checked: show_answered(),
                    onchange: move |v| {
                        walk.write().switched(&questions_now(), show_answered(), v);
                        show_answered.set(v);
                    },
                }
                Button {
                    variant: ButtonVariant::Secondary,
                    disabled: loading || offered_defaults == 0,
                    onclick: move |_| handle.send(EstateAction::AcceptDefaults),
                    CheckCheck { size: 16 }
                    "Accept {offered_defaults} defaults"
                }
            }
            div { class: "interview__progress",
                Progress { value: Some(f64::from(progress) * 100.0) }
                span { class: "interview__progress-text",
                    if s.complete {
                        "{s.answered} of {s.total} answered · nothing open — bootstrap and apply will not refuse this estate for an open question"
                    } else {
                        "{s.answered} of {s.total} answered · {s.unanswered} open · {s.blocking} need a value · {s.one_way_doors} one-way doors"
                    }
                }
            }
            if let Some(name) = rename_to {
                Card {
                    CardContent { class: "interview__rename",
                        PencilLine { size: 24, class: "placeholder__icon" }
                        p {
                            "init would have named this file " code { "{name}" }
                            ": rename it (" code { "git mv" } ") and change its " code { "estate" } " line to match."
                        }
                    }
                }
            }
            div { class: "interview__walk",
                match current {
                    Some(q) => rsx! {
                        QuestionCard {
                            key: "{card_key(&q, field, in_files.get(&q.subject))}",
                            in_file: in_files.get(&q.subject).cloned(),
                            question: q,
                            field,
                            previous_group,
                            loading,
                            can_back,
                            onback: move |_| go_back(),
                            onnext: move |_| {
                                walk.write().next(&questions_now(), show_answered());
                            },
                            onaccept: move |(subject, value): (String, serde_json::Value)| {
                                // the walk moves first, over the report as it stands before
                                // the write; the reload the write triggers finds the card on
                                // the question it moved to by subject
                                walk.write().accept(&questions_now(), show_answered());
                                handle.send(EstateAction::Answer { subject, value });
                            },
                        }
                    },
                    None => rsx! {
                        Card {
                            CardContent { class: "interview__empty",
                                MessageCircleQuestionMark { size: 48, class: "placeholder__icon" }
                                if s.total == 0 {
                                    p { "No pack this estate uses asks a question." }
                                } else {
                                    p { "Nothing is open. Switch on \"Show answered\" to revisit an answer." }
                                }
                                if can_back {
                                    Button { variant: ButtonVariant::Ghost, onclick: move |_| go_back(), ArrowLeft { size: 16 } "Back" }
                                }
                            }
                        }
                    },
                }
                // The list stands beside the walk whatever the switch says: the switch
                // decides what is IN it, not whether it is there. It keeps its own
                // height and its own scrollbar, so a long list moves nothing else.
                Card { class: "interview__list",
                    CardContent {
                        if list.is_empty() {
                            div { class: "interview__list-empty",
                                CircleCheckBig { size: 24 }
                                p {
                                    if s.total == 0 {
                                        "No pack this estate uses asks a question."
                                    } else if show_answered() {
                                        "No question to list."
                                    } else {
                                        "No unanswered question. Switch on \"Show answered\" to list the answered ones."
                                    }
                                }
                            }
                        } else {
                            ItemGroup {
                                for (n, q) in list.iter().enumerate() {
                                    Item {
                                        key: "{q.subject}",
                                        size: ItemSize::Sm,
                                        "data-selected": if n == i { "true" } else { "false" },
                                        onclick: move |_| {
                                            walk.write().open(&questions_now(), show_answered(), n);
                                        },
                                        ItemMedia { {state_icon(q, 20)} }
                                        ItemContent {
                                            ItemTitle { "{q.prompt}" }
                                            ItemDescription { "{listed(q, in_files.get(&q.subject))}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn reversal_label(r: Reversal) -> &'static str {
    match r {
        Reversal::Edit => "changing it later: an edit",
        Reversal::StateSurgery => "changing it later: state surgery",
        Reversal::Recreate => "changing it later: a recreate",
    }
}

fn blast_label(b: Blast) -> &'static str {
    match b {
        Blast::None => "blast: none",
        Blast::Low => "blast: low",
        Blast::High => "blast: high",
    }
}

/// What a question's state badge means, as its tooltip says it: satz's `answered`,
/// `unanswered` and `not-applicable`, and `blocking` for an unanswered question with no
/// usable default.
pub fn state_tip(q: &QuestionRow) -> &'static str {
    match q.state {
        QuestionState::Unanswered if q.blocking => {
            "No usable default: the pack cannot guess this answer, so a value has to be typed."
        }
        QuestionState::Unanswered => {
            "Not answered yet: the value the pack offers applies until the estate binds one."
        }
        QuestionState::Answered => "Answered: the estate binds a value for this question.",
        QuestionState::NotApplicable => {
            "Not asked: the question's ask_when param is false, so it does not apply to this estate."
        }
    }
}

/// What changing the answer later does to the estate, in satz's words for `reversal`.
pub fn reversal_tip(r: Reversal) -> &'static str {
    match r {
        Reversal::Edit => "Changing this answer later is an edit: one line, one apply.",
        Reversal::StateSurgery => {
            "Changing this answer later is state surgery: a rename plus state mv, and whatever the old name is wired into. Done with care."
        }
        Reversal::Recreate => {
            "Changing this answer later destroys and recreates resources: globally unique ids, mail addresses, buckets."
        }
    }
}

/// What changing the answer later costs the running organization, in satz's words for
/// `blast`. Independent of the reversal: one boolean to reverse can cut every existing
/// SSH path.
pub fn blast_tip(b: Blast) -> &'static str {
    match b {
        Blast::None => "Changing this answer later costs the running organization nothing.",
        Blast::Low => "Changing this answer later costs the running organization little.",
        Blast::High => {
            "Changing this answer later hits the running organization hard: what runs on the old answer can stop working."
        }
    }
}

/// What the one-way door chip means.
pub const ONE_WAY_TIP: &str = "Changing this answer later is a recreate or hits a high blast radius. Decide it with the reason above in view.";

/// An answer is written by the filled button and by Enter in a text or number field, and
/// by nothing else: not when the field loses focus — the button's own click blurs the
/// field first, so a blur that wrote would send a second answer — and not on a switch
/// flip or a tag added to or removed from a list, which change what the card holds and
/// turn its button into Accept. Params and Resources keep both, where saving what is
/// typed into the file being edited IS the contract.
const ON_BLUR: bool = false;
const ON_CHANGE: bool = false;
const _: () = assert!(
    !ON_BLUR && !ON_CHANGE,
    "only the filled button and Enter write an answer"
);

/// One question: the pack header and the card, which scroll, and below them the bar with
/// Back, Skip and the filled button, which does not. The card owns what it holds — the
/// field's draft or the chosen option — so the bar can read it; a card of another question,
/// shape or starting value is a new card (`card_key`). An answer only the estate file can
/// express (`in_file`) has no field: the card shows it as the file writes it, and its
/// button reads Next.
#[component]
fn QuestionCard(
    question: QuestionRow,
    field: Option<FieldKind>,
    in_file: Option<InFile>,
    previous_group: Option<String>,
    loading: bool,
    can_back: bool,
    onback: EventHandler<()>,
    onnext: EventHandler<()>,
    onaccept: EventHandler<(String, serde_json::Value)>,
) -> Element {
    let app = use_context::<Store<AppStore>>();
    let q = question;
    let new_group = previous_group != Some(q.group());
    let one_way = q.one_way_door();
    let recommend = recommends_otherwise(&q).map(str::to_string);
    let state_text = state_of(&q, in_file.as_ref());
    let needs_value = q.blocking && q.state == QuestionState::Unanswered;
    let subject = q.subject.clone();
    let kind = match (q.kind, &in_file) {
        (QuestionKind::Param, None) => field,
        (QuestionKind::Param, Some(_)) | (QuestionKind::Oneof, _) => None,
    };
    let initial = kind.map(|k| Draft::of_json(q.offered(), k));
    let mut draft = use_signal(|| initial.clone());
    let mut chosen = use_signal(|| initial_option(&q));
    let bound = q.bound_option().map(str::to_string);

    // What the card holds, read when it is asked for — a press can arrive before the
    // render that follows the keystroke before it — so the button and Enter agree.
    let held = {
        let offered = q.offered().cloned();
        let empty_answers = q.empty.is_some();
        let subject = q.subject.clone();
        let bound = bound.clone();
        let oneof = q.kind == QuestionKind::Oneof;
        move || -> Option<Held> {
            if oneof {
                Some(held_oneof(chosen().as_deref(), bound.as_deref()))
            } else {
                match (kind, draft()) {
                    (Some(k), Some(d)) => {
                        Some(held_param(k, &subject, offered.as_ref(), empty_answers, &d))
                    }
                    _ => None,
                }
            }
        }
    };
    let now = held.clone()().map(|h| primary(q.state, h.changed, h.valid, loading));
    let press = {
        let held = held.clone();
        let state = q.state;
        let subject = q.subject.clone();
        move || {
            let Some(h) = held() else { return };
            let p = primary(state, h.changed, h.valid, loading);
            match p.action {
                _ if !p.enabled => {}
                PrimaryAction::Next => onnext.call(()),
                PrimaryAction::Accept => onaccept.call((subject.clone(), h.value)),
            }
        }
    };
    let enter = press.clone();
    let field_hint = match (q.offered(), q.blocking, kind) {
        (None, true, Some(FieldKind::List(_))) => {
            "no default — at least one value is needed".to_string()
        }
        (None, true, _) => "no default — a value is needed".to_string(),
        (None, false, _) => String::new(),
        (Some(v), _, _) if q.state == QuestionState::Answered => {
            format!("the estate's own value: {}", q.shown(v))
        }
        (Some(v), _, _) => format!("the pack's default: {}", q.shown(v)),
    };
    // `q.shown` already carries the meaning beside an offered ""
    let offers_empty = q.offered().and_then(|v| v.as_str()) == Some("");
    let field_hint = match (&q.empty, offers_empty, field_hint.is_empty()) {
        (Some(meaning), false, true) => format!("empty means {meaning}"),
        (Some(meaning), false, false) => format!("{field_hint}; empty means {meaning}"),
        _ => field_hint,
    };
    let forward = text_forward(q.state, now.is_some());
    let picked = chosen().and_then(|c| q.options.iter().find(|o| o.param == c).cloned());

    rsx! {
        div { class: "interview__question",
            div { class: "interview__scroll",
                if new_group {
                    div { class: "interview__pack",
                        Boxes { size: 20 }
                        div {
                            span { class: "interview__pack-name", "{q.group()}" }
                            p { class: "interview__pack-description", "{q.pack_description}" }
                        }
                    }
                }
                Card { class: "interview__card",
                    CardContent {
                        div { class: "interview__card-head",
                            code { class: "interview__subject", "{q.label()}" }
                            span { class: "grow" }
                            Tip { text: state_tip(&q),
                                Badge { variant: if needs_value { BadgeVariant::Destructive } else { BadgeVariant::Outline },
                                    {state_icon(&q, 12)}
                                    "{state_text}"
                                }
                            }
                        }
                        h2 { class: "interview__prompt", "{q.prompt}" }
                        if let Some(why) = &q.why {
                            p { class: "interview__why", "{why}" }
                        }
                        div { class: "interview__tags",
                            Tip { text: reversal_tip(q.reversal),
                                Badge { variant: BadgeVariant::Outline,
                                    History { size: 12 }
                                    "{reversal_label(q.reversal)}"
                                }
                            }
                            Tip { text: blast_tip(q.blast),
                                Badge { variant: if q.blast == Blast::High { BadgeVariant::Destructive } else { BadgeVariant::Outline },
                                    Sparkles { size: 12 }
                                    "{blast_label(q.blast)}"
                                }
                            }
                            if one_way {
                                Tip { text: ONE_WAY_TIP,
                                    Badge { variant: BadgeVariant::Destructive,
                                        DoorOpen { size: 12 }
                                        "one-way door"
                                    }
                                }
                            }
                            span { class: "grow" }
                            span { class: "interview__from", "{q.from}" }
                        }
                        if let Some(r) = recommend {
                            div { class: "interview__recommend",
                                Lightbulb { size: 20 }
                                span { "The pack recommends: " code { "{r}" } ". The offered value still applies unless you change it." }
                            }
                        }
                        div { class: "interview__answer",
                            match (q.kind, kind, initial, &in_file) {
                                (_, _, _, Some(f)) => rsx! {
                                    p { class: "interview__in-file",
                                        "Answered in the estate file: {f.words}."
                                    }
                                    code { class: "interview__in-file-source", "{subject} = {f.source}" }
                                    div { class: "interview__in-file-edit",
                                        p { class: "interview__hint",
                                            "A field cannot hold this value without losing what it reads, so the card offers none; its Satz source is edited under Estate → Params."
                                        }
                                        Button {
                                            variant: ButtonVariant::Ghost,
                                            size: ButtonSize::Xs,
                                            onclick: move |_| app.nav().set(View::Estate),
                                            Code { size: 16 }
                                            "Edit in Params"
                                        }
                                    }
                                },
                                (QuestionKind::Param, Some(kind), Some(initial), None) => rsx! {
                                        TypedField {
                                            kind,
                                            draft: initial,
                                            label: q.label().to_string(),
                                            subject: q.subject.clone(),
                                            disabled: loading,
                                            supporting: field_hint,
                                            // Enter always reaches the card, which decides what it does
                                            commit_unchanged: true,
                                            commit_on_blur: ON_BLUR,
                                            commit_on_change: ON_CHANGE,
                                            onchange: move |d: Draft| draft.set(Some(d)),
                                            oncommit: move |_| enter(),
                                        }
                                },
                                (QuestionKind::Param, _, _, None) => rsx! {
                                    p { class: "interview__hint",
                                        "There is no field for this answer: its param is declared as a map, or with no shape satz names. Write the value into the estate's params block."
                                    }
                                },
                                (QuestionKind::Oneof, _, _, None) => rsx! {
                                    div { class: "interview__options",
                                        for (param, label) in choices(&q) {
                                            {
                                                let is_chosen = chosen().as_deref() == Some(param.as_str());
                                                rsx! {
                                                    Button {
                                                        key: "{param}",
                                                        variant: if is_chosen { ButtonVariant::Secondary } else { ButtonVariant::Outline },
                                                        size: ButtonSize::Xs,
                                                        "aria-pressed": if is_chosen { "true" } else { "false" },
                                                        onclick: move |_| chosen.set(Some(param.clone())),
                                                        "{label}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if let Some(o) = &picked {
                                        p { class: "interview__option-why",
                                            code { "{o.param}" }
                                            if let Some(why) = &o.why {
                                                " — {why}"
                                            }
                                        }
                                    } else if chosen().as_deref() == Some(NO_BRANCH) {
                                        p { class: "interview__option-why",
                                            code { "{NO_BRANCH}" }
                                            " — every option is bound false"
                                        }
                                    }
                                },
                            }
                        }
                        if q.kind == QuestionKind::Param && in_file.is_none() {
                            p { class: "interview__hint",
                                "Accept writes " code { "{subject} = …" } " into the estate's params through satz's own writer; a pack line the answer switches on is uncommented by satz."
                            }
                        }
                    }
                }
            }
            div { class: "interview__actions",
                Button { variant: ButtonVariant::Ghost, disabled: !can_back, onclick: move |_| onback.call(()), ArrowLeft { size: 16 } "Back" }
                span { class: "grow" }
                if let Some(label) = forward {
                    Button { variant: ButtonVariant::Ghost, onclick: move |_| onnext.call(()), "{label}" }
                }
                if let Some(p) = now {
                    Button {
                        disabled: !p.enabled,
                        onclick: move |_| press(),
                        {p.icon(16)}
                        "{p.label}"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::typed_field::{Commit, commits};
    use serde_json::json;

    fn q(subject: &str, state: QuestionState) -> QuestionRow {
        serde_json::from_value(json!({
            "subject": subject, "kind": "param", "prompt": "p", "reversal": "edit", "blast": "low",
            "state": match state { QuestionState::Answered => "answered", QuestionState::Unanswered => "unanswered", QuestionState::NotApplicable => "not-applicable" },
            "blocking": false, "pack_description": "d", "from": "f", "pack": "p"
        }))
        .unwrap()
    }

    fn names(v: &[QuestionRow]) -> Vec<&str> {
        v.iter().map(|q| q.subject.as_str()).collect()
    }

    fn subject_at(walk: &Walk, list: &[QuestionRow]) -> String {
        list[walk.position(list)].subject.clone()
    }

    fn button(p: Primary) -> (&'static str, PrimaryAction, bool) {
        (p.label, p.action, p.enabled)
    }

    const NEXT: (&str, PrimaryAction, bool) = ("Next", PrimaryAction::Next, true);

    fn accept(enabled: bool) -> (&'static str, PrimaryAction, bool) {
        ("Accept", PrimaryAction::Accept, enabled)
    }

    /// Nothing the field does on its own writes: not losing focus, and not a switch flip
    /// or a tag change either (`ON_CHANGE`), where Params and Resources save.
    #[test]
    fn only_enter_reaches_the_card_from_its_field() {
        assert!(!commits(Commit::Blur, true, true, ON_BLUR));
        assert!(commits(Commit::Pressed, false, true, ON_BLUR));
        // a param field, where blur-to-save is the contract, still saves on blur
        assert!(commits(Commit::Blur, true, false, true));
    }

    #[test]
    fn an_unanswered_question_reads_accept_changed_or_not() {
        let u = QuestionState::Unanswered;
        // the offered default, untouched: Accept writes it
        assert_eq!(button(primary(u, false, true, false)), accept(true));
        // a value typed over it
        assert_eq!(button(primary(u, true, true, false)), accept(true));
        // an empty field on a question that offers nothing
        assert_eq!(button(primary(u, false, false, false)), accept(false));
        // a value with a problem
        assert_eq!(button(primary(u, true, false, false)), accept(false));
        // a write in flight
        assert_eq!(button(primary(u, false, true, true)), accept(false));
    }

    #[test]
    fn an_answered_question_reads_next_until_its_value_differs() {
        let a = QuestionState::Answered;
        assert_eq!(button(primary(a, false, true, false)), NEXT);
        // Next writes nothing, so a write in flight does not disable it
        assert_eq!(button(primary(a, false, true, true)), NEXT);
        assert_eq!(button(primary(a, true, true, false)), accept(true));
        assert_eq!(button(primary(a, true, false, false)), accept(false));
        assert_eq!(button(primary(a, true, true, true)), accept(false));
        // a question not asked is not one to skip either
        assert_eq!(
            button(primary(QuestionState::NotApplicable, false, true, false)),
            NEXT
        );
    }

    /// The field put back to the written value is unchanged again, so the button returns
    /// to Next.
    #[test]
    fn an_answered_field_changed_and_changed_back_reads_next_again() {
        let kind = FieldKind::Text;
        let written = json!("europe-west3");
        let held = |d: Draft| held_param(kind, "region", Some(&written), false, &d);
        let open = held(Draft::of_json(Some(&written), kind));
        assert!(!open.changed && open.valid);
        let typed = held(Draft::Text("europe-west4".into()));
        assert!(typed.changed);
        assert_eq!(typed.value, json!("europe-west4"));
        let a = QuestionState::Answered;
        assert_eq!(
            button(primary(a, typed.changed, typed.valid, false)),
            accept(true)
        );
        let back = held(Draft::Text("europe-west3".into()));
        assert_eq!(button(primary(a, back.changed, back.valid, false)), NEXT);
    }

    #[test]
    fn an_empty_field_on_a_question_that_offers_nothing_cannot_be_accepted() {
        let kind = FieldKind::List(crate::widgets::typed_field::ListElem::Text);
        let h = held_param(kind, "emails", None, false, &Draft::of_json(None, kind));
        assert!(!h.valid);
        assert_eq!(
            button(primary(
                QuestionState::Unanswered,
                h.changed,
                h.valid,
                false
            )),
            accept(false)
        );
        let one = held_param(
            kind,
            "emails",
            None,
            false,
            &Draft::List(vec!["a@example.com".into()]),
        );
        assert!(one.valid);
        assert_eq!(one.value, json!(["a@example.com"]));
    }

    /// An answer the estate writes as a named read is shown as the file writes it, with no
    /// field — so no brace refusal and nothing to overwrite it — and the card moves on with
    /// Next. The report's own value is the decoded `${…}`, which the card does not show.
    #[test]
    fn an_answered_named_read_is_shown_as_the_file_writes_it_and_has_no_field() {
        use satz_studio_core::model::StrPart;
        let mut rag = q("rag_project_id", QuestionState::Answered);
        rag.current = Some(json!("${interface.rag-store.project_id}"));
        let value = SourceValue::Str {
            raw: "${{interface.rag-store.project_id}}".into(),
            parts: vec![StrPart::TfRef("interface.rag-store.project_id".into())],
        };
        let source = "\"${{interface.rag-store.project_id}}\"".to_string();
        let f = in_file(&rag, &value, source.clone()).expect("a named read is the file's");
        assert_eq!(
            f.words,
            "a named read: `project_id` from the interface folder `rag-store`"
        );
        assert_eq!(
            listed(&rag, Some(&f)),
            "rag_project_id · answered: \"${{interface.rag-store.project_id}}\""
        );
        // the field it would have had cannot start on it: the value is not a literal
        assert_eq!(Draft::of_source(&value, FieldKind::Text), None);
        // no field, so no filled button: the text button reads Next and writes nothing
        assert_eq!(text_forward(rag.state, false), Some("Next"));
        // a literal answer keeps its field, and so does a question not answered
        let plain = SourceValue::Str {
            raw: "acme-rag".into(),
            parts: vec![StrPart::Lit("acme-rag".into())],
        };
        assert_eq!(in_file(&rag, &plain, "\"acme-rag\"".into()), None);
        let open = q("rag_project_id", QuestionState::Unanswered);
        assert_eq!(in_file(&open, &value, source), None);
        // typing a brace into an unanswered question's field is still refused
        assert!(
            Draft::Text("${interface.rag-store.project_id}".into())
                .problem(FieldKind::Text, "rag_project_id")
                .is_some()
        );
    }

    /// A question whose `empty` says what `""` means takes an empty field as its answer,
    /// and the card says the meaning beside the value.
    #[test]
    fn an_empty_field_answers_a_question_that_says_what_empty_means() {
        let kind = FieldKind::Text;
        let empty = Draft::of_json(None, kind);
        assert!(!held_param(kind, "team_folder_name", None, false, &empty).valid);
        let h = held_param(kind, "team_folder_name", None, true, &empty);
        assert!(h.valid);
        assert_eq!(h.value, json!(""));
        let mut folder = q("team_folder_name", QuestionState::Answered);
        folder.empty = Some("no team folder".into());
        folder.current = Some(json!(""));
        assert_eq!(
            listed(&folder, None),
            "team_folder_name · answered: \"\" (no team folder)"
        );
    }

    /// A choice that is not required has a None button, starts on it when satz offers it,
    /// and reads Next on it once the estate binds every option false.
    #[test]
    fn a_choice_that_is_not_required_offers_none() {
        let mut c: QuestionRow = serde_json::from_value(json!({
            "subject": "interface_notice", "kind": "oneof", "prompt": "p", "reversal": "edit",
            "blast": "low", "state": "unanswered", "blocking": false, "pack_description": "d",
            "default": "none",
            "options": [{"param": "interface_notice_pubsub", "label": "Pub/Sub", "selected": false}],
            "from": "f", "pack": "estate_map"
        }))
        .unwrap();
        let params: Vec<String> = choices(&c).into_iter().map(|(p, _)| p).collect();
        assert_eq!(params, ["interface_notice_pubsub", NO_BRANCH]);
        assert_eq!(initial_option(&c).as_deref(), Some(NO_BRANCH));
        let h = held_oneof(Some(NO_BRANCH), c.bound_option());
        assert!(h.valid && h.changed);
        assert_eq!(h.value, json!("none"));
        c.state = QuestionState::Answered;
        c.default = None;
        assert_eq!(initial_option(&c).as_deref(), Some(NO_BRANCH));
        let h = held_oneof(Some(NO_BRANCH), c.bound_option());
        assert_eq!(button(primary(c.state, h.changed, h.valid, false)), NEXT);
        assert_eq!(listed(&c, None), "interface_notice · answered: None");
        c.required = true;
        c.state = QuestionState::Unanswered;
        assert_eq!(choices(&c).len(), 1, "a required choice has no None button");
    }

    #[test]
    fn a_oneof_reads_next_on_its_bound_option_and_accept_on_another() {
        let s1 = Some("security_model_s1");
        let s2 = Some("security_model_s2");
        let on = |state, chosen, bound| {
            let h = held_oneof(chosen, bound);
            button(primary(state, h.changed, h.valid, false))
        };
        let a = QuestionState::Answered;
        let u = QuestionState::Unanswered;
        assert_eq!(on(a, s1, s1), NEXT);
        assert_eq!(on(a, s2, s1), accept(true));
        // the default option on an unanswered question: Accept writes it
        assert_eq!(on(u, s1, None), accept(true));
        assert_eq!(on(u, None, None), accept(false));
        assert_eq!(held_oneof(s2, s1).value, json!("security_model_s2"));
    }

    #[test]
    fn skip_shows_only_on_a_question_not_answered() {
        assert_eq!(text_forward(QuestionState::Unanswered, true), Some("Skip"));
        assert_eq!(text_forward(QuestionState::Unanswered, false), Some("Skip"));
        // an answered question's filled button is its Next
        assert_eq!(text_forward(QuestionState::Answered, true), None);
        assert_eq!(text_forward(QuestionState::NotApplicable, true), None);
        // a param with no field has no filled button: the text button reads Next
        assert_eq!(text_forward(QuestionState::Answered, false), Some("Next"));
        assert_eq!(
            text_forward(QuestionState::NotApplicable, false),
            Some("Next")
        );
    }

    #[test]
    fn unanswered_come_first_and_the_rest_only_when_asked_for() {
        let all = vec![
            q("a", QuestionState::Answered),
            q("b", QuestionState::Unanswered),
            q("c", QuestionState::NotApplicable),
            q("d", QuestionState::Unanswered),
        ];
        assert_eq!(names(&ordered(&all, false)), ["b", "d"]);
        assert_eq!(names(&ordered(&all, true)), ["b", "d", "a", "c"]);
    }

    /// Accept moves the card before the write lands; the reload takes the answered
    /// question out of the walk and the card is on the question after it, not one further
    /// and not back at the top.
    #[test]
    fn accept_advances_and_the_reload_keeps_the_card_on_the_next_question() {
        let mut questions = vec![
            q("region", QuestionState::Unanswered),
            q("billing", QuestionState::Unanswered),
            q("domain", QuestionState::Unanswered),
        ];
        let mut walk = Walk::default();
        walk.accept(&questions, false);
        // while the write is in flight, the report is the one before it
        assert_eq!(subject_at(&walk, &ordered(&questions, false)), "billing");
        // the reload: region is answered and leaves the walk
        questions[0].state = QuestionState::Answered;
        let list = ordered(&questions, false);
        assert_eq!(names(&list), ["billing", "domain"]);
        assert_eq!(subject_at(&walk, &list), "billing");
        // the next Accept goes on from there
        walk.accept(&questions, false);
        questions[1].state = QuestionState::Answered;
        assert_eq!(subject_at(&walk, &ordered(&questions, false)), "domain");
        // and Back retraces: billing, then region, answered questions shown for both
        assert_eq!(walk.back(&questions, false), Some(true));
        assert_eq!(subject_at(&walk, &ordered(&questions, true)), "billing");
        assert_eq!(walk.back(&questions, true), Some(true));
        assert_eq!(subject_at(&walk, &ordered(&questions, true)), "region");
        assert_eq!(walk.back(&questions, true), None);
    }

    /// With answered questions shown, the answered one moves to the answered block and
    /// the card is still on the question that followed it.
    #[test]
    fn accept_with_answered_shown_lands_on_the_question_that_followed() {
        let mut questions = vec![
            q("region", QuestionState::Unanswered),
            q("billing", QuestionState::Unanswered),
            q("domain", QuestionState::Answered),
        ];
        let mut walk = Walk::default();
        walk.accept(&questions, true);
        questions[0].state = QuestionState::Answered;
        let list = ordered(&questions, true);
        assert_eq!(names(&list), ["billing", "region", "domain"]);
        assert_eq!(subject_at(&walk, &list), "billing");
    }

    /// A write that also takes the question the card moved to out of the walk (its
    /// ask_when turns false) leaves the card on the one that took its place.
    #[test]
    fn accept_whose_write_also_drops_the_next_question_shows_the_one_after() {
        let mut questions = vec![
            q("region", QuestionState::Unanswered),
            q("billing", QuestionState::Unanswered),
            q("domain", QuestionState::Unanswered),
        ];
        let mut walk = Walk::default();
        walk.accept(&questions, false);
        questions[0].state = QuestionState::Answered;
        questions[1].state = QuestionState::NotApplicable;
        assert_eq!(subject_at(&walk, &ordered(&questions, false)), "domain");
    }

    /// The last open question: Accept lets it go, the walk is empty, and Back returns to
    /// it with answered questions shown.
    #[test]
    fn accept_on_the_last_open_question_empties_the_walk() {
        let mut questions = vec![q("region", QuestionState::Unanswered)];
        let mut walk = Walk::default();
        walk.accept(&questions, false);
        questions[0].state = QuestionState::Answered;
        assert!(ordered(&questions, false).is_empty());
        assert!(walk.can_go_back());
        assert_eq!(walk.back(&questions, false), Some(true));
        assert_eq!(subject_at(&walk, &ordered(&questions, true)), "region");
    }

    /// Next on an answered question moves on; the question stays answered and in the
    /// list, and Back returns to it.
    #[test]
    fn next_on_an_answered_question_advances() {
        let questions = vec![
            q("region", QuestionState::Unanswered),
            q("billing", QuestionState::Answered),
            q("domain", QuestionState::Answered),
        ];
        let list = ordered(&questions, true);
        let mut walk = Walk::default();
        walk.open(&questions, true, 1);
        walk.next(&questions, true);
        assert_eq!(subject_at(&walk, &list), "domain");
        assert_eq!(walk.back(&questions, true), Some(true));
        assert_eq!(subject_at(&walk, &list), "billing");
    }

    /// Skip moves on and writes nothing: the question stays open and in the walk, and
    /// the walk wraps; Back retraces.
    #[test]
    fn skip_wraps_and_back_retraces_the_questions_left() {
        let questions = vec![
            q("region", QuestionState::Unanswered),
            q("billing", QuestionState::Unanswered),
        ];
        let list = ordered(&questions, false);
        let mut walk = Walk::default();
        walk.next(&questions, false);
        assert_eq!(subject_at(&walk, &list), "billing");
        walk.next(&questions, false);
        assert_eq!(subject_at(&walk, &list), "region");
        assert_eq!(walk.back(&questions, false), Some(false));
        assert_eq!(subject_at(&walk, &list), "billing");
        assert_eq!(walk.back(&questions, false), Some(false));
        assert_eq!(subject_at(&walk, &list), "region");
        assert_eq!(walk.back(&questions, false), None);
    }

    /// Skipping the only open question keeps it on the card and remembers nothing: there
    /// is nowhere to go.
    #[test]
    fn skipping_the_only_open_question_stays_on_it() {
        let questions = vec![q("region", QuestionState::Unanswered)];
        let mut walk = Walk::default();
        walk.next(&questions, false);
        assert_eq!(subject_at(&walk, &ordered(&questions, false)), "region");
        assert!(!walk.can_go_back());
    }

    #[test]
    fn a_question_opened_from_the_list_is_left_for_back() {
        let questions = vec![
            q("region", QuestionState::Unanswered),
            q("billing", QuestionState::Answered),
            q("domain", QuestionState::Answered),
        ];
        let list = ordered(&questions, true);
        let mut walk = Walk::default();
        walk.open(&questions, true, 2);
        assert_eq!(subject_at(&walk, &list), "domain");
        // opening the question already on the card leaves nothing behind
        walk.open(&questions, true, 2);
        assert_eq!(walk.back(&questions, true), Some(true));
        assert_eq!(subject_at(&walk, &list), "region");
        assert!(!walk.can_go_back());
    }

    /// A click on the list's n-th item calls `open(n)`; the card then shows that question
    /// and the list marks it, since both read the same position.
    #[test]
    fn a_click_on_any_listed_question_puts_it_on_the_card() {
        let questions = vec![
            q("region", QuestionState::Unanswered),
            q("billing", QuestionState::Answered),
            q("domain", QuestionState::Unanswered),
            q("labels", QuestionState::Answered),
        ];
        for show in [false, true] {
            let list = ordered(&questions, show);
            let mut walk = Walk::default();
            for n in (0..list.len()).rev().chain(0..list.len()) {
                walk.open(&questions, show, n);
                assert_eq!(walk.position(&list), n);
                assert_eq!(subject_at(&walk, &list), list[n].subject);
            }
        }
    }

    /// Every badge on the card explains itself: each state, reversal and blast has its own
    /// tooltip.
    #[test]
    fn every_badge_says_what_it_means() {
        let mut blocking = q("domain", QuestionState::Unanswered);
        blocking.blocking = true;
        let states: Vec<&str> = [
            q("a", QuestionState::Unanswered),
            blocking,
            q("b", QuestionState::Answered),
            q("c", QuestionState::NotApplicable),
        ]
        .iter()
        .map(state_tip)
        .collect();
        let reversals: Vec<&str> = [Reversal::Edit, Reversal::StateSurgery, Reversal::Recreate]
            .into_iter()
            .map(reversal_tip)
            .collect();
        let blasts: Vec<&str> = [Blast::None, Blast::Low, Blast::High]
            .into_iter()
            .map(blast_tip)
            .collect();
        for tips in [&states, &reversals, &blasts] {
            assert!(tips.iter().all(|t| !t.is_empty()));
            let distinct: std::collections::HashSet<_> = tips.iter().collect();
            assert_eq!(distinct.len(), tips.len(), "{tips:?}");
        }
        assert!(!ONE_WAY_TIP.is_empty());
    }

    #[test]
    fn switching_show_answered_keeps_the_card_on_its_question_when_it_can() {
        let questions = vec![
            q("region", QuestionState::Answered),
            q("billing", QuestionState::Unanswered),
            q("domain", QuestionState::Unanswered),
        ];
        let mut walk = Walk::default();
        walk.next(&questions, false);
        walk.switched(&questions, false, true);
        assert_eq!(subject_at(&walk, &ordered(&questions, true)), "domain");
        walk.switched(&questions, true, false);
        assert_eq!(subject_at(&walk, &ordered(&questions, false)), "domain");
        // on an answered question, hiding answered ones starts from the first open one
        walk.open(&questions, true, 2);
        walk.switched(&questions, true, false);
        assert_eq!(subject_at(&walk, &ordered(&questions, false)), "billing");
    }

    #[test]
    fn the_list_says_each_question_s_subject_and_its_answer() {
        let mut region = q("region", QuestionState::Answered);
        region.current = Some(json!("europe-west3"));
        assert_eq!(listed(&region, None), "region · answered: europe-west3");
        let mut billing = q("billing", QuestionState::Unanswered);
        billing.blocking = true;
        assert_eq!(listed(&billing, None), "billing · needs a value");
        assert_eq!(
            listed(&q("domain", QuestionState::NotApplicable), None),
            "domain · not asked: its ask_when is false"
        );
    }

    #[test]
    fn the_first_option_is_the_bound_one_then_the_default_then_none() {
        let mut o: QuestionRow = serde_json::from_value(json!({
            "subject": "security_model", "kind": "oneof", "prompt": "p", "reversal": "state_surgery",
            "blast": "low", "state": "unanswered", "blocking": false, "pack_description": "d",
            "default": "security_model_s1",
            "options": [
                {"param": "security_model_s1", "label": "S1", "selected": false},
                {"param": "security_model_s2", "label": "S2", "selected": false}
            ],
            "from": "f", "pack": "estate_map"
        }))
        .unwrap();
        assert_eq!(initial_option(&o).as_deref(), Some("security_model_s1"));
        o.options[1].selected = true;
        assert_eq!(initial_option(&o).as_deref(), Some("security_model_s2"));
        o.options[1].selected = false;
        o.default = None;
        assert_eq!(initial_option(&o), None);
    }

    /// A reload that changes the value a card starts on starts a new card; one that
    /// changes nothing on it keeps the card and what is typed into it.
    #[test]
    fn a_card_is_new_when_the_value_it_starts_on_changes() {
        let mut region = q("region", QuestionState::Unanswered);
        region.default = Some(json!("europe-west3"));
        let before = card_key(&region, Some(FieldKind::Text), None);
        assert_eq!(
            before,
            card_key(&region.clone(), Some(FieldKind::Text), None)
        );
        region.default = Some(json!("europe-west4"));
        assert_ne!(before, card_key(&region, Some(FieldKind::Text), None));
    }

    #[test]
    fn a_recommendation_shows_only_when_it_differs_from_the_offer() {
        let mut r = q("paid", QuestionState::Unanswered);
        r.default = Some(json!(false));
        r.recommend = Some("true".to_string());
        assert_eq!(recommends_otherwise(&r), Some("true"));
        r.recommend = Some("false".to_string());
        assert_eq!(recommends_otherwise(&r), None);
        r.default = Some(json!("europe-west3"));
        r.recommend = Some("europe-west3".to_string());
        assert_eq!(recommends_otherwise(&r), None);
        r.default = None;
        assert_eq!(recommends_otherwise(&r), Some("europe-west3"));
    }
}
