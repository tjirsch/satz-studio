# 0026 — the content uses the compact end of the scale

- **Status:** superseded by ADR-0028
- **Date:** 2026-11-18
- **Deciders:** the maintainer

## Context

The window shows dense material: a question with its reason, its consequences and its
options; a list of every question; presets and packs with their notes. It was built on
the upper steps of Material 3's type scale and the standard button: a question's title at
24 px, its reason at 16 px, list rows of 56 px, buttons 40 px high with 24 px beside the
label. In a window that is not maximized a question's title took three lines and a row of
buttons a fifth of the height. The finding from use: the text is too large at any window
size, and the buttons carry too much space around their labels.

## Options

1. **Compact sizes below a window width**, as three views already change layout below
   1100 px. Cost: two looks to keep in order, and the finding was not about width.
2. **A density setting** for the whole app. Cost: every view checked in two densities,
   for a choice one person makes once.
3. **Compact always**: each text a named step down the type scale, and the button in the
   smallest size the guideline defines.

## Decision

Option 3. Every size stays a named step of the scale; nothing is below 12 px.

- A question's title is 16 px in the emphasized weight (700), not 24 px; its reason 14 px.
- `Button` is Material 3 Expressive's extra-small size: 32 px high, 12 px beside the
  label, 8 px on the side of an icon. `SegmentedButton` items are 32 px high.
- A list row is at least 48 px high, 14 px over 12 px.
- The Presets view's section and map titles are 14 px, a card's reason and remedy 12 px,
  and its card columns are at least 340 px wide. An interface's or a pack's name in the
  Estate tabs is 14 px.
- Unchanged: a view's own heading (24 px, one line per view), icon buttons (40 px: a
  target without a label), chips (32 px), the switch and the navigation rail.

## Consequences

- More fits a window that is not maximized: a question's title is one or two lines.
- The buttons are below the 48 px touch target the guideline asks of a touch screen. The
  app is a desktop window driven by a pointer and the keyboard.
- The sizes are the components', so every view changed with them, not only the three
  that prompted it.
