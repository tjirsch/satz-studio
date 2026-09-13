//! One typed field for a value satz reads in a known shape — a switch, a number field,
//! a chip list or a text field — and the draft it edits. The shape is decided where the
//! value comes from: an interview answer takes the shape of the value it replaces
//! (satz's `parse_answer`), a param row its `ParamKind`, an attribute its `AttrType`.
//! What comes out is either a JSON value for `satz_interview` or a `TypedValue` for the
//! app's own writer; in both, a text with a brace is refused with satz's own sentence,
//! because a value is not a template.

use dioxus::prelude::*;
use satz_studio_core::cst::TypedValue;
use satz_studio_core::model::{ParamKind, SourceValue, StrPart};
use satz_studio_core::schema::AttrType;

use super::{ChipList, Switch, TextField};

/// The element shape of a list field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListElem {
    Text,
    Number,
    Bool,
}

/// The shape a typed field edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Bool,
    Number,
    List(ListElem),
    Text,
}

impl FieldKind {
    /// The shape `parse_answer` reads an answer in, from the value it replaces: a bool
    /// stays a bool, a number a number, a list a list of strings; anything else — a
    /// string, an object, or nothing offered — is a string.
    pub fn of_json(offered: Option<&serde_json::Value>) -> FieldKind {
        match offered {
            Some(serde_json::Value::Bool(_)) => FieldKind::Bool,
            Some(serde_json::Value::Number(_)) => FieldKind::Number,
            Some(serde_json::Value::Array(_)) => FieldKind::List(ListElem::Text),
            _ => FieldKind::Text,
        }
    }

    pub fn of_param(kind: ParamKind) -> FieldKind {
        match kind {
            ParamKind::Bool => FieldKind::Bool,
            ParamKind::Number => FieldKind::Number,
            ParamKind::List => FieldKind::List(ListElem::Text),
            ParamKind::String => FieldKind::Text,
        }
    }

    /// The field for an attribute type: the three scalars, and a list of one of them;
    /// everything else — sets, maps, objects, a type the schema does not decode — has
    /// no typed field and is edited as source.
    pub fn of_attr(t: &AttrType) -> Option<FieldKind> {
        Some(match t {
            AttrType::String => FieldKind::Text,
            AttrType::Number => FieldKind::Number,
            AttrType::Bool => FieldKind::Bool,
            AttrType::ListOf(inner) => FieldKind::List(match inner.as_ref() {
                AttrType::String => ListElem::Text,
                AttrType::Number => ListElem::Number,
                AttrType::Bool => ListElem::Bool,
                _ => return None,
            }),
            _ => return None,
        })
    }
}

/// What a typed field holds while it is edited.
#[derive(Debug, Clone, PartialEq)]
pub enum Draft {
    Bool(bool),
    Number(String),
    List(Vec<String>),
    Text(String),
}

impl Draft {
    /// The empty draft of a shape.
    pub fn empty(kind: FieldKind) -> Draft {
        match kind {
            FieldKind::Bool => Draft::Bool(false),
            FieldKind::Number => Draft::Number(String::new()),
            FieldKind::List(_) => Draft::List(Vec::new()),
            FieldKind::Text => Draft::Text(String::new()),
        }
    }

    /// The draft of a JSON value in a shape: a value of another shape is shown as text
    /// in that shape's field, and a missing one is the empty draft.
    pub fn of_json(value: Option<&serde_json::Value>, kind: FieldKind) -> Draft {
        use serde_json::Value;
        let Some(value) = value else {
            return Draft::empty(kind);
        };
        match (kind, value) {
            (FieldKind::Bool, Value::Bool(b)) => Draft::Bool(*b),
            (FieldKind::Bool, other) => Draft::Bool(json_truthy(other)),
            (FieldKind::Number, Value::Number(n)) => Draft::Number(n.to_string()),
            (FieldKind::Number, other) => Draft::Number(json_text(other)),
            (FieldKind::List(_), Value::Array(items)) => {
                Draft::List(items.iter().map(json_text).collect())
            }
            (FieldKind::List(_), Value::Null) => Draft::List(Vec::new()),
            (FieldKind::List(_), other) => Draft::List(vec![json_text(other)]),
            (FieldKind::Text, other) => Draft::Text(json_text(other)),
        }
    }

