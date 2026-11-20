# 0032 — an estate is an organization's or a workload estate, and a pack shows its declarations

- **Status:** accepted
- **Date:** 2026-11-20
- **Deciders:** the maintainer

## Context

[ADR 0025](0025-an-estate-works-in-one-of-three-modes-and-satz-says-which.md) read three
modes from satz's presets report — the organization, a delegated folder, a delegated
project — and each workload pack's `needs`, the delegated folder or project it read. The
window said the mode in an Overview row, "Works in", with a sentence per mode, and a pack
row carried "Reads N values".

satz 0.107.0 dropped both ([satz ADR 0100](../../vendor/satz/docs/adr/0100-a-pack-is-described-by-its-own-declarations.md)).
A delegated resource may be anything an interface folder exports, so satz knows two kinds
of estate — an organization's estate and a workload estate — and names each interface
file the estate uses by the values it exports. `satz packs --estate` reports per pack what
it declares — params, list params, exports — and every interface value it reads, matched
against the estate: the folders that give it, the param the estate sets in its place, or
the transpile's reason that none does.

A pack the operator writes or edits is opened in an external editor ([ADR
0031](0031-the-pack-review-is-in-the-palette-and-a-new-pack-opens-in-the-systems-editor.md)),
and the window does not learn when that editor is done with the file.

## Options

**What the window says of the estate**

1. Keep a sentence per kind. Cost: the sentence names what was delegated, which satz no
   longer sorts into kinds; it would be the app's own guess.
2. **The kind as satz says it, and the interface folders by what they export.** Cost: a
   long export list needs a way to see all of it.

**What a pack row shows**

1. A count of reads. Cost: says nothing about whether the estate gives them.
2. **A Details dialog with the four declarations, and an "Unmet read" badge on the row**
   where a read is not given and not set by the estate. Cost: a dialog per pack; the row
   carries a third badge.

**How the list is read again after an external edit**

1. On window focus (`use_wry_event_handler`, `WindowEvent::Focused`). Cost: every switch
   back to the window runs `satz packs`, a network read for a GitHub source, whether or not
   a pack changed.
2. **A Reload icon button on the "In this estate" card**: the estate read again, and the
   listed source with it. Cost: one click the operator has to remember.

## Decision

- **The Overview's "Kind" row** carries one assist chip, "Organization's estate"
  (`corporate_fare`) or "Workload estate" (`deployed_code`), and no sentence.
  `EstateMode` is `Organization | Workload`; `folder` and `project` fail the read.
- **"Delegated by"** names each interface folder the estate uses, the organization's
  estate that wrote it, the estate account when it carries one, and "exports …" — the
  first five, then "and N more", which opens the whole list.
- **Apply keys on Workload**: `APPLY_WORKLOAD` is the walk without the bootstrap and the
  state's move.
- **A listed pack has "Details"**, a dialog titled with its id: Params (name, default or
  "no default", "asked" with the question's prompt), List params (param, key, fields,
  defaults), Exports (by interface, core first) and Reads (`folder.value` or `value`, then
  "from …", "set by the estate in <param>", or satz's reason in the error color). A pack
  only available from a source has the same, so the list says before the fetch whether it
  fits. A row with an unmet read carries "Unmet read".
- **`satz packs` and `get-pack` are passed `--estate <the open estate>`** always; a fetch
  whose pack reads what the estate does not give is a toast of satz's reasons beside the
  fetch's.
- **Reload** is option 2.
- `MIN_SATZ` is 0.107.0: the app reads the fields that release introduced.
- ADR 0025 is superseded by this.

## Consequences

- The window holds no word for a kind of delegated resource; what a workload estate was
  given is the export list satz reads from its interface folders.
- A pack written in an external editor is seen after Reload, not on its own.
- A pack's params, list params and exports are seen without opening its file, which is
  what the New pack and Edit loop needs to check a pack against the estate.
