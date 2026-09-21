# 0019 — the pack review runs the CLI, and a private pack is placed as a `.local.satz`

- **Status:** accepted
- **Date:** 2026-09-21
- **Deciders:** the maintainer

## Context

A pack is the unit everyone extends satz with, and satz holds one to the preset
library's bar with `satz review-pack <pack> --format json` (satz 0.58.1 and later) and
the read-only MCP tool `satz_review_pack`, both returning the `Finding` shape
`Diagnostic::from_finding` reads (`vendor/satz/src/review_pack.rs`). The rules are
satz's; the app shows the findings and hands the pack on to one of two places: upstream,
into satz's library for every estate, or private, into the open estate's `presets_dir`.

Three choices were open.

**How the app runs the review.** The estate session serves `satz_review_pack` beside
every other tool, and the app prefers the session wherever it can. But `satz mcp`
confines every path to the root it was started with — the estate's directory, widened
only as far as the directories its config names ([`session_root`](../../crates/satz-studio-core/src/satz/session.rs)) —
and refuses a pack outside it. The pack under review is usually a file its author keeps
elsewhere; bringing it into the estate is the private destination, which comes after the
review.

**What the private destination writes.** satz's provenance by suffix says a
`X.local.satz` is the user's own file, which no update touches; a plain `X.satz` in the
library is upstream's and `merge-presets` may overwrite it.

**What the upstream destination does.** satz plans a `contribute-pack` command and
ships none; the privacy shapes a pack written against its author's own organisation is
full of are checked by the satz repository's shell gate, not by `review-pack`.

## Options

1. **The MCP tool, the pack copied inside the root first.** One transport for
   everything; costs a write into the estate before the review has said whether the
   pack belongs there, and a scratch file the app must own and remove.
2. **The MCP tool, refusing a pack outside the root.** No write; costs the review of
   exactly the packs that most need one — the ones not yet in any estate.
3. **The CLI with the estate's config: `satz --config <estate dir> review-pack <pack>
   [--against <estate>] --format json`.** Reviews a pack wherever it is, with the
   library, the schema and the adoption rules the estate itself compiles with. Costs a
   second transport for one command: its exit status is a verdict (non-zero when the
   pack does not clear the bar, with the report written either way), so the CLI runner
   gains `json_verdict`, which types the report and holds the status to the report's own
   verdict, refusing a run where the two disagree.

For the private destination: a plain `X.satz` (costs a file an update may overwrite)
or `X.local.satz` (the suffix satz reserves for the user). For upstream: automate the
pull request (costs a GitHub integration that would pre-empt satz's own
`contribute-pack`), or say what the pull request takes and leave it to the author.

## Decision

**Option 3.** The review is `SatzCli::json_verdict` over `satz review-pack`, with the
open estate's config; `--against` the open estate is a switch in the view. The review
holds the bytes it judged, and a pack that changes while satz reads it is refused.

**The private destination writes `<stem>.local.satz` at the top of `presets_dir`**
(`review::place_private`): the reviewed bytes only, refused when the pack changed since
its review; a file of that name holding these bytes already is nothing to do, and one
holding anything else is the estate's own fork and is refused, never written over. The
file is created with `create_new` under the write lock, the estate is checked with it in
the library, and a refusal removes it again.

**The upstream destination automates nothing.** The view says the hand-over is manual
until satz ships `contribute-pack`, names what the pull request takes — the file as
`presets/<stem>.satz`, a clean review, a changelog row, and the privacy shapes the satz
repository's gate checks — and offers the file's path and its folder.

## Consequences

- The review needs an open estate: `satz review-pack` reads a `config.toml`, and the
  estate's is the library the pack is judged against.
- The findings live with the review, not the reload: the drawer shows them beside the
  estate's own until the review is closed, and a reload leaves them.
- When satz ships `contribute-pack`, the upstream card hands over to it, and this record
  is superseded in that part.
