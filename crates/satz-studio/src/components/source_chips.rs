use dioxus::prelude::*;
use satz_studio_core::model::{SourceValue, StrPart};

use super::{Chip, ChipKind, Tooltip};

/// A value as satz reads it, as chips: a `{param}` interpolation with what it resolves
/// to, a `${…}` Terraform reference, a bare param reference, the literal text between
/// them; a list as its items in turn, an object as one word.
#[component]
pub fn SourceChips(value: SourceValue) -> Element {
    rsx! {
        span { class: "source-chips",
            {chips(&value)}
        }
    }
}

fn chips(value: &SourceValue) -> Element {
    match value {
        SourceValue::Str { parts, .. } => rsx! {
            for (i, part) in parts.iter().enumerate() {
                {part_chip(i, part)}
            }
        },
        SourceValue::Num(n) => rsx! { code { class: "source-chips__lit", "{n}" } },
        SourceValue::Bool(b) => rsx! { code { class: "source-chips__lit", "{b}" } },
        SourceValue::Ref { param, resolved } => rsx! {
            Tooltip { text: resolved_text(resolved.as_ref()),
                Chip { kind: ChipKind::Assist, icon: "data_object", label: param.clone(), class: "source-chips__param" }
            }
        },
        SourceValue::List(items) => rsx! {
            code { class: "source-chips__lit", "[" }
            for (i, item) in items.iter().enumerate() {
                span { key: "{i}", class: "source-chips__item",
                    {chips(item)}
                    if i + 1 < items.len() {
                        code { class: "source-chips__lit", "," }
                    }
                }
            }
            code { class: "source-chips__lit", "]" }
        },
        SourceValue::Obj => rsx! { code { class: "source-chips__lit", "{{ … }}" } },
    }
}

fn part_chip(i: usize, part: &StrPart) -> Element {
    match part {
        StrPart::Lit(text) => rsx! { code { key: "{i}", class: "source-chips__lit", "{text}" } },
        StrPart::Param { name, resolved } => rsx! {
            Tooltip { key: "{i}", text: resolved_text(resolved.as_ref()),
                Chip { kind: ChipKind::Assist, icon: "data_object", label: "{{{name}}}", class: "source-chips__param" }
            }
        },
        StrPart::TfRef(target) => rsx! {
            Tooltip { key: "{i}", text: "a Terraform reference, written as ${{…}}",
                Chip { kind: ChipKind::Assist, icon: "link", label: "${{{target}}}", class: "source-chips__ref" }
            }
        },
    }
}

fn resolved_text(resolved: Option<&serde_json::Value>) -> String {
    match resolved {
        Some(serde_json::Value::String(s)) => format!("resolves to \"{s}\""),
        Some(other) => format!("resolves to {other}"),
        None => "bound nowhere: the compile reports it".to_string(),
    }
}
