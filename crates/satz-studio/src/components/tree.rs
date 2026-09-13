use dioxus::prelude::*;

use super::Icon;

/// A tree: nested lists with a disclosure per branch.
#[component]
pub fn Tree(#[props(default)] class: String, children: Element) -> Element {
    rsx! {
        ul { class: "m-tree {class}", role: "tree", {children} }
    }
}

/// One tree row. With `children` it is a branch with a disclosure that starts `open`;
/// without, a leaf. `onclick` selects the row; `trailing` sits at the row's end.
#[component]
pub fn TreeItem(
    label: String,
    #[props(default)] icon: String,
    #[props(default)] supporting: String,
    #[props(default = true)] open: bool,
    #[props(default)] selected: bool,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
    #[props(default)] trailing: Option<Element>,
    #[props(default)] children: Option<Element>,
) -> Element {
    let mut expanded = use_signal(|| open);
    let is_branch = children.is_some();
    rsx! {
        li {
            class: "m-tree-item",
            class: if is_branch { "m-tree-item--branch" },
            role: "treeitem",
            "aria-expanded": if is_branch { if expanded() { "true" } else { "false" } },
            "aria-selected": if selected { "true" } else { "false" },
            div {
                class: "m-tree-item__row",
                class: if selected { "m-tree-item__row--selected" },
                onclick: move |e| {
                    if let Some(h) = &onclick {
                        h.call(e);
                    }
                },
                if is_branch {
                    button {
                        r#type: "button",
                        class: "m-tree-item__disclosure",
                        "aria-label": if expanded() { "collapse" } else { "expand" },
                        onclick: move |e| {
                            e.stop_propagation();
                            expanded.toggle();
                        },
                        Icon { name: if expanded() { "expand_more" } else { "chevron_right" }, size: 20 }
                    }
                } else {
                    span { class: "m-tree-item__disclosure m-tree-item__disclosure--leaf" }
                }
                if !icon.is_empty() {
                    Icon { name: icon.clone(), size: 20, class: "m-tree-item__icon" }
                }
                span { class: "m-tree-item__label", "{label}" }
                if !supporting.is_empty() {
                    span { class: "m-tree-item__supporting", "{supporting}" }
                }
                if let Some(trailing) = trailing {
                    span { class: "m-tree-item__trailing", {trailing} }
                }
            }
            if let Some(children) = children {
                if expanded() {
                    ul { class: "m-tree__group", role: "group", {children} }
                }
            }
        }
    }
}
