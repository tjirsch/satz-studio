# 0031 — the pack review is in the palette, and a new pack opens in the system's editor

- **Status:** accepted
- **Date:** 2026-11-20
- **Deciders:** the maintainer

## Context

The Presets view carried the pack review — `satz review-preset` over a pack the operator
wrote, and the two places a reviewed pack goes ([ADR
0019](0019-the-pack-review-runs-the-cli-and-places-a-private-pack-as-a-local-fork.md)) —
as a card above the sections, on every visit. Most visits choose presets and packs and
configure them; an organization's presets come with satz, so a review is the rare job of
someone writing a pack for the library, and the card pushed the presets down.

The Packs view listed a workload estate's packs only after a pack source was chosen,
so before that it showed none of the packs the estate uses; it offered no way to start a
pack of the repository's own, and none to open a pack's file.

## Options

**Where the pack review lives**

1. **An entry of the commands palette, "Review a pack for the preset library"**, after
   the commands, for an estate that holds the preset library; its work area is the review
   card. A finding of the reviewed pack chosen in the Findings drawer opens the palette on
   it. Cost: one more step to reach it, and the palette holds one entry that is a card
   rather than a command line.
2. Keep it in Presets, collapsed. Cost: the destination still carries the job of a few.
3. A destination of its own. Cost: a rail slot for a rare job, where the rail holds six
   of Material 3's seven.

**How a new pack is made**

1. **The app writes `packs_dir/<id>.satz`** — `pack <id with _> version "0.1"` and an empty
   `params` block, refused when the file is there — has `satz fmt` format it, adds its
   `use` line with the document layer's `Edit::AddUse` (checked by `satz transpile
   --check` like every edit), and opens the file. satz has no command that scaffolds a
   workload pack; the file is new and the repository's own, never the estate file.
2. Leave it to the operator's editor and terminal. Cost: the one view that lists packs
   cannot start one, and the `use` line is typed by hand.

**What opens a pack's file**

1. **The application the system gives `.satz` files** — `open` on macOS, `start` on
   Windows, `xdg-open` on Linux, through the `open` crate the app already uses for
   folders and exports. No setting: whoever writes Satz has an editor registered for it
   or picks one once in the system dialog.
2. An editor setting (a command line, like the agent's). Cost: a setting to keep and
   explain for what the system already answers.

**Which packs the view lists before a source is chosen**

1. **The estate's own active `use` lines that read a file of `packs_dir`**, from the
   document tree the reload keeps; a listed source adds satz's row — the state, the
   version, the source — to each, and the other files of `packs_dir`. Reading
   `packs_dir` from `config.toml` is one more key of a file the app already reads.
2. List a source unasked when the view stands. Cost: a network read for a GitHub source,
   which ADR 0024 forbids unasked.

## Decision

Option 1 in each. The Packs view is compact besides: two lines of lead, the rest of each
card's explanation behind a help icon button at its top-right corner that opens a basic
dialog, the same in Presets (the view's lead, and each pack's `why`); the pack sources a
one-line summary with "Manage", a dialog over every source — choose it to list its packs,
update the packs fetched from it (`satz update-pack --all --source <name>`), remove a
configured one, add one; a pack row its name, version, origin ("Own pack" for the
repository's own), one badge where its state needs attention ("Update available", "Edited
here", "Gone from source", "Not checked"), "Reads N values" opening the delegated values it
reads, and its actions as text and icon buttons — at most one tonal button per card.

## Consequences

- Presets shows presets alone; the review is one ⌘K away and keeps its findings, its
  drawer link and its two destinations.
- The app writes one more kind of file: a new pack, once, before any `use` line names it.
  The estate file is still written only through the document layer or satz.
- A pack opens in whatever the system opens `.satz` files with; with nothing registered
  the system asks, and a failure is a toast naming the file.
- A pack of the repository's own leaves the estate through satz alone: its row's "Store
  in a pack source…" runs `satz publish-pack` into a folder source (satz's ADR 0099),
  which checks it against `review-pack`'s bar inside the estate and records it in
  `origin.toml` as fetched from there; the app copies no pack itself, and `MIN_SATZ` is
  0.106.0 for it.
- A used pack whose file is not in `packs_dir` is not listed: it is no workload pack.
