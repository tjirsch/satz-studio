//! The Decisions destination: the questions the estate's packs declare, one at a time, the
//! unanswered ones first. Every answer is one `satz_interview` call through the estate
//! coroutine, and the view re-renders from the reloaded report. The walk remembers the
//! questions it moved away from, so Back returns to one whether it is answered by then
//! or not; with "Show answered" on, every question is listed beside the card.

use dioxus::prelude::*;
use satz_studio_core::satz::reports::{
    Blast, OptionRow, QuestionKind, QuestionRow, QuestionState, Reversal,
};

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, Draft, FieldKind, Icon,
    LinearProgress, List, ListItem, Switch, TypedField,
};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt};

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

    /// Open the question at `to`, remembering the one on the card.
    pub fn open(&mut self, list: &[QuestionRow], to: usize) {
        let Some(q) = list.get(to) else { return };
        if to != self.position(list) {
            self.leave(list);
        }
        self.subject = Some(q.subject.clone());
        self.index = to;
    }

    /// Skip, or Next: the question after the card's, the first one after the last.
    pub fn next(&mut self, list: &[QuestionRow]) {
        if !list.is_empty() {
            self.open(list, (self.position(list) + 1) % list.len());
        }
    }

    /// The card's question is being answered: the walk moves on to the question after
    /// it, which the reload puts where the answered one was.
    pub fn answered(&mut self, list: &[QuestionRow]) {
        if list.is_empty() {
            return;
        }
        let i = self.position(list);
        self.leave(list);
        self.subject = list.get(i + 1).map(|q| q.subject.clone());
        self.index = i;
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
        let subject = before.get(self.position(&before)).map(|q| &q.subject);
        let after = ordered(questions, to);
        match subject.and_then(|s| after.iter().position(|q| &q.subject == s)) {
            Some(i) => {
                self.subject = Some(after[i].subject.clone());
                self.index = i;
            }
            None => {
                self.subject = None;
                self.index = 0;
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
    let list = ordered(&report.questions, show_answered());
    let offered_defaults = report
        .questions
        .iter()
        .filter(|q| q.state == QuestionState::Unanswered && q.default.is_some())
        .count();
    let i = walk.read().position(&list);
    let can_back = walk.read().can_go_back();
    let current = list.get(i).cloned();
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
    let listing = show_answered() && !list.is_empty();

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
                    "{s.answered} of {s.total} answered · {s.unanswered} open · {s.blocking} need a value · {s.one_way_doors} one-way doors"
                }
            }
            if s.complete {
                Card { variant: CardVariant::Filled, class: "interview__complete",
                    Icon { name: "task_alt", size: 32, class: "interview__complete-icon" }
                    div {
                        h2 { class: "interview__complete-title", "Every applicable question is answered." }
                        p { "The gate is open: bootstrap and apply will not refuse this estate for an open question." }
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
            div { class: "interview__walk", class: if listing { "interview__walk--listed" },
                match current {
                    Some(q) => rsx! {
                        QuestionCard {
                            key: "{q.subject}",
                            question: q,
                            previous_pack,
                            position: (i + 1, list.len()),
                            loading,
                            can_back,
                            onback: move |_| go_back(),
                            onskip: move |_| {
                                walk.write().next(&ordered(&questions_now(), show_answered()));
                            },
                            onanswer: move |_| {
                                walk.write().answered(&ordered(&questions_now(), show_answered()));
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
                if listing {
                    Card { variant: CardVariant::Outlined, class: "interview__list",
                        List {
                            for (n, q) in list.iter().enumerate() {
                                ListItem {
                                    key: "{q.subject}",
                                    headline: q.prompt.clone(),
                                    supporting: listed(q),
                                    selected: n == i,
                                    leading: rsx! { Icon { name: state_of(q).0, size: 20 } },
                                    onclick: move |_| {
                                        walk.write().open(&ordered(&questions_now(), show_answered()), n);
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
    previous_pack: Option<String>,
    position: (usize, usize),
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
                    span { class: "interview__position", "{position.0} of {position.1}" }
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
                match q.kind {
                    QuestionKind::Param => rsx! {
                        ParamAnswer {
                            question: q.clone(),
                            loading,
                            can_back,
                            onback: move |_| onback.call(()),
                            onskip: move |_| onskip.call(()),
                            onanswer: move |_| onanswer.call(()),
                        }
                    },
                    QuestionKind::Oneof => rsx! {
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
    loading: bool,
    can_back: bool,
    onback: EventHandler<()>,
    onskip: EventHandler<()>,
    onanswer: EventHandler<()>,
) -> Element {
    let handle = use_coroutine_handle::<EstateAction>();
    let q = question;
    let kind = FieldKind::of_json(q.offered());
    let initial = Draft::of_json(q.offered(), kind);
    let mut draft = use_signal(|| initial.clone());
    let problem = draft().problem(kind, &q.subject);
    let empty_text = matches!(draft(), Draft::Text(ref t) if t.trim().is_empty());
    let unchanged = draft() == initial;
    let accept = unchanged && q.offered().is_some();
    let can_send = problem.is_none() && !loading && !(empty_text && q.offered().is_none());
    let subject = q.subject.clone();
    let send = move || {
        let d = draft();
        if d.problem(kind, &subject).is_none() {
            onanswer.call(());
            handle.send(EstateAction::Answer {
                subject: subject.clone(),
                value: d.to_json(),
            });
        }
    };
    let field_subject = q.subject.clone();
    let field_hint = match (q.offered(), q.blocking) {
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
                commit_unchanged: q.offered().is_some(),
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
        assert_eq!(names(ordered(&all, false)), ["b", "d"]);
        assert_eq!(names(ordered(&all, true)), ["b", "d", "a", "c"]);
    }

    /// The interview's own flow: an answer moves on, and Back returns to the question
    /// just answered, which has left the walk while answered questions are hidden.
    #[test]
    fn back_returns_to_the_question_just_answered_and_shows_answered_to_hold_it() {
        let mut questions = vec![
            q("region", QuestionState::Unanswered),
            q("billing", QuestionState::Unanswered),
            q("domain", QuestionState::Unanswered),
        ];
        let mut walk = Walk::default();
        assert!(!walk.can_go_back());
        walk.answered(&ordered(&questions, false));
        // the reload: region is answered and leaves the walk
        questions[0].state = QuestionState::Answered;
        let open = ordered(&questions, false);
        assert_eq!(subject_at(&walk, &open), "billing");
        assert!(walk.can_go_back());
        assert_eq!(walk.back(&questions, false), Some(true));
        assert_eq!(subject_at(&walk, &ordered(&questions, true)), "region");
        assert_eq!(walk.back(&questions, true), None);
    }

    #[test]
    fn skip_wraps_and_back_retraces_the_questions_left() {
        let questions = vec![
            q("region", QuestionState::Unanswered),
            q("billing", QuestionState::Unanswered),
        ];
        let list = ordered(&questions, false);
        let mut walk = Walk::default();
        walk.next(&list);
        assert_eq!(subject_at(&walk, &list), "billing");
        walk.next(&list);
        assert_eq!(subject_at(&walk, &list), "region");
        assert_eq!(walk.back(&questions, false), Some(false));
        assert_eq!(subject_at(&walk, &list), "billing");
        assert_eq!(walk.back(&questions, false), Some(false));
        assert_eq!(subject_at(&walk, &list), "region");
        assert_eq!(walk.back(&questions, false), None);
    }

    /// With answered questions shown, an answer moves the question into the answered
    /// block; the card goes on to the next question rather than following it there.
    #[test]
    fn an_answer_with_answered_shown_moves_on_rather_than_after_the_question() {
        let mut questions = vec![
            q("region", QuestionState::Unanswered),
            q("billing", QuestionState::Unanswered),
            q("domain", QuestionState::Answered),
        ];
        let mut walk = Walk::default();
        walk.answered(&ordered(&questions, true));
        questions[0].state = QuestionState::Answered;
        let list = ordered(&questions, true);
        assert_eq!(subject_at(&walk, &list), "billing");
        assert_eq!(walk.position(&list), 0);
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
        walk.open(&list, 2);
        assert_eq!(subject_at(&walk, &list), "domain");
        // opening the question already on the card leaves nothing behind
        walk.open(&list, 2);
        assert_eq!(walk.back(&questions, true), Some(true));
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
        walk.next(&ordered(&questions, false));
        walk.switched(&questions, false, true);
        assert_eq!(subject_at(&walk, &ordered(&questions, true)), "domain");
        walk.switched(&questions, true, false);
        assert_eq!(subject_at(&walk, &ordered(&questions, false)), "domain");
        // on an answered question, hiding answered ones starts from the first open one
        walk.open(&ordered(&questions, true), 2);
        walk.switched(&questions, true, false);
        assert_eq!(subject_at(&walk, &ordered(&questions, false)), "billing");
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