    /// The draft of a value as the file has it, decoded, for a field in value mode: a
    /// string is its literal text, a list its items' literal text. A reference or an
    /// interpolation cannot be shown as a value (that is what source mode is for) and
    /// is `None`.
    pub fn of_source(value: &SourceValue, kind: FieldKind) -> Option<Draft> {
        Some(match (kind, value) {
            (FieldKind::Bool, SourceValue::Bool(b)) => Draft::Bool(*b),
            (FieldKind::Number, SourceValue::Num(n)) => Draft::Number(n.clone()),
            (FieldKind::List(_), SourceValue::List(items)) => {
                Draft::List(items.iter().map(literal).collect::<Option<Vec<_>>>()?)
            }
            (FieldKind::Text, SourceValue::Str { parts, .. }) => Draft::Text(literal_of(parts)?),
            (FieldKind::Text, other) => Draft::Text(literal(other)?),
            (FieldKind::Bool, other) => Draft::Bool(literal(other)? == "true"),
            (FieldKind::Number, other) => Draft::Number(literal(other)?),
            (FieldKind::List(_), other) => Draft::List(vec![literal(other)?]),
        })
    }

    /// Why the draft cannot be written yet, if it cannot: a number that is not one, a
    /// brace in a text, a list item of the wrong shape. `subject` opens satz's own
    /// sentence for a brace.
    pub fn problem(&self, kind: FieldKind, subject: &str) -> Option<String> {
        match (self, kind) {
            (Draft::Number(n), _) => {
                (!is_satz_number(n)).then(|| format!("`{n}`: this one is a number"))
            }
            (Draft::Text(t), _) => brace_refusal(subject, t),
            (Draft::List(items), FieldKind::List(elem)) => {
                items.iter().find_map(|item| match elem {
                    ListElem::Text => brace_refusal(subject, item),
                    ListElem::Number => {
                        (!is_satz_number(item)).then(|| format!("`{item}`: this one is a number"))
                    }
                    ListElem::Bool => (item != "true" && item != "false")
                        .then(|| format!("`{item}`: true or false")),
                })
            }
            (Draft::List(_), _) | (Draft::Bool(_), _) => None,
        }
    }

    /// The JSON value `satz_interview` is sent; `problem` must be `None` first.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Draft::Bool(b) => serde_json::Value::Bool(*b),
            Draft::Number(n) => n
                .parse::<i64>()
                .map(serde_json::Value::from)
                .or_else(|_| n.parse::<f64>().map(serde_json::Value::from))
                .unwrap_or_else(|_| serde_json::Value::String(n.clone())),
            Draft::List(items) => serde_json::Value::Array(
                items
                    .iter()
                    .map(|s| serde_json::Value::String(s.clone()))
                    .collect(),
            ),
            Draft::Text(t) => serde_json::Value::String(t.clone()),
        }
    }

    /// The value the app's writer renders; `problem` must be `None` first.
    pub fn to_typed(&self, kind: FieldKind) -> TypedValue {
        match (self, kind) {
            (Draft::Bool(b), _) => TypedValue::Bool(*b),
            (Draft::Number(n), _) => TypedValue::Num(n.clone()),
            (Draft::Text(t), _) => TypedValue::Str(t.clone()),
            (Draft::List(items), FieldKind::List(elem)) => TypedValue::List(
                items
                    .iter()
                    .map(|item| match elem {
                        ListElem::Text => TypedValue::Str(item.clone()),
                        ListElem::Number => TypedValue::Num(item.clone()),
                        ListElem::Bool => TypedValue::Bool(item == "true"),
                    })
                    .collect(),
            ),
            (Draft::List(items), _) => {
                TypedValue::List(items.iter().map(|s| TypedValue::Str(s.clone())).collect())
            }
        }
    }
}

