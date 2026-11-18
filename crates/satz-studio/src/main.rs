//! satz-studio — the window. `main` opens it at 1280×840 (900×600 at the smallest), titled
//! with the app's version, initialises tracing from `RUST_LOG`, and launches [`app::App`].

mod app;
// The component library's own code, copied in by `dx components add` and kept as
// published so `dx components update` can replace it. It offers more variants and
// sizes than the app uses; those are part of the library, not dead code to remove.
#[allow(dead_code, unused_imports)]
mod components;
mod shell;
mod state;
mod views;
mod widgets;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let window = WindowBuilder::new()
        .with_title(concat!("satz-studio ", env!("CARGO_PKG_VERSION")))
        .with_inner_size(LogicalSize::new(1280.0, 840.0))
        .with_min_inner_size(LogicalSize::new(900.0, 600.0));
    dioxus::LaunchBuilder::new()
        .with_cfg(Config::new().with_window(window))
        .launch(app::App);
}
