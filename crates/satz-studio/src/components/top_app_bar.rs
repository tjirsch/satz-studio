use dioxus::prelude::*;

/// A small top app bar: a leading slot, title and subtitle, trailing actions as children.
#[component]
pub fn TopAppBar(
    title: String,
    #[props(default)] subtitle: String,
    #[props(default)] leading: Option<Element>,
    /// what acts on the thing the title names, set immediately beside it rather than at
    /// the bar's far end — the way an account's actions sit next to the account
    #[props(default)]
    beside: Option<Element>,
    children: Element,
) -> Element {
    rsx! {
        header { class: "m-top-app-bar",
            if let Some(leading) = leading {
                div { class: "m-top-app-bar__leading", {leading} }
            }
            div { class: "m-top-app-bar__title",
                span { class: "m-top-app-bar__headline", "{title}" }
                if !subtitle.is_empty() {
                    span { class: "m-top-app-bar__subtitle", "{subtitle}" }
                }
            }
            if let Some(beside) = beside {
                div { class: "m-top-app-bar__beside", {beside} }
            }
            div { class: "m-top-app-bar__actions", {children} }
        }
    }
}
