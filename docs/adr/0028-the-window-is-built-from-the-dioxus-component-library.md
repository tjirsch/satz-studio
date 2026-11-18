# 0028 — the window is built from the Dioxus component library

- **Status:** accepted
- **Date:** 2026-11-18
- **Deciders:** the maintainer

## Context

The window followed Material 3 Expressive ([ADR 0001](0001-dioxus-desktop-on-the-webview-renderer.md)).
No Material component library exists for Dioxus, so every control was the app's own: 21
component files, a stylesheet of design values, one of component anatomies and one of
view styles, about 4,150 lines of CSS, and the Material Symbols font. Each one only
approximated the guideline, `docs/ui.md` listed the departures component by component,
and every new view added more of it. Material 3 was wanted as one consistent look to
follow, not for itself. Dioxus has a component library of its own, with its own look.

## Options

1. **Keep hand-building Material 3 Expressive.** Cost: every control stays the app's to
   write, style and keep accessible, and the result stays an approximation.
2. **Keep the app's components and re-colour them** with a neutral set of values. Cost:
   the same amount of hand-built code under another paint, still no library behind it.
3. **The Dioxus component library, completely**: its components, its theme, its icons and
   its typeface. Cost: the library is young, and its building blocks are not published.

## Decision

Option 3.

- **The library** is <https://github.com/DioxusLabs/components>: styled components in the
  manner of shadcn/ui over Dioxus's own unstyled, accessible building blocks
  (`dioxus-primitives`). `dx components add <name>`, run in `crates/satz-studio`, copies a
  component's source and stylesheet into `crates/satz-studio/src/components/` and the
  theme into `crates/satz-studio/assets/dx-components-theme.css`. The copied code stays as
  published, so `dx components update` can replace it; the variants it brings and the app
  does not use are allowed in `src/main.rs`.
- **The look** is the library's, unchanged: neutral greys with a light and a dark variant
  that follow the system or the Settings choice (`data-theme` on the document), 0.5rem
  corners, compact controls, Lucide line icons (`dioxus-icons`), the Geist typeface
  (`assets/fonts/Geist-Variable.ttf`, bundled because a desktop app works offline).
- **The frame** is the library's `Sidebar`, which folds to icons, beside the view; the
  top bar is a plain header of library buttons; what the stores report is a library
  toast.
- **`assets/app.css` lays out and does not restyle.** It arranges library components and
  gives the app's own elements colours from the theme's variables. `dx` renames the
  components' classes when it builds, so a rule aimed at one would not match.
- **The app's own parts** are what the library has no component for, built from its
  parts, in `crates/satz-studio/src/widgets/`: a labelled input (`Field`), a labelled
  switch (`SwitchRow`), a list of short values (`TagList`), a value as the file writes it
  (`SourceTags`), the field that edits a typed value (`TypedField`), the tree and the
  connector tree.
- **Removed with it:** the Material design values, the hand-built components, the
  Material Symbols font, the Gallery view that listed those components, and the rule
  that a navigation rail holds at most six destinations.

This supersedes the Material 3 part of ADR 0001 — the renderer decision stands — and
[ADR 0026](0026-the-content-uses-the-compact-end-of-the-scale.md), whose sizes were steps
of Material 3's scale: the library's controls are compact as published.

## Consequences

- The controls are accessible and keyboard-operable as the library makes them, and are
  maintained by the Dioxus project; a new view composes them and adds layout only.
- `dioxus-primitives` is version 0.0.1 and not on crates.io: the root `Cargo.toml` pins
  it to one commit of the library's repository, and moving that pin is a deliberate step
  taken together with `dx components update`.
- The copied component code lives in this repository under its own licence, named in
  `NOTICE`.
- The window has no brand colour: the theme is grey, with blue for focus and red, amber
  and green for state. A colour is set through the theme's own variables in `app.css`
  when one is wanted.
- The sidebar turns into a sliding panel below 768 px of window width; the window's
  smallest width is 900 px, so it never does.
- Where a view needs something a component does not offer, the answer is a part of the
  app's own built from library components, not a changed copy of the component.

## Pros and cons of the options

### Hand-built Material 3 Expressive

- **Good:** a published, detailed guideline to measure against.
- **Bad:** no library for Dioxus, so the guideline is met by hand and only in part, and
  the amount of hand-built interface grows with every view.

### The app's components, re-coloured

- **Good:** the smallest change to the views.
- **Bad:** keeps everything that made the first option expensive.

### The Dioxus component library

- **Good:** one accepted look with a maintained implementation; the app's stylesheet
  shrinks to layout.
- **Bad:** a young library pinned to a commit; fewer components than Material describes,
  so the tree, the connector tree and the typed field stay the app's own.