/// A `{` or `}` in a value, refused with the sentence satz's own writer uses
/// (`vendor/satz/src/interview.rs`, `answer`): braces interpolate in a Satz string.
pub fn brace_refusal(subject: &str, text: &str) -> Option<String> {
    (text.contains('{') || text.contains('}')).then(|| {
        format!(
            "{subject}: braces interpolate in a Satz string — if `{text}` is what you mean, write that param by hand"
        )
    })
}

/// A number as satz's lexer reads one: an optional minus, digits, at most one point
/// with digits on both sides.
pub fn is_satz_number(s: &str) -> bool {
    let digits = s.strip_prefix('-').unwrap_or(s);
    let mut parts = digits.split('.');
    let whole = parts.next().unwrap_or_default();
    let all_digits = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    all_digits(whole) && parts.all(all_digits) && digits.matches('.').count() <= 1
}

/// The literal text of a value that is a plain scalar, `None` for anything that only
/// source mode can show.
fn literal(value: &SourceValue) -> Option<String> {
    match value {
        SourceValue::Str { parts, .. } => literal_of(parts),
        SourceValue::Num(n) => Some(n.clone()),
        SourceValue::Bool(b) => Some(b.to_string()),
        SourceValue::Ref { .. } | SourceValue::List(_) | SourceValue::Obj => None,
    }
}

fn literal_of(parts: &[StrPart]) -> Option<String> {
    let mut out = String::new();
    for p in parts {
        match p {
            StrPart::Lit(s) => out.push_str(s),
            StrPart::Param { .. } | StrPart::TfRef(_) => return None,
        }
    }
    Some(out)
}

/// A JSON value as the text a field shows: a string bare, anything else as JSON.
fn json_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// `truthy` as satz reads a gate: a bool is itself, a string is true unless empty or
/// `"false"`, null is false, anything else is true.
fn json_truthy(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::String(s) => !s.is_empty() && s != "false",
        serde_json::Value::Null => false,
        _ => true,
    }
}

