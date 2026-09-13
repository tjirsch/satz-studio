use dioxus::prelude::*;

use crate::components::Snackbar;
use crate::state::{AppStore, AppStoreStoreExt, Toast, ToastKind, dismiss};

/// The toasts of the queue, newest at the bottom.
#[component]
pub fn SnackbarHost() -> Element {
    let app = use_context::<Store<AppStore>>();
    let toasts: Vec<Toast> = app.snackbar().read().iter().cloned().collect();
    rsx! {
        div { class: "snackbar-host",
            for t in toasts {
                Snackbar {
                    key: "{t.id}",
                    text: t.text.clone(),
                    error: t.kind == ToastKind::Error,
                    ondismiss: move |_| dismiss(&mut app.snackbar().write(), t.id),
                }
            }
        }
    }
}
