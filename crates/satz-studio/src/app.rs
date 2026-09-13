//! The root component: the stores, the app coroutine, the stylesheets and the font,
//! the theme attribute on the document root, and the shell around the current view.

use dioxus::prelude::*;
use satz_studio_core::settings::{Settings, Theme, settings_path};

use crate::shell::Shell;
use crate::state::{AppStore, AppStoreStoreExt, DiagnosticSelection, app_coroutine};

const TOKENS: Asset = asset!("/assets/css/tokens.css");
const BASE: Asset = asset!("/assets/css/base.css");
const COMPONENTS: Asset = asset!("/assets/css/components.css");
const VIEWS: Asset = asset!("/assets/css/views.css");
const SYMBOLS: Asset = asset!("/assets/fonts/MaterialSymbolsRounded.woff2");

#[component]
pub fn App() -> Element {
    let settings = use_hook(|| Settings::load().map_err(|e| e.to_string()));
    match settings {
        Ok(settings) => rsx! { Studio { settings } },
        Err(error) => rsx! { Fatal { error } },
    }
}

/// The settings file did not load: nothing runs on defaults over a broken file.
#[component]
fn Fatal(error: String) -> Element {
    let path = settings_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| e.to_string());
    rsx! {
        document::Stylesheet { href: TOKENS }
        document::Stylesheet { href: BASE }
        main { class: "fatal",
            h1 { "satz-studio cannot start" }
            p { "{error}" }
            p { "Fix or remove the settings file and start again: " code { "{path}" } }
        }
    }
}

#[component]
fn Studio(settings: Settings) -> Element {
    let app = use_store(move || AppStore::new(settings));
    use_context_provider(|| app);
    use_context_provider(|| DiagnosticSelection(Signal::new(None)));
    use_coroutine(move |rx| app_coroutine(rx, app));

    // The theme lives on the document root so `body` and every portal see the same tokens.
    use_effect(move || {
        let theme = app.settings().read().theme;
        let script = match theme {
            Theme::System => "document.documentElement.removeAttribute('data-theme')".to_string(),
            Theme::Light => {
                "document.documentElement.setAttribute('data-theme', 'light')".to_string()
            }
            Theme::Dark => {
                "document.documentElement.setAttribute('data-theme', 'dark')".to_string()
            }
        };
        let _ = document::eval(&script);
    });

    let font_face = format!(
        "@font-face {{ font-family: 'Material Symbols Rounded'; font-style: normal; font-weight: 100 700; font-display: block; src: url('{SYMBOLS}') format('woff2'); }}"
    );

    rsx! {
        document::Stylesheet { href: TOKENS }
        document::Stylesheet { href: BASE }
        document::Stylesheet { href: COMPONENTS }
        document::Stylesheet { href: VIEWS }
        document::Style { "{font_face}" }
        Shell {}
    }
}
