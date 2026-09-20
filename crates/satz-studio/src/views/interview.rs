//! The Decisions destination: the questions the estate's packs declare, one at a time, the
//! unanswered ones first. Every answer is one `satz_interview` call through the estate
//! coroutine, and the view re-renders from the reloaded report. The card moves when the
//! operator moves it and at no other time: an answer is written and the card stays on the
//! question it answered — now reading answered, its forward button reading Next — until
//! Next, Back or a click in the list moves it. The walk remembers the questions it moved
//! away from, so Back returns to one whether it is answered by then or not; with "Show
//! answered" on, every question is listed beside the card. A typed answer's field has the
//! shape `answer_kind` reads off the report — the shape the pack declares the param with,
//! else the offered value's — so a list that offers nothing is still a list.

use dioxus::prelude::*;
use satz_studio_core::model::answer_kind;
use satz_studio_core::satz::reports::{
    Blast, OptionRow, QuestionKind, QuestionRow, QuestionState, Reversal,
};

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, Draft, FieldKind, Icon,
    LinearProgress, List, ListItem, Switch, TypedField,
};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt};

/// The questions in the order the view walks them: the unanswered ones as the report
/// lists them, then — when asked for — the answered ones and the ones not asked. `held`
/// is the subject of the question the card holds open, the one it has just answered: it
/// keeps the place it had among the unanswered ones whatever its state now says, so
/// answering moves neither the card nor a row under the operator.
pub fn ordered(
    questions: &[QuestionRow],
    show_answered: bool,
    held: Option<&str>,
) -> Vec<QuestionRow> {
    let is_held = |q: &QuestionRow| held.is_some_and(|h| h == q.subject);
    let mut out: Vec<QuestionRow> = questions
        .iter()
        .filter(|q| q.state == QuestionState::Unanswered || is_held(q))
        .cloned()
        .collect();
    if show_answered {
        out.extend(
            questions
                .iter()
                .filter(|q| q.state == QuestionState::Answered && !is_held(q))
                .cloned(),
        );
        out.extend(
            questions
                .iter()
                .filter(|q| q.state == QuestionState::NotApplicable && !is_held(q))
                .cloned(),
        );
    }
    out
}

/// Where the walk stands. The card follows its question by `subject`, because a reload
/// reorders the list: an answer moves a question into the answered block, or out of the
/// walk while answered questions are hidden. `index` is where the card was, for when its
/// question has left the list — the question that took its place is shown. `left` holds
/// the questions the walk moved away from, the latest last, for Back. `held` is the
/// question answered on the card: the walk keeps it where it was until the operator
/// moves off it, so an answer alone never carries the card to the next question.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Walk {
    subject: Option<String>,
    index: usize,
    left: Vec<String>,
    held: Option<String>,
}

