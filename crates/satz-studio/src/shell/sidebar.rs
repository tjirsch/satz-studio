use dioxus::prelude::*;

use crate::components::sidebar::{
    Sidebar, SidebarCollapsible, SidebarContent, SidebarFooter, SidebarGroup, SidebarGroupContent,
    SidebarMenu, SidebarMenuBadge, SidebarMenuButton, SidebarMenuItem,
};
use crate::state::{AppStore, AppStoreStoreExt, EstateStoreStoreExt, View};
use crate::views::overview::owed_now;

/// The sidebar: the primary destinations in the order the work happens, and Agent and
/// Settings in its foot. The commands palette has no entry here: ⌘K on macOS, Ctrl+K
/// elsewhere, opens it over whatever is showing. With no estate open there is nothing to work on, so the
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
