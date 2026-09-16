use dioxus::prelude::*;

/// How a connector is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectorLine {
    #[default]
    Solid,
    Dashed,
}

/// Boxes joined by right-angle connectors: a box, and under it a vertical trunk with a
/// horizontal branch into each box that hangs from it. The connectors are borders on the
/// tree's own elements, so they need no measuring and follow every resize and zoom.
/// `root` is the top box; the children are `ConnectorBranch`es.
#[component]
pub fn ConnectorTree(root: Element, #[props(default)] class: String, children: Element) -> Element {
    rsx! {
        div { class: "m-connector-tree {class}",
            div { class: "m-connector-tree__box", {root} }
            ul { class: "m-connector-tree__branches", {children} }
        }
    }
}

/// One branch: the connector from the trunk, an optional one-line `label` above the box
/// it leads to, the box, and the `branches` that hang from that box in turn. `line` and
/// `error` style this branch's own connector; `trunk_error` styles the trunk past it,
/// which leads on to a later sibling.
#[component]
pub fn ConnectorBranch(
    node: Element,
    #[props(default)] line: ConnectorLine,
    #[props(default)] error: bool,
    #[props(default)] trunk_error: bool,
    #[props(default)] label: Option<Element>,
    #[props(default)] branches: Option<Element>,
) -> Element {
    let labelled = label.is_some();
    rsx! {
        li {
            class: "m-connector-tree__branch",
            class: if line == ConnectorLine::Dashed { "m-connector-tree__branch--dashed" },
            class: if error { "m-connector-tree__branch--error" },
            class: if labelled { "m-connector-tree__branch--labelled" },
            span {
                class: "m-connector-tree__trunk",
                class: if trunk_error { "m-connector-tree__trunk--error" },
                "aria-hidden": "true",
            }
            span { class: "m-connector-tree__elbow", "aria-hidden": "true" }
            if let Some(label) = label {
                div { class: "m-connector-tree__label", {label} }
            }
            div { class: "m-connector-tree__box", {node} }
            if let Some(branches) = branches {
                ul { class: "m-connector-tree__branches", {branches} }
            }
        }
    }
}
