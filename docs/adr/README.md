# Architecture decision records

One file per decision that was not obvious, in [MADR](https://adr.github.io/madr/)
form: the context, the options weighed with their real trade-offs, what was chosen,
and what it costs.

A decision belongs here when reversing it would be expensive, when a reader of the
code would otherwise ask "why on earth", or when it was a genuine choice between
defensible alternatives. Most changes are none of those and need no record.

## Why they live here rather than in a commit message

A commit message explains one change to whoever reads that commit. A record answers
"why is it like this?" months later, when nobody is looking at the commit and the
alternative has started to look attractive again. satz keeps its records the same way
and for the same reason; this repository follows its conventions so a reader of one
recognises the other.

## Conventions

- `NNNN-kebab-title.md`, numbered in order, never renumbered. Take the next free number
  from the directory, not from memory.
- The header is **Status**, **Date** and **Deciders**; a decider is named by role
  (`the maintainer`), never by name.
- **Status** is `proposed`, `accepted`, `superseded by ADR-NNNN`, or `rejected`. A
  superseded record is not deleted: the reasoning that was right at the time is what
  is worth keeping, and the successor says what changed.
- The sections are Context, Decision, Consequences, and the pros and cons of every
  option, the chosen one marked *(chosen)*. Options are written with their real
  trade-offs, including the one that was chosen; a record whose alternatives are
  strawmen documents nothing.
- Nothing here names a customer, an organisation or a person: this repository goes
  public, and the privacy gate treats these files like any other.
- A record that rests on a decision satz made links satz's record
  (`vendor/satz/docs/adr/`) rather than restating it.

## Records

| | decision | status |
|---|---|---|
| [0001](0001-dioxus-desktop-on-the-webview-renderer.md) | Dioxus 0.7 desktop on the webview renderer | accepted |
| [0002](0002-a-separate-repository-with-satz-pinned-once.md) | a separate repository, with satz pinned once as a submodule and the binary required at a minimum version | accepted; the minimum's rule superseded by 0014, its Windows half by satz's Windows build |
| [0003](0003-the-document-layer-is-the-tree-sitter-grammar.md) | the document layer is the tree-sitter grammar of Satz, vendored and compiled in; satz-core stays the authority on meaning | accepted |
| [0004](0004-claude-natively-other-providers-adapt-into-its-message-model.md) | Claude natively: the Messages API wire types are the app's message model, and other providers adapt into it | accepted |
| [0005](0005-tool-approval-by-mcp-annotation-and-the-capability-ceiling.md) | tool approval by the MCP annotations satz declares; the capability ceiling stays satz's | accepted |
| [0006](0006-apply-and-bootstrap-run-in-the-users-terminal.md) | `apply` and `bootstrap` run in the user's terminal, never with `-auto-approve` | accepted |
| [0007](0007-pack-rows-are-derived-from-the-estate-file.md) | pack rows are derived from the estate file, the questions report and the resolved params; no copied table | superseded by 0018 |
| [0008](0008-transcripts-live-outside-the-estate.md) | transcripts live under the app's data directory, never inside an estate | accepted |
| [0009](0009-refusal-fallbacks-are-on-by-default.md) | refusal fallbacks are on by default, off by a Settings switch | accepted |
| [0010](0010-claude-code-as-the-subscription-backend.md) | Claude Code as the subscription backend: the installed CLI driven over stdio, the estate's satz MCP server, the app's own approval card | accepted |
| [0011](0011-the-licence-is-apache-2-0.md) | the licence is Apache 2.0: the express patent grant, contribution terms in §5, and a `NOTICE` for the material bundled under other licences | accepted |
| [0012](0012-migrate-hands-off-to-the-terminal.md) | `migrate` hands off to the terminal with `apply` and `bootstrap`; `bootstrap --dry-run` is a check that runs in the app | accepted |
| [0013](0013-the-claude-code-stream-log-is-verbatim-off-by-default-and-bounded.md) | the Claude Code stream log is verbatim, off by default, one file per conversation, and bounded | accepted |
| [0014](0014-a-newer-satz-is-a-notice-and-the-app-looks-for-releases.md) | a satz newer than the build runs and is a notice, not a gate; `MIN_SATZ` is the oldest satz the build works with and rises only for a breakage or a use; the app looks for releases of itself and of satz once per launch and says so in the title and the top bar | accepted |
| [0015](0015-the-packs-view-draws-the-dependency-tree-the-packs-declare.md) | the Packs view draws the dependency tree the packs declare: the edges are the packs' `ask_when`s read with satz-core's parser, a pack others wait on is a block across the grid with its dependents hung below it, and the right-angle connectors are CSS on the tree's own elements | superseded by 0018 |
| [0016](0016-macos-is-apple-silicon-alone.md) | macOS is Apple silicon alone, in CI and in the release: no Intel runner, no Intel bundle; Linux and Windows unchanged on x86_64 | accepted |
| [0017](0017-a-release-is-cargo-release-on-main.md) | a release is `cargo release` on `main` — one step bumps, commits, tags and pushes — and that version-bump commit is the one exception to the pull-request rule | accepted |
| [0018](0018-the-packs-view-shows-satzs-pack-graph.md) | the Packs view shows satz's pack graph: its rows are `satz_packs`, a switch is `satz_add_pack` or `satz_remove_pack`, and the app derives no pack row and no dependency; the tree hangs a pack below the one pack that meets its first requirement | accepted |
| [0019](0019-the-pack-review-runs-the-cli-and-places-a-private-pack-as-a-local-fork.md) | the pack review runs `satz review-pack` through the CLI with the estate's config, since `satz mcp` is confined to the estate's root; a private pack is placed as `<stem>.local.satz` and never over other text; the upstream hand-over is a pull request by hand | accepted |
