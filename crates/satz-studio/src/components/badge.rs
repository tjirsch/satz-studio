use dioxus::prelude::*;

/// A badge on the top-right of its content: a 6px dot with `dot`, else the count
/// (nothing at zero, `99+` above 99).
#[component]
pub fn Badge(
    #[props(default)] count: usize,
    #[props(default)] dot: bool,
    #[props(default)] class: String,
    children: Element,
) -> Element {
    let text = if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    };
    rsx! {
        span { class: "m-badge-anchor {class}",
            {children}
            if dot {
                span { class: "m-badge m-badge--dot" }
            } else if count > 0 {
                span { class: "m-badge", "{text}" }
            }
        }
    }
}
