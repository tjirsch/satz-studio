# 0038 — The app writes no `use` line and shows the one satz names

- **Status:** accepted
- **Date:** 2026-11-26
- **Deciders:** the maintainer

## Context

ADR 0024 made the `use` line that reads a workload pack the document layer's edit,
`Edit::AddUse { path }`, behind "Add to the estate" on a pack's row, and ADR 0031 had
New pack add its line the same way. The edit wrote `use "<path>"` and nothing else.

satz's ADR 0109 added named uses: a pack whose header says `multiple` is read under a
name, `use "…" named <name>`, its params bound as `<name>.<param>` and what it creates
addressed `<name>__<label>`. Since satz v0.119.0 the line satz names for such a pack —
`use_line` in the `satz packs` report, the line `satz get-pack` prints — ends in
`named <name>`, a placeholder the estate replaces with the use's name; the line as it
stands is refused. A plain `use "<path>"` for a `multiple` pack is wrong, and only the
estate's author knows the name.

## Options

1. **Write the line, as before.** "Add to the estate" stays and writes `use "<path>"`.
   Cost: wrong for every `multiple` pack; the app writes a line satz refuses, or would
   have to tell the two kinds of pack apart itself.
2. **Ask for the name of a `multiple` pack's use, then write the line.** A dialog takes
   the name, checks it, and the document layer writes `use "…" named <name>`. Cost: a
   form and an edit of the app's own for a decision the maintainer wants the estate's
   author to make in the file — where the line stands, whether it carries a `when`,
   which name it takes — and a second edit primitive beside `satz`'s own writers.
3. **Show the line satz names, to copy.** A pack here that the estate does not use shows
   `use_line` as satz reports it, with Copy and the estate file's Edit; the author writes
   it by hand. Cost: one step by hand for every pack, a plain one included.

## Decision

Option 3, the maintainer's choice, for every pack, for now: satz-studio fetches a pack,
and the `use` line that reads it is written by the estate's author. The row of a pack in
`packs_dir` the estate does not use shows "Write this line into the estate file
`<file>`:" over the line in monospace — "…, and replace <name> with the use's name:" for a
line that ends in `named <name>` — with "Copy" and the Edit icon button that opens the
estate file in the Editor setting's command (ADR 0033). A row whose report carries no
`use_line` shows nothing; the app composes no path of its own.

## Consequences

- `Edit::AddUse`, its proof in the document layer, its errors and
  `EstateDir::use_path_of` are gone; the document layer writes values and params only.
- New pack writes the file and opens it; it no longer adds a `use` line (replaces that
  part of ADR 0031). The new pack's row shows its line like any other unused pack's.
- ADR 0024's "the `use` line is the document layer's edit" no longer holds; the rest of
  0024 does.
- Writing the line, or asking for a name, can come back as its own decision once the
  author's choices about where and how a pack is used are something the app can offer.
