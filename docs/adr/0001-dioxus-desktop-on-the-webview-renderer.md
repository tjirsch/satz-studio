# 0001 — Dioxus desktop on the webview renderer

- **Status:** accepted
- **Date:** 2026-09-13
- **Deciders:** the maintainer

## Context

satz-studio is a desktop app for macOS, Linux and Windows, written in Rust end to end,
whose interface follows the Material 3 Expressive guidelines: the colour roles, the type
scale with its emphasized variants, the shape scale, the state layers, the Material
Symbols font. It edits text with structure (typed fields over a lossless document
layer), streams command output and a chat, and has to work with a screen reader and a
keyboard. It ships as one bundle per OS with an installer, and its core crate must be
testable without a window on every CI runner.

Material 3 Expressive has no official web library (material-web is in maintenance
mode) and no Rust library, so whichever framework is chosen, the component anatomies
are written by hand. The question is what they are written in: CSS over a browser
engine, or a framework's own widget and styling model.

## Decision

Dioxus 0.7 (`0.7.10`, the newest stable release; 0.8 exists only as an alpha) on the
desktop target with the webview renderer: WebView2 on Windows, WebKitGTK on Linux,
WKWebView on macOS. The UI is Rust components rendered to HTML and styled with CSS, so
the Material 3 Expressive tokens are CSS custom properties, the component anatomies
are CSS classes, and the Material Symbols variable font is driven with
`font-variation-settings`. State lives in `dioxus-stores`; `dx bundle` produces the
installers.

## Consequences

- The webview is a runtime dependency: WebView2 is preinstalled on Windows 10 and 11
  and the MSI carries the Evergreen bootstrapper; Linux needs `webkit2gtk-4.1`, which
  Ubuntu 22.04 does not ship; macOS needs nothing. CSS stays within what WebView2 120
  and webkit2gtk 2.44 render, with `cubic-bezier` fallbacks for `linear()` easing.
- Material 3 Expressive is hand-written CSS, and every deviation from the guideline is
  recorded beside the components: one committed seed palette instead of dynamic
  colour, spring motion approximated with `linear()`.
- Text editing, selection, clipboard, scrolling and the accessibility tree are the
  browser engine's, not the framework's.
- `dioxus-native` (Blitz) is not used while it is work in progress. The views are
  written against Dioxus's component model, so the renderer can change without them.
- A `dioxus-cli` at the workspace's Dioxus version is a prerequisite for `dx serve` and
  `dx bundle`; plain `cargo run` works without it.

## Pros and cons of the options

### A — Dioxus 0.7 desktop, webview renderer *(chosen)*

- **Good:** Rust end to end; a React-shaped component model; CSS gives the Material
  tokens and anatomies directly; text editing and accessibility come from the browser
  engine; `dx bundle` ships `.dmg`, `.deb`, `.AppImage` and `.msi`.
- **Bad:** a webview runtime per platform, and a CSS feature set bounded by the oldest
  engine shipped; two worlds (Rust state and the DOM) to keep in step.

### B — Tauri 2

- **Good:** the same webview runtimes, mature bundling and signing, a large ecosystem.
- **Bad:** the UI is written in TypeScript with a web framework; the Rust side is a
  backend behind an IPC boundary. Two languages and a serialisation layer for an app
  that exists to put a Rust interface over satz-core's types.

### C — iced 0.14 or egui

- **Good:** pure Rust, no runtime dependency, one rendering path everywhere.
- **Bad:** no CSS, so every Material Expressive anatomy is built per widget from
  primitives; text editing (multi-line fields, selection, IME) and the accessibility
  tree are weaker than a browser engine's; the type scale and the state layers are
  re-implemented rather than declared.

### D — Slint 1.15

- **Good:** a complete toolkit with native rendering and good accessibility.
- **Bad:** the interface is written in `.slint`, a second language beside Rust; its
  Material style is Material 3, not Expressive, so the deviation sits in the toolkit's
  own theme.

### E — GPUI

- **Good:** GPU rendering and an editor-grade text stack.
- **Bad:** pre-1.0 with breaking releases; documentation and ecosystem follow one
  editor's needs.

### F — Xilem

- **Good:** the design direction is right for Rust UI.
- **Bad:** not production-ready, by its own statement.

### G — dioxus-native (Blitz)

- **Good:** the same Dioxus code without a webview.
- **Bad:** work in progress. It is the migration path from A, not the starting point.
