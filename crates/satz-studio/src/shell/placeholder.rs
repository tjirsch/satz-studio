use dioxus::prelude::*;

use crate::components::{Button, Card, CardVariant, Icon};
use crate::state::{AppStore, AppStoreStoreExt, View};

/// A destination whose view is not part of this build.
#[component]
pub fn NotBuilt(view: View) -> Element {
    rsx! {
        div { class: "placeholder",
            Card { variant: CardVariant::Filled, class: "placeholder__card",
                Icon { name: view.icon().to_string(), size: 48, class: "placeholder__icon" }
                h2 { "{view.label()}" }
                p { "This view is not part of this build." }
            }
        }
    }
}

/// A destination that works on an open estate, shown while none is open.
#[component]
pub fn NoEstate(view: View) -> Element {
    let app = use_context::<Store<AppStore>>();
    rsx! {
        div { class: "placeholder",
            Card { variant: CardVariant::Filled, class: "placeholder__card",
                Icon { name: view.icon().to_string(), size: 48, class: "placeholder__icon" }
                h2 { "{view.label()}" }
                p { "This view works on an open estate." }
                Button { icon: "home_storage", onclick: move |_| app.nav().set(View::Estates), "Open an estate" }
            }
        }
    }
}
