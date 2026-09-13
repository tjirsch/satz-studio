use dioxus::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CardVariant {
    Elevated,
    #[default]
    Filled,
    Outlined,
}

/// A card, medium shape; with `onclick` it gets the state layer and a pointer.
#[component]
pub fn Card(
    #[props(default)] variant: CardVariant,
    #[props(default)] class: String,
    #[props(default)] onclick: Option<EventHandler<MouseEvent>>,
    children: Element,
) -> Element {
    let variant_class = match variant {
        CardVariant::Elevated => "m-card--elevated",
        CardVariant::Filled => "m-card--filled",
        CardVariant::Outlined => "m-card--outlined",
    };
    rsx! {
        div {
            class: "m-card {variant_class} {class}",
            class: if onclick.is_some() { "m-card--clickable" },
            onclick: move |e| {
                if let Some(h) = &onclick {
                    h.call(e);
                }
            },
            {children}
        }
    }
}
