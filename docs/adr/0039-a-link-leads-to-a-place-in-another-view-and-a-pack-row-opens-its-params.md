# 0039 — A link leads to a place in another view, and a pack's row opens its params

- **Status:** accepted
- **Date:** 2026-11-28
- **Deciders:** the maintainer

## Context

One fact of an estate stands in several views: a param is a row in Estate → Params, the
question that asks for it is a card in Decisions, and the pack that declares it is a row
in Packs or a card in Presets. Until now the one way between them was "Edit in Params" on
a Decisions card, which switched to Estate and left the reader to find the row. The
Packs row showed a pack's params in a "Details" dialog (ADR 0032), a step away from the
row and with nothing that leads on from a param.

The maintainer decided the views link to one another: a Params group's pack name opens
the pack in Packs or Presets, a param a question asks opens that question in Decisions,
and a param in a pack's params opens its row in Params and its question in Decisions; the
Details dialog gives way to a "Params (n)" toggle that opens under the row, beside the
"Interfaces (2 ↓ / 8 ↑)" toggle. What had to be chosen is how a link reaches the place it
names, which belongs to another view that is not mounted when the link is followed.

## Options

1. **The view switch alone, as "Edit in Params" did.** Cost: the reader searches a long
   list for the row; a link that opens a description or a question does nothing of the
   kind.
2. **A place in the application store, taken by the row or card that renders it.** `go_to`
   sets `AppStore::focus` (`Focus::Pack`, `Preset`, `Param` or `Question`) with the view;
   the row, card or walk that matches takes it in an effect once it stands — clears it,
   scrolls itself into the middle of the view, opens what the link asked for — and the
   shell drops a place whose view the window left. Cost: one store field, and each place
   renders an `id` and carries a small effect.
3. **Each target view reads a parameter of its own at mount and searches its data.**
   Cost: every view grows its own entry point and its own "find the row" step, and a row
   that only appears once a report arrives — the Packs list after `satz packs --local` —
   needs the view to wait for it itself.

## Decision

Option 2. A place waits in the store until the row or card that renders it stands, which
covers the rows that arrive later without a wait of each view's own, and the scroll and
the opening stay with the row or card that knows what it is. The links are the library's
`Button` in its `Link` variant, so each is reachable from the keyboard; a heading or a
name the app cannot place — no row in either view — is plain text, never a link to
nowhere. "Edit in Params" uses the same place and now scrolls to the param's row.

The Details dialog and `PackDetails` are gone; "Params (n)" counts the params and the
list params a pack declares. The two toggles share the row's one `Collapsible` and one
open panel, so one is open at a time, and the panel stands under the whole row, across
its width: tables with aligned columns — Param | Default | Marks and List param | Key |
Fields | Defaults for Params; Value read | From | Feeds | Marks and Export | Interface |
Description for Interfaces. The library offers no table; the tables are plain
HTML tables laid out by `app.css`. A mark on a question or a param is the library's
`Badge` with an icon and a short word, the sentence in its tooltip, with one look for one
meaning in every view (`src/widgets/marks.rs`). This replaces the Details part of ADR
0032; the rest of 0032 holds.

## Consequences

- `Focus`, `go_to`, `take_focus` and `scroll_to` (`crates/satz-studio/src/state/mod.rs`)
  are the one way a view leads into another; a new link adds a `Focus` arm and the
  place's effect, not a mechanism of its own.
- A preset card has no description, so a link to a preset scrolls to its card and opens
  nothing.
- A pack used under several names links each param through the first use that binds it,
  and its "asked" mark through the first use whose question asks it.
