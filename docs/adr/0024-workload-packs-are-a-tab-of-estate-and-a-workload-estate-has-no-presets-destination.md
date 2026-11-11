# 0024 — workload packs are a tab of Estate, and a workload estate has no Presets destination

- **Status:** accepted
- **Date:** 2026-11-11
- **Deciders:** the maintainer

## Context

satz 0.95.0 keeps two kinds of pack apart (satz's
[ADR 0082](../../vendor/satz/docs/adr/0082-the-library-commands-say-preset-and-pack-is-left-for-workload-packs.md)
and
[ADR 0083](../../vendor/satz/docs/adr/0083-a-workload-pack-is-fetched-one-at-a-time-and-its-origin-is-recorded.md)).
A **preset** is a pack of the library under `presets_dir`, offered by the estate map and
switched with `add-preset` and `remove-preset`. A **workload pack** is one file in the
estate's `packs_dir`: fetched with `satz get-pack` and recorded in `origin.toml` there,
or written by the repository itself, and kept current with `satz update-pack`, which
never writes a pack that was edited or has no row.

The window had one destination for packs, labelled Packs, which shows the preset
library. Two things followed from satz's split:

- Workload packs need a place. The rail holds six primary destinations, which is this
  app's limit ([`docs/ui.md`](../ui.md), the navigation rail's anatomy).
- An estate beside an organisation — a workload's own, which reads the organisation's
  interface — has no preset library. For it satz's presets report holds no pack and a
  note that names `satz get-presets`, which would fetch the organisation's library into
  a folder that has no use for it, and the estate's own workload pack is listed under
  "not in the pack graph".

## Options

**Where workload packs stand**

1. Two tabs under the existing destination, Presets and Workload packs. Nothing moves.
   Cost: the two kinds of pack stay under one word, the confusion satz's rename removed.
2. The destination is renamed Presets, and workload packs are a tab of Estate, beside
   the interfaces. Cost: a renamed destination for whoever knows the window.
3. A seventh destination. Cost: the rail's limit; a navigation drawer in its place.

**What a workload estate shows for presets**

1. The destination as it is. Cost: an empty view whose one sentence gives wrong advice.
2. The destination with a sentence of the app's own in place of satz's report. Cost: a
   place that is there to say it is not for this estate.
3. No destination. Cost: the rail differs by the kind of estate that is open.

**How the commands run, and what happens to the `use` line**

1. The MCP tools `satz_get_pack` and `satz_update_pack`. Cost: `satz mcp` confines
   `from` to its root, and a folder of packs shared by several repositories is outside it.
2. The CLI, as the interfaces and the pack review run (ADR 0019, ADR 0023).

satz prints the `use` line that reaches a fetched pack and has no writer for it.

## Decision

- **The destination is Presets**, and its view is what it was.
- **Workload packs are the fourth tab of Estate.** The tab lists `satz packs` against a
  source the operator names: the satz repository, or a folder chosen in the OS dialog.
  Nothing is read before that: the default source is a network read, and the window
  makes none unasked. A fetch is `satz get-pack`, an update `satz update-pack`, through
  the CLI. A pack edited here is shown with satz's difference; overwriting it
  (`--take-upstream`) and detaching a pack (`satz detach-pack`) each ask first. A fetch,
  an update and a detach hold the estate's write lock, and the estate is read again when
  a pack file was written.
- **The `use` line is the document layer's edit**, `Edit::AddUse { path }`: a line of its
  own after the file's last top-level `use`, after the params block where it has none,
  after the header where it has neither, proved like every other edit and checked by
  satz before it replaces the file. A pack whose params the estate has not bound is
  satz's refusal, shown where every refusal is shown.
- **A workload estate has no Presets destination.** The fact is satz's: its presets
  report names an interface file the estate reads and holds no pack
  (`PacksReport::workload_estate`). An estate that reads an interface and has a library
  keeps the destination. The Overview owes such an estate no repository for
  `merge-presets`.
- The app does not edit `config.toml`. A `packs_dir` no include folder reaches is satz's
  refusal of the fetch, with the line to add.

## Consequences

- The rail shows five primary destinations for a workload estate and six for any other.
- The list is as old as the last read: the tab says which source it was read from, and
  a pack that moved at the source shows only after the next read.
- An edit to a fetched pack is resolved by the operator, as satz leaves it: the source's
  file, or the pack kept as the repository's own.
- The command palette still offers `get-presets`, `check-presets` and `merge-presets`
  for every estate: it is where any satz command can be run.
