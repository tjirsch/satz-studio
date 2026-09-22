//! The Gallery view: every component in its variants, the light and the dark theme
//! side by side — the visual checklist of the component library.

use dioxus::prelude::*;

use crate::components::*;

#[component]
pub fn GalleryView() -> Element {
    rsx! {
        div { class: "view gallery",
            h1 { class: "view__title", "Gallery" }
            div { class: "gallery__panes",
                div { class: "gallery__pane", "data-theme": "light", Sheet { theme: "light" } }
                div { class: "gallery__pane", "data-theme": "dark", Sheet { theme: "dark" } }
            }
        }
    }
}

const COLOR_ROLES: &[&str] = &[
    "primary",
    "on-primary",
    "primary-container",
    "on-primary-container",
    "secondary",
    "on-secondary",
    "secondary-container",
    "on-secondary-container",
    "tertiary",
    "on-tertiary",
    "tertiary-container",
    "on-tertiary-container",
    "error",
    "on-error",
    "error-container",
    "on-error-container",
    "surface",
    "surface-dim",
    "surface-bright",
    "surface-container-lowest",
    "surface-container-low",
    "surface-container",
    "surface-container-high",
    "surface-container-highest",
    "on-surface",
    "on-surface-variant",
    "outline",
    "outline-variant",
    "inverse-surface",
    "inverse-on-surface",
    "inverse-primary",
];

const TYPE_ROLES: &[(&str, &str)] = &[
    ("display-large", "Display large"),
    ("display-medium", "Display medium"),
    ("display-small", "Display small"),
    ("headline-large", "Headline large"),
    ("headline-medium", "Headline medium"),
    ("headline-small", "Headline small"),
    ("title-large", "Title large"),
    ("title-medium", "Title medium"),
    ("title-small", "Title small"),
    ("body-large", "Body large"),
    ("body-medium", "Body medium"),
    ("body-small", "Body small"),
    ("label-large", "Label large"),
    ("label-medium", "Label medium"),
    ("label-small", "Label small"),
];

const SHAPES: &[&str] = &[
    "none",
    "extra-small",
    "small",
    "medium",
    "large",
    "large-increased",
    "extra-large",
    "extra-large-increased",
    "extra-extra-large",
    "full",
];

