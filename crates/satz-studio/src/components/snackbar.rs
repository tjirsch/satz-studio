use dioxus::prelude::*;

use super::Icon;

/// One snackbar: inverse surface, an optional action, a dismiss button.
#[component]
pub fn Snackbar(
    text: String,
    ondismiss: EventHandler<()>,
    #[props(default)] error: bool,
    #[props(default)] action_label: String,
    #[props(default)] onaction: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        div { class: "m-snackbar", class: if error { "m-snackbar--error" }, role: "status",
            span { class: "m-snackbar__text", "{text}" }
            if let Some(action) = onaction {
                button { r#type: "button", class: "m-snackbar__action", onclick: move |_| action.call(()), "{action_label}" }
            }
            button { r#type: "button", class: "m-snackbar__dismiss", "aria-label": "dismiss", onclick: move |_| ondismiss.call(()),
                Icon { name: "close", size: 20 }
            }
        }
    }
}
