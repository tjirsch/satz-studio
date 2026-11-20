use dioxus::prelude::*;

use crate::components::{Badge, CircularProgress, IconButton, TopAppBar};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, EstateStoreStoreExt, View};
use crate::views::overview::owed_now;

/// The top bar: the open estate's name and directory, what acts on that estate beside
/// them — reload, "Switch estate", "Close estate" — and at the far end the estate's status,
/// whether it still owes anything, and the drawer toggle.
///
/// It carries the estate's ADDRESS and one status: whether the estate still owes anything,
/// a button showing `task_alt` in the success color when nothing is owed and
/// `assignment_late` with the count otherwise, which opens the Overview, where the list
/// is. Hidden on the Start screen. Nothing else about the estate is here. What it IS — the
/// customer it stands for, whom its commands run as, its deployment mode, its schema, how
/// far its HCL has been taken — is the Overview's identity card, so each fact has one
/// place. The versions are not here either: the window title carries satz-studio's, a
/// newer release of either is the banner's business, and Settings holds the satz binary
/// with its update buttons.
#[component]
pub fn TopBar() -> Element {
    let app = use_context::<Store<AppStore>>();
    let estate_handle = try_use_context::<Coroutine<EstateAction>>();
    let open = app.open().cloned();
    let loading = app.estate().loading().cloned();
    let diagnostics = app.estate().diagnostics().len();
    let drawer_open = app.drawer_open().cloned();
    // `None` on the Start screen: the status is an open estate's
    let owes = owed_now(app).map(|owed| owed.len());

    let (title, subtitle) = match &open {
        Some(o) => (o.name.clone(), o.dir.display().to_string()),
        None => ("satz-studio".to_string(), String::new()),
    };

    rsx! {
        TopAppBar {
            title,
            subtitle,
            beside: rsx! {
                if let Some(estate_handle) = estate_handle {
                    IconButton {
                        icon: "refresh",
                        label: "Reload estate",
                        onclick: move |_| estate_handle.send(EstateAction::Reload),
                    }
                    IconButton {
                        icon: "swap_horiz",
                        label: "Switch estate",
                        onclick: move |_| estate_handle.send(EstateAction::Switch),
                    }
                    IconButton {
                        icon: "close",
                        label: "Close estate",
                        onclick: move |_| estate_handle.send(EstateAction::Close),
                    }
                    if loading {
                        CircularProgress { size: 24 }
                    }
                }
            },
            if let Some(owes) = owes {
                Badge { count: owes,
                    IconButton {
                        icon: if owes == 0 { "task_alt" } else { "assignment_late" },
                        label: status_label(owes),
                        filled: owes == 0,
                        class: if owes == 0 { "top-bar__status--clear" } else { "" },
                        onclick: move |_| app.nav().set(View::Overview),
                    }
                }
            }
            Badge { count: diagnostics,
                IconButton {
                    icon: "troubleshoot",
                    label: "Findings",
                    selected: drawer_open,
                    onclick: move |_| app.drawer_open().toggle(),
                }
            }
        }
    }
}

/// The status button's name, read as its tooltip and by a screen reader.
fn status_label(owes: usize) -> String {
    match owes {
        0 => "Nothing still to do".to_string(),
        1 => "1 item still to do".to_string(),
        n => format!("{n} items still to do"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_names_how_many_items_are_still_to_do() {
        assert_eq!(status_label(0), "Nothing still to do");
        assert_eq!(status_label(1), "1 item still to do");
        assert_eq!(status_label(4), "4 items still to do");
    }
}
