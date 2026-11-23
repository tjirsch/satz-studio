use std::time::Duration;

use dioxus::prelude::*;
use dioxus_primitives::toast::{ToastOptions, use_toast};

use crate::state::{AppStore, AppStoreStoreExt, ToastKind};

/// What the stores report becomes a toast of the component library. A notice is its own
/// text and leaves by itself after five seconds. Errors are counted, not quoted: the ones
/// that arrive together are one toast that says how many and that the Findings drawer
/// holds their full text ([`crate::state::toast`] keeps it there), leaving after eight
/// seconds (ADR 0036). Either can be closed before.
#[component]
pub fn ToastHost() -> Element {
    let app = use_context::<Store<AppStore>>();
    let toasts = use_toast();
    use_effect(move || {
        if app.toasts().read().is_empty() {
            return;
        }
        let mut errors = 0;
        for t in std::mem::take(&mut *app.toasts().write()) {
            match t.kind {
                ToastKind::Info => {
                    toasts.info(t.text, ToastOptions::new().duration(Duration::from_secs(5)))
                }
                ToastKind::Error => errors += 1,
            }
        }
        if errors > 0 {
            toasts.error(
                error_title(errors),
                ToastOptions::new()
                    .description("The Findings drawer holds the full text.")
                    .duration(Duration::from_secs(8)),
            );
        }
    });
    rsx! {}
}

/// The title of the toast for `n` errors that arrived together.
pub fn error_title(n: usize) -> String {
    match n {
        1 => "1 error".to_string(),
        n => format!("{n} errors"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_error_toast_counts() {
        assert_eq!(error_title(1), "1 error");
        assert_eq!(error_title(3), "3 errors");
    }
}
