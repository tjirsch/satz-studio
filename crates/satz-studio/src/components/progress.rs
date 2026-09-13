use dioxus::prelude::*;

/// A linear progress indicator: determinate with `value` in 0..=1 (with the stop
/// indicator at the track's end), indeterminate without.
#[component]
pub fn LinearProgress(
    #[props(default)] value: Option<f32>,
    #[props(default)] class: String,
) -> Element {
    let percent = value.map(|v| (v.clamp(0.0, 1.0) * 100.0).round());
    rsx! {
        div {
            class: "m-linear-progress {class}",
            class: if value.is_none() { "m-linear-progress--indeterminate" },
            role: "progressbar",
            "aria-valuemin": "0",
            "aria-valuemax": "100",
            "aria-valuenow": percent.map(|p| p.to_string()),
            span { class: "m-linear-progress__active", style: percent.map(|p| format!("width: {p}%")) }
            span { class: "m-linear-progress__track" }
            span { class: "m-linear-progress__stop" }
        }
    }
}

/// A circular progress indicator, 4px stroke: determinate with `value`, else spinning.
#[component]
pub fn CircularProgress(
    #[props(default)] value: Option<f32>,
    #[props(default = 40)] size: u32,
    #[props(default)] class: String,
) -> Element {
    const CIRCUMFERENCE: f32 = 2.0 * std::f32::consts::PI * 20.0;
    let offset = value.map(|v| CIRCUMFERENCE * (1.0 - v.clamp(0.0, 1.0)));
    rsx! {
        svg {
            class: "m-circular-progress {class}",
            class: if value.is_none() { "m-circular-progress--indeterminate" },
            role: "progressbar",
            width: "{size}",
            height: "{size}",
            view_box: "0 0 48 48",
            circle { class: "m-circular-progress__track", cx: "24", cy: "24", r: "20" }
            circle {
                class: "m-circular-progress__arc",
                cx: "24",
                cy: "24",
                r: "20",
                stroke_dasharray: "{CIRCUMFERENCE}",
                stroke_dashoffset: offset.map(|o| o.to_string()),
            }
        }
    }
}
