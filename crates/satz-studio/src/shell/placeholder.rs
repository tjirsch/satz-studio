use dioxus::prelude::*;
use dioxus_icons::lucide::HardDrive;

use crate::components::button::Button;
use crate::components::card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
use crate::state::{AppStore, AppStoreStoreExt, View};

/// A destination that works on an open estate, shown while none is open.
#[component]
pub fn NoEstate(view: View) -> Element {
    let app = use_context::<Store<AppStore>>();
    rsx! {
        div { class: "placeholder",
            Card { class: "placeholder__card",
                CardHeader {
                    {view.icon(32)}
                    CardTitle { "{view.label()}" }
                    CardDescription { "This destination works on an open estate." }
                }
                CardContent {
                    Button { onclick: move |_| app.nav().set(View::Start),
                        HardDrive { size: 16 }
                        "Open an estate"
                    }
                }
            }
        }
    }
}
