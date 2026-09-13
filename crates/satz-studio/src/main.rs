//! satz-studio — the window. The shell, the views and the components arrive with U7;
//! this is the window that proves the toolchain on every platform.

use dioxus::prelude::*;

fn main() {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    dioxus::LaunchBuilder::new()
        .with_cfg(
            dioxus::desktop::Config::new()
                .with_window(dioxus::desktop::WindowBuilder::new().with_title("satz-studio").with_inner_size(dioxus::desktop::LogicalSize::new(1280.0, 840.0))),
        )
        .launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Stylesheet { href: asset!("/assets/css/app.css") }
        main { class: "boot",
            h1 { "satz-studio" }
            p { "satz {satz_studio_core::satz::MIN_SATZ} or newer, read from the estate you open." }
        }
    }
}
