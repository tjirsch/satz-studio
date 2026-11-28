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
        SourceValue::None => rsx! {
            Tooltip {
                TooltipTrigger {
                    Badge { variant: BadgeVariant::Outline, "none" }
                }
                TooltipContent { "declared without a value: one must be provided" }
            }
        },
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
            Reference { key: "{i}", text: reference_in_words(target),
                Link { size: 12 }
                // as the file writes it: a single brace in Satz text interpolates a param
                "${{{{{target}}}}}"
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

/// What a `${…}` inside a value reads, in words: a named read of an interface folder's
/// export (`${{interface.<folder>.<export>}}`), the short form every used interface
/// folder agrees on (`${{interface.<export>}}`), or a Terraform reference.
pub fn reference_in_words(target: &str) -> String {
    match target.strip_prefix("interface.") {
        Some(read) => match read.split_once('.') {
            Some((folder, export)) => {
                format!("a named read: `{export}` from the interface folder `{folder}`")
            }
            None => {
                format!("a named read: `{read}`, which every used interface folder agrees on")
            }
        },
        None => format!("a Terraform reference: `${{{{{target}}}}}`"),
    }
}

/// A value only the estate file can write, in words — a named read, a reference to a
/// param, a string that interpolates, an object, a list holding one of those; `None` for
/// a literal, which a typed field shows as itself.
pub fn value_in_words(value: &SourceValue) -> Option<String> {
    match value {
        SourceValue::Str { parts, .. } => match parts.as_slice() {
            [StrPart::TfRef(target)] => Some(reference_in_words(target)),
            parts if parts.iter().all(|p| matches!(p, StrPart::Lit(_))) => None,
            parts => {
                let read: Vec<String> = parts
                    .iter()
                    .filter_map(|p| match p {
                        StrPart::Lit(_) => None,
                        StrPart::Param { name, .. } => Some(format!("`{{{name}}}`")),
                        StrPart::TfRef(target) => Some(format!("`${{{{{target}}}}}`")),
                    })
                    .collect();
                Some(format!("a string that interpolates {}", read.join(", ")))
            }
        },
        SourceValue::Ref { param, .. } => Some(format!("the value of the param `{param}`")),
        SourceValue::List(items) => items
            .iter()
            .any(|i| value_in_words(i).is_some())
            .then(|| "a list that holds a reference or an interpolation".to_string()),
        SourceValue::Obj => Some("an object".to_string()),
        SourceValue::Num(_) | SourceValue::Bool(_) | SourceValue::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(target: &str) -> SourceValue {
        SourceValue::Str {
            raw: format!("${{{{{target}}}}}"),
            parts: vec![StrPart::TfRef(target.to_string())],
        }
    }

    #[test]
    fn a_named_read_is_said_by_its_folder_and_its_export() {
        assert_eq!(
            value_in_words(&read("interface.rag-store.project_id")).as_deref(),
            Some("a named read: `project_id` from the interface folder `rag-store`")
        );
        assert_eq!(
            value_in_words(&read("interface.default_region")).as_deref(),
            Some("a named read: `default_region`, which every used interface folder agrees on")
        );
        assert_eq!(
            value_in_words(&read("data.google_active_folder.infra.name")).as_deref(),
            Some("a Terraform reference: `${{data.google_active_folder.infra.name}}`")
        );
    }

    #[test]
    fn a_reference_and_an_interpolation_are_said_and_a_literal_is_not() {
        assert_eq!(
            value_in_words(&SourceValue::Ref {
                param: "infra_project_name".into(),
                resolved: None
            })
            .as_deref(),
            Some("the value of the param `infra_project_name`")
        );
        let interpolated = SourceValue::Str {
            raw: "{customer_shortname}-x".into(),
            parts: vec![
                StrPart::Param {
                    name: "customer_shortname".into(),
                    resolved: None,
                },
                StrPart::Lit("-x".into()),
            ],
        };
        assert_eq!(
            value_in_words(&interpolated).as_deref(),
            Some("a string that interpolates `{customer_shortname}`")
        );
        let plain = SourceValue::Str {
            raw: "acme".into(),
            parts: vec![StrPart::Lit("acme".into())],
        };
        assert_eq!(value_in_words(&plain), None);
        assert_eq!(value_in_words(&SourceValue::Num("3".into())), None);
        assert_eq!(value_in_words(&SourceValue::List(vec![plain])), None);
    }
}
