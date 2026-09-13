use dioxus::prelude::*;

/// A Material Symbols Rounded glyph by ligature name. `filled` sets the FILL axis,
/// `size` the pixel size and the optical-size axis (20–48).
#[component]
pub fn Icon(
    name: String,
    #[props(default)] filled: bool,
    #[props(default = 24)] size: u32,
    #[props(default)] class: String,
) -> Element {
    let fill = u8::from(filled);
    let opsz = size.clamp(20, 48);
    rsx! {
        span {
            class: "m-icon {class}",
            style: "font-size: {size}px; font-variation-settings: 'FILL' {fill}, 'wght' 400, 'GRAD' 0, 'opsz' {opsz};",
            "aria-hidden": "true",
            "{name}"
        }
    }
}
