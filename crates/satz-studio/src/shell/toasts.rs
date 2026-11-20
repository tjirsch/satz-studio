use std::time::Duration;

use dioxus::prelude::*;
use dioxus_primitives::toast::{ToastOptions, use_toast};

use crate::state::{AppStore, AppStoreStoreExt, ToastKind};

/// What the stores report becomes a toast of the component library: a notice leaves by
/// itself after five seconds, an error after twelve, and either can be closed before.
#[component]
pub fn ToastHost() -> Element {
    let app = use_context::<Store<AppStore>>();
    let toasts = use_toast();
    use_effect(move || {
        if app.toasts().read().is_empty() {
            return;
        }
        for t in std::mem::take(&mut *app.toasts().write()) {
            match t.kind {
                ToastKind::Info => {
                    toasts.info(t.text, ToastOptions::new().duration(Duration::from_secs(5)))
                }
                ToastKind::Error => toasts.error(
                    t.text,
                    ToastOptions::new().duration(Duration::from_secs(12)),
                ),
            }
        }
    });
    rsx! {}
}
