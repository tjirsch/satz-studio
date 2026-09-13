//! The Interview view: the questions the estate's packs declare, one at a time, the
//! unanswered ones first. Every answer is one `satz_interview` call through the estate
//! coroutine, and the view re-renders from the reloaded report.

use dioxus::prelude::*;
use satz_studio_core::satz::reports::{
    Blast, OptionRow, QuestionKind, QuestionRow, QuestionState, Reversal,
};

use crate::components::{
    Button, ButtonVariant, Card, CardVariant, Chip, ChipKind, Draft, FieldKind, Icon,
    LinearProgress, Switch, TypedField,
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

#[component]
pub fn InterviewView() -> Element {
    let app = use_context::<Store<AppStore>>();
    let handle = use_coroutine_handle::<EstateAction>();
    let report = app.estate().questions().cloned();
    let interview = app.estate().interview().cloned();
    let loading = app.estate().loading().cloned();
    let mut show_answered = use_signal(|| false);
    let mut index = use_signal(|| 0usize);

    let Some(report) = report else {
        return rsx! {
            div { class: "view interview",
                h1 { class: "view__title", "Interview" }
                Card { variant: CardVariant::Filled, class: "interview__empty",
                    Icon { name: "quiz", size: 48, class: "placeholder__icon" }
                    p { "The questions report is not available — the drawer says why." }
                }
            }
        };
    };
    let s = report.summary.clone();
    let list = ordered(&report.questions, show_answered());
    let offered_defaults = report
        .questions
        .iter()
        .filter(|q| q.state == QuestionState::Unanswered && q.default.is_some())
        .count();
    let i = index().min(list.len().saturating_sub(1));
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

    rsx! {
        div { class: "view interview",
            div { class: "interview__head",
                h1 { class: "view__title", "Interview" }
                span { class: "grow" }
                Switch { label: "Show answered", checked: show_answered(), onchange: move |v| { show_answered.set(v); index.set(0); } }
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
            match current {
                Some(q) => rsx! {
                    QuestionCard {
                        key: "{q.subject}",
                        question: q,
                        previous_pack,
                        position: (i + 1, list.len()),
                        loading,
                        onskip: move |_| {
                            let len = list.len();
                            if len > 0 {
                                index.set((i + 1) % len);
                            }
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
                    }
                },
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

#[component]
fn QuestionCard(
    question: QuestionRow,
    previous_pack: Option<String>,
    position: (usize, usize),
    loading: bool,
    onskip: EventHandler<()>,
) -> Element {
    let q = question;
    let new_pack = previous_pack.as_deref() != Some(q.pack.as_str());
    let one_way = q.one_way_door();
    let recommend = recommends_otherwise(&q).map(str::to_string);
    let (state_icon, state_text) = match q.state {
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
    };
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
                        ParamAnswer { question: q.clone(), loading, onskip: move |_| onskip.call(()) }
                    },
                    QuestionKind::Oneof => rsx! {
                        OneofAnswer { question: q.clone(), loading, onskip: move |_| onskip.call(()) }
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
fn ParamAnswer(question: QuestionRow, loading: bool, onskip: EventHandler<()>) -> Element {
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
                Button { variant: ButtonVariant::Text, onclick: move |_| onskip.call(()), "Skip" }
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
fn OneofAnswer(question: QuestionRow, loading: bool, onskip: EventHandler<()>) -> Element {
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
                Button { variant: ButtonVariant::Text, onclick: move |_| onskip.call(()), "Skip" }
                Button {
                    variant: ButtonVariant::Filled,
                    icon: "send",
                    disabled: loading || picked.is_none(),
                    onclick: move |_| {
                        if let Some(c) = chosen() {
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