#[component]
fn Sheet(theme: String) -> Element {
    let mut switch_on = use_signal(|| true);
    let mut checked = use_signal(|| true);
    let mut radio = use_signal(|| 0u8);
    let mut text = use_signal(|| "C0example.satz".to_string());
    let mut segment = use_signal(|| "cloud".to_string());
    let mut tab = use_signal(|| 0u8);
    let mut dialog_open = use_signal(|| false);
    let mut filter_on = use_signal(|| true);
    let mut selected_row = use_signal(|| 0usize);

    rsx! {
        div { class: "sheet",
            h2 { class: "sheet__theme", "{theme}" }

            section { class: "sheet__section",
                h3 { "Colour" }
                div { class: "sheet__swatches",
                    for role in COLOR_ROLES {
                        div { key: "{role}", class: "sheet__swatch", style: "background: var(--md-sys-color-{role});",
                            span { class: "sheet__swatch-name", "{role}" }
                        }
                    }
                }
            }

            section { class: "sheet__section",
                h3 { "Type scale" }
                for (class, label) in TYPE_ROLES {
                    p { key: "{class}", class: "md-{class}", "{label}" }
                }
                p { class: "md-headline-medium md-emphasized", "Headline medium, emphasized" }
                p { class: "md-title-large md-emphasized", "Title large, emphasized" }
                p { class: "md-body-large md-emphasized", "Body large, emphasized" }
            }

            section { class: "sheet__section",
                h3 { "Shape" }
                div { class: "sheet__row",
                    for shape in SHAPES {
                        div { key: "{shape}", class: "sheet__shape", style: "border-radius: var(--md-sys-shape-corner-{shape});", span { "{shape}" } }
                    }
                }
            }

            section { class: "sheet__section",
                h3 { "Buttons" }
                div { class: "sheet__row",
                    Button { variant: ButtonVariant::Filled, onclick: |_| {}, "Filled" }
                    Button { variant: ButtonVariant::Tonal, onclick: |_| {}, "Tonal" }
                    Button { variant: ButtonVariant::Outlined, onclick: |_| {}, "Outlined" }
                    Button { variant: ButtonVariant::Text, onclick: |_| {}, "Text" }
                    Button { variant: ButtonVariant::Elevated, onclick: |_| {}, "Elevated" }
                    Button { variant: ButtonVariant::Filled, icon: "add", onclick: |_| {}, "With icon" }
                    Button { variant: ButtonVariant::Filled, disabled: true, onclick: |_| {}, "Disabled" }
                }
                div { class: "sheet__row",
                    ButtonGroup { connected: true,
                        Button { variant: ButtonVariant::Tonal, icon: "format_bold", onclick: |_| {}, "Bold" }
                        Button { variant: ButtonVariant::Tonal, icon: "format_italic", onclick: |_| {}, "Italic" }
                        Button { variant: ButtonVariant::Tonal, icon: "format_underlined", onclick: |_| {}, "Underline" }
                    }
                    ButtonGroup {
                        Button { variant: ButtonVariant::Outlined, onclick: |_| {}, "One" }
                        Button { variant: ButtonVariant::Outlined, onclick: |_| {}, "Two" }
                    }
                }
            }

            section { class: "sheet__section",
                h3 { "Icon buttons and FABs" }
                div { class: "sheet__row",
                    IconButton { icon: "favorite", label: "Standard", onclick: |_| {} }
                    IconButton { icon: "favorite", label: "Standard selected", selected: true, onclick: |_| {} }
                    IconButton { icon: "favorite", label: "Filled", variant: IconButtonVariant::Filled, onclick: |_| {} }
                    IconButton { icon: "favorite", label: "Filled selected", variant: IconButtonVariant::Filled, selected: true, onclick: |_| {} }
                    IconButton { icon: "favorite", label: "Tonal", variant: IconButtonVariant::Tonal, onclick: |_| {} }
                    IconButton { icon: "favorite", label: "Tonal selected", variant: IconButtonVariant::Tonal, selected: true, onclick: |_| {} }
                    IconButton { icon: "favorite", label: "Outlined", variant: IconButtonVariant::Outlined, onclick: |_| {} }
                    IconButton { icon: "favorite", label: "Disabled", disabled: true, onclick: |_| {} }
                }
                div { class: "sheet__row",
                    Fab { icon: "edit", label: "", size: FabSize::Small, onclick: |_| {} }
                    Fab { icon: "edit", label: "", size: FabSize::Medium, onclick: |_| {} }
                    Fab { icon: "edit", label: "", size: FabSize::Large, onclick: |_| {} }
                    Fab { icon: "add", label: "Extended", onclick: |_| {} }
                }
            }

            section { class: "sheet__section",
                h3 { "Chips" }
                div { class: "sheet__row",
                    Chip { kind: ChipKind::Assist, icon: "event", label: "Assist", onclick: |_| {} }
                    Chip { kind: ChipKind::Assist, icon: "badge", label: "Static" }
                    Chip { kind: ChipKind::Assist, icon: "error", label: "Error", error: true }
                    Chip { kind: ChipKind::Filter, label: "Filter", selected: filter_on(), onclick: move |_| filter_on.toggle() }
                    Chip { kind: ChipKind::Filter, label: "Filter off", onclick: |_| {} }
                    Chip { kind: ChipKind::Input, label: "Input", onremove: |_| {} }
                    Chip { kind: ChipKind::Input, icon: "person", label: "With icon", onremove: |_| {} }
                }
            }

            section { class: "sheet__section",
                h3 { "Cards" }
                div { class: "sheet__row sheet__row--stretch",
                    Card { variant: CardVariant::Elevated, class: "sheet__card", h4 { "Elevated" } p { "Tinted shadow, surface-container-low." } }
                    Card { variant: CardVariant::Filled, class: "sheet__card", h4 { "Filled" } p { "surface-container-highest." } }
                    Card { variant: CardVariant::Outlined, class: "sheet__card", onclick: |_| {}, h4 { "Outlined, clickable" } p { "outline-variant border, state layer." } }
                }
            }

            section { class: "sheet__section",
                h3 { "Text fields" }
                div { class: "sheet__row sheet__row--stretch",
                    TextField { label: "Empty", value: "", oninput: |_| {} }
                    TextField { label: "Estate", value: text(), leading_icon: "description", supporting: "inside yaml_dir if relative", monospace: true, oninput: move |v| text.set(v) }
                    TextField { label: "Error", value: "not a path", error: true, supporting: "the file does not exist", oninput: |_| {} }
                    TextField { label: "Disabled", value: "read-only", disabled: true, oninput: |_| {} }
                    TextField { label: "Password", value: "secret", password: true, oninput: |_| {} }
                }
            }

            section { class: "sheet__section",
                h3 { "Selection" }
                div { class: "sheet__row",
                    Switch { label: "Switch", checked: switch_on(), onchange: move |v| switch_on.set(v) }
                    Switch { label: "Disabled", checked: true, disabled: true, onchange: |_| {} }
                    Checkbox { label: "Checkbox", checked: checked(), onchange: move |v| checked.set(v) }
                    Checkbox { label: "Disabled", checked: false, disabled: true, onchange: |_| {} }
                    Radio { label: "Local", checked: radio() == 0, onselect: move |_| radio.set(0) }
                    Radio { label: "Cloud", checked: radio() == 1, onselect: move |_| radio.set(1) }
                }
                div { class: "sheet__row",
                    SegmentedButton {
                        options: vec![Segment::new("local", "local").with_icon("computer"), Segment::new("cloud", "cloud").with_icon("cloud")],
                        selected: segment(),
                        onselect: move |v| segment.set(v),
                    }
                }
            }

            section { class: "sheet__section",
                h3 { "Tabs" }
                Tabs {
                    Tab { label: "Params (12)", icon: "tune", selected: tab() == 0, onclick: move |_| tab.set(0) }
                    Tab { label: "Resources (31)", icon: "account_tree", selected: tab() == 1, badge: 2, onclick: move |_| tab.set(1) }
                }
            }

            section { class: "sheet__section",
                h3 { "Dialog" }
                Button { variant: ButtonVariant::Tonal, icon: "open_in_full", onclick: move |_| dialog_open.set(true), "Open dialog" }
                Dialog {
                    open: dialog_open(),
                    title: "Close the estate?",
                    icon: "logout",
                    ondismiss: move |_| dialog_open.set(false),
                    actions: rsx! {
                        Button { variant: ButtonVariant::Text, onclick: move |_| dialog_open.set(false), "Cancel" }
                        Button { variant: ButtonVariant::Filled, onclick: move |_| dialog_open.set(false), "Close" }
                    },
                    p { "The session ends; nothing on disk changes." }
                }
            }

            section { class: "sheet__section",
                h3 { "List and tree" }
                List {
                    ListItem { headline: "C0example.satz", supporting: "cloud · runs as the IaC service account", leading: rsx! { Icon { name: "description" } }, trailing: rsx! { Icon { name: "chevron_right" } }, selected: selected_row() == 0, onclick: move |_| selected_row.set(0) }
                    ListItem { headline: "greenfield.satz", supporting: "deployment mode not set", leading: rsx! { Icon { name: "description" } }, selected: selected_row() == 1, onclick: move |_| selected_row.set(1) }
                    ListItem { headline: "A static row", supporting: "no handler" }
                }
                Tree {
                    TreeItem { label: "google_folder.acme", icon: "folder",
                        TreeItem { label: "google_project.acme-infra-001", icon: "inventory_2", supporting: "infra",
                            TreeItem { label: "google_storage_bucket.acme-organization-audit-bucket", icon: "database", selected: true }
                        }
                        TreeItem { label: "google_project.acme-log-001", icon: "inventory_2", open: false,
                            TreeItem { label: "google_logging_project_sink.audit", icon: "receipt_long" }
                        }
                    }
                }
                ConnectorTree {
                    root: rsx! { ConnectorBox { name: "scc-service-enablement.satz", state: "on" } },
                    ConnectorBranch {
                        node: rsx! { ConnectorBox { name: "scc-notifications.satz", state: "on" } },
                        branches: rsx! {
                            ConnectorBranch {
                                line: ConnectorLine::Dashed,
                                label: rsx! { span { "dashed, labelled" } },
                                node: rsx! { ConnectorBox { name: "scc-findings-mail.satz", state: "off" } },
                            }
                            ConnectorBranch {
                                node: rsx! { ConnectorBox { name: "scc-findings-siem.satz", state: "off" } },
                            }
                        },
                    }
                    ConnectorBranch {
                        error: true,
                        label: rsx! { span { "error" } },
                        node: rsx! { ConnectorBox { name: "scc-export.satz", state: "on" } },
                    }
                }
            }

            section { class: "sheet__section",
                h3 { "Badges, progress, tooltip" }
                div { class: "sheet__row",
                    Badge { count: 3, Icon { name: "quiz" } }
                    Badge { count: 120, Icon { name: "account_tree" } }
                    Badge { dot: true, Icon { name: "notifications" } }
                    Badge { count: 0, Icon { name: "notifications" } }
                    Tooltip { text: "A plain tooltip", IconButton { icon: "help", label: "Help", onclick: |_| {} } }
                    CircularProgress { value: 0.65 }
                    CircularProgress {}
                }
                LinearProgress { value: 0.4 }
                LinearProgress {}
            }

            section { class: "sheet__section",
                h3 { "Navigation rail, top app bar, snackbar" }
                div { class: "sheet__row sheet__row--stretch",
                    div { class: "sheet__rail",
                        NavRail {
                            fab: rsx! { Fab { icon: "folder_open", label: "", onclick: |_| {} } },
                            NavRailItem { icon: "dashboard", label: "Overview", selected: true, badge: 3, onclick: |_| {} }
                            NavRailItem { icon: "quiz", label: "Decisions", badge: 4, onclick: |_| {} }
                            NavRailItem { icon: "settings", label: "Settings", onclick: |_| {} }
                        }
                    }
                    div { class: "sheet__stack grow",
                        TopAppBar { title: "C0example.satz", subtitle: "~/estates/acme",
                            Chip { kind: ChipKind::Assist, icon: "badge", label: "svc-iac@acme-infra-001.iam.gserviceaccount.com" }
                            IconButton { icon: "refresh", label: "Reload", onclick: |_| {} }
                        }
                        Snackbar { text: "Settings saved", ondismiss: |_| {} }
                        Snackbar { text: "satz 0.51.1 is too old: satz-studio needs 0.56.1 or newer", error: true, action_label: "Settings", onaction: |_| {}, ondismiss: |_| {} }
                    }
                }
            }
        }
    }
}

/// An outlined card with a 32 px head row, the box the connector tree's offsets assume.
#[component]
fn ConnectorBox(name: String, state: String) -> Element {
    rsx! {
        Card { variant: CardVariant::Outlined,
            div { class: "sheet__row",
                Icon { name: "extension", size: 20 }
                code { class: "grow", "{name}" }
                Chip { kind: ChipKind::Assist, label: "{state}" }
            }
        }
    }
}
