use dioxus::prelude::*;
use dioxus_icons::lucide::{Command, Keyboard};

use crate::components::sidebar::{
    Sidebar, SidebarCollapsible, SidebarContent, SidebarFooter, SidebarGroup, SidebarGroupContent,
    SidebarMenu, SidebarMenuBadge, SidebarMenuButton, SidebarMenuItem,
};
use crate::state::{AppStore, AppStoreStoreExt, EstateStoreStoreExt, View};
use crate::views::overview::owed_now;

/// The sidebar: the primary destinations in the order the work happens, and Agent,
/// Commands and Settings in its foot. Commands is not a destination — it toggles the
/// palette over whatever is showing — and it stands in the foot because that is where
/// the things that are not places stand. It needs an estate, like Agent: the palette's
/// entries all act on one. With no estate open there is nothing to work on, so the
/// sidebar carries Settings alone and the window stands on the Start screen — the doors.
/// Overview carries the count of what the estate owes and Decisions the count of
/// unanswered questions. Folded, the sidebar shows the icons, and each one its name on
/// hover.
#[component]
pub fn AppSidebar() -> Element {
    let app = use_context::<Store<AppStore>>();
    let current = app.nav().cloned();
    let has_estate = app.open().is_some();
    let questions = app.estate().questions().cloned();
    let unanswered = questions
        .as_ref()
        .map(|q| q.summary.unanswered)
        .unwrap_or(0);
    let owes = owed_now(app).map_or(0, |owed| owed.len());
    // a workload estate without a preset library of its own has no Presets
    // destination; until the model is read the sidebar shows every destination
    let presets = app
        .estate()
        .model()
        .read()
        .as_deref()
        .is_none_or(|m| m.packs.has_presets());
    let primary: Vec<View> = if has_estate {
        View::primary(presets)
    } else {
        Vec::new()
    };
    let secondary: Vec<View> = View::SECONDARY
        .into_iter()
        .filter(|v| has_estate || !v.needs_estate())
        .collect();

    // the shortcut is ⌘K on macOS and Ctrl+K elsewhere, and the button says which of the
    // two this machine takes
    let palette_label = if cfg!(target_os = "macos") {
        "Commands (⌘K)"
    } else {
        "Commands (Ctrl+K)"
    };

    rsx! {
        Sidebar { collapsible: SidebarCollapsible::Icon,
            SidebarContent {
                SidebarGroup {
                    SidebarGroupContent {
                        SidebarMenu {
                            for view in primary.iter().copied() {
                                {
                                    let count = match view {
                                        View::Overview => owes,
                                        View::Decisions => unanswered,
                                        _ => 0,
                                    };
                                    rsx! {
                                        SidebarMenuItem { key: "{view.label()}",
                                            Destination {
                                                label: view.label(),
                                                is_active: view == current,
                                                onclick: move |_| app.nav().set(view),
                                                {view.icon(16)}
                                            }
                                            if count > 0 {
                                                SidebarMenuBadge { "{count}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            SidebarFooter {
                SidebarMenu {
                    for view in secondary {
                        SidebarMenuItem { key: "{view.label()}",
                            Destination {
                                label: view.label(),
                                is_active: view == current,
                                onclick: move |_| app.nav().set(view),
                                {view.icon(16)}
                            }
                        }
                        if view == View::Agent {
                            SidebarMenuItem { key: "{palette_label}",
                                Destination {
                                    label: palette_label,
                                    is_active: app.palette_open().cloned(),
                                    onclick: move |_| app.palette_open().toggle(),
                                    // the shortcut is ⌘K on macOS and Ctrl+K elsewhere
                                    if cfg!(target_os = "macos") {
                                        Command { size: 16 }
                                    } else {
                                        Keyboard { size: 16 }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One entry of the sidebar: its icon, its name, and what a click does.
#[component]
fn Destination(
    label: &'static str,
    is_active: bool,
    onclick: EventHandler<MouseEvent>,
    children: Element,
) -> Element {
    rsx! {
        SidebarMenuButton {
            is_active,
            tooltip: rsx! { "{label}" },
            r#as: move |attributes: Vec<Attribute>| rsx! {
                button {
                    r#type: "button",
                    onclick: move |e| onclick.call(e),
                    ..attributes,
                    {children.clone()}
                    span { "{label}" }
                }
            },
        }
    }
}
