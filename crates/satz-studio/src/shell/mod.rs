//! The shell around every view: the navigation rail, the top bar, the satz banner,
//! the diagnostics drawer and the snackbar host. While an estate is open the whole
//! frame sits inside [`EstateHost`], which owns that estate's coroutine, so every part
//! of the shell can reach it through `try_use_context::<Coroutine<EstateAction>>()`.

mod banner;
mod drawer;
mod placeholder;
mod rail;
mod snackbar;
mod top_bar;

use dioxus::prelude::*;

pub use banner::SatzBanner;
pub use drawer::DiagnosticsDrawer;
pub use placeholder::{NoEstate, NotBuilt};
pub use rail::NavigationRail;
pub use snackbar::SnackbarHost;
pub use top_bar::TopBar;

use crate::state::{AppStore, AppStoreStoreExt, OpenEstate, View, estate_coroutine};
use crate::views::commands::CommandsView;
use crate::views::estates::EstatesView;
use crate::views::gallery::GalleryView;
use crate::views::settings::SettingsView;

#[component]
pub fn Shell() -> Element {
    let app = use_context::<Store<AppStore>>();
    match app.open().cloned() {
        Some(open) => rsx! { EstateHost { key: "{open.main.display()}", open } },
        None => rsx! { Frame {} },
    }
}

/// One per open estate: starts the estate coroutine with the session and frames the shell.
#[component]
fn EstateHost(open: OpenEstate) -> Element {
    let app = use_context::<Store<AppStore>>();
    let session = open.session.clone();
    use_coroutine(move |rx| estate_coroutine(rx, session.clone(), app));
    rsx! { Frame {} }
}

#[component]
fn Frame() -> Element {
    rsx! {
        div { class: "shell",
            NavigationRail {}
            div { class: "shell__main",
                TopBar {}
                SatzBanner {}
                main { class: "shell__content", Content {} }
                DiagnosticsDrawer {}
            }
            SnackbarHost {}
        }
    }
}

/// The current view; a destination that needs an estate shows a card without one.
#[component]
fn Content() -> Element {
    let app = use_context::<Store<AppStore>>();
    let view = app.nav().cloned();
    if view.needs_estate() && app.open().is_none() {
        return rsx! { NoEstate { view } };
    }
    match view {
        View::Estates => rsx! { EstatesView {} },
        View::Settings => rsx! { SettingsView {} },
        View::Gallery => rsx! { GalleryView {} },
        View::Commands => rsx! { CommandsView {} },
        View::Interview | View::Params | View::Map | View::Resources | View::Chat => {
            rsx! { NotBuilt { view } }
        }
    }
}
