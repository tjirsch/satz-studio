# 0031 — the pack review is a palette command, and the Packs view says only what asks for action

- **Status:** accepted
- **Date:** 2026-11-20
- **Deciders:** the maintainer

## Context

The pack review card ([ADR 0019](0019-the-pack-review-runs-the-cli-and-places-a-private-pack-as-a-local-fork.md))
stood above the sections of the Presets view, so it was the first thing on the way
through an organization's estate and through a workload estate alike. The presets come
with satz; an organization takes them and adapts them, and an edit of a preset becomes a
local version through `merge-presets`. Judging a pack against the library's bar serves
whoever writes packs for a library — the satz repository's presets, a source's workload
packs — not the person who chooses packs and configures an estate. Judging a workload
pack inside the Packs view would also need an external editor in the loop, since the
window edits no pack's text. The library's authors run `satz review-preset` and `satz
review-pack` from the command line.

The Packs view said everything at once. Each row spent its first line on a state chip
— "current" on most of them — and a second line on a sentence for that state, also where
the state asked for nothing; the row's actions were labelled buttons on a line of their
own; the lead was a paragraph; and the pack-sources card listed every source with its
explanation above the packs.

## Options

**Where the review stands**

1. Keep the card in Presets. Cost: the first card of the organization's path is a tool
   for library authors.
2. **Two entries of the commands palette, `review-pack` and `review-preset`**: the pack
   file, an optional estate to judge it in (`--against`) and the format; the log
   carries satz's text. Cost: the findings are text in the log, not rows at the pack's
   lines, and the "place as a local fork" destination goes — an edit of a preset already
   becomes a local version through `merge-presets`.
3. Remove the review from the app. Cost: no way to run it from the window.

**What a pack's row shows**

1. A chip for every state, with its sentence under the row. Cost: a line and a sentence
   per row for "current".
2. **A chip only where the state asks for something** — "update available" for a pack
   behind its source; own, edited here, gone from the source, not checked — with the
   state's sentence as the chip's tooltip, and every state explained once in a help
   dialog. Cost: the sentence is on hover or in the dialog.

**How a row's actions are offered**

1. Labelled buttons on a line of their own. Cost: a line per row, wider than the row.
2. **Icon buttons at the row's end, the action named by tooltip and accessible name.**
   Cost: a label shows only on hover.

**How the pack sources are shown**

1. The whole list in the card, with its explanation. Cost: the card is as long as the
   list before any pack shows.
2. **One line — the count and the source listed, else the first — and the whole list,
   to choose or remove one, in a dialog.** Cost: choosing another source is one click
   more.

## Decision

Option 2 in each. The Packs view also lists every `use` line of the main file in a card
of its own, "Used by this estate", which needs no source read, and a pack that is here
can be opened in the system's editor for `.satz` ("Open in your editor").

## Consequences

- No view runs a review. The palette's entries are reporting commands like the others:
  `--format` and `--out` are the app's, and the log is filled from the file.
- The review's code, its state, its drawer findings and its recorded reports are gone;
  the drawer shows the estate's own findings alone.
- A new local pack is not offered: a file in `packs_dir` that no satz command wrote is a
  second writer of the estate, and satz has no command that writes a new pack. A pack
  that is here opens in the operator's editor instead.
- Taking a pack's `use` line out of the estate is not offered: it needs an edit of its
  own in the document layer.
- A pack source cannot be changed in place: `satz pack-source` has add and remove only.
