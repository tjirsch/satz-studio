# 0034 — the window is built from the Dioxus component library

- **Status:** accepted
- **Date:** 2026-11-20
- **Deciders:** the maintainer

## Context

The window followed Material 3 Expressive
([ADR 0001](0001-dioxus-desktop-on-the-webview-renderer.md)). No Material component library
exists for Dioxus, so every control was the app's own: 26 UI component files, a stylesheet
of design values (`tokens.css`), one of UI component anatomies (`components.css`), one of base
rules (`base.css`) and one of view styles (`views.css`), about 4,400 lines of CSS, and the
Material Symbols font (5.4 MB). Each control only approximated the guideline, `docs/ui.md`
listed the departures UI component by UI component, and every new view added more of it.
Material 3 was wanted as one consistent look to follow, not for itself. Dioxus has a UI component
library of its own, with its own look, and the maintainer decided to take it completely:
its UI components, its theme, its icons and its typeface.

A first attempt made the same switch on the `main` of 2026-11-18 (pull request #108). The
window changed underneath it — Presets and Packs became two destinations, the path bar, the
pack sources, the pack details, the editor setting and the Findings drawer arrived — so it
could not be merged as it stood; this record and the change it belongs to redo it over the
window as it is now, reusing its choices and its code where they still fit.

## Options

1. **Keep hand-building Material 3 Expressive.** Cost: every control stays the app's to
   write, style and keep accessible, and the result stays an approximation.
2. **Keep the app's UI components and re-color them** with a neutral set of values. Cost: the
   same amount of hand-built code under another paint, still no library behind it.
3. **The Dioxus component library, completely** *(chosen)*: its components, its theme, its
   icons and its typeface. Cost: the library is young, its unstyled primitives are not
   published on crates.io, and the app is run through `dx`.

## Decision

Option 3.

- **The library** is <https://github.com/DioxusLabs/components>: styled components in the
  manner of shadcn/ui over Dioxus's own unstyled, accessible primitives
  (`dioxus-primitives`). `dx components add <name>`, run in `crates/satz-studio`, copies a
  component's source and stylesheet into `crates/satz-studio/src/components/` and the theme
  into `crates/satz-studio/assets/dx-components-theme.css`. The copied code stays as
  published, so `dx components update` can replace it; the variants it brings and the app
  does not use are allowed in `src/main.rs`. Only the UI components the app uses are copied in.
- **The look** is the library's, unchanged: neutral greys with a light and a dark variant
  that follow the system or the Settings choice (`data-theme` on the document), rounded
  corners, compact controls, Lucide line icons (`dioxus-icons`), the Geist typeface
  (`assets/fonts/Geist-Variable.ttf`, bundled because a desktop app works offline).
- **The frame** is the library's `Sidebar`, folding to icons, beside the view; the top bar
  is a plain header of library buttons with the estate's status and the Findings button;
  the path bar is two library buttons with library tooltips; what the stores report is a
  library toast.
- **`assets/app.css` lays out and does not restyle.** It arranges library components and
  gives the app's own elements colors from the theme's variables. `dx` renames the
  components' classes when it builds, so a rule aimed at one would not match. The one rule
  that reaches into a library UI component is layout: a card's content stands its children in a
  column with a gap.
- **The app's own parts** are what the library has no UI component for, built from its parts,
  in `crates/satz-studio/src/widgets/`: a labelled input (`Field`), a labelled switch
  (`SwitchRow`), a list of short values (`TagList`), a value as the file writes it
  (`SourceTags`), the field that edits a typed value (`TypedField`), the help button
  (`HelpButton`), a tooltip around a control (`Tip`), the tree and the connector tree. The
  pack's in/out expander is the library's `Collapsible`.
- **The app is run through `dx`** (`dx serve --package satz-studio --hot-reload true`), as
  it was already bundled (`dx bundle`): the components' stylesheets are CSS modules that
  `dx` builds into the app, and a plain `cargo run` opens a window without them.
- **Removed with it:** the Material design values and stylesheets, the hand-built
  UI components, the Material Symbols font and its `NOTICE` entry, the extended success color,
  the Gallery view and `SATZ_STUDIO_DEBUG` that showed it, the snackbar queue (the library's
  toasts hold their own), and the rule — and its test — that a navigation rail holds three
  to seven destinations.

This supersedes the Material 3 part of ADR 0001 — the renderer decision stands — and
[ADR 0026](0026-the-content-uses-the-compact-end-of-the-scale.md), whose sizes were steps
of Material 3's scale: the library's controls are compact as published. The counts of
destinations in [ADR 0027](0027-documents-is-a-destination-of-its-own.md) and
[ADR 0028](0028-apply-is-one-destination-and-starts-with-who-is-signed-in.md) were held to
the navigation rail's seven; a sidebar has no such limit, and their destinations stand as
they decided them.

## Consequences

- The controls are accessible and keyboard-operable as the library makes them, and are
  maintained by the Dioxus project; a new view composes them and adds layout only.
- `dioxus-primitives` is not on crates.io: the root `Cargo.toml` pins it to one commit of
  the library's repository, and moving that pin is a deliberate step taken together with
  `dx components update`.
- The copied UI component code and the theme live in this repository under their own license,
  named in `NOTICE`; the Geist typeface is under the SIL Open Font License, named there too.
  Every bundle carries both license texts.
- `cargo run -p satz-studio` is no longer how the app is run; `cargo build`, `cargo test`
  and `cargo clippy` work as before.
- The window has no brand color: the theme is grey, with blue for focus and red, amber and
  green for state. A color is set through the theme's own variables in `app.css` when one
  is wanted.
- The sidebar turns into a sliding panel below 768 px of window width; the window's
  smallest width is 900 px, so it never does.
- Where a view needs something a UI component does not offer, the answer is a part of the
  app's own built from library components, not a changed copy of the component.
- Nobody has looked at the window in this look when it was decided: the layout rules
  written for the Material components were carried over, and where they do not fit the
  library's components they are adjusted in `app.css`, not in the components.

## Pros and cons of the options

### Hand-built Material 3 Expressive

- **Good:** a published, detailed guideline to measure against.
- **Bad:** no library for Dioxus, so the guideline is met by hand and only in part, and the
  amount of hand-built interface grows with every view.

### The app's UI components, re-colored

- **Good:** the smallest change to the views.
- **Bad:** keeps everything that made the first option expensive.

### The Dioxus component library *(chosen)*

- **Good:** one accepted look with a maintained implementation; the app's stylesheet
  shrinks to layout, and no font of 5 MB travels for icons.
- **Bad:** a young library pinned to a commit; fewer UI components than Material describes,
  so the tree, the connector tree and the typed field stay the app's own; the app is run
  through `dx`.