impl Walk {
    /// The questions the walk carries: `ordered`, with the question the card holds open
    /// kept in its place.
    pub fn list(&self, questions: &[QuestionRow], show_answered: bool) -> Vec<QuestionRow> {
        ordered(questions, show_answered, self.held.as_deref())
    }

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
        let list = self.list(questions, show_answered);
        let Some(q) = list.get(to) else { return };
        if to == self.position(&list) {
            return;
        }
        self.leave(&list);
        self.subject = Some(q.subject.clone());
        self.index = to;
        // the card has left the question it held open; the list stops carrying it
        self.held = None;
    }

    /// Skip, or Next: the question after the card's, the first one after the last. This
    /// and Back are what move the card — writing an answer does not.
    pub fn next(&mut self, questions: &[QuestionRow], show_answered: bool) {
        let list = self.list(questions, show_answered);
        if list.is_empty() {
            return;
        }
        let from = self.position(&list);
        let to = (from + 1) % list.len();
        if to != from {
            self.open(questions, show_answered, to);
            return;
        }
        // One question in the walk, and it is there only because the card holds it open:
        // moving on lets it go, and leaves the walk with no question at all.
        let plain = ordered(questions, show_answered, None);
        let releases = self.held.is_some()
            && list
                .get(from)
                .is_some_and(|q| !plain.iter().any(|p| p.subject == q.subject));
        if releases {
            self.leave(&list);
            self.subject = None;
            self.index = 0;
            self.held = None;
        }
    }

    /// The card's question has just been answered: the walk holds on to it, so the
    /// reload leaves the card on the question it answered and only Next moves it off.
    pub fn hold(&mut self, questions: &[QuestionRow], show_answered: bool) {
        let list = self.list(questions, show_answered);
        let i = self.position(&list);
        if let Some(q) = list.get(i) {
            self.subject = Some(q.subject.clone());
            self.index = i;
            self.held = Some(q.subject.clone());
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
            // the card leaves the question it held open, as it does going forward
            self.held = None;
            self.index = ordered(questions, show, None)
                .iter()
                .position(|r| r.subject == subject)
                .unwrap_or(0);
            self.subject = Some(subject);
            return Some(show);
        }
        None
    }

    /// "Show answered" switched from `from` to `to`: the card keeps its question when
    /// the new list holds it, and starts from the first question when it does not. The
    /// switch is not a move, so a question the card holds open stays on the card.
    pub fn switched(&mut self, questions: &[QuestionRow], from: bool, to: bool) {
        let before = self.list(questions, from);
        self.subject = before
            .get(self.position(&before))
            .map(|q| q.subject.clone());
        let after = self.list(questions, to);
        match self
            .subject
            .as_ref()
            .and_then(|s| after.iter().position(|q| &q.subject == s))
        {
            Some(i) => self.index = i,
            None => {
                self.subject = None;
                self.index = 0;
                self.held = None;
            }
        }
    }
}

