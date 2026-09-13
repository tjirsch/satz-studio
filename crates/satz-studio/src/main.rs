//! satz-studio — the window. `main` opens it at 1280×840 (900×600 at the smallest),
//! initialises tracing from `RUST_LOG`, and launches [`app::App`].

mod app;
mod components;
mod shell;
mod state;
mod views;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let window = WindowBuilder::new()
        .with_title("satz-studio")
        .with_inner_size(LogicalSize::new(1280.0, 840.0))
        .with_min_inner_size(LogicalSize::new(900.0, 600.0));
    dioxus::LaunchBuilder::new()
        .with_cfg(Config::new().with_window(window))
        .launch(app::App);
}
