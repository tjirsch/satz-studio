use dioxus::prelude::*;

use super::{Button, ButtonVariant, Dialog, IconButton};

/// A standard icon button with the `help` glyph, at the top-right of a card or a view,
/// that opens a basic dialog holding what the two lines above the content leave out.
/// `title` heads the dialog and names the button ("About <title>").
#[component]
pub fn HelpButton(title: String, children: Element) -> Element {
    let mut open = use_signal(|| false);
    rsx! {
        IconButton {
            icon: "help",
            label: format!("About {title}"),
            onclick: move |_| open.set(true),
        }
        Dialog {
            open: open(),
            title: title.clone(),
            icon: "help",
            class: "help-dialog",
            ondismiss: move |_| open.set(false),
            actions: rsx! {
                Button { variant: ButtonVariant::Text, onclick: move |_| open.set(false), "Close" }
            },
            {children}
        }
    }
}