/// The option the view starts on: the one the estate binds, else the one the pack
/// offers as its default, else none.
pub fn initial_option(q: &QuestionRow) -> Option<String> {
    q.options
        .iter()
        .find(|o| o.selected)
        .map(|o| o.param.clone())
        .or_else(|| {
            let default = q.default.as_ref()?.as_str()?;
            q.options
                .iter()
                .find(|o| o.param == default)
                .map(|o| o.param.clone())
        })
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

/// A question's state as the card's chip and the list say it: an icon and the words.
pub fn state_of(q: &QuestionRow) -> (&'static str, String) {
    match q.state {
        QuestionState::Unanswered if q.blocking => ("priority_high", "needs a value".to_string()),
        QuestionState::Unanswered => ("radio_button_unchecked", "open".to_string()),
        QuestionState::Answered => (
            "check_circle",
            format!(
                "answered: {}",
                q.current.as_ref().map(shown).unwrap_or_default()
            ),
        ),
        QuestionState::NotApplicable => ("block", "not asked: its ask_when is false".to_string()),
    }
}

/// The list's supporting line for a question: its subject, then its state.
pub fn listed(q: &QuestionRow) -> String {
    format!("{} · {}", q.subject, state_of(q).1)
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
                Card { variant: CardVariant::Filled, class: "interview__empty",
                    Icon { name: "quiz", size: 48, class: "placeholder__icon" }
                    p { "The questions report is not available — the drawer says why." }
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

    let s = report.summary.clone();
    let list = walk.read().list(&report.questions, show_answered());
    let offered_defaults = report
        .questions
        .iter()
        .filter(|q| q.state == QuestionState::Unanswered && q.default.is_some())
        .count();
    let i = walk.read().position(&list);
    let can_back = walk.read().can_go_back();
    let current = list.get(i).cloned();
    // the field's shape, as the report declares it; `None` when there is no field to
    // type the answer in — a map, or a param declared without a shape that offers nothing
    let field = current
        .as_ref()
        .and_then(answer_kind)
        .map(FieldKind::of_param);
    let previous_pack = i
        .checked_sub(1)
        .and_then(|p| list.get(p))
        .map(|q| q.pack.clone());
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
                Switch {
                    label: "Show answered",
                    checked: show_answered(),
                    onchange: move |v| {
                        walk.write().switched(&questions_now(), show_answered(), v);
                        show_answered.set(v);
                    },
                }
                Button {
                    variant: ButtonVariant::Tonal,
                    icon: "done_all",
                    disabled: loading || offered_defaults == 0,
                    onclick: move |_| handle.send(EstateAction::AcceptDefaults),
                    "Accept {offered_defaults} defaults"
                }
            }
            div { class: "interview__progress",
                LinearProgress { value: progress }
                span { class: "interview__progress-text",
                    if s.complete {
                        "{s.answered} of {s.total} answered · nothing open — bootstrap and apply will not refuse this estate for an open question"
                    } else {
                        "{s.answered} of {s.total} answered · {s.unanswered} open · {s.blocking} need a value · {s.one_way_doors} one-way doors"
                    }
                }
            }
            if let Some(name) = rename_to {
                Card { variant: CardVariant::Outlined, class: "interview__rename",
                    Icon { name: "drive_file_rename_outline", size: 24, class: "placeholder__icon" }
                    p {
                        "init would have named this file " code { "{name}" }
                        ": rename it (" code { "git mv" } ") and change its " code { "estate" } " line to match."
                    }
                }
            }
            div { class: "interview__walk",
                match current {
                    Some(q) => rsx! {
                        QuestionCard {
                            key: "{q.subject}",
                            question: q,
                            field,
                            previous_pack,
                            loading,
                            can_back,
                            onback: move |_| go_back(),
                            onskip: move |_| {
                                walk.write().next(&questions_now(), show_answered());
                            },
                            // an answer does not move the card: it holds the question it
                            // answered until the operator presses Next
                            onanswer: move |_| {
                                walk.write().hold(&questions_now(), show_answered());
                            },
                        }
                    },
                    None => rsx! {
                        Card { variant: CardVariant::Outlined, class: "interview__empty",
                            Icon { name: "quiz", size: 48, class: "placeholder__icon" }
                            if s.total == 0 {
                                p { "No pack this estate uses asks a question." }
                            } else {
                                p { "Nothing is open. Switch on \"Show answered\" to revisit an answer." }
                            }
                            if can_back {
                                Button { variant: ButtonVariant::Text, icon: "arrow_back", onclick: move |_| go_back(), "Back" }
                            }
                        }
                    },
                }
                // The list stands beside the walk whatever the switch says: the switch
                // decides what is IN it, not whether it is there. It keeps its own
                // height and its own scrollbar, so a long list moves nothing else.
                Card { variant: CardVariant::Outlined, class: "interview__list",
                    if list.is_empty() {
                        div { class: "interview__list-empty",
                            Icon { name: "task_alt", size: 24 }
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
                        List {
                            for (n, q) in list.iter().enumerate() {
                                ListItem {
                                    key: "{q.subject}",
                                    headline: q.prompt.clone(),
                                    supporting: listed(q),
                                    selected: n == i,
                                    leading: rsx! { Icon { name: state_of(q).0, size: 20 } },
                                    onclick: move |_| {
                                        walk.write().open(&questions_now(), show_answered(), n);
                                    },
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

/// The walk's controls beside an answer: Back, and Skip — Next on a question that is
/// already answered or not asked, where there is nothing to skip.
#[component]
fn WalkButtons(
    state: QuestionState,
    can_back: bool,
    onback: EventHandler<()>,
    onskip: EventHandler<()>,
) -> Element {
    let forward = if state == QuestionState::Unanswered {
        "Skip"
    } else {
        "Next"
    };
    rsx! {
        Button { variant: ButtonVariant::Text, icon: "arrow_back", disabled: !can_back, onclick: move |_| onback.call(()), "Back" }
        span { class: "grow" }
        Button { variant: ButtonVariant::Text, onclick: move |_| onskip.call(()), "{forward}" }
    }
}

#[component]
fn QuestionCard(
    question: QuestionRow,
    field: Option<FieldKind>,
    previous_pack: Option<String>,
    loading: bool,
    can_back: bool,
    onback: EventHandler<()>,
    onskip: EventHandler<()>,
    onanswer: EventHandler<()>,
) -> Element {
    let q = question;
    let new_pack = previous_pack.as_deref() != Some(q.pack.as_str());
    let one_way = q.one_way_door();
    let recommend = recommends_otherwise(&q).map(str::to_string);
    let (state_icon, state_text) = state_of(&q);
    let subject = q.subject.clone();

    rsx! {
        div { class: "interview__question",
            if new_pack {
                div { class: "interview__pack",
                    Icon { name: "inventory_2", size: 20 }
                    div {
                        span { class: "interview__pack-name", "{q.pack}" }
                        p { class: "interview__pack-description", "{q.pack_description}" }
                    }
                }
            }
            Card { variant: CardVariant::Outlined, class: "interview__card",
                div { class: "interview__card-head",
                    code { class: "interview__subject", "{q.subject}" }
                    span { class: "grow" }
                    Chip { kind: ChipKind::Assist, icon: state_icon, label: state_text, error: q.blocking && q.state == QuestionState::Unanswered }
                }
                h2 { class: "interview__prompt", "{q.prompt}" }
                if let Some(why) = &q.why {
                    p { class: "interview__why", "{why}" }
                }
                div { class: "interview__chips",
                    Chip { kind: ChipKind::Assist, icon: "history", label: reversal_label(q.reversal) }
                    Chip { kind: ChipKind::Assist, icon: "flare", label: blast_label(q.blast), error: q.blast == Blast::High }
                    if one_way {
                        Chip { kind: ChipKind::Assist, icon: "door_front", label: "one-way door", error: true }
                    }
                    span { class: "grow" }
                    span { class: "interview__from", "{q.from}" }
                }
                if one_way {
                    div { class: "interview__banner", role: "alert",
                        Icon { name: "warning", filled: true }
                        span { "A one-way door: changing this answer later is a recreate or hits a high blast radius. Decide it with the reason above in view." }
                    }
                }
                if let Some(r) = recommend {
                    div { class: "interview__recommend",
                        Icon { name: "lightbulb", size: 20 }
                        span { "The pack recommends: " code { "{r}" } ". The offered value still applies unless you change it." }
                    }
                }
                match (q.kind, field) {
                    (QuestionKind::Param, Some(kind)) => rsx! {
                        ParamAnswer {
                            // a question of another shape starts the field afresh
                            key: "{kind:?}",
                            question: q.clone(),
                            kind,
                            loading,
                            can_back,
                            onback: move |_| onback.call(()),
                            onskip: move |_| onskip.call(()),
                            onanswer: move |_| onanswer.call(()),
                        }
                    },
                    (QuestionKind::Param, None) => rsx! {
                        div { class: "interview__answer",
                            p { class: "interview__hint",
                                "There is no field for this answer: its param is declared as a map, or with no shape satz names. Write the value into the estate's params block."
                            }
                            div { class: "interview__actions",
                                WalkButtons { state: q.state, can_back, onback: move |_| onback.call(()), onskip: move |_| onskip.call(()) }
                            }
                        }
                    },
                    (QuestionKind::Oneof, _) => rsx! {
                        OneofAnswer {
                            question: q.clone(),
                            loading,
                            can_back,
                            onback: move |_| onback.call(()),
                            onskip: move |_| onskip.call(()),
                            onanswer: move |_| onanswer.call(()),
                        }
                    },
                }
                if q.kind == QuestionKind::Param {
                    p { class: "interview__hint",
                        "Answering here writes " code { "{subject} = …" } " into the estate's params through satz's own writer; a pack line the answer switches on is uncommented by satz."
                    }
                }
            }
        }
    }
}

#[component]
fn ParamAnswer(
    question: QuestionRow,
    kind: FieldKind,
    loading: bool,
    can_back: bool,
    onback: EventHandler<()>,
    onskip: EventHandler<()>,
    onanswer: EventHandler<()>,
) -> Element {
    let handle = use_coroutine_handle::<EstateAction>();
    let q = question;
    let offers = q.offered().is_some();
    let initial = Draft::of_json(q.offered(), kind);
    let mut draft = use_signal(|| initial.clone());
    let problem = draft().problem(kind, &q.subject);
    let unchanged = draft() == initial;
    let accept = unchanged && offers;
    // nothing typed is no answer to a question that offers nothing
    let can_send = problem.is_none() && !loading && (offers || !draft().is_empty());
    let subject = q.subject.clone();
    let send = move || {
        let d = draft();
        if d.problem(kind, &subject).is_none() && (offers || !d.is_empty()) {
            onanswer.call(());
            handle.send(EstateAction::Answer {
                subject: subject.clone(),
                value: d.to_json(),
            });
        }
    };
    let field_subject = q.subject.clone();
    let field_hint = match (q.offered(), q.blocking) {
        (None, true) if matches!(kind, FieldKind::List(_)) => {
            "no default — at least one value is needed".to_string()
        }
        (None, true) => "no default — a value is needed".to_string(),
        (None, false) => String::new(),
        (Some(v), _) if q.state == QuestionState::Answered => {
            format!("the estate's own value: {}", shown(v))
        }
        (Some(v), _) => format!("the pack's default: {}", shown(v)),
    };
    rsx! {
        div { class: "interview__answer",
            TypedField {
                kind,
                draft: initial.clone(),
                label: q.subject.clone(),
                subject: field_subject,
                disabled: loading,
                supporting: field_hint,
                commit_unchanged: offers,
                onchange: move |d: Draft| draft.set(d),
                oncommit: {
                    let send = send.clone();
                    move |_| send()
                },
            }
            div { class: "interview__actions",
                WalkButtons { state: q.state, can_back, onback: move |_| onback.call(()), onskip: move |_| onskip.call(()) }
                Button {
                    variant: ButtonVariant::Filled,
                    icon: if accept { "check" } else { "send" },
                    disabled: !can_send,
                    onclick: move |_| send(),
                    if accept { "Accept" } else { "Answer" }
                }
            }
        }
    }
}

#[component]
fn OneofAnswer(
    question: QuestionRow,
    loading: bool,
    can_back: bool,
    onback: EventHandler<()>,
    onskip: EventHandler<()>,
    onanswer: EventHandler<()>,
) -> Element {
    let handle = use_coroutine_handle::<EstateAction>();
    let q = question;
    let mut chosen = use_signal(|| initial_option(&q));
    let picked: Option<OptionRow> =
        chosen().and_then(|c| q.options.iter().find(|o| o.param == c).cloned());
    let bound = q
        .options
        .iter()
        .find(|o| o.selected)
        .map(|o| o.param.clone());
    let subject = q.subject.clone();
    rsx! {
        div { class: "interview__answer",
            div { class: "interview__options",
                for o in q.options.iter().cloned() {
                    {
                        let param = o.param.clone();
                        let is_chosen = chosen().as_deref() == Some(o.param.as_str());
                        rsx! {
                            Chip {
                                key: "{o.param}",
                                kind: ChipKind::Filter,
                                label: o.label.clone(),
                                selected: is_chosen,
                                onclick: move |_| chosen.set(Some(param.clone())),
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
            }
            div { class: "interview__actions",
                WalkButtons { state: q.state, can_back, onback: move |_| onback.call(()), onskip: move |_| onskip.call(()) }
                Button {
                    variant: ButtonVariant::Filled,
                    icon: "send",
                    disabled: loading || picked.is_none(),
                    onclick: move |_| {
                        if let Some(c) = chosen() {
                            onanswer.call(());
                            handle.send(EstateAction::Answer { subject: subject.clone(), value: serde_json::Value::String(c) });
                        }
                    },
                    if chosen().is_some() && chosen() == bound { "Keep" } else { "Choose" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn q(subject: &str, state: QuestionState) -> QuestionRow {
        serde_json::from_value(json!({
            "subject": subject, "kind": "param", "prompt": "p", "reversal": "edit", "blast": "low",
            "state": match state { QuestionState::Answered => "answered", QuestionState::Unanswered => "unanswered", QuestionState::NotApplicable => "not-applicable" },
            "blocking": false, "pack_description": "d", "from": "f", "pack": "p"
        }))
        .unwrap()
    }

    fn subject_at(walk: &Walk, list: &[QuestionRow]) -> String {
        list[walk.position(list)].subject.clone()
    }

    #[test]
    fn unanswered_come_first_and_the_rest_only_when_asked_for() {
        let all = vec![
            q("a", QuestionState::Answered),
            q("b", QuestionState::Unanswered),
            q("c", QuestionState::NotApplicable),
            q("d", QuestionState::Unanswered),
        ];
        let names = |v: Vec<QuestionRow>| v.into_iter().map(|q| q.subject).collect::<Vec<_>>();
        assert_eq!(names(ordered(&all, false, None)), ["b", "d"]);
        assert_eq!(names(ordered(&all, true, None)), ["b", "d", "a", "c"]);
        // the question the card holds open keeps its place among the unanswered ones,
        // and is not listed a second time in the answered block
        assert_eq!(names(ordered(&all, false, Some("a"))), ["a", "b", "d"]);
        assert_eq!(names(ordered(&all, true, Some("a"))), ["a", "b", "d", "c"]);
    }

    /// The rule Thomas asked for: an answer writes a value and nothing else. The card
    /// stays on the question it answered — which now reads answered, so its forward
    /// button reads Next — and the list under it does not move either.
    #[test]
    fn an_answer_leaves_the_card_on_the_question_it_answered() {
        let mut questions = vec![
            q("region", QuestionState::Unanswered),
            q("billing", QuestionState::Unanswered),
            q("domain", QuestionState::Unanswered),
        ];
        let mut walk = Walk::default();
        walk.hold(&questions, false);
        // the reload: region is answered and would leave the walk, but the card holds it
        questions[0].state = QuestionState::Answered;
        let list = walk.list(&questions, false);
        assert_eq!(
            list.iter().map(|q| &q.subject).collect::<Vec<_>>(),
            ["region", "billing", "domain"]
        );
        assert_eq!(walk.position(&list), 0);
        assert_eq!(list[0].state, QuestionState::Answered);
        // nothing was left behind, so there is nothing to go back to yet
        assert!(!walk.can_go_back());
    }

    /// Next is what moves the card, and only then does the answered question leave the
    /// walk; Back returns to it, switching "Show answered" on to hold it.
    #[test]
    fn next_moves_the_card_off_the_answered_question_and_back_returns_to_it() {
        let mut questions = vec![
            q("region", QuestionState::Unanswered),
            q("billing", QuestionState::Unanswered),
            q("domain", QuestionState::Unanswered),
        ];
        let mut walk = Walk::default();
        walk.hold(&questions, false);
        questions[0].state = QuestionState::Answered;
        walk.next(&questions, false);
        let list = walk.list(&questions, false);
        assert_eq!(
            list.iter().map(|q| &q.subject).collect::<Vec<_>>(),
            ["billing", "domain"]
        );
        assert_eq!(subject_at(&walk, &list), "billing");
        assert!(walk.can_go_back());
        assert_eq!(walk.back(&questions, false), Some(true));
        assert_eq!(
            subject_at(&walk, &walk.list(&questions, true)),
            "region",
            "Back returns to the answered question, with answered questions shown"
        );
        assert_eq!(walk.back(&questions, true), None);
    }

    /// The last open question: answering it leaves the card on it, and Next then empties
    /// the walk rather than showing that question again.
    #[test]
    fn next_off_the_last_answered_question_empties_the_walk() {
        let mut questions = vec![q("region", QuestionState::Unanswered)];
        let mut walk = Walk::default();
        walk.hold(&questions, false);
        questions[0].state = QuestionState::Answered;
        assert_eq!(subject_at(&walk, &walk.list(&questions, false)), "region");
        walk.next(&questions, false);
        assert!(walk.list(&questions, false).is_empty());
        assert_eq!(walk.back(&questions, false), Some(true));
        assert_eq!(subject_at(&walk, &walk.list(&questions, true)), "region");
    }

    #[test]
    fn skip_wraps_and_back_retraces_the_questions_left() {
        let questions = vec![
            q("region", QuestionState::Unanswered),
            q("billing", QuestionState::Unanswered),
        ];
        let list = ordered(&questions, false, None);
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
    /// is nowhere to go, and the card was not holding it in the walk.
    #[test]
    fn skipping_the_only_open_question_stays_on_it() {
        let questions = vec![q("region", QuestionState::Unanswered)];
        let mut walk = Walk::default();
        walk.next(&questions, false);
        assert_eq!(subject_at(&walk, &walk.list(&questions, false)), "region");
        assert!(!walk.can_go_back());
    }

    /// With answered questions shown, an answer leaves the card and its row where they
    /// were rather than carrying the question into the answered block under the operator.
    #[test]
    fn an_answer_with_answered_shown_moves_neither_the_card_nor_its_row() {
        let mut questions = vec![
            q("region", QuestionState::Unanswered),
            q("billing", QuestionState::Unanswered),
            q("domain", QuestionState::Answered),
        ];
        let mut walk = Walk::default();
        walk.hold(&questions, true);
        questions[0].state = QuestionState::Answered;
        let list = walk.list(&questions, true);
        assert_eq!(
            list.iter().map(|q| &q.subject).collect::<Vec<_>>(),
            ["region", "billing", "domain"]
        );
        assert_eq!(subject_at(&walk, &list), "region");
        assert_eq!(walk.position(&list), 0);
    }

    #[test]
    fn a_question_opened_from_the_list_is_left_for_back() {
        let questions = vec![
            q("region", QuestionState::Unanswered),
            q("billing", QuestionState::Answered),
            q("domain", QuestionState::Answered),
        ];
        let list = ordered(&questions, true, None);
        let mut walk = Walk::default();
        walk.open(&questions, true, 2);
        assert_eq!(subject_at(&walk, &list), "domain");
        // opening the question already on the card leaves nothing behind
        walk.open(&questions, true, 2);
        assert_eq!(walk.back(&questions, true), Some(true));
        assert_eq!(subject_at(&walk, &list), "region");
        assert!(!walk.can_go_back());
    }

    /// The row the card holds open is the row a click on it opens: clicking it is not a
    /// move, so the question stays in the list.
    #[test]
    fn clicking_the_held_question_s_own_row_keeps_it_in_the_list() {
        let mut questions = vec![
            q("region", QuestionState::Unanswered),
            q("billing", QuestionState::Unanswered),
        ];
        let mut walk = Walk::default();
        walk.hold(&questions, false);
        questions[0].state = QuestionState::Answered;
        walk.open(&questions, false, 0);
        let list = walk.list(&questions, false);
        assert_eq!(
            list.iter().map(|q| &q.subject).collect::<Vec<_>>(),
            ["region", "billing"]
        );
        assert_eq!(subject_at(&walk, &list), "region");
        assert!(!walk.can_go_back());
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
        assert_eq!(
            subject_at(&walk, &ordered(&questions, true, None)),
            "domain"
        );
        walk.switched(&questions, true, false);
        assert_eq!(
            subject_at(&walk, &ordered(&questions, false, None)),
            "domain"
        );
        // on an answered question, hiding answered ones starts from the first open one
        walk.open(&questions, true, 2);
        walk.switched(&questions, true, false);
        assert_eq!(
            subject_at(&walk, &ordered(&questions, false, None)),
            "billing"
        );
    }

    /// The switch is not a press of the forward button: a question answered on the card
    /// stays on it through "Show answered" going on and off again.
    #[test]
    fn the_show_answered_switch_does_not_take_the_just_answered_card_away() {
        let mut questions = vec![
            q("region", QuestionState::Unanswered),
            q("billing", QuestionState::Unanswered),
        ];
        let mut walk = Walk::default();
        walk.hold(&questions, false);
        questions[0].state = QuestionState::Answered;
        walk.switched(&questions, false, true);
        assert_eq!(subject_at(&walk, &walk.list(&questions, true)), "region");
        walk.switched(&questions, true, false);
        assert_eq!(subject_at(&walk, &walk.list(&questions, false)), "region");
    }

    #[test]
    fn the_list_says_each_question_s_subject_and_its_answer() {
        let mut region = q("region", QuestionState::Answered);
        region.current = Some(json!("europe-west3"));
        assert_eq!(listed(&region), "region · answered: europe-west3");
        let mut billing = q("billing", QuestionState::Unanswered);
        billing.blocking = true;
        assert_eq!(listed(&billing), "billing · needs a value");
        assert_eq!(
            listed(&q("domain", QuestionState::NotApplicable)),
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
