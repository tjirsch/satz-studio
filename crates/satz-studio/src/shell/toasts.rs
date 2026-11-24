use std::time::Duration;

use dioxus::prelude::*;
use dioxus_primitives::toast::{ToastOptions, use_toast};

use crate::state::{AppStore, AppStoreStoreExt, ToastKind};

/// What the stores report becomes a toast of the component library. A notice is its own
/// text and leaves by itself after five seconds. Errors and warnings are counted, not
/// quoted: the ones that arrive together are one toast that says how many, "1 error" or
/// "2 errors · 3 warnings", and nothing else — the drawer holds their text
/// ([`crate::state::toast`] keeps it there) — leaving after eight seconds (ADR 0036).
/// Either can be closed before.
#[component]
pub fn ToastHost() -> Element {
    let app = use_context::<Store<AppStore>>();
    let toasts = use_toast();
    use_effect(move || {
        if app.toasts().read().is_empty() {
            return;
        }
        let (mut errors, mut warnings) = (0, 0);
        for t in std::mem::take(&mut *app.toasts().write()) {
            match t.kind {
                ToastKind::Info => {
                    toasts.info(t.text, ToastOptions::new().duration(Duration::from_secs(5)))
                }
                ToastKind::Error => errors += 1,
                ToastKind::Warning => warnings += 1,
            }
        }
        if errors + warnings > 0 {
            let options = ToastOptions::new().duration(Duration::from_secs(8));
            let title = count_title(errors, warnings);
            if errors > 0 {
                toasts.error(title, options);
            } else {
                toasts.warning(title, options);
            }
        }
    });
    rsx! {}
}

/// The title of the toast for the errors and warnings that arrived together: each
/// count that is not zero, errors first.
pub fn count_title(errors: usize, warnings: usize) -> String {
    let count = |n: usize, one: &str, many: &str| match n {
        0 => None,
        1 => Some(format!("1 {one}")),
        n => Some(format!("{n} {many}")),
    };
    [
        count(errors, "error", "errors"),
        count(warnings, "warning", "warnings"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_toast_counts_errors_and_warnings() {
        assert_eq!(count_title(1, 0), "1 error");
        assert_eq!(count_title(3, 0), "3 errors");
        assert_eq!(count_title(2, 3), "2 errors · 3 warnings");
        assert_eq!(count_title(0, 1), "1 warning");
    }
}
