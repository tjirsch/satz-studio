use dioxus::prelude::*;
use dioxus_icons::lucide::CircleQuestionMark;

use crate::components::button::{Button, ButtonSize, ButtonVariant};
use crate::components::dialog::{Dialog, DialogTitle};

/// The library's icon `Button` with a question mark, at the top-right of a card or a view,
/// that opens the library's `Dialog` holding what the two lines above the content leave
/// out. `title` heads the dialog and names the button ("About <title>").
#[component]
pub fn HelpButton(title: String, children: Element) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        Button {
            variant: ButtonVariant::Ghost,
            size: ButtonSize::Icon,
            title: "About {title}",
            "aria-label": "About {title}",
            onclick: move |_| open.set(true),
            CircleQuestionMark { size: 16 }
        }
        Dialog {
            open: open(),
            on_open_change: move |now: bool| open.set(now),
            class: "help-dialog",
            DialogTitle { "{title}" }
            div { class: "dialog-body", {children} }
            div { class: "dialog-actions",
                Button { variant: ButtonVariant::Ghost, onclick: move |_| open.set(false), "Close" }
            }
        }
    }
}
