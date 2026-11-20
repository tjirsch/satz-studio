# 0030 — pack sources are satz's, and the window adds and removes them through satz

- **Status:** accepted
- **Date:** 2026-11-20
- **Deciders:** the maintainer

## Context

Since satz v0.103.0 a workload pack comes from a named pack source
([satz ADR 0097](../../vendor/satz/docs/adr/0097-pack-sources-live-in-config-toml.md)):
the built-in `satz`, the satz repository's `packs/`, or a `[[pack_source]]` table of the
estate's `config.toml` — a GitHub repository with an optional branch or tag and folder,
or a folder written relative to `config.toml`. `--from <dir>` is gone; `satz packs` and
`get-pack` take `--source <name>`, `update-pack` reads each pack from the source its row
in `origin.toml` names, and `satz pack-source add|list|remove` edits the tables.

The Packs view offered two buttons, "Read the satz repository" and "Read a folder…",
the second passing the folder chosen in the OS dialog as `--from`. That folder was one
machine's and was never recorded.

## Options

**Where the window learns the sources**

1. **`satz pack-source list --format json`**, read when the view first stands and after
   every add or remove. It reads `config.toml` alone, so it is no network read.
2. Parse `config.toml`'s tables in the app. Cost: a second reader of satz's format, which
   the app's rule — satz owns the estate — forbids.

**How a source is added**

1. **A dialog in the Packs view** — a name, then a GitHub repository with optional branch or
   tag and folder, or a folder from the OS dialog — that runs `satz pack-source add`. satz
   judges the name and the repository and writes `config.toml`; its refusal is shown in
   the dialog as satz says it, and the form keeps what was typed.
2. Leave adding to the commands palette or a terminal. Cost: the one view that lists
   sources cannot make one, and a folder source — the replacement for "Read a folder…" —
   becomes a command line.

**How a chosen folder is written**

1. **Relative to the estate's directory**, the directory of `config.toml`, where a relative
   path exists; as chosen where none does (another drive). Every clone laid out alike
   reads the same folder, which is what satz's record asks of a folder source.
2. As chosen, absolute. Cost: `config.toml` names a path of one machine, and every other
   clone's `origin.toml` rows point at a source that is not there.

## Decision

Option 1 in each. The sources are a selectable list in their own card above the packs,
`satz` first; choosing one runs `satz packs --source <name>`, and Fetch runs `get-pack
--source <name>` from it. Update and Update all run `update-pack` without `--source`, so
each pack is read from the source it was fetched from. A configured source has a remove
button that asks first and runs `satz pack-source remove`; satz refuses while a row of
`origin.toml` names the source, and that refusal is shown in the card.

`[preset_library]`, which replaces satz's preset library, is not shown: no satz report
carries it — neither `satz pack-source list` nor the preset reports — and the app reads
no table of `config.toml` itself.

`MIN_SATZ` is 0.103.0: the commands, the flag and the report fields are that release's.

## Consequences

- The window writes no file for a source; `config.toml` changes only through satz.
- Listing a GitHub source is a network read and happens only when the operator chooses
  it; listing the sources does not reach the network.
- A pack fetched from a source other than the listed one is `not-checked` until that
  source is chosen; each fetched row names its source and, for a GitHub source, the
  commit it was read at.
- A preset library named in `config.toml` is visible in the window only when satz reports
  it.
