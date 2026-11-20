use dioxus::prelude::*;
use dioxus_icons::lucide::{Braces, Link};
use satz_studio_core::model::{SourceValue, StrPart};

use crate::components::badge::{Badge, BadgeVariant};
use crate::components::tooltip::{Tooltip, TooltipContent, TooltipTrigger};

/// A value as the estate file writes it: literal text as code, and every param or
/// Terraform reference inside it as a library `Badge` whose tooltip says what it
/// resolves to.
#[component]
pub fn SourceTags(value: SourceValue) -> Element {
    rsx! {
        span { class: "source-tags", {tags(&value)} }
    }
}

fn tags(value: &SourceValue) -> Element {
    match value {
        SourceValue::Str { parts, .. } => rsx! {
            for (i, part) in parts.iter().enumerate() {
                {part_tag(i, part)}
            }
        },
        SourceValue::Num(n) => rsx! { code { "{n}" } },
        SourceValue::Bool(b) => rsx! { code { "{b}" } },
        SourceValue::Ref { param, resolved } => rsx! {
            Reference { text: resolved_text(resolved.as_ref()),
                Braces { size: 12 }
                "{param}"
            }
        },
        SourceValue::List(items) => rsx! {
            code { "[" }
            for (i, item) in items.iter().enumerate() {
                span { key: "{i}", class: "source-tags__item",
                    {tags(item)}
                    if i + 1 < items.len() {
                        code { "," }
                    }
                }
            }
            code { "]" }
        },
        SourceValue::Obj => rsx! { code { "{{ … }}" } },
    }
}

fn part_tag(i: usize, part: &StrPart) -> Element {
    match part {
        StrPart::Lit(text) => rsx! { code { key: "{i}", "{text}" } },
        StrPart::Param { name, resolved } => rsx! {
            Reference { key: "{i}", text: resolved_text(resolved.as_ref()),
                Braces { size: 12 }
                "{{{name}}}"
            }
        },
        StrPart::TfRef(target) => rsx! {
            Reference { key: "{i}", text: "a Terraform reference, written as ${{…}}",
                Link { size: 12 }
                "${{{target}}}"
            }
        },
    }
}

/// One reference inside a value, with what it stands for in a tooltip.
#[component]
fn Reference(text: String, children: Element) -> Element {
    rsx! {
        Tooltip {
            TooltipTrigger {
                Badge { variant: BadgeVariant::Outline, {children} }
            }
            TooltipContent { "{text}" }
        }
    }
}

fn resolved_text(resolved: Option<&serde_json::Value>) -> String {
    match resolved {
        Some(serde_json::Value::String(s)) => format!("resolves to \"{s}\""),
        Some(other) => format!("resolves to {other}"),
        None => "bound nowhere: the transpile reports it".to_string(),
    }
}