/// The field: a switch, a number field, a chip list or a text field by `kind`, over
/// `draft`. `onchange` gets every keystroke's draft; `oncommit` gets the draft on
/// Enter, on blur, on a switch flip and on a chip change — only when it has no
/// `problem`, and only when it differs from what the field was given unless
/// `commit_unchanged` says an unchanged draft counts (an interview accepting its
/// offer). A problem is shown under the field with satz's sentence.
#[component]
pub fn TypedField(
    kind: FieldKind,
    draft: Draft,
    label: String,
    subject: String,
    #[props(default)] disabled: bool,
    #[props(default)] supporting: String,
    #[props(default)] commit_unchanged: bool,
    #[props(default)] onchange: Option<EventHandler<Draft>>,
    #[props(default)] oncommit: Option<EventHandler<Draft>>,
) -> Element {
    let given = draft.clone();
    let mut current = use_signal(|| draft.clone());
    let problem = current().problem(kind, &subject);
    let commit_subject = subject.clone();
    let commit = use_callback(move |()| {
        let d = current();
        if d.problem(kind, &commit_subject).is_none()
            && (commit_unchanged || d != given)
            && let Some(h) = &oncommit
        {
            h.call(d);
        }
    });
    let update = use_callback(move |d: Draft| {
        current.set(d.clone());
        if let Some(h) = &onchange {
            h.call(d);
        }
    });
    let hint = problem.clone().unwrap_or(supporting);
    match (kind, current()) {
        (FieldKind::Bool, Draft::Bool(b)) => rsx! {
            Switch {
                label,
                checked: b,
                disabled,
                onchange: move |v: bool| {
                    update.call(Draft::Bool(v));
                    if let Some(h) = &oncommit {
                        h.call(Draft::Bool(v));
                    }
                },
            }
        },
        (FieldKind::Number, Draft::Number(n)) => rsx! {
            TextField {
                label,
                value: n,
                monospace: true,
                disabled,
                supporting: hint,
                error: problem.is_some(),
                oninput: move |v: String| update.call(Draft::Number(v)),
                onenter: move |_| commit.call(()),
                onblur: move |_| commit.call(()),
            }
        },
        (FieldKind::List(_), Draft::List(items)) => rsx! {
            ChipList {
                label,
                items,
                disabled,
                supporting: hint,
                error: problem.is_some(),
                onchange: move |next: Vec<String>| {
                    update.call(Draft::List(next));
                    commit.call(());
                },
            }
        },
        (FieldKind::Text, Draft::Text(t)) => rsx! {
            TextField {
                label,
                value: t,
                monospace: true,
                disabled,
                supporting: hint,
                error: problem.is_some(),
                oninput: move |v: String| update.call(Draft::Text(v)),
                onenter: move |_| commit.call(()),
                onblur: move |_| commit.call(()),
            }
        },
        (kind, draft) => rsx! {
            p { class: "typed-field__mismatch",
                "the field is {kind:?} and the draft {draft:?} — a defect in the view, not in the file"
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_offered_value_decides_the_field_as_parse_answer_does() {
        assert_eq!(FieldKind::of_json(Some(&json!(true))), FieldKind::Bool);
        assert_eq!(FieldKind::of_json(Some(&json!(30))), FieldKind::Number);
        assert_eq!(
            FieldKind::of_json(Some(&json!(["a"]))),
            FieldKind::List(ListElem::Text)
        );
        assert_eq!(FieldKind::of_json(Some(&json!("x"))), FieldKind::Text);
        assert_eq!(FieldKind::of_json(Some(&json!({"a": 1}))), FieldKind::Text);
        assert_eq!(FieldKind::of_json(None), FieldKind::Text);
    }

    #[test]
    fn an_attribute_type_gets_a_field_only_for_the_scalars_and_their_lists() {
        assert_eq!(FieldKind::of_attr(&AttrType::String), Some(FieldKind::Text));
        assert_eq!(
            FieldKind::of_attr(&AttrType::ListOf(Box::new(AttrType::Number))),
            Some(FieldKind::List(ListElem::Number))
        );
        assert_eq!(
            FieldKind::of_attr(&AttrType::SetOf(Box::new(AttrType::String))),
            None
        );
        assert_eq!(
            FieldKind::of_attr(&AttrType::MapOf(Box::new(AttrType::String))),
            None
        );
        assert_eq!(FieldKind::of_attr(&AttrType::Object(Vec::new())), None);
        assert_eq!(FieldKind::of_attr(&AttrType::Unknown), None);
        assert_eq!(
            FieldKind::of_attr(&AttrType::ListOf(Box::new(AttrType::Object(Vec::new())))),
            None
        );
    }

    #[test]
    fn a_brace_in_a_value_is_refused_with_satz_s_sentence() {
        let p = brace_refusal("infra_project_name", "{customer_shortname}-x").unwrap();
        assert_eq!(
            p,
            "infra_project_name: braces interpolate in a Satz string — if `{customer_shortname}-x` is what you mean, write that param by hand"
        );
        assert!(brace_refusal("x", "plain").is_none());
        assert!(
            Draft::Text("a}".into())
                .problem(FieldKind::Text, "x")
                .is_some()
        );
        assert!(
            Draft::List(vec!["ok".into(), "{p}".into()])
                .problem(FieldKind::List(ListElem::Text), "x")
                .is_some()
        );
    }

    #[test]
    fn numbers_are_what_the_lexer_reads() {
        for ok in ["0", "30", "-7", "1.5", "-0.25"] {
            assert!(is_satz_number(ok), "{ok}");
        }
        for bad in ["", "-", "1.", ".5", "1.2.3", "1e5", "0x1", " 1", "abc"] {
            assert!(!is_satz_number(bad), "{bad}");
        }
        assert!(
            Draft::Number("1e5".into())
                .problem(FieldKind::Number, "n")
                .is_some()
        );
        assert!(
            Draft::Number("400".into())
                .problem(FieldKind::Number, "n")
                .is_none()
        );
    }

    #[test]
    fn a_draft_becomes_the_typed_value_its_kind_writes() {
        assert_eq!(
            Draft::Bool(true).to_typed(FieldKind::Bool),
            TypedValue::Bool(true)
        );
        assert_eq!(
            Draft::Number("30".into()).to_typed(FieldKind::Number),
            TypedValue::Num("30".into())
        );
        assert_eq!(
            Draft::Text("acme".into()).to_typed(FieldKind::Text),
            TypedValue::Str("acme".into())
        );
        assert_eq!(
            Draft::List(vec!["a".into(), "b".into()]).to_typed(FieldKind::List(ListElem::Text)),
            TypedValue::List(vec![
                TypedValue::Str("a".into()),
                TypedValue::Str("b".into())
            ])
        );
        assert_eq!(
            Draft::List(vec!["1".into()]).to_typed(FieldKind::List(ListElem::Number)),
            TypedValue::List(vec![TypedValue::Num("1".into())])
        );
        assert_eq!(
            Draft::List(vec!["true".into()]).to_typed(FieldKind::List(ListElem::Bool)),
            TypedValue::List(vec![TypedValue::Bool(true)])
        );
    }

    #[test]
    fn a_draft_becomes_the_json_satz_interview_takes() {
        assert_eq!(Draft::Bool(false).to_json(), json!(false));
        assert_eq!(Draft::Number("400".into()).to_json(), json!(400));
        assert_eq!(Draft::Number("1.5".into()).to_json(), json!(1.5));
        assert_eq!(
            Draft::List(vec!["in:eu-locations".into(), "in:us-locations".into()]).to_json(),
            json!(["in:eu-locations", "in:us-locations"])
        );
        assert_eq!(Draft::Text("acme".into()).to_json(), json!("acme"));
    }

    #[test]
    fn a_source_value_in_value_mode_shows_its_literal_and_a_reference_does_not() {
        let plain = SourceValue::Str {
            raw: "a\\\"b".into(),
            parts: vec![StrPart::Lit("a\"b".into())],
        };
        assert_eq!(
            Draft::of_source(&plain, FieldKind::Text),
            Some(Draft::Text("a\"b".into()))
        );
        let interpolated = SourceValue::Str {
            raw: "{x}-1".into(),
            parts: vec![
                StrPart::Param {
                    name: "x".into(),
                    resolved: None,
                },
                StrPart::Lit("-1".into()),
            ],
        };
        assert_eq!(Draft::of_source(&interpolated, FieldKind::Text), None);
        assert_eq!(
            Draft::of_source(
                &SourceValue::Ref {
                    param: "p".into(),
                    resolved: None
                },
                FieldKind::Text
            ),
            None
        );
        let list = SourceValue::List(vec![SourceValue::Num("1".into()), SourceValue::Bool(true)]);
        assert_eq!(
            Draft::of_source(&list, FieldKind::List(ListElem::Text)),
            Some(Draft::List(vec!["1".into(), "true".into()]))
        );
    }

    #[test]
    fn a_json_value_of_another_shape_is_shown_as_text_and_nothing_is_the_empty_draft() {
        assert_eq!(Draft::of_json(None, FieldKind::Bool), Draft::Bool(false));
        assert_eq!(
            Draft::of_json(None, FieldKind::Text),
            Draft::Text(String::new())
        );
        assert_eq!(
            Draft::of_json(Some(&json!("yes")), FieldKind::Bool),
            Draft::Bool(true)
        );
        assert_eq!(
            Draft::of_json(Some(&json!(["a", 2])), FieldKind::List(ListElem::Text)),
            Draft::List(vec!["a".into(), "2".into()])
        );
        assert_eq!(
            Draft::of_json(Some(&json!(3)), FieldKind::Text),
            Draft::Text("3".into())
        );
    }
}
