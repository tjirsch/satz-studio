//! The shell around every view: the sidebar, the top bar, the satz banner, the path bar
//! at the foot of a step, the Findings drawer and the toasts. While an estate is open the whole
//! frame sits inside [`EstateHost`], which owns that estate's coroutine, so every part
//! of the shell can reach it through `try_use_context::<Coroutine<EstateAction>>()`.

mod banner;
mod drawer;
mod notices;
mod placeholder;
mod sidebar;
mod toasts;
mod top_bar;

use dioxus::desktop::{WindowEvent, tao, use_wry_event_handler};
use dioxus::prelude::*;

pub use banner::SatzBanner;
pub use drawer::DiagnosticsDrawer;
pub use notices::NoticeDialog;
pub use placeholder::NoEstate;
pub use sidebar::AppSidebar;
pub use toasts::ToastHost;
pub use top_bar::TopBar;

use crate::components::sidebar::{SidebarInset, SidebarProvider};
use crate::state::{AppStore, AppStoreStoreExt, EstateAction, OpenEstate, View, estate_coroutine};
use crate::views::agent::AgentView;
use crate::views::apply::ApplyView;
use crate::views::commands::CommandPalette;
use crate::views::documents::DocumentsView;
use crate::views::estate::EstateView;
use crate::views::estates::EstatesView;
use crate::views::interview::DecisionsView;
use crate::views::map::PresetsView;
use crate::views::overview::OverviewView;
use crate::views::packs::PacksView;
use crate::views::path::PathBar;
use crate::views::security::SecurityView;
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
    let estate = use_coroutine(move |rx| estate_coroutine(rx, session.clone(), app));
    // `apply` and `bootstrap` run in the user's terminal (ADR 0006): the estate may
    // have changed while the window was away, so it is read again on focus.
    use_wry_event_handler(move |event, _| {
        if let tao::event::Event::WindowEvent {
            event: WindowEvent::Focused(true),
            ..
        } = event
        {
            estate.send(EstateAction::Reload);
        }
    });
    rsx! { Frame {} }
}

#[component]
fn Frame() -> Element {
    let app = use_context::<Store<AppStore>>();
    rsx! {
        SidebarProvider {
            AppSidebar {}
            SidebarInset { class: "shell__main",
                TopBar {}
                SatzBanner {}
                div { class: "shell__content", Content {} }
                PathBar {}
                DiagnosticsDrawer {}
            }
        }
        if app.palette_open().cloned() && app.open().is_some() {
            CommandPalette {}
        }
        if app.open().is_some() {
            NoticeDialog {}
        }
        ToastHost {}
    }
}

/// The current view; a destination that needs an estate shows a card without one, and
/// the Start screen is where the window stands while none is open.
#[component]
fn Content() -> Element {
    let app = use_context::<Store<AppStore>>();
    let view = app.nav().cloned();
    if view.needs_estate() && app.open().is_none() {
        return rsx! { NoEstate { view } };
    }
    match view {
        View::Start => rsx! { EstatesView {} },
        View::Overview => rsx! { OverviewView {} },
        View::Decisions => rsx! { DecisionsView {} },
        View::Presets => rsx! { PresetsView {} },
        View::Packs => rsx! { PacksView {} },
        View::Estate => rsx! { EstateView {} },
        View::Apply => rsx! { ApplyView {} },
        View::Security => rsx! { SecurityView {} },
        View::Documents => rsx! { DocumentsView {} },
        View::Agent => rsx! { AgentView {} },
        View::Settings => rsx! { SettingsView {} },
    }
}
